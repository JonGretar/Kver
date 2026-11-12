//! Lazy outline loading, tree interaction, and document-space navigation.

use std::sync::Arc;

use crate::pdf::{self, OutlineDestination, OutlineEntry};
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, IntoElement, Task, Window, div, prelude::*,
};
use gpui_kit::{
    base::{
        TestSupportExt as _,
        actions::{SelectLeft, SelectRight},
    },
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants},
        list::ListItem,
        tooltip::Tooltip,
        tree::{TreeItem, TreeState, tree},
    },
};

gpui_kit::actions!(kver, [ActivateOutline]);

pub(crate) struct OutlineNavigation {
    generation: u64,
    destination: OutlineDestination,
}

impl OutlineNavigation {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn destination(&self) -> OutlineDestination {
        self.destination
    }
}

pub(crate) struct ContentsPanel {
    focus: FocusHandle,
    document: Option<Arc<pdf::Document>>,
    generation: u64,
    task: Option<Task<()>>,
    outline: Option<Result<Arc<Vec<OutlineEntry>>, String>>,
    tree: gpui_kit::Entity<TreeState>,
}

impl EventEmitter<OutlineNavigation> for ContentsPanel {}

impl ContentsPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
            document: None,
            generation: 0,
            task: None,
            outline: None,
            tree: cx.new(|cx| TreeState::new(cx)),
        }
    }

    /// Supply the document owner's snapshot. Extraction remains lazy until
    /// Contents opens (or its host requests a load for an already open panel).
    pub(crate) fn set_document(
        &mut self,
        generation: u64,
        document: Option<Arc<pdf::Document>>,
        cx: &mut Context<Self>,
    ) {
        if generation == self.generation && self.document.is_some() && document.is_some() {
            // The outline is unchanged by annotation Save/Save As.
            self.document = document;
            return;
        }
        self.generation = generation;
        self.document = document;
        self.task = None;
        self.outline = None;
        self.tree.update(cx, |tree, cx| tree.set_items(vec![], cx));
        cx.notify();
    }

    pub(crate) fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
        if self.outline.is_some() || self.task.is_some() {
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let generation = self.generation;
        let work = cx
            .background_executor()
            .spawn(async move { document.outline() });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.install(generation, result, cx) {
                    // The new tree must render before its focus handle is tracked.
                    cx.on_next_frame(window, |this, window, cx| this.focus_loaded(window, cx));
                }
            });
        }));
    }

    fn install(
        &mut self,
        generation: u64,
        result: Result<Vec<OutlineEntry>, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        let items = result
            .as_ref()
            .map(|entries| outline_tree(entries))
            .unwrap_or_default();
        self.tree.update(cx, |tree, cx| tree.set_items(items, cx));
        self.outline = Some(result.map(Arc::new));
        cx.notify();
        true
    }

    fn destination(&self, index: usize) -> Option<OutlineDestination> {
        self.outline
            .as_ref()?
            .as_ref()
            .ok()?
            .get(index)?
            .destination()
    }

    fn selected_destination(&self, cx: &App) -> Option<OutlineDestination> {
        let id = &self.tree.read(cx).selected_item()?.id;
        self.destination(id.strip_prefix("contents-")?.parse().ok()?)
    }

    pub(crate) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_loaded(cx);
        if self.has_navigable_entries() {
            self.tree.update(cx, |tree, cx| tree.focus(window, cx));
        } else {
            window.focus(&self.focus, cx);
        }
    }

    fn has_navigable_entries(&self) -> bool {
        self.outline.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|entries| entries.iter().any(|entry| entry.destination().is_some()))
        })
    }

    fn focus_loaded(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_navigable_entries() && self.focus.is_focused(window) {
            self.tree.update(cx, |tree, cx| tree.focus(window, cx));
        }
    }

    fn activate(&self, generation: u64, ix: usize, cx: &mut Context<Self>) {
        if generation == self.generation
            && let Some(destination) = self.destination(ix)
        {
            cx.emit(OutlineNavigation {
                generation,
                destination,
            });
        }
    }

    fn activate_selected(&self, cx: &mut Context<Self>) {
        if let Some(destination) = self.selected_destination(cx) {
            cx.emit(OutlineNavigation {
                generation: self.generation,
                destination,
            });
        }
    }
}

