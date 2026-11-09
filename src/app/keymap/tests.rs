use super::*;
#[cfg(target_os = "macos")]
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{KeyContext, TestAppContext};
use std::collections::HashSet;

#[test]
fn later_layers_replace_a_key_slot_and_unbind_masks_a_global_key() {
    let map = resolve_with(
        Platform::Mac,
        Preset::Standard,
        [
            Entry::bind("cmd-o", Context::Global, NewWindow),
            Entry::unbind("cmd-o", Context::Reader),
        ],
    )
    .unwrap();
    assert_eq!(
        map.action("cmd-o", Context::Global),
        Some("kver::NewWindow")
    );
    assert_eq!(map.action("cmd-o", Context::Reader), None);
}

#[test]
fn toml_overrides_aliases_and_platform_precedence() {
    let content = r#"
[reader]
initial_zoom = "fit-width"
[[keyboard.bindings]]
keys = "right"
context = "reader"
command = "previous-page"
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "next-page"
platform = "mac"
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "previous-page"
platform = "linux"
"#;
    let mac = resolve_config(content, Platform::Mac, Preset::Standard).unwrap();
    assert_eq!(
        mac.action("right", Context::Reader),
        Some("kver::PreviousPage")
    );
    assert_eq!(
        mac.action("ctrl-e", Context::Reader),
        Some("kver::NextPage")
    );
    assert_eq!(
        mac.action("cmd-alt-right", Context::Reader),
        Some("kver::NextPage")
    );
    assert_eq!(mac.action("cmd-o", Context::Reader), Some("kver::OpenPdf"));
    assert_eq!(
        mac.action("cmd-o", Context::Settings),
        Some("kver::OpenPdf")
    );
    let linux = resolve_config(content, Platform::Linux, Preset::Standard).unwrap();
    assert_eq!(
        linux.action("ctrl-e", Context::Reader),
        Some("kver::PreviousPage")
    );
}

#[test]
fn documented_g_t_sequence_coexists_with_vim_g_g() {
    let content = r#"
[[keyboard.bindings]]
keys = "g t"
context = "reader"
command = "document-start"
"#;
    let map = resolve_config(content, Platform::Mac, Preset::Vim).unwrap();
    assert_eq!(
        map.action("g t", Context::Reader),
        Some("kver::GoToDocumentStart")
    );
    assert_eq!(
        map.action("g g", Context::Reader),
        Some("kver::GoToDocumentStart")
    );
    assert_eq!(map.action("g t", Context::Settings), None);
}

#[test]
fn config_errors_point_to_the_entry_and_remedy() {
    for (entry, expected) in [
        ("command = \"unknown\"", "unknown command"),
        (
            "command = \"next-page\"\ncontext = \"editor\"",
            "unknown context",
        ),
        (
            "command = \"next-page\"\nplatform = \"macos\"",
            "unknown platform",
        ),
        ("command = \"next-page\"", "invalid keys"),
    ] {
        let keys = if expected == "invalid keys" {
            "not-a-key"
        } else {
            "ctrl-e"
        };
        let text = format!("[[keyboard.bindings]]\nkeys = \"{keys}\"\n{entry}\n");
        let error = resolve_config(&text, Platform::Mac, Preset::Standard)
            .err()
            .unwrap();
        assert!(error.contains(expected), "{error}");
    }
    let unmatched = "[[keyboard.unbind]]\nkeys = \"right\"\ncontext = \"reader\"\n";
    assert!(
        resolve_config(unmatched, Platform::Mac, Preset::Standard)
            .err()
            .unwrap()
            .contains("not supported yet")
    );
    let prefix =
        "[[keyboard.bindings]]\nkeys = \"g\"\ncontext = \"reader\"\ncommand = \"next-page\"\n";
    assert!(
        resolve_config(prefix, Platform::Mac, Preset::Vim)
            .err()
            .unwrap()
            .contains("ambiguous sequences")
    );
}

