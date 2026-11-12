//! One window-local sidebar host. Find and Contents own their own retained content.

use gpui_kit::{
    Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, IntoElement, Window, div, prelude::*,
};
use gpui_kit::{
    base::{InteractiveElementExt as _, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable as _, Sizable as _,
        button::{Button, ButtonCustomVariant, ButtonGroup, ButtonVariants as _},
        resizable::{HandleEdge, resize_handle},
        scroll::ScrollableElement as _,
    },
};

use crate::reader::{AnnotationsPanel, ContentsPanel, FindPanel};

gpui_kit::actions!(
    kver,
    [ShowContents, ShowFind, ShowAnnotations, ToggleSidebar]
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarPanel {
    #[default]
    Contents,
    Find,
    Annotations,
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
    find: Entity<FindPanel>,
    contents: Entity<ContentsPanel>,
    annotations: Entity<AnnotationsPanel>,
}

impl ReaderSidebar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            active: None,
            last: SidebarPanel::default(),
            width_rems: DEFAULT_WIDTH_REMS,
            find: cx.new(|cx| FindPanel::new(window, cx)),
            contents: cx.new(ContentsPanel::new),
            annotations: cx.new(|cx| AnnotationsPanel::new(window, cx)),
        }
    }

    fn bounded_width(width_rems: f32, window_rems: f32) -> f32 {
        // The PDF stays wider even when a narrow window cannot accommodate
        // the sidebar's preferred minimum width.
        let maximum = MAX_WIDTH_REMS.min(window_rems * 0.4);
        width_rems.clamp(MIN_WIDTH_REMS.min(maximum), maximum)
    }

    fn resize(&mut self, position: f32, window: &Window, cx: &mut Context<Self>) {
        let rem = f32::from(window.rem_size());
        let window_rems = f32::from(window.bounds().size.width) / rem;
        self.width_rems = Self::bounded_width(position / rem, window_rems);
        cx.notify();
    }

    fn reset_width(&mut self, cx: &mut Context<Self>) {
        self.width_rems = DEFAULT_WIDTH_REMS;
        cx.notify();
    }

    pub fn active(&self) -> Option<SidebarPanel> {
        self.active
    }

    pub fn last(&self) -> SidebarPanel {
        self.last
    }

    pub(super) fn width_rems(&self, window: &Window) -> f32 {
        if self.active.is_none() {
            return 0.;
        }
        Self::bounded_width(
            self.width_rems,
            f32::from(window.bounds().size.width) / f32::from(window.rem_size()),
        )
    }

    pub(crate) fn find(&self) -> &Entity<FindPanel> {
        &self.find
    }

    pub(crate) fn contents(&self) -> &Entity<ContentsPanel> {
        &self.contents
    }

    pub(crate) fn annotations(&self) -> &Entity<AnnotationsPanel> {
        &self.annotations
    }

    pub fn open(&mut self, panel: SidebarPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.active = Some(panel);
        self.last = panel;
        match panel {
            SidebarPanel::Find => self.find.update(cx, |find, cx| find.focus(window, cx)),
            SidebarPanel::Contents => self
                .contents
                .update(cx, |contents, cx| contents.focus(window, cx)),
            SidebarPanel::Annotations => self
                .annotations
                .update(cx, |panel, cx| panel.focus(window, cx)),
        }
        cx.notify();
    }

    pub fn close(
        &mut self,
        window: &mut Window,
        reader_focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) {
        self.active = None;
        window.focus(reader_focus, cx);
        cx.notify();
    }
}

impl Render for ReaderSidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = self.active else {
            return Empty.into_any_element();
        };
        let width_rems = self.width_rems(window);
        let panel = match active {
            SidebarPanel::Annotations => div()
                .flex_1()
                .min_h_0()
                .child(self.annotations.clone())
                .into_any_element(),
            SidebarPanel::Find => div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p_3()
                .child(self.find.clone())
                .into_any_element(),
            SidebarPanel::Contents => div()
                .flex_1()
                .min_h_0()
                .flex()
                .p_2()
                .child(self.contents.clone())
                .into_any_element(),
        };
        div()
            .id("reader-sidebar")
            .test_support()
            .relative()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizeSidebar>, window, cx| {
                    this.resize(
                        f32::from(event.event.position.x - event.bounds.left()),
                        window,
                        cx,
                    );
                }),
            )
            .key_context(crate::key_context::SIDEBAR)
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
                div()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        ButtonGroup::new("sidebar-panels")
                            .w_full()
                            .small()
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(cx.theme().sidebar)
                                    .hover(cx.theme().accent)
                                    .active(cx.theme().selection),
                            )
                            .child(
                                Button::new("sidebar-contents")
                                    .label("Contents")
                                    .tooltip("Contents")
                                    .flex_1()
                                    .min_w_0()
                                    .selected(active == SidebarPanel::Contents)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ShowContents), cx);
                                    }),
                            )
                            .child(
                                Button::new("sidebar-annotations")
                                    .label("Annotations")
                                    .tooltip("Annotations")
                                    .flex_1()
                                    .min_w_0()
                                    .selected(active == SidebarPanel::Annotations)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ShowAnnotations), cx);
                                    }),
                            )
                            .child(
                                Button::new("sidebar-find")
                                    .label("Find")
                                    .tooltip("Find")
                                    .flex_1()
                                    .min_w_0()
                                    .selected(active == SidebarPanel::Find)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ShowFind), cx);
                                    }),
                            ),
                    ),
            )
            .child(
                resize_handle("sidebar-resize-handle", Axis::Horizontal)
                    .inside(HandleEdge::Trailing)
                    .on_drag(ResizeSidebar, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_double_click(cx.listener(|this, _, _, cx| {
                        this.reset_width(cx);
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_preserves_room_for_the_document_even_in_narrow_windows() {
        assert_eq!(ReaderSidebar::bounded_width(1., 100.), MIN_WIDTH_REMS);
        assert_eq!(ReaderSidebar::bounded_width(100., 100.), MAX_WIDTH_REMS);
        assert_eq!(ReaderSidebar::bounded_width(DEFAULT_WIDTH_REMS, 20.), 8.);
        assert_eq!(ReaderSidebar::bounded_width(DEFAULT_WIDTH_REMS, 0.), 0.);
    }
}
