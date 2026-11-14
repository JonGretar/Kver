use super::*;

#[gpui_kit::test]
fn ruler_external_url_and_two_active_windows_keep_independent_targets(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("external-link.pdf");
    let mut pdf = lopdf::Document::load("examples/reading-order.pdf").unwrap();
    for object in pdf.objects.values_mut() {
        if let Ok(dictionary) = object.as_dict_mut()
            && dictionary
                .get(b"Subtype")
                .is_ok_and(|value| value.as_name().is_ok_and(|name| name == b"Link"))
        {
            dictionary.remove(b"Dest");
            dictionary.set(
                "A",
                lopdf::dictionary! {
                    "S" => "URI", "URI" => lopdf::Object::string_literal("https://example.com/")
                },
            );
        }
    }
    pdf.save(&path).unwrap();
    let (first_window, first) = open(cx, path.to_str().unwrap(), 2);
    let (second_window, second) = open(cx, path.to_str().unwrap(), 2);
    cx.simulate_keystrokes(first_window, "r j");
    cx.simulate_keystrokes(second_window, "r j j j");
    let before = target(&first, cx);
    let other = target(&second, cx);
    cx.update_window(first_window, |_, frame, cx| frame.click("pdf-link-2-1", cx))
        .unwrap();
    settle(cx, first_window);
    assert_eq!(target(&first, cx), before);
    assert_eq!(target(&second, cx), other);
    cx.simulate_keystrokes(first_window, "shift-j");
    settle(cx, first_window);
    assert_eq!(target(&first, cx), (3, 0));
    assert_eq!(target(&second, cx), other);
    first.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
    second.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
}

#[gpui_kit::test]
fn ruler_links_history_and_contents_retarget_without_per_line_jump_events(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    let jumps = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = jumps.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&reader, move |_, _: &JumpEvent, _| {
            count.set(count.get() + 1);
        })
    });
    cx.simulate_keystrokes(window, "r j j k");
    assert_eq!(jumps.get(), 0);
    cx.update_window(window, |_, frame, cx| frame.click("pdf-link-2-0", cx))
        .unwrap();
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 3);
    assert_eq!(jumps.get(), 1);
    cx.update_window(window, |_, frame, cx| frame.click("navigate-back", cx))
        .unwrap();
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 2);
    cx.update_window(window, |_, frame, cx| frame.click("navigate-forward", cx))
        .unwrap();
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 3);
    assert_eq!(jumps.get(), 1);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));

    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    settle(cx, window);
    cx.simulate_keystrokes(window, "r");
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(ShowContents), cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        frame.click(("contents-entry", 0usize), cx)
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 2);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_marks_retarget_and_modal_escape_preserves_mode(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r m a");
    settle(cx, window);
    cx.simulate_keystrokes(window, "shift-j");
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 3);
    cx.simulate_keystrokes(window, "' a");
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 2);
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(ShowMarks), cx)
    })
    .unwrap();
    settle(cx, window);
    let initial = target(&reader, cx);
    cx.update_window(window, |_, frame, cx| {
        frame.click("name-mark:a", cx);
        frame.input("rjk", cx);
        frame.press("escape", cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.marks_open(cx));
        assert!(reader.ruler.enabled);
    });
    assert_eq!(target(&reader, cx), initial);
    cx.simulate_keystrokes(window, "escape");
    settle(cx, window);
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.marks_open(cx));
        assert!(reader.ruler.enabled);
    });
}

#[gpui_kit::test]
fn ruler_geometry_changes_preserve_target_and_cancel_pending_browse(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r j j");
    let initial = target(&reader, cx);
    reader.update(cx, |reader, cx| {
        reader.browse_ruler(cx);
        reader.zoom_in(cx);
        reader.set_reading_mode(ReadingMode::Continuous, cx);
    });
    cx.simulate_window_resize(window, size(px(650.), px(600.)));
    cx.background_executor
        .advance_clock(Duration::from_millis(200));
    settle(cx, window);
    assert_eq!(target(&reader, cx), initial);
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.fit_page(cx);
    });
    settle(cx, window);
    assert_eq!(target(&reader, cx), initial);
}

#[gpui_kit::test]
fn ruler_reload_resolves_fresh_text_independently_and_rejects_old_reply(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reload.pdf");
    std::fs::copy("examples/reading-order.pdf", &path).unwrap();
    let (first_window, first) = open(cx, path.to_str().unwrap(), 2);
    let (second_window, second) = recent_test_window(cx, Some(path.clone()));
    settle(cx, second_window);
    cx.simulate_keystrokes(first_window, "r j j");
    let initial = target(&first, cx);
    let old_text = first.read_with(cx, |reader, _| {
        reader.ruler.target.as_ref().unwrap().text.clone()
    });
    let generation = first.read_with(cx, |reader, _| reader.document_generation);
    let request = super::traversal::hold_next(&first, cx);
    // External rewrite exercises the shared session epoch and production reload.
    let mut pdf = lopdf::Document::load(&path).unwrap();
    pdf.renumber_objects_with(500);
    pdf.save(&path).unwrap();
    cx.executor().advance_clock(Duration::from_secs(2));
    settle(cx, first_window);
    settle(cx, second_window);
    first.read_with(cx, |reader, cx| {
        assert!(reader.document_generation > generation);
        assert!(reader.ruler.enabled);
        assert!(!std::sync::Arc::ptr_eq(
            &old_text,
            &reader.ruler.target.as_ref().unwrap().text
        ));
        assert!(!reader.is_dirty(cx));
    });
    assert_eq!(target(&first, cx), initial);
    first.update(cx, |reader, _| {
        assert!(!reader.page_text.finish(request, Err("old reply".into())));
    });
    second.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
    });
}

