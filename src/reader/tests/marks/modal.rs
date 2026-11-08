use super::*;

fn open_marks(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    RecentStore,
    PathBuf,
    AnyWindowHandle,
    Entity<ReaderView>,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("marks.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    store
        .replace_marks(
            &pdf,
            &BTreeMap::from([
                ('a', mark(ReadingPosition { page: 1, y: 80.0 })),
                ('b', mark(ReadingPosition { page: 2, y: 40.0 })),
            ]),
        )
        .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "cmd-shift-m");
    cx.run_until_parked();
    (root, store, pdf, window, reader)
}

#[gpui_kit::test]
fn deleting_mark_keeps_popup_keyboard_input_and_escape(cx: &mut TestAppContext) {
    let (_root, store, pdf, window, reader) = open_marks(cx);
    let position = reader.read_with(cx, |reader, _| reader.reading_position());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("delete-mark:a", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'a'));
    reader.read_with(cx, |reader, cx| assert!(reader.marks_open(cx)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("b", cx);
        window.press("down", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).query(cx),
            "b",
            "typing after deletion must still reach marks search"
        );
        assert_eq!(
            reader.reading_position(),
            position,
            "modal arrows must not scroll the PDF"
        );
    });
    cx.simulate_keystrokes(window, "escape escape");
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| assert!(!reader.marks_open(cx)));
}

#[gpui_kit::test]
fn clicking_marks_search_preserves_keyboard_focus(cx: &mut TestAppContext) {
    let (_root, _store, _pdf, window, reader) = open_marks(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("input-state", cx);
        window.input("b", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.marks.as_ref().unwrap().read(cx).query(cx), "b");
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
}

#[gpui_kit::test]
fn clicking_outside_marks_after_delete_dismisses_popup(cx: &mut TestAppContext) {
    let (_root, store, pdf, window, reader) = open_marks(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("delete-mark:a", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'a'));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click_at("document-window", point(px(10.0), px(100.0)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(
            !reader.marks_open(cx),
            "clicking outside the marks surface must dismiss it"
        )
    });
    cx.simulate_keystrokes(window, "down");
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.reading_position()),
        (0, 100.0)
    );
}

#[gpui_kit::test]
fn standard_marks_set_jump_and_delete_through_dialogs(cx: &mut TestAppContext) {
    let (_root, store, pdf, window, reader) = open_marks(cx);
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    // Open Marks starts with the Set command selected, even when marks exist.
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).view(),
            Some(MarksView::Set)
        );
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("c    Unset", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).query(cx),
            "c    Unset"
        );
    });
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(store.marks(&pdf).unwrap()[&'c'].position.page, 1);
    reader.read_with(cx, |reader, cx| assert!(!reader.marks_open(cx)));

    reader.update(cx, |reader, cx| reader.select_page(2, cx));
    cx.simulate_keystrokes(window, "cmd-shift-m");
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("c    Page", cx);
    })
    .unwrap();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.page_index(), 1);
        assert!(!reader.marks_open(cx));
    });

    cx.simulate_keystrokes(window, "cmd-shift-p");
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("Delete local mark", cx);
    })
    .unwrap();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).view(),
            Some(MarksView::Delete)
        );
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("c    Page", cx);
    })
    .unwrap();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'c'));
}

#[gpui_kit::test]
fn standard_name_cancel_and_save_restore_search_focus(cx: &mut TestAppContext) {
    let (_root, store, pdf, window, reader) = open_marks(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("tab", cx);
        assert_eq!(frame.find("name-mark:a").focused(), Some(true));
        frame.press("space", cx);
        reader.read_with(cx, |reader, cx| {
            assert_eq!(
                reader.marks.as_ref().unwrap().read(cx).view(),
                Some(MarksView::Name('a'))
            );
        });
        frame.input("Draft", cx);
        frame.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(store.marks(&pdf).unwrap()[&'a'].label, None);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("a", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.marks.as_ref().unwrap().read(cx).query(cx), "a");
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("name-mark:a", cx);
        frame.input("Chapter", cx);
        frame.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
        Some("Chapter")
    );
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("b", cx);
        frame.press("down", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.marks.as_ref().unwrap().read(cx).query(cx), "ab");
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
    cx.simulate_keystrokes(window, "escape escape");
    reader.read_with(cx, |reader, cx| assert!(!reader.marks_open(cx)));
    cx.simulate_keystrokes(window, "down");
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.reading_position()),
        (0, 100.0)
    );
}

#[gpui_kit::test]
fn standard_keyboard_delete_keeps_search_focused(cx: &mut TestAppContext) {
    let (_root, store, pdf, window, reader) = open_marks(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("tab", cx);
        frame.press("tab", cx);
        assert_eq!(frame.find("delete-mark:a").focused(), Some(true));
        frame.press("space", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'a'));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("b", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.marks.as_ref().unwrap().read(cx).query(cx), "b");
    });
}
