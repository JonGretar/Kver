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
