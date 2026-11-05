//! Window-local outline state and presentation inside the shared sidebar.

use std::sync::Arc;

use crate::pdf::{OutlineDestination, OutlineEntry};
use gpui_kit::{
    Context, FocusHandle, IntoElement, ScrollStrategy, UniformListScrollHandle, Window, div,
    prelude::*, uniform_list,
};
use gpui_kit::{
    base::{Disableable, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable,
        button::{Button, ButtonVariants},
    },
};

pub(crate) struct ContentsPanel {
    focus: FocusHandle,
    outline: Option<Result<Arc<Vec<OutlineEntry>>, String>>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
}

impl ContentsPanel {
    pub(crate) fn new(cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
            outline: None,
            selected: None,
            scroll: UniformListScrollHandle::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.outline = None;
        self.selected = None;
        self.scroll = UniformListScrollHandle::default();
    }

    pub(crate) fn has_outline(&self) -> bool {
        self.outline.is_some()
    }

    pub(crate) fn install(&mut self, result: Result<Vec<OutlineEntry>, String>) {
        self.outline = Some(result.map(Arc::new));
        self.selected = None;
    }

    pub(crate) fn destination(&self, index: usize) -> Option<OutlineDestination> {
        self.outline
            .as_ref()?
            .as_ref()
            .ok()?
            .get(index)?
            .destination()
    }

    pub(crate) fn select(&mut self, index: usize) -> Option<OutlineDestination> {
        let dest = self.destination(index)?;
        self.selected = Some(index);
        Some(dest)
    }

    pub(crate) fn step(&mut self, forward: bool) {
        let Some(Ok(entries)) = &self.outline else {
            return;
        };
        let start = self
            .selected
            .unwrap_or(if forward { usize::MAX } else { entries.len() });
        let next = if forward {
            (start.wrapping_add(1)..entries.len()).find(|&ix| entries[ix].destination().is_some())
        } else {
            (0..start)
                .rev()
                .find(|&ix| entries[ix].destination().is_some())
        };
        if let Some(ix) = next {
            self.selected = Some(ix);
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    pub(crate) fn selected_destination(&self) -> Option<OutlineDestination> {
        self.destination(self.selected?)
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<crate::reader::Preview>) {
        window.focus(&self.focus, cx);
    }

    pub(crate) fn render(&self, cx: &mut Context<crate::reader::Preview>) -> impl IntoElement {
        let body = match &self.outline {
            None => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Loading contents…")
                .into_any_element(),
            Some(Err(_)) => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Couldn’t read this PDF’s contents. You can still read its pages.")
                .into_any_element(),
            Some(Ok(entries)) if entries.iter().all(|entry| entry.destination().is_none()) => div()
                .id("contents-empty")
                .test_support()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("This PDF has no navigable contents.")
                .into_any_element(),
            Some(Ok(entries)) => {
                let entries = entries.clone();
                let selected = self.selected;
                let owner = cx.weak_entity();
                uniform_list("contents-list", entries.len(), move |range, _, _| {
                    range
                        .map(|ix| {
                            let entry = &entries[ix];
                            let owner = owner.clone();
                            div()
                                .h_8()
                                .w_full()
                                .pl(gpui_kit::rems((entry.depth().min(8) as f32) * 0.75))
                                .child(
                                    Button::new(("contents-entry", ix))
                                        .label(entry.title().to_owned())
                                        .ghost()
                                        .w_full()
                                        .selected(selected == Some(ix))
                                        .disabled(entry.destination().is_none())
                                        .on_click(move |_, window, cx| {
                                            let _ = owner.update(cx, |reader, cx| {
                                                reader.select_outline(ix, window, cx)
                                            });
                                        }),
                                )
                        })
                        .collect::<Vec<_>>()
                })
                .track_scroll(&self.scroll)
                .size_full()
                .into_any_element()
            }
        };
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
            .child(body)
    }
}
