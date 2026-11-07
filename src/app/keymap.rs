//! Default application shortcut policy.
//!
//! Install after GPUI Kit initialization and before building native menus.
//! Views still own focused contexts and action handlers; GPUI owns dispatch
//! and pending multi-keystroke sequences. User overrides are not implemented.

use gpui_kit::{App, KeyBinding};

use super::{
    commands::Quit,
    document_window::command_palette::ShowCommandPalette,
    document_window::sidebar::{ShowContents, ShowFind, ToggleSidebar},
    windows::{CloseWindow, NewWindow, NextWindow, PreviousWindow},
};
use crate::{
    about::CloseAbout,
    reader::{
        ActivateOutline, Continuous, Copy, DeleteLocalMark, FitPage, FitWidth, JumpLocalMark,
        NextMatch, NextPage, OpenPdf, PreviousMatch, PreviousPage, SetLocalMark, ShowMarks,
        SinglePage, ZoomIn, ZoomOut,
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

/// Register Kver's default bindings once during application startup.
pub fn install(cx: &mut App) {
    cx.bind_keys(default_bindings());
}

fn default_bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        // Application and document-window commands.
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
        // Bare reading keys must not intercept text input.
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
        // Modified bindings also provide palette shortcut hints.
        KeyBinding::new("cmd-alt-right", NextPage, None),
        KeyBinding::new("cmd-alt-left", PreviousPage, None),
        KeyBinding::new("cmd-alt-=", ZoomIn, None),
        KeyBinding::new("cmd-alt--", ZoomOut, None),
        KeyBinding::new("cmd-alt-w", FitWidth, None),
        KeyBinding::new("cmd-alt-f", FitPage, None),
        KeyBinding::new("cmd-c", Copy, Some("Reader")),
    ];
    bindings.extend(
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
    // Preserve startup precedence: these were registered after reader bindings.
    bindings.extend([
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-,", ShowSettings, None),
        KeyBinding::new("escape", CloseSettings, Some("KverSettings")),
        KeyBinding::new("escape", CloseAbout, Some("AboutKver")),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    bindings
}

#[cfg(test)]
mod tests;
