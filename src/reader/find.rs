//! Window-local Find workflow. The host supplies document snapshots and
//! handles navigation events; query, results, and background work stay here.

use std::sync::Arc;

use gpui_kit::base::{Disableable, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    App, Context, Entity, EventEmitter, Focusable as _, IntoElement, Subscription, Task, Window,
    div, prelude::*,
};

use crate::{
    find_search::{self, FindMatch},
    pdf,
};

gpui_kit::actions!(kver, [NextMatch, PreviousMatch]);

const MAX_MATCHES: usize = 10_000;
const VISIBLE_RESULTS: usize = 80;
const BATCH_PAGES: usize = 4;

/// A selection is meaningful only for the document and query that produced it.
/// The host checks both before applying a queued navigation event.
pub(crate) struct FindNavigation {
    document_generation: u64,
    revision: u64,
    hit: FindMatch,
}

impl FindNavigation {
    pub(crate) fn document_generation(&self) -> u64 {
        self.document_generation
    }

    pub(crate) fn hit(&self) -> &FindMatch {
        &self.hit
    }
}

enum PageMatches {
    Mapped(Vec<FindMatch>),
    Unmapped,
    Failed,
}

pub(crate) struct FindPanel {
    query: Entity<InputState>,
    _subscription: Subscription,
    // A snapshot supplied by the document owner, never changed by Find itself.
    document: Option<Arc<pdf::Document>>,
    total_pages: usize,
    document_generation: u64,
    pending_query_reset: bool,
    task: Option<Task<()>>,
    results: Vec<FindMatch>,
    active: Option<usize>,
    pending_next: bool,
    pulse: u64,
    scanned: usize,
    mapped_pages: usize,
    failed_pages: usize,
    searching: bool,
    truncated: bool,
    revision: u64,
}

impl EventEmitter<FindNavigation> for FindPanel {}

