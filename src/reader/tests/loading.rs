use super::*;

#[gpui_kit::test]
fn opening_and_replacing_documents_ignores_obsolete_loads(cx: &mut TestAppContext) {
    let reader = cx.new(|cx| Preview::new(None, 1.0, cx));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.status(), "Open a PDF to start reading");
    });

    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/missing.pdf"), cx);
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(100));
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.status(), "Page 1 of 100 · 100%");
    });

    reader.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.zoom_in(cx);
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.scale(), 1.25);
    });
    reader.update(cx, |reader, cx| reader.previous_page(cx));
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
}

#[gpui_kit::test]
fn opening_zoom_uses_settings_per_document_without_refitting_other_windows(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let pdf = root.path().join("pages.pdf");
    std::fs::write(&pdf, three_page_pdf([(400, 1200), (400, 800), (400, 1200)])).unwrap();
    let config = root.path().join("config.toml");
    std::fs::write(&config, "[reader]\ninitial_zoom = 'fit_width'\n").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        crate::settings_ui::install_with_paths(
            cx,
            Some(crate::settings::AppPaths::for_locations(
                config.clone(),
                root.path().join("data"),
            )),
        );
    });
    let first =
        cx.add_window(|frame, cx| Preview::new(Some(pdf.clone()), frame.scale_factor(), cx));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
            assert_eq!(reader.page_count(), Some(3));
            assert!(reader.scale() > 1.0);
        })
        .unwrap();
    let first_scale = first.read_with(cx, |reader, _| reader.scale()).unwrap();

    // A live preference edit changes only subsequent document opens.
    std::fs::write(
        &config,
        "[reader]\ninitial_zoom = 'fit_page'\nreading_view = 'page'\n",
    )
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
            assert_eq!(reader.scale(), first_scale);
            assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        })
        .unwrap();
    let second =
        cx.add_window(|frame, cx| Preview::new(Some(pdf.clone()), frame.scale_factor(), cx));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert!(reader.scale() < first_scale);
        })
        .unwrap();
    let second_scale = second.read_with(cx, |reader, _| reader.scale()).unwrap();
    cx.simulate_window_resize(second.into(), size(px(600.), px(500.)));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
            assert!(reader.scale() < second_scale);
        })
        .unwrap();

    // The reader's actual keyboard Actions still change its local mode
    // without changing the preference or another reader.
    second
        .update(cx, |reader, _, cx| reader.scroll_by(80.0, cx))
        .unwrap();
    let position = second
        .read_with(cx, |reader, _| reader.reading_position())
        .unwrap();
    cx.simulate_keystrokes(second.into(), "w");
    cx.run_until_parked();
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
            let after = reader.reading_position();
            assert_eq!(after.0, position.0);
            assert!((after.1 - position.1).abs() < 2.0);
        })
        .unwrap();
    cx.simulate_keystrokes(second.into(), "=");
    cx.run_until_parked();
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
            let after = reader.reading_position();
            assert_eq!(after.0, position.0);
            assert!((after.1 - position.1).abs() < 2.0);
        })
        .unwrap();
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
            assert_eq!(reader.scale(), first_scale);
        })
        .unwrap();

    // Replacing a PDF through the same path as picker/Recent takes the new
    // default, while selecting 100% later resets only future opens.
    std::fs::write(&config, "[reader]\ninitial_zoom = '100_percent'\n").unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    second
        .update(cx, |reader, _, cx| reader.open_path(pdf, cx))
        .unwrap();
    cx.run_until_parked();
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
            assert!((reader.scale() - 1.0).abs() < 0.001);
        })
        .unwrap();
}

#[gpui_kit::test]
fn missing_config_finishes_startup_before_loading_at_100_percent(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("missing/config.toml");
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::settings_ui::install_with_paths(
            cx,
            Some(crate::settings::AppPaths::for_locations(
                config.clone(),
                root.path().join("data"),
            )),
        );
    });
    let reader = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    reader
        .read_with(cx, |reader, _| {
            assert!(reader.page_count().is_some());
            assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
            assert_eq!(reader.scale(), 1.0);
        })
        .unwrap();
    assert!(!config.exists());
}

#[gpui_kit::test]
fn loading_errors_leave_the_reader_without_a_document(cx: &mut TestAppContext) {
    let reader = cx.new(|cx| Preview::new(Some(PathBuf::from("examples/missing.pdf")), 1.0, cx));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert!(
            reader.status().starts_with("Cannot read "),
            "{}",
            reader.status()
        );
        assert_eq!(reader.page_count(), None);
    });

    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("Cargo.toml"), cx);
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.status(),
            "Cannot open PDF: invalid or unsupported PDF file"
        );
        assert_eq!(reader.page_count(), None);
    });
}
