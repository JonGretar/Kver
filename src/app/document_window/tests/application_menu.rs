use super::*;
use gpui_kit::test::TestEventExt as _;

fn open_menu(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("application-menu", cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn choose(cx: &mut TestAppContext, handle: AnyWindowHandle, label: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let at = window
            .within("popup-menu")
            .find_by_label(label)
            .bounds()
            .center();
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn setup_menu(cx: &mut TestAppContext) {
    let mut info = cx.update(|cx| cx.platform_info());
    info.os = gpui_kit::OperatingSystem::Linux;
    cx.set_platform_info(info);
    setup(cx);
}

fn hint(cx: &mut TestAppContext, handle: AnyWindowHandle, action: &dyn Action) -> Option<String> {
    let id = format!("application-menu-shortcut:{}", action.name());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|hint| hint.label().map(str::to_owned))
    })
    .unwrap()
}

fn formatted(cx: &mut TestAppContext, keys: &str) -> String {
    cx.update(|cx| crate::app::shortcut_label::format(keys, cx))
}

#[gpui_kit::test]
fn menu_routes_application_commands_to_existing_owners(cx: &mut TestAppContext) {
    setup_menu(cx);
    cx.update(|cx| {
        menu::install(cx);
        windows::install(cx);
    });
    let (handle, _) = open(cx, None);
    cx.run_until_parked();
    open_menu(cx, handle);
    choose(cx, handle, "New Window");
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    open_menu(cx, handle);
    choose(cx, handle, "Settings…");
    assert_eq!(cx.update(|cx| cx.windows().len()), 3);
    // The existing owner reuses its window rather than creating another.
    open_menu(cx, handle);
    choose(cx, handle, "Settings…");
    assert_eq!(cx.update(|cx| cx.windows().len()), 3);
    open_menu(cx, handle);
    choose(cx, handle, "About Kver");
    assert_eq!(cx.update(|cx| cx.windows().len()), 4);
    open_menu(cx, handle);
    choose(cx, handle, "About Kver");
    assert_eq!(cx.update(|cx| cx.windows().len()), 4);
}

#[gpui_kit::test]
fn menu_disabled_commands_dismissal_and_keyboard_do_not_leak(cx: &mut TestAppContext) {
    setup_menu(cx);
    let (handle, document) = open(cx, None);
    cx.run_until_parked();
    open_menu(cx, handle);
    // Kit 0.8.1 styles/disables menu rows but does not expose aria-disabled.
    // Prove the unavailable rows reject input rather than inferring availability
    // from the missing accessibility flag.
    choose(cx, handle, "Save");
    choose(cx, handle, "Save As…");
    assert!(!cx.did_prompt_for_new_path());
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
        assert!(window.focused(cx).is_some());
    })
    .unwrap();

    document.update(cx, |document, cx| {
        document.reader.update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
        });
    });
    cx.run_until_parked();
    open_menu(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.press("right", cx);
        window.press("down", cx);
        window.input("jj", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).page_index(), 0);
    });
    cx.update_window(handle, |_, window, cx| {
        let reader = document.read(cx).reader.read(cx);
        assert!(reader.focus_handle(cx).is_focused(window));
        window.press("tab", cx);
        assert_eq!(window.find("application-menu").focused(), Some(true));
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_some());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("application-menu").focused(), Some(true));
    })
    .unwrap();
}

