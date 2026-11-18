//! A second viewport joins the existing session without reopening the PDF or
//! replaying persisted reading state. Entity identity owns all pending work.

use super::*;

pub(crate) struct PaneActivation;
impl gpui_kit::EventEmitter<PaneActivation> for ReaderView {}

impl ReaderView {
    pub(crate) fn replacement(path: PathBuf, generation: u64, cx: &mut Context<Self>) -> Self {
        let mut pane = Self::new(None, 1., cx);
        pane.record_active = false;
        pane.focus_on_mount = false;
        pane.document_generation = generation;
        pane.open_path(path, cx);
        pane
    }

    pub(crate) fn duplicate(&self, cx: &mut Context<Self>) -> Self {
        let mut pane = Self::new(None, self.display_scale, cx);
        pane.focus_on_mount = false;
        pane.path = self.path.clone();
        pane.session = self.session.clone();
        pane.session_epoch = self.session_epoch;
        pane.document = self.document.clone();
        pane.document_generation = self.document_generation;
        pane.record_path = self.record_path.clone();
        pane.position_ready = true;
        pane.position_touched = true;
        pane.scale = self.scale;
        pane.zoom_mode = self.zoom_mode;
        pane.reading_mode = self.reading_mode;
        pane.mode_override = self.mode_override;
        pane.pdf_dark_mode_override = self.pdf_dark_mode_override;
        pane.page_colors = self.page_colors;
        pane.page_padding = self.page_padding;
        pane.pages = self.pages.clone();
        pane.page_index = self.page_index;
        pane.selected_page = self.selected_page;
        // The copy starts at the same document-space anchor. The first measured
        // viewport refits independently; it does not create a navigation jump.
        pane.scroll.set_offset(self.scroll.offset());
        pane.status = self.ready_status();
        if let Some(session) = &pane.session {
            let participant = cx.entity_id();
            session.update(cx, |session, _| session.attach(participant));
            pane.session_observation =
                Some(cx.observe(session, |pane, _, cx| pane.sync_shared_session(cx)));
        }
        pane
    }

    pub(crate) fn accepts_pane_command(&self, window: &Window) -> bool {
        self.focus.is_focused(window) || self.hints.focused(window)
    }

    pub(super) fn cancel_pane_interaction(&mut self, cx: &mut Context<Self>) {
        self.invalidate_hints(cx);
        self.ruler.cancel_work();
        self.selection.finish();
        self.annotation_press = None;
        self.annotation_choice = None;
    }

    pub(crate) fn set_pane_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.record_active = active;
        if !active {
            // Cancel intent, not completed document-space reading state. A late
            // extraction must not resume a traversal after switching away.
            self.cancel_pane_interaction(cx);
        } else {
            // Another pane may have recorded since this pane last moved.
            self.recorded_position = None;
            self.record_position(cx);
        }
        cx.notify();
    }

    /// Retained callback handles must not keep a removed viewport participating
    /// in the session or writing the window's ordinary reading position.
    pub(crate) fn retire_pane(&mut self, cx: &mut Context<Self>) {
        self.detach_session(cx);
        self.document_generation += 1;
        self.record_active = false;
        self.record_path = None;
        self.pending_restore = None;
        self.ruler = ruler::Ruler::default();
        self.ruler_history = ruler_history::RulerHistory::default();
        self.marks = None;
        self.marks_subscriptions.clear();
        self.links.reset();
        self.rasters.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document_window::{ClosePane, DocumentWindow, SplitSideBySide};
    use gpui_kit::{TestAppContext, test::TestWindowExt};

    #[gpui_kit::test]
    fn removed_pane_rejects_late_text_and_hint_work_even_when_retained(cx: &mut TestAppContext) {
        stale_pane_work(cx, false);
    }

    #[gpui_kit::test]
    fn replaced_panes_reject_late_text_and_hint_work_even_when_retained(cx: &mut TestAppContext) {
        stale_pane_work(cx, true);
    }

    fn stale_pane_work(cx: &mut TestAppContext, replace: bool) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            crate::settings_ui::install_with_paths(cx, None);
        });
        let (handle, document) = cx.update(|cx| {
            gpui_kit::open_window(Default::default(), cx, |window, cx| {
                cx.new(|cx| {
                    DocumentWindow::new(Some("examples/selection-sample.pdf".into()), window, cx)
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.simulate_window_resize(handle, gpui::size(px(1200.), px(900.)));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(SplitSideBySide), cx);
        })
        .unwrap();
        cx.run_until_parked();
        let removed = document.read_with(cx, |d, _| d.reader().clone());
        let pending = removed.update(cx, |r, _| {
            r.page_text = page_text::PageTexts::default();
            r.page_text
                .request(0..1, None, [None], r.document_generation)
                .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(VisualSelect), cx);
        })
        .unwrap();
        cx.run_until_parked();
        removed.read_with(cx, |r, _| assert!(r.hints.is_active()));
        if replace {
            document.update(cx, |d, cx| {
                d.open_path("examples/field-guide.pdf".into(), cx)
            });
        } else {
            cx.update_window(handle, |_, window, cx| {
                window.dispatch_action(Box::new(ClosePane), cx);
            })
            .unwrap();
        }
        cx.run_until_parked();
        let survivor = document.read_with(cx, |d, _| d.reader().clone());
        assert_ne!(survivor, removed);
        removed.update(cx, |r, _| {
            assert!(
                !r.page_text
                    .finish(pending, Err("late removed-pane reply".into()))
            );
            assert!(!r.hints.is_active());
            assert!(r.acquisition.is_none());
            assert!(r.session.is_none());
            assert!(r.ruler.pending.is_none());
            assert!(r.ruler.traversal.is_none());
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("hint-input").is_none());
            assert!(survivor.read(cx).focus.is_focused(window));
            assert!(!survivor.read(cx).is_dirty(cx));
        })
        .unwrap();
    }
}
