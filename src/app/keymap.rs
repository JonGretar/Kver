//! Embedded keyboard policy. Resolve slots before installing them with GPUI.
//!
//! Install after GPUI Kit initialization and before building native menus.
//! Views still own focused contexts and action handlers; GPUI owns dispatch
//! and pending multi-keystroke sequences.

use gpui_kit::component::{WindowExt as _, notification::Notification};
use gpui_kit::{
    Action, App, AppContext as _, Global, KeyBinding, KeyBindingContextPredicate, Keystroke,
    OperatingSystem,
};
use std::{collections::HashSet, fs, path::PathBuf, rc::Rc, time::Duration};

mod config;
pub use config::resolve_config;

use super::{
    commands::{AboutKver, CheckForUpdates, Quit},
    document_window::command_palette::ShowCommandPalette,
    document_window::sidebar::{ShowAnnotations, ShowContents, ShowFind, ToggleSidebar},
    document_window::{NavigateBack, NavigateForward, OpenPdf, Save, SaveAs},
    windows::{CloseWindow, NewWindow, NextWindow, PreviousWindow},
};
use crate::{
    about::CloseAbout,
    key_context,
    reader::{
        ActivateOutline, Continuous, Copy, DeleteHighlight, DeleteLocalMark, FitPage, FitWidth,
        GoToDocumentEnd, GoToDocumentStart, Highlight, JumpLocalMark, NextMatch, NextPage,
        PreviousMatch, PreviousPage, ScrollDown, ScrollHalfScreenDown, ScrollHalfScreenUp,
        ScrollLeft, ScrollRight, ScrollScreenDown, ScrollScreenUp, ScrollUp, SetLocalMark,
        ShowDeleteMark, ShowJumpMark, ShowMarks, ShowSetMark, SinglePage, UseDefaultReadingMode,
        ZoomIn, ZoomOut,
    },
    reading_history::ShowRecent,
    settings::AppPaths,
    settings_ui::{CloseSettings, ShowSettings},
};

// GPUI resolves a pending sequence before one-key shortcuts such as p/n/s/c.
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

/// Names in the keyboard configuration model, not GPUI view or widget names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    Global,
    Reader,
    Sidebar,
    Contents,
    Annotations,
    Settings,
    About,
}

impl Context {
    pub const fn name(self) -> Option<&'static str> {
        match self {
            Self::Global => None,
            Self::Reader => Some(key_context::READER),
            Self::Sidebar => Some(key_context::SIDEBAR),
            Self::Contents => Some(key_context::CONTENTS),
            Self::Annotations => Some(key_context::ANNOTATIONS),
            Self::Settings => Some(key_context::SETTINGS),
            Self::About => Some(key_context::ABOUT),
        }
    }

    fn inherits_from(self, parent: Self) -> bool {
        self == parent
            || parent == Self::Global
            || (matches!(self, Self::Contents | Self::Annotations) && parent == Self::Sidebar)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
    Windows,
}

impl Platform {
    fn os(self) -> OperatingSystem {
        match self {
            Self::Mac => OperatingSystem::Mac,
            Self::Linux => OperatingSystem::Linux,
            Self::Windows => OperatingSystem::Windows,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Standard,
    Vim,
}

impl Preset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Vim => "Vim",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Vim => "vim",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "standard" => Ok(Self::Standard),
            "vim" => Ok(Self::Vim),
            _ => Err(format!(
                "Unknown keyboard preset {value:?}; use standard or vim"
            )),
        }
    }
}

/// One user-facing key slot. A missing action is an explicit unbind, not an
/// absent override. A platform qualifier filters the entry before precedence.
pub struct Entry {
    keys: String,
    context: Context,
    action: Option<Box<dyn Action>>,
    platform: Option<Platform>,
}

impl Entry {
    pub fn bind(keys: impl Into<String>, context: Context, action: impl Action) -> Self {
        Self {
            keys: keys.into(),
            context,
            action: Some(Box::new(action)),
            platform: None,
        }
    }

    pub fn unbind(keys: impl Into<String>, context: Context) -> Self {
        Self {
            keys: keys.into(),
            context,
            action: None,
            platform: None,
        }
    }