#[test]
fn mac_menu_rejects_promoting_bare_reader_keys_after_rebind() {
    for (preset, changed, bare) in [
        (Preset::Standard, "cmd-alt-right", "right"),
        (Preset::Vim, "cmd-alt-right", "n"),
        (Preset::Vim, "cmd-f", "/"),
        (Preset::Standard, "cmd-b", "escape"),
        (Preset::Vim, "cmd-o", "o"),
        (Preset::Vim, "cmd-shift-o", "shift-o"),
    ] {
        let config = format!(
            "[[keyboard.bindings]]\nkeys = \"{changed}\"\ncontext = \"{}\"\ncommand = \"zoom-in\"\n",
            if matches!(changed, "cmd-b" | "cmd-f" | "cmd-o" | "cmd-shift-o") {
                "global"
            } else {
                "reader"
            }
        );
        let error = resolve_config(&config, Platform::Mac, preset)
            .err()
            .unwrap();
        assert!(
            error.contains("typing-key macOS menu accelerator"),
            "{error}"
        );
        assert!(error.contains(bare), "{error}");
        assert!(resolve_config(&config, Platform::Linux, preset).is_ok());
    }
    // Moving both slots keeps the menu from inheriting an unintended bare key.
    let config = r#"
[[keyboard.bindings]]
keys = "cmd-alt-right"
context = "reader"
command = "zoom-in"
[[keyboard.bindings]]
keys = "right"
context = "reader"
command = "previous-page"
"#;
    let map = resolve_config(config, Platform::Mac, Preset::Standard).unwrap();
    assert_eq!(
        map.action("cmd-alt-right", Context::Reader),
        Some("kver::ZoomIn")
    );
    assert_eq!(
        map.action("right", Context::Reader),
        Some("kver::PreviousPage")
    );
}

#[test]
fn mac_menu_prefers_global_alias_and_rejects_a_typing_key() {
    // GPUI's native menu prefers a later global binding over an earlier
    // contextual one, so merely checking the first resolved slot misses this.
    let global = "[[keyboard.bindings]]\nkeys = 'x'\ncontext = 'global'\ncommand = 'next-page'\n";
    let error = resolve_config(global, Platform::Mac, Preset::Standard)
        .err()
        .unwrap();
    assert!(
        error.contains("typing-key macOS menu accelerator"),
        "{error}"
    );
    assert!(error.contains("restrict this entry to reader"), "{error}");
    let reader = global.replace("context = 'global'", "context = 'reader'");
    assert!(resolve_config(&reader, Platform::Mac, Preset::Standard).is_ok());

    // Vim already has a bare 's' as its Single Page fallback. An edit must
    // not promote a different global typing key in its place.
    let vim = "[[keyboard.bindings]]\nkeys = 'x'\ncontext = 'global'\ncommand = 'single-page'\n";
    assert!(
        resolve_config(vim, Platform::Mac, Preset::Vim)
            .err()
            .unwrap()
            .contains("typing-key macOS menu accelerator")
    );
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn reloaded_shortcut_dispatches_in_reader_but_not_in_palette(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
    });
    let (handle, document) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| {
                super::super::document_window::DocumentWindow::new(
                    Some("examples/readme-sample.pdf".into()),
                    window,
                    cx,
                )
            })
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        reload(
            cx,
            Some("[[keyboard.bindings]]\nkeys = \"ctrl-e\"\ncontext = \"reader\"\ncommand = \"next-page\"\n".into()),
            Preset::Standard,
        )
        .unwrap();
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-e", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 1);
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-shift-p", cx);
        window.render_frame(cx);
        window.press("ctrl-e", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 1);
        assert!(document.palette_open());
    });
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn unsafe_menu_edit_does_not_replace_live_shortcuts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
        let unsafe_edit = "[[keyboard.bindings]]\nkeys = \"cmd-alt-right\"\ncontext = \"reader\"\ncommand = \"zoom-in\"\n";
        assert!(reload(cx, Some(unsafe_edit.into()), Preset::Standard)
            .unwrap_err()
            .contains("typing-key macOS menu accelerator"));
        let bindings = cx.key_bindings();
        let bindings = bindings.borrow();
        let first = bindings.bindings_for_action(&NextPage).next().unwrap();
        assert_eq!(
            first.match_keystrokes(&parse("cmd-alt-right", Platform::Mac.os()).unwrap()),
            Some(false)
        );
    });
}

