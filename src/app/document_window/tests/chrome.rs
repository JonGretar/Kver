use super::*;

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
