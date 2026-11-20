//! Two retained reader entities, not a general workspace/layout framework.
//! `reader` and `jumplist` on DocumentWindow always name the active pane.
//! Swapping the active slot moves its subscriptions with it, never its identity.

use super::*;
use gpui_kit::{
    AnyElement, Axis, Bounds, DragMoveEvent, EntityId, Pixels, canvas,
    component::resizable::{HandleEdge, resize_handle},
    px,
};

pub(super) struct Pane {
    pub(super) reader: Entity<ReaderView>,
    jumplist: Jumplist,
    subscriptions: Vec<Subscription>,
}

pub(super) struct Replacement {
    pub(super) reader: Entity<ReaderView>,
    pub(super) path: PathBuf,
    pub(super) recent_request: Option<u64>,
    _subscription: Subscription,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Orientation {
    #[default]
    SideBySide,
    Stacked,
}

impl Orientation {
    fn axis(self) -> Axis {
        match self {
            Self::SideBySide => Axis::Horizontal,
            Self::Stacked => Axis::Vertical,
        }
    }
}

pub(super) struct Split {
    orientation: Orientation,
    fraction: f32,
    active_first: bool,
    bounds: Bounds<Pixels>,
}

impl Default for Split {
    fn default() -> Self {
        Self {
            orientation: Orientation::SideBySide,
            fraction: 0.5,
            active_first: true,
            bounds: Bounds::default(),
        }
    }
}

// A reading region needs room for a passage, not a second copy of the toolbar.
// If the window is subsequently made smaller, share its reduced extent without
// changing orientation or losing a pane.
fn divided_extent(extent: f32, minimum: f32, fraction: f32) -> f32 {
    let extent = extent.max(0.);
    let minimum = minimum.min(extent / 2.).max(0.);
    (extent * fraction).clamp(minimum, extent - minimum)
}

#[derive(Clone)]
struct ResizePanes;
impl Render for ResizePanes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

impl DocumentWindow {
    fn accepts_pane_command(&self, window: &mut Window, cx: &mut App) -> bool {
        self.palette.is_some()
            || (self.reader.read(cx).accepts_pane_command(window)
                && self.recent.is_none()
                && !window.has_active_dialog(cx))
    }

    pub(super) fn observe_pane(
        reader: &Entity<ReaderView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let id = reader.entity_id();
        vec![
            cx.subscribe_in(
                reader,
                window,
                |this, source, _: &crate::reader::PaneActivation, window, cx| {
                    this.activate_pane(source.entity_id(), window, cx);
                },
            ),
            cx.on_focus(
                &reader.read(cx).focus_handle(cx),
                window,
                move |this, window, cx| {
                    this.activate_pane(id, window, cx);
                    this.refresh_navigation_menu(window, cx);
                    let owner = cx.weak_entity();
                    window.on_next_frame(move |window, cx| {
                        let _ =
                            owner.update(cx, |this, cx| this.refresh_navigation_menu(window, cx));
                    });
                },
            ),
            cx.subscribe_in(reader, window, |this, source, event, window, cx| {
                if let DocumentEvent::Updated { generation } = event
                    && *generation == source.read(cx).document_generation()
                    && let Some(current) = source.read(cx).jump_location()
                {
                    if source == &this.reader {
                        this.jumplist.follow_saved_path(&current);
                    } else if let Some(pane) = &mut this.other_pane
                        && &pane.reader == source
                    {
                        pane.jumplist.follow_saved_path(&current);
                    }
                }
                if source == &this.reader {
                    this.on_document_event(event, window, cx);
                } else if let Some(pane) = &mut this.other_pane
                    && &pane.reader == source
                    && matches!(event, DocumentEvent::Replaced { .. })
                {
                    pane.jumplist.clear();
                }
            }),
            cx.subscribe_in(
                reader,
                window,
                |this, source, jump: &JumpEvent, window, cx| {
                    if source.read(cx).jump_location().as_ref() != Some(&jump.to) {
                        return;
                    }
                    if source == &this.reader {
                        this.jumplist.record(jump);
                    } else if let Some(pane) = &mut this.other_pane
                        && &pane.reader == source
                    {
                        pane.jumplist.record(jump);
                    } else {
                        return;
                    }
                    this.refresh_navigation_menu(window, cx);
                    cx.notify();
                },
            ),
            cx.observe_in(reader, window, |this, source, window, cx| {
                if source == this.reader {
                    chrome::sync_window_identity(source.read(cx), window, cx);
                    this.refresh_navigation_menu(window, cx);
                    this.offer_external_change(window, cx);
                }
                cx.notify();
            }),
        ]
    }