#[gpui_kit::test]
fn valid_reload_replaces_bindings_and_invalid_reload_keeps_last_valid(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
        // A component can register its own keys after the product keymap.
        cx.bind_keys([KeyBinding::new("ctrl-y", NewWindow, Some("Component"))]);
        let valid = "[[keyboard.bindings]]\nkeys = \"ctrl-e\"\ncontext = \"reader\"\ncommand = \"next-page\"\n";
        assert!(reload(cx, Some(valid.into()), Preset::Standard).unwrap());
        let keys = parse("ctrl-e", Platform::Mac.os()).unwrap();
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        assert_eq!(
            cx.key_bindings().borrow().bindings_for_input(&keys, &reader).0[0]
                .action().name(),
            "kver::NextPage"
        );
        assert!(reload(cx, Some("[[keyboard.bindings]]\nkeys = \"??\"\ncommand = \"bad\"\n".into()), Preset::Standard).is_err());
        assert_eq!(
            cx.key_bindings().borrow().bindings_for_input(&keys, &reader).0[0]
                .action().name(),
            "kver::NextPage"
        );
        assert!(reload(cx, None, Preset::Standard).unwrap());
        assert!(cx.key_bindings().borrow().bindings_for_input(&keys, &reader).0.is_empty());
        assert!(cx.key_bindings().borrow().bindings().any(|binding| {
            binding.match_keystrokes(&parse("ctrl-y", Platform::Mac.os()).unwrap()) == Some(false)
        }));
    });
}

#[test]
fn file_edits_recover_and_leave_unrelated_toml_intact() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let original = "# personal note\n[reader]\ninitial_zoom = \"fit_page\"\n";
    std::fs::write(&path, original).unwrap();
    let first = read_config(&path).unwrap().unwrap();
    assert_eq!(
        resolve_config(&first, Platform::Mac, Preset::Standard)
            .unwrap()
            .action("right", Context::Reader),
        Some("kver::NextPage")
    );
    let broken = format!(
        "{original}[[keyboard.bindings]]\nkeys = \"ctrl-e\"\ncommand = \"not-a-command\"\n"
    );
    std::fs::write(&path, broken).unwrap();
    assert!(
        resolve_config(
            &read_config(&path).unwrap().unwrap(),
            Platform::Mac,
            Preset::Standard
        )
        .is_err()
    );
    let fixed = format!(
        "{original}[[keyboard.bindings]]\nkeys = \"ctrl-e\"\ncontext = \"reader\"\ncommand = \"next-page\"\n"
    );
    std::fs::write(&path, &fixed).unwrap();
    let recovered = read_config(&path).unwrap().unwrap();
    assert_eq!(recovered, fixed);
    assert_eq!(
        resolve_config(&recovered, Platform::Mac, Preset::Standard)
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::NextPage")
    );
    assert_eq!(
        crate::settings::load(&path).unwrap().initial_zoom(),
        crate::settings::InitialZoom::FitPage
    );
    crate::settings::save(
        &path,
        crate::settings::SettingsChange::Appearance(crate::settings::Appearance::Dark),
    )
    .unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("# personal note"));
    assert_eq!(
        resolve_config(&saved, Platform::Mac, Preset::Standard)
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::NextPage")
    );
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn override_keeps_modified_native_menu_key_ahead_of_reader_alias(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
        reload(
            cx,
            Some("[[keyboard.bindings]]\nkeys = \"cmd-alt-right\"\ncontext = \"reader\"\ncommand = \"next-page\"\n".into()),
            Preset::Standard,
        ).unwrap();
        let bindings = cx.key_bindings();
        let bindings = bindings.borrow();
        let first = bindings.bindings_for_action(&NextPage).next().unwrap();
        assert_eq!(
            first.match_keystrokes(&parse("cmd-alt-right", Platform::Mac.os()).unwrap()),
            Some(false)
        );
    });
}

