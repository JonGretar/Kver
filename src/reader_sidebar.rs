//! One window-local sidebar host. Find and Contents own their own retained content.

use gpui_kit::{Context, FocusHandle, IntoElement, Window, div, prelude::*};
use gpui_kit::{
    base::TestSupportExt as _,
    component::{
        ActiveTheme, Selectable, Sizable,
        button::{Button, ButtonVariants},
        scroll::ScrollableElement as _,
    },
};

use crate::{contents_panel::ContentsPanel, find_panel::FindPanel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarPanel {
    Find,
    Contents,
}

/// Host-to-panel seam: the host selects and focuses a panel; each panel owns
/// its retained content and implements its own actions without host-owned data.
pub struct ReaderSidebar {
    active: Option<SidebarPanel>,
    last: SidebarPanel,
    find: FindPanel,
    contents: ContentsPanel,
}

impl ReaderSidebar {
    pub fn new(window: &mut Window, cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            active: None,
            last: SidebarPanel::Find,
            find: FindPanel::new(window, cx),
            contents: ContentsPanel::new(cx),
        }
    }

    pub fn active(&self) -> Option<SidebarPanel> {
        self.active
    }

    pub fn last(&self) -> SidebarPanel {
        self.last
    }

    pub fn open(
        &mut self,
        panel: SidebarPanel,
        window: &mut Window,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        self.active = Some(panel);
        self.last = panel;
        match panel {
            SidebarPanel::Find => self.find.focus(window, cx),
            SidebarPanel::Contents => self.contents.focus(window, cx),
        }
        cx.notify();
    }

    pub fn close(
        &mut self,
        window: &mut Window,
        reader_focus: &FocusHandle,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        self.active = None;
        window.focus(reader_focus, cx);
        cx.notify();
    }

    pub fn render(
        &self,
        window: &Window,
        cx: &mut Context<crate::reader::Preview>,
    ) -> impl IntoElement {
        let active = self.active.expect("only render an open sidebar");
        // Keep the document wider than the auxiliary region at the minimum
        // window width, while growing the sidebar to a comfortable reading size.
        let width_rems = (f32::from(window.bounds().size.width) / f32::from(window.rem_size())
            * 0.4)
            .clamp(10., 16.);
        div()
            .id("reader-sidebar")
            .test_support()
            .key_context("ReaderSidebar")
            .role(gpui_kit::Role::Complementary)
            .aria_label("Document sidebar")
            .flex()
            .flex_col()
            .h_full()
            .w(gpui_kit::rems(width_rems))
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("sidebar-find")
                                    .label("Find")
                                    .ghost()
                                    .small()
                                    .selected(active == SidebarPanel::Find)
                                    .on_click(|_, window, cx| {
                                        window
                                            .dispatch_action(Box::new(crate::reader::ShowFind), cx)
                                    }),
                            )
                            .child(
                                Button::new("sidebar-contents")
                                    .label("Contents")
                                    .ghost()
                                    .small()
                                    .selected(active == SidebarPanel::Contents)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(
                                            Box::new(crate::reader::ShowContents),
                                            cx,
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div().flex().justify_end().child(
                            Button::new("sidebar-close")
                                .label("Close")
                                .accessibility_label("Close sidebar")
                                .ghost()
                                .small()
                                .on_click(|_, window, cx| {
                                    window
                                        .dispatch_action(Box::new(crate::reader::CloseSidebar), cx)
                                }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .p_3()
                    .child(match active {
                        SidebarPanel::Find => self.find.render(cx).into_any_element(),
                        SidebarPanel::Contents => self.contents.render(cx).into_any_element(),
                    }),
            )
    }
}
