//! Page-local restoration, shared saved places, and session-only positions.
use super::*;

#[gpui_kit::test]
fn wheel_scrolled_position_is_saved_for_reopen(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("wheel.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.scroll(
                "page",
                gpui::ScrollDelta::Pixels(point(px(0.0), px(-1800.0))),
                cx,
            );
        })
        .unwrap();
    });
    cx.run_until_parked();
    let position = first.read_with(cx, |reader, _| reader.reading_position());
    assert!(position.0 > 0 || position.1 > 0.0, "wheel did not scroll");
    let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), position)
    });
}

#[gpui_kit::test]
fn saved_single_page_mode_restores_its_page_local_position(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("single-page.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
            frame.dispatch_action(Box::new(NextPage), cx);
        })
        .unwrap();
    });
    first.update(cx, |reader, cx| reader.scroll_by(180.0, cx));
    cx.run_until_parked();
    let position = first.read_with(cx, |reader, _| reader.reading_position());
    assert_eq!(position.0, 1);
    assert!(position.1 > 0.0);
    let (reopened_window, reopened) = recent_test_window(cx, Some(pdf));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(reopened_window, |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.reading_position().0, position.0);
        assert!((reader.reading_position().1 - position.1).abs() < 2.0);
    });
}

#[gpui_kit::test]
fn private_single_page_place_and_mark_follow_the_pdf_across_windows_and_restart(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let file = root.path().join("rotated.pdf");
    std::fs::write(
        &file,
        three_page_pdf_with_boxes(
            [(600, 1400), (800, 1500), (700, 1300)],
            ["", "/Rotate 90 /CropBox [40 30 700 1350]", ""],
        ),
    )
    .unwrap();
    let pdf = RecentStore::normalize(&file).unwrap();
    let store = RecentStore::new(&paths);
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
            frame.dispatch_action(Box::new(NextPage), cx);
            frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(store.marks(&pdf).unwrap()[&'a'].position.page, 1);

    // Disabling history keeps the reader usable without writing new state.
    cx.update(|cx| cx.dispatch_action(&ShowSettings));
    cx.run_until_parked();
    let settings_window = cx.update(|cx| *cx.windows().last().unwrap());
    cx.update(|cx| {
        cx.update_window(settings_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("reading-history", cx);
            frame.click("remember-reading-history", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(NextPage), cx);
            frame.dispatch_action(Box::new(SetLocalMark('b')), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| first.read(cx).page_index()), 2);
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
    assert!(!store.marks(&pdf).unwrap().contains_key(&'b'));

    let (other_window, other) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    other.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.page_index(), 2);
    });
    cx.update(|cx| {
        cx.update_window(other_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(JumpLocalMark('b')), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));

    cx.update(|cx| {
        cx.update_window(settings_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("remember-reading-history", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, 2);
    assert_eq!(store.marks(&pdf).unwrap()[&'b'].position.page, 2);

    // Reinstalling the services represents a fresh app session reading disk.
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reopened) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.page_index(), 2);
    });
    reopened.update(cx, |reader, cx| {
        reader.select_page(0, cx);
        reader.jump_mark('b', cx);
    });
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
}

#[gpui_kit::test]
fn page_turn_to_short_page_reopens_at_selected_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("slides.pdf");
    std::fs::write(&pdf, pdf_with_boxes(&[(600, 120); 2], &["", ""])).unwrap();
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
            frame.dispatch_action(Box::new(NextPage), cx);
            frame.render_frame(cx);
            frame.scroll(
                "page",
                gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                cx,
            );
        })
        .unwrap();
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert!(reader.position_ready);
    });
    assert_eq!(
        RecentStore::new(&paths)
            .position(&pdf)
            .unwrap()
            .map(|position| position.page),
        Some(1),
        "the selected page was never saved"
    );
    let (reopened_window, reopened) = recent_test_window(cx, Some(pdf));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(reopened_window, |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
}

