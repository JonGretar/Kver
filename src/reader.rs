//! Reader geometry drives layout; only the viewport owns bitmaps.

mod commands;
mod contents;
mod find;
mod marks;
mod navigation;
mod selection;
pub use crate::app::document_window::OpenPdf;
pub use crate::app::document_window::command_palette::ShowCommandPalette;
pub use crate::app::document_window::sidebar::{ShowContents, ShowFind, ToggleSidebar};
pub use crate::reading_history::ShowRecent;
pub use commands::*;
pub use contents::ActivateOutline;
pub(crate) use contents::ContentsPanel;
pub(crate) use find::FindPanel;
pub use find::{NextMatch, PreviousMatch};
// Preserve the former public name while callers adopt the narrowed view.
pub use ReaderView as Preview;
use marks::MarksEvent;
pub use marks::{
    DeleteLocalMark, JumpLocalMark, SetLocalMark, ShowDeleteMark, ShowJumpMark, ShowMarks,
    ShowSetMark,
};
pub(crate) use marks::{LocalMarks, MarksView};
use navigation::{GAP, PAGE_PADDING, PageLayout, Position};
pub use navigation::{ReadingMode, ZoomMode};
use selection::Selection;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::{
    Animation, AnimationExt, App, BoxShadow, ClipboardItem, Context, Entity, FocusHandle,
    RenderImage, ScrollHandle, Subscription, Window, div, img, point, prelude::*, px, rgb,
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
        spinner::Spinner,
    },
};
use image::{Frame, RgbaImage};

use crate::{
    coordinates::{PageViewport, Point},
    find_search, pdf,
    recent::{self, ReadingPosition, RecentStore},
    settings::{InitialZoom, ReadingView},
    settings_ui,
};

const BITMAP_BUDGET: usize = 128 * 1024 * 1024;

fn raster_scale(zoom: f32, density: f32, (width, height): (f32, f32)) -> f32 {
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    (zoom * density).clamp(0.25, 8.0).min(max_scale)
}

struct Bitmap {
    image: Arc<RenderImage>,
    bytes: usize,
}

/// Central PDF surface and document lifecycle, independent of window chrome.
pub struct ReaderView {
    path: Option<PathBuf>,
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
    images: HashMap<usize, Bitmap>,
    // Only one rasterization runs at once; the next job is chosen from the
    // *current* viewport, rather than queued ahead of a later jump.
    in_flight: Option<(usize, u64)>,
    generation: u64,
    document_generation: u64,
    selection: Selection,
    failed: Option<usize>,
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
    status: String,
}

/// Document lifecycle, independent of the UI that requested an open.
pub(crate) enum DocumentEvent {
    Replaced {
        generation: u64,
    },
    Opened {
        generation: u64,
        result: Result<(), String>,
    },
}

impl gpui::EventEmitter<DocumentEvent> for ReaderView {}

