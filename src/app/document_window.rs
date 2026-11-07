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
use crate::{reader::Preview, settings::Appearance, settings_ui};

gpui_kit::actions!(kver, [OpenPdf]);

pub struct DocumentWindow {
    reader: Entity<Preview>,
    file_picker: Option<Task<()>>,
    _appearance_subscription: Subscription,
}

impl DocumentWindow {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance_subscription = cx.observe_window_appearance(window, |_, window, cx| {
            if settings_ui::state(cx).read(cx).appearance() == Appearance::System {
                Theme::sync_system_appearance(Some(window), cx);
            }
            cx.notify();
        });
        Self {
            reader: cx.new(|cx| Preview::new(path, window.scale_factor(), cx)),
            file_picker: None,
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
}

impl Render for DocumentWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(|this, _: &OpenPdf, _, cx| this.open_dialog(cx)))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(|_: &NextWindow, window, cx| windows::cycle(window, cx, 1))
            .on_action(|_: &PreviousWindow, window, cx| windows::cycle(window, cx, -1))
            .child(self.reader.clone())
    }
}

#[cfg(test)]
mod tests;
