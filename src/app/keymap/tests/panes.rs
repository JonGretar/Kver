use super::*;

const PLATFORMS: [Platform; 3] = [Platform::Mac, Platform::Linux, Platform::Windows];

#[test]
fn pane_strokes_parse_as_literal_backslash_and_control_tab() {
    for platform in PLATFORMS {
        for (keys, shift) in [(r"secondary-\", false), (r"secondary-shift-\", true)] {
            let strokes = parse(keys, platform.os()).unwrap();
            assert_eq!(strokes.len(), 1);
            assert_eq!(strokes[0].key, "\\");
            assert_eq!(strokes[0].modifiers.shift, shift);
            assert_eq!(strokes[0].modifiers.platform, platform == Platform::Mac);
            assert_eq!(strokes[0].modifiers.control, platform != Platform::Mac);
        }
        let stroke = &parse("ctrl-tab", platform.os()).unwrap()[0];
        assert_eq!(stroke.key, "tab");
        assert!(stroke.modifiers.control);
        assert!(!stroke.modifiers.platform);
    }
}

#[test]
fn pane_defaults_are_conflict_free_and_reader_only() {
    for platform in PLATFORMS {
        for preset in [Preset::Standard, Preset::Vim] {
            // The complete resolver rejects duplicate slots and prefix conflicts.
            let map = resolve_config("", platform, preset).unwrap();
            let mut expected = vec![("ctrl-tab", SwitchPane.name())];
            if preset == Preset::Standard {
                expected.extend([
                    (r"secondary-\", SplitSideBySide.name()),
                    (r"secondary-shift-\", SplitStacked.name()),
                ]);
                assert!(map.shortcuts(&ClosePane, Context::Reader).is_empty());
                assert!(map.shortcuts(&KeepOnlyPane, Context::Reader).is_empty());
            } else {
                expected.extend([
                    ("g w v", SplitSideBySide.name()),
                    ("g w s", SplitStacked.name()),
                    ("g w w", SwitchPane.name()),
                    ("g w c", ClosePane.name()),
                    ("g w o", KeepOnlyPane.name()),
                ]);
                // The shared g prefix and single-letter commands stay intact.
                assert_eq!(
                    map.action("g g", Context::Reader),
                    Some(GoToDocumentStart.name())
                );
                assert_eq!(map.action("w", Context::Reader), Some(FitWidth.name()));
                assert_eq!(map.action("s", Context::Reader), Some(SinglePage.name()));
                assert_eq!(map.action("c", Context::Reader), Some(Continuous.name()));
            }
            for (keys, action) in expected {
                assert_eq!(map.action(keys, Context::Reader), Some(action));
                for context in [
                    Context::Global,
                    Context::Sidebar,
                    Context::Contents,
                    Context::Annotations,
                    Context::Settings,
                    Context::About,
                ] {
                    assert_eq!(map.action(keys, context), None);
                }
            }
        }
    }
}

#[test]
fn pane_overrides_replace_slots_and_effective_hints() {
    for platform in PLATFORMS {
        for preset in [Preset::Standard, Preset::Vim] {
            let default = if preset == Preset::Standard {
                r"secondary-\"
            } else {
                "g w v"
            };
            let config = format!(
                "[[keyboard.bindings]]\nkeys = '{default}'\ncontext = 'reader'\ncommand = 'split-stacked'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-alt-v'\ncontext = 'reader'\ncommand = 'split-side-by-side'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-alt-c'\ncontext = 'reader'\ncommand = 'close-pane'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-alt-o'\ncontext = 'reader'\ncommand = 'keep-only-pane'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-alt-w'\ncontext = 'reader'\ncommand = 'switch-pane'\n"
            );
            let map = resolve_config(&config, platform, preset).unwrap();
            assert_eq!(
                map.action(default, Context::Reader),
                Some(SplitStacked.name())
            );
            assert_eq!(
                map.shortcuts(&SplitSideBySide, Context::Reader),
                ["ctrl-alt-v"]
            );
            for (action, hint) in [
                (&ClosePane as &dyn Action, "ctrl-alt-c"),
                (&KeepOnlyPane, "ctrl-alt-o"),
                (&SwitchPane, "ctrl-alt-w"),
            ] {
                assert_eq!(map.shortcuts(action, Context::Reader).last(), Some(&hint));
            }
            assert!(resolve_config(
                "[[keyboard.bindings]]\nkeys = 'g w'\ncontext = 'reader'\ncommand = 'close-pane'\n",
                platform,
                Preset::Vim,
            ).err().unwrap().contains("ambiguous sequences"));
        }
    }
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn pane_native_menus_preserve_availability_and_never_install_key_equivalents(
    cx: &mut TestAppContext,
) {
    use crate::app::menu;
    use gpui_kit::OwnedMenuItem;

    cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx);
        menu::install(cx);
        menu::set_pane_availability(cx, true, false, true);
        menu::refresh_navigation(cx, true, false);
        for preset in [Preset::Standard, Preset::Vim] {
            reload(cx, None, preset).unwrap();
            let view = cx
                .get_menus()
                .unwrap()
                .into_iter()
                .find(|menu| menu.name == "View")
                .unwrap();
            let mut count = 0;
            for item in view.items {
                if let OwnedMenuItem::Action {
                    name,
                    action,
                    disabled,
                    ..
                } = item
                    && matches!(
                        name.as_ref(),
                        "Split side by side"
                            | "Split top and bottom"
                            | "Switch pane"
                            | "Close pane"
                            | "Keep only this pane"
                    )
                {
                    count += 1;
                    assert_eq!(disabled, name == "Split top and bottom");
                    assert_eq!(action.name(), "kver::NativeMenuAction");
                    assert_eq!(
                        cx.key_bindings()
                            .borrow()
                            .bindings_for_action(action.as_ref())
                            .count(),
                        0
                    );
                }
            }
            assert_eq!(count, 5);
        }
    });
}
