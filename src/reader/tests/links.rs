use super::*;

fn linked_reader(cx: &mut TestAppContext) -> (tempfile::TempDir, AnyWindowHandle, Entity<Preview>) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("linked.pdf");
    std::fs::write(&path, pdf::linked_fixture()).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (window, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    (root, window, reader)
}

#[gpui_kit::test]
fn pointer_activation_follows_link_and_invalid_links_report_without_navigation(
    cx: &mut TestAppContext,
) {
    let (_fixture, window, reader) = linked_reader(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(
            frame.find("pdf-link-0-0").label(),
            Some("Follow PDF link to page 2")
        );
        frame.click("pdf-link-0-4", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(
            reader.status(),
            "This PDF link has an invalid or unsupported destination"
        );
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1, "{}", reader.status());
        assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
        assert_eq!(reader.scale(), 1.5);
        assert_eq!(reader.reading_position().0, 1);
        assert!((reader.reading_position().1 - 500.0).abs() < 1.0);
        assert_eq!(reader.selected_text(), None);
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
}

#[gpui_kit::test]
fn keyboard_activation_and_fits_use_same_destination_path(cx: &mut TestAppContext) {
    let (_fixture, window, reader) = linked_reader(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        // The document chrome has preceding Tab stops. Establish an existing
        // focus path, then use the real Root Tab binding to reach PDF links.
        for _ in 0..24 {
            if frame.find("pdf-link-0-0").focused() == Some(true) {
                break;
            }
            frame.focus_next(cx);
            frame.render_frame(cx);
        }
        assert_eq!(frame.find("pdf-link-0-0").focused(), Some(true));
        frame.press("tab", cx);
        assert_eq!(frame.find("pdf-link-0-1").focused(), Some(true));
        frame.press("tab", cx);
        assert_eq!(frame.find("pdf-link-0-2").focused(), Some(true));
        frame.press("enter", cx);
        assert_eq!(reader.read(cx).page_index(), 1);
        assert!(reader.read(cx).focus.is_focused(frame));
        frame.press("p", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn fit_width_link_changes_zoom_mode_without_affecting_other_windows(cx: &mut TestAppContext) {
    let (_fixture, window, reader) = linked_reader(cx);
    let (_, other) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-3", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
    });
    other.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn illustrated_field_guide_link_follows_named_destination(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (window, reader) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(
            frame.find("pdf-link-0-0").label(),
            Some("Follow PDF link to page 8")
        );
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 7));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn stale_link_activation_cannot_navigate_replacement(cx: &mut TestAppContext) {
    let (_fixture, _, reader) = linked_reader(cx);
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        reader.activate_link(generation, 0, 0, cx);
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.path(), Some(Path::new("examples/field-guide.pdf")));
        assert!(reader.links.visible(0).is_some());
    });
}
