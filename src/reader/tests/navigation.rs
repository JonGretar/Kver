use super::*;

#[gpui_kit::test]
fn reading_keys_navigate_and_zoom_without_screenshots(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let window = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    window
        .read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)))
        .unwrap();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
            .unwrap();
    });
    cx.simulate_keystrokes(window.into(), "right =");
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert_eq!(reader.scale(), 1.25);
        })
        .unwrap();
}

#[gpui_kit::test]
fn reader_navigation_is_window_local(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let first = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    let second = cx.add_window(|window, cx| Preview::new(None, window.scale_factor(), cx));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
        cx.update_window(second.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    cx.simulate_keystrokes(first.into(), "right =");
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 1);
            assert_eq!(reader.scale(), 1.25);
        })
        .unwrap();
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_count(), None);
            assert_eq!(reader.page_index(), 0);
            assert_eq!(reader.scale(), 1.0);
        })
        .unwrap();
}

#[gpui_kit::test]
fn fit_width_tracks_window_resize_and_manual_zoom_exits_fit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    window
        .update(cx, |reader, _, cx| reader.fit_width(cx))
        .unwrap();
    let before = window.read_with(cx, |reader, _| reader.scale()).unwrap();
    assert!(before > 0.25);

    cx.simulate_window_resize(window.into(), size(px(600.0), px(700.0)));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    let after = window.read_with(cx, |reader, _| reader.scale()).unwrap();
    assert!(after < before, "a narrower viewport must reduce fit width");
    window
        .update(cx, |reader, _, cx| reader.zoom_in(cx))
        .unwrap();
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
            assert!((reader.scale() - after * 1.25).abs() < 0.01);
        })
        .unwrap();
}

#[gpui_kit::test]
fn fit_page_scrolling_does_not_jump_back_across_a_page_boundary(cx: &mut TestAppContext) {
    let path = PathBuf::from("target/test-fixtures/fit-scroll-regression.pdf");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, three_page_pdf([(384, 504), (378, 288), (352, 294)])).unwrap();
    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| Preview::new(Some(path), window.scale_factor(), cx));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    window
        .update(cx, |reader, _, cx| reader.fit_page(cx))
        .unwrap();

    let mut highest = 0;
    for _ in 0..20 {
        window
            .update(cx, |reader, _, cx| reader.scroll_by(80.0, cx))
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        let state = window
            .read_with(cx, |reader, _| (reader.page_index(), reader.scale()))
            .unwrap();
        assert!(
            state.0 >= highest,
            "fit-page went backwards without scrolling backwards"
        );
        highest = state.0;
        cx.update(|cx| {
            cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
                .unwrap();
        });
        assert_eq!(
            window
                .read_with(cx, |reader, _| (reader.page_index(), reader.scale()))
                .unwrap(),
            state,
            "fit-page changed without input"
        );
    }
    assert!(highest >= 1, "scrolling must reach page two");
}
