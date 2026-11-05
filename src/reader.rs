//! Continuous reader: geometry drives layout; only the viewport owns bitmaps.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::{
    App, ClipboardItem, Context, Entity, FocusHandle, KeyBinding, RenderImage, ScrollHandle,
    Window, div, img, point, prelude::*, px, rgb,
};
use gpui_kit::{
    base::{Disableable, FocusTrapElement, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable, Sizable, TitleBar,
        button::{Button, ButtonVariants},
        command::{Command, CommandItem, CommandState},
        empty::{
            Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant,
            EmptyTitle,
        },
        spinner::Spinner,
        status_bar::StatusBar,
        theme::Theme,
        toolbar::{Toolbar, ToolbarGroup},
        tooltip::Tooltip,
    },
};
use image::{Frame, RgbaImage};

use crate::{
    coordinates::{PageViewport, Point},
    pdf,
    reader_sidebar::{ReaderSidebar, SidebarPanel},
    reader_windows::{self, NewWindow, NextWindow, PreviousWindow},
};

const GAP: f32 = 16.0;
const BITMAP_BUDGET: usize = 128 * 1024 * 1024;
const PAGE_PADDING: f32 = 16.0;
const FIT_LABEL_MIN_REMS: f32 = 35.0;

gpui::actions!(
    kver,
    [
        OpenPdf,
        ShowCommandPalette,
        NextPage,
        PreviousPage,
        ZoomIn,
        ZoomOut,
        FitWidth,
        FitPage,
        Copy,
        ShowFind,
        ShowContents,
        ToggleSidebar
    ]
);

