//! Find owns its query and future search state; the sidebar only hosts it.

use gpui_kit::base::{Disableable, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme, Selectable,
    input::{Input, InputState},
};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    input::InputEvent,
};
use gpui_kit::{Context, Entity, IntoElement, Subscription, Window, div, prelude::*};

use crate::find_search::FindMatch;

pub(crate) const MAX_MATCHES: usize = 10_000;
const VISIBLE_RESULTS: usize = 80;

pub(crate) struct FindPanel {
    query: Entity<InputState>,
    _subscription: Subscription,
    pub(crate) results: Vec<FindMatch>,
    pub(crate) active: Option<usize>,
    pub(crate) pending_next: bool,
    pub(crate) pulse: u64,
    pub(crate) scanned: usize,
    pub(crate) mapped_pages: usize,
    pub(crate) failed_pages: usize,
    pub(crate) searching: bool,
    pub(crate) truncated: bool,
    pub(crate) revision: u64,
}

impl FindPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<crate::reader::Preview>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in PDF"));
        let subscription =
            cx.subscribe_in(&query, window, |reader, input, event, _, cx| match event {
                InputEvent::Change => {
                    reader.find_query_changed(input.read(cx).value().to_string(), cx);
                }
                InputEvent::PressEnter { .. } => reader.next_find_match(cx),
                _ => {}
            });
        Self {
            query,
            _subscription: subscription,
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

    pub(crate) fn query(&self, cx: &gpui_kit::App) -> String {
        self.query.read(cx).value().to_string()
    }

    pub(crate) fn clear_results(&mut self) {
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

    /// A new document gets a fresh query (and no undo history from the old PDF).
    /// `open_path` has no Window, so the reader performs this in its next frame.
    pub(crate) fn reset_query(
        &self,
        window: &mut Window,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        self.query
            .update(cx, |query, cx| query.set_value("", window, cx));
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<crate::reader::Preview>) {
        self.query.update(cx, |query, cx| query.focus(window, cx));
    }

    pub(crate) fn render(
        &self,
        total_pages: usize,
        cx: &mut Context<crate::reader::Preview>,
    ) -> impl IntoElement {
        let owner = cx.weak_entity();
        let blank = self.query(cx).trim().is_empty();
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
                total_pages
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
        } else if self.results.len() == 1 {
            "1 match".to_owned()
        } else {
            format!("{} matches", self.results.len())
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
            .gap_3()
            .child(
                Input::new(&self.query)
                    .id("find-query")
                    .w_full()
                    .cleanable(true),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("find-previous")
                            .small()
                            .label("Previous")
                            .disabled(self.results.is_empty())
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(crate::reader::PreviousMatch), cx)
                            }),
                    )
                    .child(
                        Button::new("find-next")
                            .small()
                            .label("Next")
                            .disabled(self.results.is_empty())
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(crate::reader::NextMatch), cx)
                            }),
                    ),
            )
            .child(
                div()
                    .id("find-status")
                    .test_support()
                    .role(gpui_kit::Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .children(
                self.results[visible_start..visible_end]
                    .iter()
                    .enumerate()
                    .map(|(offset, hit)| {
                        let index = visible_start + offset;
                        let label = format!("Page {} · {}", hit.page + 1, hit.snippet);
                        Button::new(("find-result", index))
                            .ghost()
                            .w_full()
                            .selected(self.active == Some(index))
                            .label(label)
                            .on_click({
                                let owner = owner.clone();
                                move |_, _, cx| {
                                    let _ = owner
                                        .update(cx, |reader, cx| reader.select_match(index, cx));
                                }
                            })
                    }),
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
