//! Embedded policy exercised through the same effective resolver as live config.
use super::*;

const PLATFORMS: [Platform; 3] = [Platform::Mac, Platform::Linux, Platform::Windows];
const PRESETS: [Preset; 2] = [Preset::Standard, Preset::Vim];

#[test]
fn follow_links_are_reader_scoped_in_every_preset_and_platform() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for (key, action) in [
                ("f", "kver::FollowLink"),
                ("shift-f", "kver::FollowLinkInNewWindow"),
            ] {
                assert_eq!(map.action(key, Context::Reader), Some(action));
                for context in [
                    Context::Global,
                    Context::Sidebar,
                    Context::Contents,
                    Context::Annotations,
                ] {
                    assert_eq!(map.action(key, context), None);
                }
            }
            let map = resolve_config(
                "[[keyboard.bindings]]\nkeys = 'q'\ncontext = 'reader'\ncommand = 'follow-link-in-new-window'\n",
                platform, preset,
            ).unwrap();
            assert_eq!(
                map.action("q", Context::Reader),
                Some("kver::FollowLinkInNewWindow")
            );
        }
    }
}

#[test]
fn visual_select_is_reader_scoped_and_customizable_in_both_presets() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            assert_eq!(
                map.action("v", Context::Reader),
                (preset == Preset::Vim).then_some("kver::VisualSelect")
            );
            assert_eq!(
                map.action("shift-v", Context::Reader),
                (preset == Preset::Vim).then_some("kver::VisualSelectLine")
            );
            for context in [
                Context::Global,
                Context::Sidebar,
                Context::Contents,
                Context::Annotations,
            ] {
                assert_eq!(map.action("v", context), None);
                assert_eq!(map.action("shift-v", context), None);
            }
            let map = resolve_config(
                "[[keyboard.bindings]]\nkeys = 'q'\ncontext = 'reader'\ncommand = 'visual-select'\n",
                platform,
                preset,
            ).unwrap();
            assert!(
                map.shortcuts(&crate::reader::VisualSelect, Context::Reader)
                    .contains(&"q")
            );
            let map = resolve_config(
                "[[keyboard.bindings]]\nkeys = 'q'\ncontext = 'reader'\ncommand = 'visual-select-line'\n\
                 [[keyboard.bindings]]\nkeys = 'shift-v'\ncontext = 'reader'\ncommand = 'visual-select'\n",
                platform,
                preset,
            ).unwrap();
            assert!(
                map.shortcuts(&crate::reader::VisualSelectLine, Context::Reader)
                    .contains(&"q")
            );
            assert_eq!(
                map.action("shift-v", Context::Reader),
                Some("kver::VisualSelect")
            );
            assert_eq!(
                map.action("v", Context::Reader),
                (preset == Preset::Vim).then_some("kver::VisualSelect")
            );
        }
    }
}

#[test]
fn complete_presets_keep_save_marks_and_platform_modifier_semantics() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for context in [Context::Reader, Context::Annotations, Context::Settings] {
                assert_eq!(map.action("secondary-s", context), Some(Save.name()));
                assert_eq!(
                    map.action("secondary-shift-s", context),
                    Some(SaveAs.name())
                );
            }
            assert_eq!(
                map.action("ctrl-o", Context::Reader),
                Some(if platform == Platform::Mac || preset == Preset::Vim {
                    NavigateBack.name()
                } else {
                    OpenPdf.name()
                })
            );
            let window_key = if platform == Platform::Mac {
                "cmd-`"
            } else {
                "ctrl-f6"
            };
            assert_eq!(
                map.action(window_key, Context::Global),
                Some(NextWindow.name())
            );
            assert_eq!(
                map.action("cmd-q", Context::Global),
                (platform == Platform::Mac).then_some(Quit.name())
            );
            assert_eq!(
                map.action("ctrl-q", Context::Global),
                (platform == Platform::Linux).then_some(Quit.name())
            );
            if preset == Preset::Standard {
                let annotation_key = if platform == Platform::Mac {
                    "cmd-enter"
                } else {
                    "ctrl-enter"
                };
                assert_eq!(
                    map.action(annotation_key, Context::Reader),
                    Some("kver::ApplyMarkup")
                );
                assert_eq!(map.action("g g", Context::Reader), None);
                assert_eq!(map.action("a h", Context::Reader), None);
            } else {
                assert_eq!(map.action("cmd-f", Context::Reader), None);
                assert_eq!(map.action("a h", Context::Reader), Some(Highlight.name()));
            }
            for letter in 'a'..='z' {
                for (keys, action) in [
                    (
                        format!("m {letter}"),
                        Box::new(SetLocalMark(letter)) as Box<dyn Action>,
                    ),
                    (format!("' {letter}"), Box::new(JumpLocalMark(letter))),
                    (format!("d m {letter}"), Box::new(DeleteLocalMark(letter))),
                ] {
                    let slot = map.slots.iter().find(|slot| slot.entry.keys == keys);
                    if preset == Preset::Vim {
                        let slot = slot.expect("complete letter-specific mark policy");
                        assert_eq!(slot.entry.context, Context::Reader);
                        assert!(
                            slot.entry
                                .action
                                .as_ref()
                                .unwrap()
                                .partial_eq(action.as_ref())
                        );
                        assert_eq!(map.action(&keys, Context::Settings), None);
                    } else {
                        assert!(slot.is_none(), "Standard must not fall back to Vim");
                    }
                }
            }
        }
    }
}

