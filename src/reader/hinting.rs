//! Reader composition for the independent hint module.

use std::{ops::Range, sync::Arc};

use gpui_kit::component::{Sizable as _, button::ButtonVariants as _};

use super::{hint, *};

/// A word anchor in shared PageText. A caller may use its page/line identity
/// for a larger semantic target without changing this anchor's glyph geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordTarget {
    page: usize,
    line: usize,
    glyphs: Range<usize>,
}

impl WordTarget {
    pub fn page(&self) -> usize {
        self.page
    }
    pub fn line(&self) -> usize {
        self.line
    }
    pub fn glyphs(&self) -> Range<usize> {
        self.glyphs.clone()
    }
}

/// The caller can retain the immutable assignments for a subsequent stage.
pub enum HintEvent {
    Chosen {
        snapshot: Arc<hint::Snapshot<WordTarget>>,
        index: usize,
    },
    Cancelled,
}

impl gpui::EventEmitter<HintEvent> for ReaderView {}

type Filter = Arc<dyn Fn(&WordTarget) -> bool + Send + Sync>;

pub(super) struct Hinting {
    focus: FocusHandle,
    blur: Option<Subscription>,
    serial: u64,
    active: Option<Session>,
}

struct Session {
    geometry: Geometry,
    filter: Filter,
    metrics: hint::Metrics,
    input: hint::Input,
    state: State,
    marked: Option<WordTarget>,
}

enum State {
    Loading,
    Preparing,
    Ready(Arc<hint::Snapshot<WordTarget>>),
    Refused(&'static str),
}

#[derive(PartialEq)]
struct Geometry {
    bounds: gpui::Bounds<gpui::Pixels>,
    offset: gpui::Point<gpui::Pixels>,
    scale: f32,
    page: usize,
    mode: ReadingMode,
    generation: u64,
    rem: gpui::Pixels,
    density: f32,
    font: gpui::SharedString,
}

impl Hinting {
    pub(super) fn new(cx: &App) -> Self {
        Self {
            focus: cx.focus_handle(),
            blur: None,
            serial: 0,
            active: None,
        }
    }
    pub(super) fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

impl ReaderView {
    /// Begin a bounded visible-word acquisition. The caller chooses its subset;
    /// hinting owns legibility and returns an identity, never an action.
    /// Must be invoked from reader focus after the window's layout is settled.
    pub fn begin_word_hints(
        &mut self,
        filter: impl Fn(&WordTarget) -> bool + Send + Sync + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) || self.pages.is_empty() || self.marks_open(cx) {
            return;
        }
        self.invalidate_hints(cx);
        self.hints.active = Some(Session {
            geometry: self.hint_geometry(window, cx),
            filter: Arc::new(filter),
            metrics: hint::measure_labels(window, cx),
            input: hint::Input::default(),
            state: State::Loading,
            marked: None,
        });
        self.ruler.cancel_work();
        let focus = self.hints.focus.clone();
        self.hints.blur = Some(cx.on_focus_out(&focus, window, |reader, _, _, cx| {
            reader.invalidate_hints(cx);
        }));
        window.focus(&focus, cx);
        self.schedule_text(cx);
        cx.notify();
    }

    pub(super) fn invalidate_hints(&mut self, cx: &mut Context<Self>) {
        self.acquisition = None;
        self.hints.serial += 1;
        if self.hints.active.take().is_some() {
            cx.emit(HintEvent::Cancelled);
            cx.notify();
        }
    }

    pub(super) fn resume_word_hints(
        &mut self,
        snapshot: Arc<hint::Snapshot<WordTarget>>,
        marked: Option<WordTarget>,
    ) {
        if let Some(session) = &mut self.hints.active {
            session.state = State::Ready(snapshot);
            session.input = hint::Input::default();
            session.marked = marked;
        }
    }

    pub(super) fn finish_word_hints(&mut self) {
        self.hints.active = None;
        self.hints.serial += 1;
    }

    fn hint_geometry(&self, window: &Window, cx: &App) -> Geometry {
        Geometry {
            bounds: self.scroll.bounds(),
            offset: self.scroll.offset(),
            scale: self.scale,
            page: self.page_index,
            mode: self.reading_mode,
            generation: self.document_generation,
            rem: window.rem_size(),
            density: window.scale_factor(),
            font: cx.theme().mono_font_family.clone(),
        }
    }