#[gpui_kit::test]
fn installed_policy_masks_global_keys_only_in_the_unbound_context(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::unbind("cmd-o", Context::Reader)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let keymap = cx.key_bindings();
        let key = parse("cmd-o", Platform::Mac.os()).unwrap();
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        let settings = [KeyContext::parse(key_context::SETTINGS).unwrap()];
        let bindings = keymap.borrow();
        assert!(bindings.bindings_for_input(&key, &reader).0.is_empty());
        assert_eq!(
            bindings.bindings_for_input(&key, &settings).0[0]
                .action()
                .name(),
            "kver::OpenPdf"
        );
    });
}

#[gpui_kit::test]
fn contents_unbind_masks_the_parent_sidebar_key(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::unbind("escape", Context::Contents)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let key = parse("escape", Platform::Mac.os()).unwrap();
        let sidebar = [KeyContext::parse(key_context::SIDEBAR).unwrap()];
        let contents = [
            KeyContext::parse(key_context::SIDEBAR).unwrap(),
            KeyContext::parse(key_context::CONTENTS).unwrap(),
        ];
        let keymap = cx.key_bindings();
        let bindings = keymap.borrow();
        assert_eq!(
            bindings.bindings_for_input(&key, &sidebar).0[0]
                .action()
                .name(),
            "kver::ToggleSidebar"
        );
        assert!(bindings.bindings_for_input(&key, &contents).0.is_empty());
    });
}

#[gpui_kit::test]
fn installed_contextual_action_wins_over_global_action(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::bind("cmd-o", Context::Reader, NextPage)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let key = parse("cmd-o", Platform::Mac.os()).unwrap();
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        let keymap = cx.key_bindings();
        assert_eq!(
            keymap.borrow().bindings_for_input(&key, &reader).0[0]
                .action()
                .name(),
            "kver::NextPage"
        );
    });
}

#[gpui_kit::test]
fn switching_presets_replaces_reader_bindings_without_losing_kit_or_app_keys(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let kit_bindings = cx.key_bindings().borrow().bindings().count();
        install(cx);
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        let assert_reader = |cx: &App, keys: &str, expected: &str| {
            let input = parse(keys, Platform::Mac.os()).unwrap();
            let map = cx.key_bindings();
            let bindings = map.borrow();
            assert_eq!(
                bindings.bindings_for_input(&input, &reader).0[0]
                    .action()
                    .name(),
                expected
            );
        };
        assert_reader(cx, "right", "kver::NextPage");
        select_preset(cx, Preset::Vim).unwrap();
        assert_eq!(active_preset(cx), Preset::Vim);
        assert_reader(cx, "right", "kver::ScrollRight");
        assert_reader(cx, "n", "kver::NextPage");
        assert_reader(cx, "cmd-o", "kver::OpenPdf");
        let vim_count = cx.key_bindings().borrow().bindings().count();
        select_preset(cx, Preset::Standard).unwrap();
        assert_eq!(active_preset(cx), Preset::Standard);
        assert_reader(cx, "right", "kver::NextPage");
        let input = parse("n", Platform::Mac.os()).unwrap();
        assert!(
            cx.key_bindings()
                .borrow()
                .bindings_for_input(&input, &reader)
                .0
                .is_empty()
        );
        assert_eq!(
            cx.key_bindings().borrow().bindings().count(),
            kit_bindings
                + resolve(platform(cx), Preset::Standard)
                    .unwrap()
                    .bindings(cx)
                    .len()
        );
        select_preset(cx, Preset::Vim).unwrap();
        assert_eq!(cx.key_bindings().borrow().bindings().count(), vim_count);
    });
}

