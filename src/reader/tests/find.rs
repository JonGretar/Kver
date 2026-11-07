//! Reader geometry and raster invariants for composed Find/Contents navigation.

use super::*;

#[gpui_kit::test]
fn stale_highlight_cannot_restore_previous_document_presentation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-f", cx);
        frame.render_frame(cx);
        frame.input("This is page", cx);
        frame.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (generation, highlight) = reader.read_with(cx, |reader, _| {
        (
            reader.document_generation,
            reader.find_highlight.clone().expect("selected Find result"),
        )
    });
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        assert!(reader.find_highlight.is_none());
        reader.set_find_highlight(generation, Some(highlight), cx);
        assert!(reader.find_highlight.is_none());
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert!(reader.find_highlight.is_none()));
}

#[gpui_kit::test]
fn single_page_find_jump_and_window_modes_are_local(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (first_window, first) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let (_, second) = recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    first.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx)
    });
    cx.update_window(first_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowFind), cx);
    })
    .unwrap();
    cx.update_window(first_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("This is page 3 of 100", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(first_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(
            frame
                .find(("find-result", 0usize))
                .label()
                .unwrap()
                .starts_with("Page 3:")
        );
        frame.click(("find-result", 0usize), cx);
    })
    .unwrap();
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 2);
        assert_eq!(reader.reading_position().0, 2);
        assert_eq!(reader.layout().visible_pages(0.0, 700.0), 2..3);
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
    });
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
    });
    first.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    cx.run_until_parked();
    let destination = pdf::document_outline(Path::new("examples/field-guide.pdf")).unwrap()[0]
        .destination()
        .unwrap();
    first.update(cx, |reader, cx| {
        // A new document starts with the configured default view.
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.navigate_outline(destination, cx);
    });
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 2);
        assert_eq!(reader.reading_position().0, 2);
        assert_eq!(reader.layout().visible_pages(0.0, 700.0), 2..3);
    });
    cx.run_until_parked();
    first.read_with(cx, |reader, _| {
        assert!(reader.images.keys().all(|page| *page == 2));
        assert!(reader.retained_bitmap_bytes() <= BITMAP_BUDGET);
    });
}
