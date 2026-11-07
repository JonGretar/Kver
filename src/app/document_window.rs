//! Per-window composition and native file-open routing.
//!
//! The reader owns the active document and its load/restoration lifecycle.
//! This owner mounts it once and handles commands shared by all focused regions.

mod chrome;
pub mod command_palette;
pub mod sidebar;

use std::path::PathBuf;

use gpui_kit::{
    Action, App, Context, Entity, Focusable as _, Subscription, Task, Window,
    base::TestSupportExt as _,
    component::{ActiveTheme, theme::Theme},
    div,
    prelude::*,
};

use super::windows::{self, CloseWindow, NextWindow, PreviousWindow};
use crate::{
    reader::{
        Continuous, Copy, DeleteLocalMark, DocumentEvent, FitPage, FitWidth, JumpLocalMark,
        MarksView, NextMatch, NextPage, Preview, PreviousMatch, PreviousPage, SetLocalMark,
        ShowContents, ShowDeleteMark, ShowFind, ShowJumpMark, ShowMarks, ShowSetMark, SinglePage,
        ToggleSidebar, UseDefaultReadingMode, ZoomIn, ZoomOut,
    },
    reading_history::{
        ShowRecent,
        recent_picker::{RecentPicker, RecentPickerEvent},
    },
    settings::Appearance,
    settings_ui,
};
use command_palette::{
    CommandAvailability, CommandPalette, DismissCommandPalette, ShowCommandPalette,
};

gpui_kit::actions!(kver, [OpenPdf]);

pub struct DocumentWindow {
    reader: Entity<Preview>,
    file_picker: Option<Task<()>>,
    recent: Option<Entity<RecentPicker>>,
    recent_subscription: Option<Subscription>,
    recent_open: Option<PendingRecentOpen>,
    palette: Option<CommandPalette>,
    _reader_observation: Subscription,
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
        let reader_observation = cx.observe(&reader, |_, _, cx| cx.notify());
        Self {
            reader,
            file_picker: None,
            recent: None,
            recent_subscription: None,
            recent_open: None,
            palette: None,
            _reader_observation: reader_observation,
            _document_subscription: document_subscription,
            _appearance_subscription: appearance_subscription,
        }
    }

    /// The window owns the reader; callers use its existing navigation and
    /// diagnostic interface rather than a second copy of document state.
    pub fn reader(&self) -> &Entity<Preview> {
        &self.reader
    }

    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    pub fn palette_query(&self, cx: &App) -> Option<gpui_kit::SharedString> {
        self.palette.as_ref().map(|palette| palette.query(cx))
    }

    fn show_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() || self.recent.is_some() {
            return;
        }
        self.reader.update(cx, |reader, cx| reader.hide_marks(cx));
        self.palette = Some(CommandPalette::new(window, cx));
        cx.notify();
    }

    fn dismiss_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The selected action may already have replaced the palette with marks
        // or Recent before Command's deferred confirmation runs.
        if self.palette.take().is_some() {
            self.reader
                .update(cx, |reader, cx| reader.restore_focus(window, cx));
            cx.notify();
        }
    }

    fn show_marks(&mut self, view: MarksView, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_some() {
            return;
        }
        if self
            .reader
            .update(cx, |reader, cx| reader.show_marks(view, window, cx))
        {
            self.palette = None;
            cx.notify();
        }
    }

    /// Sibling overlays use the reader's existing Action handlers without
    /// moving focus or duplicating navigation/availability policy here.
    fn route_reader_action<A: Action>(
        &mut self,
        action: &A,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reader
            .read(cx)
            .focus_handle(cx)
            .dispatch_action(action, window, cx);
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
        self.palette = None;
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
        let reader = self.reader.read(cx);
        let title = chrome::title_bar(reader.path(), cx);
        let marks = reader.marks_overlay();
        div()
            .id("document-window")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &OpenPdf, _, cx| this.open_dialog(cx)))
            .on_action(cx.listener(|this, _: &ShowRecent, window, cx| this.show_recent(window, cx)))
            .on_action(
                cx.listener(|this, _: &ShowCommandPalette, window, cx| {
                    this.show_palette(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DismissCommandPalette, window, cx| {
                this.dismiss_palette(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowMarks, window, cx| {
                this.show_marks(MarksView::List, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSetMark, window, cx| {
                this.show_marks(MarksView::Set, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowJumpMark, window, cx| {
                this.show_marks(MarksView::Jump, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowDeleteMark, window, cx| {
                this.show_marks(MarksView::Delete, window, cx)
            }))
            .on_action(cx.listener(Self::route_reader_action::<NextPage>))
            .on_action(cx.listener(Self::route_reader_action::<PreviousPage>))
            .on_action(cx.listener(Self::route_reader_action::<ZoomIn>))
            .on_action(cx.listener(Self::route_reader_action::<ZoomOut>))
            .on_action(cx.listener(Self::route_reader_action::<FitWidth>))
            .on_action(cx.listener(Self::route_reader_action::<FitPage>))
            .on_action(cx.listener(Self::route_reader_action::<SinglePage>))
            .on_action(cx.listener(Self::route_reader_action::<Continuous>))
            .on_action(cx.listener(Self::route_reader_action::<UseDefaultReadingMode>))
            .on_action(cx.listener(Self::route_reader_action::<Copy>))
            .on_action(cx.listener(Self::route_reader_action::<ShowContents>))
            .on_action(cx.listener(Self::route_reader_action::<ShowFind>))
            .on_action(cx.listener(Self::route_reader_action::<ToggleSidebar>))
            .on_action(cx.listener(Self::route_reader_action::<NextMatch>))
            .on_action(cx.listener(Self::route_reader_action::<PreviousMatch>))
            .on_action(cx.listener(Self::route_reader_action::<SetLocalMark>))
            .on_action(cx.listener(Self::route_reader_action::<JumpLocalMark>))
            .on_action(cx.listener(Self::route_reader_action::<DeleteLocalMark>))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(|_: &NextWindow, window, cx| windows::cycle(window, cx, 1))
            .on_action(|_: &PreviousWindow, window, cx| windows::cycle(window, cx, -1))
            .child(title)
            .child(
                div()
                    .id("reader-workspace")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.reader.clone()),
            )
            .when_some(self.palette.clone(), |root, palette| {
                let reader = self.reader.read(cx);
                root.children(palette.render(
                    CommandAvailability {
                        page_ix: reader.page_index(),
                        page_count: reader.page_count().unwrap_or(0),
                        mode_override: reader.mode_override(),
                        has_selection: reader.selected_text().is_some(),
                        has_matches: reader.has_matches(cx),
                    },
                    cx,
                ))
            })
            .children(marks)
    }
}

#[cfg(test)]
mod tests;
