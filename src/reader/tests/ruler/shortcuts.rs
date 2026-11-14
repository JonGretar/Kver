use super::*;

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, keys: &str) {
    cx.update_window(window, |_, frame, cx| {
        for key in keys.split_whitespace() {
            frame.press(key, cx);
        }
    })
    .unwrap();
    settle(cx, window);
}

fn toggle(preset: Preset) -> &'static str {
    if preset == Preset::Standard {
        "secondary-shift-r"
    } else {
        "r"
    }
}

#[gpui_kit::test]
fn ruler_arrows_share_traversal_and_scroll_when_off_in_both_presets(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        for mode in [ReadingMode::SinglePage, ReadingMode::Continuous] {
            reader.update(cx, |reader, cx| {
                reader.set_reading_mode(mode, cx);
                reader.select_page(2, cx);
                reader.change_scale(2. / reader.scale, cx);
                reader.scroll_by(-10000., cx);
            });
            settle(cx, window);
            let offset = reader.read_with(cx, |reader, _| reader.scroll.offset());
            press(cx, window, "down");
            reader.read_with(cx, |reader, _| {
                assert!(!reader.ruler.enabled);
                assert_ne!(reader.scroll.offset(), offset);
            });
            press(cx, window, "up");
            reader.read_with(cx, |reader, _| assert_eq!(reader.scroll.offset(), offset));
            press(cx, window, toggle(preset));
            let initial = target(&reader, cx);
            press(cx, window, "down");
            assert_eq!(target(&reader, cx), (initial.0, initial.1 + 1));
            press(cx, window, "up");
            assert_eq!(target(&reader, cx), initial);
            // Use the existing traversal owner at a page boundary in both directions.
            boundary(&reader, cx, 2, true);
            settle(cx, window);
            press(cx, window, "down");
            assert_eq!(target(&reader, cx), (3, 0));
            press(cx, window, "up");
            assert_eq!(target(&reader, cx).0, 2);
            // Explicit scroll commands remain scrolling, not ruler-line movement.
            let before = reader.read_with(cx, |reader, _| reader.scroll.offset());
            let line = target(&reader, cx);
            cx.update_window(window, |_, frame, cx| {
                frame.dispatch_action(Box::new(ScrollUp), cx);
            })
            .unwrap();
            settle(cx, window);
            assert_eq!(target(&reader, cx), line);
            reader.read_with(cx, |reader, _| assert_ne!(reader.scroll.offset(), before));
            press(cx, window, toggle(preset));
            reader.read_with(cx, |reader, cx| {
                assert!(!reader.ruler.enabled);
                assert!(!reader.is_dirty(cx));
            });
        }
    }
}

#[gpui_kit::test]
fn ruler_shortcuts_leave_find_palette_and_dialog_navigation_alone(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        press(cx, window, toggle(preset));
        let initial = target(&reader, cx);
        cx.update_window(window, |_, frame, cx| {
            frame.dispatch_action(Box::new(ShowFind), cx);
        })
        .unwrap();
        settle(cx, window);
        cx.update_window(window, |_, frame, cx| {
            frame.press("secondary-a", cx);
            frame.press("backspace", cx);
            frame.input("rjk", cx);
            frame.press("down", cx);
            frame.press("up", cx);
            frame.press(toggle(preset), cx);
            assert_eq!(
                frame.find("find-query").value(),
                Some(if preset == Preset::Vim { "rjkr" } else { "rjk" })
            );
        })
        .unwrap();
        assert_eq!(target(&reader, cx), initial);
        reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
        press(cx, window, "escape");
        press(cx, window, "cmd-shift-p");
        cx.update_window(window, |_, frame, cx| {
            frame.press("down", cx);
            frame.press("up", cx);
            frame.press(toggle(preset), cx);
            frame.input("rjk", cx);
        })
        .unwrap();
        assert_eq!(target(&reader, cx), initial);
        reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
        press(cx, window, "escape escape");
        // A real Kit dialog lives above the production document window.
        cx.update_window(window, |_, frame, cx| {
            use gpui_kit::component::WindowExt as _;
            frame.open_dialog(cx, |dialog, _, _| {
                dialog
                    .title("Shortcut isolation")
                    .child("Reader keys stay below")
            });
        })
        .unwrap();
        settle(cx, window);
        press(cx, window, "down up");
        press(cx, window, toggle(preset));
        assert_eq!(target(&reader, cx), initial);
        reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
        press(cx, window, "escape");
        press(cx, window, toggle(preset));
        reader.read_with(cx, |reader, _| assert!(!reader.ruler.enabled));
    }
}

#[gpui_kit::test]
fn ruler_shortcuts_leave_focused_contents_and_annotations_navigation_alone(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/field-guide.pdf", 0);
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        press(cx, window, toggle(preset));
        cx.update_window(window, |_, frame, cx| {
            frame.dispatch_action(Box::new(ShowContents), cx);
        })
        .unwrap();
        settle(cx, window);
        let initial = target(&reader, cx);
        press(cx, window, "down up");
        press(cx, window, toggle(preset));
        if preset == Preset::Vim {
            press(cx, window, "j k");
        }
        assert_eq!(target(&reader, cx), initial);
        reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
        press(cx, window, "escape");
        press(cx, window, toggle(preset));
    }
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/annotations-clean.pdf"), cx)
    });
    settle(cx, window);
    for preset in [Preset::Standard, Preset::Vim] {
        cx.update(|cx| keymap::select_preset(cx, preset).unwrap());
        press(cx, window, toggle(preset));
        cx.update_window(window, |_, frame, cx| {
            frame.dispatch_action(Box::new(ShowAnnotations), cx);
        })
        .unwrap();
        settle(cx, window);
        let copy = |cx: &mut TestAppContext| {
            press(cx, window, "cmd-c");
            cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap())
        };
        press(cx, window, "down");
        let first = copy(cx);
        press(cx, window, "down");
        let second = copy(cx);
        assert_ne!(first, second);
        press(cx, window, "up");
        assert_eq!(copy(cx), first);
        if preset == Preset::Vim {
            press(cx, window, "j");
            assert_eq!(copy(cx), second);
            press(cx, window, "k");
            assert_eq!(copy(cx), first);
        }
        // Selecting an annotation deliberately retargets the ruler. Only a
        // reader shortcut leaking through list focus would change it now.
        let initial = target(&reader, cx);
        press(cx, window, toggle(preset));
        assert_eq!(target(&reader, cx), initial);
        reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
        press(cx, window, "escape");
        press(cx, window, toggle(preset));
    }
}
