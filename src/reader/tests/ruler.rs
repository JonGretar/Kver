use super::*;
use crate::app::keymap::{self, Preset};
use crate::reader::ruler::Target;
use gpui_kit::{
    Modifiers,
    test::{ClickOptions, TestEventExt},
};

mod traversal;

fn open(cx: &mut TestAppContext, path: &str, page: usize) -> (AnyWindowHandle, Entity<ReaderView>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
    });
    let (window, reader) = recent_test_window(cx, Some(PathBuf::from(path)));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.select_page(page, cx);
        reader.fit_page(cx);
    });
    settle(cx, window);
    (window, reader)
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn target(reader: &Entity<ReaderView>, cx: &mut TestAppContext) -> (usize, usize) {
    reader.read_with(cx, |reader, _| {
        let target = reader.ruler.target.as_ref().expect("ruler target");
        (target.page, target.line)
    })
}

#[gpui_kit::test]
fn ruler_keyboard_reads_columns_preserves_focus_and_resumes_without_scroll(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    let initial = target(&reader, cx);
    let offset = reader.read_with(cx, |reader, _| reader.scroll.offset());
    cx.simulate_keystrokes(window, "j");
    assert_eq!(target(&reader, cx), (2, initial.1 + 1));
    cx.simulate_keystrokes(window, "k");
    assert_eq!(target(&reader, cx), initial);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        let wash = frame.find("reading-ruler-wash").bounds();
        let marker = frame.find("reading-ruler-marker").bounds();
        let page = frame.find("pdf-page-2").bounds();
        assert_eq!(wash.left(), page.left());
        assert_eq!(wash.right(), page.right());
        assert!((wash.top() - marker.bottom()).abs() < px(0.01));
        assert!((wash.bottom() - page.bottom()).abs() < px(0.01));
        assert!(marker.left() >= wash.left() && marker.right() <= wash.right());
        assert!(marker.size.width < wash.size.width);
        assert!(reader.read(cx).focus.is_focused(frame));
    })
    .unwrap();
    cx.simulate_keystrokes(window, "r");
    reader.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert_eq!(reader.scroll.offset(), offset);
    });
    cx.simulate_keystrokes(window, "r");
    assert_eq!(target(&reader, cx), initial);
    // Traverse all remaining columns in this page, then reverse within it.
    let last = reader.read_with(cx, |reader, _| {
        reader.ruler.target.as_ref().unwrap().text.lines.len() - 1
    });
    for _ in initial.1..last {
        cx.simulate_keystrokes(window, "j");
    }
    assert_eq!(target(&reader, cx), (2, last));
    for _ in 0..last {
        cx.simulate_keystrokes(window, "k");
    }
    assert_eq!(target(&reader, cx), (2, 0));
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| assert!(!reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_alt_click_on_link_places_without_navigation_or_selection(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.update_window(window, |_, frame, cx| {
        frame.click_with_modifiers("pdf-link-2-0", Modifiers::alt(), cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.enabled);
        assert_eq!(reader.page_index(), 2);
        assert_eq!(reader.selection.dragging_page(), None);
        assert_eq!(reader.selected_text(), None);
        assert_eq!(reader.selected_annotation, None);
    });
    assert_eq!(target(&reader, cx).0, 2);
    cx.update_window(window, |_, frame, cx| {
        let at = frame.find("pdf-link-2-0").bounds().center();
        frame.change_modifiers(Modifiers::alt(), cx);
        frame.pointer_down(at, gpui::MouseButton::Left, cx);
        frame.change_modifiers(Modifiers::default(), cx);
        frame.pointer_up(at, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
    cx.update_window(window, |_, frame, cx| frame.click("pdf-link-2-0", cx))
        .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 3));
}

#[gpui_kit::test]
fn ruler_palette_owns_typing_and_escape_before_reader_mode(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/readme-sample.pdf", 0);
    cx.simulate_keystrokes(window, "r");
    let initial = target(&reader, cx);
    cx.simulate_keystrokes(window, ":");
    cx.update_window(window, |_, frame, cx| {
        frame.input("rjk", cx);
        frame.press("escape", cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(target(&reader, cx), initial);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
    // Command clears a nonempty query before dismissing its own surface.
    cx.simulate_keystrokes(window, "escape");
    settle(cx, window);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| assert!(!reader.ruler.enabled));
    // The palette's command invokes the same owner action with reader focus.
    cx.simulate_keystrokes(window, ":");
    cx.update_window(window, |_, frame, cx| {
        frame.input("Toggle reading ruler", cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_alt_click_markup_does_not_select_annotation_and_escape_keeps_mode(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/annotations.pdf", 0);
    let at = reader.read_with(cx, |reader, cx| {
        let catalog = reader
            .session
            .as_ref()
            .unwrap()
            .read(cx)
            .annotations()
            .unwrap()
            .unwrap();
        let quad = &catalog
            .iter()
            .find(|annotation| annotation.page() == 0)
            .unwrap()
            .quads()[0];
        let corners = quad.points();
        let center = Point {
            x: corners.iter().map(|p| p.x).sum::<f64>() / 4.,
            y: corners.iter().map(|p| p.y).sum::<f64>() / 4.,
        };
        let screen = reader.page_viewport(0).unwrap().document_to_screen(center);
        point(px(screen.x as f32), px(screen.y as f32))
    });
    cx.update_window(window, |_, frame, cx| {
        let page = frame.find("pdf-page-0").bounds();
        frame.click_with_options(
            "pdf-page-0",
            ClickOptions::new()
                .with_offset(at - page.origin)
                .with_modifiers(Modifiers::alt()),
            cx,
        );
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.enabled);
        assert!(reader.ruler.target.is_some());
        assert!(reader.selected_annotation.is_none());
        assert!(reader.annotation_choice.is_none());
    });
    cx.update_window(window, |_, frame, cx| {
        let page = frame.find("pdf-page-0").bounds();
        frame.click_at("pdf-page-0", at - page.origin, cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.selected_annotation.is_some())
    });
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.enabled);
        assert!(reader.selected_annotation.is_none());
    });
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| assert!(!reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_standard_commands_and_find_input_have_separate_ownership(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/readme-sample.pdf", 0);
    cx.update(|cx| {
        keymap::install_preset(cx, Preset::Standard);
        let config = r#"
            [[keyboard.bindings]]
            keys = "ctrl-alt-r"
            context = "reader"
            command = "toggle-reading-ruler"
            [[keyboard.bindings]]
            keys = "ctrl-alt-j"
            context = "reader"
            command = "next-ruler-line"
            [[keyboard.bindings]]
            keys = "ctrl-alt-k"
            context = "reader"
            command = "previous-ruler-line"
        "#;
        let map = keymap::resolve_config(config, keymap::Platform::Mac, Preset::Standard).unwrap();
        cx.bind_keys(map.bindings(cx));
    });
    cx.simulate_keystrokes(window, "ctrl-alt-r");
    let initial = target(&reader, cx);
    cx.simulate_keystrokes(window, "ctrl-alt-j");
    assert_eq!(target(&reader, cx), (0, initial.1 + 1));
    cx.simulate_keystrokes(window, "ctrl-alt-k");
    assert_eq!(target(&reader, cx), initial);
    cx.simulate_keystrokes(window, "cmd-f");
    cx.update_window(window, |_, frame, cx| {
        frame.input("rjk", cx);
        frame.press("ctrl-alt-r", cx);
        frame.press("ctrl-alt-j", cx);
        assert_eq!(frame.find("find-query").value(), Some("rjk"));
    })
    .unwrap();
    assert_eq!(target(&reader, cx), initial);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_windows_and_replacement_do_not_share_live_targets(cx: &mut TestAppContext) {
    let (first_window, first) = open(cx, "examples/readme-sample.pdf", 0);
    let (second_window, second) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    settle(cx, second_window);
    cx.simulate_keystrokes(first_window, "r");
    let initial = target(&first, cx);
    second.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
    });
    cx.simulate_keystrokes(first_window, "j");
    assert_eq!(target(&first, cx).1, initial.1 + 1);
    first.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/reading-order.pdf"), cx)
    });
    settle(cx, first_window);
    first.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
        assert!(reader.ruler.pending.is_none());
    });
}

#[gpui_kit::test]
fn ruler_late_text_does_not_reenable_a_cancelled_activation(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/readme-sample.pdf", 0);
    // Hold the shared production request slot to model a slow extractor while
    // exercising activation and cancellation through the composed window.
    let request = reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        reader
            .page_text
            .request(0..1, None, [None; 2], reader.document_generation)
            .unwrap()
    });
    cx.simulate_keystrokes(window, "r");
    reader.read_with(cx, |reader, _| assert!(reader.ruler.pending.is_some()));
    cx.simulate_keystrokes(window, "escape");
    let text = pdf::page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
    reader.update(cx, |reader, cx| {
        assert!(reader.page_text.finish(request, Ok(text)));
        reader.finish_ruler_placement();
        cx.notify();
    });
    settle(cx, window);
    reader.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
    });
}

