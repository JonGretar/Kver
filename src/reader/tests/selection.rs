use super::*;
use gpui_kit::test::TestEventExt;

fn multi_click_down(
    window: &mut Window,
    at: gpui::Point<gpui::Pixels>,
    count: usize,
    cx: &mut App,
) {
    use gpui_kit::InputEvent as _;
    for click_count in 1..=count {
        window.render_frame(cx);
        window.dispatch_event(
            gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position: at,
                modifiers: window.modifiers(),
                click_count,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        if click_count < count {
            window.pointer_up(at, gpui::MouseButton::Left, cx);
        }
    }
}

#[gpui_kit::test]
fn multi_click_selection_drags_copies_and_retains_word_or_paragraph_granularity(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let text = pdf::page_text(&path, 0).unwrap();
    let (window, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.zoom_in(cx);
        });
        super::authoring::draw(cx, window);
        let positions = reader.read_with(cx, |reader, _| {
            let viewport = reader.page_viewport(0).unwrap();
            let screen = |line: usize, offset: usize| {
                let glyph = &text.glyphs[text.lines[line].start + offset];
                let at = viewport.document_to_screen(Point {
                    x: glyph.start.x + (glyph.end.x - glyph.start.x) * 0.8,
                    y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
                });
                point(px(at.x as f32), px(at.y as f32))
            };
            [
                screen(0, 8),
                screen(0, 14),
                screen(0, 1),
                screen(1, 8),
                screen(2, 8),
            ]
        });
        cx.update_window(window, |_, frame, cx| {
            multi_click_down(frame, positions[0], 2, cx)
        })
        .unwrap();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.selected_text().as_deref(), Some("café"));
        });
        for (at, expected) in [
            (positions[1], "café world"),
            (positions[0], "café"),
            (positions[2], "Alpha café"),
            (positions[3], "café world\nSecond naïve"),
        ] {
            cx.update_window(window, |_, frame, cx| {
                frame.pointer_move(at, Some(gpui::MouseButton::Left), cx)
            })
            .unwrap();
            reader.read_with(cx, |reader, _| {
                assert_eq!(reader.selected_text().as_deref(), Some(expected));
            });
        }
        cx.update_window(window, |_, frame, cx| {
            frame.pointer_up(positions[3], gpui::MouseButton::Left, cx)
        })
        .unwrap();
        cx.simulate_keystrokes(window, "cmd-c");
        assert_eq!(
            cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("café world\nSecond naïve".into())
        );
        cx.update_window(window, |_, frame, cx| {
            multi_click_down(frame, positions[0], 3, cx)
        })
        .unwrap();
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.selected_text().as_deref(),
                Some("Alpha café world\nSecond naïve line")
            );
        });
        cx.update_window(window, |_, frame, cx| {
            frame.pointer_move(positions[4], Some(gpui::MouseButton::Left), cx);
            frame.pointer_up(positions[4], gpui::MouseButton::Left, cx);
        })
        .unwrap();
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.selected_text().as_deref(),
                Some("Alpha café world\nSecond naïve line\n\nThird Þing ends")
            );
            assert_eq!(reader.selection.dragging_page(), None);
        });
    }
}

#[gpui_kit::test]
fn double_clicking_marked_text_keeps_text_selection_instead_of_selecting_annotation(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (window, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    super::authoring::select_title(cx, window, &reader, &path);
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(Highlight), cx)
    })
    .unwrap();
    super::authoring::draw(cx, window);
    let text = pdf::page_text(&path, 0).unwrap();
    let at = reader.read_with(cx, |reader, _| {
        let glyph = &text.glyphs[2];
        let at = reader.page_viewport(0).unwrap().document_to_screen(Point {
            x: (glyph.start.x + glyph.end.x) / 2.0,
            y: glyph.start.y + 2.0,
        });
        point(px(at.x as f32), px(at.y as f32))
    });
    cx.update_window(window, |_, frame, cx| {
        multi_click_down(frame, at, 2, cx);
        frame.pointer_up(at, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
        assert!(reader.selected_annotation.is_none());
        assert!(reader.annotation_choice.is_none());
    });
}

#[gpui_kit::test]
fn dragging_from_column_text_onto_a_link_copies_without_following_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/reading-order.pdf");
    let text = pdf::page_text(&path, 2).unwrap();
    let heading = text
        .lines
        .iter()
        .find(|range| {
            text.glyphs[(*range).clone()]
                .iter()
                .map(|glyph| glyph.text.as_str())
                .collect::<String>()
                .starts_with("Article right begins.")
        })
        .unwrap();
    let start = text.glyphs[heading.start].start;
    let (window, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.select_page(2, cx);
        reader.fit_page(cx);
    });
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let start = reader.read_with(cx, |reader, _| {
        let start = reader.page_viewport(2).unwrap().document_to_screen(start);
        point(px(start.x as f32), px(start.y as f32))
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        let end = frame.find("pdf-link-2-0").bounds().center();
        frame.drag(start, end, cx);
    })
    .unwrap();
    cx.simulate_keystrokes(window, "cmd-c");
    let copied = cx
        .update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert!(copied.starts_with("Article right begins."), "{copied}");
    assert!(copied.contains("The procedure is described"), "{copied}");
    assert!(!copied.contains("Article left"), "{copied}");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
    // A subsequent ordinary click must still use the same link.
    cx.update_window(window, |_, frame, cx| frame.click("pdf-link-2-0", cx))
        .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 3));
}