    fn activate_pane(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(other) = &mut self.other_pane else {
            return;
        };
        if other.reader.entity_id() != id {
            return;
        }
        self.reader
            .update(cx, |reader, cx| reader.set_pane_active(false, cx));
        std::mem::swap(&mut self.reader, &mut other.reader);
        std::mem::swap(&mut self.jumplist, &mut other.jumplist);
        std::mem::swap(&mut self.pane_subscriptions, &mut other.subscriptions);
        self.split.active_first = !self.split.active_first;
        self.reader
            .update(cx, |reader, cx| reader.set_pane_active(true, cx));
        // The sidebar and its Find query are window-owned and are not rebuilt
        // merely because the intended reader changes.
        self.sync_sidebar_target(cx);
        chrome::sync_window_identity(self.reader.read(cx), window, cx);
        self.refresh_navigation_menu(window, cx);
        cx.notify();
    }

    pub(super) fn pane_availability(&self, window: &Window, cx: &App) -> (bool, bool, bool) {
        let has_document =
            self.reader.read(cx).page_count().is_some() && self.replacement.is_none();
        let rem = f32::from(window.rem_size());
        let size = self.split.bounds.size;
        (
            has_document
                && f32::from(size.width) >= 32. * rem
                && f32::from(size.height) >= 10. * rem,
            has_document
                && f32::from(size.width) >= 16. * rem
                && f32::from(size.height) >= 20. * rem,
            self.other_pane.is_some(),
        )
    }

    pub(super) fn arrange_panes(
        &mut self,
        orientation: Orientation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_pane_command(window, cx) {
            return;
        }
        self.dismiss_palette(window, cx);
        let (side_by_side, stacked, _) = self.pane_availability(window, cx);
        if !match orientation {
            Orientation::SideBySide => side_by_side,
            Orientation::Stacked => stacked,
        } {
            return;
        }
        if self.ensure_other_pane(window, cx) {
            self.focus_other_pane(window, cx);
        }
        self.set_split_orientation(orientation, window, cx);
    }

    fn ensure_other_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.other_pane.is_some() {
            return false;
        }
        let source = self.reader.clone();
        let reader = source.update(cx, |source, cx| cx.new(|cx| source.duplicate(cx)));
        let subscriptions = Self::observe_pane(&reader, window, cx);
        self.other_pane = Some(Pane {
            reader,
            jumplist: Jumplist::default(),
            subscriptions,
        });
        self.split.active_first = true;
        true
    }

    fn set_split_orientation(
        &mut self,
        orientation: Orientation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split.orientation = orientation;
        self.split.fraction = 0.5;
        self.schedule_refit(window, cx);
        cx.notify();
    }

    pub(super) fn open_destination_in_split(
        &mut self,
        generation: u64,
        destination: crate::pdf::OutlineDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.reader.read(cx);
        if generation != source.document_generation()
            || source
                .page_count()
                .is_none_or(|count| destination.page() >= count)
            || self.replacement.is_some()
        {
            return;
        }
        if self.other_pane.is_none() && !self.pane_availability(window, cx).0 {
            self.reader.update(cx, |reader, cx| {
                reader.report_link_error(
                    "Enlarge the window to open the link side by side.".into(),
                    cx,
                );
            });
            return;
        }
        if self.ensure_other_pane(window, cx) {
            self.set_split_orientation(Orientation::SideBySide, window, cx);
        }
        if let Some(pane) = &self.other_pane {
            pane.reader.update(cx, |reader, cx| {
                reader.open_split_destination(destination, cx);
            });
            self.focus_other_pane(window, cx);
        }
    }