    pub fn on(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Diagnostic {
    InvalidSequence(String),
    UnmatchedUnbind {
        keys: String,
        context: Context,
    },
    DuplicateSlot {
        keys: String,
        context: Context,
    },
    PrefixConflict {
        first: String,
        second: String,
        context: Context,
    },
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSequence(keys) => write!(
                formatter,
                "invalid keys {keys:?}; check the key spelling and modifiers"
            ),
            Self::UnmatchedUnbind { keys, context } => write!(
                formatter,
                "unbind {keys:?} in {context:?} has no inherited binding; remove this unbind or choose a bound key"
            ),
            Self::DuplicateSlot { keys, context } => write!(
                formatter,
                "conflicting entries for {keys:?} in {context:?}; keep one binding per key and context"
            ),
            Self::PrefixConflict {
                first,
                second,
                context,
            } => write!(
                formatter,
                "ambiguous sequences {first:?} and {second:?} in {context:?}; change or unbind one prefix"
            ),
        }
    }
}

struct Slot {
    entry: Entry,
    sequence: Vec<Keystroke>,
}

pub struct ResolvedKeymap {
    slots: Vec<Slot>,
    platform: Platform,
}

struct InstalledKeymap {
    product_start: usize,
    product_len: usize,
    content: Option<String>,
    preset: Preset,
    platform: Platform,
    path: Option<PathBuf>,
    reported_error: Option<String>,
    resolved: ResolvedKeymap,
}
impl Global for InstalledKeymap {}

pub fn active_preset(cx: &App) -> Preset {
    cx.try_global::<InstalledKeymap>()
        .map_or(Preset::default(), |map| map.preset)
}

pub fn platform(cx: &App) -> Platform {
    match cx.platform_info().os {
        OperatingSystem::Mac => Platform::Mac,
        OperatingSystem::Windows => Platform::Windows,
        _ => Platform::Linux,
    }
}

/// Rebuild the complete map rather than accumulating keys from prior presets.
pub fn select_preset(cx: &mut App, preset: Preset) -> Result<bool, String> {
    let Some(state) = cx.try_global::<InstalledKeymap>() else {
        return Ok(false);
    };
    let content = match &state.path {
        Some(path) => read_config(path)?,
        None => state.content.clone(),
    };
    reload(cx, content, preset)
}

pub fn active_map(cx: &App) -> Option<&ResolvedKeymap> {
    cx.try_global::<InstalledKeymap>()
        .map(|state| &state.resolved)
}

impl ResolvedKeymap {
    /// Return the effective command, including contextual unbinding and fallback.
    pub fn action(&self, keys: &str, context: Context) -> Option<&str> {
        let sequence = parse(keys, self.platform.os()).ok()?;
        [context, Context::Sidebar, Context::Global]
            .into_iter()
            .filter(|candidate| context.inherits_from(*candidate))
            .find_map(|candidate| {
                self.slots
                    .iter()
                    .find(|slot| slot.sequence == sequence && slot.entry.context == candidate)
            })
            .and_then(|slot| slot.entry.action.as_ref().map(|action| action.name()))
    }

    pub fn shortcuts(&self, action: &dyn Action, context: Context) -> Vec<&str> {
        self.slots
            .iter()
            .filter_map(|slot| {
                let bound = slot.entry.action.as_ref()?;
                (bound.name() == action.name()
                    && context.inherits_from(slot.entry.context)
                    && self.action(&slot.entry.keys, context) == Some(action.name()))
                .then_some(slot.entry.keys.as_str())
            })
            .collect()
    }

