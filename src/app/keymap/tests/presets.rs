//! Embedded policy exercised through the same effective resolver as live config.
use super::*;

const PLATFORMS: [Platform; 3] = [Platform::Mac, Platform::Linux, Platform::Windows];
const PRESETS: [Preset; 2] = [Preset::Standard, Preset::Vim];

#[test]
fn complete_presets_keep_save_marks_and_platform_modifier_semantics() {
    for platform in PLATFORMS {
        for preset in PRESETS {
            let map = resolve_config("", platform, preset).unwrap();
            for context in [Context::Reader, Context::Annotations, Context::Settings] {
                assert_eq!(map.action("cmd-s", context), Some(Save.name()));
                assert_eq!(map.action("cmd-shift-s", context), Some(SaveAs.name()));
            }
            // Preserve literal cmd policy on non-Mac platforms for #178.
            assert_eq!(map.action("ctrl-s", Context::Reader), None);
            assert_eq!(map.action("cmd-o", Context::Reader), Some(OpenPdf.name()));
            assert_eq!(
                map.action("ctrl-o", Context::Reader),
                Some(NavigateBack.name())
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
                (platform != Platform::Windows).then_some(Quit.name())
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
                    map.action("cmd-alt-right", Context::Reader),
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
