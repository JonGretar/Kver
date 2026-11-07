//! Reader geometry drives layout; only the viewport owns bitmaps.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::{
    Animation, AnimationExt, App, BoxShadow, ClipboardItem, Context, Entity, FocusHandle,
    KeyBinding, RenderImage, ScrollHandle, Subscription, Window, div, img, point, prelude::*, px,
    rgb,
};
use gpui_kit::{
    base::{Disableable, FocusTrapElement, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable, Sizable, TitleBar, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants},
        command::{Command, CommandItem, CommandState},
        dialog::DialogEntrance,
        empty::{
            Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant,
            EmptyTitle,
        },
        input::{Input, InputEvent, InputState},
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
    find_panel::MAX_MATCHES,
    find_search, pdf,
    reader_sidebar::{ReaderSidebar, SidebarPanel},
    reader_windows::{self, CloseWindow, NewWindow, NextWindow, PreviousWindow},
    recent::{self, LocalMark, ReadingPosition, RecentFile, RecentStore},
    settings::{InitialZoom, ReadingView},
    settings_ui::{self, ShowSettings},
};

const GAP: f32 = 16.0;
const BITMAP_BUDGET: usize = 128 * 1024 * 1024;
const PAGE_PADDING: f32 = 16.0;
const FIT_LABEL_MIN_REMS: f32 = 35.0;

