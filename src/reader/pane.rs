//! A second viewport joins the existing session without reopening the PDF or
//! replaying persisted reading state. Entity identity owns all pending work.

use super::*;

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

    pub(crate) fn set_pane_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.record_active = active;
        if !active {
            self.invalidate_hints(cx);
        } else {
            self.record_position(cx);
        }
        cx.notify();
    }
}
