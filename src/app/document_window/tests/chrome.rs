use super::*;

#[gpui_kit::test]
fn filename_stays_centered_without_overlapping_titlebar_controls(cx: &mut TestAppContext) {
    use gpui_kit::{component::theme::Theme, px, size};

    setup(cx);
    for path in [
        None,
        Some(PathBuf::from("examples/readme-sample.pdf")),
        Some(PathBuf::from(format!(
            "{}.pdf",
            "long-document-name-".repeat(12)
        ))),
    ] {
        let expected = path
            .as_ref()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .unwrap_or_else(|| "A quiet place to read".into());
        let (handle, _) = open(cx, path);
        cx.run_until_parked();
        for font_size in [12., 16., 20.] {
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            for width in [360., 600., 920., 1200.] {
                cx.simulate_window_resize(handle, size(px(width), px(800.)));
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    let title = window.find("document-name");
                    let open = window.find("open-pdf");
                    assert!(window.try_find("reader-brand").is_none());
                    assert!(window.try_find("document-unsaved").is_none());
                    assert_eq!(title.label(), Some(expected.as_str()));
                    assert!((title.bounds().center().x - px(width / 2.)).abs() <= px(0.5));
                    assert!(title.bounds().size.width > px(0.));
                    assert!(title.bounds().left() >= px(0.));
                    assert!(title.bounds().right() < open.bounds().left());
                    assert!(open.visible());
                    assert!(open.bounds().right() <= px(width));
                })
                .unwrap();
            }
        }
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
    }
}

#[gpui_kit::test]
fn page_count_and_zoom_keep_their_anchors_as_page_numbers_grow(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let mut anchors = None;
    for page in [0, 8, 9, 99] {
        reader.update(cx, |reader, cx| reader.select_page(page, cx));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let status = window.find("reader-status");
            let zoom = window.find("reader-zoom");
            assert!(window.try_find("document-unsaved").is_none());
            assert_eq!(
                status.label(),
                Some(format!("Page {} of 100", page + 1).as_str())
            );
            assert_eq!(zoom.label(), Some("100%"));
            let current = (status.bounds().left(), zoom.bounds().right());
            assert_eq!(*anchors.get_or_insert(current), current);
            assert!(status.bounds().right() < zoom.bounds().left());
        })
        .unwrap();
    }
}