#[test]
fn filters_use_any_of_semantics_before_replacement_and_alias_resolution() {
    for filter in [
        "",
        "platform = ['mac', 'linux', 'windows']",
        "platform = ['mac', 'mac', 'linux', 'windows']",
    ] {
        let text = format!(
            r#"
[[keyboard.bindings]]
keys = "right"
context = "reader"
command = "previous-page"
{filter}
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "next-page"
platform = ["linux", "windows"]
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "previous-page"
platform = "mac"
"#
        );
        for platform in PLATFORMS {
            for preset in PRESETS {
                let map = resolve_config(&text, platform, preset).unwrap();
                assert_eq!(
                    map.action("right", Context::Reader),
                    Some(PreviousPage.name())
                );
                assert_eq!(
                    map.action("ctrl-e", Context::Reader),
                    Some(if platform == Platform::Mac {
                        PreviousPage.name()
                    } else {
                        NextPage.name()
                    })
                );
                assert_eq!(
                    map.action(
                        if platform == Platform::Mac {
                            "cmd-alt-right"
                        } else {
                            "ctrl-pagedown"
                        },
                        Context::Reader,
                    ),
                    Some(NextPage.name())
                );
                assert_eq!(map.action("ctrl-e", Context::Settings), None);
                assert!(
                    map.shortcuts(&PreviousPage, Context::Reader)
                        .contains(&"right")
                );
            }
        }
    }
    for (name, only) in [
        ("mac", Platform::Mac),
        ("linux", Platform::Linux),
        ("windows", Platform::Windows),
    ] {
        for filter in [format!("'{name}'"), format!("['{name}']")] {
            let text = format!(
                "[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = 'next-page'\nplatform = {filter}"
            );
            for platform in PLATFORMS {
                for preset in PRESETS {
                    let map = resolve_config(&text, platform, preset).unwrap();
                    assert_eq!(
                        map.action("ctrl-e", Context::Reader),
                        (platform == only).then_some(NextPage.name())
                    );
                }
            }
        }
    }
}

#[test]
fn application_shortcuts_use_actual_command_or_control_without_meta_leakage() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for (suffix, action) in [
                ("o", &OpenPdf as &dyn Action),
                ("shift-o", &ShowRecent),
                ("n", &NewWindow),
                ("w", &CloseWindow),
                (",", &ShowSettings),
                ("shift-p", &ShowCommandPalette),
                ("shift-m", &ShowMarks),
                ("s", &Save),
                ("shift-s", &SaveAs),
            ] {
                let modifier = if platform == Platform::Mac {
                    "cmd"
                } else {
                    "ctrl"
                };
                let keys = format!("{modifier}-{suffix}");
                assert_eq!(map.action(&keys, Context::Global), Some(action.name()));
                let slot = config::menu_binding_for(&map, action).unwrap();
                let modifiers = slot.sequence[0].modifiers;
                assert_eq!(
                    modifiers.platform,
                    platform == Platform::Mac,
                    "{platform:?}/{preset:?}/{keys}"
                );
                assert_eq!(
                    modifiers.control,
                    platform != Platform::Mac,
                    "{platform:?}/{preset:?}/{keys}"
                );
                assert!(!modifiers.alt && !modifiers.function);
                assert_eq!(modifiers.shift, suffix.starts_with("shift-"));
                let other = if platform == Platform::Mac {
                    "ctrl"
                } else {
                    "cmd"
                };
                assert_eq!(
                    map.action(&format!("{other}-{suffix}"), Context::Global),
                    None
                );
            }
            // No shipped shortcut on Linux/Windows may require Super/Windows.
            if platform != Platform::Mac {
                assert!(map.slots.iter().all(|slot| {
                    slot.sequence
                        .iter()
                        .all(|stroke| !stroke.modifiers.platform)
                }));
            }
        }
    }
}

