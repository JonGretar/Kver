//! Reader geometry drives layout; only the viewport owns bitmaps.

mod annotations;
mod commands;
mod contents;
pub(crate) mod document_session;
mod find;
mod links;
mod marks;
mod navigation;
mod page_annotations;
mod page_text;
mod rendering;
mod ruler;
mod selection;
pub use crate::app::document_window::OpenPdf;
pub use crate::app::document_window::command_palette::ShowCommandPalette;
pub use crate::app::document_window::sidebar::{
    ShowAnnotations, ShowContents, ShowFind, ToggleSidebar,
};
pub use crate::reading_history::ShowRecent;
pub(crate) use annotations::{AnnotationEdit, AnnotationNavigation, AnnotationsPanel};
pub(crate) use annotations::{markup_color_icon, markup_icon};
pub use commands::*;
pub use contents::ActivateOutline;
pub(crate) use contents::ContentsPanel;
pub(crate) use find::FindPanel;
pub use find::{NextMatch, PreviousMatch};
// Preserve the former public name while callers adopt the narrowed view.
pub use ReaderView as Preview;
use document_session::DocumentSession;
use links::PageLinks;
use marks::MarksEvent;
pub use marks::{
    DeleteLocalMark, JumpLocalMark, SetLocalMark, ShowDeleteMark, ShowJumpMark, ShowMarks,
    ShowSetMark,
};
pub(crate) use marks::{LocalMarks, MarksView};
use navigation::{GAP, PAGE_PADDING, PageLayout, Position};
pub use navigation::{ReadingMode, ZoomMode};
use rendering::{Completion, PageRasters, Schedule};
use selection::Selection;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    coordinates::{PageViewport, Point},
    find_search, pdf,
    recent::{self, ReadingPosition},
    settings::{InitialZoom, ReadingView},
    settings_ui,
};
use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::{
    Animation, AnimationExt, App, BoxShadow, ClipboardItem, Context, Entity, FocusHandle,
    ScrollHandle, Subscription, Window, div, img, point, prelude::*, px, rgb,
};
use gpui_kit::{
    base::TestSupportExt as _,
    component::{
        ActiveTheme,
        button::Button,
        empty::{
            Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant,
            EmptyTitle,
        },
        menu::{ContextMenuExt as _, PopupMenuItem},
        spinner::Spinner,
    },
};

/// Central PDF surface and window-local session participation, independent of chrome.
pub struct ReaderView {
    path: Option<PathBuf>,
    session: Option<Entity<DocumentSession>>,
    session_wait: Option<Subscription>,
    session_observation: Option<Subscription>,
    session_epoch: u64,
    document: Option<Arc<pdf::Document>>,
    record_path: Option<PathBuf>,
    position_ready: bool,
    position_touched: bool,
    pending_restore: Option<ReadingPosition>,
    recorded_position: Option<ReadingPosition>,
    scale: f32,
    zoom_mode: ZoomMode,
    settings_wait: Option<Subscription>,
    reading_mode: ReadingMode,
    mode_override: Option<ReadingMode>,
    display_scale: f32,
    pages: Vec<(f32, f32)>,
    rasters: PageRasters,
    document_generation: u64,
    page_text: page_text::PageTexts,
    ruler: ruler::Ruler,
    selection: Selection,
    // A remembered one-shot command, not a persistent pointer tool.
    active_markup: pdf::annotations::MarkupKind,
    links: PageLinks,
    page_index: usize,
    // A deliberate page turn remains current even if the viewport also shows
    // part of a neighboring page. Scrolling chooses a new current page.
    selected_page: Option<usize>,
    last_viewport: (f32, f32, f32),
    scroll: ScrollHandle,
    focus: FocusHandle,
    focus_initialized: bool,
    marks: Option<Entity<LocalMarks>>,
    marks_subscriptions: Vec<Subscription>,
    find_highlight: Option<(find_search::FindMatch, u64)>,
    annotation_identification: Option<(pdf::annotations::AnnotationId, usize, [f64; 4])>,
    selected_annotation: Option<pdf::annotations::AnnotationId>,
    annotation_press: Option<(usize, gpui::Point<gpui::Pixels>, bool)>,
    annotation_choice: Option<page_annotations::AnnotationChoice>,
    status: String,
}

/// Window-local open lifecycle, independent of the UI that requested it.
pub(crate) enum DocumentEvent {
    Replaced {
        generation: u64,
    },
    Opened {
        generation: u64,
        result: Result<(), String>,
    },
    OpenLinkedDocument {
        generation: u64,
        path: PathBuf,
        page: usize,
    },
    Updated {
        generation: u64,
    },
}

impl gpui::EventEmitter<DocumentEvent> for ReaderView {}

/// A semantic jump, emitted only after a valid destination has been applied.
pub(crate) struct JumpEvent {
    pub(crate) from: JumpLocation,
    pub(crate) to: JumpLocation,
}

/// Window-local logical location; no viewport pixels or transient selection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JumpLocation {
    path: PathBuf,
    generation: u64,
    page: usize,
    x: f32,
    y: f32,
    zoom_mode: ZoomMode,
    scale: f32,
    reading_mode: ReadingMode,
}

impl JumpLocation {
    pub(crate) fn equivalent(&self, other: &Self) -> bool {
        self.path == other.path
            && self.generation == other.generation
            && self.page == other.page
            && (self.x - other.x).abs() < 1.0
            && (self.y - other.y).abs() < 1.0
            && self.zoom_mode == other.zoom_mode
            && (self.scale - other.scale).abs() < 0.001
            && self.reading_mode == other.reading_mode
    }

    #[cfg(test)]
    pub(crate) fn with_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }
}

impl gpui::EventEmitter<JumpEvent> for ReaderView {}

impl gpui::Focusable for ReaderView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Capture history's privacy epoch and opening preference before any I/O. A
/// history clear during a pending open must not let its late reply record again.
#[derive(Clone)]
struct OpenRestoration {
    history: Option<recent::RecentHistory>,
    epoch: Option<u64>,
    restore_position: bool,
}

impl ReaderView {
    pub fn new(path: Option<PathBuf>, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let participant = cx.entity_id();
        cx.on_release(move |reader, cx| {
            if let Some(session) = &reader.session {
                session.update(cx, |session, _| session.detach(participant));
            }
        })
        .detach();
        let mut reader = Self {
            path,
            session: None,
            session_wait: None,
            session_observation: None,
            session_epoch: 0,
            document: None,
            record_path: None,
            position_ready: false,
            position_touched: false,
            pending_restore: None,
            recorded_position: None,
            scale: 1.0,
            zoom_mode: ZoomMode::Fixed,
            settings_wait: None,
            reading_mode: ReadingMode::Continuous,
            mode_override: None,
            display_scale,
            pages: Vec::new(),
            rasters: PageRasters::default(),
            document_generation: 0,
            page_text: page_text::PageTexts::default(),
            ruler: ruler::Ruler::default(),
            selection: Selection::default(),
            active_markup: pdf::annotations::MarkupKind::Highlight,
            links: PageLinks::default(),
            page_index: 0,
            selected_page: None,
            last_viewport: (f32::NAN, f32::NAN, f32::NAN),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            focus_initialized: false,
            marks: None,
            marks_subscriptions: Vec::new(),
            find_highlight: None,
            annotation_identification: None,
            selected_annotation: None,
            annotation_press: None,
            annotation_choice: None,
            status: "Open a PDF to start reading".into(),
        };
        if reader.path.is_some() {
            reader.status = "Loading document…".into();
            reader.load(cx);
        }
        reader
    }

