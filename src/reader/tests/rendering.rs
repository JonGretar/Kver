use super::*;

mod documents;

pub(super) fn cached_pages(reader: &ReaderView) -> impl Iterator<Item = usize> + '_ {
    reader.rasters.cached_pages()
}

#[gpui_kit::test]
fn display_density_change_during_open_does_not_discard_document_load(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        settings_ui::install_with_paths(cx, None);
    });
    cx.run_until_parked();
    let window = cx.add_window(|window, cx| {
        ReaderView::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    // Do not run the pending document load before changing display density.
    let before = window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), None);
            assert_eq!(reader.status(), "Loading document…");
            reader.display_scale
        })
        .unwrap();
    let density = if before == 2.0 { 1.0 } else { 2.0 };
    cx.simulate_window_scale_factor_change(window.into(), density);
    cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
        .unwrap();
    window
        .read_with(cx, |reader, _| assert_eq!(reader.display_scale, density))
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), Some(100));
            assert_eq!(reader.displayed_page_index(), Some(0));
            assert_eq!(reader.scale(), 1.0);
            assert!(reader.status().starts_with("Page 1 of "));
        })
        .unwrap();
}

#[gpui_kit::test]
fn zoom_and_density_replace_pixels_without_changing_document_position(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        settings_ui::install_with_paths(cx, None);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.scroll_by(100.0, cx);
    });
    cx.run_until_parked();
    let (old_image, position, generation) = reader.read_with(cx, |reader, _| {
        (
            reader.rasters.image(0).unwrap().clone(),
            reader.reading_position(),
            reader.document_generation(),
        )
    });
    reader.update(cx, |reader, cx| {
        reader.zoom_in(cx);
        assert_eq!(
            reader.retained_bitmap_bytes(),
            0,
            "old scale pixels are released immediately"
        );
    });
    cx.run_until_parked();
    let (zoomed_image, bytes, density, scale) = reader.read_with(cx, |reader, _| {
        let image = reader.rasters.image(0).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&old_image, image));
        assert_eq!(reader.reading_position().0, position.0);
        assert!((reader.reading_position().1 - position.1).abs() < 1.0);
        (
            image.clone(),
            reader.retained_bitmap_bytes(),
            reader.display_scale,
            reader.scale(),
        )
    });
    cx.simulate_window_scale_factor_change(window, if density == 2.0 { 1.0 } else { 2.0 });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.retained_bitmap_bytes(),
            0,
            "old density pixels are released immediately"
        );
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert!(!std::sync::Arc::ptr_eq(
            &zoomed_image,
            reader.rasters.image(0).unwrap()
        ));
        assert_ne!(reader.retained_bitmap_bytes(), bytes);
        assert_eq!(reader.scale(), scale);
        assert_eq!(reader.document_generation(), generation);
        assert_eq!(reader.reading_position().0, position.0);
        assert!((reader.reading_position().1 - position.1).abs() < 1.0);
        assert_eq!(cached_pages(reader).collect::<Vec<_>>(), vec![0]);
    });
}

#[gpui_kit::test]
fn replacing_document_during_raster_work_keeps_only_new_document_pixels(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        settings_ui::install_with_paths(cx, None);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let replacement = PathBuf::from("examples/field-guide.pdf");
    let page_count = pdf::document_pages(&replacement).unwrap().len();
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.select_page(50, cx);
        reader.zoom_in(cx);
        reader.open_path(replacement.clone(), cx);
        assert_eq!(reader.retained_bitmap_bytes(), 0);
        assert_eq!(reader.displayed_page_index(), None);
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.path(), Some(replacement.as_path()));
        assert_eq!(reader.page_count(), Some(page_count));
        assert_eq!(reader.displayed_page_index(), Some(0));
        assert_eq!(reader.scale(), 1.0);
        assert!(cached_pages(reader).all(|page| page < page_count));
        assert!(reader.retained_bitmap_bytes() <= BITMAP_BUDGET);
        assert!(reader.status().starts_with("Page 1 of "));
    });
}
