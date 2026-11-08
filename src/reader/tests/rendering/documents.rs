use super::*;

#[gpui_kit::test]
fn long_pdf_keeps_single_page_work_bounded_and_restores_its_last_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let file = root.path().join("long.pdf");
    std::fs::write(&file, pdf_with_boxes(&[(612, 792); 500], &[""; 500])).unwrap();
    let pdf = RecentStore::normalize(&file).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(500)));
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
        })
        .unwrap();
    });
    reader.update(cx, |reader, cx| reader.select_page(499, cx));
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 499);
        assert!(
            cached_pages(reader).count() <= 2,
            "Single Page eagerly rendered {} pages",
            cached_pages(reader).count()
        );
    });

    cx.update(|cx| {
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, reopened) = recent_test_window(cx, Some(pdf));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.page_index(), 499);
        assert!(cached_pages(reader).count() <= 2);
    });
}

#[gpui_kit::test]
fn image_only_page_can_scroll_and_keep_a_local_mark_without_text(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let file = root.path().join("image-only.pdf");
    std::fs::write(&file, image_only_pdf()).unwrap();
    let pdf = RecentStore::normalize(&file).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    assert!(pdf::page_text(&pdf, 0).unwrap().glyphs.is_empty());
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(SinglePage), cx);
            frame.scroll(
                "page",
                gpui::ScrollDelta::Pixels(point(px(0.0), px(-1200.0))),
                cx,
            );
            frame.dispatch_action(Box::new(SetLocalMark('s')), cx);
            frame.dispatch_action(Box::new(NextPage), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(1));
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert!(cached_pages(reader).count() <= 1);
    });
    let (_, other) = recent_test_window(cx, Some(pdf));
    cx.run_until_parked();
    other.update(cx, |reader, cx| reader.jump_mark('s', cx));
    cx.run_until_parked();
    other.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}