#[test]
fn contextual_open_navigation_and_find_policy_is_deliberate() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for context in [Context::Reader, Context::Contents] {
                let history = platform == Platform::Mac || preset == Preset::Vim;
                assert_eq!(
                    map.action("ctrl-o", context),
                    Some(if history {
                        NavigateBack.name()
                    } else {
                        OpenPdf.name()
                    })
                );
                assert_eq!(
                    map.action("ctrl-i", context),
                    history.then_some(NavigateForward.name())
                );
                if preset == Preset::Standard {
                    assert_eq!(map.action("alt-left", context), Some(NavigateBack.name()));
                    assert_eq!(
                        map.action("alt-right", context),
                        Some(NavigateForward.name())
                    );
                }
                let open_hints = map.shortcuts(&OpenPdf, context);
                if platform != Platform::Mac && preset == Preset::Vim {
                    assert!(
                        !open_hints.contains(&"ctrl-o"),
                        "shadowed Open must not be advertised"
                    );
                }
            }
            for context in [
                Context::Global,
                Context::Sidebar,
                Context::Annotations,
                Context::Settings,
                Context::About,
            ] {
                assert_eq!(
                    map.action("ctrl-o", context),
                    (platform != Platform::Mac).then_some(OpenPdf.name())
                );
                assert_eq!(map.action("ctrl-i", context), None);
            }
            if preset == Preset::Vim {
                assert_eq!(map.action("o", Context::Reader), Some(OpenPdf.name()));
                assert_eq!(
                    map.action("shift-o", Context::Reader),
                    Some(ShowRecent.name())
                );
                assert_eq!(
                    map.action("ctrl-f", Context::Reader),
                    Some(ScrollScreenDown.name())
                );
                assert_eq!(
                    map.action("ctrl-b", Context::Reader),
                    Some(ScrollScreenUp.name())
                );
                for context in [Context::Sidebar, Context::Settings, Context::Global] {
                    for key in ["o", "/", "ctrl-f", "ctrl-b", "g c"] {
                        assert_eq!(map.action(key, context), None);
                    }
                }
                assert_eq!(
                    map.action("secondary-f", Context::Reader),
                    (platform != Platform::Mac).then_some(ScrollScreenDown.name())
                );
                assert_eq!(
                    map.action("g c", Context::Reader),
                    Some(ShowContents.name())
                );
            } else {
                for context in [
                    Context::Reader,
                    Context::Sidebar,
                    Context::Contents,
                    Context::Settings,
                ] {
                    for (key, command) in [
                        ("secondary-f", "kver::ShowFind"),
                        ("secondary-b", "kver::ToggleSidebar"),
                        ("secondary-g", "kver::NextMatch"),
                        ("secondary-shift-g", "kver::PreviousMatch"),
                        ("secondary-t", "kver::ShowContents"),
                    ] {
                        assert_eq!(map.action(key, context), Some(command));
                    }
                    assert_eq!(
                        map.action("f3", context),
                        (platform != Platform::Mac).then_some(NextMatch.name())
                    );
                    assert_eq!(
                        map.action("shift-f3", context),
                        (platform != Platform::Mac).then_some(PreviousMatch.name())
                    );
                }
            }
        }
    }
}

