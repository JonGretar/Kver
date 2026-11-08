use super::*;

#[gpui_kit::test]
fn mode_override_survives_reopen_but_use_default_follows_new_preference(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let file = root.path().join("pages.pdf");
    std::fs::write(&file, three_page_pdf([(400, 1200); 3])).unwrap();
    let pdf = RecentStore::normalize(&file).unwrap();
    let store = RecentStore::new(&paths);
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (first_window, first) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| first.read(cx).reading_mode()),
        ReadingMode::Continuous
    );
    assert_eq!(cx.update(|cx| first.read(cx).mode_override()), None);
    let (_, already_open) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    first.update(cx, |reader, cx| reader.scroll_by(80.0, cx));
    let position = cx.update(|cx| first.read(cx).reading_position());
    cx.update(|cx| {
        cx.update_window(first_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(store.reading_mode(&pdf).unwrap(), Some(ReadingView::Page));
    let after = cx.update(|cx| first.read(cx).reading_position());
    assert_eq!(after.0, position.0);
    assert!(
        (after.1 - position.1).abs() < 2.0,
        "before {position:?}, after {after:?}"
    );
    assert_eq!(
        cx.update(|cx| already_open.read(cx).reading_mode()),
        ReadingMode::Continuous
    );

    // Another window already reading this PDF keeps its live mode, while
    // every fresh open path converges on the saved override.
    let (_, second) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| second.read(cx).reading_mode()),
        ReadingMode::SinglePage
    );
    let (empty_window, empty) = recent_test_window(cx, None);
    cx.update(|cx| {
        cx.update_window(empty_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update_window(empty_window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| empty.read(cx).mode_override()),
        Some(ReadingMode::SinglePage)
    );

    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::ReadingView(ReadingView::Page),
    )
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(first_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(UseDefaultReadingMode), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| first.read(cx).mode_override()), None);
    assert_eq!(store.reading_mode(&pdf).unwrap(), None);
    // Clearing the override does not copy Page: the next preference edit
    // affects the next open, not an existing reader.
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::ReadingView(ReadingView::Continuous),
    )
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| first.read(cx).reading_mode()),
        ReadingMode::SinglePage
    );
    assert_eq!(
        cx.update(|cx| second.read(cx).reading_mode()),
        ReadingMode::SinglePage
    );
    empty.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| empty.read(cx).reading_mode()),
        ReadingMode::Continuous
    );
    assert_eq!(cx.update(|cx| empty.read(cx).mode_override()), None);

    // Even a choice equal to the default becomes a real PDF override.
    cx.update(|cx| {
        cx.update_window(first_window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(Continuous), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(
        store.reading_mode(&pdf).unwrap(),
        Some(ReadingView::Continuous)
    );
    empty.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| empty.read(cx).mode_override()),
        Some(ReadingMode::Continuous)
    );
}

#[gpui_kit::test]
fn private_mode_is_session_only_then_flushes_on_reenable(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    let store = RecentStore::new(&paths);
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
            frame.dispatch_action(Box::new(SinglePage), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    assert!(!paths.local_data().join("reading.sqlite3").exists());
    reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| reader.read(cx).mode_override()),
        Some(ReadingMode::SinglePage)
    );
    // Replace the worker as if the process restarted: no private state
    // crossed the disk boundary.
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| reader.read(cx).mode_override()), None);
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
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
    assert_eq!(store.reading_mode(&pdf).unwrap(), Some(ReadingView::Page));
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| reader.read(cx).mode_override()),
        Some(ReadingMode::SinglePage)
    );
    cx.update(|cx| {
        let history = recent::history(cx).unwrap();
        cx.spawn(async move |cx| {
            let _ = history.clear().await;
            cx.update(recent::notify_changed);
        })
        .detach();
    });
    cx.run_until_parked();
    assert_eq!(store.reading_mode(&pdf).unwrap(), None);
    reader.update(cx, |reader, cx| reader.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| reader.read(cx).mode_override()), None);
}