gpui::actions!(
    kver,
    [
        OpenPdf,
        ShowRecent,
        ShowCommandPalette,
        ShowMarks,
        ShowSetMark,
        ShowJumpMark,
        ShowDeleteMark,
        NextPage,
        PreviousPage,
        ZoomIn,
        ZoomOut,
        FitWidth,
        FitPage,
        SinglePage,
        Continuous,
        UseDefaultReadingMode,
        Copy,
        ShowFind,
        NextMatch,
        PreviousMatch,
        ShowContents,
        ToggleSidebar,
        ActivateOutline
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct SetLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct JumpLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct DeleteLocalMark(pub char);

// GPUI keeps incomplete key bindings pending and resolves the next keystroke
// before dispatching one-key reader shortcuts (notably `p`, `n`, `s`, and `c`).
const MARK_KEY_SEQUENCES: [(char, &str, &str, &str); 26] = [
    ('a', "m a", "' a", "d m a"),
    ('b', "m b", "' b", "d m b"),
    ('c', "m c", "' c", "d m c"),
    ('d', "m d", "' d", "d m d"),
    ('e', "m e", "' e", "d m e"),
    ('f', "m f", "' f", "d m f"),
    ('g', "m g", "' g", "d m g"),
    ('h', "m h", "' h", "d m h"),
    ('i', "m i", "' i", "d m i"),
    ('j', "m j", "' j", "d m j"),
    ('k', "m k", "' k", "d m k"),
    ('l', "m l", "' l", "d m l"),
    ('m', "m m", "' m", "d m m"),
    ('n', "m n", "' n", "d m n"),
    ('o', "m o", "' o", "d m o"),
    ('p', "m p", "' p", "d m p"),
    ('q', "m q", "' q", "d m q"),
    ('r', "m r", "' r", "d m r"),
    ('s', "m s", "' s", "d m s"),
    ('t', "m t", "' t", "d m t"),
    ('u', "m u", "' u", "d m u"),
    ('v', "m v", "' v", "d m v"),
    ('w', "m w", "' w", "d m w"),
    ('x', "m x", "' x", "d m x"),
    ('y', "m y", "' y", "d m y"),
    ('z', "m z", "' z", "d m z"),
];

/// Bind reading keys only in a focused reader, not in the palette's text input.
/// Kit resolves the displayed shortcuts from these active Action bindings.
pub fn bind_reader_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenPdf, None),
        KeyBinding::new("cmd-shift-o", ShowRecent, None),
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("cmd-`", NextWindow, None),
        KeyBinding::new("cmd-shift-`", PreviousWindow, None),
        KeyBinding::new("cmd-shift-p", ShowCommandPalette, None),
        KeyBinding::new("cmd-shift-m", ShowMarks, None),
        KeyBinding::new("cmd-f", ShowFind, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-g", NextMatch, None),
        KeyBinding::new("cmd-shift-g", PreviousMatch, None),
        KeyBinding::new("cmd-t", ShowContents, None),
        KeyBinding::new("escape", ToggleSidebar, Some("ReaderSidebar")),
        KeyBinding::new("enter", ActivateOutline, Some("ContentsPanel")),
        KeyBinding::new("right", NextPage, Some("Reader")),
        KeyBinding::new("n", NextPage, Some("Reader")),
        KeyBinding::new("left", PreviousPage, Some("Reader")),
        KeyBinding::new("p", PreviousPage, Some("Reader")),
        KeyBinding::new("=", ZoomIn, Some("Reader")),
        KeyBinding::new("shift-=", ZoomIn, Some("Reader")),
        KeyBinding::new("-", ZoomOut, Some("Reader")),
        KeyBinding::new("w", FitWidth, Some("Reader")),
        KeyBinding::new("f", FitPage, Some("Reader")),
        KeyBinding::new("s", SinglePage, Some("Reader")),
        KeyBinding::new("c", Continuous, Some("Reader")),
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
    cx.bind_keys(
        MARK_KEY_SEQUENCES
            .into_iter()
            .flat_map(|(letter, set, jump, delete)| {
                [
                    KeyBinding::new(set, SetLocalMark(letter), Some("Reader")),
                    KeyBinding::new(jump, JumpLocalMark(letter), Some("Reader")),
                    KeyBinding::new(delete, DeleteLocalMark(letter), Some("Reader")),
                ]
            }),
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomMode {
    Fixed,
    FitWidth,
    FitPage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingMode {
    #[default]
    Continuous,
    SinglePage,
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
    marks_panel: Option<MarksPanel>,
    _mark_name_subscription: Option<Subscription>,
    recent: Option<RecentPanel>,
    recent_revision: u64,
    recent_open: Option<u64>,
    sidebar: Option<ReaderSidebar>,
    pending_find_reset: bool,
    outline_in_flight: bool,
    status: String,
}

#[derive(Clone)]
struct RecentPanel {
    command: Entity<CommandState>,
    files: Vec<RecentFile>,
    error: Option<String>,
    loading: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarksView {
    List,
    Set,
    Jump,
    Delete,
    Name(char),
}

#[derive(Clone)]
struct MarksPanel {
    command: Entity<CommandState>,
    view: MarksView,
    marks: BTreeMap<char, LocalMark>,
    name_input: Option<Entity<InputState>>,
    loading: bool,
    error: Option<String>,
}

fn recent_content(
    panel: &RecentPanel,
    owner: gpui::WeakEntity<Preview>,
    cx: &mut App,
) -> impl IntoElement {
    let entries = panel
        .files
        .iter()
        .map(|file| {
            let path = file.path.display().to_string();
            let name = document_name(&file.path);
            let file_path = file.path.clone();
            let owner = owner.clone();
            let loading = panel.loading;
            CommandItem::new()
                .label(name.clone())
                .keywords([path.clone()])
                .disabled(loading)
                .child(move |_, cx| {
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().truncate().child(name.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(path.clone()),
                                ),
                        )
                        .child(
                            Button::new(format!("forget-pdf:{path}"))
                                .icon(IconName::Close)
                                .accessibility_label(format!("Forget {}", name))
                                .tooltip("Forget PDF")
                                .ghost()
                                .xsmall()
                                .disabled(loading)
                                .on_click({
                                    let owner = owner.clone();
                                    let file_path = file_path.clone();
                                    let name = name.clone();
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        let owner = owner.clone();
                                        let path = file_path.clone();
                                        let name = name.clone();
                                        window.open_alert_dialog(cx, move |alert, _, _| {
                                            let owner = owner.clone();
                                            let focus_owner = owner.clone();
                                            let path = path.clone();
                                            alert
                                                .title(format!("Forget \"{name}\"?"))
                                                .description("Remove this PDF from Open Recent and erase its saved reading data. The PDF file will not be deleted.")
                                                .confirm()
                                                .centered()
                                                .entrance(DialogEntrance::None)
                                                .ok_text("Forget PDF")
                                                .ok_variant(ButtonVariant::Danger)
                                                .on_ok(move |_, _, cx| {
                                                    let _ = owner.update(cx, |reader, cx| {
                                                        reader.remove_recent(path.clone(), cx);
                                                    });
                                                    true
                                                })
                                                .on_close(move |_, window, _| {
                                                    let owner = focus_owner.clone();
                                                    window.on_next_frame(move |window, cx| {
                                                        let _ = owner.update(cx, |reader, cx| {
                                                            if let Some(panel) = reader.recent.as_ref() {
                                                                panel.command.update(cx, |state, cx| state.focus(window, cx));
                                                            }
                                                        });
                                                    });
                                                })
                                        });
                                    }
                                }),
                        )
                })
        })
        .collect::<Vec<_>>();
    let loading = panel.loading;
    let error = panel.error.clone();
    div()
        .id("recent-panel")
        .test_support()
        .role(gpui::Role::Dialog)
        .aria_label("Open Recent")
        .w_full()
        .child(
            Command::new(&panel.command)
                .bordered(false)
                .placeholder("Search stored PDFs…")
                .items(entries)
                .empty(|_, _, cx| {
                    div()
                        .p_4()
                        .text_color(cx.theme().muted_foreground)
                        .child("No stored PDFs match this search")
                })
                .on_query({
                    let owner = owner.clone();
                    move |query, _, cx| {
                        let _ = owner
                            .update(cx, |reader, cx| reader.search_recent(query.to_owned(), cx));
                    }
                })
                .on_confirm(move |index, _, cx| {
                    let _ = owner.update(cx, |reader, cx| reader.open_recent(index.row, cx));
                }),
        )
        .when(loading, |content| {
            content.child(
                div()
                    .px_3()
                    .py_2()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading stored PDFs…"),
            )
        })
        .when_some(error, |content, error| {
            content.child(
                div()
                    .id("recent-error")
                    .test_support()
                    .role(gpui::Role::Status)
                    .aria_label(error.clone())
                    .px_3()
                    .py_2()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
}

impl Preview {
    pub fn new(path: Option<PathBuf>, display_scale: f32, cx: &mut Context<Self>) -> Self {
        if let Some(revision) = recent::revision(cx) {
            cx.observe(&revision, |reader, _, cx| {
                if let Some(panel) = reader.recent.as_ref() {
                    let query = panel.command.read(cx).query(cx).to_string();
                    reader.search_recent(query, cx);
                }
                if reader.marks_panel.is_some() {
                    reader.refresh_marks(cx);
                }
            })
            .detach();
        }
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
            marks_panel: None,
            _mark_name_subscription: None,
            recent: None,
            recent_revision: 0,
            recent_open: None,
            sidebar: None,
            pending_find_reset: false,
            outline_in_flight: false,
            status: "Open a PDF to start reading".into(),
        };
        let settings = settings_ui::state(cx);
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
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
        self.recent_open = None;
        self.marks_panel = None;
        self._mark_name_subscription = None;
        if let Some(sidebar) = self.sidebar.as_mut() {
            sidebar.reset_document(cx);
        }
        self.outline_in_flight = false;
        self.text.clear();
        self.text_failed.clear();
        self.text_in_flight = None;
        self.selection = None;
        if let Some(sidebar) = self.sidebar.as_mut() {
            sidebar.find_mut().clear_results();
            self.pending_find_reset = true;
        }
        self.dragging = false;
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
        self.marks_panel = None;
        self._mark_name_subscription = None;
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

    fn show_marks(&mut self, view: MarksView, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.is_empty() || self.record_path.is_none() || self.recent.is_some() {
            return;
        }
        self.palette = None;
        self._mark_name_subscription = None;
        let command = cx.new(|cx| CommandState::new(window, cx));
        command.update(cx, |state, cx| state.focus(window, cx));
        self.marks_panel = Some(MarksPanel {
            command,
            view,
            marks: BTreeMap::new(),
            name_input: None,
            loading: true,
            error: None,
        });
        self.refresh_marks(cx);
    }

    fn dismiss_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marks_panel = None;
        self._mark_name_subscription = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn edit_mark_name(&mut self, letter: char, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mark) = self
            .marks_panel
            .as_ref()
            .and_then(|panel| panel.marks.get(&letter))
        else {
            return;
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Optional name")
                .default_value(mark.label.clone().unwrap_or_default())
        });
        let subscription = cx.subscribe_in(&input, window, move |reader, _, event, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                reader.save_mark_name(letter, cx);
            }
        });
        if let Some(panel) = self.marks_panel.as_mut() {
            panel.view = MarksView::Name(letter);
            panel.name_input = Some(input.clone());
            panel.error = None;
        }
        self._mark_name_subscription = Some(subscription);
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn cancel_mark_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.marks_panel.as_mut() {
            panel.name_input = None;
            self._mark_name_subscription = None;
            panel.view = MarksView::List;
            panel.error = None;
            panel
                .command
                .update(cx, |command, cx| command.focus(window, cx));
            cx.notify();
        }
    }

    fn save_mark_name(&mut self, letter: char, cx: &mut Context<Self>) {
        let (Some(panel), Some(path), Some(history)) = (
            self.marks_panel.as_mut(),
            self.record_path.clone(),
            recent::history(cx),
        ) else {
            return;
        };
        if panel.loading {
            return;
        }
        let Some(input) = panel.name_input.as_ref() else {
            return;
        };
        let name = input.read(cx).value().trim().to_owned();
        if name.chars().count() > 80 || name.chars().any(char::is_control) {
            panel.error = Some("Use at most 80 characters on one line.".into());
            cx.notify();
            return;
        }
        let label = (!name.is_empty()).then_some(name);
        panel.loading = true;
        let generation = self.document_generation;
        let work = history.name_mark(path.clone(), letter, label);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.document_generation != generation
                    || this.record_path.as_ref() != Some(&path)
                {
                    return;
                }
                if let Some(panel) = this.marks_panel.as_mut() {
                    if panel.view != MarksView::Name(letter) {
                        return;
                    }
                    panel.loading = false;
                    match result {
                        Ok(marks) => {
                            panel.marks = marks;
                            panel.name_input = None;
                            this._mark_name_subscription = None;
                            panel.view = MarksView::List;
                            panel
                                .command
                                .update(cx, |command, cx| command.focus(window, cx));
                        }
                        Err(error) => {
                            panel.error = Some(format!("Mark name could not be saved: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_marks(&mut self, cx: &mut Context<Self>) {
        let (Some(path), Some(history)) = (self.record_path.clone(), recent::history(cx)) else {
            return;
        };
        let generation = self.document_generation;
        let epoch = history.epoch();
        let work = history.marks(path.clone(), epoch);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != generation
                    || this.record_path.as_ref() != Some(&path)
                {
                    return;
                }
                if let Some(panel) = this.marks_panel.as_mut() {
                    panel.loading = false;
                    match result {
                        Ok(marks) => panel.marks = marks,
                        Err(error) => {
                            panel.error = Some(format!("Marks could not be loaded: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn change_mark(
        &mut self,
        letter: char,
        position: Option<ReadingPosition>,
        cx: &mut Context<Self>,
    ) {
        let (Some(path), Some(history)) = (self.record_path.clone(), recent::history(cx)) else {
            return;
        };
        if !letter.is_ascii_lowercase() || self.pages.is_empty() {
            return;
        }
        let generation = self.document_generation;
        let work = history.change_mark(path.clone(), letter, position);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != generation
                    || this.record_path.as_ref() != Some(&path)
                {
                    return;
                }
                match result {
                    Ok(marks) => {
                        if let Some(panel) = this.marks_panel.as_mut() {
                            panel.marks = marks;
                        }
                        this.status = if position.is_some() {
                            format!("Set local mark {letter}")
                        } else {
                            format!("Deleted local mark {letter}")
                        };
                    }
                    Err(error) => this.status = format!("Local mark could not be saved: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn jump_mark(&mut self, letter: char, cx: &mut Context<Self>) {
        let (Some(path), Some(history)) = (self.record_path.clone(), recent::history(cx)) else {
            return;
        };
        if !letter.is_ascii_lowercase() || self.pages.is_empty() {
            return;
        }
        let generation = self.document_generation;
        let work = history.marks(path.clone(), history.epoch());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != generation
                    || this.record_path.as_ref() != Some(&path)
                {
                    return;
                }
                match result {
                    Ok(marks) => match marks.get(&letter).map(|mark| mark.position) {
                        Some(position)
                            if position.page < this.pages.len()
                                && position.y.is_finite()
                                && position.y >= 0.0 =>
                        {
                            this.position_touched = true;
                            this.pending_restore = None;
                            this.restore_position(position);
                            this.sync_viewport(cx);
                        }
                        Some(_) => this.status = format!("Local mark {letter} is outside this PDF"),
                        None => this.status = format!("Local mark {letter} is not set"),
                    },
                    Err(error) => this.status = format!("Local mark could not be loaded: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_some() {
            return;
        }
        self.palette = None;
        let command = cx.new(|cx| CommandState::new(window, cx));
        self.recent = Some(RecentPanel {
            command: command.clone(),
            files: Vec::new(),
            error: None,
            loading: true,
        });
        self.search_recent(String::new(), cx);
        let owner = cx.weak_entity();
        let initial_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if initial_focus.replace(false) {
                command.update(cx, |state, cx| state.focus(window, cx));
            }
            let title_command = command.clone();
            let current = owner
                .upgrade()
                .and_then(|reader| reader.read(cx).recent.clone());
            let dialog_owner = owner.clone();
            let content_owner = owner.clone();
            dialog
                .title(
                    div()
                        .id("recent-header")
                        .test_support()
                        .px_3()
                        .pt_3()
                        .pb_2()
                        .on_click(move |_, window, cx| {
                            title_command.update(cx, |state, cx| state.focus(window, cx));
                        })
                        .child("Open Recent"),
                )
                .close_button(false)
                .centered()
                .entrance(DialogEntrance::None)
                .p_0()
                .w(px(544.)) // Dialog's width API accepts Pixels, not rem-based lengths.
                .on_close(move |_, _, cx| {
                    let _ = dialog_owner.update(cx, |reader, cx| {
                        reader.recent_revision += 1;
                        reader.recent = None;
                        cx.notify();
                    });
                })
                .content(move |content, _, cx| {
                    content.child(
                        current
                            .clone()
                            .map(|panel| {
                                recent_content(&panel, content_owner.clone(), cx).into_any_element()
                            })
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
        });
        cx.notify();
    }

    fn dismiss_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.recent_revision += 1;
        self.recent = None;
        window.close_dialog(cx);
        cx.notify();
    }

    fn search_recent(&mut self, query: String, cx: &mut Context<Self>) {
        self.recent_revision += 1;
        let revision = self.recent_revision;
        let Some(panel) = self.recent.as_mut() else {
            return;
        };
        panel.loading = true;
        panel.error = None;
        let Some(history) = recent::history(cx) else {
            panel.error = Some("Local reading history is unavailable.".into());
            panel.loading = false;
            cx.notify();
            return;
        };
        let excluded = self.record_path.clone();
        let work = history.search_excluding(query, excluded);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.recent_revision != revision {
                    return;
                }
                if let Some(panel) = this.recent.as_mut() {
                    panel.loading = false;
                    match result {
                        Ok(files) => panel.files = files,
                        Err(error) => {
                            panel.files.clear();
                            panel.error = Some(format!(
                                "{error}. Check the local data directory and try again."
                            ));
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_recent(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .recent
            .as_ref()
            .and_then(|panel| panel.files.get(index))
            .map(|file| file.path.clone())
        else {
            return;
        };
        self.open_path(path.clone(), cx);
        self.recent_open = Some(self.generation);
        if let Some(panel) = self.recent.as_mut() {
            panel.error = None;
            panel.loading = true;
        }
        cx.notify();
    }

    fn remove_recent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self
            .recent
            .as_ref()
            .is_none_or(|panel| panel.loading || !panel.files.iter().any(|file| file.path == path))
        {
            return;
        }
        let Some(history) = recent::history(cx) else {
            return;
        };
        if let Some(panel) = self.recent.as_mut() {
            panel.loading = true;
        }
        cx.notify();
        let work = history.remove(path.clone());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(panel) = this.recent.as_mut() {
                    panel.loading = false;
                    match result {
                        Ok(_) => {
                            panel.files.retain(|file| file.path != path);
                            panel.error = None;
                            recent::notify_changed(cx);
                        }
                        Err(error) => {
                            panel.error = Some(format!("Cannot remove saved data: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
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
        if panel == SidebarPanel::Contents && !self.outline_in_flight && !sidebar.has_outline() {
            self.load_outline(cx);
        }
    }

    pub(crate) fn find_query_changed(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(sidebar) = self.sidebar.as_mut() else {
            return;
        };
        sidebar.find_mut().clear_results();
        if !query.trim().is_empty() && !self.pages.is_empty() {
            sidebar.find_mut().searching = true;
            self.search_next_batch(query, cx);
        }
        cx.notify();
    }

    fn search_next_batch(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(find) = self.sidebar.as_ref().map(ReaderSidebar::find) else {
            return;
        };
        if !find.searching || find.scanned >= self.pages.len() || find.results.len() >= MAX_MATCHES
        {
            return;
        }
        let start = find.scanned;
        let end = (start + 4).min(self.pages.len());
        let revision = find.revision;
        let document = self.document_generation;
        let Some(path) = self.path.clone() else {
            return;
        };
        let worker_query = query.clone();
        let work = cx.background_executor().spawn(async move {
            pdf::page_text_batch(&path, start..end)
                .into_iter()
                .enumerate()
                .map(|(offset, page)| match page {
                    Ok(text) => {
                        let mapped = !text.glyphs.is_empty();
                        (
                            mapped,
                            find_search::matches_on_page(&text, start + offset, &worker_query),
                            false,
                        )
                    }
                    Err(_) => (false, Vec::new(), true),
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let batch = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != document {
                    return;
                }
                let Some(find) = this.sidebar.as_mut().map(ReaderSidebar::find_mut) else {
                    return;
                };
                if find.revision != revision {
                    return;
                }
                for (mapped, hits, failed) in batch {
                    find.scanned += 1;
                    find.mapped_pages += usize::from(mapped);
                    find.failed_pages += usize::from(failed);
                    let remaining = MAX_MATCHES.saturating_sub(find.results.len());
                    if hits.len() > remaining {
                        find.truncated = true;
                    }
                    find.results.extend(hits.into_iter().take(remaining));
                }
                let activate_first = find.pending_next && !find.results.is_empty();
                if activate_first {
                    find.pending_next = false;
                }
                if find.scanned >= this.pages.len() || find.results.len() >= MAX_MATCHES {
                    find.searching = false;
                    find.truncated |= find.scanned < this.pages.len();
                } else {
                    this.search_next_batch(query, cx);
                }
                if activate_first {
                    this.select_match(0, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn select_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(hit) = self
            .sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.find().results.get(index))
            .cloned()
        else {
            return;
        };
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
        let y = find_search::page_rect(&hit, self.pages[hit.page]).1;
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.page_top(hit.page) + y * self.scale - height * 0.25,
        );
        if let Some(sidebar) = self.sidebar.as_mut() {
            let find = sidebar.find_mut();
            find.active = Some(index);
            find.pulse = find.pulse.wrapping_add(1);
        }
        self.sync_viewport(cx);
        cx.notify();
    }

    fn step_match(&mut self, direction: i32, cx: &mut Context<Self>) {
        let Some(find) = self.sidebar.as_ref().map(ReaderSidebar::find) else {
            return;
        };
        if find.results.is_empty() {
            // Enter before the first batch completes should still select the
            // first result as soon as it is discovered.
            if direction > 0
                && find.searching
                && let Some(sidebar) = self.sidebar.as_mut()
            {
                sidebar.find_mut().pending_next = true;
            }
            return;
        }
        let next = match (find.active, direction) {
            (Some(0), -1) | (None, -1) => find.results.len() - 1,
            (Some(index), -1) => index - 1,
            (Some(index), _) => (index + 1) % find.results.len(),
            (None, _) => 0,
        };
        self.select_match(next, cx);
    }

    pub(crate) fn next_find_match(&mut self, cx: &mut Context<Self>) {
        self.step_match(1, cx);
    }

    fn close_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sidebar) = self.sidebar.as_mut()
            && sidebar.active().is_some()
        {
            sidebar.close(window, &self.focus, cx);
        }
    }

    pub(crate) fn resize_sidebar(
        &mut self,
        position: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sidebar) = self.sidebar.as_mut() {
            sidebar.resize(position, window);
            cx.notify();
        }
    }

    pub(crate) fn reset_sidebar_width(&mut self, cx: &mut Context<Self>) {
        if let Some(sidebar) = self.sidebar.as_mut() {
            sidebar.reset_width();
            cx.notify();
        }
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_panel().is_some() {
            self.close_sidebar(window, cx);
        } else if !self.pages.is_empty() {
            let panel = self
                .sidebar
                .as_ref()
                .map_or(SidebarPanel::default(), ReaderSidebar::last);
            self.show_sidebar(panel, window, cx);
        }
    }

    pub fn sidebar_panel(&self) -> Option<SidebarPanel> {
        self.sidebar.as_ref().and_then(ReaderSidebar::active)
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
            let dismiss = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return false;
                }
                let mut dismiss = false;
                match result {
                    Ok(pages) if !pages.is_empty() => {
                        if let Ok(Some(mode)) = saved_mode {
                            this.reading_mode = match mode {
                                ReadingView::Continuous => ReadingMode::Continuous,
                                ReadingView::Page => ReadingMode::SinglePage,
                            };
                            this.mode_override = Some(this.reading_mode);
                        }
                        self::Preview::install_pages(this, pages, cx);
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
                        if this.recent_open == Some(generation) {
                            this.recent_open = None;
                            dismiss = true;
                        }
                    }
                    Ok(_) => this.status = "PDF has no pages".into(),
                    Err(error) => this.status = error,
                }
                if let Some(opened) = this.recent_open.take()
                    && opened == generation
                    && let Some(panel) = this.recent.as_mut()
                {
                    panel.error = Some(format!("Could not open this PDF: {}. Use the X on its row to forget its record without deleting the PDF.", this.status));
                    panel.loading = false;
                }
                cx.notify();
                dismiss
            });
            if matches!(dismiss, Ok(true)) {
                let _ = this.update_in(cx, |this, window, cx| this.dismiss_recent(window, cx));
            }
        })
        .detach();
    }

    fn install_pages(&mut self, pages: Vec<(f32, f32)>, cx: &mut Context<Self>) {
        self.pages = pages;
        self.selected_page = Some(0);
        self.status = self.ready_status();
        if self.sidebar_panel() == Some(SidebarPanel::Contents) {
            self.load_outline(cx);
        }
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
            self.page_top(position.page) + y * self.scale,
        );
    }

    fn load_outline(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        self.outline_in_flight = true;
        let generation = self.document_generation;
        let work = cx
            .background_executor()
            .spawn(async move { pdf::document_outline(&path) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if generation != this.document_generation {
                    return;
                }
                this.outline_in_flight = false;
                if let Some(sidebar) = this.sidebar.as_mut() {
                    sidebar.install_outline(result, cx);
                    // Tree's focus handle is tracked by the newly rendered tree.
                    // Transfer focus after that render, not before it exists.
                    cx.on_next_frame(window, |this, window, cx| {
                        if let Some(sidebar) = &this.sidebar {
                            sidebar.focus_loaded_outline(window, cx);
                        }
                    });
                }
                cx.notify();
            });
        })
        .detach();
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
        let position = if self.pages.is_empty() {
            Position { page: 0, y: 0.0 }
        } else {
            self.position_at(top)
        };
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

    fn page_top(&self, index: usize) -> f32 {
        match self.reading_mode {
            ReadingMode::Continuous => page_top(&self.pages, self.scale, index),
            ReadingMode::SinglePage => 0.0,
        }
    }

    fn position_at(&self, top: f32) -> Position {
        match self.reading_mode {
            ReadingMode::Continuous => position_at(&self.pages, self.scale, top),
            ReadingMode::SinglePage => Position {
                page: self.page_index,
                y: (top / self.scale).clamp(0.0, self.pages[self.page_index].1),
            },
        }
    }

    fn visible_pages(&self, top: f32, height: f32) -> std::ops::Range<usize> {
        match self.reading_mode {
            ReadingMode::Continuous => visible_range(&self.pages, self.scale, top, height),
            ReadingMode::SinglePage => self.page_index..self.page_index + 1,
        }
    }

    fn maximum_scroll(&self, height: f32) -> f32 {
        match self.reading_mode {
            ReadingMode::Continuous => {
                (page_top(&self.pages, self.scale, self.pages.len()) - height).max(0.0)
            }
            ReadingMode::SinglePage => {
                (self.pages[self.page_index].1 * self.scale + GAP + PAGE_PADDING * 2.0 - height)
                    .max(0.0)
            }
        }
    }

    fn scroll_to(&mut self, x: f32, top: f32) {
        let maximum = self.maximum_scroll(self.viewport().2);
        self.scroll
            .set_offset(point(px(-x.max(0.0)), px(-top.clamp(0.0, maximum))));
    }

    fn focus_page(&mut self, page: usize) {
        if self.reading_mode == ReadingMode::SinglePage && page != self.page_index {
            self.selection = None;
            self.dragging = false;
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
        let position = self.position_at(top);
        let page = if mode == ReadingMode::SinglePage && self.selected_page.is_none() {
            current_page_at(&self.pages, self.scale, top, height)
        } else {
            self.page_index
        };
        self.reading_mode = mode;
        if mode == ReadingMode::SinglePage
            && self
                .selection
                .is_some_and(|selection| selection.start.page != page)
        {
            self.selection = None;
            self.dragging = false;
        }
        self.focus_page(page);
        let y = if position.page == page {
            position.y
        } else {
            0.0
        };
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.page_top(page) + y * self.scale,
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
                + self.page_top(index) as f64,
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
        let mut visible = self.visible_pages(top, height);
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
        let visible = self.visible_pages(top, height);
        if self
            .selected_page
            .is_some_and(|page| !visible.contains(&page))
        {
            self.selected_page = None;
        }
        let target = if self.reading_mode == ReadingMode::SinglePage {
            self.page_index
        } else {
            self.selected_page
                .unwrap_or_else(|| current_page_at(&self.pages, self.scale, top, height))
        };
        // When a scroll changes the fit target, keep the point at the
        // viewport center stable. Anchoring the top instead can move the
        // center back across the boundary and refit the previous page.
        let anchor = if self.selected_page.is_none() && target != self.page_index {
            height / 2.0
        } else {
            0.0
        };
        self.page_index = target;
        let position = self.position_at(top + anchor);
        self.apply_fit(self.page_index, position, (width, height), anchor);
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

    /// Explicit page turns remain the reading place even when short pages
    /// share a viewport and the scroll offset cannot reach their top.
    fn position_to_save(&self) -> ReadingPosition {
        let (visible_page, visible_y) = self.reading_position();
        if let Some(page) = self.selected_page
            && page != visible_page
        {
            return ReadingPosition { page, y: 0.0 };
        }
        // A short final page can be current even when the viewport's top is
        // stuck on the preceding page. Preserve the current page in that case.
        if self.reading_mode == ReadingMode::Continuous
            && self.selected_page.is_none()
            && self.page_index == self.pages.len() - 1
            && self.page_index != visible_page
            && self.maximum_scroll(self.viewport().2) < self.page_top(self.page_index)
        {
            return ReadingPosition {
                page: self.page_index,
                y: 0.0,
            };
        }
        ReadingPosition {
            page: visible_page,
            y: visible_y,
        }
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
        self.scroll_to(
            -f32::from(self.scroll.offset().x),
            self.page_top(position.page) + position.y * next - anchor,
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
            self.page_top(index) + y * self.scale,
        );
        self.record_position(cx);
        self.schedule(cx);
        self.schedule_text(cx);
        cx.notify();
    }
    pub(crate) fn select_outline(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destination = self
            .sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.outline_destination(index));
        if let Some(destination) = destination {
            self.navigate_outline(destination, cx);
        }
    }

    fn navigate_outline(&mut self, destination: pdf::OutlineDestination, cx: &mut Context<Self>) {
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
        self.scroll_to(x, self.page_top(page) + y * self.scale);
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
                    .unwrap_or_else(|| current_page_at(&self.pages, self.scale, top, height))
            };
            let position = self.position_at(top);
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
        let position = self.position_at(top);
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
        let range = self.visible_pages(top, height);
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
            .min_by_key(|index| (self.page_top(*index) - top).abs() as i64);
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
                if this.visible_pages(top, height).contains(&index) {
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
        let settings = settings_ui::state(cx);
        let show_toolbar = settings.read(cx).show_toolbar();
        let show_statusbar = settings.read(cx).show_statusbar();
        if self.pending_find_reset {
            self.pending_find_reset = false;
            // InputState::set_value needs a Window. Defer the one-shot reset
            // until after this render, without changing open_path's document API.
            cx.defer_in(window, |this, window, cx| {
                if let Some(sidebar) = this.sidebar.as_ref() {
                    sidebar.find().reset_query(window, cx);
                    cx.notify();
                }
            });
        }
        if !self.focus_initialized {
            window.focus(&self.focus, cx);
            cx.observe_window_appearance(window, |_, window, cx| {
                if settings_ui::state(cx).read(cx).appearance()
                    == crate::settings::Appearance::System
                {
                    Theme::sync_system_appearance(Some(window), cx);
                }
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
        let range = if !self.pages.is_empty() {
            self.visible_pages(top, height)
        } else {
            0..0
        };
        let has_document = !self.pages.is_empty();
        let show_fit_labels = f32::from(window.bounds().size.width)
            >= FIT_LABEL_MIN_REMS * f32::from(window.rem_size());
        let filename = self
            .path
            .as_deref()
            .map(document_name)
            .map(gpui::SharedString::from);
        let top_space = if self.reading_mode == ReadingMode::SinglePage {
            0.0
        } else {
            page_top(&self.pages, self.scale, range.start)
        };
        let end_space = if self.reading_mode == ReadingMode::SinglePage {
            0.0
        } else {
            page_top(&self.pages, self.scale, self.pages.len())
                - page_top(&self.pages, self.scale, range.end)
        };
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
            if let Some((hit, pulse)) = self
                .sidebar
                .as_ref()
                .and_then(|sidebar| {
                    let find = sidebar.find();
                    find.active
                        .and_then(|active| find.results.get(active))
                        .map(|hit| (hit, find.pulse))
                })
                .filter(|(hit, _)| hit.page == index)
            {
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
                self.palette.is_none() && self.marks_panel.is_none() && self.recent.is_none() && self.focus.is_focused(window),
                |reader| reader.key_context("Reader"),
            )
            .on_action(cx.listener(Self::on_open_pdf))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(cx.listener(|_, _: &NextWindow, window, cx| {
                reader_windows::cycle(window, cx, 1);
            }))
            .on_action(cx.listener(|_, _: &PreviousWindow, window, cx| {
                reader_windows::cycle(window, cx, -1);
            }))
            .on_action(cx.listener(Self::on_show_palette))
            .on_action(cx.listener(|this, _: &ShowMarks, window, cx| this.show_marks(MarksView::List, window, cx)))
            .on_action(cx.listener(|this, _: &ShowSetMark, window, cx| this.show_marks(MarksView::Set, window, cx)))
            .on_action(cx.listener(|this, _: &ShowJumpMark, window, cx| this.show_marks(MarksView::Jump, window, cx)))
            .on_action(cx.listener(|this, _: &ShowDeleteMark, window, cx| this.show_marks(MarksView::Delete, window, cx)))
            .on_action(cx.listener(|this, action: &SetLocalMark, _, cx| {
                if !this.pages.is_empty() {
                    this.change_mark(action.0, Some(this.position_to_save()), cx);
                }
            }))
            .on_action(cx.listener(|this, action: &JumpLocalMark, _, cx| this.jump_mark(action.0, cx)))
            .on_action(cx.listener(|this, action: &DeleteLocalMark, _, cx| this.change_mark(action.0, None, cx)))
            .on_action(cx.listener(|this, _: &ShowRecent, window, cx| {
                this.show_recent(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowFind, window, cx| {
                if !this.pages.is_empty() {
                    this.show_sidebar(SidebarPanel::Find, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextMatch, _, cx| this.step_match(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousMatch, _, cx| this.step_match(-1, cx)))
            .on_action(cx.listener(|this, _: &ShowContents, window, cx| {
                if !this.pages.is_empty() {
                    this.show_sidebar(SidebarPanel::Contents, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.toggle_sidebar(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ActivateOutline, _, cx| {
                if let Some(dest) = this
                    .sidebar
                    .as_ref()
                    .and_then(|sidebar| sidebar.selected_outline(cx))
                {
                    this.navigate_outline(dest, cx);
                }
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
                if this.palette.is_some() || this.marks_panel.is_some() || this.recent.is_some() || !this.focus.is_focused(window)
                {
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
                        |row, sidebar| row.child(sidebar.render(window, self.pages.len(), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .children(show_toolbar.then(|| {
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
                                        reader_button(
                                            "reading-mode",
                                            match (self.reading_mode, self.mode_override) {
                                                (ReadingMode::SinglePage, Some(_)) => "Single page for this PDF · switch to continuous",
                                                (ReadingMode::SinglePage, None) => "Single page (default) · switch to continuous",
                                                (ReadingMode::Continuous, Some(_)) => "Continuous for this PDF · switch to single page",
                                                (ReadingMode::Continuous, None) => "Continuous (default) · switch to single page",
                                            },
                                            Some(IconName::FileText),
                                            if self.reading_mode == ReadingMode::SinglePage {
                                                Box::new(Continuous)
                                            } else {
                                                Box::new(SinglePage)
                                            },
                                            has_document,
                                            self.reading_mode == ReadingMode::SinglePage,
                                        )
                                        .small()
                                        .when(
                                            show_fit_labels,
                                            |button| {
                                                button.label(
                                                    if self.reading_mode == ReadingMode::SinglePage
                                                    {
                                                        "Single page"
                                                    } else {
                                                        "Continuous"
                                                    },
                                                )
                                            },
                                        ),
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
                                    .content(
                                        reader_button(
                                            "settings-button",
                                            "Settings…",
                                            Some(IconName::Settings),
                                            Box::new(ShowSettings),
                                            true,
                                            false,
                                        )
                                        .small(),
                                    )
                            }))
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
                            .children(show_statusbar.then(|| {
                                StatusBar::new()
                                    .left(
                                        div()
                                            .id("reader-status")
                                            .test_support()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .child(if has_document {
                                                format!(
                                                    "PAGE {} / {}",
                                                    self.page_index + 1,
                                                    self.pages.len()
                                                )
                                            } else {
                                                self.status.clone()
                                            }),
                                    )
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
                                    })
                            })),
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
                                                .label("Open Recent…")
                                                .action(Box::new(ShowRecent)),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Open marks…")
                                                .action(Box::new(ShowMarks))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Set local mark…")
                                                .action(Box::new(ShowSetMark))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Jump to local mark…")
                                                .action(Box::new(ShowJumpMark))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Delete local mark…")
                                                .action(Box::new(ShowDeleteMark))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Settings…")
                                                .action(Box::new(ShowSettings)),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Find")
                                                .action(Box::new(ShowFind))
                                                .disabled(!has_document),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Next match")
                                                .action(Box::new(NextMatch))
                                                .disabled(
                                                    self.sidebar.as_ref().is_none_or(|s| {
                                                        s.find().results.is_empty()
                                                    }),
                                                ),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Previous match")
                                                .action(Box::new(PreviousMatch))
                                                .disabled(
                                                    self.sidebar.as_ref().is_none_or(|s| {
                                                        s.find().results.is_empty()
                                                    }),
                                                ),
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
                                                .label("Single page")
                                                .action(Box::new(SinglePage))
                                                .disabled(
                                                    !has_document
                                                        || self.mode_override == Some(ReadingMode::SinglePage),
                                                ),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Continuous")
                                                .action(Box::new(Continuous))
                                                .disabled(
                                                    !has_document
                                                        || self.mode_override == Some(ReadingMode::Continuous),
                                                ),
                                        )
                                        .item(
                                            CommandItem::new()
                                                .label("Use default reading mode")
                                                .action(Box::new(UseDefaultReadingMode))
                                                .disabled(!has_document || self.mode_override.is_none()),
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
                                                    if reader.palette.is_some() {
                                                        reader.dismiss_palette(window, cx);
                                                    }
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
            .when_some(self.marks_panel.clone(), |reader, panel| {
                let owner = cx.weak_entity();
                let rows: Vec<_> = match panel.view {
                    MarksView::Set => ('a'..='z')
                        .map(|letter| (letter, panel.marks.get(&letter).cloned())).collect(),
                    MarksView::Name(_) => Vec::new(),
                    _ => panel.marks.iter().map(|(&letter, mark)| (letter, Some(mark.clone()))).collect(),
                };
                let mut entries = Vec::new();
                if panel.view == MarksView::List {
                    entries.push(CommandItem::new().label("Set local mark…").keywords(["new mark"]));
                }
                for (letter, mark) in &rows {
                    let letter = *letter;
                    let label = match mark {
                        Some(mark) => format!("{letter}    {}Page {} · {:.0} pt from top",
                            mark.label.as_ref().map(|label| format!("{label} · ")).unwrap_or_default(),
                            mark.position.page + 1, mark.position.y),
                        None => format!("{letter}    Unset"),
                    };
                    let mut entry = CommandItem::new().label(label.clone()).keywords([letter.to_string()]);
                    if panel.view == MarksView::List {
                        let owner = owner.clone();
                        let has_name = mark.as_ref().and_then(|mark| mark.label.as_ref()).is_some();
                        entry = entry.child(move |_, _| {
                            let owner = owner.clone();
                            div().w_full().flex().items_center().justify_between().gap_2()
                                .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                                .child(div().flex().flex_shrink_0().items_center().gap_1()
                                    .child(Button::new(format!("name-mark:{letter}"))
                                        .icon(IconName::Pencil)
                                        .accessibility_label(format!("{} local mark {letter}", if has_name { "Rename" } else { "Name" }))
                                        .tooltip(if has_name { "Rename mark" } else { "Name mark" })
                                        .ghost().xsmall()
                                        .on_click({
                                            let owner = owner.clone();
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                let _ = owner.update(cx, |reader, cx| reader.edit_mark_name(letter, window, cx));
                                            }
                                        }))
                                    .child(Button::new(format!("delete-mark:{letter}"))
                                        .icon(IconName::Close)
                                        .accessibility_label(format!("Delete local mark {letter}"))
                                        .tooltip("Delete mark")
                                        .ghost().xsmall()
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            let _ = owner.update(cx, |reader, cx| reader.change_mark(letter, None, cx));
                                        })))
                        });
                    }
                    entries.push(entry);
                }
                let title = match panel.view {
                    MarksView::List => "Local marks",
                    MarksView::Set => "Set local mark",
                    MarksView::Jump => "Jump to local mark",
                    MarksView::Delete => "Delete local mark",
                    MarksView::Name(_) => "Name local mark",
                };
                reader
                    .child(div().absolute().inset_0().bg(cx.theme().overlay))
                    .child(div().id("marks-panel").test_support().role(gpui::Role::Dialog)
                        .aria_label(title)
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape"
                                && let Some(panel) = this.marks_panel.as_ref()
                                && matches!(panel.view, MarksView::Name(_)) {
                                if !panel.loading {
                                    this.cancel_mark_name(window, cx);
                                }
                            } else {
                                cx.propagate();
                            }
                        }))
                        .absolute().inset_0().flex().items_center()
                        .justify_center().p_4()
                        .child(div().w_full().max_w(gpui::rems(28.)).bg(cx.theme().popover)
                            .child(div().px_3().pt_3().pb_2().child(title))
                            .when(panel.name_input.is_none(), {
                                let command_owner = owner.clone();
                                |content| content.child(Command::new(&panel.command)
                                .placeholder("Search marks…")
                                .items(entries)
                                .empty(|_, _, cx| div().p_4()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("No local marks match. Set one with m then a letter."))
                                .on_confirm({
                                    let owner = command_owner.clone();
                                    move |index, window, cx| {
                                        if panel.loading { return; }
                                        let view = panel.view;
                                        let letter = if view == MarksView::List && index.row == 0 {
                                            None
                                        } else {
                                            rows.get(index.row - usize::from(view == MarksView::List)).map(|(letter, _)| *letter)
                                        };
                                        let _ = owner.update(cx, |reader, cx| {
                                            match letter {
                                                None if view == MarksView::List => reader.show_marks(MarksView::Set, window, cx),
                                                Some(letter) => {
                                                    reader.dismiss_marks(window, cx);
                                                    let action: Box<dyn gpui::Action> = match view {
                                                        MarksView::Set => Box::new(SetLocalMark(letter)),
                                                        MarksView::Delete => Box::new(DeleteLocalMark(letter)),
                                                        MarksView::Jump | MarksView::List => Box::new(JumpLocalMark(letter)),
                                                        MarksView::Name(_) => return,
                                                    };
                                                    window.dispatch_action(action, cx);
                                                }
                                                None => {}
                                            }
                                        });
                                    }
                                })
                                .on_cancel(move |window, cx| {
                                    let _ = command_owner.update(cx, |reader, cx| {
                                        if reader.marks_panel.as_ref().is_some_and(|panel| matches!(panel.view, MarksView::Name(_))) {
                                            return;
                                        }
                                        reader.dismiss_marks(window, cx);
                                    });
                                }))
                            })
                            .when_some(panel.name_input.clone(), |content, input| {
                                let MarksView::Name(letter) = panel.view else { return content; };
                                content.child(div().px_3().pb_3().flex().flex_col().gap_3()
                                    .child(div().text_color(cx.theme().muted_foreground)
                                        .child(format!("Mark {letter} · Leave blank to remove its name")))
                                    .child(Input::new(&input))
                                    .child(div().flex().justify_end().gap_2()
                                        .child(Button::new("cancel-mark-name").label("Cancel").ghost()
                                            .disabled(panel.loading)
                                            .on_click({
                                                let owner = owner.clone();
                                                move |_, window, cx| {
                                                    let _ = owner.update(cx, |reader, cx| {
                                                        reader.cancel_mark_name(window, cx);
                                                    });
                                                }
                                            }))
                                        .child(Button::new("save-mark-name").label("Save").primary()
                                            .disabled(panel.loading)
                                            .on_click({
                                                let owner = owner.clone();
                                                move |_, _, cx| {
                                                    let _ = owner.update(cx, |reader, cx| reader.save_mark_name(letter, cx));
                                                }
                                            }))))
                            })
                            .when(panel.loading, |content| content.child(div().px_3().py_2().child("Loading marks…")))
                            .when_some(panel.error, |content, error| content.child(div().px_3().py_2()
                                .text_color(cx.theme().danger).child(error))))
                        .focus_trap("reader-marks-palette", &self.palette_focus))
            })
    }
}

impl Preview {
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
    use gpui_kit::{
        AnyWindowHandle, Focusable as _, TestAppContext, point, size,
        test::{TestEventExt as _, TestWindowExt},
    };

    fn recent_test_window(
        cx: &mut TestAppContext,
        path: Option<PathBuf>,
    ) -> (AnyWindowHandle, Entity<Preview>) {
        cx.update(|cx| {
            gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Preview::new(path, window.scale_factor(), cx))
            })
            .unwrap()
        })
    }

    fn mark(position: ReadingPosition) -> LocalMark {
        LocalMark {
            position,
            label: None,
        }
    }

    #[gpui_kit::test]
    fn mark_letter_overrides_reader_page_shortcut(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("book.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reader.update(cx, |reader, cx| reader.select_page(1, cx));
        cx.simulate_keystrokes(window, "m");
        cx.simulate_keystrokes(window, "p");
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        let original = reader.read_with(cx, |reader, _| reader.position_to_save());
        for letter in ['c', 'f', 'j', 'k', 'n', 's', 'w'] {
            cx.simulate_keystrokes(window, &format!("m {letter}"));
            reader.read_with(cx, |reader, _| {
                assert_eq!(
                    reader.position_to_save(),
                    original,
                    "reader shortcut ran for {letter}"
                );
                assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
            });
        }
        cx.run_until_parked();
        let store = RecentStore::new(&paths);
        for letter in ['c', 'f', 'j', 'k', 'n', 'p', 's', 'w'] {
            assert_eq!(
                store
                    .marks(&pdf)
                    .unwrap()
                    .get(&letter)
                    .map(|mark| mark.position.page),
                Some(1),
                "mark {letter} was not set"
            );
        }
        reader.update(cx, |reader, cx| reader.select_page(2, cx));
        cx.simulate_keystrokes(window, "' p");
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        cx.simulate_keystrokes(window, "d m p");
        cx.run_until_parked();
        assert!(!store.marks(&pdf).unwrap().contains_key(&'p'));
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        // Once a sequence completes, the ordinary one-key binding is active again.
        cx.simulate_keystrokes(window, "p");
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
        reader.update(cx, |reader, cx| reader.select_page(2, cx));
        cx.simulate_keystrokes(window, "m escape p");
        reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        cx.simulate_keystrokes(window, "m cmd-f");
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.input("e", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(!store.marks(&pdf).unwrap().contains_key(&'e'));
    }

    #[gpui_kit::test]
    fn naming_a_local_mark_in_popup_survives_replacing_and_restart(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = root.path().join("named.pdf");
        std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
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
                frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
                frame.dispatch_action(Box::new(ShowMarks), cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("name-mark:a", cx);
            })
            .unwrap()
        });
        reader.read_with(cx, |reader, _| {
            assert!(
                reader.marks_panel.is_some(),
                "name button closed popup synchronously: {}",
                reader.status
            );
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.marks_panel.as_ref().unwrap().view,
                MarksView::Name('a')
            )
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| frame.press("escape", cx))
                .unwrap()
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.marks_panel.as_ref().unwrap().view, MarksView::List)
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("name-mark:a", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.input("Introduction", cx);
                frame.press("enter", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.marks_panel.as_ref().unwrap().view, MarksView::List)
        });
        let store = RecentStore::new(&paths);
        assert_eq!(
            store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
            Some("Introduction")
        );
        reader.update(cx, |reader, cx| reader.select_page(1, cx));
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        assert_eq!(
            store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
            Some("Introduction")
        );
        assert_eq!(store.marks(&pdf).unwrap()[&'a'].position.page, 1);
        cx.update(|cx| {
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        reopened.update(cx, |reader, cx| reader.jump_mark('a', cx));
        cx.run_until_parked();
        reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
        assert_eq!(
            store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
            Some("Introduction")
        );
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("name-mark:a", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.press("cmd-a", cx);
                frame.press("backspace", cx);
                frame.click("save-mark-name", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        assert_eq!(store.marks(&pdf).unwrap()[&'a'].label, None);
        reader.read_with(cx, |reader, _| {
            assert!(
                reader.marks_panel.is_some(),
                "saving name unexpectedly closed popup"
            );
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("delete-mark:a", cx);
            })
            .unwrap()
        });
        cx.run_until_parked();
        assert!(store.marks(&pdf).unwrap().is_empty());
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
            let panel = reader.marks_panel.as_ref().expect("marks popup opened");
            assert_eq!(panel.marks.get(&'a'), Some(&mark(saved)));
            assert_eq!(panel.command.read(cx).query(cx), "");
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
        other.read_with(cx, |reader, _| assert!(reader.marks_panel.is_none()));
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
        other.read_with(cx, |reader, cx| {
            let palette = reader.palette.as_ref().expect("command palette opened");
            assert_eq!(palette.read(cx).query(cx), "Open marks");
        });
        cx.update(|cx| {
            cx.update_window(other_window, |_, frame, cx| frame.press("enter", cx))
                .unwrap()
        });
        cx.run_until_parked();
        other.read_with(cx, |reader, _| assert!(reader.marks_panel.is_some()));
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
        empty.update(cx, |reader, cx| reader.open_recent(0, cx));
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
        let window = cx.add_window(|frame, cx| Preview::new(Some(path), frame.scale_factor(), cx));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(
                    reader.pages,
                    vec![(360.0, 300.0), (560.0, 1500.0), (940.0, 500.0)]
                );
            })
            .unwrap();

        cx.simulate_keystrokes(window.into(), "s");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
                assert_eq!(reader.visible_pages(0.0, 700.0), 0..1);
            })
            .unwrap();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
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
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 0);
                assert_eq!(reader.reading_position(), (0, 0.0));
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "right");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert_eq!(reader.reading_position().0, 1);
            })
            .unwrap();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                    cx,
                );
            })
            .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert!(reader.reading_position().1 > 0.0);
            })
            .unwrap();
        window
            .update(cx, |reader, _, cx| reader.scroll_by(10000.0, cx))
            .unwrap();
        let bottom = window
            .read_with(cx, |reader, _| reader.reading_position().1)
            .unwrap();
        assert!(bottom > 500.0);
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
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
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert!((reader.reading_position().1 - bottom).abs() < 1.0);
                assert_eq!(reader.visible_pages(0.0, 700.0), 1..2);
            })
            .unwrap();
        window
            .update(cx, |reader, _, cx| reader.zoom_in(cx))
            .unwrap();
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert!((reader.reading_position().1 - bottom).abs() < 1.0);
            })
            .unwrap();
        window
            .update(cx, |reader, _, cx| reader.fit_width(cx))
            .unwrap();
        let before_resize = window.read_with(cx, |reader, _| reader.scale()).unwrap();
        cx.simulate_window_resize(window.into(), size(px(600.0), px(700.0)));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert!(reader.scale() < before_resize);
                assert!(reader.reading_position().1 <= reader.pages[1].1);
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "right");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 2);
                assert_eq!(reader.reading_position().0, 2);
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "right");
        assert_eq!(
            window
                .read_with(cx, |reader, _| reader.page_index())
                .unwrap(),
            2
        );
        cx.simulate_keystrokes(window.into(), "left c");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
                assert_eq!(reader.reading_position().0, 1);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn single_page_find_jump_and_window_modes_are_local(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let first = cx.add_window(|frame, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                frame.scale_factor(),
                cx,
            )
        });
        let second = cx.add_window(|frame, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                frame.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        first
            .update(cx, |reader, frame, cx| {
                reader.set_reading_mode(ReadingMode::SinglePage, cx);
                reader.show_sidebar(SidebarPanel::Find, frame, cx);
                reader.find_query_changed("This is page 3 of 100".into(), cx);
            })
            .unwrap();
        cx.run_until_parked();
        first
            .update(cx, |reader, _, cx| {
                let index = reader
                    .sidebar
                    .as_ref()
                    .unwrap()
                    .find()
                    .results
                    .iter()
                    .position(|hit| hit.page == 2)
                    .expect("page 3 has a find result");
                reader.select_match(index, cx);
            })
            .unwrap();
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 2);
                assert_eq!(reader.reading_position().0, 2);
                assert_eq!(reader.visible_pages(0.0, 700.0), 2..3);
                assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            })
            .unwrap();
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 0);
                assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
            })
            .unwrap();
        first
            .update(cx, |reader, _, cx| {
                reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
            })
            .unwrap();
        cx.run_until_parked();
        let destination = pdf::document_outline(Path::new("examples/field-guide.pdf")).unwrap()[0]
            .destination()
            .unwrap();
        first
            .update(cx, |reader, _, cx| {
                // A new document starts with the configured default view.
                assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
                reader.set_reading_mode(ReadingMode::SinglePage, cx);
                reader.navigate_outline(destination, cx);
            })
            .unwrap();
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 2);
                assert_eq!(reader.reading_position().0, 2);
                assert_eq!(reader.visible_pages(0.0, 700.0), 2..3);
            })
            .unwrap();
        cx.run_until_parked();
        first
            .read_with(cx, |reader, _| {
                assert!(reader.images.keys().all(|page| *page == 2));
                assert!(reader.retained_bitmap_bytes() <= BITMAP_BUDGET);
            })
            .unwrap();
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
    fn reading_keys_navigate_and_zoom_without_screenshots(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let window = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        window
            .read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)))
            .unwrap();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
                .unwrap();
        });
        cx.simulate_keystrokes(window.into(), "right =");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert_eq!(reader.scale(), 1.25);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn reader_sidebar_opens_and_remembers_its_panel(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let window = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("toggle-sidebar", cx);
            })
            .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.sidebar_panel(), Some(SidebarPanel::Contents));
            })
            .unwrap();

        cx.simulate_keystrokes(window.into(), "cmd-f");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.sidebar_panel(), Some(SidebarPanel::Find));
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "cmd-b cmd-b");
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.sidebar_panel(), Some(SidebarPanel::Find));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn find_query_survives_closing_and_reopening_the_sidebar(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let window = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        cx.simulate_keystrokes(window.into(), "cmd-f");
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(frame.find("find-query").focused(), Some(true));
                frame.input("KVER", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.simulate_keystrokes(window.into(), "cmd-b cmd-b");
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(frame.find("find-query").value(), Some("KVER"));
                assert_eq!(frame.find("find-query").focused(), Some(true));
                assert!(
                    frame
                        .find("find-status")
                        .label()
                        .unwrap()
                        .contains("matches")
                );
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn reader_navigation_is_window_local(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let first = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        let second = cx.add_window(|window, cx| Preview::new(None, window.scale_factor(), cx));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
            cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        cx.simulate_keystrokes(first.into(), "right =");
        first
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_index(), 1);
                assert_eq!(reader.scale(), 1.25);
            })
            .unwrap();
        second
            .read_with(cx, |reader, _| {
                assert_eq!(reader.page_count(), None);
                assert_eq!(reader.page_index(), 0);
                assert_eq!(reader.scale(), 1.0);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn fresh_private_session_never_creates_reading_database(cx: &mut TestAppContext) {
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
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
        });
        let (first, _) = recent_test_window(cx, Some(pdf.clone()));
        let (second, second_reader) = recent_test_window(cx, None);
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(second, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        second_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.recent.as_ref().unwrap().files[0].path, pdf)
        });
        assert!(!paths.local_data().join("reading.sqlite3").exists());
        cx.update(|cx| {
            cx.update_window(first, |_, window, _| window.remove_window())
                .unwrap()
        });
    }

    #[gpui_kit::test]
    fn history_off_is_session_only_across_reader_windows_and_reenable_saves_it(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        let missing = root.path().join("offline-drive.pdf");
        store.record(&missing).unwrap();
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
            settings_ui::install_with_paths(cx, Some(paths.clone()));
        });
        let (_first, first_reader) = recent_test_window(cx, Some(pdf.clone()));
        let (second, second_reader) = recent_test_window(cx, None);
        cx.run_until_parked();
        assert!(!store.contains(&pdf).unwrap(), "off wrote to SQLite");
        assert!(store.contains(&missing).unwrap(), "off erased old data");
        cx.update(|cx| {
            cx.update_window(second, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        second_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.recent.as_ref().unwrap().files[0].path, pdf);
            assert_eq!(reader.recent.as_ref().unwrap().files[1].path, missing);
        });
        // Restart while recording is off: only the previously saved missing
        // path survives; a session-only open is never written implicitly.
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        cx.run_until_parked();
        assert_eq!(store.search("").unwrap().len(), 1);
        assert_eq!(store.search("").unwrap()[0].path, missing);
        // Reopen the PDF in this session before re-enabling recording.
        first_reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
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
        assert!(
            crate::settings::load(paths.config())
                .unwrap()
                .remember_reading_history()
        );
        assert!(
            store.contains(&pdf).unwrap(),
            "re-enabling did not flush the session"
        );
        first_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.path(), Some(pdf.as_path()))
        });
        second_reader.read_with(cx, |reader, _| assert_eq!(reader.path(), None));

        // Reinitializing the service against the same locations represents a
        // new process; only the saved state should be visible on disk.
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        cx.run_until_parked();
        assert_eq!(store.search("").unwrap()[0].path, pdf);
    }

    #[gpui_kit::test]
    fn forgetting_one_pdf_invalidates_open_recent_in_other_windows_and_does_not_resurrect(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        let missing = root.path().join("offline.pdf");
        store.record(&pdf).unwrap();
        store.record(&missing).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths.clone()));
        });
        let (first, first_reader) = recent_test_window(cx, None);
        let (second, second_reader) = recent_test_window(cx, Some(pdf.clone()));
        cx.run_until_parked();
        for window in [first, second] {
            cx.update(|cx| {
                cx.update_window(window, |_, frame, cx| {
                    frame.render_frame(cx);
                    frame.dispatch_action(Box::new(ShowRecent), cx);
                })
                .unwrap();
            });
        }
        cx.run_until_parked();
        first_reader.update(cx, |reader, cx| reader.remove_recent(missing.clone(), cx));
        cx.run_until_parked();
        assert!(!store.contains(&missing).unwrap());
        assert!(store.contains(&pdf).unwrap());
        second_reader.read_with(cx, |reader, _| {
            assert!(
                !reader
                    .recent
                    .as_ref()
                    .unwrap()
                    .files
                    .iter()
                    .any(|file| file.path == missing)
            );
            assert_eq!(reader.path(), Some(pdf.as_path()));
        });
        cx.update(|cx| recent::install(cx, Some(paths.clone())));
        cx.run_until_parked();
        assert!(!store.contains(&missing).unwrap());
    }

    #[gpui_kit::test]
    fn open_recent_excludes_the_active_pdf_without_changing_stored_history(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        let active = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        let older = root.path().join("older.pdf");
        store.record(&older).unwrap();
        store.record(&active).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (first, first_reader) = recent_test_window(cx, Some(active.clone()));
        let (second, second_reader) = recent_test_window(cx, None);
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(first, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            let files = &reader.recent.as_ref().unwrap().files;
            assert_eq!(
                files.iter().map(|file| &file.path).collect::<Vec<_>>(),
                vec![&older]
            );
        });
        cx.update(|cx| {
            cx.update_window(first, |_, frame, cx| {
                frame.input("readme-sample", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            assert!(reader.recent.as_ref().unwrap().files.is_empty());
        });
        assert!(store.contains(&active).unwrap());
        cx.update(|cx| {
            cx.update_window(second, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        second_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.recent.as_ref().unwrap().files[0].path, active);
        });
    }

    #[gpui_kit::test]
    fn command_palette_opens_recent_with_search_focus(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (window, reader) = recent_test_window(cx, None);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowCommandPalette), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.input("Open Recent", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, cx| {
            let palette = reader.palette.as_ref().unwrap().read(cx);
            assert_eq!(palette.query(cx), "Open Recent");
            assert!(palette.selected_index().is_some());
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
                .unwrap();
        });
        cx.run_until_parked();
        reader.read_with(cx, |reader, _| {
            assert!(reader.palette.is_none());
            assert!(reader.recent.is_some());
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                assert_eq!(frame.find("recent-panel").label(), Some("Open Recent"));
                frame.input("book", cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_eq!(
                reader.recent.as_ref().unwrap().command.read(cx).query(cx),
                "book"
            );
        });
    }

    #[gpui_kit::test]
    fn successful_open_records_normal_path_and_recent_reopens_in_active_window(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (_first, first_reader) = recent_test_window(cx, None);
        let (second, second_reader) = recent_test_window(cx, None);
        first_reader.update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
        });
        cx.run_until_parked();
        let normalized = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        assert!(store.contains(&normalized).unwrap());
        second_reader.read_with(cx, |reader, _| assert!(reader.path.is_none()));
        cx.update(|cx| {
            cx.update_window(second, |_, window, cx| {
                window.render_frame(cx);
                window.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(second, |_, window, cx| window.press("enter", cx))
                .unwrap();
        });
        cx.run_until_parked();
        second_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), Some(100));
            assert!(reader.recent.is_none());
        });
        first_reader.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)));
    }

    #[gpui_kit::test]
    fn recent_dialog_arrows_do_not_scroll_the_pdf(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        for name in ["field-guide", "selection-sample"] {
            store
                .record(
                    &RecentStore::normalize(&PathBuf::from(format!("examples/{name}.pdf")))
                        .unwrap(),
                )
                .unwrap();
        }
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (handle, reader) = recent_test_window(
            cx,
            Some(RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap()),
        );
        cx.run_until_parked();
        let before = reader.read_with(cx, |reader, _| reader.scroll.offset());
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| frame.press("down", cx))
                .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_eq!(reader.scroll.offset(), before);
            assert_eq!(
                reader
                    .recent
                    .as_ref()
                    .unwrap()
                    .command
                    .read(cx)
                    .selected_index()
                    .unwrap()
                    .row,
                1
            );
        });
    }

    #[gpui_kit::test]
    fn clicking_outside_recent_closes_it_and_restores_reader_focus(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        let store = RecentStore::new(&paths);
        store.record(&pdf).unwrap();
        store
            .record(&RecentStore::normalize(Path::new("examples/field-guide.pdf")).unwrap())
            .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (handle, reader) = cx.update(|cx| {
            gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.click("input-state", cx);
                let focus = reader
                    .read(cx)
                    .recent
                    .as_ref()
                    .unwrap()
                    .command
                    .read(cx)
                    .focus_handle(cx);
                assert!(
                    focus.is_focused(frame),
                    "clicking fresh search moved focus to the reader"
                );
            })
            .unwrap();
        });
        let before = reader.read_with(cx, |reader, cx| {
            reader
                .recent
                .as_ref()
                .unwrap()
                .command
                .read(cx)
                .selected_index()
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.click("recent-header", cx);
                frame.press("down", cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_ne!(
                reader
                    .recent
                    .as_ref()
                    .unwrap()
                    .command
                    .read(cx)
                    .selected_index(),
                before,
                "arrow keys must navigate recent rows after clicking the header"
            );
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| frame.input("read", cx))
                .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_eq!(
                reader.recent.as_ref().unwrap().command.read(cx).query(cx),
                "read",
                "clicking the panel header must not strand keyboard search"
            );
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                let outside = point(px(20.), px(200.));
                frame.pointer_down(outside, gpui::MouseButton::Left, cx);
                frame.pointer_up(outside, gpui::MouseButton::Left, cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert!(
                reader.recent.is_none(),
                "outside click should dismiss Open Recent"
            );
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                assert!(
                    reader.read(cx).focus.is_focused(frame),
                    "outside click should return focus to the reader"
                );
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.render_frame(cx);
                frame.press("escape", cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert!(reader.recent.is_none(), "Escape should dismiss Open Recent");
        });
    }

    #[gpui_kit::test]
    fn row_x_confirms_the_specific_pdf_without_opening_it(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        store.record(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (handle, reader) = cx.update(|cx| {
            gpui_kit::open_window(gpui::WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        let forget_id = format!("forget-pdf:{}", pdf.display());
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                assert_eq!(
                    frame.find(forget_id.clone()).label(),
                    Some("Forget readme-sample.pdf")
                );
                assert!(
                    frame
                        .find("recent-panel")
                        .bounds()
                        .contains(&frame.find(forget_id.clone()).bounds().center()),
                    "X is outside the recent overlay"
                );
                frame.click(forget_id.clone(), cx);
                assert!(reader.read(cx).recent.is_some(), "X click dismissed Recent");
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(frame.find("ok").label(), Some("Forget PDF"));
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, _| {
            assert!(reader.path.is_none(), "X must not open the PDF row");
        });
        assert!(
            store.contains(&pdf).unwrap(),
            "X alone must not delete data"
        );
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| frame.click("cancel", cx))
                .unwrap();
        });
        cx.run_until_parked();
        assert!(
            store.contains(&pdf).unwrap(),
            "Cancel must preserve the record"
        );
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.simulate_next_frame(cx);
                let focus = reader
                    .read(cx)
                    .recent
                    .as_ref()
                    .unwrap()
                    .command
                    .read(cx)
                    .focus_handle(cx);
                assert!(
                    focus.is_focused(frame),
                    "search not focused after confirmation"
                );
                frame.click("input-state", cx);
                assert!(
                    reader.read(cx).recent.is_some(),
                    "search click dismissed recent"
                );
                assert!(focus.is_focused(frame), "clicking search changed focus");
                frame.input("readme", cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_eq!(
                reader.recent.as_ref().unwrap().command.read(cx).query(cx),
                "readme",
                "cancelling Forget must return focus to recent-file search"
            );
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| frame.click(forget_id.clone(), cx))
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| frame.click("ok", cx))
                .unwrap();
        });
        cx.run_until_parked();
        assert!(!store.contains(&pdf).unwrap());
        assert!(pdf.exists(), "forgetting must not delete the PDF");
        cx.update(|cx| {
            cx.update_window(handle, |_, frame, cx| {
                frame.simulate_next_frame(cx);
                let focus = reader
                    .read(cx)
                    .recent
                    .as_ref()
                    .unwrap()
                    .command
                    .read(cx)
                    .focus_handle(cx);
                assert!(
                    focus.is_focused(frame),
                    "search lost focus after forgetting a row"
                );
                frame.input("x", cx);
            })
            .unwrap();
        });
        reader.read_with(cx, |reader, cx| {
            assert_eq!(
                reader.recent.as_ref().unwrap().command.read(cx).query(cx),
                "readmex",
                "confirming Forget must return focus even when its row disappears"
            );
        });
    }

    #[gpui_kit::test]
    fn recent_action_search_and_missing_removal_work_across_windows(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("local"),
        );
        let store = RecentStore::new(&paths);
        let missing = root.path().join("missing-older.pdf");
        store.record(&missing).unwrap();
        let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
        store.record(&pdf).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
            recent::install(cx, Some(paths));
        });
        let (first, first_reader) = recent_test_window(cx, None);
        let (_second, second_reader) = recent_test_window(cx, None);
        cx.update(|cx| {
            cx.update_window(first, |_, window, cx| {
                window.render_frame(cx);
                window.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            let panel = reader.recent.as_ref().unwrap();
            assert_eq!(panel.files.len(), 2);
            assert_eq!(panel.files[0].path, pdf);
        });
        second_reader.read_with(cx, |reader, _| {
            assert!(reader.recent.is_none());
            assert!(reader.path.is_none());
        });

        cx.update(|cx| {
            cx.update_window(first, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(frame.find("recent-panel").label(), Some("Open Recent"));
                frame.input("missing-older", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            assert_eq!(reader.recent.as_ref().unwrap().files[0].path, missing);
        });
        cx.update(|cx| {
            cx.update_window(first, |_, frame, cx| {
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            let panel = reader.recent.as_ref().unwrap();
            assert!(panel.error.as_ref().unwrap().contains("Could not open"));
            assert_eq!(reader.page_count(), None);
        });
        first_reader.update(cx, |reader, cx| reader.remove_recent(missing.clone(), cx));
        cx.run_until_parked();
        first_reader.read_with(cx, |reader, _| {
            let panel = reader.recent.as_ref().unwrap();
            assert!(!panel.loading, "removal still pending");
            assert!(panel.error.is_none(), "removal failed: {:?}", panel.error);
        });
        assert!(!store.contains(&missing).unwrap());
        assert!(store.contains(&pdf).unwrap());
        second_reader.read_with(cx, |reader, _| assert!(reader.path.is_none()));
    }

    #[gpui_kit::test]
    fn fit_width_tracks_window_resize_and_manual_zoom_exits_fit(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let window = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        window
            .update(cx, |reader, _, cx| reader.fit_width(cx))
            .unwrap();
        let before = window.read_with(cx, |reader, _| reader.scale()).unwrap();
        assert!(before > 0.25);

        cx.simulate_window_resize(window.into(), size(px(600.0), px(700.0)));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        let after = window.read_with(cx, |reader, _| reader.scale()).unwrap();
        assert!(after < before, "a narrower viewport must reduce fit width");
        window
            .update(cx, |reader, _, cx| reader.zoom_in(cx))
            .unwrap();
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
                assert!((reader.scale() - after * 1.25).abs() < 0.01);
            })
            .unwrap();
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
    fn fit_page_scrolling_does_not_jump_back_across_a_page_boundary(cx: &mut TestAppContext) {
        let path = PathBuf::from("target/test-fixtures/fit-scroll-regression.pdf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, three_page_pdf([(384, 504), (378, 288), (352, 294)])).unwrap();
        cx.update(gpui_kit::init);
        let window =
            cx.add_window(|window, cx| Preview::new(Some(path), window.scale_factor(), cx));
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        window
            .update(cx, |reader, _, cx| reader.fit_page(cx))
            .unwrap();

        let mut highest = 0;
        for _ in 0..20 {
            window
                .update(cx, |reader, _, cx| reader.scroll_by(80.0, cx))
                .unwrap();
            cx.run_until_parked();
            cx.update(|cx| {
                cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                    .unwrap();
            });
            let state = window
                .read_with(cx, |reader, _| (reader.page_index(), reader.scale()))
                .unwrap();
            assert!(
                state.0 >= highest,
                "fit-page went backwards without scrolling backwards"
            );
            highest = state.0;
            cx.update(|cx| {
                cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                    .unwrap();
            });
            assert_eq!(
                window
                    .read_with(cx, |reader, _| (reader.page_index(), reader.scale()))
                    .unwrap(),
                state,
                "fit-page changed without input"
            );
        }
        assert!(highest >= 1, "scrolling must reach page two");
    }

    #[gpui_kit::test]
    fn dragging_text_and_copying_uses_document_coordinates(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_reader_keys(cx);
        });
        let window = cx.add_window(|window, cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        cx.run_until_parked();
        let (start, end) = window
            .read_with(cx, |reader, _| {
                let text = reader.text.get(&0).expect("page text loaded");
                let viewport = reader.page_viewport(0).expect("page viewport measured");
                let screen_point = |index: usize, fraction: f64| {
                    let glyph = &text.glyphs[index];
                    let screen = viewport.document_to_screen(Point {
                        x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                        y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
                    });
                    point(px(screen.x as f32), px(screen.y as f32))
                };
                (screen_point(0, 0.1), screen_point(4, 0.8))
            })
            .unwrap();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.drag(start, end, cx);
            })
            .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
            })
            .unwrap();
        cx.simulate_keystrokes(window.into(), "cmd-c");
        assert_eq!(
            cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("KVER ".into())
        );
        window
            .update(cx, |reader, _, cx| {
                reader.set_reading_mode(ReadingMode::SinglePage, cx)
            })
            .unwrap();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| {
                frame.render_frame(cx);
                frame.drag(start, end, cx);
            })
            .unwrap();
        });
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
            })
            .unwrap();
        window
            .update(cx, |reader, _, cx| reader.next_page(cx))
            .unwrap();
        window
            .read_with(cx, |reader, _| {
                assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
                assert_eq!(reader.selected_text(), None);
            })
            .unwrap();
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