impl Render for ContentsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = match &self.outline {
            None => Some(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading contents…")
                    .into_any_element(),
            ),
            Some(Err(_)) => Some(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Couldn’t read this PDF’s contents. You can still read its pages.")
                    .into_any_element(),
            ),
            Some(Ok(entries)) if entries.iter().all(|entry| entry.destination().is_none()) => Some(
                div()
                    .id("contents-empty")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .child("No contents"),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("This PDF doesn’t include a navigable table of contents."),
                    )
                    .into_any_element(),
            ),
            Some(Ok(_)) => None,
        };
        let available = status.is_none();
        let entries = self
            .outline
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned()
            .unwrap_or_default();
        let owner = cx.weak_entity();
        let generation = self.generation;
        let tree_state = self.tree.clone();
        let content = tree(&self.tree, move |row_ix, entry, selected, _, _| {
            let ix = entry
                .item()
                .id
                .strip_prefix("contents-")
                .and_then(|id| id.parse::<usize>().ok())
                .expect("Contents tree IDs come from outline_tree");
            let navigable = entries[ix].destination().is_some();
            let folder = entry.is_folder();
            let expanded = entry.is_expanded();
            let owner = owner.clone();
            let selection = tree_state.clone();
            let disclosure = tree_state.clone();
            let title = entry.item().label.clone();
            ListItem::new(("contents-entry", ix))
                .accessibility_label(title.clone())
                .selected(selected)
                .h_8()
                .text_xs()
                .pl(gpui_kit::rems((entry.depth().min(8) as f32) * 0.75))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap_2()
                        .child(if folder {
                            Button::new(("contents-disclosure", ix))
                                .ghost()
                                .xsmall()
                                .icon(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .accessibility_label(format!(
                                    "{} {}",
                                    if expanded { "Collapse" } else { "Expand" },
                                    entry.item().label
                                ))
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    disclosure.update(cx, |tree, cx| {
                                        tree.set_selected_index(Some(row_ix), cx);
                                        tree.focus(window, cx);
                                    });
                                    if expanded {
                                        window.dispatch_action(Box::new(SelectLeft), cx);
                                    } else {
                                        window.dispatch_action(Box::new(SelectRight), cx);
                                    }
                                })
                                .into_any_element()
                        } else {
                            div().w_5().flex_shrink_0().into_any_element()
                        })
                        .child(
                            div()
                                .id(("contents-title", ix))
                                .test_support()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .tooltip(move |window, cx| {
                                    Tooltip::new(title.clone()).build(window, cx)
                                })
                                .child(entry.item().label.clone()),
                        ),
                )
                .on_click(move |_, window, cx| {
                    // Tree toggles the entire row by default. Keep its keyboard
                    // behavior, but reserve pointer disclosure for the chevron.
                    cx.stop_propagation();
                    selection.update(cx, |tree, cx| {
                        tree.set_selected_index(Some(row_ix), cx);
                        tree.focus(window, cx);
                    });
                    if navigable {
                        let _ =
                            owner.update(cx, |contents, cx| contents.activate(generation, ix, cx));
                    }
                })
        })
        .when(available, |tree| tree.size_full())
        .when(!available, |tree| tree.h_0());
        div()
            .id("contents-panel")
            .test_support()
            .track_focus(&self.focus)
            .key_context(crate::key_context::CONTENTS)
            .on_action(cx.listener(|this, _: &ActivateOutline, _, cx| this.activate_selected(cx)))
            .role(gpui_kit::Role::Navigation)
            .aria_label("PDF contents")
            .flex_1()
            .min_h_0()
            .w_full()
            .when_some(status, |panel, status| {
                panel.child(div().p_2().max_w_64().child(status))
            })
            .child(content)
    }
}

/// Convert flat PDF outline order to Tree's owned hierarchy without recursion.
/// PDF indices are document-local stable IDs even when branches are collapsed.
fn outline_tree(entries: &[OutlineEntry]) -> Vec<TreeItem> {
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, TreeItem)> = Vec::new();
    for (ix, entry) in entries.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|(depth, _)| *depth >= entry.depth())
        {
            let (_, child) = stack.pop().unwrap();
            if let Some((_, parent)) = stack.last_mut() {
                parent.children.push(child);
            } else {
                roots.push(child);
            }
        }
        stack.push((
            entry.depth(),
            TreeItem::new(format!("contents-{ix}"), entry.title())
                .expanded(entry.depth() == 0)
                .disabled(
                    entry.destination().is_none()
                        && entries
                            .get(ix + 1)
                            .is_none_or(|next| next.depth() <= entry.depth()),
                ),
        ));
    }
    while let Some((_, child)) = stack.pop() {
        if let Some((_, parent)) = stack.last_mut() {
            parent.children.push(child);
        } else {
            roots.push(child);
        }
    }
    roots
}

#[cfg(test)]
mod tests;