    /// Cocoa turns a one-stroke menu binding into a window-independent key
    /// equivalent. Reader typing keys must instead stay in GPUI's focus tree.
    pub(crate) fn has_typing_menu_shortcut(&self, action: &dyn Action) -> bool {
        config::menu_binding_for(self, action).is_some_and(|slot| {
            slot.sequence.len() == 1 && {
                let modifiers = slot.sequence[0].modifiers;
                !modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function
            }
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = (Context, &str, &str)> {
        self.slots.iter().filter_map(|slot| {
            Some((
                slot.entry.context,
                slot.entry.action.as_ref()?.name(),
                slot.entry.keys.as_str(),
            ))
        })
    }

    /// Bind the resolved policy and component behavior masks, rather than
    /// adding each configuration layer to GPUI.
    pub fn bindings(&self, cx: &App) -> Vec<KeyBinding> {
        self.slots
            .iter()
            .filter_map(|slot| {
                let action = slot.entry.action.as_ref()?;
                let excluded = self
                    .slots
                    .iter()
                    .filter(|other| {
                        other.entry.context != slot.entry.context
                            && other.entry.context.inherits_from(slot.entry.context)
                            && other.sequence == slot.sequence
                    })
                    .map(|other| format!("!{}", other.entry.context.name().unwrap()))
                    .collect::<Vec<_>>();
                let mut predicate = slot.entry.context.name().map(str::to_owned);
                for exclusion in excluded {
                    predicate = Some(match predicate {
                        Some(current) => format!("{current} && {exclusion}"),
                        None => exclusion,
                    });
                }
                let predicate = predicate.as_deref().map(|text| {
                    Rc::new(KeyBindingContextPredicate::parse(text).expect("product context"))
                });
                Some(
                    KeyBinding::load(
                        &slot.entry.keys,
                        action.boxed_clone(),
                        predicate,
                        false,
                        None,
                        cx,
                    )
                    .expect("sequence validated by resolver"),
                )
            })
            // Nested row buttons use native Enter/Space activation, not List's
            // inherited Enter-to-confirm binding. Reinstall this mask with the
            // product bindings so preset/config replacement cannot lose it.
            .chain([KeyBinding::new(
                "enter",
                gpui_kit::NoAction,
                Some(crate::key_context::ANNOTATION_ROW_ACTIONS),
            )])
            // Selection dismissal is a surface lifecycle key, including while
            // its toolbar buttons have focus. Menus handle Escape first.
            .chain([KeyBinding::new(
                "escape",
                crate::reader::DeselectAnnotation,
                Some(crate::key_context::SELECTED_ANNOTATION),
            )])
            .collect()
    }
}

fn parse(keys: &str, os: OperatingSystem) -> Result<Vec<Keystroke>, Diagnostic> {
    if keys.trim() != keys || keys.is_empty() {
        return Err(Diagnostic::InvalidSequence(keys.into()));
    }
    keys.split_whitespace()
        .map(|key| {
            let stroke = Keystroke::parse_with_os(key, os)
                .map_err(|_| Diagnostic::InvalidSequence(keys.into()))?;
            let name = stroke.key.as_str();
            let known = (name.len() == 1 && name.is_ascii())
                || matches!(
                    name,
                    "space"
                        | "enter"
                        | "escape"
                        | "tab"
                        | "backspace"
                        | "delete"
                        | "up"
                        | "down"
                        | "left"
                        | "right"
                        | "pageup"
                        | "pagedown"
                        | "home"
                        | "end"
                        | "insert"
                )
                || name
                    .strip_prefix('f')
                    .and_then(|number| number.parse::<u8>().ok())
                    .is_some_and(|number| (1..=24).contains(&number));
            if !known || stroke.key_char.is_some() {
                return Err(Diagnostic::InvalidSequence(keys.into()));
            }
            Ok(stroke)
        })
        .collect()
}

fn apply_layer(
    slots: &mut Vec<Slot>,
    layer: impl IntoIterator<Item = Entry>,
    platform: Platform,
) -> Result<(), Diagnostic> {
    let mut seen = HashSet::new();
    for entry in layer {
        if entry.platform.is_some_and(|only| only != platform) {
            continue;
        }
        let sequence = parse(&entry.keys, platform.os())?;
        let identity = (entry.context, sequence.clone());
        if !seen.insert(identity) {
            return Err(Diagnostic::DuplicateSlot {
                keys: entry.keys,
                context: entry.context,
            });
        }
        if entry.action.is_none()
            && !slots.iter().any(|slot| {
                slot.sequence == sequence
                    && entry.context.inherits_from(slot.entry.context)
                    && slot.entry.action.is_some()
            })
        {
            return Err(Diagnostic::UnmatchedUnbind {
                keys: entry.keys,
                context: entry.context,
            });
        }
        if let Some(index) = slots
            .iter()
            .position(|slot| slot.sequence == sequence && slot.entry.context == entry.context)
        {
            slots[index] = Slot { entry, sequence };
        } else {
            slots.push(Slot { entry, sequence });
        }
    }
    Ok(())
}

fn contexts_overlap(a: Context, b: Context) -> bool {
    a.inherits_from(b) || b.inherits_from(a)
}

fn validate_prefixes(slots: &[Slot]) -> Result<(), Diagnostic> {
    for (index, first) in slots.iter().enumerate() {
        if first.entry.action.is_none() {
            continue;
        }
        for second in &slots[index + 1..] {
            if second.entry.action.is_none() {
                continue;
            }
            if !contexts_overlap(first.entry.context, second.entry.context) {
                continue;
            }
            let (short, long) = if first.sequence.len() < second.sequence.len() {
                (first, second)
            } else {
                (second, first)
            };
            if short.sequence.len() < long.sequence.len()
                && long.sequence.starts_with(&short.sequence)
                && !(short.entry.context == Context::Global
                    && slots.iter().any(|slot| {
                        slot.entry.context == long.entry.context && slot.sequence == short.sequence
                    }))
            {
                return Err(Diagnostic::PrefixConflict {
                    first: short.entry.keys.clone(),
                    second: long.entry.keys.clone(),
                    context: long.entry.context,
                });
            }
        }
    }
    Ok(())
}

pub fn resolve(platform: Platform, preset: Preset) -> Result<ResolvedKeymap, Diagnostic> {
    resolve_with(platform, preset, [])
}

/// Layers are common, platform, preset, then user overrides. Replacing a
/// slot retains its display order, so a modified menu shortcut is not displaced
/// by a bare reader alias. No stale earlier-layer binding reaches GPUI.
pub fn resolve_with(
    platform: Platform,
    preset: Preset,
    overrides: impl IntoIterator<Item = Entry>,
) -> Result<ResolvedKeymap, Diagnostic> {
    let mut slots = Vec::new();
    apply_layer(&mut slots, common(), platform)?;
    apply_layer(&mut slots, platform_defaults(), platform)?;
    apply_layer(&mut slots, preset_defaults(preset), platform)?;
    apply_layer(&mut slots, overrides, platform)?;
    validate_prefixes(&slots)?;
    Ok(ResolvedKeymap { slots, platform })
}

/// Register the effective defaults once, before native menus snapshot keys.
pub fn install(cx: &mut App) {
    install_with_paths(cx, Preset::default(), None);
}

/// Install a selected preset before building native menus.
pub fn install_preset(cx: &mut App, preset: Preset) {
    install_with_paths(cx, preset, None);
}

/// A failed edit never reaches GPUI. Polling catches atomic editor replacements
/// as well as in-place writes; equality avoids rebuilding menus on every tick.
pub fn install_with_paths(cx: &mut App, preset: Preset, paths: Option<AppPaths>) {
    let platform = match cx.platform_info().os {
        OperatingSystem::Mac => Platform::Mac,
        OperatingSystem::Windows => Platform::Windows,
        _ => Platform::Linux,
    };
    let product_start = cx.key_bindings().borrow().bindings().len();
    let path = paths.map(|paths| paths.config().to_path_buf());
    let content = path.as_ref().and_then(|path| fs::read_to_string(path).ok());
    let result = content
        .as_deref()
        .map(|content| resolve_config(content, platform, preset))
        .unwrap_or_else(|| resolve(platform, preset).map_err(|error| error.to_string()));
    let valid = result.is_ok();
    let map = result.unwrap_or_else(|error| {
        eprintln!("Keyboard settings not applied: {error}");
        resolve(platform, preset).expect("embedded keyboard defaults are valid")
    });
    let bindings = map.bindings(cx);
    let product_len = bindings.len();
    cx.bind_keys(bindings);
    cx.set_global(InstalledKeymap {
        product_start,
        product_len,
        content: if valid { content } else { None },
        preset,
        platform,
        path: path.clone(),
        reported_error: None,
        resolved: map,
    });
    if let Some(path) = path {
        cx.spawn(async move |cx| {
            loop {
                let file = path.clone();
                let result = cx.background_spawn(async move { read_config(&file) }).await;
                cx.update(|cx| {
                    if let Err(error) = reload_result(cx, result) {
                        report_error(cx, error);
                    }
                });
                cx.background_executor()
                    .timer(Duration::from_millis(350))
                    .await;
            }
        })
        .detach();
    }
}

fn read_config(path: &PathBuf) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "Cannot read keyboard settings at {}: {error}",
            path.display()
        )),
    }
}