#[gpui_kit::test]
fn ruler_distinguishes_loading_failure_and_textless_without_spatial_fallback(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/readme-sample.pdf", 0);
    let request = reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        reader
            .page_text
            .request(0..1, None, [None; 2], reader.document_generation)
            .unwrap()
    });
    cx.simulate_keystrokes(window, "r");
    reader.read_with(cx, |reader, _| {
        assert!(reader.status().contains("loading page text"))
    });
    reader.update(cx, |reader, cx| {
        assert!(
            reader
                .page_text
                .finish(request, Err("extraction failed".into()))
        );
        reader.finish_ruler_placement();
        cx.notify();
    });
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
        assert!(reader.status().contains("couldn’t read page text"));
    });
    cx.simulate_keystrokes(window, "escape");
    let mut text = pdf::page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
    text.glyphs.clear();
    text.lines.clear();
    reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        let request = reader
            .page_text
            .request(0..1, None, [None; 2], reader.document_generation)
            .unwrap();
        assert!(reader.page_text.finish(request, Ok(text)));
    });
    cx.simulate_keystrokes(window, "r");
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.target.is_none());
        assert!(reader.status().contains("no usable text"));
    });
    cx.simulate_keystrokes(window, "j");
    settle(cx, window);
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.enabled);
        assert!(
            reader
                .ruler
                .target
                .as_ref()
                .is_some_and(|target| target.page > 0)
        );
    });
}

