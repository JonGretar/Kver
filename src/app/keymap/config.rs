//! User keyboard entries are parsed independently from the rest of the settings
//! document; preset selection and Settings UI remain their respective owners.

use super::*;
use toml_edit::DocumentMut;

/// Resolve the complete document without applying any bindings on failure.
pub fn resolve_config(
    content: &str,
    platform: Platform,
    preset: Preset,
) -> Result<ResolvedKeymap, String> {
    let document: DocumentMut = content
        .parse()
        .map_err(|error| format!("Invalid settings TOML: {error}"))?;
    let Some(keyboard) = document.get("keyboard") else {
        return resolve(platform, preset).map_err(|error| error.to_string());
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
                if !matches!(name, "keys" | "context" | "command" | "platform") {
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
                "settings" => Context::Settings,
                "about" => Context::About,
                name => {
                    return Err(format!(
                        "{where_}: unknown context {name:?}; use global, reader, sidebar, contents, settings, or about"
                    ));
                }
            };
            let platform = match string("platform")? {
                None => None,
                Some("mac") => Some(Platform::Mac),
                Some("linux") => Some(Platform::Linux),
                Some("windows") => Some(Platform::Windows),
                Some(name) => {
                    return Err(format!(
                        "{where_}: unknown platform {name:?}; use mac, linux, or windows"
                    ));
                }
            };
            let command =
                string("command")?.ok_or_else(|| format!("{where_}.command is required"))?;
            let mut entry = Entry {
                keys: keys.into(),
                context,
                action: Some(command_action(command).ok_or_else(|| {
                    format!(
                        "{where_}: unknown command {command:?}; use one of: {}",
                        command_names().join(", ")
                    )
                })?),
                platform: None,
            };
            entry.platform = platform;
            // Validate even inactive platform entries so a typo isn't hidden
            // until the file is opened on another machine.
            parse(keys, platform.unwrap_or(Platform::Mac).os()).map_err(|_| {
                format!("{where_}: invalid keys {keys:?}; use GPUI keys such as cmd-o or g g")
            })?;
            entries.push(entry);
        }
    }
    let resolved = resolve_with(platform, preset, entries)
        .map_err(|error| format!("Invalid [keyboard] keymap: {error}"))?;
    if platform == Platform::Mac {
        let defaults = resolve(platform, preset).expect("embedded keyboard defaults are valid");
        for command in super::super::menu::ACCELERATOR_COMMANDS {
            let action = command_action(command).expect("native menu command is registered");
            let Some(default) = first_binding_for(&defaults, action.as_ref()) else {
                continue;
            };
            let Some(current) = first_binding_for(&resolved, action.as_ref()) else {
                continue;
            };
            let default_is_modified =
                default.sequence.len() == 1 && default.sequence[0].modifiers.modified();
            let current_is_bare =
                current.sequence.len() == 1 && !current.sequence[0].modifiers.modified();
            if default_is_modified && current_is_bare {
                return Err(format!(
                    "Rebinding the modified shortcut for {} would make {:?} a bare macOS menu accelerator; also rebind {:?} or keep a modified shortcut for this command",
                    action.name(),
                    current.entry.keys,
                    current.entry.keys
                ));
            }
        }
    }
    Ok(resolved)
}

fn first_binding_for<'a>(map: &'a ResolvedKeymap, action: &dyn Action) -> Option<&'a Slot> {
    map.slots.iter().find(|slot| {
        slot.entry
            .action
            .as_ref()
            .is_some_and(|candidate| candidate.partial_eq(action))
    })
}

macro_rules! commands {
    ($($key:literal => $action:expr),* $(,)?) => {
        fn command_action(name: &str) -> Option<Box<dyn Action>> {
            match name {
                $($key => Some(Box::new($action)),)*
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
        "quit" => Quit, "about-kver" => AboutKver, "check-for-updates" => CheckForUpdates,
        "show-settings" => ShowSettings, "close-settings" => CloseSettings,
        "close-about" => CloseAbout, "command-palette" => ShowCommandPalette,
        "show-contents" => ShowContents, "show-find" => ShowFind,
        "toggle-sidebar" => ToggleSidebar, "activate-outline" => ActivateOutline,
        "next-page" => NextPage, "previous-page" => PreviousPage,
        "scroll-down" => ScrollDown, "scroll-up" => ScrollUp,
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
        "show-marks" => ShowMarks, "set-local-mark" => ShowSetMark,
        "jump-local-mark" => ShowJumpMark, "delete-local-mark" => ShowDeleteMark
}