#[test]
fn reading_and_annotation_shortcuts_filter_genuine_os_differences() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for context in [Context::Reader, Context::Annotations] {
                assert_eq!(
                    map.action("secondary-z", context),
                    Some("kver::UndoAnnotation")
                );
                let redo = match platform {
                    Platform::Mac => "cmd-shift-z",
                    Platform::Linux => "ctrl-shift-z",
                    Platform::Windows => "ctrl-y",
                };
                assert_eq!(map.action(redo, context), Some("kver::RedoAnnotation"));
                assert_eq!(
                    map.action("ctrl-y", context),
                    (platform == Platform::Windows).then_some("kver::RedoAnnotation")
                );
                assert_eq!(
                    map.action("ctrl-shift-z", context),
                    (platform == Platform::Linux).then_some("kver::RedoAnnotation")
                );
                assert_eq!(map.action("secondary-c", context), Some(Copy.name()));
                for key in ["backspace", "delete"] {
                    assert_eq!(map.action(key, context), Some(DeleteHighlight.name()));
                    assert_eq!(map.action(key, Context::Settings), None);
                }
                assert_eq!(map.action("secondary-z", Context::Settings), None);
            }
            let next = if platform == Platform::Mac {
                "cmd-alt-right"
            } else {
                "ctrl-pagedown"
            };
            let previous = if platform == Platform::Mac {
                "cmd-alt-left"
            } else {
                "ctrl-pageup"
            };
            assert_eq!(map.action(next, Context::Reader), Some(NextPage.name()));
            assert_eq!(
                map.action(previous, Context::Reader),
                Some(PreviousPage.name())
            );
            assert_eq!(map.action(next, Context::Sidebar), None);
            assert_eq!(
                map.shortcuts(&NextPage, Context::Reader).first(),
                Some(&next)
            );
            if platform != Platform::Mac {
                // Avoid Linux workspace shortcuts; do not translate Cmd-Option arrows.
                assert_eq!(map.action("ctrl-alt-right", Context::Reader), None);
                assert_eq!(map.action("cmd-alt-right", Context::Reader), None);
            }
            if preset == Preset::Standard {
                assert_eq!(
                    map.action("secondary-=", Context::Reader),
                    Some(ZoomIn.name())
                );
                assert_eq!(
                    map.action("secondary--", Context::Reader),
                    Some(ZoomOut.name())
                );
                assert_eq!(
                    map.action(
                        if platform == Platform::Mac {
                            "cmd-alt-f"
                        } else {
                            "ctrl-0"
                        },
                        Context::Reader
                    ),
                    Some(FitPage.name())
                );
                for (key, command) in [
                    ("secondary-enter", "kver::ApplyMarkup"),
                    ("secondary-shift-h", "kver::Highlight"),
                    ("secondary-shift-u", "kver::Underline"),
                    ("secondary-shift-x", "kver::StrikeOut"),
                    ("secondary-alt-u", "kver::Squiggly"),
                ] {
                    assert_eq!(map.action(key, Context::Reader), Some(command));
                    assert_eq!(map.action(key, Context::Sidebar), None);
                }
            }
        }
    }
}

#[test]
fn overrides_replace_contextual_slots_and_refresh_effective_hints_on_every_os() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config(
                "[[keyboard.bindings]]\nkeys = 'secondary-o'\ncontext = 'reader'\ncommand = 'next-page'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = 'open-pdf'\nplatform = ['mac', 'linux', 'windows']",
                platform, preset,
            ).unwrap();
            assert_eq!(
                map.action("secondary-o", Context::Reader),
                Some(NextPage.name())
            );
            assert_eq!(
                map.action("secondary-o", Context::Settings),
                Some(OpenPdf.name())
            );
            assert_eq!(map.action("ctrl-e", Context::Reader), Some(OpenPdf.name()));
            assert_eq!(map.action("ctrl-e", Context::Settings), None);
            let hints = map.shortcuts(&OpenPdf, Context::Reader);
            assert!(!hints.contains(&"cmd-o") && !hints.contains(&"ctrl-o"));
            assert!(hints.contains(&"ctrl-e"));
            assert_eq!(hints.contains(&"o"), preset == Preset::Vim);
        }
    }
}

#[test]
fn malformed_platform_filters_report_entry_field_and_remedy_on_every_os() {
    for (filter, diagnostic) in [
        ("[]", "omit it to match all platforms"),
        ("'macos'", "unknown platform"),
        ("['linux', 'macos']", "unknown platform"),
        ("42", "string or a nonempty array"),
        ("true", "string or a nonempty array"),
        ("{ mac = true }", "string or a nonempty array"),
        ("['linux', 1]", "platform[2] must be a platform string"),
        ("[['mac']]", "platform[1] must be a platform string"),
    ] {
        let text = format!(
            "[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncommand = 'next-page'\nplatform = {filter}"
        );
        for platform in PLATFORMS {
            for preset in PRESETS {
                let error = resolve_config(&text, platform, preset).err().unwrap();
                assert!(
                    error.contains("[[keyboard.bindings]] entry 1.platform"),
                    "{error}"
                );
                assert!(error.contains(diagnostic), "{error}");
            }
        }
    }
}

