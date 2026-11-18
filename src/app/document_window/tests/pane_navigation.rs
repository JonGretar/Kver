//! Shared controls operate on stable panes through the production window.

use super::super::panes::Orientation;
use super::*;
use gpui_kit::{point, px, size};

fn prepare(
    cx: &mut TestAppContext,
    path: &str,
) -> (
    AnyWindowHandle,
    Entity<DocumentWindow>,
    Entity<ReaderView>,
    Entity<ReaderView>,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from(path)));
    cx.run_until_parked();
    cx.simulate_window_resize(handle, size(px(1200.), px(850.)));
    let first = document.read_with(cx, |d, _| d.reader.clone());
    keys(cx, handle, "secondary-\\");
    let second = document.read_with(cx, |d, _| d.reader.clone());
    assert_ne!(first, second);
    (handle, document, first, second)
}

fn keys(cx: &mut TestAppContext, handle: AnyWindowHandle, keys: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for key in keys.split_whitespace() {
            window.press(key, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
}

fn click_pane(window: &mut Window, reader: &Entity<ReaderView>, cx: &mut App) {
    window.click_at(
        ("reading-pane", reader.entity_id()),
        point(px(5.), px(5.)),
        cx,
    );
}

fn page(reader: &Entity<ReaderView>, cx: &TestAppContext) -> usize {
    reader.read_with(cx, |r, _| r.page_index())
}

#[gpui_kit::test]
fn shared_find_keeps_query_and_results_without_replaying_on_switch(cx: &mut TestAppContext) {
    let (handle, document, first, second) = prepare(cx, "examples/readme-sample.pdf");
    keys(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| window.input("This is page", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click(("find-result", 2usize), cx);
        window.press("right", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 2);
    let history = document.read_with(cx, |d, _| d.jumplist.entries.len());
    cx.update_window(handle, |_, window, cx| click_pane(window, &first, cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, first);
        assert!(d.jumplist.entries.is_empty());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").value(), Some("This is page"));
        assert!(
            window
                .find("find-status")
                .label()
                .unwrap()
                .starts_with("3 /")
        );
        assert!(
            window
                .within(("reading-pane", first.entity_id()))
                .try_find("find-highlight")
                .is_none()
        );
        window.click(("find-result", 4usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 4);
    assert_eq!(page(&second, cx), 2);
    // Close the sidebar to restore active-reader focus, then use real history keys.
    keys(cx, handle, "secondary-b alt-left");
    assert_eq!(page(&first, cx), 0);
    keys(cx, handle, "alt-right");
    assert_eq!(page(&first, cx), 4);
    keys(cx, handle, "ctrl-tab");
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, second);
        assert_eq!(d.jumplist.entries.len(), history);
    });
    keys(cx, handle, "alt-left");
    assert_eq!(page(&second, cx), 0);
    assert_eq!(page(&first, cx), 4);
}

#[gpui_kit::test]
fn pending_find_enter_keeps_invoking_pane_after_pointer_activation(cx: &mut TestAppContext) {
    let (handle, document, first, second) = prepare(cx, "examples/readme-sample.pdf");
    keys(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.input("This is page 99 of 100", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        // Deliver the input subscription before the next native event, but do
        // not run the background search between invocation and activation.
        window.render_frame(cx);
        assert!(window.try_find(("find-result", 0usize)).is_none());
        click_pane(window, &first, cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 98);
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, first);
        assert!(d.jumplist.entries.is_empty());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(first.read(cx).focus_handle(cx).is_focused(window));
        assert!(
            window
                .within(("reading-pane", second.entity_id()))
                .find("find-highlight")
                .visible()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn removed_find_target_cannot_navigate_a_new_pane(cx: &mut TestAppContext) {
    let (handle, document, first, removed) = prepare(cx, "examples/readme-sample.pdf");
    keys(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.input("This is page 99 of 100", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("find-result", 0usize)).is_none());
        click_pane(window, &removed, cx);
        document.update(cx, |d, cx| d.remove_pane(false, window, cx));
        document.update(cx, |d, cx| {
            d.arrange_panes(Orientation::SideBySide, window, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    document.read_with(cx, |d, cx| {
        assert_ne!(d.reader, removed);
        assert_eq!(d.reader.read(cx).page_index(), 0);
        assert!(d.jumplist.entries.is_empty());
    });
    // A retained test handle does not make a detached pane a valid destination.
    assert_eq!(page(&removed, cx), 0);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-status").label(), Some("1 match"));
        assert!(window.try_find("find-highlight").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn contents_routes_to_active_pane_and_keeps_sidebar_keyboard_ownership(cx: &mut TestAppContext) {
    let (handle, document, first, second) = prepare(cx, "examples/field-guide.pdf");
    keys(cx, handle, "secondary-t");
    cx.update_window(handle, |_, window, cx| {
        window.click(("contents-entry", 0usize), cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 2);
    cx.update_window(handle, |_, window, cx| {
        click_pane(window, &first, cx);
        window.click(("contents-entry", 0usize), cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 2);
    assert_eq!(page(&second, cx), 2);
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    keys(cx, handle, "escape alt-left");
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 2);
}

#[gpui_kit::test]
fn annotation_list_routes_destination_and_history_without_moving_other_pane(
    cx: &mut TestAppContext,
) {
    let (handle, document, first, second) = prepare(cx, "examples/annotations-clean.pdf");
    for reader in [&first, &second] {
        reader.update(cx, |r, cx| r.select_page(1, cx));
    }
    let origin = first.read_with(cx, |r, _| r.jump_location().unwrap());
    let epoch = second.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click(format!("annotation-{epoch}-3"), cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |r, _| {
        assert!(r.jump_location().unwrap().equivalent(&origin))
    });
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    let second_location = second.read_with(cx, |r, _| r.jump_location().unwrap());
    cx.update_window(handle, |_, window, cx| {
        click_pane(window, &first, cx);
        window.click(format!("annotation-{epoch}-1"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    second.read_with(cx, |r, _| {
        assert!(r.jump_location().unwrap().equivalent(&second_location));
    });
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    keys(cx, handle, "secondary-b alt-left");
    first.read_with(cx, |r, _| {
        assert!(r.jump_location().unwrap().equivalent(&origin))
    });
}

#[gpui_kit::test]
fn marks_pending_jump_and_modal_input_remain_with_invoking_reader(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let path = root.path().join("marked.pdf");
    std::fs::copy("examples/readme-sample.pdf", &path).unwrap();
    setup(cx);
    cx.update(|cx| {
        crate::recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (handle, document) = open(cx, Some(path));
    cx.run_until_parked();
    cx.simulate_window_resize(handle, size(px(1200.), px(850.)));
    let first = document.read_with(cx, |d, _| d.reader.clone());
    first.update(cx, |r, cx| r.select_page(20, cx));
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(SetLocalMark('a')), cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.update(cx, |r, cx| r.select_page(0, cx));
    keys(cx, handle, "secondary-\\");
    let second = document.read_with(cx, |d, _| d.reader.clone());
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(JumpLocalMark('a')), cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| click_pane(window, &first, cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 20);
    document.read_with(cx, |d, _| assert!(d.jumplist.entries.is_empty()));
    keys(cx, handle, "secondary-shift-m");
    cx.update_window(handle, |_, window, cx| {
        window.input("Page 21", cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&first, cx), 0);
    assert_eq!(page(&second, cx), 20);
    keys(cx, handle, "enter");
    assert_eq!(page(&first, cx), 20);
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    cx.update_window(handle, |_, window, cx| {
        assert!(first.read(cx).focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn active_history_controls_and_effective_menu_palette_hints_follow_pane(cx: &mut TestAppContext) {
    use crate::app::keymap::{self, Preset};
    let mut info = cx.update(|cx| cx.platform_info());
    info.os = gpui_kit::OperatingSystem::Linux;
    cx.set_platform_info(info);
    let (handle, document, first, second) = prepare(cx, "examples/field-guide.pdf");
    cx.update(|cx| {
        keymap::reload(
            cx,
            Some(
                "[[keyboard.bindings]]\nkeys = 'ctrl-shift-h'\ncontext = 'reader'\ncommand = 'navigate-back'\n\
                 [[keyboard.bindings]]\nkeys = 'alt-left'\ncontext = 'reader'\ncommand = 'next-page'\n".into()
            ),
            Preset::Standard,
        ).unwrap();
    });
    keys(cx, handle, "secondary-t");
    cx.update_window(handle, |_, window, cx| {
        window.click(("contents-entry", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    keys(cx, handle, "secondary-b");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_ne!(window.find("navigate-back").disabled(), Some(true));
        window.click("application-menu", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let expected = cx.update(|cx| crate::app::shortcut_label::format("ctrl-shift-h", cx));
    let switch_hint = cx.update(|cx| crate::app::shortcut_label::format("ctrl-tab", cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find("application-menu-shortcut:kver::SwitchPane")
                .label(),
            Some(switch_hint.as_str()),
        );
        window.press("escape", cx);
    })
    .unwrap();
    keys(cx, handle, "secondary-shift-p");
    cx.update_window(handle, |_, window, cx| window.input("Back", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find("command-palette-shortcut:kver::NavigateBack")
                .label(),
            Some(expected.as_str()),
        );
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(page(&second, cx), 0);
    assert_eq!(page(&first, cx), 0);
    cx.update_window(handle, |_, window, cx| click_pane(window, &first, cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("navigate-back").disabled(), Some(true));
    })
    .unwrap();
    keys(cx, handle, "secondary-shift-p");
    cx.update_window(handle, |_, window, cx| window.input("Back", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(!d.palette.as_ref().unwrap().has_selection(cx));
    });
    keys(cx, handle, "enter escape escape");
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, first);
        assert!(d.jumplist.entries.is_empty());
    });
}

#[gpui_kit::test]
fn pending_find_cannot_cross_successful_document_replacement(cx: &mut TestAppContext) {
    let (handle, document, _, _) = prepare(cx, "examples/readme-sample.pdf");
    keys(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.input("This is page 99 of 100", cx);
        window.press("enter", cx);
    })
    .unwrap();
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).page_index(), 0);
        assert!(d.jumplist.entries.is_empty());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").value(), Some(""));
        assert!(window.try_find(("find-result", 0usize)).is_none());
        assert!(window.try_find("find-highlight").is_none());
        window.input("landscape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| assert_eq!(d.reader.read(cx).page_index(), 1));
}
