//! Open Recent through the composed reader, including palette and input routing.

use super::*;

#[gpui_kit::test]
fn command_palette_opens_recent_with_search_focus(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (window, reader) = recent_test_window(cx, None);
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowCommandPalette), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.input("Open Recent", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        let palette = reader.palette.as_ref().unwrap();
        assert_eq!(palette.query(cx), "Open Recent");
        assert!(palette.has_selection(cx));
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
            .unwrap();
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert!(reader.palette.is_none());
        assert!(reader.recent.is_some());
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            assert_eq!(frame.find("recent-panel").label(), Some("Open Recent"));
            frame.input("book", cx);
        })
        .unwrap();
    });
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.recent.as_ref().unwrap().read(cx).query(cx), "book");
    });
}

#[gpui_kit::test]
fn successful_open_records_normal_path_and_recent_reopens_in_active_window(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (_first, first_reader) = recent_test_window(cx, None);
    let (second, second_reader) = recent_test_window(cx, None);
    first_reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
    });
    cx.run_until_parked();
    let normalized = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    assert!(store.contains(&normalized).unwrap());
    second_reader.read_with(cx, |reader, _| assert!(reader.path.is_none()));
    cx.update(|cx| {
        cx.update_window(second, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(second, |_, window, cx| window.press("enter", cx))
            .unwrap();
    });
    cx.run_until_parked();
    second_reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(100));
        assert!(reader.recent.is_none());
    });
    first_reader.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)));
}

#[gpui_kit::test]
fn recent_dialog_arrows_do_not_scroll_the_pdf(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    for name in ["field-guide", "selection-sample"] {
        store
            .record(
                &RecentStore::normalize(&PathBuf::from(format!("examples/{name}.pdf"))).unwrap(),
            )
            .unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (handle, reader) = recent_test_window(
        cx,
        Some(RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap()),
    );
    cx.run_until_parked();
    let before = reader.read_with(cx, |reader, _| reader.scroll.offset());
    cx.update(|cx| {
        cx.update_window(handle, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(handle, |_, frame, cx| frame.press("down", cx))
            .unwrap();
    });
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.scroll.offset(), before);
        assert_eq!(
            reader
                .recent
                .as_ref()
                .unwrap()
                .read(cx)
                .selected_row(cx)
                .unwrap(),
            1
        );
    });
}

#[gpui_kit::test]
fn an_old_document_open_cannot_dismiss_a_new_recent_dialog(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    RecentStore::new(&paths).record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, reader) = recent_test_window(cx, None);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let old = reader.read_with(cx, |reader, _| reader.recent.as_ref().unwrap().entity_id());
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.update_window(window, |_, frame, cx| frame.press("escape", cx))
        .unwrap();
    assert!(reader.read_with(cx, |reader, _| reader.recent.is_none()));
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    let new = reader.read_with(cx, |reader, _| reader.recent.as_ref().unwrap().entity_id());
    assert_ne!(old, new);
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.page_count(), Some(100));
        let picker = reader
            .recent
            .as_ref()
            .expect("new picker was dismissed by old open");
        assert_eq!(picker.entity_id(), new);
        assert!(!picker.read(cx).files().iter().any(|file| file.path == pdf));
        assert!(!picker.read(cx).is_loading());
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(frame.find("recent-panel").visible());
    })
    .unwrap();
}
