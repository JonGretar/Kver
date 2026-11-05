//! Contents owns its focus and future outline/navigation state.

use gpui_kit::{Context, FocusHandle, IntoElement, Window, div, prelude::*};
use gpui_kit::{base::TestSupportExt as _, component::ActiveTheme};

pub(crate) struct ContentsPanel {
    focus: FocusHandle,
}

impl ContentsPanel {
    pub(crate) fn new(cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<crate::reader::Preview>) {
        window.focus(&self.focus, cx);
    }

    pub(crate) fn render(&self, cx: &mut Context<crate::reader::Preview>) -> impl IntoElement {
        div()
            .id("contents-placeholder")
            .test_support()
            .track_focus(&self.focus)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("Document contents are coming soon.")
    }
}