#[test]
fn platform_filters_entries_before_duplicate_detection() {
    let overrides = || {
        [
            Entry::bind("ctrl-e", Context::Reader, NextPage).on(Platform::Mac),
            Entry::bind("ctrl-e", Context::Reader, PreviousPage).on(Platform::Linux),
        ]
    };
    assert_eq!(
        resolve_with(Platform::Mac, Preset::Standard, overrides())
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::NextPage"),
    );
    assert_eq!(
        resolve_with(Platform::Linux, Preset::Standard, overrides())
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::PreviousPage"),
    );
    for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
        for preset in [Preset::Standard, Preset::Vim] {
            let map = resolve(platform, preset).unwrap();
            assert_eq!(
                map.action("right", Context::Reader),
                Some(match preset {
                    Preset::Standard => "kver::NextPage",
                    Preset::Vim => "kver::ScrollRight",
                })
            );
            assert_eq!(map.action("cmd-o", Context::Global), Some("kver::OpenPdf"));
            assert_eq!(
                map.action("ctrl-o", Context::Reader),
                Some("kver::NavigateBack")
            );
            assert_eq!(
                map.action("ctrl-i", Context::Reader),
                Some("kver::NavigateForward")
            );
            assert_eq!(
                map.action("ctrl-o", Context::Contents),
                Some("kver::NavigateBack")
            );
            assert_eq!(
                map.action("ctrl-i", Context::Contents),
                Some("kver::NavigateForward")
            );
            assert_eq!(map.action("ctrl-i", Context::Sidebar), None);
            assert_eq!(map.action("ctrl-i", Context::Global), None);
        }
    }
}

#[test]
fn overlapping_prefixes_and_duplicate_slots_are_reported() {
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Vim,
            [Entry::bind("m", Context::Reader, NextPage)],
        )
        .err(),
        Some(Diagnostic::PrefixConflict {
            first: "m".into(),
            second: "m a".into(),
            context: Context::Reader,
        }),
    );
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [
                Entry::bind("ctrl-e", Context::Reader, NextPage),
                Entry::bind("ctrl-e", Context::Reader, ScrollDown),
            ],
        )
        .err(),
        Some(Diagnostic::DuplicateSlot {
            keys: "ctrl-e".into(),
            context: Context::Reader,
        }),
    );
    assert!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::bind("m", Context::Settings, NextPage)],
        )
        .is_ok()
    );
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [
                Entry::bind("ctrl-e", Context::Sidebar, NextPage),
                Entry::bind("ctrl-e p", Context::Contents, PreviousPage),
            ],
        )
        .err(),
        Some(Diagnostic::PrefixConflict {
            first: "ctrl-e".into(),
            second: "ctrl-e p".into(),
            context: Context::Contents,
        }),
    );
    for (key, continuation) in [("'", "' a"), ("d", "d m a"), ("d m", "d m a"), ("g", "g g")] {
        assert_eq!(
            resolve_with(
                Platform::Mac,
                Preset::Vim,
                [Entry::bind(key, Context::Reader, NextPage)],
            )
            .err(),
            Some(Diagnostic::PrefixConflict {
                first: key.into(),
                second: continuation.into(),
                context: Context::Reader,
            }),
            "{key}"
        );
    }
}

#[test]
fn defaults_have_no_duplicate_key_and_context_pairs() {
    let mut seen = HashSet::new();
    for slot in resolve(Platform::Mac, Preset::Standard).unwrap().slots {
        assert!(
            seen.insert((slot.sequence, slot.entry.context)),
            "duplicate slot: {}",
            slot.entry.keys
        );
        assert!(
            slot.entry.action.unwrap().name().starts_with("kver::"),
            "action identity changed: {}",
            slot.entry.keys
        );
    }
}

#[test]
fn every_mark_letter_has_set_jump_and_delete_bindings() {
    let map = resolve(Platform::Mac, Preset::Vim).unwrap();
    for letter in 'a'..='z' {
        let actions: [(Box<dyn gpui_kit::Action>, usize); 3] = [
            (Box::new(SetLocalMark(letter)), 2),
            (Box::new(JumpLocalMark(letter)), 2),
            (Box::new(DeleteLocalMark(letter)), 3),
        ];
        for (action, length) in actions {
            let matches: Vec<_> = map
                .slots
                .iter()
                .filter(|slot| {
                    slot.entry
                        .action
                        .as_ref()
                        .is_some_and(|binding| binding.partial_eq(action.as_ref()))
                })
                .collect();
            assert_eq!(matches.len(), 1, "{} for {letter}", action.name());
            assert_eq!(matches[0].sequence.len(), length);
            assert_eq!(matches[0].entry.context, Context::Reader);
        }
    }
}