#[gpui_kit::test]
fn ruler_page_jumps_immediately_choose_first_line_in_both_layouts(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r j j");
    for mode in [ReadingMode::SinglePage, ReadingMode::Continuous] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.select_page(2, cx);
        });
        settle(cx, window);
        assert_eq!(target(&reader, cx), (2, 0));
        reader.update(cx, |reader, cx| {
            reader.change_scale(2. / reader.scale, cx);
            reader.scroll_horizontal_by(1000., cx);
        });
        cx.simulate_keystrokes(window, "shift-j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (3, 0));
        reader.read_with(cx, |reader, _| assert_eq!(reader.scroll.offset().x, px(0.)));
        cx.simulate_keystrokes(window, "shift-k");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (2, 0));
        reader.read_with(cx, |reader, cx| {
            assert!(reader.ruler.enabled);
            assert!(!reader.is_dirty(cx));
        });
    }
}

#[gpui_kit::test]
fn ruler_search_targets_matching_column_and_escape_is_layered(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r /");
    cx.update_window(window, |_, frame, cx| {
        frame.input("Article right begins", cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        frame.click(("find-result", 0usize), cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, _| {
        let target = reader.ruler.target.as_ref().unwrap();
        let line = target.text.glyphs[target.text.lines[target.line].clone()]
            .iter()
            .map(|glyph| glyph.text.as_str())
            .collect::<String>();
        assert!(line.contains("Article right begins"), "{line}");
        assert!(reader.ruler.enabled);
    });
    cx.update_window(window, |_, frame, cx| {
        frame.click("find-query", cx);
        frame.press("cmd-a", cx);
        frame.input("rjk", cx);
        assert_eq!(frame.find("find-query").value(), Some("rjk"));
        frame.press("escape", cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| assert!(!reader.ruler.enabled));
}

#[gpui_kit::test]
fn ruler_selection_copy_markup_and_dismissal_restore_without_scroll(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("markup.pdf");
    std::fs::copy("examples/readme-sample.pdf", &path).unwrap();
    let (window, reader) = open(cx, path.to_str().unwrap(), 0);
    cx.simulate_keystrokes(window, "r");
    let initial = target(&reader, cx);
    let offset = reader.read_with(cx, |reader, _| reader.scroll.offset());
    let (start, end) = reader.read_with(cx, |reader, _| {
        let text = reader.page_text.get(0).unwrap();
        let viewport = reader.page_viewport(0).unwrap();
        let at = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let at = viewport.document_to_screen(Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            point(px(at.x as f32), px(at.y as f32))
        };
        (at(0, 0.1), at(3, 0.9))
    });
    cx.update_window(window, |_, frame, cx| {
        frame.pointer_down(start, gpui::MouseButton::Left, cx);
        assert!(frame.try_find("reading-ruler-marker").is_none());
        frame.pointer_move(end, Some(gpui::MouseButton::Left), cx);
        frame.pointer_up(end, gpui::MouseButton::Left, cx);
        frame.press("cmd-c", cx);
        assert!(frame.try_find("reading-ruler-marker").is_none());
    })
    .unwrap();
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .as_deref(),
        Some("KVER")
    );
    cx.simulate_keystrokes(window, "j");
    assert_eq!(target(&reader, cx), initial);
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(Highlight), cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx));
        assert!(reader.selected_annotation.is_some());
        assert_eq!(reader.scroll.offset(), offset);
    });
    cx.simulate_keystrokes(window, "escape");
    reader.read_with(cx, |reader, _| assert!(reader.ruler.enabled));
    // Annotation deselection leaves the selected passage available for copy.
    cx.simulate_keystrokes(window, "escape");
    settle(cx, window);
    assert_eq!(target(&reader, cx), initial);
    cx.update_window(window, |_, frame, cx| {
        assert!(frame.find("reading-ruler-marker").visible());
        assert!(reader.read(cx).focus.is_focused(frame));
    })
    .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.scroll.offset(), offset));
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(UndoAnnotation), cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(RedoAnnotation), cx);
        frame.dispatch_action(Box::new(crate::app::document_window::Save), cx);
    })
    .unwrap();
    settle(cx, window);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.ruler.enabled);
        assert!(!reader.is_dirty(cx));
        assert_eq!(reader.scroll.offset(), offset);
    });
    assert_eq!(target(&reader, cx), initial);
    assert_eq!(
        pdf::Document::open(&path)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
}

#[gpui_kit::test]
fn ruler_delayed_destination_supersession_never_steals_find_focus(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    cx.simulate_keystrokes(window, "r");
    let request = super::traversal::hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "shift-j shift-k /");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.ruler.pending.unwrap().0, 2);
        assert!(reader.ruler.target.is_none());
    });
    let text = pdf::page_text(Path::new("examples/reading-order.pdf"), 3).unwrap();
    reader.update(cx, |reader, cx| {
        assert!(reader.page_text.finish(request, Ok(text)));
        reader.finish_ruler_placement();
        reader.schedule_text(cx);
    });
    settle(cx, window);
    assert_eq!(target(&reader, cx), (2, 0));
    cx.update_window(window, |_, frame, cx| {
        assert_eq!(frame.find("find-query").focused(), Some(true));
        frame.input("still typing", cx);
        assert_eq!(frame.find("find-query").value(), Some("still typing"));
    })
    .unwrap();
}
