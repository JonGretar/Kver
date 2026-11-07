//! Per-window composition and native file-open routing.
//!
//! The reader owns the active document and its load/restoration lifecycle.
//! This owner mounts it once and handles commands shared by all focused regions.

pub mod command_palette;
pub mod sidebar;

use std::path::PathBuf;

use gpui_kit::{
    Context, Entity, Subscription, Task, Window, component::theme::Theme, div, prelude::*,
};

use super::windows::{self, CloseWindow, NextWindow, PreviousWindow};
use crate::{
    reader::{DocumentEvent, Preview},
    reading_history::{
        ShowRecent,
        recent_picker::{RecentPicker, RecentPickerEvent},
    },
    settings::Appearance,
    settings_ui,
};

gpui_kit::actions!(kver, [OpenPdf]);

pub struct DocumentWindow {
    reader: Entity<Preview>,
    file_picker: Option<Task<()>>,
    recent: Option<Entity<RecentPicker>>,
    recent_subscription: Option<Subscription>,
    recent_open: Option<PendingRecentOpen>,
    _document_subscription: Subscription,
    _appearance_subscription: Subscription,
}

struct PendingRecentOpen {
    generation: u64,
    picker: gpui_kit::WeakEntity<RecentPicker>,
    request: u64,
}

impl DocumentWindow {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance_subscription = cx.observe_window_appearance(window, |_, window, cx| {
            if settings_ui::state(cx).read(cx).appearance() == Appearance::System {
                Theme::sync_system_appearance(Some(window), cx);
            }
            cx.notify();
        });
        let reader = cx.new(|cx| Preview::new(path, window.scale_factor(), cx));
        let document_subscription =
            cx.subscribe_in(&reader, window, |this, _, event, window, cx| {
                this.on_document_event(event, window, cx);
            });
        Self {
            reader,
            file_picker: None,
            recent: None,
            recent_subscription: None,
            recent_open: None,
            _document_subscription: document_subscription,
            _appearance_subscription: appearance_subscription,
        }
    }

    /// The window owns the reader; callers use its existing navigation and
    /// diagnostic interface rather than a second copy of document state.
    pub fn reader(&self) -> &Entity<Preview> {
        &self.reader
    }

    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.file_picker = None;
        self.reader
            .update(cx, |reader, cx| reader.open_path(path, cx));
    }

    pub fn open_dialog(&mut self, cx: &mut Context<Self>) {
        let generation = self.reader.read(cx).document_generation();
        let choice = rfd::AsyncFileDialog::new()
            .set_title("Open PDF")
            .add_filter("PDF documents", &["pdf"])
            .pick_file();
        self.file_picker = Some(cx.spawn(async move |this, cx| {
            let path = choice.await.map(|file| file.path().to_path_buf());
            let _ = this.update(cx, |this, cx| {
                this.finish_file_choice(generation, path, cx);
            });
        }));
    }

    fn finish_file_choice(
        &mut self,
        generation: u64,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // Recent and platform opens share the reader's document identity.
        // Zoom/raster invalidation is not a new document and must not discard
        // a valid picker result.
        if self.reader.read(cx).document_generation() != generation {
            return;
        }
        if let Some(path) = path {
            self.reader
                .update(cx, |reader, cx| reader.open_path(path, cx));
        }
    }

    fn show_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_some() {
            return;
        }
        self.reader.update(cx, |reader, cx| reader.hide_palette(cx));
        let excluded = self.reader.read(cx).record_path().map(ToOwned::to_owned);
        let picker = cx.new(|cx| RecentPicker::new(excluded, window, cx));
        self.recent_subscription = Some(cx.subscribe(&picker, |this, picker, event, cx| {
            if this.recent.as_ref() != Some(&picker) {
                return;
            }
            match event {
                RecentPickerEvent::Open { path, request } => {
                    this.open_path(path.clone(), cx);
                    this.recent_open = Some(PendingRecentOpen {
                        generation: this.reader.read(cx).document_generation(),
                        picker: picker.downgrade(),
                        request: *request,
                    });
                }
                RecentPickerEvent::Closed => {
                    this.recent = None;
                    this.recent_subscription = None;
                }
            }
            cx.notify();
        }));
        self.recent = Some(picker.clone());
        picker.update(cx, |picker, cx| picker.show(window, cx));
        cx.notify();
    }

    fn on_document_event(
        &mut self,
        event: &DocumentEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = match event {
            DocumentEvent::Replaced { generation } | DocumentEvent::Opened { generation, .. } => {
                *generation
            }
        };
        if generation != self.reader.read(cx).document_generation() {
            return;
        }
        if self
            .recent_open
            .as_ref()
            .is_some_and(|pending| pending.generation != generation)
            && let Some(pending) = self.recent_open.take()
        {
            let _ = pending
                .picker
                .update(cx, |picker, cx| picker.cancel_open(pending.request, cx));
        }
        if let Some(picker) = &self.recent {
            let excluded = self.reader.read(cx).record_path().map(ToOwned::to_owned);
            picker.update(cx, |picker, cx| picker.set_active_path(excluded, cx));
        }
        if let DocumentEvent::Opened { result, .. } = event
            && let Some(pending) = self.recent_open.take()
        {
            // Reply to the initiating picker, never to whichever modal happens
            // to be open now. A reopened dialog has a different entity identity.
            let _ = pending.picker.update(cx, |picker, cx| {
                picker.finish_open(pending.request, result.clone(), window, cx);
            });
        }
    }
}

impl Render for DocumentWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(|this, _: &OpenPdf, _, cx| this.open_dialog(cx)))
            .on_action(cx.listener(|this, _: &ShowRecent, window, cx| this.show_recent(window, cx)))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(|_: &NextWindow, window, cx| windows::cycle(window, cx, 1))
            .on_action(|_: &PreviousWindow, window, cx| windows::cycle(window, cx, -1))
            .child(self.reader.clone())
    }
}

#[cfg(test)]
mod tests;