#[gpui_kit::test]
fn ruler_reveals_edges_at_zoom_in_both_layouts_and_reactivation_uses_visible_text(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    for mode in [ReadingMode::SinglePage, ReadingMode::Continuous] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.select_page(2, cx);
            reader.change_scale(2. / reader.scale, cx);
        });
        settle(cx, window);
        cx.simulate_keystrokes(window, "r");
        let initial = target(&reader, cx);
        for _ in 0..12 {
            cx.simulate_keystrokes(window, "j");
        }
        assert_ne!(target(&reader, cx), initial);
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            let marker = frame.find("reading-ruler-marker").bounds();
            let viewport = frame.find("page").bounds();
            assert!(marker.top() >= viewport.top(), "{marker:?} {viewport:?}");
            assert!(
                marker.bottom() <= viewport.bottom(),
                "{marker:?} {viewport:?}"
            );
            assert!(marker.left() < viewport.right() && marker.right() > viewport.left());
        })
        .unwrap();
        cx.simulate_keystrokes(window, "r");
        reader.update(cx, |reader, cx| reader.scroll_by(-10000., cx));
        settle(cx, window);
        cx.simulate_keystrokes(window, "r");
        assert!(reader.read_with(cx, |reader, _| reader.ruler_visible()));
        cx.simulate_keystrokes(window, "r");
    }
}