    /// Replace the active document immediately, before any asynchronous work completes.
    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.document_generation += 1;
        self.detach_session(cx);
        self.session_wait = None;
        self.session_observation = None;
        self.session = None;
        self.document = None;
        self.marks = None;
        self.marks_subscriptions.clear();
        self.find_highlight = None;
        self.annotation_identification = None;
        self.selected_annotation = None;
        self.annotation_press = None;
        self.annotation_choice = None;
        self.selection.reset();
        self.page_text = page_text::PageTexts::default();
        self.ruler = ruler::Ruler::default();
        self.links.reset();
        self.path = Some(path);
        self.record_path = None;
        self.position_ready = false;
        self.position_touched = false;
        self.pending_restore = None;
        self.recorded_position = None;
        self.mode_override = None;
        self.scale = 1.0;
        self.zoom_mode = ZoomMode::Fixed;
        self.pages.clear();
        self.rasters.invalidate();
        self.page_index = 0;
        self.selected_page = None;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.last_viewport = (f32::NAN, f32::NAN, f32::NAN);
        self.status = "Loading document…".into();
        cx.emit(DocumentEvent::Replaced {
            generation: self.document_generation,
        });
        self.load(cx);
        cx.notify();
    }

    pub(crate) fn hide_marks(&mut self, cx: &mut Context<Self>) {
        if let Some(marks) = &self.marks {
            marks.update(cx, |marks, cx| marks.hide(cx));
        }
    }

    /// The window mounts this document-owned feature in its full-window layer.
    pub(crate) fn marks_overlay(&self) -> Option<Entity<LocalMarks>> {
        self.marks.clone()
    }

    pub(crate) fn show_marks(
        &mut self,
        view: MarksView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(marks) = self.ensure_marks(cx) else {
            return false;
        };
        marks.update(cx, |marks, cx| {
            marks.show(view, self.focus.clone(), window, cx)
        });
        cx.notify();
        true
    }

    fn marks_open(&self, cx: &App) -> bool {
        self.marks
            .as_ref()
            .is_some_and(|marks| marks.read(cx).is_open())
    }

    fn ensure_marks(&mut self, cx: &mut Context<Self>) -> Option<Entity<LocalMarks>> {
        if self.pages.is_empty() {
            return None;
        }
        let path = self.record_path.clone()?;
        if let Some(marks) = &self.marks {
            return Some(marks.clone());
        }
        let marks = cx.new(|cx| LocalMarks::new(path, self.document_generation, cx));
        self.marks_subscriptions = vec![
            cx.observe(&marks, |_, _, cx| cx.notify()),
            cx.subscribe(&marks, |reader, source, event, cx| {
                if reader.document_generation != source.read(cx).generation()
                    || reader.marks.as_ref() != Some(&source)
                {
                    return;
                }
                match event {
                    MarksEvent::Status(status) => reader.status = status.clone(),
                    MarksEvent::Jump { letter, position } => {
                        if position.page < reader.pages.len()
                            && position.y.is_finite()
                            && position.y >= 0.0
                        {
                            let before = reader.jump_location();
                            reader.position_touched = true;
                            reader.pending_restore = None;
                            reader.restore_position(*position);
                            reader.sync_viewport(cx);
                            reader.retarget_ruler(
                                position.page,
                                ruler::Placement::Page(Point {
                                    x: 0.,
                                    y: position.y as f64,
                                }),
                                cx,
                            );
                            reader.emit_jump(before, cx);
                        } else {
                            reader.status = format!("Local mark {letter} is outside this PDF");
                        }
                    }
                }
                cx.notify();
            }),
        ];
        self.marks = Some(marks.clone());
        Some(marks)
    }

    fn change_mark(
        &mut self,
        letter: char,
        position: Option<ReadingPosition>,
        cx: &mut Context<Self>,
    ) {
        if let Some(marks) = self.ensure_marks(cx) {
            marks.update(cx, |marks, cx| marks.change(letter, position, cx));
        }
    }

    fn jump_mark(&mut self, letter: char, cx: &mut Context<Self>) {
        if let Some(marks) = self.ensure_marks(cx) {
            marks.update(cx, |marks, cx| marks.jump(letter, cx));
        }
    }

    pub(crate) fn set_find_highlight(
        &mut self,
        generation: u64,
        highlight: Option<(find_search::FindMatch, u64)>,
        cx: &mut Context<Self>,
    ) {
        if generation == self.document_generation {
            self.find_highlight = highlight;
            cx.notify();
        }
    }

    pub(crate) fn navigate_find(&mut self, hit: &find_search::FindMatch, cx: &mut Context<Self>) {
        if hit.page >= self.pages.len() {
            return;
        }
        let before = self.jump_location();
        self.position_touched = true;
        let (_, width, height) = self.viewport();
        self.focus_page(hit.page);
        self.apply_fit(
            hit.page,
            Position {
                page: hit.page,
                y: 0.,
            },
            (width, height),
            0.,
        );
        let y = find_search::page_rect(hit, self.pages[hit.page]).1;
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.layout()
                .offset_for(Position { page: hit.page, y }, height * 0.25),
        );
        self.sync_viewport(cx);
        self.retarget_ruler(hit.page, ruler::Placement::Document(hit.corners[0]), cx);
        self.emit_jump(before, cx);
        cx.notify();
    }

    pub(crate) fn navigate_annotation(
        &mut self,
        generation: u64,
        revision: u64,
        annotation: &pdf::annotations::Annotation,
        cx: &mut Context<Self>,
    ) {
        if generation != self.document_generation
            || annotation.page() >= self.pages.len()
            || self.session.as_ref().is_none_or(|session| {
                let session = session.read(cx);
                session.epoch() != annotation.id().epoch() || session.revision() != revision
            })
        {
            return;
        }
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let bounds = document.annotation_bounds(annotation);
        // The list is navigation-only here. Its outline must not leave an older
        // on-page target armed for deletion from the keyboard.
        self.selected_annotation = None;
        self.annotation_identification = bounds.map(|b| (annotation.id(), annotation.page(), b));
        let before = self.jump_location();
        let page = annotation.page();
        self.position_touched = true;
        self.focus_page(page);
        let (_, width, height) = self.viewport();
        self.apply_fit(page, Position { page, y: 0. }, (width, height), 0.);
        let [left, top, _, _] = bounds.unwrap_or([0.; 4]);
        self.scroll_to(
            (left as f32 * self.scale - width * 0.1).max(0.),
            self.layout().offset_for(
                Position {
                    page,
                    y: top as f32,
                },
                height * 0.25,
            ),
        );
        if bounds.is_none() {
            self.status = "This annotation has no usable location; showing its page".into();
        }
        self.sync_viewport(cx);
        self.retarget_ruler(page, ruler::Placement::Page(Point { x: left, y: top }), cx);
        self.emit_jump(before, cx);
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let settings = settings_ui::state(cx);
        if !settings.read(cx).ready() {
            if self.settings_wait.is_none() {
                self.settings_wait = Some(cx.observe(&settings, |reader, settings, cx| {
                    if settings.read(cx).ready() {
                        reader.settings_wait = None;
                        reader.load(cx);
                    }
                }));
            }
            return;
        }
        self.settings_wait = None;
        // Capture the app default once per open; subsequent edits never refit
        // an existing reader. All entry points (new window, picker, Recent, OS)
        // converge on this load path.
        self.scale = 1.0;
        self.zoom_mode = match settings.read(cx).initial_zoom() {
            InitialZoom::ActualSize => ZoomMode::Fixed,
            InitialZoom::FitWidth => ZoomMode::FitWidth,
            InitialZoom::FitPage => ZoomMode::FitPage,
        };
        self.reading_mode = match settings.read(cx).reading_view() {
            ReadingView::Continuous => ReadingMode::Continuous,
            ReadingView::Page => ReadingMode::SinglePage,
        };
        self.mode_override = None;
        let Some(path) = self.path.clone() else {
            return;
        };
        let history = recent::history(cx);
        let restoration = OpenRestoration {
            epoch: history.as_ref().map(|history| history.epoch()),
            history,
            restore_position: settings.read(cx).reopen_at_last_position(),
        };
        let generation = self.document_generation;
        let work = cx.background_executor().spawn(async move {
            std::fs::canonicalize(&path)
                .map_err(|error| format!("Cannot read {}: {error}", path.display()))
        });
        cx.spawn(async move |this, cx| {
            let normalized = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.document_generation {
                    return;
                }
                match normalized {
                    Ok(path) => {
                        let session = DocumentSession::join(path, cx);
                        let participant = cx.entity_id();
                        session.update(cx, |session, _| session.attach(participant));
                        let pending = restoration.clone();
                        this.session_observation =
                            Some(cx.observe(&session, |this, _, cx| this.sync_shared_session(cx)));
                        this.session_wait = Some(cx.observe(&session, move |this, source, cx| {
                            if this.document_generation == generation
                                && this.session.as_ref() == Some(&source)
                            {
                                this.finish_session_load(pending.clone(), cx);
                            }
                        }));
                        this.session = Some(session);
                        this.finish_session_load(restoration, cx);
                    }
                    Err(error) => {
                        this.status = error.clone();
                        cx.emit(DocumentEvent::Opened {
                            generation,
                            result: Err(error),
                        });
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn finish_session_load(&mut self, restoration: OpenRestoration, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(result) = session.read(cx).result() else {
            return;
        };
        let path = session.read(cx).path().to_path_buf();
        let session_epoch = session.read(cx).epoch();
        self.session_wait = None;
        let OpenRestoration {
            history,
            epoch: history_epoch,
            restore_position,
        } = restoration;
        let document_generation = self.document_generation;
        let work = cx.background_executor().spawn(async move {
            let pages = result
                .as_ref()
                .map(|document| document.pages())
                .map_err(Clone::clone);
            (pages, result)
        });
        cx.spawn(async move |this, cx| {
            let (result, document) = work.await;
            let saved_mode = if result.is_ok()
                && let Some(history) = &history
            {
                history
                    .reading_mode(path.clone(), history_epoch.unwrap_or(0))
                    .await
            } else {
                Ok(None)
            };
            let _ = this.update(cx, |this, cx| {
                if document_generation != this.document_generation {
                    return;
                }
                if this
                    .session
                    .as_ref()
                    .is_some_and(|session| session.read(cx).epoch() != session_epoch)
                {
                    this.finish_session_load(
                        OpenRestoration {
                            history,
                            epoch: history_epoch,
                            restore_position,
                        },
                        cx,
                    );
                    return;
                }
                let mut opened = false;
                match result {
                    Ok(pages) if !pages.is_empty() => {
                        opened = true;
                        // A joining window may already have shared unsaved edits,
                        // or receive them while page/history loading is in flight.
                        // Page geometry is unchanged by annotation edits; install
                        // the latest presentation, not the captured saved PDF.
                        this.document = this
                            .session
                            .as_ref()
                            .and_then(|session| session.read(cx).presentation())
                            .or_else(|| document.ok());
                        if let Some(session) = this.session.clone() {
                            this.session_epoch = session.read(cx).epoch();
                            session.update(cx, |session, cx| session.ensure_annotations(cx));
                        }
                        if let Ok(Some(mode)) = saved_mode {
                            this.reading_mode = match mode {
                                ReadingView::Continuous => ReadingMode::Continuous,
                                ReadingView::Page => ReadingMode::SinglePage,
                            };
                            this.mode_override = Some(this.reading_mode);
                        }
                        this.install_pages(pages, cx);
                        if let Err(error) = saved_mode {
                            this.status = format!("Reading mode could not be loaded: {error}");
                        }
                        this.record_path = Some(path.clone());
                        if let Some(history) = history {
                            let record =
                                history.record_at(path.clone(), history_epoch.unwrap_or(0));
                            let position = history.position(path, history_epoch.unwrap_or(0));
                            cx.spawn(async move |this, cx| {
                                let result = match record.await {
                                    Ok(()) => position.await,
                                    Err(error) => Err(error),
                                };
                                let _ = this.update(cx, |this, cx| {
                                    if document_generation != this.document_generation {
                                        return;
                                    }
                                    match result {
                                        Ok(saved) => this.finish_position_load(
                                            if restore_position { saved } else { None },
                                            cx,
                                        ),
                                        Err(error) => {
                                            this.status = format!(
                                                "Reading history could not be loaded: {error}"
                                            );
                                            this.position_ready = true;
                                            cx.notify();
                                        }
                                    }
                                });
                            })
                            .detach();
                        } else {
                            this.finish_position_load(None, cx);
                        }
                    }
                    Ok(_) => this.status = "PDF has no pages".into(),
                    Err(error) => this.status = error,
                }
                cx.emit(DocumentEvent::Opened {
                    generation: document_generation,
                    result: if opened {
                        Ok(())
                    } else {
                        Err(this.status.clone())
                    },
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn install_pages(&mut self, pages: Vec<(f32, f32)>, cx: &mut Context<Self>) {
        self.pages = pages;
        self.selected_page = Some(0);
        self.status = self.ready_status();
        self.sync_viewport(cx);
    }

    fn finish_position_load(&mut self, saved: Option<ReadingPosition>, cx: &mut Context<Self>) {
        if !self.position_touched
            && let Some(position) = saved
            && position.page < self.pages.len()
            && position.y.is_finite()
            && position.y >= 0.0
        {
            self.pending_restore = Some(position);
            self.restore_position(position);
        }
        self.recorded_position = (!self.position_touched).then(|| self.position_to_save());
        self.position_ready = true;
        self.sync_viewport(cx);
    }

    fn restore_position(&mut self, position: ReadingPosition) {
        let y = position.y.min(self.pages[position.page].1);
        self.focus_page(position.page);
        let (_, width, height) = self.viewport();
        self.apply_fit(
            position.page,
            Position {
                page: position.page,
                y,
            },
            (width, height),
            0.0,
        );
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.layout().offset_for(
                Position {
                    page: position.page,
                    y,
                },
                0.0,
            ),
        );
    }

    fn ready_status(&self) -> String {
        format!(
            "Page {} of {} · {}%",
            self.page_index + 1,
            self.pages.len(),
            (self.scale * 100.0).round()
        )
    }

    pub fn status(&self) -> &str {
        if self.ruler.enabled {
            if self.ruler.traversal.is_some() {
                return "Reading ruler: looking for text…";
            }
            if let Some(notice) = self.ruler.notice {
                return notice;
            }
        }
        if self.ruler.enabled && self.ruler.target.is_none() {
            if self.page_text.failed(self.page_index) {
                return "Reading ruler: couldn’t read page text";
            }
            return if self.page_text.get(self.page_index).is_some() {
                "Reading ruler: no usable text on this page"
            } else {
                "Reading ruler: loading page text…"
            };
        }
        &self.status
    }
    pub(crate) fn report_link_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.status = error;
        cx.notify();
    }
    pub fn page_index(&self) -> usize {
        self.page_index
    }
    pub fn zoom_mode(&self) -> ZoomMode {
        self.zoom_mode
    }
    pub fn reading_mode(&self) -> ReadingMode {
        self.reading_mode
    }
    pub fn mode_override(&self) -> Option<ReadingMode> {
        self.mode_override
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    /// Page and PDF-point offset at the top of the viewport.
    pub fn reading_position(&self) -> (usize, f32) {
        let (top, _, _) = self.viewport();
        let position = self.layout().position_at(top);
        (position.page, position.y)
    }
    pub(crate) fn jump_location(&self) -> Option<JumpLocation> {
        let path = self.path.clone()?;
        if self.pages.is_empty() {
            return None;
        }
        let position = self.position_to_save();
        Some(JumpLocation {
            path,
            generation: self.document_generation,
            page: position.page,
            x: (-f32::from(self.scroll.offset().x)).max(0.0) / self.scale,
            y: position.y,
            zoom_mode: self.zoom_mode,
            scale: self.scale,
            reading_mode: self.reading_mode,
        })
    }

    fn emit_jump(&self, before: Option<JumpLocation>, cx: &mut Context<Self>) {
        if let (Some(from), Some(to)) = (before, self.jump_location())
            && !from.equivalent(&to)
        {
            cx.emit(JumpEvent { from, to });
        }
    }

    /// Apply a stored logical location without creating another semantic jump.
    pub(crate) fn restore_jump(&mut self, location: &JumpLocation, cx: &mut Context<Self>) -> bool {
        if self.path.as_ref() != Some(&location.path)
            || self.document_generation != location.generation
            || location.page >= self.pages.len()
            || !location.x.is_finite()
            || !location.y.is_finite()
            || !location.scale.is_finite()
            || location.x < 0.0
            || location.y < 0.0
            || !(0.25..=4.0).contains(&location.scale)
            || location.x > self.pages[location.page].0
            || location.y > self.pages[location.page].1
        {
            return false;
        }
        self.position_touched = true;
        self.pending_restore = None;
        self.reading_mode = location.reading_mode;
        self.mode_override = Some(location.reading_mode);
        self.zoom_mode = location.zoom_mode;
        self.focus_page(location.page);
        self.set_scale(
            location.scale,
            Position {
                page: location.page,
                y: location.y,
            },
            0.0,
        );
        self.scroll_to(
            location.x * self.scale,
            self.layout().offset_for(
                Position {
                    page: location.page,
                    y: location.y.min(self.pages[location.page].1),
                },
                0.0,
            ),
        );
        self.sync_viewport(cx);
        self.retarget_ruler(
            location.page,
            ruler::Placement::Page(Point {
                x: location.x as f64,
                y: location.y as f64,
            }),
            cx,
        );
        true
    }
    pub fn page_count(&self) -> Option<usize> {
        (!self.pages.is_empty()).then_some(self.pages.len())
    }
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
    pub(crate) fn document_generation(&self) -> u64 {
        self.document_generation
    }
    pub(crate) fn document(&self) -> Option<Arc<pdf::Document>> {
        self.document.clone()
    }

    pub(crate) fn session(&self) -> Option<Entity<DocumentSession>> {
        self.session.clone()
    }

    pub(crate) fn detach_session(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.session.take() {
            let participant = cx.entity_id();
            session.update(cx, |session, _| session.detach(participant));
        }
        self.session_wait = None;
        self.session_observation = None;
    }

    fn sync_shared_session(&mut self, cx: &mut Context<Self>) {
        let epoch = self
            .session
            .as_ref()
            .map(|session| session.read(cx).epoch());
        if self.document.is_some() && epoch.is_some_and(|epoch| epoch != self.session_epoch) {
            self.reload_shared_session(cx);
            return;
        }
        let shared = self
            .session
            .as_ref()
            .and_then(|session| session.read(cx).presentation());
        if let (Some(current), Some(shared)) = (&self.document, shared)
            && !Arc::ptr_eq(current, &shared)
        {
            let path = self.session.as_ref().unwrap().read(cx).path().to_path_buf();
            if current.path() != path {
                self.path = Some(path.clone());
                self.record_path = Some(path);
            }
            self.document = Some(shared);
            self.links.reset();
            self.rasters.invalidate();
            self.schedule(cx);
            self.status = if self.is_dirty(cx) {
                "Newer changes remain unsaved".into()
            } else {
                format!("Page {} of {}", self.page_index + 1, self.pages.len())
            };
            cx.emit(DocumentEvent::Updated {
                generation: self.document_generation,
            });
        }
        if self.selected_annotation.is_some() && self.selected_page_annotation(cx).is_none() {
            self.selected_annotation = None;
            self.annotation_identification = None;
        }
        if let Some((id, _, _)) = self.annotation_identification
            && self.session.as_ref().is_none_or(|session| {
                session
                    .read(cx)
                    .annotations()
                    .and_then(Result::ok)
                    .is_none_or(|entries| !entries.iter().any(|a| a.id() == id))
            })
        {
            self.annotation_identification = None;
        }
        if let Some(error) = self
            .session
            .as_ref()
            .and_then(|s| s.read(cx).preview_error())
        {
            self.status = error.to_owned();
        }
        if let Some(error) = self
            .session
            .as_ref()
            .and_then(|s| s.read(cx).external_error())
        {
            self.status = format!("{error} The previous PDF remains available.");
        }
        cx.notify();
    }

    fn reload_shared_session(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(document) = session.read(cx).presentation() else {
            return;
        };
        let mut location = self.jump_location();
        // Jumplist/history locations favor explicit page turns. Reload instead
        // anchors the actual viewport, retaining that page-choice hint separately.
        let position = self.reading_position();
        let selected_page = self.selected_page;
        if let Some(location) = &mut location {
            location.page = position.0;
            location.y = position.1;
        }
        let ruler_enabled = self.ruler.enabled;
        let ruler_anchor = self.ruler.target.as_ref().and_then(|target| {
            let [left, bottom, right, top] = ruler::document_rect(&target.text, target.line)?;
            Some((
                target.page,
                Point {
                    x: (left + right) / 2.,
                    y: (bottom + top) / 2.,
                },
            ))
        });
        self.session_epoch = session.read(cx).epoch();
        self.document_generation += 1;
        self.document = Some(document.clone());
        self.pages = session.read(cx).reloaded_pages();
        self.selection.reset();
        self.page_text = page_text::PageTexts::default();
        self.ruler = ruler::Ruler::default();
        self.links.reset();
        self.rasters.invalidate();
        self.find_highlight = None;
        self.annotation_identification = None;
        self.selected_annotation = None;
        self.annotation_press = None;
        self.annotation_choice = None;
        self.marks = None;
        self.marks_subscriptions.clear();
        self.pending_restore = None;
        self.position_touched = true;
        self.position_ready = true;
        if let Some(mut location) = location {
            location.generation = self.document_generation;
            location.page = location.page.min(self.pages.len() - 1);
            location.x = location.x.min(self.pages[location.page].0);
            location.y = location.y.min(self.pages[location.page].1);
            self.restore_jump(&location, cx);
        }
        self.selected_page = selected_page.map(|page| page.min(self.pages.len() - 1));
        self.ruler.enabled = ruler_enabled;
        if let Some((page, at)) = ruler_anchor.filter(|(page, _)| *page < self.pages.len()) {
            self.ruler.pending = Some((page, ruler::Placement::Document(at)));
        } else if ruler_enabled {
            self.queue_ruler_placement(self.page_index, self.scroll.bounds().center(), false);
        }
        self.status = self.ready_status();
        cx.emit(DocumentEvent::Replaced {
            generation: self.document_generation,
        });
        self.sync_viewport(cx);
        self.schedule(cx);
        cx.notify();
    }

    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.read(cx).is_dirty())
    }

    pub(crate) fn has_annotation_undo(&self, cx: &App) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.read(cx).has_annotation_undo())
    }

    pub(crate) fn has_annotation_redo(&self, cx: &App) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.read(cx).has_annotation_redo())
    }

    pub(crate) fn traverse_annotation_history(&mut self, redo: bool, cx: &mut Context<Self>) {
        if self.marks_open(cx)
            || self.annotation_choice.is_some()
            || self.annotation_press.is_some()
        {
            return;
        }
        if let Some(session) = self.session.clone() {
            session.update(cx, |session, cx| {
                if redo {
                    session.redo_annotation(cx);
                } else {
                    session.undo_annotation(cx);
                }
            });
        }
        // Session observation removes only invalid annotation targets. Text
        // selection, reading position and focus are never part of edit history.
    }

    pub(crate) fn is_saving(&self, cx: &App) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.read(cx).is_saving())
    }

    pub(crate) fn save_available(&self, cx: &App) -> bool {
        self.is_dirty(cx) && self.save_as_available(cx)
    }

    pub(crate) fn save_as_available(&self, cx: &App) -> bool {
        self.session.as_ref().is_some_and(|session| {
            let session = session.read(cx);
            !session.is_saving()
                && session.editing_rejection().is_none()
                && session.annotations().is_some_and(|catalog| catalog.is_ok())
        })
    }

    pub(crate) fn annotation_editing_rejection(&self, cx: &App) -> Option<String> {
        self.session
            .as_ref()
            .and_then(|session| session.read(cx).editing_rejection().map(ToOwned::to_owned))
    }

    pub(crate) fn highlight_available(&self, cx: &App) -> bool {
        self.selected_text().is_some()
            && self.session.as_ref().is_some_and(|session| {
                let session = session.read(cx);
                session.annotations().is_some_and(|result| result.is_ok())
                    && session.editing_rejection().is_none()
            })
    }

    pub(crate) fn active_markup(&self) -> pdf::annotations::MarkupKind {
        self.active_markup
    }

    pub(crate) fn markup_color(&self, cx: &App) -> pdf::annotations::HighlightColor {
        self.session
            .as_ref()
            .map_or(pdf::annotations::HighlightColor::Yellow, |session| {
                session.read(cx).markup_color(self.active_markup)
            })
    }

    fn markup_selection(
        &mut self,
        kind: pdf::annotations::MarkupKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_markup = kind;
        cx.notify();
        let Some(page) = self.selection.page() else {
            return;
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        let quads = self
            .selection
            .document_quads()
            .into_iter()
            .map(pdf::annotations::Quad::new)
            .collect::<Option<Vec<_>>>();
        let Some(quads) = quads else {
            self.status = "This selection has no usable annotation geometry.".into();
            cx.notify();
            return;
        };
        let existing = session.read(cx).existing_markup(kind, page, &quads);
        if existing.len() > 1 {
            // Duplicate imported markup is still ambiguous. Do not let
            // idempotent creation silently arm an arbitrary deletion target.
            self.selected_annotation = None;
            self.annotation_identification = None;
            self.open_annotation_choice(existing, window, cx);
            return;
        }
        match session.update(cx, |session, cx| {
            session.create_markup(kind, page, quads, cx)
        }) {
            Ok(id) => {
                self.selected_annotation = Some(id);
                let catalog = session.read(cx).annotations().and_then(Result::ok);
                if let Some(annotation) = catalog
                    .as_ref()
                    .and_then(|catalog| catalog.iter().find(|a| a.id() == id))
                    && let Some(document) = &self.document
                {
                    self.annotation_identification = document
                        .annotation_bounds(annotation)
                        .map(|bounds| (id, page, bounds));
                }
                self.status = format!("{} selected", kind.label());
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    pub(crate) fn record_path(&self) -> Option<&Path> {
        self.record_path.as_deref()
    }
    pub fn displayed_page_index(&self) -> Option<usize> {
        self.rasters
            .image(self.page_index)
            .is_some()
            .then_some(self.page_index)
    }
    /// Diagnostic for asserting that sustained scrolling does not retain every page.
    pub fn retained_bitmap_bytes(&self) -> usize {
        self.rasters.retained_bytes()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection.selected_text()
    }

    fn layout(&self) -> PageLayout<'_> {
        PageLayout::new(&self.pages, self.scale, self.reading_mode, self.page_index)
    }

    fn scroll_to(&mut self, x: f32, top: f32) {
        let (x, top) = self.layout().clamp_scroll(x, top, self.viewport().2);
        self.scroll.set_offset(point(px(-x), px(-top)));
    }

    fn focus_page(&mut self, page: usize) {
        if self.reading_mode == ReadingMode::SinglePage && page != self.page_index {
            self.selection.clear();
        }
        self.page_index = page;
        self.selected_page = Some(page);
    }

    pub fn set_reading_mode(&mut self, mode: ReadingMode, cx: &mut Context<Self>) {
        if self.reading_mode == mode {
            return;
        }
        self.ruler.browse_task = None;
        self.position_touched = true;
        if self.pages.is_empty() {
            self.reading_mode = mode;
            cx.notify();
            return;
        }
        let (top, _, height) = self.viewport();
        let position = self.layout().position_at(top);
        let page = if mode == ReadingMode::SinglePage && self.ruler.enabled && self.ruler_visible()
        {
            self.ruler.target.as_ref().unwrap().page
        } else if mode == ReadingMode::SinglePage && self.selected_page.is_none() {
            self.layout().current_page_at(top, height)
        } else {
            self.page_index
        };
        self.reading_mode = mode;
        if mode == ReadingMode::SinglePage
            && self
                .selection
                .page()
                .is_some_and(|selected| selected != page)
        {
            self.selection.clear();
        }
        self.focus_page(page);
        let y = if position.page == page {
            position.y
        } else {
            0.0
        };
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.layout().offset_for(Position { page, y }, 0.0),
        );
        self.sync_viewport(cx);
    }

    /// A reader choice is an override even if it matches today's default.
    fn choose_reading_mode(&mut self, mode: Option<ReadingMode>, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let effective =
            mode.unwrap_or_else(|| match settings_ui::state(cx).read(cx).reading_view() {
                ReadingView::Continuous => ReadingMode::Continuous,
                ReadingView::Page => ReadingMode::SinglePage,
            });
        self.set_reading_mode(effective, cx);
        self.mode_override = mode;
        if let (Some(history), Some(path)) = (recent::history(cx), self.record_path.clone()) {
            let saved = mode.map(|mode| match mode {
                ReadingMode::Continuous => ReadingView::Continuous,
                ReadingMode::SinglePage => ReadingView::Page,
            });
            let generation = self.document_generation;
            let work = history.set_reading_mode(path, saved);
            cx.spawn(async move |this, cx| {
                if let Err(error) = work.await {
                    let _ = this.update(cx, |this, cx| {
                        if this.document_generation == generation {
                            this.status = format!("Reading mode could not be saved: {error}");
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
        }
        cx.notify();
    }

    /// Measured scroll bounds and centered page layout define the window origin.
    fn page_viewport(&self, index: usize) -> Option<PageViewport> {
        let transform = self
            .page_text
            .get(index)
            .map(|text| text.transform)
            .or_else(|| {
                self.ruler
                    .target
                    .as_ref()
                    .filter(|target| target.page == index)
                    .map(|target| target.text.transform)
            })?;
        let bounds = self.scroll.bounds();
        self.layout().page_viewport(
            index,
            transform,
            Point {
                x: f32::from(bounds.origin.x) as f64,
                y: f32::from(bounds.origin.y) as f64,
            },
            f32::from(bounds.size.width),
            Point {
                x: -f32::from(self.scroll.offset().x) as f64,
                y: -f32::from(self.scroll.offset().y) as f64,
            },
        )
    }

    fn pointer_document(&self, page: usize, position: gpui::Point<gpui::Pixels>) -> Option<Point> {
        let viewport = self.page_viewport(page)?;
        viewport.screen_to_document(Point {
            x: f32::from(position.x) as f64,
            y: f32::from(position.y) as f64,
        })
    }

    fn start_selection(
        &mut self,
        page: usize,
        position: gpui::Point<gpui::Pixels>,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        self.ruler.cancel_work();
        self.annotation_identification = None;
        self.annotation_press = Some((page, position, false));
        self.selection.begin(
            page,
            self.pointer_document(page, position),
            self.page_text.get(page).cloned(),
            click_count,
        );
        cx.notify();
    }

    fn extend_selection(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if let Some((_, start, dragged)) = &mut self.annotation_press {
            // Physical pointer slop, not product spacing. Once crossed, returning
            // to the origin must not turn a text drag back into an annotation click.
            *dragged |=
                (position.x - start.x).abs() > px(3.) || (position.y - start.y).abs() > px(3.);
        }
        if let Some(page) = self.selection.dragging_page()
            && let Some(at) = self.pointer_document(page, position)
            && self.selection.extend(at)
        {
            cx.notify();
        }
    }

    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn ruler_visible(&self) -> bool {
        let Some(target) = &self.ruler.target else {
            return false;
        };
        let (top, _, height) = self.viewport();
        if !self
            .layout()
            .visible_pages(top, height)
            .contains(&target.page)
        {
            return false;
        }
        let Some([left, y, right, bottom]) = target.page_rect(self.pages[target.page]) else {
            return false;
        };
        let Some(viewport) = self.page_viewport(target.page) else {
            return false;
        };
        let project = |x, y| {
            viewport.document_to_screen(
                target
                    .text
                    .transform
                    .page_to_document(Point { x, y })
                    .unwrap_or(Point { x, y }),
            )
        };
        let a = project(left, y);
        let b = project(right, bottom);
        let bounds = self.scroll.bounds();
        a.y >= f32::from(bounds.top()) as f64
            && b.y <= f32::from(bounds.bottom()) as f64
            && b.x > f32::from(bounds.left()) as f64
            && a.x < f32::from(bounds.right()) as f64
    }

    fn toggle_ruler(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        self.ruler.enabled = !self.ruler.enabled;
        self.ruler.cancel_work();
        if self.ruler.enabled && !self.ruler_visible() {
            self.ruler.target = None;
            let bounds = self.scroll.bounds();
            let center = bounds.center();
            let (top, _, height) = self.viewport();
            let page = self.layout().current_page_at(top, height);
            if let Some(text) = self.page_text.get(page).cloned() {
                if let Some(at) = self.pointer_document(page, center) {
                    self.ruler.place(page, text, at, false);
                }
            } else {
                self.queue_ruler_placement(page, center, false);
            }
            self.schedule_text(cx);
        }
        cx.notify();
    }

    fn finish_ruler_placement(&mut self) {
        let Some((page, placement)) = self.ruler.pending else {
            return;
        };
        // Reload also refreshes a remembered, hidden anchor. It must not turn
        // mode on, retain old text identities, or reveal/refocus the reader.
        if !self.ruler.enabled && !matches!(placement, ruler::Placement::Document(_)) {
            return;
        }
        if let Some(text) = self.page_text.get(page).cloned() {
            let exact = matches!(placement, ruler::Placement::ExactPage(_));
            let at = match placement {
                ruler::Placement::First => {
                    let line =
                        (0..text.lines.len()).find_map(|line| ruler::document_rect(&text, line));
                    line.map(|[left, bottom, right, top]| Point {
                        x: (left + right) / 2.,
                        y: (bottom + top) / 2.,
                    })
                }
                ruler::Placement::Page(at) | ruler::Placement::ExactPage(at) => {
                    text.transform.page_to_document(at)
                }
                ruler::Placement::Document(at) => Some(at),
            };
            self.ruler.pending = None;
            if let Some(at) = at {
                let placed = self.ruler.place(page, text, at, exact);
                if exact && placed {
                    self.selection.clear();
                    self.selected_annotation = None;
                    self.annotation_identification = None;
                }
            }
        } else if self.page_text.failed(page) {
            self.ruler.pending = None;
        }
    }

    fn place_ruler(
        &mut self,
        page: usize,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.ruler.cancel_work();
        self.annotation_press = None;
        self.selection.finish();
        if self.page_text.get(page).is_none() && !self.page_text.failed(page) {
            self.ruler.enabled = true;
            self.queue_ruler_placement(page, position, true);
            self.schedule_text(cx);
            cx.notify();
            return;
        }
        if let Some(at) = self.pointer_document(page, position)
            && let Some(text) = self.page_text.get(page).cloned()
            && self.ruler.place(page, text, at, true)
        {
            self.ruler.enabled = true;
            self.selection.clear();
            self.selected_annotation = None;
            self.annotation_identification = None;
        }
        cx.notify();
    }

    fn queue_ruler_placement(
        &mut self,
        page: usize,
        position: gpui::Point<gpui::Pixels>,
        exact: bool,
    ) {
        // Capture page-local points until extraction supplies the PDF transform.
        // No backend work runs in an input handler.
        let bounds = self.scroll.bounds();
        if let Some(viewport) = self.layout().page_viewport(
            page,
            crate::coordinates::PageTransform::from_coefficients([1., 0., 0., 1., 0., 0.]),
            Point {
                x: f32::from(bounds.left()) as f64,
                y: f32::from(bounds.top()) as f64,
            },
            f32::from(bounds.size.width),
            Point {
                x: -f32::from(self.scroll.offset().x) as f64,
                y: -f32::from(self.scroll.offset().y) as f64,
            },
        ) && let Some(at) = viewport.screen_to_document(Point {
            x: f32::from(position.x) as f64,
            y: f32::from(position.y) as f64,
        }) {
            self.ruler.pending = Some((
                page,
                if exact {
                    ruler::Placement::ExactPage(at)
                } else {
                    ruler::Placement::Page(at)
                },
            ));
        }
    }

    fn move_ruler(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.traverse_ruler(forward, cx);
    }

    fn reveal_ruler(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.ruler.target.as_ref().map(|target| target.page) else {
            return;
        };
        self.position_touched = true;
        if self.page_index != page {
            let position = if self.reading_mode == ReadingMode::SinglePage {
                self.selection.clear();
                Position { page, y: 0. }
            } else {
                self.layout().position_at(self.viewport().0)
            };
            self.page_index = page;
            self.selected_page = Some(page);
            let (_, width, height) = self.viewport();
            self.apply_fit(page, position, (width, height), 0.);
        } else {
            self.selected_page = Some(page);
        }
        if let Some(target) = &self.ruler.target
            && let Some([left, top, right, bottom]) = target.page_rect(self.pages[target.page])
            && let Some(viewport) = self.page_viewport(target.page)
        {
            let bounds = self.scroll.bounds();
            let project = |x, y| {
                viewport.document_to_screen(
                    target
                        .text
                        .transform
                        .page_to_document(Point { x, y })
                        .unwrap_or(Point { x, y }),
                )
            };
            let a = project(left, top);
            let b = project(right, bottom);
            let (scroll_top, _, height) = self.viewport();
            let margin = ((b.y - a.y) * 2.).min(height as f64 * 0.2);
            let dx = ruler::reveal_delta(
                a.x,
                b.x,
                f32::from(bounds.left()) as f64,
                f32::from(bounds.right()) as f64,
                margin,
            );
            let dy = ruler::reveal_delta(
                a.y,
                b.y,
                f32::from(bounds.top()) as f64,
                f32::from(bounds.bottom()) as f64,
                margin,
            );
            let new_x = -f32::from(self.scroll.offset().x) + dx as f32;
            let new_top = scroll_top + dy as f32;
            self.scroll_to(new_x, new_top);
            self.sync_viewport(cx);
        }
        cx.notify();
    }

    fn schedule_text(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let (top, _, height) = self.viewport();
        let visible = self.layout().visible_pages(top, height);
        let priority = self.ruler.text_priority();
        self.refresh_ruler_evidence();
        let evidence_page =
            priority.and_then(|page| self.ruler.evidence.missing(page, self.pages.len()));
        let Some(request) = self.page_text.request(
            visible,
            evidence_page.or(priority),
            [
                self.selection.page(),
                self.ruler.target.as_ref().map(|target| target.page),
                priority,
            ],
            self.document_generation,
        ) else {
            return;
        };
        let size = self.pages[request.page()];
        let work = cx.background_executor().spawn(async move {
            let text = document.page_text(request.page());
            let evidence = text.as_ref().map_or_else(
                |_| Vec::new(),
                |text| ruler::furniture::candidates(text, size),
            );
            (text, evidence)
        });
        cx.spawn(async move |this, cx| {
            let (result, evidence) = work.await;
            let _ = this.update(cx, |this, cx| {
                if !this.page_text.finish(request, result) {
                    return;
                }
                this.page_text
                    .set_evidence(request.page(), evidence.clone());
                let center = this
                    .ruler
                    .text_priority()
                    .or_else(|| this.ruler.target.as_ref().map(|target| target.page))
                    .unwrap_or(this.page_index);
                this.ruler.evidence.insert(
                    request.page(),
                    evidence,
                    ruler::furniture::window(center, this.pages.len()),
                );
                this.finish_ruler_placement();
                this.continue_ruler(cx);
                this.schedule_text(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn viewport(&self) -> (f32, f32, f32) {
        let top = (-f32::from(self.scroll.offset().y)).max(0.0);
        let width = f32::from(self.scroll.bounds().size.width);
        let measured = f32::from(self.scroll.bounds().size.height);
        let height = if measured > 0.0 { measured } else { 700.0 };
        (top, width, height)
    }

    pub(crate) fn sync_viewport(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        if self.position_touched {
            self.pending_restore = None;
        } else if self.scroll.bounds().size.height > px(0.0)
            && self.scroll.bounds().size.width > px(0.0)
            && let Some(position) = self.pending_restore.take()
        {
            self.restore_position(position);
        }
        let (top, width, height) = self.viewport();
        if (width, height) != (self.last_viewport.1, self.last_viewport.2) {
            self.ruler.browse_task = None;
        }
        if self.reading_mode == ReadingMode::SinglePage {
            self.scroll_to(-f32::from(self.scroll.offset().x), top);
        }
        let (top, _, _) = self.viewport();
        let refit = self.layout().refit(top, height, self.selected_page);
        self.selected_page = refit.selected_page;
        self.page_index = refit.page;
        self.apply_fit(refit.page, refit.position, (width, height), refit.anchor);
        self.record_position(cx);
        self.last_viewport = self.viewport();
        self.schedule(cx);
        self.schedule_text(cx);
        self.schedule_links(cx);
        cx.notify();
    }

    fn record_position(&mut self, cx: &mut Context<Self>) {
        if !self.position_ready || self.pending_restore.is_some() {
            return;
        }
        let position = self.position_to_save();
        if self.recorded_position == Some(position) {
            return;
        }
        let (Some(path), Some(history)) = (self.record_path.clone(), recent::history(cx)) else {
            return;
        };
        self.recorded_position = Some(position);
        let work = history.save_position(path, position);
        let document_generation = self.document_generation;
        cx.spawn(async move |this, cx| {
            if let Err(error) = work.await {
                let _ = this.update(cx, |this, cx| {
                    if document_generation == this.document_generation {
                        this.status = format!("Reading position could not be saved: {error}");
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn position_to_save(&self) -> ReadingPosition {
        let (top, _, height) = self.viewport();
        let position = self
            .layout()
            .position_to_save(top, height, self.selected_page);
        ReadingPosition {
            page: position.page,
            y: position.y,
        }
    }

    fn apply_fit(&mut self, page: usize, position: Position, viewport: (f32, f32), anchor: f32) {
        if let Some(scale) = self.layout().fit_scale(self.zoom_mode, page, viewport) {
            self.set_scale(scale, position, anchor);
        }
    }

    fn set_scale(&mut self, next: f32, position: Position, anchor: f32) {
        if next == self.scale {
            return;
        }
        self.ruler.browse_task = None;
        self.scale = next;
        self.rasters.invalidate();
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.layout().offset_for(position, anchor),
        );
    }

    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        self.select_page(self.page_index.saturating_add(1), cx);
    }
    pub fn previous_page(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.page_index.checked_sub(1) {
            self.select_page(index, cx);
        }
    }
    pub fn select_page(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.pages.len() {
            return;
        }
        self.position_touched = true;
        let y = if self.reading_mode == ReadingMode::SinglePage && !self.ruler.enabled {
            self.reading_position().1
        } else {
            0.0
        };
        self.focus_page(index);
        self.rasters.on_page_selected(index);
        let (_, width, height) = self.viewport();
        self.apply_fit(index, Position { page: index, y }, (width, height), 0.0);
        self.scroll_to(
            if self.ruler.enabled {
                0.
            } else {
                -f32::from(self.scroll.offset().x)
            },
            self.layout().offset_for(Position { page: index, y }, 0.0),
        );
        self.retarget_ruler(index, ruler::Placement::First, cx);
        self.record_position(cx);
        self.schedule(cx);
        self.schedule_text(cx);
        self.schedule_links(cx);
        cx.notify();
    }
    pub(crate) fn navigate_outline(
        &mut self,
        destination: pdf::OutlineDestination,
        cx: &mut Context<Self>,
    ) {
        let page = destination.page();
        if page >= self.pages.len() {
            return;
        }
        let before = self.jump_location();
        self.position_touched = true;
        self.focus_page(page);
        if let Some(zoom) = destination.zoom() {
            self.zoom_mode = ZoomMode::Fixed;
            self.set_scale(zoom, Position { page, y: 0.0 }, 0.0);
        } else {
            if let Some(fit) = destination.fit() {
                self.zoom_mode = match fit {
                    pdf::DestinationFit::Page => ZoomMode::FitPage,
                    pdf::DestinationFit::Width => ZoomMode::FitWidth,
                };
            }
            let (_, width, height) = self.viewport();
            self.apply_fit(page, Position { page, y: 0.0 }, (width, height), 0.0);
        }
        let (page_x, page_y) = destination.page_position().unwrap_or((0.0, 0.0));
        let y = page_y.clamp(0.0, self.pages[page].1);
        let (_, width, _) = self.viewport();
        let page_width = self.pages[page].0 * self.scale;
        let x = if page_width > width - PAGE_PADDING * 2.0 {
            page_x.clamp(0.0, self.pages[page].0) * self.scale
        } else {
            0.0
        };
        self.scroll_to(x, self.layout().offset_for(Position { page, y }, 0.0));
        self.retarget_ruler(
            page,
            ruler::Placement::Page(Point {
                x: page_x as f64,
                y: y as f64,
            }),
            cx,
        );
        self.record_position(cx);
        self.schedule(cx);
        self.schedule_text(cx);
        self.schedule_links(cx);
        self.emit_jump(before, cx);
        cx.notify();
    }
    fn activate_link(
        &mut self,
        generation: u64,
        page: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.document_generation {
            return false;
        }
        let Some(link) = self.links.destination(page, index) else {
            return false;
        };
        match link.target().cloned() {
            Some(pdf::LinkDestination::SameDocument(destination)) => {
                self.selection.clear();
                self.navigate_outline(destination, cx);
                true
            }
            Some(pdf::LinkDestination::AnotherDocument { path, page }) => {
                cx.emit(DocumentEvent::OpenLinkedDocument {
                    generation,
                    path,
                    page,
                });
                false // Focus stays in the source window until the new one is ready.
            }
            None => {
                self.report_link_error(
                    "This PDF link has an invalid or unsupported destination".into(),
                    cx,
                );
                false
            }
        }
    }
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.change_scale(1.25, cx);
    }
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.change_scale(0.8, cx);
    }
    pub fn fit_width(&mut self, cx: &mut Context<Self>) {
        self.set_zoom_mode(ZoomMode::FitWidth, cx);
    }
    pub fn fit_page(&mut self, cx: &mut Context<Self>) {
        self.set_zoom_mode(ZoomMode::FitPage, cx);
    }
    fn set_zoom_mode(&mut self, mode: ZoomMode, cx: &mut Context<Self>) {
        self.position_touched = true;
        self.zoom_mode = mode;
        if !self.pages.is_empty() {
            let (top, width, height) = self.viewport();
            self.page_index = if self.reading_mode == ReadingMode::SinglePage {
                self.page_index
            } else {
                self.selected_page
                    .unwrap_or_else(|| self.layout().current_page_at(top, height))
            };
            let position = self.layout().position_at(top);
            self.apply_fit(self.page_index, position, (width, height), 0.0);
            self.schedule(cx);
        }
        cx.notify();
    }
    fn change_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.position_touched = true;
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next == self.scale && self.zoom_mode == ZoomMode::Fixed {
            return;
        }
        let (top, _, _) = self.viewport();
        let position = self.layout().position_at(top);
        self.zoom_mode = ZoomMode::Fixed;
        self.set_scale(next, position, 0.0);
        self.schedule(cx);
        cx.notify();
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let (top, _, height) = self.viewport();
        let layout = PageLayout::new(&self.pages, self.scale, self.reading_mode, self.page_index);
        let request = match self
            .rasters
            .schedule(&layout, top, height, self.display_scale)
        {
            Schedule::Unchanged => return,
            Schedule::Ready => {
                self.status = self.ready_status();
                return;
            }
            Schedule::Render(request) => request,
        };
        self.status = format!(
            "Rendering page {} at {}%…",
            request.page() + 1,
            (self.scale * 100.0).round()
        );
        let work = cx
            .background_executor()
            .spawn(async move { document.render_bitmap(request.page(), request.scale()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let (top, _, height) = this.viewport();
                let visible = this.layout().visible_pages(top, height);
                match this.rasters.finish(request, result, visible) {
                    Completion::Obsolete => return,
                    Completion::Accepted => {}
                    Completion::Failed(error) => this.status = error,
                }
                this.schedule(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn schedule_links(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let (top, _, height) = self.viewport();
        let visible = self.layout().visible_pages(top, height);
        let Some(request) = self.links.request(visible, self.document_generation) else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { document.page_links(request.page()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if !this.links.finish(request, result) {
                    return;
                }
                this.schedule_links(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ReaderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_initialized {
            window.focus(&self.focus, cx);
            cx.observe_window_bounds(window, |_, window, cx| {
                // The scroll area's bounds are laid out in the resize frame.
                // Refit after that frame, using its measured content viewport.
                let reader = cx.weak_entity();
                window.on_next_frame(move |_, cx| {
                    let _ = reader.update(cx, |reader, cx| reader.sync_viewport(cx));
                });
            })
            .detach();
            self.focus_initialized = true;
        }
        let density = window.scale_factor();
        if self.display_scale != density {
            self.display_scale = density;
            self.rasters.invalidate();
            self.schedule(cx);
        }
        if !self.pages.is_empty() && self.viewport() != self.last_viewport {
            self.sync_viewport(cx);
        }
        let (top, _, height) = self.viewport();
        let layout = self.layout();
        let range = layout.visible_pages(top, height);
        let (top_space, end_space) = layout.spacers(range.clone());
        let widest_page = self.pages[range.clone()]
            .iter()
            .map(|(width, _)| width * self.scale)
            .fold(0.0_f32, f32::max);
        let mut content = div()
            .w_full()
            .min_w(px(widest_page))
            .flex()
            .flex_col()
            .child(div().h(px(top_space)).flex_shrink_0());
        let find_highlight = self.find_highlight.as_ref();
        for index in range {
            let (width, page_height) = self.pages[index];
            let page_width = px(width * self.scale);
            let page_height = px(page_height * self.scale);
            let image = self.rasters.image(index).cloned();
            let mut page = div()
                .id(format!("pdf-page-{index}"))
                .test_support()
                .relative()
                .w(page_width)
                .h(page_height)
                .flex_shrink_0()
                .bg(rgb(0xffffff))
                .when_some(image, |element, image| {
                    element.child(img(image).size_full())
                });
            if let Some((hit, pulse)) = find_highlight.as_ref().filter(|(hit, _)| hit.page == index)
            {
                let pulse = *pulse;
                let (left, top, width, height) =
                    find_search::highlight_rect(hit, self.pages[index]);
                let (x, y, w, h) = (
                    left * self.scale,
                    top * self.scale,
                    width * self.scale,
                    height * self.scale,
                );
                let color = cx.theme().primary;
                let scale = self.scale;
                page = page.child(
                    div()
                        .id("find-highlight")
                        .test_support()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(2. * scale))
                        .bg(color.opacity(0.27))
                        .shadow(vec![
                            BoxShadow::new(px(0.), px(0.), color.opacity(0.28))
                                .blur_radius(px(4. * scale))
                                .spread_radius(px(scale)),
                        ])
                        .with_animation(
                            ("find-jump", pulse),
                            Animation::new(Duration::from_millis(340))
                                .with_easing(gpui::ease_out_quint()),
                            move |highlight, phase| {
                                let jump = (1. - phase) * 5. * scale;
                                highlight
                                    .left(px(x - jump))
                                    .top(px(y - jump))
                                    .w(px(w + 2. * jump))
                                    .h(px(h + 2. * jump))
                            },
                        ),
                );
            }
            for highlight in self.selection.highlights(index, self.pages[index]) {
                page = page.child(
                    div()
                        .absolute()
                        .left(px((highlight.left * self.scale as f64) as f32))
                        .top(px((highlight.top * self.scale as f64) as f32))
                        .w(px((highlight.width * self.scale as f64) as f32))
                        .h(px((highlight.height * self.scale as f64) as f32))
                        .bg(gpui::rgba(0x278acb70)),
                );
            }
            if self.ruler.enabled
                && !self.ruler_editing()
                && (self.ruler.browse_task.is_none() || self.ruler_visible())
                && let Some(target) = self
                    .ruler
                    .target
                    .as_ref()
                    .filter(|target| target.page == index)
                && let Some([left, _, right, bottom]) = target.page_rect(self.pages[index])
            {
                let (wash, marker) = crate::app::theme::reading_ruler(cx.theme().mode.is_dark());
                // All overlay measurements are document geometry in PDF points.
                // Leave active text unobscured; quiet the unread portion below it.
                let marker_bottom = (bottom + 3.).min(self.pages[index].1 as f64);
                page = page
                    .child(
                        div()
                            .id("reading-ruler-wash")
                            .test_support()
                            .absolute()
                            .left(px(0.))
                            .top(px(marker_bottom as f32 * self.scale))
                            .w(page_width)
                            .h(px(
                                (self.pages[index].1 as f64 - marker_bottom) as f32 * self.scale
                            ))
                            .bg(wash),
                    )
                    .child(
                        div()
                            .id("reading-ruler-marker")
                            .test_support()
                            .absolute()
                            .left(px(left as f32 * self.scale))
                            .top(px(bottom as f32 * self.scale))
                            .w(px((right - left) as f32 * self.scale))
                            .h(px((marker_bottom - bottom) as f32 * self.scale))
                            .bg(marker),
                    );
            }
            if let Some((_, _, [left, top, right, bottom])) = self
                .annotation_identification
                .filter(|(_, page, _)| *page == index)
            {
                // PDF geometry in logical points; no extra markup fill is
                // painted over the annotation's embedded appearance.
                page = page.child(
                    div()
                        .id("annotation-identification")
                        .test_support()
                        .absolute()
                        .left(px(left as f32 * self.scale))
                        .top(px(top as f32 * self.scale))
                        .w(px((right - left) as f32 * self.scale))
                        .h(px((bottom - top) as f32 * self.scale))
                        .border_2()
                        .border_color(crate::app::theme::document_identification()),
                );
            }
            if let Some(links) = self.links.visible(index) {
                for (link_index, link) in links.iter().enumerate() {
                    let [left, top, right, bottom] = link.page_rect();
                    let id = gpui::ElementId::from(format!("pdf-link-{index}-{link_index}"));
                    let focus = window
                        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true));
                    let generation = self.document_generation;
                    let label = match link.target() {
                        Some(pdf::LinkDestination::SameDocument(destination)) => {
                            format!("Follow PDF link to page {}", destination.page() + 1)
                        }
                        Some(pdf::LinkDestination::AnotherDocument { path, .. }) => {
                            format!(
                                "Open {} in a new window",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            )
                        }
                        None => "Unsupported PDF link".to_owned(),
                    };
                    page = page.child(
                        div()
                            .id(id)
                            .test_support()
                            .role(gpui::Role::Button)
                            .aria_label(label)
                            .absolute()
                            .left(px(left * self.scale))
                            .top(px(top * self.scale))
                            .w(px((right - left) * self.scale))
                            .h(px((bottom - top) * self.scale))
                            .cursor_pointer()
                            .hover(|element| element.bg(cx.theme().primary.opacity(0.12)))
                            .focus_visible(|element| {
                                element.border_2().border_color(cx.theme().ring)
                            })
                            .track_focus(focus.read(cx))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        // Following a link must not begin a selection in its page.
                                        if event.modifiers.alt {
                                            this.place_ruler(index, event.position, cx);
                                            window.focus(&this.focus, cx);
                                        }
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                // Gesture ownership comes from mouse-down, even
                                // if Option is released before mouse-up.
                                if matches!(event, gpui::ClickEvent::Mouse(mouse) if mouse.down.modifiers.alt) {
                                    return;
                                }
                                if this.activate_link(generation, index, link_index, cx) {
                                    window.focus(&this.focus, cx);
                                }
                            })),
                    );
                }
            }
            page = page.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if event.modifiers.alt {
                        this.place_ruler(index, event.position, cx);
                    } else {
                        this.start_selection(index, event.position, event.click_count, cx);
                    }
                }),
            );
            content = content.child(
                div()
                    .w_full()
                    .min_w(page_width)
                    .flex()
                    .justify_center()
                    .child(page)
                    .mb(px(GAP)),
            );
        }
        content = content.child(div().h(px(end_space)).flex_shrink_0());

        div()
            .size_full()
            .tab_group()
            .track_focus(&self.focus)
            .when(
                self.selected_annotation.is_some()
                    && !self.marks_open(cx)
                    && self.annotation_choice.is_none(),
                |reader| reader.key_context(crate::key_context::SELECTED_ANNOTATION),
            )
            .when(
                !self.marks_open(cx)
                    && self.annotation_choice.is_none()
                    && self.focus.is_focused(window),
                |reader| {
                    reader.key_context(if self.selected_annotation.is_some() {
                        "Reader SelectedAnnotation"
                    } else {
                        crate::key_context::READER
                    })
                },
            )
            .on_action(cx.listener(|this, action: &SetLocalMark, _, cx| {
                if !this.pages.is_empty() {
                    this.change_mark(action.0, Some(this.position_to_save()), cx);
                }
            }))
            .on_action(
                cx.listener(|this, action: &JumpLocalMark, _, cx| this.jump_mark(action.0, cx)),
            )
            .on_action(cx.listener(|this, action: &DeleteLocalMark, _, cx| {
                this.change_mark(action.0, None, cx)
            }))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(|this, _: &ToggleReadingRuler, _, cx| this.toggle_ruler(cx)))
            .on_action(cx.listener(|this, _: &NextRulerLine, _, cx| this.move_ruler(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousRulerLine, _, cx| this.move_ruler(false, cx)))
            .on_action(cx.listener(|this, _: &ReadDown, _, cx| {
                if this.ruler.enabled {
                    this.move_ruler(true, cx);
                } else {
                    this.scroll_by(100., cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ReadUp, _, cx| {
                if this.ruler.enabled {
                    this.move_ruler(false, cx);
                } else {
                    this.scroll_by(-100., cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ExitReadingRuler, _, cx| {
                if this.selection.page().is_some() {
                    this.selection.clear();
                } else {
                    this.ruler.enabled = false;
                    this.ruler.cancel_work();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ApplyMarkup, window, cx| {
                this.markup_selection(this.active_markup, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Highlight, window, cx| {
                this.markup_selection(pdf::annotations::MarkupKind::Highlight, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Underline, window, cx| {
                this.markup_selection(pdf::annotations::MarkupKind::Underline, window, cx)
            }))
            .on_action(cx.listener(|this, _: &StrikeOut, window, cx| {
                this.markup_selection(pdf::annotations::MarkupKind::StrikeOut, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Squiggly, window, cx| {
                this.markup_selection(pdf::annotations::MarkupKind::Squiggly, window, cx)
            }))
            .on_action(cx.listener(|this, action: &RecolorHighlight, _, cx| {
                this.edit_selected_highlight(Some(action.0), cx);
            }))
            .on_action(cx.listener(|this, action: &SetMarkupColor, _, cx| {
                if let Some(session) = &this.session {
                    session.update(cx, |session, cx| {
                        session.set_markup_color(this.active_markup, action.0, cx);
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &DeleteHighlight, window, cx| {
                if this.annotation_edit_available(cx) {
                    // A focused strip control disappears with the selection.
                    window.focus(&this.focus, cx);
                }
                this.edit_selected_highlight(None, cx);
            }))
            .on_action(cx.listener(|this, _: &DeselectAnnotation, window, cx| {
                this.deselect_annotation(window, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_pointer_gesture(cx)),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_pointer_gesture(cx)),
            )
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                if !this.pages.is_empty() {
                    this.next_page(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                if !this.pages.is_empty() {
                    this.previous_page(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoToDocumentStart, _, cx| {
                this.select_page(0, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToDocumentEnd, _, cx| {
                if let Some(last) = this.pages.len().checked_sub(1) {
                    this.select_page(last, cx);
                    this.scroll_by(f32::MAX, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                if !this.pages.is_empty() {
                    this.zoom_in(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                if !this.pages.is_empty() {
                    this.zoom_out(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FitWidth, _, cx| {
                if !this.pages.is_empty() {
                    this.fit_width(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FitPage, _, cx| {
                if !this.pages.is_empty() {
                    this.fit_page(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SinglePage, _, cx| {
                if !this.pages.is_empty() {
                    this.choose_reading_mode(Some(ReadingMode::SinglePage), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Continuous, _, cx| {
                if !this.pages.is_empty() {
                    this.choose_reading_mode(Some(ReadingMode::Continuous), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &UseDefaultReadingMode, _, cx| {
                this.choose_reading_mode(None, cx);
            }))
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_by(100.0, cx)))
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_by(-100.0, cx)))
            .on_action(
                cx.listener(|this, _: &ScrollLeft, _, cx| this.scroll_horizontal_by(-100.0, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ScrollRight, _, cx| this.scroll_horizontal_by(100.0, cx)),
            )
            .on_action(cx.listener(|this, _: &ScrollHalfScreenDown, _, cx| {
                this.scroll_by(this.viewport().2 * 0.5, cx)
            }))
            .on_action(cx.listener(|this, _: &ScrollHalfScreenUp, _, cx| {
                this.scroll_by(-this.viewport().2 * 0.5, cx)
            }))
            .on_action(cx.listener(|this, _: &ScrollScreenDown, _, cx| {
                this.scroll_by(this.viewport().2 * 0.9, cx)
            }))
            .on_action(cx.listener(|this, _: &ScrollScreenUp, _, cx| {
                this.scroll_by(-this.viewport().2 * 0.9, cx)
            }))
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when(self.selected_page_annotation(cx).is_some(), |reader| {
                reader.child(self.render_annotation_controls(cx))
            })
            .child(
                div()
                    .id("page")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll)
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.extend_selection(event.position, cx);
                        }
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            window.focus(&this.focus, cx);
                        }),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                            this.extend_selection(event.position, cx);
                            this.selection.finish();
                            this.finish_annotation_click(event.position, window, cx);
                        }),
                    )
                    .on_scroll(cx.listener(|this, _, _, cx| {
                        if this.viewport().0 != this.last_viewport.0 {
                            this.position_touched = true;
                            this.selected_page = None;
                        }
                        this.sync_viewport(cx);
                        this.browse_ruler(cx);
                    }))
                    .bg(cx.theme().muted)
                    .p_4()
                    .child(if self.path.is_none() || self.pages.is_empty() {
                        let loading = self.status == "Loading document…";
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Empty::new()
                                    .flex_none()
                                    .max_w(gpui::rems(28.))
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(cx.theme().background)
                                    .header(
                                        EmptyHeader::new()
                                            .media(
                                                EmptyMedia::new()
                                                    .with_variant(EmptyMediaVariant::Icon)
                                                    .child(if loading {
                                                        Spinner::new().into_any_element()
                                                    } else {
                                                        gpui_kit::component::Icon::new(
                                                            IconName::FileText,
                                                        )
                                                        .into_any_element()
                                                    }),
                                            )
                                            .title(EmptyTitle::new().child(
                                                if self.path.is_none() {
                                                    "Open a PDF to start reading"
                                                } else if loading {
                                                    "Opening document…"
                                                } else {
                                                    "Couldn’t open this PDF"
                                                },
                                            ))
                                            .description(EmptyDescription::new().child(
                                                if self.path.is_none() {
                                                    "Choose a document to begin.".to_owned()
                                                } else {
                                                    self.status.clone()
                                                },
                                            )),
                                    )
                                    .content(
                                        EmptyContent::new().child(
                                            Button::new("open-pdf-center")
                                                .label(if self.path.is_none() {
                                                    "Open PDF…"
                                                } else {
                                                    "Choose another PDF…"
                                                })
                                                .on_click(|_, window, cx| {
                                                    window.dispatch_action(Box::new(OpenPdf), cx)
                                                }),
                                        ),
                                    ),
                            )
                            .into_any_element()
                    } else {
                        content.into_any_element()
                    }),
            )
            .context_menu({
                let enabled = self.highlight_available(cx);
                let has_selection = self.selected_text().is_some();
                let editable = self.annotation_edit_available(cx);
                move |menu, _, _| {
                    let mut menu = menu
                        .item(
                            PopupMenuItem::new("Highlight")
                                .action(Box::new(Highlight))
                                .disabled(!enabled),
                        )
                        .item(
                            PopupMenuItem::new("Underline")
                                .action(Box::new(Underline))
                                .disabled(!enabled),
                        )
                        .item(
                            PopupMenuItem::new("Strikethrough")
                                .action(Box::new(StrikeOut))
                                .disabled(!enabled),
                        )
                        .item(
                            PopupMenuItem::new("Squiggly underline")
                                .action(Box::new(Squiggly))
                                .disabled(!enabled),
                        )
                        .item(
                            PopupMenuItem::new("Copy")
                                .action(Box::new(Copy))
                                .disabled(!has_selection),
                        );
                    if editable {
                        menu = menu.separator();
                        for color in pdf::annotations::HighlightColor::ALL {
                            menu = menu.item(
                                PopupMenuItem::new(format!("Recolor {}", color.label()))
                                    .action(Box::new(RecolorHighlight(color))),
                            );
                        }
                        menu = menu.separator().item(
                            PopupMenuItem::new("Delete annotation")
                                .action(Box::new(DeleteHighlight)),
                        );
                    }
                    menu
                }
            })
            .child(self.render_annotation_choice(cx))
    }
}

impl ReaderView {
    fn scroll_horizontal_by(&mut self, amount: f32, cx: &mut Context<Self>) {
        let Some((page_width, _)) = self.pages.get(self.page_index).copied() else {
            return;
        };
        let (_, width, _) = self.viewport();
        let max = (page_width * self.scale + PAGE_PADDING * 2.0 - width).max(0.0);
        let old = -f32::from(self.scroll.offset().x);
        let next = (old + amount).clamp(0.0, max);
        if next == old {
            return;
        }
        self.position_touched = true;
        self.scroll_to(next, self.viewport().0);
        self.sync_viewport(cx);
        self.browse_ruler(cx);
    }

    pub fn scroll_by(&mut self, amount: f32, cx: &mut Context<Self>) {
        let top = (-f32::from(self.scroll.offset().y) + amount).max(0.0);
        if self.pages.is_empty() {
            return;
        }
        let old_top = self.viewport().0;
        self.scroll_to(-f32::from(self.scroll.offset().x), top);
        if self.viewport().0 != old_top {
            self.position_touched = true;
            self.selected_page = None;
        }
        self.sync_viewport(cx);
        self.browse_ruler(cx);
    }
}

#[cfg(test)]
mod tests;