/// Bind reading keys only in a focused reader, not in the palette's text input.
/// Kit resolves the displayed shortcuts from these active Action bindings.
pub fn bind_reader_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenPdf, None),
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("cmd-`", NextWindow, None),
        KeyBinding::new("cmd-shift-`", PreviousWindow, None),
        KeyBinding::new("cmd-shift-p", ShowCommandPalette, None),
        KeyBinding::new("cmd-f", ShowFind, None),
        KeyBinding::new("escape", ToggleSidebar, Some("ReaderSidebar")),
        KeyBinding::new("right", NextPage, Some("Reader")),
        KeyBinding::new("n", NextPage, Some("Reader")),
        KeyBinding::new("left", PreviousPage, Some("Reader")),
        KeyBinding::new("p", PreviousPage, Some("Reader")),
        KeyBinding::new("=", ZoomIn, Some("Reader")),
        KeyBinding::new("shift-=", ZoomIn, Some("Reader")),
        KeyBinding::new("-", ZoomOut, Some("Reader")),
        KeyBinding::new("w", FitWidth, Some("Reader")),
        KeyBinding::new("f", FitPage, Some("Reader")),
        // These modified bindings stay active while the palette has focus,
        // so Kit can show real shortcuts without hijacking search text.
        KeyBinding::new("cmd-alt-right", NextPage, None),
        KeyBinding::new("cmd-alt-left", PreviousPage, None),
        KeyBinding::new("cmd-alt-=", ZoomIn, None),
        KeyBinding::new("cmd-alt--", ZoomOut, None),
        KeyBinding::new("cmd-alt-w", FitWidth, None),
        KeyBinding::new("cmd-alt-f", FitPage, None),
        KeyBinding::new("cmd-c", Copy, Some("Reader")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomMode {
    Fixed,
    FitWidth,
    FitPage,
}

fn fit_scale(mode: ZoomMode, page: (f32, f32), viewport: (f32, f32)) -> Option<f32> {
    let (width, height) = viewport;
    if width <= PAGE_PADDING * 2.0 || height <= PAGE_PADDING * 2.0 {
        return None;
    }
    let width_scale = (width - PAGE_PADDING * 2.0) / page.0;
    let scale = match mode {
        ZoomMode::Fixed => return None,
        ZoomMode::FitWidth => width_scale,
        ZoomMode::FitPage => width_scale.min((height - PAGE_PADDING * 2.0) / page.1),
    };
    Some(scale.clamp(0.25, 4.0))
}

fn document_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn reader_button(
    id: &'static str,
    label: &'static str,
    icon: Option<IconName>,
    action: Box<dyn gpui::Action>,
    enabled: bool,
    selected: bool,
) -> Button {
    Button::new(id)
        .accessibility_label(label)
        .when_some(icon, |button, icon| button.icon(icon))
        .when(icon.is_none(), |button| button.label(label))
        .ghost()
        .selected(selected)
        .disabled(!enabled)
        .tooltip(label)
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

fn raster_scale(zoom: f32, density: f32, (width, height): (f32, f32)) -> f32 {
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    (zoom * density).clamp(0.25, 8.0).min(max_scale)
}

/// Page-local position stays in PDF points when layout or zoom changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    page: usize,
    y: f32,
}

fn page_top(pages: &[(f32, f32)], zoom: f32, index: usize) -> f32 {
    pages.iter().take(index).map(|&(_, h)| h * zoom + GAP).sum()
}

fn position_at(pages: &[(f32, f32)], zoom: f32, offset: f32) -> Position {
    let mut top = 0.0;
    for (index, &(_, height)) in pages.iter().enumerate() {
        let next = top + height * zoom + GAP;
        if offset < next || index + 1 == pages.len() {
            return Position {
                page: index,
                y: ((offset - top) / zoom).clamp(0.0, height),
            };
        }
        top = next;
    }
    Position { page: 0, y: 0.0 }
}

fn visible_range(pages: &[(f32, f32)], zoom: f32, top: f32, height: f32) -> std::ops::Range<usize> {
    let mut y = 0.0;
    let mut first = pages.len();
    let mut end = pages.len();
    for (index, &(_, page_height)) in pages.iter().enumerate() {
        let bottom = y + page_height * zoom;
        if bottom >= top && y <= top + height {
            first = first.min(index);
            end = index + 1;
        } else if y > top + height {
            break;
        }
        y = bottom + GAP;
    }
    first..end
}

fn current_page_at(pages: &[(f32, f32)], zoom: f32, top: f32, height: f32) -> usize {
    let last = pages.len() - 1;
    let document_end = page_top(pages, zoom, pages.len());
    // A short final page cannot be scrolled to the top; reaching the end
    // must still make it the current page.
    if top + height >= document_end - 1.0 {
        last
    } else {
        position_at(pages, zoom, top + height / 2.0).page
    }
}

struct Bitmap {
    image: Arc<RenderImage>,
    bytes: usize,
}

/// Boundary indices refer to visually ordered document-space glyphs, not pixels.
#[derive(Clone, Copy)]
struct TextAnchor {
    page: usize,
    line: usize,
    boundary: usize,
}

#[derive(Clone, Copy)]
struct TextSelection {
    start: TextAnchor,
    end: TextAnchor,
}

fn selected_ranges(selection: TextSelection, text: &pdf::PageText) -> Vec<std::ops::Range<usize>> {
    let (start, end) = if (selection.start.line, selection.start.boundary)
        <= (selection.end.line, selection.end.boundary)
    {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    (start.line..=end.line)
        .filter_map(|line| {
            let range = text.lines.get(line)?;
            let from = if line == start.line {
                start.boundary
            } else {
                range.start
            };
            let to = if line == end.line {
                end.boundary
            } else {
                range.end
            };
            (from < to && from >= range.start && to <= range.end).then_some(from..to)
        })
        .collect()
}

fn selected_text(selection: TextSelection, text: &pdf::PageText) -> Option<String> {
    let ranges = selected_ranges(selection, text);
    let mut result = String::new();
    let mut previous: Option<&pdf::TextGlyph> = None;
    for range in ranges {
        let first = text.glyphs.get(range.start)?;
        if let Some(last) = previous {
            result.push('\n');
            let height = last.top - last.bottom;
            if (last.start.y - first.start.y).abs() > height * 1.7 {
                result.push('\n');
            }
        }
        for glyph in &text.glyphs[range.clone()] {
            result.push_str(&glyph.text);
        }
        previous = text.glyphs.get(range.end - 1);
    }
    (!result.is_empty()).then_some(result)
}

fn hit_boundary(text: &pdf::PageText, at: Point) -> Option<(usize, usize)> {
    text.lines.iter().enumerate().find_map(|(line, range)| {
        text.glyphs[range.clone()]
            .iter()
            .enumerate()
            .filter(|(_, glyph)| {
                at.y >= glyph.bottom - 3.0
                    && at.y <= glyph.top + 3.0
                    && at.x >= glyph.start.x - 8.0
                    && at.x <= glyph.end.x + 8.0
            })
            .flat_map(|(offset, glyph)| {
                [
                    (line, range.start + offset, glyph.start),
                    (line, range.start + offset + 1, glyph.end),
                ]
            })
            .min_by(|(_, _, a), (_, _, b)| (a.x - at.x).abs().total_cmp(&(b.x - at.x).abs()))
            .map(|(line, boundary, _)| (line, boundary))
    })
}

pub struct Preview {
    path: Option<PathBuf>,
    scale: f32,
    zoom_mode: ZoomMode,
    display_scale: f32,
    pages: Vec<(f32, f32)>,
    images: HashMap<usize, Bitmap>,
    // Only one rasterization runs at once; the next job is chosen from the
    // *current* viewport, rather than queued ahead of a later jump.
    in_flight: Option<(usize, u64)>,
    generation: u64,
    document_generation: u64,
    text: HashMap<usize, pdf::PageText>,
    text_failed: std::collections::HashSet<usize>,
    text_in_flight: Option<usize>,
    selection: Option<TextSelection>,
    dragging: bool,
    failed: Option<usize>,
    page_index: usize,
    // A deliberate page turn remains current even if the viewport also shows
    // part of a neighboring page. Scrolling chooses a new current page.
    selected_page: Option<usize>,
    last_viewport: (f32, f32, f32),
    scroll: ScrollHandle,
    focus: FocusHandle,
    palette_focus: FocusHandle,
    focus_initialized: bool,
    palette: Option<Entity<CommandState>>,
    sidebar: Option<ReaderSidebar>,
    status: String,
}

impl Preview {
    pub fn new(path: Option<PathBuf>, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let mut reader = Self {
            path,
            scale: 1.0,
            zoom_mode: ZoomMode::Fixed,
            display_scale,
            pages: Vec::new(),
            images: HashMap::new(),
            in_flight: None,
            generation: 0,
            document_generation: 0,
            text: HashMap::new(),
            text_failed: std::collections::HashSet::new(),
            text_in_flight: None,
            selection: None,
            dragging: false,
            failed: None,
            page_index: 0,
            selected_page: None,
            last_viewport: (f32::NAN, f32::NAN, f32::NAN),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            palette_focus: cx.focus_handle(),
            focus_initialized: false,
            palette: None,
            sidebar: None,
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
        self.text.clear();
        self.text_failed.clear();
        self.text_in_flight = None;
        self.selection = None;
        self.dragging = false;
        self.path = Some(path);
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
        self.load(cx);
        cx.notify();
    }

    pub fn open_dialog(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        let choice = rfd::AsyncFileDialog::new()
            .set_title("Open PDF")
            .add_filter("PDF documents", &["pdf"])
            .pick_file();
        cx.spawn(async move |this, cx| {
            let result = choice.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                if let Some(file) = result {
                    this.open_path(file.path().to_path_buf(), cx);
                }
            });
        })
        .detach();
    }

    fn on_open_pdf(&mut self, _: &OpenPdf, _: &mut Window, cx: &mut Context<Self>) {
        self.open_dialog(cx);
    }

    fn show_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        let palette = cx.new(|cx| CommandState::new(window, cx));
        self.palette = Some(palette.clone());
        palette.update(cx, |palette, cx| palette.focus(window, cx));
        cx.notify();
    }

    fn on_show_palette(
        &mut self,
        _: &ShowCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_palette(window, cx);
    }

    fn dismiss_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        if let Some(sidebar) = self.sidebar.as_mut()
            && let Some(panel) = sidebar.active()
        {
            sidebar.open(panel, window, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn show_sidebar(&mut self, panel: SidebarPanel, window: &mut Window, cx: &mut Context<Self>) {
        let sidebar = self
            .sidebar
            .get_or_insert_with(|| ReaderSidebar::new(window, cx));
        sidebar.open(panel, window, cx);
    }

    fn close_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sidebar) = self.sidebar.as_mut()
            && sidebar.active().is_some()
        {
            sidebar.close(window, &self.focus, cx);
        }
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_panel().is_some() {
            self.close_sidebar(window, cx);
        } else if !self.pages.is_empty() {
            let panel = self
                .sidebar
                .as_ref()
                .map_or(SidebarPanel::Find, ReaderSidebar::last);
            self.show_sidebar(panel, window, cx);
        }
    }

    // The root receives bubbled key-down events even when a panel owns focus.
    // Only the focused reader handles bare scrolling keys.
    fn reading_allowed(&self, window: &Window) -> bool {
        self.palette.is_some()
            || self
                .sidebar
                .as_ref()
                .is_none_or(|sidebar| sidebar.active().is_none())
            || self.focus.is_focused(window)
    }

    pub fn sidebar_panel(&self) -> Option<SidebarPanel> {
        self.sidebar.as_ref().and_then(ReaderSidebar::active)
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let generation = self.generation;
        let work = cx
            .background_executor()
            .spawn(async move { pdf::document_pages(&path) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                match result {
                    Ok(pages) if !pages.is_empty() => {
                        self::Preview::install_pages(this, pages, cx);
                    }
                    Ok(_) => this.status = "PDF has no pages".into(),
                    Err(error) => this.status = error,
                }
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
    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }
    pub fn palette_query(&self, cx: &gpui::App) -> Option<gpui::SharedString> {
        self.palette
            .as_ref()
            .map(|palette| palette.read(cx).query(cx))
    }
    #[cfg(feature = "screenshot")]
    pub fn palette_matches(&self, cx: &gpui::App) -> Option<usize> {
        self.palette
            .as_ref()
            .map(|palette| palette.read(cx).matched_count())
    }
    #[cfg(feature = "screenshot")]
    pub fn reader_focus(&self) -> FocusHandle {
        self.focus.clone()
    }
    pub fn page_index(&self) -> usize {
        self.page_index
    }
    pub fn zoom_mode(&self) -> ZoomMode {
        self.zoom_mode
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    /// Page and PDF-point offset at the top of the viewport.
    pub fn reading_position(&self) -> (usize, f32) {
        let (top, _, _) = self.viewport();
        let position = position_at(&self.pages, self.scale, top);
        (position.page, position.y)
    }
    pub fn page_count(&self) -> Option<usize> {
        (!self.pages.is_empty()).then_some(self.pages.len())
    }
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
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
        let selection = self.selection?;
        self.text
            .get(&selection.start.page)
            .and_then(|page| selected_text(selection, page))
    }

    #[cfg(feature = "screenshot")]
    pub fn text_screen_point(
        &self,
        page: usize,
        glyph_index: usize,
        fraction: f64,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let glyph = self.text.get(&page)?.glyphs.get(glyph_index)?;
        let document = Point {
            x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
            y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
        };
        let screen = self.page_viewport(page)?.document_to_screen(document);
        Some(point(px(screen.x as f32), px(screen.y as f32)))
    }

    /// Measured scroll bounds and centered page layout define the window origin.
    fn page_viewport(&self, index: usize) -> Option<PageViewport> {
        let text = self.text.get(&index)?;
        let bounds = self.scroll.bounds();
        let width = self.pages.get(index)?.0 * self.scale;
        let inner_width = (f32::from(bounds.size.width) - PAGE_PADDING * 2.0).max(0.0);
        let origin = Point {
            x: f32::from(bounds.origin.x) as f64
                + PAGE_PADDING as f64
                + ((inner_width - width).max(0.0) / 2.0) as f64,
            y: f32::from(bounds.origin.y) as f64
                + PAGE_PADDING as f64
                + page_top(&self.pages, self.scale, index) as f64,
        };
        PageViewport::new(
            text.transform,
            self.scale as f64,
            origin,
            Point {
                x: -f32::from(self.scroll.offset().x) as f64,
                y: -f32::from(self.scroll.offset().y) as f64,
            },
        )
    }

    fn pointer_anchor(
        &self,
        page: usize,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<TextAnchor> {
        let viewport = self.page_viewport(page)?;
        let document = viewport.screen_to_document(Point {
            x: f32::from(position.x) as f64,
            y: f32::from(position.y) as f64,
        })?;
        let (line, boundary) = hit_boundary(self.text.get(&page)?, document)?;
        Some(TextAnchor {
            page,
            line,
            boundary,
        })
    }

    fn start_selection(
        &mut self,
        page: usize,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.selection = self
            .pointer_anchor(page, position)
            .map(|anchor| TextSelection {
                start: anchor,
                end: anchor,
            });
        self.dragging = self.selection.is_some();
        cx.notify();
    }

    fn extend_selection(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        let Some(mut selection) = self.selection else {
            return;
        };
        if let Some(anchor) = self.pointer_anchor(selection.start.page, position) {
            selection.end = anchor;
            self.selection = Some(selection);
            cx.notify();
        }
    }

    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn schedule_text(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() || self.text_in_flight.is_some() {
            return;
        }
        let (top, _, height) = self.viewport();
        let mut visible = visible_range(&self.pages, self.scale, top, height);
        self.text.retain(|index, _| {
            visible.contains(index)
                || self
                    .selection
                    .is_some_and(|selection| selection.start.page == *index)
        });
        let Some(index) = visible
            .find(|index| !self.text.contains_key(index) && !self.text_failed.contains(index))
        else {
            return;
        };
        let Some(path) = self.path.clone() else {
            return;
        };
        self.text_in_flight = Some(index);
        let generation = self.document_generation;
        let work = cx
            .background_executor()
            .spawn(async move { pdf::page_text(&path, index) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != generation {
                    return;
                }
                this.text_in_flight = None;
                match result {
                    Ok(text) => {
                        this.text.insert(index, text);
                    }
                    Err(_) => {
                        this.text_failed.insert(index);
                    }
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

    fn sync_viewport(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let (top, width, height) = self.viewport();
        let visible = visible_range(&self.pages, self.scale, top, height);
        if self
            .selected_page
            .is_some_and(|page| !visible.contains(&page))
        {
            self.selected_page = None;
        }
        let target = self
            .selected_page
            .unwrap_or_else(|| current_page_at(&self.pages, self.scale, top, height));
        // When a scroll changes the fit target, keep the point at the
        // viewport center stable. Anchoring the top instead can move the
        // center back across the boundary and refit the previous page.
        let anchor = if self.selected_page.is_none() && target != self.page_index {
            height / 2.0
        } else {
            0.0
        };
        self.page_index = target;
        let position = position_at(&self.pages, self.scale, top + anchor);
        self.apply_fit(self.page_index, position, (width, height), anchor);
        self.last_viewport = self.viewport();
        self.schedule(cx);
        self.schedule_text(cx);
        cx.notify();
    }

    fn apply_fit(&mut self, page: usize, position: Position, viewport: (f32, f32), anchor: f32) {
        if let Some(scale) = fit_scale(self.zoom_mode, self.pages[page], viewport) {
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
        self.scroll.set_offset(point(
            self.scroll.offset().x,
            px(-(page_top(&self.pages, next, position.page) + position.y * next - anchor).max(0.0)),
        ));
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
        self.page_index = index;
        self.selected_page = Some(index);
        if self.failed != Some(index) {
            self.failed = None;
        }
        let (_, width, height) = self.viewport();
        self.apply_fit(
            index,
            Position {
                page: index,
                y: 0.0,
            },
            (width, height),
            0.0,
        );
        self.scroll.set_offset(point(
            self.scroll.offset().x,
            px(-page_top(&self.pages, self.scale, index)),
        ));
        self.schedule(cx);
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
        self.zoom_mode = mode;
        if !self.pages.is_empty() {
            let (top, width, height) = self.viewport();
            self.page_index = self
                .selected_page
                .unwrap_or_else(|| current_page_at(&self.pages, self.scale, top, height));
            let position = position_at(&self.pages, self.scale, top);
            self.apply_fit(self.page_index, position, (width, height), 0.0);
            self.schedule(cx);
        }
        cx.notify();
    }
    fn change_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next == self.scale && self.zoom_mode == ZoomMode::Fixed {
            return;
        }
        let (top, _, _) = self.viewport();
        let position = position_at(&self.pages, self.scale, top);
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
        let range = visible_range(&self.pages, self.scale, top, height);
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
            .min_by_key(|index| (page_top(&self.pages, self.scale, *index) - top).abs() as i64);
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
                if visible_range(&this.pages, this.scale, top, height).contains(&index) {
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

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_initialized {
            window.focus(&self.focus, cx);
            cx.observe_window_appearance(window, |_, window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                cx.notify();
            })
            .detach();
            cx.observe_window_bounds(window, |_, window, cx| {
                // The scroll area's bounds are laid out in the resize frame.
                // Refit after that frame, using its measured content viewport.
                cx.on_next_frame(window, |this, _, cx| this.sync_viewport(cx));
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
        let range = visible_range(&self.pages, self.scale, top, height);
        let has_document = !self.pages.is_empty();
        let show_fit_labels = f32::from(window.bounds().size.width)
            >= FIT_LABEL_MIN_REMS * f32::from(window.rem_size());
        let filename = self
            .path
            .as_deref()
            .map(document_name)
            .map(gpui::SharedString::from);
        let top_space = page_top(&self.pages, self.scale, range.start);
        let end_space = page_top(&self.pages, self.scale, self.pages.len())
            - page_top(&self.pages, self.scale, range.end);
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .child(div().h(px(top_space)).flex_shrink_0());
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
            if let (Some(selection), Some(text)) = (self.selection, self.text.get(&index))
                && selection.start.page == index
                && selected_text(selection, text).is_some()
            {
                for range in selected_ranges(selection, text) {
                    let glyphs = &text.glyphs[range];
                    let first = &glyphs[0];
                    let last = &glyphs[glyphs.len() - 1];
                    let top = glyphs
                        .iter()
                        .map(|glyph| glyph.top)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let bottom = glyphs
                        .iter()
                        .map(|glyph| glyph.bottom)
                        .fold(f64::INFINITY, f64::min);
                    let corners = [
                        Point {
                            x: first.start.x,
                            y: top,
                        },
                        Point {
                            x: last.end.x,
                            y: top,
                        },
                        Point {
                            x: first.start.x,
                            y: bottom,
                        },
                        Point {
                            x: last.end.x,
                            y: bottom,
                        },
                    ]
                    .map(|corner| text.transform.document_to_page(corner));
                    let left = corners
                        .iter()
                        .map(|point| point.x)
                        .fold(f64::INFINITY, f64::min)
                        .clamp(0.0, width as f64);
                    let right = corners
                        .iter()
                        .map(|point| point.x)
                        .fold(f64::NEG_INFINITY, f64::max)
                        .clamp(0.0, width as f64);
                    let top = corners
                        .iter()
                        .map(|point| point.y)
                        .fold(f64::INFINITY, f64::min)
                        .clamp(0.0, self.pages[index].1 as f64);
                    let bottom = corners
                        .iter()
                        .map(|point| point.y)
                        .fold(f64::NEG_INFINITY, f64::max)
                        .clamp(0.0, self.pages[index].1 as f64);
                    page = page.child(
                        div()
                            .absolute()
                            .left(px((left * self.scale as f64) as f32))
                            .top(px((top * self.scale as f64) as f32))
                            .w(px(((right - left) * self.scale as f64) as f32))
                            .h(px(((bottom - top) * self.scale as f64) as f32))
                            .bg(gpui::rgba(0x278acb70)),
                    );
                }
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
                self.palette.is_none() && self.focus.is_focused(window),
                |reader| reader.key_context("Reader"),
            )
            .on_action(cx.listener(Self::on_open_pdf))
            .on_action(cx.listener(|_, _: &NewWindow, _, cx| {
                if let Err(error) = reader_windows::open(cx, None) {
                    eprintln!("Cannot open window: {error}");
                }
            }))
            .on_action(cx.listener(|_, _: &NextWindow, window, cx| {
                reader_windows::cycle(window, cx, 1);
            }))
            .on_action(cx.listener(|_, _: &PreviousWindow, window, cx| {
                reader_windows::cycle(window, cx, -1);
            }))
            .on_action(cx.listener(Self::on_show_palette))
            .on_action(cx.listener(|this, _: &ShowFind, window, cx| {
                if !this.pages.is_empty() {
                    this.show_sidebar(SidebarPanel::Find, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ShowContents, window, cx| {
                if !this.pages.is_empty() {
                    this.show_sidebar(SidebarPanel::Contents, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.toggle_sidebar(window, cx);
            }))
            .on_action(cx.listener(Self::on_copy))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
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
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.palette.is_some() || !this.reading_allowed(window) {
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
                TitleBar::new()
                    .pr_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // TitleBar's inner bar does not shrink; give the
                            // filename a zero basis so Open PDF stays visible.
                            .w_0()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_color(cx.theme().primary)
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("K /"),
                            )
                            .child(
                                div()
                                    .id("document-name")
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_sm()
                                    .when_some(filename.clone(), |title, name| {
                                        title.tooltip(move |window, cx| {
                                            Tooltip::new(name.clone()).build(window, cx)
                                        })
                                    })
                                    .child(
                                        filename.unwrap_or_else(|| "A quiet place to read".into()),
                                    ),
                            ),
                    )
                    .child(
                        Button::new("open-pdf")
                            .label("Open PDF…")
                            .icon(IconName::FolderOpen)
                            .ghost()
                            .small()
                            .tooltip_with_action("Choose a PDF", &OpenPdf, None)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenPdf), cx)
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .when_some(
                        self.sidebar
                            .as_ref()
                            .filter(|sidebar| sidebar.active().is_some()),
                        |row, sidebar| row.child(sidebar.render(window, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                Toolbar::new("reading-toolbar")
                                    .px_3()
                                    .content(reader_button(
                                        "toggle-sidebar",
                                        if self.sidebar_panel().is_some() {
                                            "Hide sidebar"
                                        } else {
                                            "Show sidebar"
                                        },
                                        Some(IconName::PanelLeft),
                                        Box::new(ToggleSidebar),
                                        has_document,
                                        self.sidebar_panel().is_some(),
                                    ))
                                    .content(
                                        gpui_kit::component::separator::Separator::vertical().h_5(),
                                    )
                                    .content(
                                        ToolbarGroup::new("page-controls")
                                            .label("Page navigation")
                                            .child(reader_button(
                                                "previous-page",
                                                "Previous page",
                                                Some(IconName::ArrowUp),
                                                Box::new(PreviousPage),
                                                has_document && self.page_index > 0,
                                                false,
                                            ))
                                            .child(reader_button(
                                                "next-page",
                                                "Next page",
                                                Some(IconName::ArrowDown),
                                                Box::new(NextPage),
                                                has_document
                                                    && self.page_index + 1 < self.pages.len(),
                                                false,
                                            )),
                                    )
                                    .content(
                                        gpui_kit::component::separator::Separator::vertical().h_5(),
                                    )
                                    .content(
                                        ToolbarGroup::new("zoom-controls")
                                            .label("Zoom and fit")
                                            .child(reader_button(
                                                "zoom-out",
                                                "Zoom out",
                                                Some(IconName::ZoomOut),
                                                Box::new(ZoomOut),
                                                has_document,
                                                false,
                                            ))
                                            .child(reader_button(
                                                "zoom-in",
                                                "Zoom in",
                                                Some(IconName::ZoomIn),
                                                Box::new(ZoomIn),
                                                has_document,
                                                false,
                                            ))
                                            .child(
                                                reader_button(
                                                    "fit-width",
                                                    "Fit width",
                                                    Some(IconName::MoveHorizontal),
                                                    Box::new(FitWidth),
                                                    has_document,
                                                    has_document
                                                        && self.zoom_mode == ZoomMode::FitWidth,
                                                )
                                                .when(show_fit_labels, |button| {
                                                    button.label("Fit width")
                                                }),
                                            )
                                            .child(
                                                reader_button(
                                                    "fit-page",
                                                    "Fit page",
                                                    Some(IconName::Expand),
                                                    Box::new(FitPage),
                                                    has_document,
                                                    has_document
                                                        && self.zoom_mode == ZoomMode::FitPage,
                                                )
                                                .when(show_fit_labels, |button| {
                                                    button.label("Fit page")
                                                }),
                                            ),
                                    )
                                    .content(div().flex_1())
                                    .child(reader_button(
                                        "copy-selection",
                                        "Copy selection",
                                        Some(IconName::Copy),
                                        Box::new(Copy),
                                        self.selected_text().is_some(),
                                        false,
                                    )),
                            )
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
                                    .on_mouse_move(cx.listener(
                                        |this, event: &gpui::MouseMoveEvent, _, cx| {
                                            if event.pressed_button == Some(gpui::MouseButton::Left)
                                            {
                                                this.extend_selection(event.position, cx);
                                            }
                                        },
                                    ))
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
                                            this.dragging = false;
                                        }),
                                    )
                                    .on_scroll(cx.listener(|this, _, _, cx| {
                                        this.selected_page = None;
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
                                                            window.dispatch_action(
                                                                Box::new(OpenPdf),
                                                                cx,
                                                            )
                                                        }),
                                                ),
                                            ),
                                    )
                                    .into_any_element()
                                    } else {
                                        content.into_any_element()
                                    }),
                            )
                            .child(
                                StatusBar::new()
                                    .left(div().min_w(px(0.0)).truncate().child(if has_document {
                                        format!(
                                            "PAGE {} / {}",
                                            self.page_index + 1,
                                            self.pages.len()
                                        )
                                    } else {
                                        self.status.clone()
                                    }))
                                    .when(
                                        has_document && !self.status.starts_with("Page "),
                                        |bar| {
                                            bar.child(
                                                div()
                                                    .min_w(px(0.0))
                                                    .truncate()
                                                    .child(self.status.clone()),
                                            )
                                        },
                                    )
                                    .when(has_document, |bar| {
                                        bar.right(div().child(format!(
                                            "{}%{}",
                                            (self.scale * 100.0).round(),
                                            match self.zoom_mode {
                                                ZoomMode::Fixed => "",
                                                ZoomMode::FitWidth => " · Fit width",
                                                ZoomMode::FitPage => " · Fit page",
                                            }
                                        )))
                                    }),
                            ),
                    ),
            )
            .when_some(self.palette.clone(), |reader, palette| {
                let owner = cx.weak_entity();
                reader
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(cx.theme().overlay)
                            .on_mouse_down(gpui::MouseButton::Left, {
                                let owner = owner.clone();
                                move |_, window, cx| {
                                    let _ = owner.update(cx, |reader, cx| {
                                        reader.dismiss_palette(window, cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .id("palette-panel")
                            .test_support()
                            .role(gpui::Role::Dialog)
                            .aria_label("Commands")
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_4()
                            .child(
                                div().w_full().max_w(gpui::rems(24.)).child(
                                    Command::new(&palette)
                                        .placeholder("Search actions…")
                                        .item(
                                            CommandItem::new()
                                                .label("Open PDF…")
                                                .action(Box::new(OpenPdf)),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Find")
                                                .action(Box::new(ShowFind))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Contents")
                                                .action(Box::new(ShowContents))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Toggle Sidebar")
                                                .action(Box::new(ToggleSidebar))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("New Window")
                                                .action(Box::new(NewWindow)),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Next Window")
                                                .action(Box::new(NextWindow))
                                                .disabled(cx.windows().len() < 2),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Previous Window")
                                                .action(Box::new(PreviousWindow))
                                                .disabled(cx.windows().len() < 2),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Next Page")
                                                .action(Box::new(NextPage))
                                                .disabled(
                                                    !has_document
                                                        || self.page_index + 1 >= self.pages.len(),
                                                ),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Previous Page")
                                                .action(Box::new(PreviousPage))
                                                .disabled(!has_document || self.page_index == 0),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Zoom In")
                                                .action(Box::new(ZoomIn))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Zoom Out")
                                                .action(Box::new(ZoomOut))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Fit Width")
                                                .action(Box::new(FitWidth))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Fit Page")
                                                .action(Box::new(FitPage))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Copy")
                                                .action(Box::new(Copy))
                                                .disabled(self.selected_text().is_none()),
                                        )
                                        .on_confirm({
                                            let owner = owner.clone();
                                            move |_, window, cx| {
                                                let _ = owner.update(cx, |reader, cx| {
                                                    reader.dismiss_palette(window, cx)
                                                });
                                            }
                                        })
                                        .on_cancel(move |window, cx| {
                                            let _ = owner.update(cx, |reader, cx| {
                                                reader.dismiss_palette(window, cx)
                                            });
                                        }),
                                ),
                            )
                            .focus_trap("reader-command-palette", &self.palette_focus),
                    )
            })
    }
}

impl Preview {
    pub fn scroll_by(&mut self, amount: f32, cx: &mut Context<Self>) {
        self.selected_page = None;
        let top = (-f32::from(self.scroll.offset().y) + amount).max(0.0);
        let maximum =
            (page_top(&self.pages, self.scale, self.pages.len()) - self.viewport().2).max(0.0);
        self.scroll
            .set_offset(point(self.scroll.offset().x, px(-top.min(maximum))));
        self.sync_viewport(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_uses_filename_instead_of_parent_directories() {
        assert_eq!(
            document_name(Path::new("/long/parent/directory/report.pdf")),
            "report.pdf"
        );
        assert_eq!(
            document_name(Path::new("/long/parent/directory/very-long-file-name.pdf")),
            "very-long-file-name.pdf"
        );
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

    #[test]
    fn long_document_only_lays_out_visible_pages() {
        let pages = vec![(612.0, 792.0); 1000];
        assert_eq!(visible_range(&pages, 1.0, 0.0, 700.0), 0..1);
        assert_eq!(
            visible_range(&pages, 1.0, page_top(&pages, 1.0, 500), 700.0),
            500..501
        );
        assert_eq!(
            visible_range(&pages, 1.0, page_top(&pages, 1.0, 999), 700.0),
            999..1000
        );
    }

    #[test]
    fn current_page_ignores_a_sliver_but_counts_a_short_final_page() {
        let pages = [(600.0, 1200.0), (300.0, 400.0), (600.0, 180.0)];
        let zoom = 1.0;
        // Ten points of the previous page are visible above page two.
        assert_eq!(
            current_page_at(&pages, zoom, page_top(&pages, zoom, 1) - 10.0, 450.0),
            1
        );
        // The last page is shorter than half a viewport, but at the document
        // end it is still the page the reader has reached.
        let height = 700.0;
        assert_eq!(
            current_page_at(
                &pages,
                zoom,
                page_top(&pages, zoom, pages.len()) - height,
                height
            ),
            2
        );
    }

    #[test]
    fn page_local_position_survives_zoom() {
        let pages = vec![(100.0, 200.0), (150.0, 300.0)];
        let position = position_at(&pages, 1.0, 216.0 + 75.0);
        assert_eq!(position, Position { page: 1, y: 75.0 });
        assert_eq!(
            position_at(&pages, 2.0, page_top(&pages, 2.0, 1) + 150.0),
            position
        );
    }

    #[test]
    fn fit_uses_current_page_and_inner_viewport() {
        let viewport = (632.0, 832.0);
        assert_eq!(
            fit_scale(ZoomMode::FitWidth, (600.0, 1200.0), viewport),
            Some(1.0)
        );
        assert_eq!(
            fit_scale(ZoomMode::FitPage, (600.0, 1200.0), viewport),
            Some(2.0 / 3.0)
        );
        assert_eq!(
            fit_scale(ZoomMode::FitWidth, (300.0, 400.0), viewport),
            Some(2.0)
        );
        assert_eq!(
            fit_scale(ZoomMode::FitPage, (300.0, 400.0), viewport),
            Some(2.0)
        );
        assert_eq!(fit_scale(ZoomMode::Fixed, (300.0, 400.0), viewport), None);
        assert_eq!(
            fit_scale(ZoomMode::FitWidth, (300.0, 400.0), (0.0, 800.0)),
            None
        );
        assert_eq!(
            fit_scale(ZoomMode::FitPage, (300.0, 400.0), (632.0, 20.0)),
            None
        );
        assert_eq!(
            fit_scale(ZoomMode::FitWidth, (600.0, 1200.0), (332.0, 832.0)),
            Some(0.5)
        );
        assert_eq!(
            fit_scale(ZoomMode::FitPage, (600.0, 1200.0), (632.0, 632.0)),
            Some(0.5)
        );
    }
}
