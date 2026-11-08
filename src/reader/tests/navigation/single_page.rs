use super::*;

#[gpui_kit::test]
fn single_page_bounds_wheel_and_explicit_turns_on_real_page_geometry(cx: &mut TestAppContext) {
    let path = PathBuf::from("target/test-fixtures/single-page-geometry.pdf");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        three_page_pdf_with_boxes(
            [(360, 300), (600, 1600), (600, 1000)],
            [
                "",
                "/CropBox [20 40 580 1540] /Rotate 180",
                "/CropBox [50 30 550 970] /Rotate 90",
            ],
        ),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (window, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.pages,
            vec![(360.0, 300.0), (560.0, 1500.0), (940.0, 500.0)]
        );
    });

    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(SinglePage), cx)
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.layout().visible_pages(0.0, 700.0), 0..1);
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            for _ in 0..4 {
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-5000.0))),
                    cx,
                );
            }
        })
        .unwrap();
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
    cx.simulate_keystrokes(window, "right");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.reading_position().0, 1);
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.scroll(
                "page",
                gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                cx,
            );
        })
        .unwrap();
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert!(reader.reading_position().1 > 0.0);
    });
    reader.update(cx, |reader, cx| reader.scroll_by(10000.0, cx));
    let bottom = reader.read_with(cx, |reader, _| reader.reading_position().1);
    assert!(bottom > 500.0);
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            for _ in 0..4 {
                frame.scroll(
                    "page",
                    gpui::ScrollDelta::Pixels(point(px(0.0), px(-5000.0))),
                    cx,
                );
            }
        })
        .unwrap();
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert!((reader.reading_position().1 - bottom).abs() < 1.0);
        assert_eq!(reader.layout().visible_pages(0.0, 700.0), 1..2);
    });
    reader.update(cx, |reader, cx| reader.zoom_in(cx));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert!((reader.reading_position().1 - bottom).abs() < 1.0);
    });
    reader.update(cx, |reader, cx| reader.fit_width(cx));
    let before_resize = reader.read_with(cx, |reader, _| reader.scale());
    cx.simulate_window_resize(window, size(px(600.0), px(700.0)));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert!(reader.scale() < before_resize);
        assert!(reader.reading_position().1 <= reader.pages[1].1);
    });
    cx.simulate_keystrokes(window, "right");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 2);
        assert_eq!(reader.reading_position().0, 2);
    });
    cx.simulate_keystrokes(window, "right");
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 2);
    cx.simulate_keystrokes(window, "left");
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(Continuous), cx)
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        assert_eq!(reader.reading_position().0, 1);
    });
}
