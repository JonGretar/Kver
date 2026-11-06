//! One window-local sidebar host. Find and Contents own their own retained content.

use gpui_kit::{
    Axis, Context, DragMoveEvent, Empty, FocusHandle, IntoElement, Window, div, prelude::*,
};
use gpui_kit::{
    base::{InteractiveElementExt as _, TestSupportExt as _},
    component::{
        ActiveTheme,
        resizable::{HandleEdge, resize_handle},
        scroll::ScrollableElement as _,
        tab::{Tab, TabBar},
    },
};

use crate::pdf::{OutlineDestination, OutlineEntry};
use crate::{contents_panel::ContentsPanel, find_panel::FindPanel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarPanel {
    #[default]
    Contents,
    Find,
}

const DEFAULT_WIDTH_REMS: f32 = 16.;
const MIN_WIDTH_REMS: f32 = 10.;
const MAX_WIDTH_REMS: f32 = 24.;

#[derive(Clone)]
struct ResizeSidebar;

impl gpui_kit::Render for ResizeSidebar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Host-to-panel seam: the host selects and focuses a panel; each panel owns
/// its retained content and implements its own actions without host-owned data.
pub struct ReaderSidebar {
    active: Option<SidebarPanel>,
    last: SidebarPanel,
    width_rems: f32,
    find: FindPanel,
    contents: ContentsPanel,
}

impl ReaderSidebar {
    pub fn new(window: &mut Window, cx: &mut Context<crate::reader::Preview>) -> Self {
        Self {
            active: None,
            last: SidebarPanel::default(),
            width_rems: DEFAULT_WIDTH_REMS,
            find: FindPanel::new(window, cx),
            contents: ContentsPanel::new(cx),
        }
    }

    fn bounded_width(width_rems: f32, window_rems: f32) -> f32 {
        // The PDF stays wider even when a narrow window cannot accommodate
        // the sidebar's preferred minimum width.
        let maximum = MAX_WIDTH_REMS.min(window_rems * 0.4);
        width_rems.clamp(MIN_WIDTH_REMS.min(maximum), maximum)
    }

    pub(crate) fn resize(&mut self, position: f32, window: &Window) {
        let rem = f32::from(window.rem_size());
        let window_rems = f32::from(window.bounds().size.width) / rem;
        self.width_rems = Self::bounded_width(position / rem, window_rems);
    }

    pub(crate) fn reset_width(&mut self) {
        self.width_rems = DEFAULT_WIDTH_REMS;
    }

    pub fn active(&self) -> Option<SidebarPanel> {
        self.active
    }

    pub fn last(&self) -> SidebarPanel {
        self.last
    }

    pub(crate) fn find(&self) -> &FindPanel {
        &self.find
    }

    pub(crate) fn find_mut(&mut self) -> &mut FindPanel {
        &mut self.find
    }

    pub fn reset_document(&mut self, cx: &mut Context<crate::reader::Preview>) {
        self.contents.reset(cx);
    }

    pub fn has_outline(&self) -> bool {
        self.contents.has_outline()
    }

    pub fn install_outline(
        &mut self,
        result: Result<Vec<OutlineEntry>, String>,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        self.contents.install(result, cx);
    }

    pub fn focus_loaded_outline(
        &self,
        window: &mut Window,
        cx: &mut Context<crate::reader::Preview>,
    ) {
        if self.active == Some(SidebarPanel::Contents) {
            self.contents.focus_loaded(window, cx);
        }
    }

    pub fn outline_destination(&self, index: usize) -> Option<OutlineDestination> {
        self.contents.destination(index)
    }

    pub fn selected_outline(&self, cx: &gpui_kit::App) -> Option<OutlineDestination> {
        self.contents.selected_destination(cx)
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
        total_pages: usize,
        cx: &mut Context<crate::reader::Preview>,
    ) -> impl IntoElement {
        let active = self.active.expect("only render an open sidebar");
        let width_rems = Self::bounded_width(
            self.width_rems,
            f32::from(window.bounds().size.width) / f32::from(window.rem_size()),
        );
        let panel = match active {
            SidebarPanel::Find => div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p_3()
                .child(self.find.render(total_pages, cx))
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
            .relative()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizeSidebar>, window, cx| {
                    this.resize_sidebar(
                        f32::from(event.event.position.x - event.bounds.left()),
                        window,
                        cx,
                    );
                }),
            )
            .key_context("ReaderSidebar")
            .role(gpui_kit::Role::Complementary)
            .aria_label("Document sidebar")
            .flex()
            .flex_col()
            .h_full()
            .w(gpui_kit::rems(width_rems))
            .flex_shrink_0()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .child(panel)
            .child(
                div().border_t_1().border_color(cx.theme().border).child(
                    TabBar::new("sidebar-panels")
                        .w_full()
                        .underline()
                        .selected_index(match active {
                            SidebarPanel::Contents => 0,
                            SidebarPanel::Find => 1,
                        })
                        .last_empty_space(div().w_0())
                        .child(Tab::new().label("Contents").flex_1())
                        .child(Tab::new().label("Find").flex_1())
                        .on_click(|ix, window, cx| {
                            let action: Box<dyn gpui_kit::Action> = match ix {
                                0 => Box::new(crate::reader::ShowContents),
                                _ => Box::new(crate::reader::ShowFind),
                            };
                            window.dispatch_action(action, cx);
                        }),
                ),
            )
            .child(
                resize_handle("sidebar-resize-handle", Axis::Horizontal)
                    .inside(HandleEdge::Trailing)
                    .on_drag(ResizeSidebar, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_double_click(cx.listener(|this, _, _, cx| {
                        this.reset_sidebar_width(cx);
                    })),
            )
    }
}