    pub(super) fn sync_hints(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hints.active.as_ref().is_some_and(|session| {
            session.geometry != self.hint_geometry(window, cx)
                || !self.hints.focus.contains_focused(window, cx)
        }) {
            self.invalidate_hints(cx);
        }
        if !self.hints.is_active() {
            // Never restore through a menu/dialog/input which superseded hints.
            if self.hints.focus.contains_focused(window, cx) {
                window.focus(&self.focus, cx);
            }
            return;
        }
        if !matches!(
            self.hints.active.as_ref().map(|s| &s.state),
            Some(State::Loading)
        ) {
            return;
        }
        let (top, _, height) = self.viewport();
        let visible = self.layout().visible_pages(top, height);
        if visible.clone().any(|page| self.page_text.failed(page)) {
            if let Some(session) = &mut self.hints.active {
                session.state =
                    State::Refused("Couldn’t read page text. Reopen the document to retry.");
            }
            return;
        }
        let texts: Option<Vec<_>> = visible
            .clone()
            .map(|page| self.page_text.get(page).cloned().map(|text| (page, text)))
            .collect();
        let Some(texts) = texts else {
            return;
        };
        let bounds = self.scroll.bounds();
        // The command surface occupies the top two rem of the reader. It is
        // excluded before publishing labels, not covered up after assignment.
        let clip = hint::Rect::new(
            f32::from(bounds.left()) as f64,
            f32::from(bounds.top() + window.rem_size() * 2.) as f64,
            f32::from(bounds.right()) as f64,
            f32::from(bounds.bottom()) as f64,
        );
        let pages: Vec<_> = texts
            .iter()
            .filter_map(|(page, text)| {
                let projection = self.page_viewport(*page)?;
                let (width, height) = self.pages[*page];
                let origin = projection
                    .document_to_screen(text.transform.page_to_document(Point { x: 0., y: 0. })?);
                let page_clip = hint::Rect::new(
                    origin.x,
                    origin.y,
                    origin.x + width as f64 * self.scale as f64,
                    origin.y + height as f64 * self.scale as f64,
                );
                Some(hint::Page::new(*page, projection, page_clip, clip))
            })
            .collect();
        let Some(session) = self.hints.active.as_mut() else {
            return;
        };
        session.state = State::Preparing;
        let filter = session.filter.clone();
        let metrics = session.metrics;
        let serial = self.hints.serial;
        let lines = self
            .acquisition
            .as_ref()
            .is_some_and(|acquisition| acquisition.mode == selection::Targeting::Lines);
        let work = cx.background_executor().spawn(async move {
            let targets = texts.iter().flat_map(|(page, text)| {
                text.lines
                    .iter()
                    .enumerate()
                    .flat_map(|(line, _)| {
                        let mut anchored = false;
                        text.word_units(line)
                            .into_iter()
                            .filter_map(|glyphs| {
                                if lines && anchored {
                                    return None;
                                }
                                let id = WordTarget {
                                    page: *page,
                                    line,
                                    glyphs: glyphs.clone(),
                                };
                                // Boundary units include whitespace and punctuation for
                                // mouse selection; neither is a word hint target.
                                if !filter(&id)
                                    || !text.glyphs[glyphs.clone()]
                                        .iter()
                                        .any(|g| g.text.chars().any(char::is_alphanumeric))
                                {
                                    return None;
                                }
                                let glyphs = &text.glyphs[glyphs];
                                let left = glyphs
                                    .iter()
                                    .map(|g| g.start.x)
                                    .fold(f64::INFINITY, f64::min);
                                let right = glyphs
                                    .iter()
                                    .map(|g| g.ink_right.unwrap_or(g.end.x))
                                    .fold(f64::NEG_INFINITY, f64::max);
                                let top = glyphs
                                    .iter()
                                    .map(|g| g.top)
                                    .fold(f64::NEG_INFINITY, f64::max);
                                let bottom = glyphs
                                    .iter()
                                    .map(|g| g.bottom)
                                    .fold(f64::INFINITY, f64::min);
                                let target = hint::Target::new(
                                    id,
                                    *page,
                                    [
                                        Point { x: left, y: top },
                                        Point { x: right, y: top },
                                        Point { x: left, y: bottom },
                                        Point {
                                            x: right,
                                            y: bottom,
                                        },
                                    ],
                                );
                                if lines {
                                    // Identity remains the whole line; geometry belongs
                                    // to its first visible eligible word, not its first word.
                                    if !target.is_eligible(&pages, &[], metrics) {
                                        return None;
                                    }
                                    anchored = true;
                                }
                                Some(target)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            });
            hint::Snapshot::prepare(targets, &pages, &[], metrics)
        });
        cx.spawn(async move |reader, cx| {
            let result = work.await;
            let _ = reader.update(cx, |reader, cx| {
                if reader.hints.serial != serial {
                    return;
                }
                let Some(session) = reader.hints.active.as_mut() else {
                    return;
                };
                session.state = match result {
                    Ok(snapshot) => State::Ready(Arc::new(snapshot)),
                    Err(hint::Refusal::NoText) => State::Refused("No visible text to target"),
                    Err(hint::Refusal::Crowded) => {
                        State::Refused("Hints are too crowded. Zoom in and try again.")
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_hints(&self, page: usize, cx: &App) -> Vec<gpui::AnyElement> {
        let Some(session) = &self.hints.active else {
            return Vec::new();
        };
        let State::Ready(snapshot) = &session.state else {
            return Vec::new();
        };
        let Some(text) = self.page_text.get(page) else {
            return Vec::new();
        };
        let Some(viewport) = self.page_viewport(page) else {
            return Vec::new();
        };
        let Some(zero) = text.transform.page_to_document(Point { x: 0., y: 0. }) else {
            return Vec::new();
        };
        let origin = viewport.document_to_screen(zero);
        snapshot
            .paint_labels(page, &session.input, session.marked.as_ref())
            .into_iter()
            .map(|label| {
                label.render(
                    origin,
                    &session.input,
                    session.marked.as_ref() == Some(label.target().id()),
                    cx,
                )
            })
            .collect()
    }

    pub(super) fn render_hint_input(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let session = self.hints.active.as_ref()?;
        let prompt = match self
            .acquisition
            .as_ref()
            .map(|acquisition| &acquisition.stage)
        {
            Some(selection::Stage::Start) => "Choose start",
            Some(selection::Stage::End { .. }) => "Choose end",
            None => "Type two letters",
        };
        let line_mode = self
            .acquisition
            .as_ref()
            .is_some_and(|acquisition| acquisition.mode == selection::Targeting::Lines);
        let cancel = if line_mode {
            "Cancel line hints"
        } else {
            "Cancel word hints"
        };
        let status = match &session.state {
            State::Loading | State::Preparing => "Loading visible text…".to_owned(),
            State::Refused(message) => message.to_string(),
            State::Ready(_) if session.input.is_retry() => {
                "No matching hint. Try again.".to_owned()
            }
            State::Ready(_) => format!(
                "{prompt}: {}{}",
                session.input.prefix(),
                if session.input.prefix().is_empty() {
                    "__"
                } else {
                    "_"
                }
            ),
        };
        Some(
            div()
                .id("hint-input")
                .test_support()
                .track_focus(&self.hints.focus)
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h_8()
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .text_sm()
                .aria_label(status.clone())
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(if line_mode {
                            "Lines"
                        } else if self.acquisition.is_some() {
                            "Visual select"
                        } else {
                            "Word hints"
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .when(
                            matches!(session.state, State::Ready(_)) && !session.input.is_retry(),
                            |row| {
                                row.child(div().flex_shrink_0().child(prompt))
                                    .children((0..2).map(|index| {
                                        div()
                                            .flex_shrink_0()
                                            .w_5()
                                            .h_5()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .bg(cx.theme().muted)
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(
                                                session
                                                    .input
                                                    .prefix()
                                                    .chars()
                                                    .nth(index)
                                                    .map_or("·".to_owned(), |c| c.to_string()),
                                            )
                                    }))
                            },
                        )
                        .when(
                            !matches!(session.state, State::Ready(_)) || session.input.is_retry(),
                            |row| row.child(div().min_w_0().truncate().child(status)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .text_xs()
                        .child("Backspace corrects · Escape cancels"),
                )
                .child(
                    Button::new("cancel-word-hints")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .flex_shrink_0()
                        .accessibility_label(cancel)
                        .tooltip(format!("{cancel} (Escape)"))
                        .on_click(cx.listener(|reader, _, window, cx| {
                            reader.invalidate_hints(cx);
                            window.focus(&reader.focus, cx);
                        })),
                )
                .on_key_down(
                    cx.listener(|reader, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.modifiers.platform
                            || event.keystroke.modifiers.control
                            || event.keystroke.modifiers.alt
                        {
                            return;
                        }
                        // Consume hint input even while loading/refused; reader bare
                        // letters must never act as an accidental fallback.
                        cx.stop_propagation();
                        reader.sync_hints(window, cx);
                        let Some(session) = reader.hints.active.as_mut() else {
                            return;
                        };
                        if event.keystroke.key == "backspace"
                            && session.input.prefix().is_empty()
                            && matches!(
                                reader
                                    .acquisition
                                    .as_ref()
                                    .map(|acquisition| &acquisition.stage),
                                Some(selection::Stage::End { .. })
                            )
                        {
                            reader.back_to_selection_start();
                            cx.notify();
                            return;
                        }
                        let snapshot = match &session.state {
                            State::Ready(snapshot) => Some(snapshot.clone()),
                            _ => None,
                        };
                        match session.input.key(&event.keystroke.key, snapshot.as_deref()) {
                            hint::Outcome::Pending => {}
                            hint::Outcome::Cancelled => {
                                reader.invalidate_hints(cx);
                                if reader.hints.focus.contains_focused(window, cx) {
                                    window.focus(&reader.focus, cx);
                                }
                            }
                            hint::Outcome::Chosen(index) => {
                                if let Some(snapshot) = snapshot
                                    && !reader.choose_selection_target(snapshot.clone(), index)
                                {
                                    reader.finish_word_hints();
                                    cx.emit(HintEvent::Chosen { snapshot, index });
                                }
                                if !reader.hints.is_active()
                                    && reader.hints.focus.contains_focused(window, cx)
                                {
                                    window.focus(&reader.focus, cx);
                                }
                            }
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
        )
    }
}
