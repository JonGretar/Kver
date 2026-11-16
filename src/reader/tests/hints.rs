use super::*;

#[gpui_kit::test]
fn visual_select_ignores_late_text_and_reloaded_endpoint_choices(cx: &mut TestAppContext) {
    stale_selection_work(cx, false);
}

#[gpui_kit::test]
fn visual_select_line_ignores_late_text_and_reloaded_endpoint_choices(cx: &mut TestAppContext) {
    stale_selection_work(cx, true);
}

fn stale_selection_work(cx: &mut TestAppContext, lines: bool) {
    let action = || {
        if lines {
            Box::new(VisualSelectLine) as Box<dyn gpui::Action>
        } else {
            Box::new(VisualSelect)
        }
    };
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    super::authoring::select_title(cx, handle, &reader, &path);
    let request = reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        reader
            .page_text
            .request(0..1, None, [None], reader.document_generation)
            .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(action(), cx);
    })
    .unwrap();
    super::authoring::draw(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window.find("hint-input").label(),
            Some("Loading visible text…")
        );
        window.press("a", cx);
        window.press("a", cx);
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(
        reader
            .read_with(cx, |reader, _| reader.selected_text())
            .as_deref(),
        Some("Alpha")
    );
    reader.update(cx, |reader, _| {
        assert!(
            reader
                .page_text
                .finish(request, Err("delayed failure".into()))
        );
    });
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("hint-input").is_none());
        window.dispatch_action(action(), cx);
    })
    .unwrap();
    super::authoring::draw(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window.find("hint-input").label(),
            Some("Couldn’t read page text. Reopen the document to retry.")
        );
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(
        reader
            .read_with(cx, |reader, _| reader.selected_text())
            .as_deref(),
        Some("Alpha")
    );
    reader.update(cx, |reader, cx| reader.open_path(path.clone(), cx));
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    cx.update_window(handle, |_, window, cx| window.dispatch_action(action(), cx))
        .unwrap();
    for _ in 0..3 {
        cx.run_until_parked();
        super::authoring::draw(cx, handle);
    }
    cx.update_window(handle, |_, window, cx| {
        window.press("s", cx);
        window.press("a", cx);
        assert_eq!(window.find("hint-input").label(), Some("Choose end: __"));
    })
    .unwrap();
    reader.update(cx, |reader, cx| reader.open_path(path, cx));
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    reader.update(cx, |reader, _| {
        assert!(!reader.page_text.finish(request, Err("obsolete".into())));
        assert_eq!(reader.selected_text(), None);
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("hint-input").is_none());
        window.press("s", cx);
        window.press("a", cx);
    })
    .unwrap();
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.selected_text()),
        None
    );
}

fn begin(cx: &mut TestAppContext, handle: AnyWindowHandle, reader: &Entity<ReaderView>) {
    cx.update_window(handle, |_, window, cx| {
        reader.update(cx, |reader, cx| {
            reader.begin_word_hints(|_| true, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
}

#[gpui_kit::test]
fn delayed_failed_and_stale_text_cannot_reopen_or_choose(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) = recent_test_window(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    // Hold the real shared-text request seam to control completion order.
    let request = reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        reader
            .page_text
            .request(0..1, None, [None], reader.document_generation)
            .unwrap()
    });
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window.find("hint-input").label(),
            Some("Loading visible text…")
        );
        window.press("a", cx);
        window.press("a", cx);
        assert!(window.try_find("hint-aa").is_none());
        assert!(window.try_find("hint-input").is_some());
    })
    .unwrap();
    reader.update(cx, |reader, cx| {
        reader.zoom_in(cx);
        assert!(
            reader
                .page_text
                .finish(request, Err("fixture extraction failure".into()))
        );
    });
    super::authoring::draw(cx, handle);
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window.find("hint-input").label(),
            Some("Couldn’t read page text. Reopen the document to retry.")
        );
        window.press("escape", cx);
    })
    .unwrap();
    // Replacement retires both pending text identity and target preparation.
    reader.update(cx, |reader, cx| {
        reader.open_path("examples/readme-sample.pdf".into(), cx)
    });
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    reader.update(cx, |reader, _| {
        assert!(!reader.page_text.finish(request, Err("obsolete".into())));
    });
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn hint_cancellation_and_choice_preserve_mouse_selection_and_ruler_target(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    super::authoring::select_title(cx, handle, &reader, &path);
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.selected_text()),
        Some("Alpha".into())
    );
    for keys in [["a", "escape"], ["a", "a"]] {
        begin(cx, handle, &reader);
        cx.update_window(handle, |_, window, cx| {
            for key in keys {
                window.press(key, cx);
            }
            assert!(window.try_find("hint-input").is_none());
        })
        .unwrap();
        assert_eq!(
            reader.read_with(cx, |reader, _| reader.selected_text()),
            Some("Alpha".into())
        );
    }
    // A ruler target is hidden during acquisition without changing its identity.
    reader.update(cx, |reader, cx| {
        reader.selection.clear();
        reader.toggle_ruler(cx);
    });
    cx.run_until_parked();
    super::authoring::draw(cx, handle);
    let target = reader.read_with(cx, |reader, _| {
        reader
            .ruler
            .target
            .as_ref()
            .map(|target| (target.page, target.line))
    });
    assert!(target.is_some());
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("reading-ruler-marker").is_none());
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(
        reader.read_with(cx, |reader, _| reader
            .ruler
            .target
            .as_ref()
            .map(|target| (target.page, target.line))),
        target
    );
    cx.update_window(handle, |_, window, _| {
        assert!(window.find("reading-ruler-marker").visible())
    })
    .unwrap();
}