#[gpui_kit::test]
fn field_guide_cross_column_copy_uses_reading_order_after_zoom_and_scroll(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/field-guide.pdf");
    let text = pdf::page_text(&path, 7).unwrap();
    let heading = |phrase: &str| {
        let range = text
            .lines
            .iter()
            .find(|range| {
                text.glyphs[(*range).clone()]
                    .iter()
                    .map(|glyph| glyph.text.as_str())
                    .collect::<String>()
                    .starts_with(phrase)
            })
            .unwrap();
        text.glyphs[range.start].start
    };
    let start = heading("Route and reference points.");
    let right = heading("Reading across columns.");
    let (window, reader) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.select_page(7, cx);
        });
        cx.run_until_parked();
        for zoomed in [false, true] {
            if zoomed {
                reader.update(cx, |reader, cx| {
                    reader.zoom_in(cx);
                    reader.scroll_by(50.0, cx);
                });
                cx.run_until_parked();
            }
            cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
            let (start, end) = reader.read_with(cx, |reader, _| {
                let viewport = reader.page_viewport(7).unwrap();
                let screen = |at| {
                    let at = viewport.document_to_screen(at);
                    point(px(at.x as f32), px(at.y as f32))
                };
                // Stop at the next column's first boundary. The entire left
                // column, not just its first physical row, must be copied.
                (screen(start), screen(right))
            });
            for (start, end) in [(start, end), (end, start)] {
                cx.update_window(window, |_, frame, cx| frame.drag(start, end, cx))
                    .unwrap();
                cx.simulate_keystrokes(window, "cmd-c");
                let copied = cx
                    .update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
                    .unwrap();
                assert!(
                    copied.starts_with("Route and reference points."),
                    "{copied}"
                );
                assert!(copied.trim_end().ends_with("without color."), "{copied}");
                assert!(!copied.contains("Reading across columns."), "{copied}");
                reader.read_with(cx, |reader, _| {
                    assert_eq!(reader.page_index(), 7);
                    assert_eq!(reader.selection.dragging_page(), None);
                });
            }
        }
    }
}

fn title_selection_points(
    reader: &ReaderView,
    text: &pdf::PageText,
) -> (gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>) {
    let viewport = reader.page_viewport(0).expect("page viewport measured");
    let screen_point = |index: usize, fraction: f64| {
        let glyph = &text.glyphs[index];
        let screen = viewport.document_to_screen(Point {
            x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
            y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
        });
        point(px(screen.x as f32), px(screen.y as f32))
    };
    (screen_point(0, 0.1), screen_point(4, 0.8))
}

#[gpui_kit::test]
fn dragging_text_and_copying_uses_document_coordinates(cx: &mut TestAppContext) {
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
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    cx.run_until_parked();
    let text = pdf::page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
    let (start, end) = window
        .read_with(cx, |reader, _| title_selection_points(reader, &text))
        .unwrap();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| {
            frame.drag(start, end, cx);
        })
        .unwrap();
    });
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "cmd-c");
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("KVER ".into())
    );
    window
        .update(cx, |reader, _, cx| {
            reader.set_reading_mode(ReadingMode::SinglePage, cx)
        })
        .unwrap();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| {
            frame.render_frame(cx);
            frame.drag(start, end, cx);
        })
        .unwrap();
    });
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
        })
        .unwrap();
    window
        .update(cx, |reader, _, cx| reader.next_page(cx))
        .unwrap();
    window
        .read_with(cx, |reader, _| {
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
            assert_eq!(reader.selected_text(), None);
        })
        .unwrap();
}

#[gpui_kit::test]
fn selection_copy_survives_zoom_and_scrolling_in_its_window(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/readme-sample.pdf");
    let text = pdf::page_text(&path, 0).unwrap();
    let (window, reader) = recent_test_window(cx, Some(path.clone()));
    let (_, other) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    for zoomed in [false, true] {
        if zoomed {
            cx.simulate_keystrokes(window, "cmd-=");
            cx.run_until_parked();
            reader.read_with(cx, |reader, _| {
                assert!(reader.scale() > 1.0);
                assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
            });
        }
        let (start, end) = reader.read_with(cx, |reader, _| title_selection_points(reader, &text));
        cx.update_window(window, |_, window, cx| window.drag(end, start, cx))
            .unwrap();
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
            assert_eq!(reader.selection.dragging_page(), None);
        });
        other.read_with(cx, |reader, _| assert_eq!(reader.selected_text(), None));
    }
    cx.simulate_keystrokes(window, "right");
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
    });
    cx.simulate_keystrokes(window, "cmd-c");
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("KVER ".into())
    );
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(SinglePage), cx)
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(
            reader.selected_text(),
            None,
            "the selected page is no longer in the layout"
        );
    });
}

#[gpui_kit::test]
fn replacing_document_during_drag_clears_selection_and_late_pointer_input(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = PathBuf::from("examples/readme-sample.pdf");
    let text = pdf::page_text(&path, 0).unwrap();
    let (window, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let (start, end) = reader.read_with(cx, |reader, _| title_selection_points(reader, &text));
    cx.update_window(window, |_, window, cx| {
        window.pointer_down(start, gpui::MouseButton::Left, cx);
        window.pointer_move(end, Some(gpui::MouseButton::Left), cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selection.dragging_page(), Some(0));
        assert_eq!(reader.selected_text().as_deref(), Some("KVER "));
    });
    reader.update(cx, |reader, cx| {
        reader.open_path(path, cx);
        assert_eq!(reader.selected_text(), None);
        assert_eq!(reader.selection.dragging_page(), None);
    });
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.pointer_move(end, Some(gpui::MouseButton::Left), cx);
        window.pointer_up(end, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.page_text.get(0).is_some(), "replacement text loaded");
        assert_eq!(reader.selected_text(), None);
    });
}
