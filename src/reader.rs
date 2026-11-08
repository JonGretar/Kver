//! Reader geometry drives layout; only the viewport owns bitmaps.

mod commands;
mod contents;
mod find;
mod marks;
mod navigation;
mod rendering;
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
use rendering::{Completion, PageRasters, Schedule};
use selection::Selection;

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    coordinates::{PageViewport, Point},
    find_search, pdf,
    recent::{self, ReadingPosition, RecentStore},
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
        spinner::Spinner,
    },
};

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
    rasters: PageRasters,
    document_generation: u64,
    selection: Selection,
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
            rasters: PageRasters::default(),
            document_generation: 0,
            selection: Selection::default(),
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
                if document_generation != this.document_generation {
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
        let y = if self.reading_mode == ReadingMode::SinglePage {
            self.reading_position().1
        } else {
            0.0
        };
        self.focus_page(index);
        self.rasters.on_page_selected(index);
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
        let Some(path) = self.path.as_ref() else {
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
        let path = path.clone();
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_bitmap(&path, request.page(), request.scale()) });
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
            let image = self.rasters.image(index).cloned();
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
                |reader| reader.key_context(crate::key_context::READER),
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
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_by(100.0, cx)))
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_by(-100.0, cx)))
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
mod tests;