#[test]
fn vim_defaults_inherit_native_commands_without_leaking_reader_keys_to_other_contexts() {
    for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
        let map = resolve(platform, Preset::Vim).unwrap();
        for (keys, action) in [
            ("cmd-w", "kver::CloseWindow"),
            ("cmd-n", "kver::NewWindow"),
            ("cmd-o", "kver::OpenPdf"),
            ("cmd-f", "kver::ShowFind"),
        ] {
            assert_eq!(map.action(keys, Context::Global), Some(action));
            assert_eq!(map.action(keys, Context::Reader), Some(action));
        }
        for (keys, action) in [
            ("j", "kver::ScrollDown"),
            ("k", "kver::ScrollUp"),
            ("h", "kver::ScrollLeft"),
            ("l", "kver::ScrollRight"),
            ("left", "kver::ScrollLeft"),
            ("right", "kver::ScrollRight"),
            ("space", "kver::ScrollHalfScreenDown"),
            ("shift-space", "kver::ScrollHalfScreenUp"),
            ("g g", "kver::GoToDocumentStart"),
            ("shift-g", "kver::GoToDocumentEnd"),
            ("n", "kver::NextPage"),
            ("p", "kver::PreviousPage"),
            ("o", "kver::OpenPdf"),
            ("shift-o", "kver::ShowRecent"),
            ("/", "kver::ShowFind"),
            (":", "kver::ShowCommandPalette"),
            ("w", "kver::FitWidth"),
            ("f", "kver::FitPage"),
            ("s", "kver::SinglePage"),
            ("c", "kver::Continuous"),
        ] {
            assert_eq!(map.action(keys, Context::Reader), Some(action), "{keys}");
            assert_eq!(map.action(keys, Context::Contents), None, "{keys}");
            assert_eq!(map.action(keys, Context::Global), None, "{keys}");
        }
        for key in ["o", "shift-o", "/", ":"] {
            assert_eq!(map.action(key, Context::Sidebar), None, "{key}");
            assert_eq!(map.action(key, Context::Settings), None, "{key}");
        }
        let standard = resolve(platform, Preset::Standard).unwrap();
        for key in ["o", "shift-o", "/", ":"] {
            assert_eq!(standard.action(key, Context::Reader), None, "{key}");
        }
        assert_eq!(
            map.action("ctrl-o", Context::Reader),
            Some("kver::NavigateBack")
        );
        assert_eq!(
            map.action("ctrl-i", Context::Reader),
            Some("kver::NavigateForward")
        );
        assert_eq!(
            map.action("m a", Context::Reader),
            Some("kver::SetLocalMark")
        );
        assert_eq!(
            map.action("' a", Context::Reader),
            Some("kver::JumpLocalMark")
        );
        assert_eq!(
            map.action("d m a", Context::Reader),
            Some("kver::DeleteLocalMark")
        );
    }
    let mac = resolve(Platform::Mac, Preset::Vim).unwrap();
    assert_eq!(mac.action("cmd-q", Context::Global), Some("kver::Quit"));
    assert_eq!(
        mac.action("cmd-`", Context::Global),
        Some("kver::NextWindow")
    );
    assert_eq!(
        mac.action("cmd-shift-`", Context::Global),
        Some("kver::PreviousWindow")
    );
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn vim_install_keeps_native_menu_accelerators_modified(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install_preset(cx, Preset::Vim);
        let bindings = cx.key_bindings();
        let bindings = bindings.borrow();
        for (action, keys) in [
            (&NextPage as &dyn gpui_kit::Action, "cmd-alt-right"),
            (&PreviousPage as &dyn gpui_kit::Action, "cmd-alt-left"),
            (&CloseWindow as &dyn gpui_kit::Action, "cmd-w"),
            (&Quit as &dyn gpui_kit::Action, "cmd-q"),
        ] {
            let first = bindings.bindings_for_action(action).next().unwrap();
            assert_eq!(
                first.match_keystrokes(&parse(keys, Platform::Mac.os()).unwrap()),
                Some(false),
            );
        }
    });
}