fn boundary(reader: &Entity<ReaderView>, cx: &mut TestAppContext, page: usize, last: bool) {
    let text =
        std::sync::Arc::new(pdf::page_text(Path::new("examples/reading-order.pdf"), page).unwrap());
    reader.update(cx, |reader, cx| {
        let line = if last { text.lines.len() - 1 } else { 0 };
        reader.ruler.cancel_work();
        reader.ruler.enabled = true;
        reader.ruler.target = Some(Target { page, line, text });
        reader.reveal_ruler(cx);
    });
}

#[gpui_kit::test]
fn ruler_crosses_pages_both_directions_and_reveals_columns_at_high_zoom(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.change_scale(3. / reader.scale, cx);
        });
        boundary(&reader, cx, 2, true);
        settle(cx, window);
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (3, 0));
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            let marker = frame.find("reading-ruler-marker").bounds();
            let viewport = frame.find("page").bounds();
            assert!(marker.top() >= viewport.top());
            assert!(marker.bottom() <= viewport.bottom());
            assert!(marker.left() < viewport.right() && marker.right() > viewport.left());
        })
        .unwrap();
        cx.simulate_keystrokes(window, "k");
        settle(cx, window);
        assert_eq!(target(&reader, cx).0, 2);
        reader.read_with(cx, |reader, _| {
            assert!(reader.ruler_visible());
            assert!(reader.ruler.traversal.is_none());
        });
        // Column transition has an upward reveal, not a jump to another page.
        let column_end = reader.read_with(cx, |reader, _| {
            let text = &reader.ruler.target.as_ref().unwrap().text;
            (0..text.lines.len() - 1)
                .find(|ix| {
                    let a = Target {
                        page: 2,
                        line: *ix,
                        text: text.clone(),
                    }
                    .page_rect(reader.pages[2])
                    .unwrap();
                    let b = Target {
                        page: 2,
                        line: ix + 1,
                        text: text.clone(),
                    }
                    .page_rect(reader.pages[2])
                    .unwrap();
                    b[1] < a[1] && b[0] > a[0]
                })
                .expect("column transition")
        });
        reader.update(cx, |reader, cx| {
            reader.ruler.target.as_mut().unwrap().line = column_end;
            reader.reveal_ruler(cx);
        });
        let before = reader.read_with(cx, |reader, _| reader.viewport().0);
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (2, column_end + 1));
        reader.read_with(cx, |reader, _| {
            assert!(reader.viewport().0 < before);
            assert!(reader.ruler_visible());
        });
    }
}

#[gpui_kit::test]
fn ruler_browsing_debounces_retarget_without_reveal_or_feedback(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    reader.update(cx, |reader, cx| reader.change_scale(3. / reader.scale, cx));
    boundary(&reader, cx, 2, false);
    settle(cx, window);
    let initial = target(&reader, cx);
    // A small gesture retains the same line.
    cx.update_window(window, |_, frame, cx| {
        frame.scroll(
            "page",
            gpui::ScrollDelta::Pixels(point(px(0.), px(-1.))),
            cx,
        );
    })
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(200));
    settle(cx, window);
    assert_eq!(target(&reader, cx), initial);
    for _ in 0..4 {
        cx.update_window(window, |_, frame, cx| {
            frame.scroll(
                "page",
                gpui::ScrollDelta::Pixels(point(px(0.), px(-200.))),
                cx,
            );
        })
        .unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(100));
        settle(cx, window);
        assert_eq!(target(&reader, cx), initial);
    }
    let offset = reader.read_with(cx, |reader, _| reader.scroll.offset());
    cx.background_executor
        .advance_clock(Duration::from_millis(200));
    settle(cx, window);
    assert_ne!(target(&reader, cx), initial);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.scroll.offset(), offset);
        assert!(reader.ruler_visible());
        assert!(reader.ruler.browse_task.is_none());
    });
    settle(cx, window);
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.scroll.offset()),
        offset
    );
}