#[gpui_kit::test]
fn reading_position_survives_restart_zoom_and_independent_windows(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("rotated.pdf");
    std::fs::write(
        &pdf,
        three_page_pdf_with_boxes(
            [(600, 1400), (800, 1500), (700, 1300)],
            ["", "/Rotate 90 /CropBox [40 30 700 1350]", ""],
        ),
    )
    .unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_first_window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.read_with(cx, |reader, _| {
        assert!(reader.position_ready, "{}", reader.status())
    });
    first.update(cx, |reader, cx| reader.scroll_by(1550.0, cx));
    cx.run_until_parked();
    let saved = first.read_with(cx, |reader, _| reader.reading_position());
    assert_eq!(saved.0, 1);
    assert!(saved.1 > 0.0);
    first.read_with(cx, |reader, _| {
        assert_eq!(
            reader.recorded_position,
            Some(ReadingPosition {
                page: saved.0,
                y: saved.1
            }),
            "{}",
            reader.status()
        );
    });
    let store = RecentStore::new(&paths);
    assert!(store.contains(&pdf).unwrap());
    assert_eq!(
        store.position(&pdf).unwrap(),
        Some(ReadingPosition {
            page: saved.0,
            y: saved.1
        })
    );

    // A second live window restores the place, then moves independently.
    let (_second_window, second) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    second.read_with(cx, |reader, _| assert_eq!(reader.reading_position(), saved));
    second.update(cx, |reader, cx| reader.next_page(cx));
    cx.run_until_parked();
    first.read_with(cx, |reader, _| assert_eq!(reader.reading_position(), saved));
    let later = second.read_with(cx, |reader, _| reader.reading_position());
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, later.0);

    // Reinstalling both services against the same paths models a new process.
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::InitialZoom(InitialZoom::FitWidth),
    )
    .unwrap();
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_third_window, third) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    third.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position().0, later.0);
        assert!((reader.reading_position().1 - later.1).abs() < 2.0);
    });
}

#[gpui_kit::test]
fn restoration_off_preserves_saved_place_until_a_new_position_is_chosen(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("document.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    store
        .save_position(&pdf, ReadingPosition { page: 1, y: 200.0 })
        .unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::ReopenAtLastPosition(false),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.read_with(cx, |reader, _| {
        assert!(reader.position_ready, "{}", reader.status())
    });
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), (0, 0.0))
    });
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::ReopenAtLastPosition(true),
    )
    .unwrap();
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, restored) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    restored.read_with(cx, |reader, _| {
        assert!(reader.position_ready, "{}", reader.status());
    });
    restored.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), (1, 200.0))
    });
}

#[gpui_kit::test]
fn private_positions_share_with_windows_and_flush_only_when_reenabled(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("private.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
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
    first.update(cx, |reader, cx| reader.select_page(1, cx));
    cx.run_until_parked();
    let store = RecentStore::new(&paths);
    assert!(!paths.local_data().join("reading.sqlite3").exists());
    let (_, second) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    second.read_with(cx, |reader, _| assert_eq!(reader.reading_position().0, 1));
    // Reinstalling the service models a restart; the private place disappears.
    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, after_restart) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    after_restart.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), (0, 0.0))
    });
    after_restart.update(cx, |reader, cx| reader.select_page(2, cx));
    cx.run_until_parked();
    let history = cx.update(|cx| recent::history(cx).unwrap());
    let work = history.set_recording(true);
    cx.update(|cx| {
        cx.spawn(async move |cx| {
            work.await.unwrap();
            cx.update(|_| ());
        })
        .detach()
    });
    cx.run_until_parked();
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, 2);
}

#[gpui_kit::test]
fn forgetting_pdf_clears_session_position_before_reopen(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("forgotten.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    cx.run_until_parked();
    let store = RecentStore::new(&paths);
    assert_eq!(store.position(&pdf).unwrap().unwrap().page, 1);
    let history = cx.update(|cx| recent::history(cx).unwrap());
    let removal = history.remove(pdf.clone());
    cx.update(|cx| {
        cx.spawn(async move |cx| {
            removal.await.unwrap();
            cx.update(|_| ());
        })
        .detach()
    });
    cx.run_until_parked();
    assert_eq!(store.position(&pdf).unwrap(), None);
    reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), (0, 0.0))
    });
}

#[gpui_kit::test]
fn changed_pdf_bounds_saved_position_and_stale_load_cannot_replace_new_document(
    cx: &mut TestAppContext,
) {
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
        .save_position(&pdf, ReadingPosition { page: 2, y: 9000.0 })
        .unwrap();
    // Rebuild at the same path with different page geometry: clamp the saved offset.
    std::fs::write(
        &pdf,
        three_page_pdf([(600, 1400), (600, 1400), (600, 2000)]),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position().0, 2);
        assert!(reader.reading_position().1 <= 2000.0);
    });
    store
        .save_position(&pdf, ReadingPosition { page: 80, y: 100.0 })
        .unwrap();
    // A rebuilt file with fewer pages rejects the old index.
    std::fs::write(&pdf, pdf_with_boxes(&[(600, 1400)], &[""])).unwrap();
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    let (_, next) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    next.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), (0, 0.0))
    });
    next.update(cx, |reader, cx| {
        reader.open_path(pdf.clone(), cx);
        reader.open_path(root.path().join("missing.pdf"), cx);
    });
    cx.run_until_parked();
    next.read_with(cx, |reader, _| {
        assert_eq!(
            reader.path(),
            Some(root.path().join("missing.pdf").as_path())
        );
        assert_eq!(reader.page_count(), None);
    });
}