#[test]
fn standard_defaults_keep_application_keys_and_use_contextual_reader_keys() {
    assert_eq!(Preset::default(), Preset::Standard);
    for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
        let map = resolve(platform, Preset::Standard).unwrap();
        for (keys, action) in [
            ("cmd-o", "kver::OpenPdf"),
            ("cmd-n", "kver::NewWindow"),
            ("cmd-w", "kver::CloseWindow"),
            ("cmd-f", "kver::ShowFind"),
            ("cmd-t", "kver::ShowContents"),
        ] {
            assert_eq!(map.action(keys, Context::Global), Some(action));
        }
        for (keys, action) in [
            ("down", "kver::ScrollDown"),
            ("up", "kver::ScrollUp"),
            ("pagedown", "kver::ScrollScreenDown"),
            ("pageup", "kver::ScrollScreenUp"),
            ("right", "kver::NextPage"),
            ("left", "kver::PreviousPage"),
            ("cmd-=", "kver::ZoomIn"),
            ("cmd--", "kver::ZoomOut"),
            ("cmd-alt-w", "kver::FitWidth"),
            ("cmd-alt-f", "kver::FitPage"),
            ("alt-left", "kver::NavigateBack"),
            ("alt-right", "kver::NavigateForward"),
        ] {
            assert_eq!(map.action(keys, Context::Reader), Some(action), "{keys}");
        }
        for key in [
            "j", "k", "n", "p", "w", "f", "s", "c", "m a", "' a", "d m a",
        ] {
            assert_eq!(map.action(key, Context::Reader), None, "{key}");
        }
        assert_eq!(
            map.action("ctrl-o", Context::Contents),
            Some("kver::NavigateBack")
        );
        assert_eq!(
            map.action("ctrl-i", Context::Contents),
            Some("kver::NavigateForward")
        );
        assert_eq!(
            map.action("alt-left", Context::Contents),
            Some("kver::NavigateBack")
        );
        assert_eq!(
            map.action("alt-right", Context::Contents),
            Some("kver::NavigateForward")
        );
        assert_eq!(map.action("alt-left", Context::Sidebar), None);
        assert_eq!(map.action("pagedown", Context::Sidebar), None);
        match platform {
            Platform::Mac => {
                assert_eq!(
                    map.action("cmd-`", Context::Global),
                    Some("kver::NextWindow")
                );
                assert_eq!(
                    map.action("alt-down", Context::Reader),
                    Some("kver::NextPage")
                );
                assert_eq!(map.action("cmd-q", Context::Global), Some("kver::Quit"));
            }
            Platform::Linux | Platform::Windows => {
                assert_eq!(
                    map.action("ctrl-f6", Context::Global),
                    Some("kver::NextWindow")
                );
                assert_eq!(
                    map.action("ctrl-pagedown", Context::Reader),
                    Some("kver::NextPage")
                );
                assert_eq!(map.action("cmd-`", Context::Global), None);
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn native_page_menu_uses_modified_resolved_keys_not_bare_reader_arrows(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
        let bindings = cx.key_bindings();
        let bindings = bindings.borrow();
        for (action, keys) in [
            (&NextPage as &dyn gpui_kit::Action, "cmd-alt-right"),
            (&PreviousPage as &dyn gpui_kit::Action, "cmd-alt-left"),
        ] {
            let first = bindings.bindings_for_action(action).next().unwrap();
            assert_eq!(
                first.match_keystrokes(&parse(keys, Platform::Mac.os()).unwrap()),
                Some(false)
            );
            assert!(first.predicate().is_some());
        }
    });
}
