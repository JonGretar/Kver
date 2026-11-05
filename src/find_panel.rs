//! Find owns its query and future search state; the sidebar only hosts it.

use gpui_kit::component::{
    ActiveTheme,
    input::{Input, InputState},
};
use gpui_kit::{Context, Entity, IntoElement, Window, div, prelude::*};

pub(crate) struct FindPanel {
    query: Entity<InputState>,
}

impl FindPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Find in PDF")),
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<crate::reader::Preview>) {
        self.query.update(cx, |query, cx| query.focus(window, cx));
    }

    pub(crate) fn render(&self, cx: &mut Context<crate::reader::Preview>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(Input::new(&self.query).id("find-query").w_full())
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Document search is coming soon."),
            )
    }
}
