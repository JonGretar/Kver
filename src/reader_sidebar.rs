//! One window-local sidebar host. Find and Contents own their own retained content.

use gpui_kit::{Context, FocusHandle, IntoElement, Window, div, prelude::*};
use gpui_kit::{
    base::TestSupportExt as _,
    component::{
        ActiveTheme,
        scroll::ScrollableElement as _,
        tab::{Tab, TabBar},
    },
};

use crate::pdf::{OutlineDestination, OutlineEntry};
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

    pub fn reset_document(&mut self) {
        self.contents.reset();
    }

    pub fn has_outline(&self) -> bool {
        self.contents.has_outline()
    }

    pub fn install_outline(&mut self, result: Result<Vec<OutlineEntry>, String>) {
        self.contents.install(result);
    }

    pub fn select_outline(&mut self, index: usize) -> Option<OutlineDestination> {
        self.contents.select(index)
    }

    pub fn step_outline(&mut self, forward: bool) {
        self.contents.step(forward);
    }

    pub fn selected_outline(&self) -> Option<OutlineDestination> {
        self.contents.selected_destination()
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
        let panel = match active {
            SidebarPanel::Find => div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p_3()
                .child(self.find.render(cx))
                .into_any_element(),
            SidebarPanel::Contents => div()
                .flex_1()
                .min_h_0()
                .flex()
                .p_2()
                .child(self.contents.render(cx))
                .into_any_element(),
        };
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
            .child(panel)
            .child(
                div().border_t_1().border_color(cx.theme().border).child(
                    TabBar::new("sidebar-panels")
                        .w_full()
                        .underline()
                        .selected_index(match active {
                            SidebarPanel::Find => 0,
                            SidebarPanel::Contents => 1,
                        })
                        .last_empty_space(div().w_0())
                        .child(Tab::new().label("Find").flex_1())
                        .child(Tab::new().label("Contents").flex_1())
                        .on_click(|ix, window, cx| {
                            let action: Box<dyn gpui_kit::Action> = match ix {
                                0 => Box::new(crate::reader::ShowFind),
                                _ => Box::new(crate::reader::ShowContents),
                            };
                            window.dispatch_action(action, cx);
                        }),
                ),
            )
    }
}