impl gpui::Focusable for ReaderView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ReaderView {
    pub fn new(path: Option<PathBuf>, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let mut reader = Self {
            path,
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
            images: HashMap::new(),
            in_flight: None,
            generation: 0,
            document_generation: 0,
            selection: Selection::default(),
            failed: None,
            page_index: 0,
            selected_page: None,
            last_viewport: (f32::NAN, f32::NAN, f32::NAN),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            focus_initialized: false,
            marks: None,
            marks_subscriptions: Vec::new(),
            find_highlight: None,
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
        self.generation += 1;
        self.document_generation += 1;
        self.marks = None;
        self.marks_subscriptions.clear();
        self.find_highlight = None;
        self.selection.reset();
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
        self.images.clear();
        self.in_flight = None;
        self.failed = None;
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
                            reader.position_touched = true;
                            reader.pending_restore = None;
                            reader.restore_position(*position);
                            reader.sync_viewport(cx);
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
        let generation = self.generation;
        let history = recent::history(cx);
        let history_epoch = history.as_ref().map(|history| history.epoch());
        let restore_position = settings.read(cx).reopen_at_last_position();
        let document_generation = self.document_generation;
        let work = cx.background_executor().spawn(async move {
            let pages = pdf::document_pages(&path);
            let normalized = if matches!(&pages, Ok(pages) if !pages.is_empty()) {
                Some(RecentStore::normalize(&path))
            } else {
                None
            };
            (pages, normalized)
        });
        cx.spawn(async move |this, cx| {
            let (result, normalized) = work.await;
            let saved_mode = if matches!(&result, Ok(pages) if !pages.is_empty()) {
                if let (Some(history), Some(Ok(path))) = (&history, &normalized) {
                    history.reading_mode(path.clone(), history_epoch.unwrap_or(0)).await
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            };
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                let mut opened = false;
                match result {
                    Ok(pages) if !pages.is_empty() => {
                        opened = true;
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
                        match normalized {
                            Some(Ok(path)) => {
                                this.record_path = Some(path.clone());
                                if let Some(history) = history {
                                    let record = history.record_at(path.clone(), history_epoch.unwrap_or(0));
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
                                                    if restore_position { saved } else { None }, cx
                                                ),
                                                Err(error) => {
                                                    this.status = format!("Reading history could not be loaded: {error}");
                                                    this.position_ready = true;
                                                    cx.notify();
                                                }
                                            }
                                        });
                                    }).detach();
                                } else {
                                    this.finish_position_load(None, cx);
                                }
                            }
                            Some(Err(error)) => {
                                this.status = format!("Reading history could not be saved: {error}");
                            }
                            None => {}
                        }
                        if this.record_path.is_none() {
                            this.finish_position_load(None, cx);
                        }
                    }
                    Ok(_) => this.status = "PDF has no pages".into(),
                    Err(error) => this.status = error,
                }
                cx.emit(DocumentEvent::Opened {
                    generation: document_generation,
                    result: if opened { Ok(()) } else { Err(this.status.clone()) },
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
        &self.status
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
    pub fn page_count(&self) -> Option<usize> {
        (!self.pages.is_empty()).then_some(self.pages.len())
    }
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
    pub(crate) fn document_generation(&self) -> u64 {
        self.document_generation
    }
    pub(crate) fn record_path(&self) -> Option<&Path> {
        self.record_path.as_deref()
    }
    pub fn displayed_page_index(&self) -> Option<usize> {
        self.images
            .contains_key(&self.page_index)
            .then_some(self.page_index)
    }
    /// Diagnostic for asserting that sustained scrolling does not retain every page.
    pub fn retained_bitmap_bytes(&self) -> usize {
        self.images.values().map(|image| image.bytes).sum()
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
        self.position_touched = true;
        if self.pages.is_empty() {
            self.reading_mode = mode;
            cx.notify();
            return;
        }
        let (top, _, height) = self.viewport();
        let position = self.layout().position_at(top);
        let page = if mode == ReadingMode::SinglePage && self.selected_page.is_none() {
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
            let generation = self.generation;
            let work = history.set_reading_mode(path, saved);
            cx.spawn(async move |this, cx| {
                if let Err(error) = work.await {
                    let _ = this.update(cx, |this, cx| {
                        if this.generation == generation {
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
        let transform = self.selection.page_transform(index)?;
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
        cx: &mut Context<Self>,
    ) {
        self.selection
            .begin(page, self.pointer_document(page, position));
        cx.notify();
    }

    fn extend_selection(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
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

    fn schedule_text(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let Some(path) = self.path.as_ref() else {
            return;
        };
        let (top, _, height) = self.viewport();
        let visible = self.layout().visible_pages(top, height);
        let Some(request) = self
            .selection
            .request_text(visible, self.document_generation)
        else {
            return;
        };
        let path = path.clone();
        let work = cx
            .background_executor()
            .spawn(async move { pdf::page_text(&path, request.page()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if !this.selection.finish_text(request, result) {
                    return;
                }
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
        self.scale = next;
        self.generation += 1;
        self.images.clear();
        self.in_flight = None;
        self.failed = None;
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
        let y = if self.reading_mode == ReadingMode::SinglePage {
            self.reading_position().1
        } else {
            0.0
        };
        self.focus_page(index);
        if self.failed != Some(index) {
            self.failed = None;
        }
        let (_, width, height) = self.viewport();
        self.apply_fit(index, Position { page: index, y }, (width, height), 0.0);
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.layout().offset_for(Position { page: index, y }, 0.0),
        );
        self.record_position(cx);
        self.schedule(cx);
        self.schedule_text(cx);
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
        self.position_touched = true;
        self.focus_page(page);
        if let Some(zoom) = destination.zoom() {
            self.zoom_mode = ZoomMode::Fixed;
            self.set_scale(zoom, Position { page, y: 0.0 }, 0.0);
        } else {
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
        self.record_position(cx);
        self.schedule(cx);
        self.schedule_text(cx);
        cx.notify();
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
        let (top, _, height) = self.viewport();
        let range = self.layout().visible_pages(top, height);
        if self.failed.is_some_and(|index| !range.contains(&index)) {
            self.failed = None;
        }
        // No offscreen cache: bitmap ownership and memory scale with the viewport.
        self.images.retain(|index, _| range.contains(index));
        if self.in_flight.is_some() {
            return;
        }
        let used = self.retained_bitmap_bytes();
        let index = range
            .filter(|index| !self.images.contains_key(index) && self.failed != Some(*index))
            .min_by_key(|index| (self.layout().page_top(*index) - top).abs() as i64);
        let Some(index) = index else {
            if self.failed.is_none() {
                self.status = self.ready_status();
            }
            return;
        };
        let dimensions = self.pages[index];
        let density = raster_scale(self.scale, self.display_scale, dimensions);
        let estimated =
            (dimensions.0 * density).ceil() as usize * (dimensions.1 * density).ceil() as usize * 4;
        if used > 0 && used.saturating_add(estimated) > BITMAP_BUDGET {
            self.status = self.ready_status();
            return;
        }
        let generation = self.generation;
        self.in_flight = Some((index, generation));
        self.status = format!(
            "Rendering page {} at {}%…",
            index + 1,
            (self.scale * 100.0).round()
        );
        let Some(path) = self.path.clone() else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_bitmap(&path, index, density) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.in_flight = None;
                let (top, _, height) = this.viewport();
                if this.layout().visible_pages(top, height).contains(&index) {
                    match result {
                        Ok(rendered) => {
                            let bytes = rendered.bgra.len();
                            if let Some(pixels) =
                                RgbaImage::from_raw(rendered.width, rendered.height, rendered.bgra)
                            {
                                this.images.insert(
                                    index,
                                    Bitmap {
                                        image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
                                        bytes,
                                    },
                                );
                            } else {
                                this.status =
                                    "Cannot display page image: invalid pixel dimensions".into();
                                this.failed = Some(index);
                            }
                        }
                        Err(error) => {
                            this.status = error;
                            this.failed = Some(index);
                        }
                    }
                }
                this.schedule(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
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
            self.generation += 1;
            self.images.clear();
            self.in_flight = None;
            self.schedule(cx);
        }
        if !self.pages.is_empty() && self.viewport() != self.last_viewport {
            self.sync_viewport(cx);
        }
        let (top, _, height) = self.viewport();
        let layout = self.layout();
        let range = layout.visible_pages(top, height);
        let (top_space, end_space) = layout.spacers(range.clone());
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .child(div().h(px(top_space)).flex_shrink_0());
        let find_highlight = self.find_highlight.as_ref();
        for index in range {
            let (width, page_height) = self.pages[index];
            let page_width = px(width * self.scale);
            let page_height = px(page_height * self.scale);
            let image = self.images.get(&index).map(|bitmap| bitmap.image.clone());
            let mut page = div()
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
            page = page.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.start_selection(index, event.position, cx);
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
            .track_focus(&self.focus)
            .when(
                !self.marks_open(cx) && self.focus.is_focused(window),
                |reader| reader.key_context("Reader"),
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
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.selection.finish()),
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
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.marks_open(cx) || !this.focus.is_focused(window) {
                    cx.propagate();
                    return;
                }
                if event.keystroke.modifiers == gpui::Modifiers::default() {
                    match event.keystroke.key.as_str() {
                        "down" | "j" => this.scroll_by(100.0, cx),
                        "up" | "k" => this.scroll_by(-100.0, cx),
                        _ => cx.propagate(),
                    }
                } else {
                    cx.propagate();
                }
            }))
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
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
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            this.extend_selection(event.position, cx);
                            this.selection.finish();
                        }),
                    )
                    .on_scroll(cx.listener(|this, _, _, cx| {
                        if this.viewport().0 != this.last_viewport.0 {
                            this.position_touched = true;
                            this.selected_page = None;
                        }
                        this.sync_viewport(cx);
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
    }
}

impl ReaderView {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keymap::install as bind_reader_keys;
    use crate::reading_history::LocalMark;
    use crate::settings_ui::ShowSettings;
    use gpui_kit::{AnyWindowHandle, TestAppContext, point, size, test::TestWindowExt};
    use std::collections::BTreeMap;

    mod find;
    mod marks;
    mod navigation;
    mod selection;

    fn recent_test_window(
        cx: &mut TestAppContext,
        path: Option<PathBuf>,
    ) -> (AnyWindowHandle, Entity<Preview>) {
        cx.update(|cx| {
            let (handle, document) =
                gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| crate::app::document_window::DocumentWindow::new(path, window, cx))
                })
                .unwrap();
            (handle, document.read(cx).reader().clone())
        })
    }

    fn mark(position: ReadingPosition) -> LocalMark {
        LocalMark {
            position,
            label: None,
        }
    }

    fn three_page_pdf(sizes: [(u32, u32); 3]) -> Vec<u8> {
        three_page_pdf_with_boxes(sizes, ["", "", ""])
    }

    #[gpui_kit::test]
    fn local_marks_share_across_windows_restart_and_palette(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("marked.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.update(cx, |reader, cx| reader.scroll_by(1550.0, cx));
        let saved = first.read_with(cx, |reader, _| reader.position_to_save());
        cx.simulate_keystrokes(window, "m a");
        cx.run_until_parked();
        assert_eq!(
            RecentStore::new(&paths).marks(&pdf).unwrap().get(&'a'),
            Some(&mark(saved))
        );

        let (other_window, other) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        other.update(cx, |reader, cx| {
            reader.select_page(2, cx);
            reader.choose_reading_mode(Some(ReadingMode::SinglePage), cx);
            reader.fit_width(cx);
        });
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(JumpLocalMark('a')), cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        other.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), saved.page);
            assert!((reader.reading_position().1 - saved.y).abs() < 2.0);
        });
        first.read_with(cx, |reader, _| assert_eq!(reader.position_to_save(), saved));

        cx.simulate_keystrokes(other_window, "cmd-shift-m");
        cx.run_until_parked();
        other.read_with(cx, |reader, cx| {
            let marks = reader.marks.as_ref().expect("marks owner").read(cx);
            assert!(marks.is_open());
            assert_eq!(marks.listed_marks().get(&'a'), Some(&mark(saved)));
            assert_eq!(marks.query(cx), "");
        });
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.find("reader-marks-palette");
                frame.input("Page 2", cx);
            })
            .unwrap()
        });
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| frame.press("enter", cx))
                .unwrap()
        });
        cx.run_until_parked();
        other.read_with(cx, |reader, cx| assert!(!reader.marks_open(cx)));
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowCommandPalette), cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.input("Open marks", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            assert_eq!(frame.find("input-state").value(), Some("Open marks"));
        })
        .unwrap();
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| frame.press("enter", cx))
                .unwrap()
        });
        cx.run_until_parked();
        other.read_with(cx, |reader, cx| assert!(reader.marks_open(cx)));
        cx.simulate_keystrokes(other_window, "escape");
        other.update(cx, |reader, cx| reader.select_page(2, cx));
        cx.simulate_keystrokes(other_window, "' a");
        cx.run_until_parked();
        other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), saved.page));

        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reopened.update(cx, |reader, cx| reader.select_page(0, cx));
        reopened.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), saved.page));
        cx.simulate_keystrokes(other_window, "d m a");
        cx.run_until_parked();
        assert!(RecentStore::new(&paths).marks(&pdf).unwrap().is_empty());
    }

    #[gpui_kit::test]
    fn private_marks_replace_delete_flush_and_clear(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("private.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        let old = ReadingPosition { page: 0, y: 40.0 };
        store
            .replace_marks(&pdf, &BTreeMap::from([('a', mark(old))]))
            .unwrap();
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        first.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position().1, old.y)
        });
        first.update(cx, |reader, cx| reader.select_page(1, cx));
        first.update(cx, |reader, cx| {
            reader.change_mark('a', Some(reader.position_to_save()), cx)
        });
        first.update(cx, |reader, cx| {
            reader.change_mark('b', Some(reader.position_to_save()), cx)
        });
        cx.run_until_parked();
        assert_eq!(
            store.marks(&pdf).unwrap(),
            BTreeMap::from([('a', mark(old))])
        );
        let (_, second) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        second.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        second.update(cx, |reader, cx| reader.change_mark('a', None, cx));
        cx.run_until_parked();
        let history = cx.update(|cx| recent::history(cx).unwrap());
        let name = history.name_mark(pdf.clone(), 'b', Some("Chapter two".into()));
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                name.await.unwrap();
                cx.update(|_| ());
            })
            .detach()
        });
        cx.run_until_parked();
        assert!(!store.marks(&pdf).unwrap().contains_key(&'b'));
        let flush = history.set_recording(true);
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                flush.await.unwrap();
                cx.update(|_| ());
            })
            .detach()
        });
        cx.run_until_parked();
        assert_eq!(
            store
                .marks(&pdf)
                .unwrap()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec!['b']
        );
        assert_eq!(
            store.marks(&pdf).unwrap()[&'b'].label.as_deref(),
            Some("Chapter two")
        );
        let clear = history.clear();
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                clear.await.unwrap();
                cx.update(|_| ());
            })
            .detach()
        });
        cx.run_until_parked();
        assert!(store.marks(&pdf).unwrap().is_empty());
        second.update(cx, |reader, cx| reader.jump_mark('b', cx));
        cx.run_until_parked();
        second.read_with(cx, |reader, _| assert!(reader.status.contains("not set")));
    }

    #[gpui_kit::test]
    fn invalid_marks_and_text_focus_leave_reader_in_place(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("changed.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        store
            .replace_marks(
                &pdf,
                &BTreeMap::from([
                    ('a', mark(ReadingPosition { page: 99, y: 20.0 })),
                    ('b', mark(ReadingPosition { page: 0, y: 9000.0 })),
                ]),
            )
            .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reader.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 0);
            assert!(reader.status.contains("outside this PDF"));
        });
        reader.update(cx, |reader, cx| reader.jump_mark('b', cx));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert!(reader.reading_position().1 <= 1400.0)
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowFind), cx);
                frame.render_frame(cx);
                frame.input("m a", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        assert!(!store.marks(&pdf).unwrap().contains_key(&'m'));
        std::fs::remove_file(&pdf).unwrap();
        assert_eq!(store.marks(&pdf).unwrap().len(), 2);
    }

    #[gpui_kit::test]
    fn recording_off_discards_new_marks_on_restart_but_reads_saved_ones(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("book.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        store
            .replace_marks(
                &pdf,
                &BTreeMap::from([('a', mark(ReadingPosition { page: 1, y: 80.0 }))]),
            )
            .unwrap();
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.update(cx, |reader, cx| {
            reader.change_mark('a', None, cx);
            reader.change_mark('b', Some(ReadingPosition { page: 2, y: 200.0 }), cx);
        });
        cx.run_until_parked();
        let (_, same_session) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        same_session.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        same_session.read_with(cx, |reader, _| assert!(reader.status.contains("not set")));
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, restarted) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        restarted.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        restarted.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        assert_eq!(
            store
                .marks(&pdf)
                .unwrap()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec!['a']
        );
    }

    fn three_page_pdf_with_boxes(sizes: [(u32, u32); 3], boxes: [&str; 3]) -> Vec<u8> {
        pdf_with_boxes(&sizes, &boxes)
    }

    fn pdf_with_boxes(sizes: &[(u32, u32)], boxes: &[&str]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                (3..3 + sizes.len())
                    .map(|id| format!("{id} 0 R"))
                    .collect::<Vec<_>>()
                    .join(" "),
                sizes.len()
            ),
        ];
        for (size, box_options) in sizes.iter().zip(boxes) {
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] {} >>",
                size.0, size.1, box_options
            ));
        }
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    fn image_only_pdf() -> Vec<u8> {
        let width = 595;
        let height = 842;
        let pixels: Vec<u8> = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| if (x / 13 + y / 13) % 2 == 0 { 224 } else { 64 })
            })
            .collect();
        let content = b"q\n595 0 0 842 0 0 cm\n/Im1 Do\nQ\n";
        let mut image = format!(
            "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
             /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n",
            pixels.len()
        )
        .into_bytes();
        image.extend_from_slice(&pixels);
        image.extend_from_slice(b"\nendstream");
        let mut page_content = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        page_content.extend_from_slice(content);
        page_content.extend_from_slice(b"endstream");
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /XObject << /Im1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
            image,
            page_content,
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    #[gpui_kit::test]
    fn mode_override_survives_reopen_but_use_default_follows_new_preference(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let file = root.path().join("pages.pdf");
        std::fs::write(&file, three_page_pdf([(400, 1200); 3])).unwrap();
        let pdf = RecentStore::normalize(&file).unwrap();
        let store = RecentStore::new(&paths);
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (first_window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| first.read(cx).reading_mode()),
            ReadingMode::Continuous
        );
        assert_eq!(cx.update(|cx| first.read(cx).mode_override()), None);
        let (_, already_open) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.update(cx, |reader, cx| reader.scroll_by(80.0, cx));
        let position = cx.update(|cx| first.read(cx).reading_position());
        cx.update(|cx| {
            cx.update_window(first_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(store.reading_mode(&pdf).unwrap(), Some(ReadingView::Page));
        let after = cx.update(|cx| first.read(cx).reading_position());
        assert_eq!(after.0, position.0);
        assert!(
            (after.1 - position.1).abs() < 2.0,
            "before {position:?}, after {after:?}"
        );
        assert_eq!(
            cx.update(|cx| already_open.read(cx).reading_mode()),
            ReadingMode::Continuous
        );

        // Another window already reading this PDF keeps its live mode, while
        // every fresh open path converges on the saved override.
        let (_, second) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| second.read(cx).reading_mode()),
            ReadingMode::SinglePage
        );
        let (empty_window, empty) = recent_test_window(cx, None);
        cx.update(|cx| {
            cx.update_window(empty_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update_window(empty_window, |_, frame, cx| frame.press("enter", cx))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| empty.read(cx).mode_override()),
            Some(ReadingMode::SinglePage)
        );

        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::ReadingView(ReadingView::Page),
        )
        .unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(first_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(UseDefaultReadingMode), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| first.read(cx).mode_override()), None);
        assert_eq!(store.reading_mode(&pdf).unwrap(), None);
        // Clearing the override does not copy Page: the next preference edit
        // affects the next open, not an existing reader.
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::ReadingView(ReadingView::Continuous),
        )
        .unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| first.read(cx).reading_mode()),
            ReadingMode::SinglePage
        );
        assert_eq!(
            cx.update(|cx| second.read(cx).reading_mode()),
            ReadingMode::SinglePage
        );
        empty.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| empty.read(cx).reading_mode()),
            ReadingMode::Continuous
        );
        assert_eq!(cx.update(|cx| empty.read(cx).mode_override()), None);

        // Even a choice equal to the default becomes a real PDF override.
        cx.update(|cx| {
            cx.update_window(first_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(Continuous), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(
            store.reading_mode(&pdf).unwrap(),
            Some(ReadingView::Continuous)
        );
        empty.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| empty.read(cx).mode_override()),
            Some(ReadingMode::Continuous)
        );
    }

    #[gpui_kit::test]
    fn private_mode_is_session_only_then_flushes_on_reenable(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        let store = RecentStore::new(&paths);
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(!paths.local_data().join("reading.sqlite3").exists());
        reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| reader.read(cx).mode_override()),
            Some(ReadingMode::SinglePage)
        );
        // Replace the worker as if the process restarted: no private state
        // crossed the disk boundary.
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| reader.read(cx).mode_override()), None);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| cx.dispatch_action(&ShowSettings));
        cx.run_until_parked();
        let settings_window = cx.update(|cx| *cx.windows().last().unwrap());
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("reading-history", cx);
                frame.click("remember-reading-history", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(store.reading_mode(&pdf).unwrap(), Some(ReadingView::Page));
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| reader.read(cx).mode_override()),
            Some(ReadingMode::SinglePage)
        );
        cx.update(|cx| {
            let history = recent::history(cx).unwrap();
            cx.spawn(async move |cx| {
                let _ = history.clear().await;
                cx.update(recent::notify_changed);
            })
            .detach();
        });
        cx.run_until_parked();
        assert_eq!(store.reading_mode(&pdf).unwrap(), None);
        reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| reader.read(cx).mode_override()), None);
    }

    #[gpui_kit::test]
    fn wheel_scrolled_position_is_saved_for_reopen(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("wheel.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-1800.0))),
                    cx,
                );
            })
            .unwrap();
        });
        cx.run_until_parked();
        let position = first.read_with(cx, |reader, _| reader.reading_position());
        assert!(position.0 > 0 || position.1 > 0.0, "wheel did not scroll");
        let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), position)
        });
    }

    #[gpui_kit::test]
    fn saved_single_page_mode_restores_its_page_local_position(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("single-page.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
                frame.dispatch_action(Box::new(NextPage), cx);
            })
            .unwrap();
        });
        first.update(cx, |reader, cx| reader.scroll_by(180.0, cx));
        cx.run_until_parked();
        let position = first.read_with(cx, |reader, _| reader.reading_position());
        assert_eq!(position.0, 1);
        assert!(position.1 > 0.0);
        let (reopened_window, reopened) = recent_test_window(cx, Some(pdf));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(reopened_window, |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.reading_position().0, position.0);
            assert!((reader.reading_position().1 - position.1).abs() < 2.0);
        });
    }

    #[gpui_kit::test]
    fn private_single_page_place_and_mark_follow_the_pdf_across_windows_and_restart(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let file = root.path().join("rotated.pdf");
        std::fs::write(
            &file,
            three_page_pdf_with_boxes(
                [(600, 1400), (800, 1500), (700, 1300)],
                ["", "/Rotate 90 /CropBox [40 30 700 1350]", ""],
            ),
        )
        .unwrap();
        let pdf = RecentStore::normalize(&file).unwrap();
        let store = RecentStore::new(&paths);
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
                frame.dispatch_action(Box::new(NextPage), cx);
                frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(store.marks(&pdf).unwrap()[&'a'].position.page, 1);

        // Disabling history keeps the reader usable without writing new state.
        cx.update(|cx| cx.dispatch_action(&ShowSettings));
        cx.run_until_parked();
        let settings_window = cx.update(|cx| *cx.windows().last().unwrap());
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("reading-history", cx);
                frame.click("remember-reading-history", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(NextPage), cx);
                frame.dispatch_action(Box::new(SetLocalMark('b')), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| first.read(cx).page_index()), 2);
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
        assert!(!store.marks(&pdf).unwrap().contains_key(&'b'));

        let (other_window, other) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        other.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.page_index(), 2);
        });
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(JumpLocalMark('b')), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));

        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("remember-reading-history", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, 2);
        assert_eq!(store.marks(&pdf).unwrap()[&'b'].position.page, 2);

        // Reinstalling the services represents a fresh app session reading disk.
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.page_index(), 2);
        });
        reopened.update(cx, |reader, cx| {
            reader.select_page(0, cx);
            reader.jump_mark('b', cx);
        });
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
    }

    #[gpui_kit::test]
    fn long_pdf_keeps_single_page_work_bounded_and_restores_its_last_page(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let file = root.path().join("long.pdf");
        std::fs::write(&file, pdf_with_boxes(&[(612, 792); 500], &[""; 500])).unwrap();
        let pdf = RecentStore::normalize(&file).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(500)));
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
            })
            .unwrap();
        });
        reader.update(cx, |reader, cx| reader.select_page(499, cx));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 499);
            assert!(
                reader.images.len() <= 2,
                "Single Page eagerly rendered {} pages",
                reader.images.len()
            );
        });

        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reopened) = recent_test_window(cx, Some(pdf));
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.page_index(), 499);
            assert!(reader.images.len() <= 2);
        });
    }

    #[gpui_kit::test]
    fn image_only_page_can_scroll_and_keep_a_local_mark_without_text(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let file = root.path().join("image-only.pdf");
        std::fs::write(&file, image_only_pdf()).unwrap();
        let pdf = RecentStore::normalize(&file).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        assert!(pdf::page_text(&pdf, 0).unwrap().glyphs.is_empty());
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SinglePage), cx);
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-1200.0))),
                    cx,
                );
                frame.dispatch_action(Box::new(SetLocalMark('s')), cx);
                frame.dispatch_action(Box::new(NextPage), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), Some(1));
            assert_eq!(reader.page_index(), 0);
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert!(reader.images.len() <= 1);
        });
        let (_, other) = recent_test_window(cx, Some(pdf));
        cx.run_until_parked();
        other.update(cx, |reader, cx| reader.jump_mark('s', cx));
        cx.run_until_parked();
        other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    }

    #[gpui_kit::test]
    fn page_turn_to_short_page_reopens_at_selected_page(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("slides.pdf");
        std::fs::write(&pdf, pdf_with_boxes(&[(600, 120); 2], &["", ""])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(NextPage), cx);
                frame.render_frame(cx);
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                    cx,
                );
            })
            .unwrap();
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert!(reader.position_ready);
        });
        assert_eq!(
            RecentStore::new(&paths)
                .position(&pdf)
                .unwrap()
                .map(|position| position.page),
            Some(1),
            "the selected page was never saved"
        );
        let (reopened_window, reopened) = recent_test_window(cx, Some(pdf));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(reopened_window, |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    }

    #[gpui_kit::test]
    fn reading_position_survives_restart_zoom_and_independent_windows(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("rotated.pdf");
        std::fs::write(
            &pdf,
            three_page_pdf_with_boxes(
                [(600, 1400), (800, 1500), (700, 1300)],
                ["", "/Rotate 90 /CropBox [40 30 700 1350]", ""],
            ),
        )
        .unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_first_window, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.read_with(cx, |reader, _| {
            assert!(reader.position_ready, "{}", reader.status())
        });
        first.update(cx, |reader, cx| reader.scroll_by(1550.0, cx));
        cx.run_until_parked();
        let saved = first.read_with(cx, |reader, _| reader.reading_position());
        assert_eq!(saved.0, 1);
        assert!(saved.1 > 0.0);
        first.read_with(cx, |reader, _| {
            assert_eq!(
                reader.recorded_position,
                Some(ReadingPosition {
                    page: saved.0,
                    y: saved.1
                }),
                "{}",
                reader.status()
            );
        });
        let store = RecentStore::new(&paths);
        assert!(store.contains(&pdf).unwrap());
        assert_eq!(
            store.position(&pdf).unwrap(),
            Some(ReadingPosition {
                page: saved.0,
                y: saved.1
            })
        );

        // A second live window restores the place, then moves independently.
        let (_second_window, second) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        second.read_with(cx, |reader, _| assert_eq!(reader.reading_position(), saved));
        second.update(cx, |reader, cx| reader.next_page(cx));
        cx.run_until_parked();
        first.read_with(cx, |reader, _| assert_eq!(reader.reading_position(), saved));
        let later = second.read_with(cx, |reader, _| reader.reading_position());
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, later.0);

        // Reinstalling both services against the same paths models a new process.
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::InitialZoom(InitialZoom::FitWidth),
        )
        .unwrap();
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_third_window, third) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        third.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position().0, later.0);
            assert!((reader.reading_position().1 - later.1).abs() < 2.0);
        });
    }

    #[gpui_kit::test]
    fn restoration_off_preserves_saved_place_until_a_new_position_is_chosen(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("document.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        store
            .save_position(&pdf, ReadingPosition { page: 1, y: 200.0 })
            .unwrap();
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::ReopenAtLastPosition(false),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.read_with(cx, |reader, _| {
            assert!(reader.position_ready, "{}", reader.status())
        });
        first.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), (0, 0.0))
        });
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::ReopenAtLastPosition(true),
        )
        .unwrap();
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, restored) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        restored.read_with(cx, |reader, _| {
            assert!(reader.position_ready, "{}", reader.status());
        });
        restored.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), (1, 200.0))
        });
    }

    #[gpui_kit::test]
    fn private_positions_share_with_windows_and_flush_only_when_reenabled(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("private.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, first) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        first.update(cx, |reader, cx| reader.select_page(1, cx));
        cx.run_until_parked();
        let store = RecentStore::new(&paths);
        assert!(!paths.local_data().join("reading.sqlite3").exists());
        let (_, second) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        second.read_with(cx, |reader, _| assert_eq!(reader.reading_position().0, 1));
        // Reinstalling the service models a restart; the private place disappears.
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, after_restart) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        after_restart.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), (0, 0.0))
        });
        after_restart.update(cx, |reader, cx| reader.select_page(2, cx));
        cx.run_until_parked();
        let history = cx.update(|cx| recent::history(cx).unwrap());
        let work = history.set_recording(true);
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                work.await.unwrap();
                cx.update(|_| ());
            })
            .detach()
        });
        cx.run_until_parked();
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, 2);
    }

    #[gpui_kit::test]
    fn forgetting_pdf_clears_session_position_before_reopen(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("forgotten.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reader.update(cx, |reader, cx| reader.select_page(1, cx));
        cx.run_until_parked();
        let store = RecentStore::new(&paths);
        assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
        let history = cx.update(|cx| recent::history(cx).unwrap());
        let removal = history.remove(pdf.clone());
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                removal.await.unwrap();
                cx.update(|_| ());
            })
            .detach()
        });
        cx.run_until_parked();
        assert_eq!(store.position(&pdf).unwrap(), None);
        reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), (0, 0.0))
        });
    }

    #[gpui_kit::test]
    fn changed_pdf_bounds_saved_position_and_stale_load_cannot_replace_new_document(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("changed.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        store
            .save_position(&pdf, ReadingPosition { page: 2, y: 9000.0 })
            .unwrap();
        // Rebuild at the same path with different page geometry: clamp the saved offset.
        std::fs::write(
            &pdf,
            three_page_pdf([(600, 1400), (600, 1400), (600, 2000)]),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position().0, 2);
            assert!(reader.reading_position().1 <= 2000.0);
        });
        store
            .save_position(&pdf, ReadingPosition { page: 80, y: 100.0 })
            .unwrap();
        // A rebuilt file with fewer pages rejects the old index.
        std::fs::write(&pdf, pdf_with_boxes(&[(600, 1400)], &[""])).unwrap();
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        let (_, next) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        next.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_position(), (0, 0.0))
        });
        next.update(cx, |reader, cx| {
            reader.open_path(pdf.clone(), cx);
            reader.open_path(root.path().join("missing.pdf"), cx);
        });
        cx.run_until_parked();
        next.read_with(cx, |reader, _| {
            assert_eq!(
                reader.path(),
                Some(root.path().join("missing.pdf").as_path())
            );
            assert_eq!(reader.page_count(), None);
        });
    }

    #[gpui_kit::test]
    fn single_page_bounds_wheel_and_explicit_turns_on_real_page_geometry(cx: &mut TestAppContext) {
        let path = PathBuf::from("target/test-fixtures/single-page-geometry.pdf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            three_page_pdf_with_boxes(
                [(360, 300), (600, 1600), (600, 1000)],
                [
                    "",
                    "/CropBox [20 40 580 1540] /Rotate 180",
                    "/CropBox [50 30 550 970] /Rotate 90",
                ],
            ),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let (window, reader) = recent_test_window(cx, Some(path));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.pages,
                vec![(360.0, 300.0), (560.0, 1500.0), (940.0, 500.0)]
            );
        });

        cx.simulate_keystrokes(window, "s");
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.layout().visible_pages(0.0, 700.0), 0..1);
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                for _ in 0..4 {
                    frame.scroll(
                        "page",
                        gpui::ScrollDelta::Pixels(point(px(0.0), px(-5000.0))),
                        cx,
                    );
                }
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 0);
            assert_eq!(reader.reading_position(), (0, 0.0));
        });
        cx.simulate_keystrokes(window, "right");
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert_eq!(reader.reading_position().0, 1);
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                    cx,
                );
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert!(reader.reading_position().1 > 0.0);
        });
        reader.update(cx, |reader, cx| reader.scroll_by(10000.0, cx));
        let bottom = reader.read_with(cx, |reader, _| reader.reading_position().1);
        assert!(bottom > 500.0);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                for _ in 0..4 {
                    frame.scroll(
                        "page",
                        gpui::ScrollDelta::Pixels(point(px(0.0), px(-5000.0))),
                        cx,
                    );
                }
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert!((reader.reading_position().1 - bottom).abs() < 1.0);
            assert_eq!(reader.layout().visible_pages(0.0, 700.0), 1..2);
        });
        reader.update(cx, |reader, cx| reader.zoom_in(cx));
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert!((reader.reading_position().1 - bottom).abs() < 1.0);
        });
        reader.update(cx, |reader, cx| reader.fit_width(cx));
        let before_resize = reader.read_with(cx, |reader, _| reader.scale());
        cx.simulate_window_resize(window, size(px(600.0), px(700.0)));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert!(reader.scale() < before_resize);
            assert!(reader.reading_position().1 <= reader.pages[1].1);
        });
        cx.simulate_keystrokes(window, "right");
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 2);
            assert_eq!(reader.reading_position().0, 2);
        });
        cx.simulate_keystrokes(window, "right");
        assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 2);
        cx.simulate_keystrokes(window, "left c");
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
            assert_eq!(reader.reading_position().0, 1);
        });
    }

    #[gpui_kit::test]
    fn opening_and_replacing_documents_ignores_obsolete_loads(cx: &mut TestAppContext) {
        let reader = cx.new(|cx| Preview::new(None, 1.0, cx));
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.status(), "Open a PDF to start reading");
        });

        reader.update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("examples/missing.pdf"), cx);
            reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), Some(100));
            assert_eq!(reader.page_index(), 0);
            assert_eq!(reader.status(), "Page 1 of 100 · 100%");
        });

        reader.update(cx, |reader, cx| {
            reader.next_page(cx);
            reader.zoom_in(cx);
        });
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert_eq!(reader.scale(), 1.25);
        });
        reader.update(cx, |reader, cx| reader.previous_page(cx));
        assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
    }

    #[gpui_kit::test]
    fn opening_zoom_uses_settings_per_document_without_refitting_other_windows(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let pdf = root.path().join("pages.pdf");
        std::fs::write(&pdf, three_page_pdf([(400, 1200), (400, 800), (400, 1200)])).unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(&config, "[reader]\ninitial_zoom = 'fit_width'\n").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            crate::settings_ui::install_with_paths(
                cx,
                Some(crate::settings::AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
        });
        let first =
            cx.add_window(|frame, cx| Preview::new(Some(pdf.clone()), frame.scale_factor(), cx));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
                assert_eq!(reader.page_count(), Some(3));
                assert!(reader.scale() > 1.0);
            })
            .unwrap();
        let first_scale = first.read_with(cx, |reader, _| reader.scale()).unwrap();

        // A live preference edit changes only subsequent document opens.
        std::fs::write(
            &config,
            "[reader]\ninitial_zoom = 'fit_page'\nreading_view = 'page'\n",
        )
        .unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
                assert_eq!(reader.scale(), first_scale);
                assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
            })
            .unwrap();
        let second =
            cx.add_window(|frame, cx| Preview::new(Some(pdf.clone()), frame.scale_factor(), cx));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
                assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
                assert!(reader.scale() < first_scale);
            })
            .unwrap();
        let second_scale = second.read_with(cx, |reader, _| reader.scale()).unwrap();
        cx.simulate_window_resize(second.into(), size(px(600.), px(500.)));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
                assert!(reader.scale() < second_scale);
            })
            .unwrap();

        // The reader's actual keyboard Actions still change its local mode
        // without changing the preference or another reader.
        second
            .update(cx, |reader, _, cx| reader.scroll_by(80.0, cx))
            .unwrap();
        let position = second
            .read_with(cx, |reader, _| reader.reading_position())
            .unwrap();
        cx.simulate_keystrokes(second.into(), "w");
        cx.run_until_parked();
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
                let after = reader.reading_position();
                assert_eq!(after.0, position.0);
                assert!((after.1 - position.1).abs() < 2.0);
            })
            .unwrap();
        cx.simulate_keystrokes(second.into(), "=");
        cx.run_until_parked();
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
                let after = reader.reading_position();
                assert_eq!(after.0, position.0);
                assert!((after.1 - position.1).abs() < 2.0);
            })
            .unwrap();
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
                assert_eq!(reader.scale(), first_scale);
            })
            .unwrap();

        // Replacing a PDF through the same path as picker/Recent takes the new
        // default, while selecting 100% later resets only future opens.
        std::fs::write(&config, "[reader]\ninitial_zoom = '100_percent'\n").unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        second
            .update(cx, |reader, _, cx| reader.open_path(pdf, cx))
            .unwrap();
        cx.run_until_parked();
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
                assert!((reader.scale() - 1.0).abs() < 0.001);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn missing_config_finishes_startup_before_loading_at_100_percent(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("missing/config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::settings_ui::install_with_paths(
                cx,
                Some(crate::settings::AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
        });
        let reader = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        reader
            .read_with(cx, |reader, _| {
                assert!(reader.page_count().is_some());
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
                assert_eq!(reader.scale(), 1.0);
            })
            .unwrap();
        assert!(!config.exists());
    }

    #[gpui_kit::test]
    fn loading_errors_leave_the_reader_without_a_document(cx: &mut TestAppContext) {
        let reader =
            cx.new(|cx| Preview::new(Some(PathBuf::from("examples/missing.pdf")), 1.0, cx));
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert!(
                reader.status().starts_with("Cannot read "),
                "{}",
                reader.status()
            );
            assert_eq!(reader.page_count(), None);
        });

        reader.update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("Cargo.toml"), cx);
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.status(),
                "Cannot open PDF: invalid or unsupported PDF file"
            );
            assert_eq!(reader.page_count(), None);
        });
    }

    #[test]
    fn display_density_changes_bitmap_resolution_not_logical_zoom() {
        assert_eq!(raster_scale(1.0, 2.0, (612.0, 792.0)), 2.0);
        assert_eq!(raster_scale(1.0, 1.0, (612.0, 792.0)), 1.0);
    }

    #[test]
    fn high_zoom_is_bounded_for_a4_scans() {
        let scale = raster_scale(4.0, 2.0, (595.44, 842.04));
        assert!(scale < 8.0);
        assert!(scale > 7.0);
        assert!(595.44 * 842.04 * scale * scale <= 32_000_000.0);
    }
}
