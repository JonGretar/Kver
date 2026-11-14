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
        press(cx, handle, "cmd-c");
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
