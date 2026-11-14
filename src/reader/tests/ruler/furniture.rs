use super::*;

#[gpui_kit::test]
fn reading_order_footer_is_skipped_by_keyboard_across_pages(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 0);
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    let assert_not_footer = |cx: &mut TestAppContext| {
        let value = line_text(&reader, cx);
        assert!(
            !value.contains("KVER / MARGINS") && value.trim().parse::<u32>().is_err(),
            "keyboard traversal reached footer: {value:?}"
        );
    };
    for _ in 0..200 {
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_not_footer(cx);
        if target(&reader, cx).0 == 1 {
            break;
        }
    }
    assert_eq!(target(&reader, cx).0, 1);
    cx.simulate_keystrokes(window, "k");
    settle(cx, window);
    assert_not_footer(cx);
    assert_eq!(target(&reader, cx).0, 0);
}

fn line_text(reader: &Entity<ReaderView>, cx: &mut TestAppContext) -> String {
    reader.read_with(cx, |reader, _| {
        let target = reader.ruler.target.as_ref().unwrap();
        target.text.glyphs[target.text.lines[target.line].clone()]
            .iter()
            .map(|glyph| glyph.text.as_str())
            .collect()
    })
}

fn click_line(
    window: AnyWindowHandle,
    reader: &Entity<ReaderView>,
    cx: &mut TestAppContext,
    contains: &str,
) {
    let at = reader.read_with(cx, |reader, _| {
        let text = reader.page_text.get(reader.page_index).unwrap();
        let line = text
            .lines
            .iter()
            .position(|range| {
                text.glyphs[range.clone()]
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<String>()
                    .contains(contains)
            })
            .unwrap();
        let [left, bottom, right, top] = crate::reader::ruler::document_rect(text, line).unwrap();
        let at = reader
            .page_viewport(reader.page_index)
            .unwrap()
            .document_to_screen(Point {
                x: (left + right) / 2.,
                y: (bottom + top) / 2.,
            });
        point(px(at.x as f32), px(at.y as f32))
    });
    cx.update_window(window, |_, frame, cx| {
        let page = frame.find("pdf-page-0").bounds();
        frame.click_with_options(
            "pdf-page-0",
            ClickOptions::new()
                .with_offset(at - page.origin)
                .with_modifiers(Modifiers::alt()),
            cx,
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn furniture_window_preserves_explicit_placement_and_cross_page_continuation(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/ruler-furniture.pdf", 0);
    reader.read_with(cx, |reader, _| {
        for page in 1..15 {
            assert!(
                reader.page_text.get(page).is_none(),
                "eager extraction of page {page}"
            );
        }
    });
    click_line(window, &reader, cx, "Coastal observations");
    assert!(line_text(&reader, cx).contains("Coastal observations"));
    cx.simulate_keystrokes(window, "j");
    settle(cx, window);
    assert!(line_text(&reader, cx).contains("Chapter 1"));
    // Header is omitted in reverse movement, stopping at the document boundary.
    cx.simulate_keystrokes(window, "k");
    settle(cx, window);
    assert!(line_text(&reader, cx).contains("Chapter 1"));
    click_line(window, &reader, cx, "Page 1");
    assert!(line_text(&reader, cx).contains("Page 1"));
    cx.simulate_keystrokes(window, "k");
    settle(cx, window);
    assert!(line_text(&reader, cx).contains("next footnote"));
    cx.simulate_keystrokes(window, "j");
    settle(cx, window);
    assert!(line_text(&reader, cx).contains("Chapter 2"));
    assert_eq!(target(&reader, cx).0, 1);
    cx.simulate_keystrokes(window, "k");
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 0);
    assert!(line_text(&reader, cx).contains("next footnote"));
    reader.read_with(cx, |reader, _| {
        assert!(reader.selection.dragging_page().is_none());
        assert!(reader.selected_text().is_none());
        assert!(reader.ruler_visible());
        for page in 5..15 {
            assert!(
                reader.page_text.get(page).is_none(),
                "unbounded look-ahead to page {page}"
            );
        }
    });
    // Source text still contains the running header and page counter.
    let text = pdf::page_text(Path::new("examples/ruler-furniture.pdf"), 0).unwrap();
    let all = text
        .glyphs
        .iter()
        .map(|g| g.text.as_str())
        .collect::<String>();
    assert!(all.contains("Coastal observations") && all.contains("Page 1"));
    reader.update(cx, |reader, cx| {
        reader.select_page(0, cx);
        reader.fit_page(cx);
    });
    settle(cx, window);
    let (start, end) = reader.read_with(cx, |reader, _| {
        let line = text
            .lines
            .iter()
            .position(|range| {
                text.glyphs[range.clone()]
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<String>()
                    .contains("Coastal observations")
            })
            .unwrap();
        let [left, bottom, right, top] = crate::reader::ruler::document_rect(&text, line).unwrap();
        let viewport = reader.page_viewport(0).unwrap();
        let screen = |x| {
            let at = viewport.document_to_screen(Point {
                x,
                y: (bottom + top) / 2.,
            });
            point(px(at.x as f32), px(at.y as f32))
        };
        (screen(left - 1.), screen(right + 1.))
    });
    cx.update_window(window, |_, frame, cx| {
        let page = frame.find("pdf-page-0").bounds();
        assert!(
            page.contains(&start) && page.contains(&end),
            "{page:?} {start:?} {end:?}"
        );
        frame.drag(start, end, cx);
        frame.press("cmd-c", cx);
    })
    .unwrap();
    assert_eq!(
        reader
            .read_with(cx, |reader, _| reader.selected_text())
            .unwrap()
            .trim(),
        "Coastal observations"
    );
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .unwrap()
            .trim(),
        "Coastal observations"
    );
    cx.simulate_keystrokes(window, "escape");
    cx.simulate_keystrokes(window, "/");
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        assert!(
            frame.try_find("find-query").is_some(),
            "Find should own typing"
        );
        frame.click("find-query", cx);
        frame.input("Coastal observations", cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, _| {
        assert!(
            frame
                .find(("find-result", 0usize))
                .label()
                .unwrap()
                .contains("Coastal observations")
        );
    })
    .unwrap();
}