/// Also used when Settings changes the preset, after its updated file is saved.
pub fn reload(cx: &mut App, content: Option<String>, preset: Preset) -> Result<bool, String> {
    let state = cx.global::<InstalledKeymap>();
    if state.content == content && state.preset == preset {
        return Ok(false);
    }
    let platform = state.platform;
    let map = match &content {
        Some(text) => resolve_config(text, platform, preset)?,
        None => resolve(platform, preset).map_err(|error| error.to_string())?,
    };
    let old_range = state.product_start..state.product_start + state.product_len;
    let retained = cx
        .key_bindings()
        .borrow()
        .bindings()
        .enumerate()
        .filter(|(index, _)| !old_range.contains(index))
        .map(|(_, binding)| binding.clone())
        .collect::<Vec<_>>();
    let product_start = retained.len();
    let bindings = map.bindings(cx);
    let product_len = bindings.len();
    cx.clear_key_bindings();
    cx.bind_keys(retained);
    cx.bind_keys(bindings);
    let state = cx.global_mut::<InstalledKeymap>();
    state.product_start = product_start;
    state.product_len = product_len;
    state.content = content;
    state.preset = preset;
    state.reported_error = None;
    state.resolved = map;
    super::menu::refresh_shortcuts(cx);
    Ok(true)
}

