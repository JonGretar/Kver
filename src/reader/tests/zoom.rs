use super::*;
use gpui_kit::{InputEvent, Modifiers, PinchEvent, ScrollDelta, ScrollEvent, TouchPhase};

fn dispatch(cx: &mut TestAppContext, handle: AnyWindowHandle, event: impl InputEvent) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(event.to_platform_input(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn modifier() -> Modifiers {
    Modifiers {
        platform: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

#[gpui_kit::test]
fn precise_wheel_requires_platform_modifier_and_reader_hitbox(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    let pointer = reader.read_with(cx, |reader, _| reader.scroll.bounds().center());
    let scale = reader.read_with(cx, |reader, _| reader.scale);
    for modifiers in [
        Modifiers::default(),
        Modifiers {
            alt: true,
            ..modifier()
        },
        Modifiers {
            shift: true,
            ..modifier()
        },
        Modifiers {
            control: cfg!(target_os = "macos"),
            platform: !cfg!(target_os = "macos"),
            ..Default::default()
        },
    ] {
        dispatch(
            cx,
            handle,
            ScrollEvent {
                position: pointer,
                delta: ScrollDelta::Pixels(point(px(0.0), px(10.0))),
                modifiers,
                ..Default::default()
            },
        );
        assert_eq!(reader.read_with(cx, |reader, _| reader.scale), scale);
    }
    dispatch(
        cx,
        handle,
        ScrollEvent {
            position: point(px(5.0), px(5.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(10.0))),
            modifiers: modifier(),
            ..Default::default()
        },
    );
    assert_eq!(reader.read_with(cx, |reader, _| reader.scale), scale);
    for delta in [10.0, -10.0] {
        dispatch(
            cx,
            handle,
            ScrollEvent {
                position: pointer,
                delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
                modifiers: modifier(),
                ..Default::default()
            },
        );
        let expected = if delta > 0.0 {
            scale * 0.05_f32.exp()
        } else {
            scale
        };
        assert!((reader.read_with(cx, |reader, _| reader.scale) - expected).abs() < 0.0001);
    }
}

#[gpui_kit::test]
fn pinch_and_wheel_zoom_anchor_document_points_in_both_reading_modes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("zoom.pdf");
    std::fs::write(&path, three_page_pdf([(1200, 1600); 3])).unwrap();
    let (handle, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();

    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.select_page(1, cx);
            reader.scroll_by(200.0, cx);
        });
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let pointer = reader.read_with(cx, |reader, _| reader.scroll.bounds().center());
        for wheel in [false, true] {
            let (before, scale) = reader.read_with(cx, |reader, _| {
                (reader.pointer_document(1, pointer).unwrap(), reader.scale)
            });
            if wheel {
                dispatch(
                    cx,
                    handle,
                    ScrollEvent {
                        position: pointer,
                        delta: ScrollDelta::Lines(point(0.0, 1.0)),
                        modifiers: modifier(),
                        ..Default::default()
                    },
                );
            } else {
                dispatch(
                    cx,
                    handle,
                    PinchEvent {
                        position: pointer,
                        delta: 0.25,
                        phase: TouchPhase::Moved,
                        ..Default::default()
                    },
                );
            }
            reader.read_with(cx, |reader, _| {
                assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
                assert!((reader.scale - scale * 1.25).abs() < 0.001);
                let after = reader.pointer_document(1, pointer).unwrap();
                assert!((after.x - before.x).abs() < 1.0, "{before:?} -> {after:?}");
                assert!((after.y - before.y).abs() < 1.0, "{before:?} -> {after:?}");
                assert!(reader.retained_bitmap_bytes() > 0);
            });
        }
    }
}

#[gpui_kit::test]
fn zoom_input_preserves_plain_scroll_fit_and_modal_ownership(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.fit_width(cx));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let pointer = reader.read_with(cx, |reader, _| reader.scroll.bounds().center());
    for delta in [0.0, f32::NAN, -1.0] {
        dispatch(
            cx,
            handle,
            PinchEvent {
                position: pointer,
                delta,
                ..Default::default()
            },
        );
        assert_eq!(
            reader.read_with(cx, |reader, _| reader.zoom_mode()),
            ZoomMode::FitWidth
        );
    }
    dispatch(
        cx,
        handle,
        PinchEvent {
            position: pointer,
            delta: 0.1,
            ..Default::default()
        },
    );
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.zoom_mode()),
        ZoomMode::Fixed
    );
    let (scale, top) = reader.read_with(cx, |reader, _| (reader.scale, reader.viewport().0));
    dispatch(
        cx,
        handle,
        ScrollEvent {
            position: pointer,
            delta: ScrollDelta::Pixels(point(px(0.0), px(-80.0))),
            ..Default::default()
        },
    );
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.scale, scale);
        assert!(reader.viewport().0 > top);
    });

    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowRecent), cx);
        window.render_frame(cx);
    })
    .unwrap();
    dispatch(
        cx,
        handle,
        PinchEvent {
            position: pointer,
            delta: 0.25,
            ..Default::default()
        },
    );
    dispatch(
        cx,
        handle,
        ScrollEvent {
            position: pointer,
            delta: ScrollDelta::Lines(point(0.0, 1.0)),
            modifiers: modifier(),
            ..Default::default()
        },
    );
    assert_eq!(reader.read_with(cx, |reader, _| reader.scale), scale);
}

#[gpui_kit::test]
fn zoom_input_clamps_and_ignores_empty_reader(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, reader) = recent_test_window(cx, None);
    cx.run_until_parked();
    let pointer = reader.read_with(cx, |reader, _| reader.scroll.bounds().center());
    dispatch(
        cx,
        handle,
        PinchEvent {
            position: pointer,
            delta: 1.0,
            ..Default::default()
        },
    );
    assert_eq!(reader.read_with(cx, |reader, _| reader.scale), 1.0);
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/selection-sample.pdf"), cx)
    });
    cx.run_until_parked();
    for (delta, expected) in [(100.0, 4.0), (-0.99, 0.25)] {
        dispatch(
            cx,
            handle,
            PinchEvent {
                position: pointer,
                delta,
                ..Default::default()
            },
        );
        assert_eq!(reader.read_with(cx, |reader, _| reader.scale), expected);
    }
}
