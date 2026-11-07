//! Marks input, popup, and document-lifecycle integration.

use super::*;

#[gpui_kit::test]
fn mark_letter_overrides_reader_page_shortcut(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("book.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    cx.simulate_keystrokes(window, "m");
    cx.simulate_keystrokes(window, "p");
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    let original = reader.read_with(cx, |reader, _| reader.position_to_save());
    for letter in ['c', 'f', 'j', 'k', 'n', 's', 'w'] {
        cx.simulate_keystrokes(window, &format!("m {letter}"));
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.position_to_save(),
                original,
                "reader shortcut ran for {letter}"
            );
            assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
            assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
        });
    }
    cx.run_until_parked();
    let store = RecentStore::new(&paths);
    for letter in ['c', 'f', 'j', 'k', 'n', 'p', 's', 'w'] {
        assert_eq!(
            store
                .marks(&pdf)
                .unwrap()
                .get(&letter)
                .map(|mark| mark.position.page),
            Some(1),
            "mark {letter} was not set"
        );
    }
    reader.update(cx, |reader, cx| reader.select_page(2, cx));
    cx.simulate_keystrokes(window, "' p");
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    cx.simulate_keystrokes(window, "d m p");
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'p'));
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    // Once a sequence completes, the ordinary one-key binding is active again.
    cx.simulate_keystrokes(window, "p");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    reader.update(cx, |reader, cx| reader.select_page(2, cx));
    cx.simulate_keystrokes(window, "m escape p");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    cx.simulate_keystrokes(window, "m cmd-f");
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.input("e", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'e'));
}

#[gpui_kit::test]
fn naming_a_local_mark_in_popup_survives_replacing_and_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("named.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
            frame.dispatch_action(Box::new(ShowMarks), cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("name-mark:a", cx);
        })
        .unwrap()
    });
    reader.read_with(cx, |reader, cx| {
        assert!(
            reader.marks_open(cx),
            "name button closed popup synchronously: {}",
            reader.status
        );
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).view(),
            Some(MarksView::Name('a'))
        )
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| frame.press("escape", cx))
            .unwrap()
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).view(),
            Some(MarksView::List)
        )
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("name-mark:a", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.input("Introduction", cx);
            frame.press("enter", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.marks.as_ref().unwrap().read(cx).view(),
            Some(MarksView::List)
        )
    });
    let store = RecentStore::new(&paths);
    assert_eq!(
        store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
        Some("Introduction")
    );
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    assert_eq!(
        store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
        Some("Introduction")
    );
    assert_eq!(store.marks(&pdf).unwrap()[&'a'].position.page, 1);
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reopened.update(cx, |reader, cx| reader.jump_mark('a', cx));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    assert_eq!(
        store.marks(&pdf).unwrap()[&'a'].label.as_deref(),
        Some("Introduction")
    );
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("name-mark:a", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            // The test API's synthetic "cmd-a" does not model the Linux
            // text-selection modifier. Set an empty field, then exercise
            // the real popup Save control on both platforms.
            reader.update(cx, |reader, cx| {
                let input = reader.marks.as_ref().unwrap().read(cx).name_input().clone();
                input.update(cx, |input, cx| input.set_value("", frame, cx));
            });
            frame.render_frame(cx);
            frame.click("save-mark-name", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    assert_eq!(store.marks(&pdf).unwrap()[&'a'].label, None);
    reader.read_with(cx, |reader, cx| {
        assert!(
            reader.marks_open(cx),
            "saving name unexpectedly closed popup"
        );
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("delete-mark:a", cx);
        })
        .unwrap()
    });
    cx.run_until_parked();
    assert!(store.marks(&pdf).unwrap().is_empty());
}

#[gpui_kit::test]
fn replacing_document_rejects_old_mark_events_and_drops_popup(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let first = root.path().join("first.pdf");
    let next = root.path().join("next.pdf");
    for path in [&first, &next] {
        std::fs::write(path, three_page_pdf([(600, 1400); 3])).unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, reader) = recent_test_window(cx, Some(first));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "cmd-shift-m");
    cx.run_until_parked();
    let old = reader.read_with(cx, |reader, _| reader.marks.clone().unwrap());
    reader.update(cx, |reader, cx| reader.open_path(next, cx));
    cx.run_until_parked();
    old.update(cx, |_, cx| {
        cx.emit(MarksEvent::Jump {
            letter: 'a',
            position: ReadingPosition { page: 2, y: 80.0 },
        });
        cx.emit(MarksEvent::Status("obsolete reply".into()));
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.page_index(), 0);
        assert!(!reader.marks_open(cx));
        assert_ne!(reader.status(), "obsolete reply");
    });
    let weak = old.downgrade();
    drop(old);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}