impl FindPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in PDF"));
        let subscription =
            cx.subscribe_in(&query, window, |find, input, event, _, cx| match event {
                InputEvent::Change => {
                    find.query_changed(input.read(cx).value().to_string(), cx);
                }
                InputEvent::PressEnter { .. } => find.step_match(1, cx),
                _ => {}
            });
        Self {
            query,
            _subscription: subscription,
            document: None,
            total_pages: 0,
            document_generation: 0,
            pending_query_reset: false,
            task: None,
            results: Vec::new(),
            active: None,
            pending_next: false,
            pulse: 0,
            scanned: 0,
            mapped_pages: 0,
            failed_pages: 0,
            searching: false,
            truncated: false,
            revision: 0,
        }
    }

    fn query(&self, cx: &App) -> String {
        self.query.read(cx).value().to_string()
    }

    fn clear_results(&mut self) {
        // Dropping the foreground task stops further batches. Revision checks
        // also reject results/events already queued before cancellation.
        self.task = None;
        self.revision += 1;
        self.results.clear();
        self.active = None;
        self.pending_next = false;
        self.scanned = 0;
        self.mapped_pages = 0;
        self.failed_pages = 0;
        self.truncated = false;
        self.searching = false;
    }

    /// Invalidate immediately on replacement, including while the next PDF is
    /// loading. Installing its page count later does not revive the old query.
    pub(crate) fn set_document(
        &mut self,
        generation: u64,
        document: Option<Arc<pdf::Document>>,
        total_pages: usize,
        cx: &mut Context<Self>,
    ) {
        if generation == self.document_generation
            && self.document.is_some()
            && document.is_some()
            && total_pages == self.total_pages
        {
            // Annotation saves replace the byte baseline, not the text/query.
            self.document = document;
            return;
        }
        if generation != self.document_generation {
            self.pending_query_reset = true;
        }
        self.document_generation = generation;
        self.document = document;
        self.total_pages = total_pages;
        self.clear_results();
        cx.notify();
    }

    pub(crate) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_query_if_needed(window, cx);
        self.query.update(cx, |query, cx| query.focus(window, cx));
    }

    pub(crate) fn query_focus(&self, cx: &App) -> gpui_kit::FocusHandle {
        self.query.read(cx).focus_handle(cx)
    }

    fn reset_query_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_query_reset {
            self.query
                .update(cx, |query, cx| query.set_value("", window, cx));
            self.pending_query_reset = false;
        }
    }

    pub(crate) fn has_matches(&self) -> bool {
        !self.results.is_empty()
    }

    pub(crate) fn generation(&self) -> u64 {
        self.document_generation
    }

    pub(crate) fn highlight(&self) -> Option<(&FindMatch, u64)> {
        self.active
            .and_then(|ix| self.results.get(ix))
            .map(|hit| (hit, self.pulse))
    }

    pub(crate) fn is_current(&self, navigation: &FindNavigation) -> bool {
        navigation.document_generation == self.document_generation
            && navigation.revision == self.revision
    }

    fn query_changed(&mut self, query: String, cx: &mut Context<Self>) {
        self.clear_results();
        if !self.pending_query_reset
            && !query.trim().is_empty()
            && self.total_pages > 0
            && let Some(baseline) = self.document.clone()
        {
            self.searching = true;
            let document = self.document_generation;
            let revision = self.revision;
            let total_pages = self.total_pages;
            self.task = Some(cx.spawn(async move |this, cx| {
                for start in (0..total_pages).step_by(BATCH_PAGES) {
                    let end = (start + BATCH_PAGES).min(total_pages);
                    let baseline = baseline.clone();
                    let query = query.clone();
                    let batch = cx
                        .background_executor()
                        .spawn(async move {
                            baseline
                                .page_text_batch(start..end)
                                .into_iter()
                                .enumerate()
                                .map(|(offset, page)| match page {
                                    Ok(text) if !text.glyphs.is_empty() => PageMatches::Mapped(
                                        find_search::matches_on_page(&text, start + offset, &query),
                                    ),
                                    Ok(_) => PageMatches::Unmapped,
                                    Err(_) => PageMatches::Failed,
                                })
                                .collect()
                        })
                        .await;
                    let keep_searching = this.update(cx, |this, cx| {
                        this.apply_batch(document, revision, batch, cx)
                    });
                    if !matches!(keep_searching, Ok(true)) {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    fn apply_batch(
        &mut self,
        document: u64,
        revision: u64,
        batch: Vec<PageMatches>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.searching || self.document_generation != document || self.revision != revision {
            return false;
        }
        for page in batch {
            self.scanned += 1;
            match page {
                PageMatches::Mapped(hits) => {
                    self.mapped_pages += 1;
                    let remaining = MAX_MATCHES.saturating_sub(self.results.len());
                    self.truncated |= hits.len() > remaining;
                    self.results.extend(hits.into_iter().take(remaining));
                }
                PageMatches::Unmapped => {}
                PageMatches::Failed => self.failed_pages += 1,
            }
        }
        if self.scanned >= self.total_pages || self.results.len() >= MAX_MATCHES {
            self.searching = false;
            self.truncated |= self.scanned < self.total_pages;
        }
        if self.pending_next && !self.results.is_empty() {
            self.pending_next = false;
            self.select_match(0, cx);
        }
        cx.notify();
        self.searching
    }

    pub(crate) fn step_match(&mut self, direction: i32, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            // Enter before the first batch completes selects the first result
            // as soon as it arrives, without stealing focus from the query.
            self.pending_next |= direction > 0 && self.searching;
            return;
        }
        let next = match (self.active, direction) {
            (Some(0), -1) | (None, -1) => self.results.len() - 1,
            (Some(ix), -1) => ix - 1,
            (Some(ix), _) => (ix + 1) % self.results.len(),
            (None, _) => 0,
        };
        self.select_match(next, cx);
    }

    fn select_match(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(hit) = self.results.get(ix).cloned() else {
            return;
        };
        self.active = Some(ix);
        self.pulse = self.pulse.wrapping_add(1);
        cx.emit(FindNavigation {
            document_generation: self.document_generation,
            revision: self.revision,
            hit,
        });
        cx.notify();
    }
}

impl Render for FindPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_query_reset {
            // Document replacement has no Window. Clear the input after this
            // frame; opening a hidden panel clears it synchronously in focus().
            cx.defer_in(window, |this, window, cx| {
                this.reset_query_if_needed(window, cx);
                cx.notify();
            });
        }
        let owner = cx.weak_entity();
        let blank = self.pending_query_reset || self.query(cx).trim().is_empty();
        let visible_start = self
            .active
            .unwrap_or(0)
            .saturating_sub(4)
            .min(self.results.len().saturating_sub(VISIBLE_RESULTS));
        let visible_end = (visible_start + VISIBLE_RESULTS).min(self.results.len());
        let status = if blank {
            "Type a phrase to find it in this PDF".to_owned()
        } else if self.searching {
            format!(
                "Searching… {} matches · {} of {} pages",
                self.results.len(),
                self.scanned,
                self.total_pages
            )
        } else if self.truncated {
            format!("First {} matches · search limit reached", MAX_MATCHES)
        } else if self.results.is_empty() && self.failed_pages > 0 {
            format!(
                "No mapped matches · {} pages could not be read",
                self.failed_pages
            )
        } else if self.results.is_empty() && self.mapped_pages == 0 {
            "No searchable text in this PDF · scans need OCR".to_owned()
        } else if self.results.is_empty() {
            "No matches in mapped text".to_owned()
        } else {
            let count = self.results.len();
            let label = if count == 1 { "match" } else { "matches" };
            match self.active {
                Some(active) => format!("{} / {count} {label}", active + 1),
                None => format!("{count} {label}"),
            }
        };
        let status =
            if self.failed_pages > 0 && !self.searching && !blank && !self.results.is_empty() {
                format!("{status} · {} pages unreadable", self.failed_pages)
            } else {
                status
            };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Input::new(&self.query)
                    .id("find-query")
                    .w_full()
                    .prefix(Icon::new(IconName::Search).small())
                    .cleanable(true),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_1()
                    .child(
                        div()
                            .id("find-status")
                            .test_support()
                            .role(gpui_kit::Role::Status)
                            .aria_label(status.clone())
                            .min_w_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .child(
                                Button::new("find-previous")
                                    .small()
                                    .ghost()
                                    .icon(IconName::ArrowUp)
                                    .accessibility_label("Previous match")
                                    .tooltip_with_action("Previous match", &PreviousMatch, None)
                                    .disabled(self.results.is_empty())
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(PreviousMatch), cx)
                                    }),
                            )
                            .child(
                                Button::new("find-next")
                                    .small()
                                    .ghost()
                                    .icon(IconName::ArrowDown)
                                    .accessibility_label("Next match")
                                    .tooltip_with_action("Next match", &NextMatch, None)
                                    .disabled(self.results.is_empty())
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(NextMatch), cx)
                                    }),
                            ),
                    ),
            )
            .child(
                div().flex().flex_col().gap_1().children(
                    self.results[visible_start..visible_end]
                        .iter()
                        .enumerate()
                        .map(|(offset, hit)| {
                            let index = visible_start + offset;
                            let page_label = format!("Page {}", hit.page + 1);
                            Button::new(("find-result", index))
                                .ghost()
                                .small()
                                .compact()
                                .w_full()
                                .when(self.active == Some(index), |button| {
                                    button.custom(
                                        ButtonCustomVariant::new(cx).active(cx.theme().list_active),
                                    )
                                })
                                .selected(self.active == Some(index))
                                .accessibility_label(format!("{page_label}: {}", hit.snippet))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .min_w_0()
                                        .w_full()
                                        .child(
                                            div()
                                                .id(("find-snippet", index))
                                                .test_support()
                                                .min_w_0()
                                                .flex_1()
                                                .truncate()
                                                .child(hit.snippet.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(("find-location", index))
                                                .test_support()
                                                .flex_shrink_0()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(page_label),
                                        ),
                                )
                                .on_click({
                                    let owner = owner.clone();
                                    move |_, _, cx| {
                                        let _ = owner
                                            .update(cx, |find, cx| find.select_match(index, cx));
                                    }
                                })
                        }),
                ),
            )
            .when(self.results.len() > VISIBLE_RESULTS, |panel| {
                panel.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "Showing {}–{} of {} · use Next and Previous to browse all",
                            visible_start + 1,
                            visible_end,
                            self.results.len()
                        )),
                )
            })
    }
}

#[cfg(test)]
mod tests;