    pub(super) fn switch_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.accepts_pane_command(window, cx) {
            return;
        }
        self.dismiss_palette(window, cx);
        self.focus_other_pane(window, cx);
    }

    fn focus_other_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = &self.other_pane {
            let id = pane.reader.entity_id();
            self.activate_pane(id, window, cx);
            window.focus(&self.reader.read(cx).focus_handle(cx), cx);
        }
    }

    pub(super) fn remove_pane(
        &mut self,
        keep_active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_pane_command(window, cx)
            || self.other_pane.is_none()
            || self.replacement.is_some()
        {
            return;
        }
        self.dismiss_palette(window, cx);
        self.collapse_panes(keep_active, window, cx);
    }

    fn collapse_panes(&mut self, keep_active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !keep_active {
            self.focus_other_pane(window, cx);
        }
        if let Some(pane) = self.other_pane.take() {
            if let Some(sidebar) = &self.sidebar {
                sidebar.read(cx).find().clone().update(cx, |find, _| {
                    find.retire_target(pane.reader.entity_id());
                });
            }
            pane.reader.update(cx, |reader, cx| reader.retire_pane(cx));
        }
        self.split.active_first = true;
        window.focus(&self.reader.read(cx).focus_handle(cx), cx);
        self.schedule_refit(window, cx);
        self.refresh_navigation_menu(window, cx);
        cx.notify();
    }

    pub(super) fn render_panes(&self, cx: &mut Context<Self>) -> AnyElement {
        let measure = cx.weak_entity();
        let mut region = div()
            .id("reading-panes")
            .test_support()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let _ = measure.update(cx, |this, cx| {
                            if this.split.bounds != bounds {
                                this.split.bounds = bounds;
                                this.refresh_navigation_menu(window, cx);
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        let Some(other) = &self.other_pane else {
            return region.child(self.reader.clone()).into_any_element();
        };
        let orientation = self.split.orientation;
        let axis = orientation.axis();
        let extent = match orientation {
            Orientation::SideBySide => self.split.bounds.size.width,
            Orientation::Stacked => self.split.bounds.size.height,
        };
        let first_extent = divided_extent(
            f32::from(extent),
            f32::from(cx.theme().font_size)
                * match orientation {
                    Orientation::SideBySide => 16.,
                    Orientation::Stacked => 10.,
                },
            self.split.fraction,
        );
        let (first, second) = if self.split.active_first {
            (&self.reader, &other.reader)
        } else {
            (&other.reader, &self.reader)
        };
        region = region
            .when(orientation == Orientation::Stacked, |region| {
                region.flex_col()
            })
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<ResizePanes>, window, cx| {
                    let (distance, extent, minimum) = match this.split.orientation {
                        Orientation::SideBySide => (
                            event.event.position.x - event.bounds.left(),
                            event.bounds.size.width,
                            window.rem_size() * 16.,
                        ),
                        Orientation::Stacked => (
                            event.event.position.y - event.bounds.top(),
                            event.bounds.size.height,
                            window.rem_size() * 10.,
                        ),
                    };
                    if extent > px(0.) {
                        this.split.fraction = divided_extent(
                            extent.into(),
                            minimum.into(),
                            f32::from(distance) / f32::from(extent),
                        ) / f32::from(extent);
                        this.schedule_refit(window, cx);
                        cx.notify();
                    }
                },
            ));
        let pane = |reader: &Entity<ReaderView>, first: bool| {
            let id = reader.entity_id();
            let active = reader == &self.reader;
            div()
                .id(("reading-pane", id))
                .test_support()
                .role(gpui_kit::Role::Group)
                .aria_label(if active {
                    "Active reading pane"
                } else {
                    "Reading pane"
                })
                .relative()
                .flex()
                .flex_col()
                .min_w_0()
                .min_h_0()
                // Reserve the same hairline in both panes: activation changes
                // the presence of a frame, never the content's measured size.
                .border_1()
                .border_color(cx.theme().primary.opacity(if active { 1. } else { 0. }))
                .when(first, |pane| match orientation {
                    Orientation::SideBySide => pane.w(px(first_extent)).h_full().flex_shrink_0(),
                    Orientation::Stacked => pane.h(px(first_extent)).w_full().flex_shrink_0(),
                })
                .when(!first, |pane| pane.flex_1())
                .on_mouse_down(
                    gpui_kit::MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.activate_pane(id, window, cx);
                        let focus = this.reader.read(cx).focus_handle(cx);
                        if !focus.contains_focused(window, cx) {
                            window.focus(&focus, cx);
                        }
                    }),
                )
                .child(div().flex_1().min_w_0().min_h_0().child(reader.clone()))
                .when(first, |pane| {
                    pane.child(
                        resize_handle("pane-divider", axis)
                            .inside(HandleEdge::Trailing)
                            .on_drag(ResizePanes, |drag, _, _, cx| cx.new(|_| drag.clone())),
                    )
                })
        };
        region
            .child(pane(first, true))
            .child(pane(second, false))
            .into_any_element()
    }

    pub(super) fn stage_replacement(&mut self, source: DocumentSource, cx: &mut Context<Self>) {
        self.stage_replacement_after_decision(source, false, cx);
    }

    pub(super) fn stage_replacement_after_decision(
        &mut self,
        source: DocumentSource,
        discard: bool,
        cx: &mut Context<Self>,
    ) {
        let target = source.name_or_path(cx);
        if self
            .recent_open
            .as_ref()
            .is_some_and(|pending| pending.path != target)
            && let Some(pending) = self.recent_open.take()
        {
            let _ = pending
                .picker
                .update(cx, |picker, cx| picker.cancel_open(pending.request, cx));
        }
        let recent_request = self.recent_open.as_ref().map(|pending| pending.request);
        let origin = self.reader.read(cx).session();
        let revision = origin.as_ref().map(|session| {
            let session = session.read(cx);
            (session.epoch(), session.revision())
        });
        let generation = self.reader.read(cx).document_generation();
        let reader = cx.new(|cx| ReaderView::replacement(source, generation, cx));
        let handle = self.window;
        let subscription = cx.subscribe(&reader, move |this, source, event, cx| {
            let DocumentEvent::Opened { result, .. } = event else {
                return;
            };
            if this
                .replacement
                .as_ref()
                .is_none_or(|pending| pending.reader != source)
            {
                return;
            }
            let owner = cx.weak_entity();
            let source = source.clone();
            let mut result = result.clone();
            let origin = origin.clone();
            cx.defer(move |cx| {
                let _ = cx.update_window(handle, |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if this
                            .replacement
                            .as_ref()
                            .is_none_or(|pending| pending.reader != source)
                        {
                            return;
                        }
                        let recent_request = this.replacement.take().and_then(|pending| pending.recent_request);
                        if this.reader.read(cx).session() != origin
                            || origin.as_ref().map(|session| {
                                let session = session.read(cx);
                                (session.epoch(), session.revision())
                            }) != revision
                        {
                            result = Err("The current PDF changed while opening its replacement. Both panes were kept; try opening again.".into());
                        }
                        if result.is_ok() {
                            // A picker or decision opened on the old document
                            // while loading must not survive adoption.
                            if this.pending_leave.is_some()
                                || (this.save_prompt_open && this.save_picker.is_none())
                            {
                                window.close_dialog(cx);
                            }
                            this.file_picker = None;
                            this.save_picker = None;
                            this.save_observation = None;
                            this.cancel_pending_leave(cx);
                            // Approval to discard is not a discard until the
                            // replacement has loaded and the source is unchanged.
                            if discard && let Some(origin) = &origin {
                                origin.update(cx, |session, cx| session.discard(cx));
                            }
                            let reader_focused = this.reader.read(cx).accepts_pane_command(window);
                            if let Some(other) = this.other_pane.take() {
                                other.reader.update(cx, |reader, cx| reader.retire_pane(cx));
                            }
                            this.split.active_first = true;
                            this.reader
                                .update(cx, |reader, cx| reader.retire_pane(cx));
                            this.reader = source.clone();
                            this.reader.update(cx, |reader, cx| reader.set_pane_active(true, cx));
                            this.pane_subscriptions = Self::observe_pane(&source, window, cx);
                            this.jumplist.clear();
                            this.sync_sidebar_document(window, cx);
                            this.schedule_refit(window, cx);
                            if reader_focused {
                                window.focus(&source.read(cx).focus_handle(cx), cx);
                            }
                        } else if let Err(error) = &result {
                            window.push_notification(Notification::error(error.clone()), cx);
                        }
                        if this.recent_open.as_ref().is_some_and(|pending| Some(pending.request) == recent_request)
                            && let Some(pending) = this.recent_open.take()
                        {
                            pending.finish(result, window, cx);
                        }
                        this.refresh_navigation_menu(window, cx);
                        cx.notify();
                    });
                });
            });
        });
        self.replacement = Some(Replacement {
            reader,
            path: target,
            recent_request,
            _subscription: subscription,
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_clamps_and_never_loses_a_pane_in_a_small_region() {
        assert_eq!(divided_extent(1000., 256., 0.5), 500.);
        assert_eq!(divided_extent(1000., 256., -1.), 256.);
        assert_eq!(divided_extent(1000., 256., 2.), 744.);
        assert_eq!(divided_extent(400., 256., 0.8), 200.);
        assert_eq!(divided_extent(0., 256., 0.8), 0.);
        assert_eq!(divided_extent(1000., 512., 0.8), 500.);
    }
}