fn report_error(cx: &mut App, error: String) {
    let state = cx.global_mut::<InstalledKeymap>();
    if state.reported_error.as_ref() == Some(&error) {
        return;
    }
    state.reported_error = Some(error.clone());
    eprintln!("Keyboard settings not applied: {error}");
    if let Some(handle) = cx.active_window() {
        let _ = cx.update_window(handle, |_, window, cx| {
            window.push_notification(
                Notification::warning(format!(
                    "Keyboard settings not applied: {error}. Last valid shortcuts remain active."
                )),
                cx,
            );
        });
    }
}

fn reload_result(cx: &mut App, result: Result<Option<String>, String>) -> Result<bool, String> {
    let content = result?;
    let preset = cx.global::<InstalledKeymap>().preset;
    reload(cx, content, preset)
}

fn common() -> Vec<Entry> {
    let mut entries = vec![
        Entry::bind("cmd-o", Context::Global, OpenPdf),
        Entry::bind("cmd-shift-o", Context::Global, ShowRecent),
        Entry::bind("cmd-n", Context::Global, NewWindow),
        Entry::bind("cmd-shift-p", Context::Global, ShowCommandPalette),
        Entry::bind("cmd-shift-m", Context::Global, ShowMarks),
        Entry::bind("cmd-z", Context::Reader, crate::reader::UndoAnnotation),
        Entry::bind(
            "cmd-shift-z",
            Context::Reader,
            crate::reader::RedoAnnotation,
        ),
        Entry::bind("cmd-z", Context::Annotations, crate::reader::UndoAnnotation),
        Entry::bind(
            "cmd-shift-z",
            Context::Annotations,
            crate::reader::RedoAnnotation,
        ),
        Entry::bind("backspace", Context::Reader, DeleteHighlight),
        Entry::bind("delete", Context::Reader, DeleteHighlight),
        Entry::bind("backspace", Context::Annotations, DeleteHighlight),
        Entry::bind("delete", Context::Annotations, DeleteHighlight),
        Entry::bind("cmd-shift-s", Context::Global, SaveAs),
        Entry::bind("cmd-s", Context::Global, Save),
        Entry::bind("escape", Context::Sidebar, ToggleSidebar),
        Entry::bind("enter", Context::Contents, ActivateOutline),
        Entry::bind("cmd-c", Context::Annotations, Copy),
        // Native menus snapshot the first binding for an Action. Keep their
        // page accelerators modified; bare reader arrows must never become
        // menu key equivalents in a focused text control.
        Entry::bind("cmd-alt-right", Context::Reader, NextPage),
        Entry::bind("cmd-alt-left", Context::Reader, PreviousPage),
        Entry::bind("cmd-c", Context::Reader, Copy),
        Entry::bind("ctrl-o", Context::Reader, NavigateBack),
        Entry::bind("ctrl-i", Context::Reader, NavigateForward),
        Entry::bind("ctrl-o", Context::Contents, NavigateBack),
        Entry::bind("ctrl-i", Context::Contents, NavigateForward),
    ];
    entries.extend([
        Entry::bind("cmd-w", Context::Global, CloseWindow),
        Entry::bind("cmd-,", Context::Global, ShowSettings),
        Entry::bind("escape", Context::Settings, CloseSettings),
        Entry::bind("escape", Context::About, CloseAbout),
    ]);
    entries
}

