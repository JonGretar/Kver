//! Window-local outline state and presentation inside the shared sidebar.

use std::sync::Arc;

use crate::pdf::{OutlineDestination, OutlineEntry};
use gpui_kit::{App, Context, FocusHandle, IntoElement, Window, div, prelude::*};
use gpui_kit::{
    base::{
        TestSupportExt as _,
        actions::{SelectLeft, SelectRight},
    },
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants},
        list::ListItem,
        tree::{TreeItem, TreeState, tree},
    },
};

pub(crate) struct ContentsPanel {
    focus: FocusHandle,
    outline: Option<Result<Arc<Vec<OutlineEntry>>, String>>,
    tree: gpui_kit::Entity<TreeState>,
}

impl ContentsPanel {
    pub(crate) fn new(cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
            outline: None,
            tree: cx.new(|cx| TreeState::new(cx)),
        }
    }

    pub(crate) fn reset(&mut self, cx: &mut Context<crate::reader::Preview>) {
        self.outline = None;
        self.tree.update(cx, |tree, cx| tree.set_items(vec![], cx));
    }

    pub(crate) fn has_outline(&self) -> bool {
        self.outline.is_some()
    }

    pub(crate) fn install(
        &mut self,
        result: Result<Vec<OutlineEntry>, String>,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        let items = result
            .as_ref()
            .map(|entries| outline_tree(entries))
            .unwrap_or_default();
        self.tree.update(cx, |tree, cx| tree.set_items(items, cx));
        self.outline = Some(result.map(Arc::new));
    }

    pub(crate) fn destination(&self, index: usize) -> Option<OutlineDestination> {
        self.outline
            .as_ref()?
            .as_ref()
            .ok()?
            .get(index)?
            .destination()
    }

    pub(crate) fn selected_destination(&self, cx: &App) -> Option<OutlineDestination> {
        let id = &self.tree.read(cx).selected_item()?.id;
        self.destination(id.strip_prefix("contents-")?.parse().ok()?)
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<crate::reader::Preview>) {
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

    pub(crate) fn focus_loaded(
        &self,
        window: &mut Window,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        if self.has_navigable_entries() && self.focus.is_focused(window) {
            self.tree.update(cx, |tree, cx| tree.focus(window, cx));
        }
    }

    pub(crate) fn render(&self, cx: &mut Context<crate::reader::Preview>) -> impl IntoElement {
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
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("This PDF has no navigable contents.")
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
            ListItem::new(("contents-entry", ix))
                .accessibility_label(entry.item().label.clone())
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
                        .child(div().min_w_0().text_xs().child(entry.item().label.clone())),
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
                            owner.update(cx, |reader, cx| reader.select_outline(ix, window, cx));
                    }
                })
        })
        .when(available, |tree| tree.size_full())
        .when(!available, |tree| tree.h_0());
        div()
            .id("contents-panel")
            .test_support()
            .track_focus(&self.focus)
            .key_context("ContentsPanel")
            .role(gpui_kit::Role::Navigation)
            .aria_label("PDF contents")
            .flex_1()
            .min_h_0()
            .w_full()
            .when_some(status, |panel, status| panel.child(status))
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