#[test]
fn overlapping_filters_validate_effective_slots_and_context_prefixes() {
    let duplicate = r#"
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "next-page"
platform = ["mac", "linux"]
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "previous-page"
platform = "linux"
"#;
    let prefix = r#"
[[keyboard.bindings]]
keys = "ctrl-e"
context = "sidebar"
command = "next-page"
platform = ["mac", "linux"]
[[keyboard.bindings]]
keys = "ctrl-e p"
context = "contents"
command = "previous-page"
platform = ["linux", "windows"]
"#;
    for (text, diagnostic) in [
        (duplicate, "conflicting entries"),
        (prefix, "ambiguous sequences"),
    ] {
        for platform in PLATFORMS {
            for preset in PRESETS {
                let result = resolve_config(text, platform, preset);
                if platform == Platform::Linux {
                    assert!(result.err().unwrap().contains(diagnostic));
                } else {
                    assert!(result.is_ok());
                }
            }
        }
    }
    // Sibling contexts are intentionally allowed to give a key different meanings.
    let siblings = duplicate
        .replace("platform = \"linux\"", "platform = ['mac', 'linux']\n")
        .replacen("context = \"reader\"", "context = \"settings\"", 1);
    for platform in PLATFORMS {
        for preset in PRESETS {
            assert!(resolve_config(&siblings, platform, preset).is_ok());
        }
    }
}

#[test]
fn local_mark_arguments_share_the_schema_without_changing_chooser_commands() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            for (command, chooser, direct) in [
                (
                    "set-local-mark",
                    Box::new(ShowSetMark) as Box<dyn Action>,
                    Box::new(SetLocalMark('z')) as Box<dyn Action>,
                ),
                (
                    "jump-local-mark",
                    Box::new(ShowJumpMark),
                    Box::new(JumpLocalMark('z')),
                ),
                (
                    "delete-local-mark",
                    Box::new(ShowDeleteMark),
                    Box::new(DeleteLocalMark('z')),
                ),
            ] {
                for (argument, expected) in [("", &chooser), ("\nletter = 'z'", &direct)] {
                    let text = format!(
                        "[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = '{command}'{argument}"
                    );
                    let map = resolve_config(&text, platform, preset).unwrap();
                    let slot = map
                        .slots
                        .iter()
                        .find(|slot| slot.entry.keys == "ctrl-e")
                        .unwrap();
                    assert!(
                        slot.entry
                            .action
                            .as_ref()
                            .unwrap()
                            .partial_eq(expected.as_ref())
                    );
                }
            }
            for (command, argument, diagnostic) in [
                ("set-local-mark", "''", "one lowercase letter"),
                ("set-local-mark", "'aa'", "one lowercase letter"),
                ("set-local-mark", "'A'", "one lowercase letter"),
                ("set-local-mark", "'é'", "one lowercase letter"),
                ("set-local-mark", "1", "must be a string"),
                ("next-page", "'a'", "only supported for"),
            ] {
                let text = format!(
                    "[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncommand = '{command}'\nletter = {argument}"
                );
                let error = resolve_config(&text, platform, preset).err().unwrap();
                assert!(error.contains("entry 1.letter"), "{error}");
                assert!(error.contains(diagnostic), "{error}");
            }
        }
    }
}

#[test]
fn filtered_duplicates_compare_effective_modifiers_not_key_spelling() {
    for (modified, only) in [
        ("cmd", Platform::Mac),
        ("ctrl", Platform::Linux),
        ("ctrl", Platform::Windows),
    ] {
        let name = match only {
            Platform::Mac => "mac",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
        };
        let text = format!(
            r#"
[[keyboard.bindings]]
keys = "secondary-e"
context = "reader"
command = "next-page"
platform = ["mac", "linux", "windows"]
[[keyboard.bindings]]
keys = "{modified}-e"
context = "reader"
command = "previous-page"
platform = "{name}"
"#
        );
        for platform in PLATFORMS {
            for preset in PRESETS {
                let result = resolve_config(&text, platform, preset);
                if platform == only {
                    assert!(result.err().unwrap().contains("conflicting entries"));
                } else {
                    assert!(result.is_ok());
                }
            }
        }
    }
    for preset in PRESETS {
        let text = "[[keyboard.bindings]]\nkeys = 'not-a-key'\ncommand = 'next-page'\nplatform = ['linux', 'windows']";
        let error = resolve_config(text, Platform::Mac, preset).err().unwrap();
        assert!(error.contains("entry 1: invalid keys"), "{error}");
    }
}
