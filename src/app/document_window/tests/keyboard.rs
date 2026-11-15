//! Reader movement, Find, and native-menu routing under the Vim preset.
use super::*;
use crate::app::keymap::{self, Preset};

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, keys: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for key in keys.split_whitespace() {
            window.press(key, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn platform_shortcuts_switch_in_place_without_leaking_reader_typing(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(menu::install);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    let page = |cx: &TestAppContext| document.read_with(cx, |d, cx| d.reader.read(cx).page_index());
    for preset in [Preset::Standard, Preset::Vim, Preset::Standard] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        press(
            cx,
            handle,
            if preset == Preset::Standard {
                "secondary-f"
            } else {
                "/"
            },
        );
        let before = page(cx);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("find-query").focused(), Some(true));
            window.press("secondary-a", cx);
            window.input("oJma/aaus", cx);
            assert_eq!(window.find("find-query").value(), Some("oJma/aaus"));
        })
        .unwrap();
        assert_eq!(page(cx), before);
        document.read_with(cx, |d, cx| assert!(!d.reader.read(cx).is_dirty(cx)));
        press(cx, handle, "escape");
        press(
            cx,
            handle,
            if preset == Preset::Standard {
                "secondary-t"
            } else {
                "g c"
            },
        );
        cx.update_window(handle, |_, window, cx| {
            window.click(("contents-entry", 0usize), cx);
        })
        .unwrap();
        assert_eq!(page(cx), 2);
        // This is an actual dispatch assertion, not just a resolver lookup:
        // Standard non-Mac Open cannot shadow Vim's contextual history.
        let back = if preset == Preset::Vim || cfg!(target_os = "macos") {
            "ctrl-o"
        } else {
            "alt-left"
        };
        let forward = if preset == Preset::Vim || cfg!(target_os = "macos") {
            "ctrl-i"
        } else {
            "alt-right"
        };
        press(cx, handle, back);
        assert_eq!(page(cx), before);
        press(cx, handle, forward);
        assert_eq!(page(cx), 2);
        press(cx, handle, "escape");
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(crate::reader::GoToDocumentStart), cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn overrides_refresh_reader_hints_and_installed_accelerators_in_both_presets(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(menu::install);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| {
            keymap::reload(cx, Some(
                "[[keyboard.bindings]]\nkeys = 'secondary-s'\ncommand = 'save-as'\n\
                 [[keyboard.bindings]]\nkeys = 'secondary-shift-s'\ncommand = 'save'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = 'next-page'\n\
                 [[keyboard.bindings]]\nkeys = 'right'\ncontext = 'reader'\ncommand = 'previous-page'\n".into()
            ), preset).unwrap();
            let map = keymap::active_map(cx).unwrap();
            assert_eq!(map.shortcuts(&crate::reader::NextPage, keymap::Context::Reader).last(), Some(&"ctrl-e"));
            assert_eq!(map.shortcuts(&Save, keymap::Context::Reader), vec!["secondary-shift-s"]);
            assert!(map.shortcuts(&SaveAs, keymap::Context::Reader).contains(&"secondary-s"));
            let bindings = cx.key_bindings();
            let bindings = bindings.borrow();
            let first = bindings.bindings_for_action(&SaveAs).next().unwrap();
            let stroke = gpui_kit::Keystroke::parse_with_os("secondary-s", cx.platform_info().os).unwrap();
            assert_eq!(first.match_keystrokes(&[stroke]), Some(false));
        });
        let before = document.read_with(cx, |d, cx| d.reader.read(cx).page_index());
        press(cx, handle, "ctrl-e");
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader.read(cx).page_index(), before + 1)
        });
        press(cx, handle, "right");
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader.read(cx).page_index(), before)
        });
        press(cx, handle, "secondary-s");
        assert!(
            cx.did_prompt_for_new_path(),
            "override dispatches Save As, not Save"
        );
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();
        press(cx, handle, "secondary-shift-p");
        cx.update_window(handle, |_, window, cx| window.input("oJma/aaus", cx))
            .unwrap();
        assert_eq!(
            document.read_with(cx, |d, cx| d.palette_query(cx).unwrap().to_string()),
            "oJma/aaus"
        );
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader.read(cx).page_index(), before)
        });
        press(cx, handle, "escape escape");
        cx.update(|cx| keymap::reload(cx, None, preset).unwrap());
        press(cx, handle, "ctrl-e");
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader.read(cx).page_index(), before)
        });
        cx.update(|cx| {
            assert_eq!(
                keymap::active_map(cx)
                    .unwrap()
                    .shortcuts(&Save, keymap::Context::Reader),
                vec![if cfg!(target_os = "macos") {
                    "cmd-s"
                } else {
                    "ctrl-s"
                }],
            )
        });
    }
}

