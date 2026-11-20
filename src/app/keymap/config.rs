//! Presets and user keyboard entries share one schema and command registry;
//! preset selection and Settings UI remain their respective owners.

use super::*;
use toml_edit::DocumentMut;

/// Resolve the complete document without applying any bindings on failure.
pub fn resolve_config(
    content: &str,
    platform: Platform,
    preset: Preset,
) -> Result<ResolvedKeymap, String> {
    let entries = parse_entries(content)?;
    let resolved = resolve_with(platform, preset, entries)
        .map_err(|error| format!("Invalid [keyboard] keymap: {error}"))?;
    validate_menu_shortcuts(&resolved, platform, preset)?;
    Ok(resolved)
}

/// Both embedded presets and user overrides use this schema and registry.
pub(super) fn parse_entries(content: &str) -> Result<Vec<Entry>, String> {
    let document: DocumentMut = content
        .parse()
        .map_err(|error| format!("Invalid settings TOML: {error}"))?;
    let Some(keyboard) = document.get("keyboard") else {
        return Ok(Vec::new());
    };
    let keyboard = keyboard.as_table().ok_or("[keyboard] must be a table")?;
    if keyboard.contains_key("unbind") {
        return Err("[[keyboard.unbind]] is not supported yet; rebind the key to another command or remove this entry".into());
    }
    let mut entries = Vec::new();
    if let Some(item) = keyboard.get("bindings") {
        let array = item
            .as_array_of_tables()
            .ok_or("[keyboard].bindings must be an array of tables ([[keyboard.bindings]])")?;
        for (index, table) in array.iter().enumerate() {
            let where_ = format!("[[keyboard.bindings]] entry {}", index + 1);
            for (name, _) in table.iter() {
                if !matches!(name, "keys" | "context" | "command" | "platform" | "letter") {
                    return Err(format!("{where_}: unknown field {name:?}"));
                }
            }
            let string = |name: &str| -> Result<Option<&str>, String> {
                table
                    .get(name)
                    .map(|item| {
                        item.as_str()
                            .ok_or_else(|| format!("{where_}.{name} must be a string"))
                    })
                    .transpose()
            };
            let keys = string("keys")?.ok_or_else(|| format!("{where_}.keys is required"))?;
            let context = match string("context")?.unwrap_or("global") {
                "global" => Context::Global,
                "reader" => Context::Reader,
                "sidebar" => Context::Sidebar,
                "contents" => Context::Contents,
                "annotations" => Context::Annotations,
                "settings" => Context::Settings,
                "about" => Context::About,
                name => {
                    return Err(format!(
                        "{where_}: unknown context {name:?}; use global, reader, sidebar, contents, annotations, settings, or about"
                    ));
                }
            };
            let platforms = parse_platforms(table.get("platform"), &where_)?;
            let command =
                string("command")?.ok_or_else(|| format!("{where_}.command is required"))?;
            let letter = string("letter")?
                .map(|letter| {
                    letter
                        .chars()
                        .next()
                        .filter(|letter| letter.is_ascii_lowercase())
                        .filter(|_| letter.len() == 1)
                        .ok_or_else(|| format!("{where_}.letter must be one lowercase letter a-z"))
                })
                .transpose()?;
            let action = command_action(command, letter).ok_or_else(|| {
                if letter.is_some() && command_names().contains(&command) {
                    format!("{where_}.letter is only supported for local-mark commands")
                } else {
                    format!(
                        "{where_}: unknown command {command:?}; use one of: {}",
                        command_names().join(", ")
                    )
                }
            })?;
            let entry = Entry {
                keys: keys.into(),
                context,
                action: Some(action),
                platforms,
            };
            // Validate even inactive platform entries so a typo isn't hidden
            // until the file is opened on another machine.
            parse(keys, Platform::Mac.os()).map_err(|_| {
                format!("{where_}: invalid keys {keys:?}; use GPUI keys such as cmd-o or g g")
            })?;
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn parse_platforms(
    item: Option<&toml_edit::Item>,
    where_: &str,
) -> Result<Option<Vec<Platform>>, String> {
    let Some(item) = item else { return Ok(None) };
    let name = |name: &str| match name {
        "mac" => Ok(Platform::Mac),
        "linux" => Ok(Platform::Linux),
        "windows" => Ok(Platform::Windows),
        _ => Err(format!(
            "{where_}.platform: unknown platform {name:?}; use mac, linux, or windows"
        )),
    };
    if let Some(single) = item.as_str() {
        return Ok(Some(vec![name(single)?]));
    }
    let invalid_type = || {
        format!(
            "{where_}.platform must be a platform string or a nonempty array of platform strings (mac, linux, windows)"
        )
    };
    let array = item.as_array().ok_or_else(invalid_type)?;
    if array.is_empty() {
        return Err(format!(
            "{where_}.platform must not be empty; omit it to match all platforms"
        ));
    }
    array
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = value.as_str().ok_or_else(|| {
                format!(
                    "{where_}.platform[{}] must be a platform string (mac, linux, windows)",
                    index + 1
                )
            })?;
            name(value)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn validate_menu_shortcuts(
    resolved: &ResolvedKeymap,
    platform: Platform,
    preset: Preset,
) -> Result<(), String> {
    if platform == Platform::Mac {
        let defaults = resolve(platform, preset).expect("embedded keyboard defaults are valid");
        for command in super::super::menu::ACCELERATOR_COMMANDS {
            let action = command_action(command, None).expect("native menu command is registered");
            let default = menu_binding_for(&defaults, action.as_ref());
            let Some(current) = menu_binding_for(resolved, action.as_ref()) else {
                continue;
            };
            // Shift alone still types into a text field. Only Command,
            // Control, Option, or Function prevents a key from
            // becoming an ordinary typing shortcut in a native menu.
            let current_is_typing_key = resolved.has_typing_menu_shortcut(action.as_ref());
            if current_is_typing_key
                && default.is_none_or(|default| current.sequence != default.sequence)
            {
                let remedy = if current.entry.context == Context::Global {
                    "use a modified shortcut or restrict this entry to reader"
                } else {
                    "also rebind this reader key or keep a modified shortcut for the command"
                };
                return Err(format!(
                    "Binding {} to {:?} would make it a typing-key macOS menu accelerator; {remedy}",
                    action.name(),
                    current.entry.keys,
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn menu_binding_for<'a>(
    map: &'a ResolvedKeymap,
    action: &dyn Action,
) -> Option<&'a Slot> {
    let matches = |slot: &&Slot| {
        slot.entry
            .action
            .as_ref()
            .is_some_and(|candidate| candidate.partial_eq(action))
    };
    // GPUI's macOS menu chooses the first action binding whose predicate
    // matches its default context; if none do, it falls back to the first
    // binding. Product contexts do not match that default; Global does.
    map.slots
        .iter()
        .find(|slot| slot.entry.context == Context::Global && matches(slot))
        .or_else(|| map.slots.iter().find(matches))
}

macro_rules! commands {
    ($($key:literal => $action:expr $(=> $letter_action:path)?),* $(,)?) => {
        fn command_action(name: &str, letter: Option<char>) -> Option<Box<dyn Action>> {
            match (name, letter) {
                $(
                    ($key, None) => Some(Box::new($action)),
                    $(($key, Some(letter)) => Some(Box::new($letter_action(letter))),)?
                )*
                _ => None,
            }
        }

        fn command_names() -> &'static [&'static str] {
            &[$($key),*]
        }
    };
}
commands! {
        "open-pdf" => OpenPdf, "show-recent" => ShowRecent,
        "new-window" => NewWindow, "close-window" => CloseWindow,
        "next-window" => NextWindow, "previous-window" => PreviousWindow,
        "split-side-by-side" => SplitSideBySide, "split-stacked" => SplitStacked,
        "switch-pane" => SwitchPane, "close-pane" => ClosePane,
        "keep-only-pane" => KeepOnlyPane,
        "quit" => Quit, "about-kver" => AboutKver, "check-for-updates" => CheckForUpdates,
        "show-settings" => ShowSettings, "close-settings" => CloseSettings,
        "close-about" => CloseAbout, "command-palette" => ShowCommandPalette,
        "show-contents" => ShowContents, "show-find" => ShowFind,
        "show-annotations" => ShowAnnotations,
        "visual-select" => crate::reader::VisualSelect,
        "visual-select-line" => crate::reader::VisualSelectLine,
        "follow-link" => crate::reader::FollowLink,
        "follow-link-in-new-window" => crate::reader::FollowLinkInNewWindow,
        "follow-link-in-split" => crate::reader::FollowLinkInSplit,
        "apply-markup" => crate::reader::ApplyMarkup,
        "highlight" => Highlight,
        "underline" => crate::reader::Underline,
        "strikethrough" => crate::reader::StrikeOut,
        "squiggly-underline" => crate::reader::Squiggly,
        "undo-annotation" => crate::reader::UndoAnnotation,
        "redo-annotation" => crate::reader::RedoAnnotation,
        "toggle-reading-ruler" => crate::reader::ToggleReadingRuler,
        "toggle-pdf-dark-mode" => crate::reader::TogglePdfDarkMode,
        "next-ruler-line" => crate::reader::NextRulerLine,
        "previous-ruler-line" => crate::reader::PreviousRulerLine,
        "delete-highlight" => DeleteHighlight,
        "recolor-highlight-yellow" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Yellow),
        "recolor-highlight-green" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Green),
        "recolor-highlight-blue" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Blue),
        "recolor-highlight-pink" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Pink),
        "recolor-highlight-purple" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Purple),
        "recolor-highlight-red" => crate::reader::RecolorHighlight(crate::pdf::annotations::HighlightColor::Red),
        "save-as" => SaveAs,
        "save" => Save,
        "toggle-sidebar" => ToggleSidebar, "activate-outline" => ActivateOutline,
        "next-page" => NextPage, "previous-page" => PreviousPage,
        "scroll-down" => ScrollDown, "scroll-up" => ScrollUp,
        "read-down" => crate::reader::ReadDown, "read-up" => crate::reader::ReadUp,
        "select-down" => SelectDown, "select-up" => SelectUp,
        "scroll-left" => ScrollLeft, "scroll-right" => ScrollRight,
        "scroll-screen-down" => ScrollScreenDown, "scroll-screen-up" => ScrollScreenUp,
        "scroll-half-screen-down" => ScrollHalfScreenDown,
        "scroll-half-screen-up" => ScrollHalfScreenUp,
        "document-start" => GoToDocumentStart, "document-end" => GoToDocumentEnd,
        "zoom-in" => ZoomIn, "zoom-out" => ZoomOut,
        "fit-width" => FitWidth, "fit-page" => FitPage,
        "single-page" => SinglePage, "continuous" => Continuous,
        "use-default-reading-mode" => UseDefaultReadingMode,
        "copy" => Copy, "next-match" => NextMatch, "previous-match" => PreviousMatch,
        "navigate-back" => NavigateBack, "navigate-forward" => NavigateForward,
        "show-marks" => ShowMarks, "set-local-mark" => ShowSetMark => SetLocalMark,
        "jump-local-mark" => ShowJumpMark => JumpLocalMark,
        "delete-local-mark" => ShowDeleteMark => DeleteLocalMark
}
