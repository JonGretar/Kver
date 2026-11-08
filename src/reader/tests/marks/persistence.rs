//! Marks across shared history, private sessions, and document windows.
use super::*;

#[gpui_kit::test]
fn local_marks_share_across_windows_restart_and_palette(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("marked.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = crate::app::keymap::resolve(
            crate::app::keymap::Platform::Mac,
            crate::app::keymap::Preset::Vim,
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.update(cx, |reader, cx| reader.scroll_by(1550.0, cx));
    let saved = first.read_with(cx, |reader, _| reader.position_to_save());
    cx.simulate_keystrokes(window, "m a");
    cx.run_until_parked();
    assert_eq!(
        RecentStore::new(&paths).marks(&pdf).unwrap().get(&'a'),
        Some(&mark(saved))
    );

    let (other_window, other) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    other.update(cx, |reader, cx| {
        reader.select_page(2, cx);
        reader.choose_reading_mode(Some(ReadingMode::SinglePage), cx);
        reader.fit_width(cx);
    });
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(JumpLocalMark('a')), cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    other.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), saved.page);
        assert!((reader.reading_position().1 - saved.y).abs() < 2.0);
    });
    first.read_with(cx, |reader, _| assert_eq!(reader.position_to_save(), saved));

    cx.simulate_keystrokes(other_window, "cmd-shift-m");
    cx.run_until_parked();
    other.read_with(cx, |reader, cx| {
        let marks = reader.marks.as_ref().expect("marks owner").read(cx);
        assert!(marks.is_open());
        assert_eq!(marks.listed_marks().get(&'a'), Some(&mark(saved)));
        assert_eq!(marks.query(cx), "");
    });
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.find("reader-marks-palette");
            frame.input("Page 2", cx);
        })
        .unwrap()
    });
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| frame.press("enter", cx))
            .unwrap()
    });
    cx.run_until_parked();
    other.read_with(cx, |reader, cx| assert!(!reader.marks_open(cx)));
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowCommandPalette), cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.input("Open marks", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update_window(other_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("input-state").value(), Some("Open marks"));
    })
    .unwrap();
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| frame.press("enter", cx))
            .unwrap()
    });
    cx.run_until_parked();
    other.read_with(cx, |reader, cx| assert!(reader.marks_open(cx)));
    cx.simulate_keystrokes(other_window, "escape");
    other.update(cx, |reader, cx| reader.select_page(2, cx));
    cx.simulate_keystrokes(other_window, "' a");
    cx.run_until_parked();
    other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), saved.page));

    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reopened.update(cx, |reader, cx| reader.select_page(0, cx));
    reopened.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), saved.page));
    cx.simulate_keystrokes(other_window, "d m a");
    cx.run_until_parked();
    assert!(RecentStore::new(&paths).marks(&pdf).unwrap().is_empty());
}

#[gpui_kit::test]
fn private_marks_replace_delete_flush_and_clear(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("private.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    let old = ReadingPosition { page: 0, y: 40.0 };
    store
        .replace_marks(&pdf, &BTreeMap::from([('a', mark(old))]))
        .unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position().1, old.y)
    });
    first.update(cx, |reader, cx| reader.select_page(1, cx));
    first.update(cx, |reader, cx| {
        reader.change_mark('a', Some(reader.position_to_save()), cx)
    });
    first.update(cx, |reader, cx| {
        reader.change_mark('b', Some(reader.position_to_save()), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        store.marks(&pdf).unwrap(),
        BTreeMap::from([('a', mark(old))])
    );
    let (_, second) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    second.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    second.update(cx, |reader, cx| reader.change_mark('a', None, cx));
    cx.run_until_parked();
    let history = cx.update(|cx| recent::history(cx).unwrap());
    let name = history.name_mark(pdf.clone(), 'b', Some("Chapter two".into()));
    cx.update(|cx| {
        cx.spawn(async move |cx| {
            name.await.unwrap();
            cx.update(|_| ());
        })
        .detach()
    });
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'b'));
    let flush = history.set_recording(true);
    cx.update(|cx| {
        cx.spawn(async move |cx| {
            flush.await.unwrap();
            cx.update(|_| ());
        })
        .detach()
    });
    cx.run_until_parked();
    assert_eq!(
        store
            .marks(&pdf)
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec!['b']
    );
    assert_eq!(
        store.marks(&pdf).unwrap()[&'b'].label.as_deref(),
        Some("Chapter two")
    );
    let clear = history.clear();
    cx.update(|cx| {
        cx.spawn(async move |cx| {
            clear.await.unwrap();
            cx.update(|_| ());
        })
        .detach()
    });
    cx.run_until_parked();
    assert!(store.marks(&pdf).unwrap().is_empty());
    second.update(cx, |reader, cx| reader.jump_mark('b', cx));
    cx.run_until_parked();
    second.read_with(cx, |reader, _| assert!(reader.status.contains("not set")));
}

#[gpui_kit::test]
fn invalid_marks_and_text_focus_leave_reader_in_place(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("changed.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    store
        .replace_marks(
            &pdf,
            &BTreeMap::from([
                ('a', mark(ReadingPosition { page: 99, y: 20.0 })),
                ('b', mark(ReadingPosition { page: 0, y: 9000.0 })),
            ]),
        )
        .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert!(reader.status.contains("outside this PDF"));
    });
    reader.update(cx, |reader, cx| reader.jump_mark('b', cx));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert!(reader.reading_position().1 <= 1400.0)
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowFind), cx);
            frame.render_frame(cx);
            frame.input("m a", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'m'));
    std::fs::remove_file(&pdf).unwrap();
    assert_eq!(store.marks(&pdf).unwrap().len(), 2);
}

#[gpui_kit::test]
fn recording_off_discards_new_marks_on_restart_but_reads_saved_ones(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("book.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    store
        .replace_marks(
            &pdf,
            &BTreeMap::from([('a', mark(ReadingPosition { page: 1, y: 80.0 }))]),
        )
        .unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.update(cx, |reader, cx| {
        reader.change_mark('a', None, cx);
        reader.change_mark('b', Some(ReadingPosition { page: 2, y: 200.0 }), cx);
    });
    cx.run_until_parked();
    let (_, same_session) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    same_session.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    same_session.read_with(cx, |reader, _| assert!(reader.status.contains("not set")));
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, restarted) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    restarted.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    restarted.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    assert_eq!(
        store
            .marks(&pdf)
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec!['a']
    );
}