#[gpui_kit::test]
fn embedded_presets_switch_and_recover_live_filters_in_an_open_document(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    let paths = crate::settings::AppPaths::for_locations(config.clone(), root.path().join("data"));
    let store = crate::reading_history::RecentStore::new(&paths);
    let pdf = crate::reading_history::RecentStore::normalize(std::path::Path::new(
        "examples/readme-sample.pdf",
    ))
    .unwrap();
    let content = |preset: &str, filter: &str, command: &str| {
        format!(
            "[keyboard]\npreset = '{preset}'\n[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = '{command}'\nplatform = {filter}\n"
        )
    };
    std::fs::write(
        &config,
        content("standard", "['mac', 'linux', 'windows']", "next-page"),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_with_paths(cx, Preset::Standard, Some(paths.clone()));
        crate::reading_history::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
        menu::install(cx);
    });
    let (handle, document) = open(cx, Some(pdf.clone()));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let page = |cx: &TestAppContext| reader.read_with(cx, |r, _| r.page_index());
    let poll = |cx: &mut TestAppContext| {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(400));
        cx.run_until_parked();
    };
    press(cx, handle, "ctrl-e");
    assert_eq!(page(cx), 1);

    // Atomic editor replacement also changes the selected embedded preset.
    let replacement = root.path().join("replacement.toml");
    std::fs::write(
        &replacement,
        content("vim", "['mac', 'linux', 'windows']", "previous-page"),
    )
    .unwrap();
    std::fs::rename(&replacement, &config).unwrap();
    poll(cx);
    cx.update(|cx| assert_eq!(keymap::active_preset(cx), Preset::Vim));
    press(cx, handle, "ctrl-e");
    assert_eq!(page(cx), 0);
    press(cx, handle, "shift-j");
    assert_eq!(page(cx), 1);
    press(cx, handle, "m s");
    assert_eq!(store.marks(&pdf).unwrap()[&'s'].position.page, 1);
    press(cx, handle, "g g");
    assert_eq!(page(cx), 0);
    press(cx, handle, "' s");
    assert_eq!(page(cx), 1);
    press(cx, handle, "d m s");
    assert!(!store.marks(&pdf).unwrap().contains_key(&'s'));
    press(cx, handle, "g g");
    assert_eq!(page(cx), 0);

    // Invalid new filter syntax cannot partially replace either preset or overrides.
    std::fs::write(&config, content("standard", "[]", "next-page")).unwrap();
    poll(cx);
    cx.update(|cx| assert_eq!(keymap::active_preset(cx), Preset::Vim));
    press(cx, handle, "shift-j ctrl-e");
    assert_eq!(page(cx), 0);
    cx.update(|cx| {
        assert_eq!(
            keymap::active_map(cx)
                .unwrap()
                .shortcuts(&crate::reader::PreviousPage, keymap::Context::Reader)
                .last(),
            Some(&"ctrl-e"),
        );
    });

    // Recovery keeps the legacy single-string form and removes Vim-only sequences.
    let os = cx.update(|cx| match keymap::platform(cx) {
        keymap::Platform::Mac => "mac",
        keymap::Platform::Linux => "linux",
        keymap::Platform::Windows => "windows",
    });
    std::fs::write(
        &config,
        content("standard", &format!("'{os}'"), "next-page"),
    )
    .unwrap();
    poll(cx);
    cx.update(|cx| assert_eq!(keymap::active_preset(cx), Preset::Standard));
    press(cx, handle, "ctrl-e");
    assert_eq!(page(cx), 1);
    press(cx, handle, "g g");
    assert_eq!(page(cx), 1);
    press(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input("ggma", cx);
        window.press("ctrl-e", cx);
        assert_eq!(window.find("find-query").value(), Some("ggma"));
    })
    .unwrap();
    assert_eq!(page(cx), 1);
    press(cx, handle, "escape");

    // Omitting filters still works; deleting the override file restores the preset.
    std::fs::write(&config, "[keyboard]\npreset = 'vim'\n").unwrap();
    poll(cx);
    press(cx, handle, "shift-j");
    assert_eq!(page(cx), 2);
    std::fs::remove_file(&config).unwrap();
    poll(cx);
    cx.update(|cx| assert_eq!(keymap::active_preset(cx), Preset::Standard));
    press(cx, handle, "ctrl-e");
    assert_eq!(page(cx), 2);
    press(cx, handle, "right");
    assert_eq!(page(cx), 3);
}

