//! Embedded keyboard policy. Resolve slots before installing them with GPUI.
//!
//! Install after GPUI Kit initialization and before building native menus.
//! Views still own focused contexts and action handlers; GPUI owns dispatch
//! and pending multi-keystroke sequences. User file loading is not implemented.

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, Keystroke, OperatingSystem};
use std::{collections::HashSet, rc::Rc};

use super::{
    commands::Quit,
    document_window::command_palette::ShowCommandPalette,
    document_window::sidebar::{ShowContents, ShowFind, ToggleSidebar},
    document_window::{NavigateBack, NavigateForward, OpenPdf},
    windows::{CloseWindow, NewWindow, NextWindow, PreviousWindow},
};
use crate::{
    about::CloseAbout,
    key_context,
    reader::{
        ActivateOutline, Continuous, Copy, DeleteLocalMark, FitPage, FitWidth, JumpLocalMark,
        NextMatch, NextPage, PreviousMatch, PreviousPage, ScrollDown, ScrollUp, SetLocalMark,
        ShowMarks, SinglePage, ZoomIn, ZoomOut,
    },
    reading_history::ShowRecent,
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
            Self::Settings => Some(key_context::SETTINGS),
            Self::About => Some(key_context::ABOUT),
        }
    }

    fn inherits_from(self, parent: Self) -> bool {
        self == parent
            || parent == Self::Global
            || (self == Self::Contents && parent == Self::Sidebar)
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

struct Slot {
    entry: Entry,
    sequence: Vec<Keystroke>,
}

pub struct ResolvedKeymap {
    slots: Vec<Slot>,
    platform: Platform,
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

    /// Bind only the resolved policy, rather than adding each layer to GPUI.
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
            .collect()
    }
}

fn parse(keys: &str, os: OperatingSystem) -> Result<Vec<Keystroke>, Diagnostic> {
    if keys.trim() != keys || keys.is_empty() {
        return Err(Diagnostic::InvalidSequence(keys.into()));
    }
    keys.split_whitespace()
        .map(|key| {
            Keystroke::parse_with_os(key, os).map_err(|_| Diagnostic::InvalidSequence(keys.into()))
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
        if let Some(index) = slots
            .iter()
            .position(|slot| slot.sequence == sequence && slot.entry.context == entry.context)
        {
            slots.remove(index);
        }
        slots.push(Slot { entry, sequence });
    }
    Ok(())
}

fn contexts_overlap(a: Context, b: Context) -> bool {
    a == b
        || a == Context::Global
        || b == Context::Global
        || matches!(
            (a, b),
            (Context::Sidebar, Context::Contents) | (Context::Contents, Context::Sidebar)
        )
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

/// Layers are common, platform, preset, then future user overrides. Replacing a
/// slot moves it to the later layer's position; no GPUI binding is ever shadowed
/// by a stale binding from an earlier layer.
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
    let platform = match cx.platform_info().os {
        OperatingSystem::Mac => Platform::Mac,
        OperatingSystem::Windows => Platform::Windows,
        _ => Platform::Linux,
    };
    let map = resolve(platform, Preset::default()).expect("embedded keyboard defaults are valid");
    cx.bind_keys(map.bindings(cx));
}

fn common() -> Vec<Entry> {
    let mut entries = vec![
        Entry::bind("cmd-o", Context::Global, OpenPdf),
        Entry::bind("cmd-shift-o", Context::Global, ShowRecent),
        Entry::bind("cmd-n", Context::Global, NewWindow),
        Entry::bind("cmd-shift-p", Context::Global, ShowCommandPalette),
        Entry::bind("cmd-shift-m", Context::Global, ShowMarks),
        Entry::bind("cmd-f", Context::Global, ShowFind),
        Entry::bind("cmd-b", Context::Global, ToggleSidebar),
        Entry::bind("cmd-g", Context::Global, NextMatch),
        Entry::bind("cmd-shift-g", Context::Global, PreviousMatch),
        Entry::bind("cmd-t", Context::Global, ShowContents),
        Entry::bind("escape", Context::Sidebar, ToggleSidebar),
        Entry::bind("enter", Context::Contents, ActivateOutline),
        Entry::bind("cmd-alt-right", Context::Global, NextPage),
        Entry::bind("cmd-alt-left", Context::Global, PreviousPage),
        Entry::bind("cmd-alt-=", Context::Global, ZoomIn),
        Entry::bind("cmd-alt--", Context::Global, ZoomOut),
        Entry::bind("cmd-alt-w", Context::Global, FitWidth),
        Entry::bind("cmd-alt-f", Context::Global, FitPage),
        Entry::bind("cmd-c", Context::Reader, Copy),
        Entry::bind("ctrl-o", Context::Reader, NavigateBack),
        Entry::bind("ctrl-i", Context::Reader, NavigateForward),
        Entry::bind("ctrl-o", Context::Contents, NavigateBack),
        Entry::bind("ctrl-i", Context::Contents, NavigateForward),
    ];
    for (letter, set, jump, delete) in MARK_KEY_SEQUENCES {
        entries.extend([
            Entry::bind(set, Context::Reader, SetLocalMark(letter)),
            Entry::bind(jump, Context::Reader, JumpLocalMark(letter)),
            Entry::bind(delete, Context::Reader, DeleteLocalMark(letter)),
        ]);
    }
    entries.extend([
        Entry::bind("cmd-w", Context::Global, CloseWindow),
        Entry::bind("cmd-,", Context::Global, ShowSettings),
        Entry::bind("escape", Context::Settings, CloseSettings),
        Entry::bind("escape", Context::About, CloseAbout),
        Entry::bind("cmd-q", Context::Global, Quit),
    ]);
    entries
}

fn platform_defaults() -> Vec<Entry> {
    vec![
        Entry::bind("cmd-`", Context::Global, NextWindow).on(Platform::Mac),
        Entry::bind("cmd-shift-`", Context::Global, PreviousWindow).on(Platform::Mac),
        Entry::bind("cmd-`", Context::Global, NextWindow).on(Platform::Linux),
        Entry::bind("cmd-shift-`", Context::Global, PreviousWindow).on(Platform::Linux),
        Entry::bind("cmd-`", Context::Global, NextWindow).on(Platform::Windows),
        Entry::bind("cmd-shift-`", Context::Global, PreviousWindow).on(Platform::Windows),
    ]
}

fn preset_defaults(preset: Preset) -> Vec<Entry> {
    match preset {
        // The Vim preset is built in #104; until then it shares the existing
        // reader bindings so selecting it cannot silently remove commands.
        Preset::Standard | Preset::Vim => vec![
            Entry::bind("right", Context::Reader, NextPage),
            Entry::bind("n", Context::Reader, NextPage),
            Entry::bind("left", Context::Reader, PreviousPage),
            Entry::bind("p", Context::Reader, PreviousPage),
            Entry::bind("down", Context::Reader, ScrollDown),
            Entry::bind("j", Context::Reader, ScrollDown),
            Entry::bind("up", Context::Reader, ScrollUp),
            Entry::bind("k", Context::Reader, ScrollUp),
            Entry::bind("=", Context::Reader, ZoomIn),
            Entry::bind("shift-=", Context::Reader, ZoomIn),
            Entry::bind("-", Context::Reader, ZoomOut),
            Entry::bind("w", Context::Reader, FitWidth),
            Entry::bind("f", Context::Reader, FitPage),
            Entry::bind("s", Context::Reader, SinglePage),
            Entry::bind("c", Context::Reader, Continuous),
        ],
    }
}

#[cfg(test)]
mod tests;