#[gpui_kit::test]
fn menu_shortcuts_follow_live_overrides_and_keyboard_activation(cx: &mut TestAppContext) {
    use crate::app::keymap::{self, Preset};
    setup_menu(cx);
    cx.update(windows::install);
    cx.update(|cx| {
        keymap::reload(
            cx,
            Some("[[keyboard.bindings]]\nkeys = 'ctrl-n'\ncontext = 'global'\ncommand = 'new-window'\n".into()),
            Preset::Standard,
        ).unwrap();
    });
    let (handle, _) = open(cx, None);
    cx.run_until_parked();
    open_menu(cx, handle);
    assert_eq!(
        hint(cx, handle, &windows::NewWindow),
        Some(formatted(cx, "ctrl-n"))
    );
    cx.update(|cx| {
        keymap::reload(
            cx,
            Some("[[keyboard.bindings]]\nkeys = 'ctrl-shift-n'\ncontext = 'global'\ncommand = 'new-window'\n".into()),
            Preset::Vim,
        ).unwrap();
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert_eq!(
        hint(cx, handle, &windows::NewWindow),
        Some(formatted(cx, "ctrl-shift-n"))
    );
    // An override must also work, not merely paint a label.
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.press("ctrl-shift-n", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    open_menu(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 3);
    open_menu(cx, handle);
    cx.update(|cx| keymap::reload(cx, None, Preset::Standard).unwrap());
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert_ne!(
        hint(cx, handle, &windows::NewWindow),
        Some(formatted(cx, "ctrl-shift-n"))
    );
}

#[gpui_kit::test]
fn menu_open_hint_omits_global_accelerator_shadowed_in_reader(cx: &mut TestAppContext) {
    use crate::app::keymap::{self, Preset};
    setup_menu(cx);
    // The nonMac Vim policy from #178, expressed through the existing
    // override interface so this regression does not depend on its landing.
    cx.update(|cx| {
        keymap::reload(
            cx,
            Some(
                "[[keyboard.bindings]]\nkeys = 'ctrl-o'\ncontext = 'global'\ncommand = 'open-pdf'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-o'\ncontext = 'reader'\ncommand = 'navigate-back'\n\
                 [[keyboard.bindings]]\nkeys = 'o'\ncontext = 'reader'\ncommand = 'open-pdf'\n".into()
            ),
            Preset::Vim,
        ).unwrap();
        let hints = keymap::active_map(cx).unwrap().shortcuts(&OpenPdf, keymap::Context::Reader);
        assert!(!hints.contains(&"ctrl-o"));
        assert!(hints.contains(&"o"));
    });
    let (handle, _) = open(cx, None);
    cx.run_until_parked();
    open_menu(cx, handle);
    assert_eq!(hint(cx, handle, &OpenPdf), Some("o".into()));
}

#[gpui_kit::test]
fn windows_menu_preserves_find_focus_and_contains_incidental_input(cx: &mut TestAppContext) {
    setup_menu(cx);
    let mut info = cx.update(|cx| cx.platform_info());
    info.os = gpui_kit::OperatingSystem::Windows;
    cx.set_platform_info(info);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowFind), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let before = cx
        .update_window(handle, |_, window, cx| window.focused(cx).unwrap())
        .unwrap();
    open_menu(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.press("down", cx);
        window.input("j", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.focused(cx), Some(before));
    })
    .unwrap();
    open_menu(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.click_at(
            "popup-menu",
            gpui_kit::point(gpui_kit::px(1.), gpui_kit::px(1.)),
            cx,
        );
        window.press("right", cx);
        window.input("j", cx);
        // Nonmodal Kit menus retain their normal outside-click policy.
        // Dismiss on a non-command surface, not an intentional toolbar action.
        window.click("reader-status", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).page_index(), 0);
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
    })
    .unwrap();
    // An intentional outside command remains usable; this is not a modal.
    open_menu(cx, handle);
    cx.update_window(handle, |_, window, cx| window.click("next-page", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).page_index(), 1);
    });
}

#[gpui_kit::test]
fn menu_title_geometry_preserves_center_and_unsaved_space_at_interface_scales(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{component::theme::Theme, px, size};
    setup_menu(cx);
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join(format!("{}.pdf", "long-filename-".repeat(12)));
    std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
    let (handle, document) = open(cx, Some(path));
    cx.run_until_parked();
    for dirty in [false, true] {
        if dirty {
            super::saving::highlight(cx, handle, &document);
        }
        for font_size in [12., 16., 20.] {
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            for width in [360., 600., 920., 1200.] {
                cx.simulate_window_resize(handle, size(px(width), px(800.)));
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    let menu = window.find("application-menu").bounds();
                    let name = window.find("document-name").bounds();
                    let title = window.find("document-title").bounds();
                    let open = window.find("open-pdf").bounds();
                    assert!(name.size.width > px(0.), "width={width}, font={font_size}, dirty={dirty}, name={name:?}, title={title:?}");
                    assert!(menu.right() < name.left(), "menu={menu:?}, name={name:?}");
                    if dirty {
                        let status = window.find("document-unsaved").bounds();
                        assert!(status.left() > name.right());
                        assert!(status.right() < open.left());
                    } else {
                        assert!((name.center().x - px(width / 2.)).abs() <= px(0.5), "width={width}, font={font_size}, name={name:?}, title={title:?}");
                    }
                    assert!(name.right() < open.left());
                }).unwrap();
            }
        }
    }
}

#[gpui_kit::test]
fn mac_keeps_only_the_native_application_menu(cx: &mut TestAppContext) {
    let mut info = cx.update(|cx| cx.platform_info());
    info.os = gpui_kit::OperatingSystem::Mac;
    cx.set_platform_info(info);
    setup(cx);
    let (handle, _) = open(cx, None);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("application-menu").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn menu_quit_preserves_unsaved_work_and_save_uses_document_owner(cx: &mut TestAppContext) {
    setup_menu(cx);
    cx.update(menu::install);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("menu.pdf");
    std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
    let (handle, document) = open(cx, Some(path));
    cx.run_until_parked();
    super::saving::highlight(cx, handle, &document);
    open_menu(cx, handle);
    choose(cx, handle, "Close Window");
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    open_menu(cx, handle);
    choose(cx, handle, "Quit Kver");
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).is_dirty(cx))
    });
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    open_menu(cx, handle);
    choose(cx, handle, "Save");
    document.read_with(cx, |document, cx| {
        assert!(!document.reader.read(cx).is_dirty(cx))
    });
    open_menu(cx, handle);
    choose(cx, handle, "Close Window");
    assert!(cx.update(|cx| cx.windows().is_empty()));
}