#[gpui_kit::test]
fn embedded_presets_keep_annotation_save_and_save_as_shortcuts(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap();
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        let source = directory.join(format!("{}.pdf", preset.key()));
        let copy = directory.join(format!("{}-copy.pdf", preset.key()));
        std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let (handle, document) = open(cx, Some(source.clone()));
        cx.run_until_parked();
        super::saving::select_text(cx, handle, &document);
        press(
            cx,
            handle,
            if preset == Preset::Standard {
                "secondary-shift-h"
            } else {
                "a h"
            },
        );
        document.read_with(cx, |d, cx| assert!(d.reader.read(cx).is_dirty(cx)));
        press(cx, handle, "secondary-s");
        document.read_with(cx, |d, cx| assert!(!d.reader.read(cx).is_dirty(cx)));
        let saved = std::fs::read(&source).unwrap();
        assert_ne!(saved, original);
        press(cx, handle, "secondary-shift-s");
        assert!(cx.did_prompt_for_new_path());
        cx.simulate_new_path_selection(|_| Some(copy.clone()));
        cx.run_until_parked();
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader.read(cx).path(), Some(copy.as_path()));
            assert!(!d.reader.read(cx).is_dirty(cx));
        });
        assert_eq!(std::fs::read(&source).unwrap(), saved);
        assert!(copy.exists());
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
        drop(document);
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn vim_contents_j_k_match_arrow_selection(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| keymap::select_preset(cx, Preset::Vim).unwrap());
    let (handle, document) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    press(cx, handle, "g c");
    cx.update_window(handle, |_, window, cx| {
        window.click(("contents-entry", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let first = reader.read_with(cx, |r, _| r.reading_position());
    press(cx, handle, "down enter");
    let second = reader.read_with(cx, |r, _| r.reading_position());
    assert_ne!(second, first);
    press(cx, handle, "up enter");
    assert_eq!(reader.read_with(cx, |r, _| r.reading_position()), first);
    press(cx, handle, "j enter");
    assert_eq!(reader.read_with(cx, |r, _| r.reading_position()), second);
    press(cx, handle, "k enter");
    assert_eq!(reader.read_with(cx, |r, _| r.reading_position()), first);
}

#[gpui_kit::test]
fn vim_annotations_j_k_match_arrow_selection(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| keymap::select_preset(cx, Preset::Vim).unwrap());
    let (handle, _) = open(cx, Some(PathBuf::from("examples/annotations-clean.pdf")));
    cx.run_until_parked();
    let copied = |cx: &mut TestAppContext| {
        press(cx, handle, "secondary-c");
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap())
    };
    press(cx, handle, "g a");
    press(cx, handle, "down");
    let first = copied(cx);
    press(cx, handle, "down");
    let second = copied(cx);
    assert_ne!(second, first);
    press(cx, handle, "up");
    assert_eq!(copied(cx), first);
    press(cx, handle, "j");
    assert_eq!(copied(cx), second);
    press(cx, handle, "k");
    assert_eq!(copied(cx), first);
    press(cx, handle, "escape /");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        window.input("jk", cx);
        assert_eq!(window.find("find-query").value(), Some("jk"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn vim_distinguishes_pdf_pages_viewport_scroll_and_find_matches(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| keymap::select_preset(cx, Preset::Vim).unwrap());
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let position = |cx: &TestAppContext| reader.read_with(cx, |r, _| r.reading_position());
    let reset = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(crate::reader::GoToDocumentStart), cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    press(cx, handle, "shift-j");
    assert_eq!(position(cx).0, 1);
    press(cx, handle, "shift-k");
    assert_eq!(position(cx).0, 0);
    press(cx, handle, "ctrl-f");
    let screen = position(cx);
    assert_ne!(screen, (0, 0.));
    reset(cx);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ScrollScreenDown), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(position(cx), screen);
    press(cx, handle, "ctrl-b");
    assert_eq!(position(cx), (0, 0.));
    press(cx, handle, "ctrl-d");
    let half = position(cx);
    assert_ne!(half, (0, 0.));
    reset(cx);
    press(cx, handle, "space");
    assert_eq!(position(cx), half);
    press(cx, handle, "ctrl-u");
    assert_eq!(position(cx), (0, 0.));
    press(cx, handle, "/");
    cx.update_window(handle, |_, window, cx| {
        window.input("This is page", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    press(cx, handle, "escape");
    press(cx, handle, "n");
    assert_eq!(position(cx).0, 1);
    press(cx, handle, "shift-n");
    assert_eq!(position(cx).0, 0);
    let before = position(cx);
    press(cx, handle, "p");
    assert_eq!(position(cx), before);
    press(cx, handle, "g c");
    document.read_with(cx, |d, cx| {
        assert_eq!(d.sidebar_panel(cx), Some(SidebarPanel::Contents))
    });
    press(cx, handle, "escape");
    press(cx, handle, "g a");
    document.read_with(cx, |d, cx| {
        assert_eq!(d.sidebar_panel(cx), Some(SidebarPanel::Annotations))
    });
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn vim_native_find_forwards_without_installing_typing_key_equivalents(cx: &mut TestAppContext) {
    use gpui_kit::OwnedMenuItem;

    setup(cx);
    cx.update(|cx| {
        keymap::select_preset(cx, Preset::Vim).unwrap();
        menu::install(cx);
    });
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let action = cx.update(|cx| {
        let menus = cx.get_menus().unwrap();
        let edit = menus.iter().find(|m| m.name == "Edit").unwrap();
        let find = edit
            .items
            .iter()
            .find_map(|item| match item {
                OwnedMenuItem::Action { name, action, .. } if name == "Find" => {
                    Some(action.boxed_clone())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(find.name(), "kver::NativeMenuAction");
        assert_eq!(
            cx.key_bindings()
                .borrow()
                .bindings_for_action(find.as_ref())
                .count(),
            0
        );
        find
    });
    cx.update_window(handle, |_, window, cx| window.dispatch_action(action, cx))
        .unwrap();
    cx.run_until_parked();
    let before = document.read_with(cx, |d, cx| d.reader.read(cx).reading_position());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        window.input("/nNJKaaus", cx);
        assert_eq!(window.find("find-query").value(), Some("/nNJKaaus"));
    })
    .unwrap();
    document.read_with(cx, |d, cx| {
        assert_eq!(d.reader.read(cx).reading_position(), before);
        assert!(!d.reader.read(cx).is_dirty(cx));
    });
    // Switching presets restores Standard's real modified menu accelerator.
    cx.update(|cx| {
        keymap::select_preset(cx, Preset::Standard).unwrap();
        let edit = cx
            .get_menus()
            .unwrap()
            .into_iter()
            .find(|m| m.name == "Edit")
            .unwrap();
        assert!(edit.items.iter().any(|item| {
            matches!(item, OwnedMenuItem::Action { name, action, .. }
                if name == "Find" && action.name() == "kver::ShowFind")
        }));
    });
}