fn platform_defaults() -> Vec<Entry> {
    vec![
        Entry::bind("cmd-`", Context::Global, NextWindow).on(Platform::Mac),
        Entry::bind("cmd-shift-`", Context::Global, PreviousWindow).on(Platform::Mac),
        Entry::bind("ctrl-f6", Context::Global, NextWindow).on(Platform::Linux),
        Entry::bind("ctrl-shift-f6", Context::Global, PreviousWindow).on(Platform::Linux),
        Entry::bind("ctrl-f6", Context::Global, NextWindow).on(Platform::Windows),
        Entry::bind("ctrl-shift-f6", Context::Global, PreviousWindow).on(Platform::Windows),
        Entry::bind("cmd-q", Context::Global, Quit).on(Platform::Mac),
        Entry::bind("cmd-q", Context::Global, Quit).on(Platform::Linux),
    ]
}

fn preset_defaults(preset: Preset) -> Vec<Entry> {
    match preset {
        Preset::Standard => vec![
            Entry::bind("cmd-f", Context::Global, ShowFind),
            Entry::bind("cmd-b", Context::Global, ToggleSidebar),
            Entry::bind("cmd-g", Context::Global, NextMatch),
            Entry::bind("cmd-shift-g", Context::Global, PreviousMatch),
            Entry::bind("cmd-t", Context::Global, ShowContents),
            Entry::bind(
                "secondary-enter",
                Context::Reader,
                crate::reader::ApplyMarkup,
            ),
            Entry::bind("secondary-shift-h", Context::Reader, Highlight),
            Entry::bind(
                "secondary-shift-u",
                Context::Reader,
                crate::reader::Underline,
            ),
            Entry::bind(
                "secondary-shift-x",
                Context::Reader,
                crate::reader::StrikeOut,
            ),
            Entry::bind("secondary-alt-u", Context::Reader, crate::reader::Squiggly),
            Entry::bind("secondary-shift-a", Context::Global, ShowAnnotations),
            Entry::bind("right", Context::Reader, NextPage),
            Entry::bind("left", Context::Reader, PreviousPage),
            Entry::bind("down", Context::Reader, ScrollDown),
            Entry::bind("up", Context::Reader, ScrollUp),
            Entry::bind("pagedown", Context::Reader, ScrollScreenDown),
            Entry::bind("pageup", Context::Reader, ScrollScreenUp),
            Entry::bind("cmd-=", Context::Reader, ZoomIn),
            Entry::bind("cmd-shift-=", Context::Reader, ZoomIn),
            Entry::bind("cmd--", Context::Reader, ZoomOut),
            Entry::bind("cmd-alt-w", Context::Reader, FitWidth),
            Entry::bind("cmd-alt-f", Context::Reader, FitPage),
            Entry::bind("alt-down", Context::Reader, NextPage).on(Platform::Mac),
            Entry::bind("alt-up", Context::Reader, PreviousPage).on(Platform::Mac),
            Entry::bind("ctrl-pagedown", Context::Reader, NextPage).on(Platform::Linux),
            Entry::bind("ctrl-pageup", Context::Reader, PreviousPage).on(Platform::Linux),
            Entry::bind("ctrl-pagedown", Context::Reader, NextPage).on(Platform::Windows),
            Entry::bind("ctrl-pageup", Context::Reader, PreviousPage).on(Platform::Windows),
            Entry::bind("alt-left", Context::Reader, NavigateBack),
            Entry::bind("alt-right", Context::Reader, NavigateForward),
            Entry::bind("alt-left", Context::Contents, NavigateBack),
            Entry::bind("alt-right", Context::Contents, NavigateForward),
        ],
        // Keep bare reader keys inside Reader; app/window commands are inherited.
        Preset::Vim => {
            let mut entries = vec![
                Entry::bind("a a", Context::Reader, crate::reader::ApplyMarkup),
                Entry::bind("a h", Context::Reader, Highlight),
                Entry::bind("a u", Context::Reader, crate::reader::Underline),
                Entry::bind("a s", Context::Reader, crate::reader::StrikeOut),
                Entry::bind("a w", Context::Reader, crate::reader::Squiggly),
                Entry::bind("g a", Context::Reader, ShowAnnotations),
                Entry::bind("g c", Context::Reader, ShowContents),
                Entry::bind("u", Context::Reader, crate::reader::UndoAnnotation),
                Entry::bind("ctrl-r", Context::Reader, crate::reader::RedoAnnotation),
                Entry::bind("x", Context::Reader, DeleteHighlight),
                Entry::bind("u", Context::Annotations, crate::reader::UndoAnnotation),
                Entry::bind(
                    "ctrl-r",
                    Context::Annotations,
                    crate::reader::RedoAnnotation,
                ),
                Entry::bind("x", Context::Annotations, DeleteHighlight),
                Entry::bind("right", Context::Reader, ScrollRight),
                Entry::bind("shift-j", Context::Reader, NextPage),
                Entry::bind("left", Context::Reader, ScrollLeft),
                Entry::bind("shift-k", Context::Reader, PreviousPage),
                Entry::bind("n", Context::Reader, NextMatch),
                Entry::bind("shift-n", Context::Reader, PreviousMatch),
                Entry::bind("ctrl-f", Context::Reader, ScrollScreenDown),
                Entry::bind("ctrl-b", Context::Reader, ScrollScreenUp),
                Entry::bind("ctrl-d", Context::Reader, ScrollHalfScreenDown),
                Entry::bind("ctrl-u", Context::Reader, ScrollHalfScreenUp),
                Entry::bind("down", Context::Reader, ScrollDown),
                Entry::bind("j", Context::Reader, ScrollDown),
                Entry::bind("up", Context::Reader, ScrollUp),
                Entry::bind("k", Context::Reader, ScrollUp),
                Entry::bind("h", Context::Reader, ScrollLeft),
                Entry::bind("l", Context::Reader, ScrollRight),
                Entry::bind("space", Context::Reader, ScrollHalfScreenDown),
                Entry::bind("shift-space", Context::Reader, ScrollHalfScreenUp),
                Entry::bind("g g", Context::Reader, GoToDocumentStart),
                Entry::bind("shift-g", Context::Reader, GoToDocumentEnd),
                Entry::bind("o", Context::Reader, OpenPdf),
                Entry::bind("shift-o", Context::Reader, ShowRecent),
                Entry::bind("/", Context::Reader, ShowFind),
                Entry::bind(":", Context::Reader, ShowCommandPalette),
                Entry::bind("=", Context::Reader, ZoomIn),
                Entry::bind("shift-=", Context::Reader, ZoomIn),
                Entry::bind("-", Context::Reader, ZoomOut),
                Entry::bind("w", Context::Reader, FitWidth),
                Entry::bind("f", Context::Reader, FitPage),
                Entry::bind("s", Context::Reader, SinglePage),
                Entry::bind("c", Context::Reader, Continuous),
            ];
            for (letter, set, jump, delete) in MARK_KEY_SEQUENCES {
                entries.extend([
                    Entry::bind(set, Context::Reader, SetLocalMark(letter)),
                    Entry::bind(jump, Context::Reader, JumpLocalMark(letter)),
                    Entry::bind(delete, Context::Reader, DeleteLocalMark(letter)),
                ]);
            }
            entries
        }
    }
}

#[cfg(test)]
mod tests;
