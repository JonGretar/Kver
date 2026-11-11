use super::*;

fn phrase_points(text: &PageText, phrase: &str) -> (Point, Point) {
    for range in &text.lines {
        let glyphs = &text.glyphs[range.clone()];
        let line: String = glyphs.iter().map(|glyph| glyph.text.as_str()).collect();
        if let Some(start) = line.find(phrase) {
            let end = start + phrase.len();
            let mut offset = 0;
            let selected: Vec<_> = glyphs
                .iter()
                .filter(|glyph| {
                    let from = offset;
                    offset += glyph.text.len();
                    from < end && offset > start
                })
                .collect();
            return (selected[0].start, selected.last().unwrap().end);
        }
    }
    panic!("missing phrase {phrase:?}");
}

#[test]
fn column_ranges_survive_cropped_rotated_projection_zoom_and_scroll() {
    for coefficients in [
        [1.0, 0.0, 0.0, -1.0, -20.0, 772.0],
        [0.0, 1.0, 1.0, 0.0, -20.0, -20.0],
        [-1.0, 0.0, 0.0, 1.0, 592.0, -20.0],
        [0.0, -1.0, -1.0, 0.0, 772.0, 592.0],
    ] {
        // Use real extracted column geometry with synthetic crop/page-rotation
        // projections. This is not a claim of arbitrary rotated-text support.
        let mut text =
            crate::pdf::page_text(std::path::Path::new("examples/field-guide.pdf"), 7).unwrap();
        let start = phrase_points(&text, "Route and reference points.").0;
        let end = phrase_points(&text, "Reading across columns.").1;
        text.transform = PageTransform::from_coefficients(coefficients);
        let transform = text.transform;
        let mut selection = Selection::default();
        load(&mut selection, 7, text);
        for zoom in [1.0, 1.5] {
            let viewport = crate::coordinates::PageViewport::new(
                transform,
                zoom,
                Point { x: 95.0, y: 120.0 },
                Point { x: 30.0, y: 215.0 },
            )
            .unwrap();
            let pointer = |at| {
                viewport
                    .screen_to_document(viewport.document_to_screen(at))
                    .unwrap()
            };
            selection.begin(7, Some(pointer(start)));
            assert!(selection.extend(pointer(end)));
            let copied = selection.selected_text().unwrap();
            assert!(
                copied.starts_with("Route and reference points."),
                "{copied}"
            );
            assert!(copied.contains("without color."), "{copied}");
            assert!(copied.ends_with("Reading across columns."), "{copied}");
            let rotated = coefficients[0] == 0.0;
            let size = if rotated {
                (752.0, 572.0)
            } else {
                (572.0, 752.0)
            };
            for highlight in selection.highlights(7, size) {
                assert!(highlight.left >= 0.0 && highlight.top >= 0.0);
                assert!(highlight.left + highlight.width <= size.0 as f64);
                assert!(highlight.top + highlight.height <= size.1 as f64);
                assert!(
                    if rotated {
                        highlight.height
                    } else {
                        highlight.width
                    } < 260.0
                );
            }
        }
    }
}

#[test]
fn reading_order_specimen_each_column_selects_independently_in_both_directions() {
    for (page, columns) in [
        (0, vec!["Left column", "Right column"]),
        (1, vec!["First column", "Second column", "Third column"]),
        (2, vec!["Article left", "Article right"]),
    ] {
        let text = crate::pdf::page_text(std::path::Path::new("examples/reading-order.pdf"), page)
            .unwrap();
        let points: Vec<_> = columns
            .iter()
            .map(|column| {
                (
                    phrase_points(&text, &format!("{column} begins.")).0,
                    phrase_points(&text, &format!("{column} ends.")).1,
                )
            })
            .collect();
        let mut selection = Selection::default();
        load(&mut selection, page, text);
        for (index, &(start, end)) in points.iter().enumerate() {
            for (start, end) in [(start, end), (end, start)] {
                selection.begin(page, Some(start));
                assert!(selection.extend(end));
                let copied = selection.selected_text().unwrap();
                assert!(
                    copied.starts_with(&format!("{} begins.", columns[index])),
                    "{copied}"
                );
                assert!(
                    copied.ends_with(&format!("{} ends.", columns[index])),
                    "{copied}"
                );
                for (other, column) in columns.iter().enumerate() {
                    if other != index {
                        assert!(!copied.contains(&format!("{column} begins.")), "{copied}");
                        assert!(!copied.contains(&format!("{column} ends.")), "{copied}");
                    }
                }
                assert!(
                    selection
                        .highlights(page, (595.276, 841.89))
                        .all(|highlight| highlight.width < 260.0)
                );
            }
        }
    }
}

#[test]
fn reading_order_specimen_cross_column_drags_follow_complete_columns() {
    for (page, markers) in [
        (
            0,
            vec![
                "Left column begins.",
                "Left column ends.",
                "Right column begins.",
                "Right column ends.",
            ],
        ),
        (
            1,
            vec![
                "First column begins.",
                "First column ends.",
                "Second column begins.",
                "Second column ends.",
                "Third column begins.",
                "Third column ends.",
            ],
        ),
        (
            2,
            vec![
                "Article left begins.",
                "Article left ends.",
                "Article right begins.",
                "Article right ends.",
            ],
        ),
    ] {
        let text = crate::pdf::page_text(std::path::Path::new("examples/reading-order.pdf"), page)
            .unwrap();
        let start = phrase_points(&text, markers[0]).0;
        // Deliberately stop at the TOP of the next column, not its bottom.
        let end = phrase_points(&text, markers[2]).1;
        let full_end = phrase_points(&text, markers.last().unwrap()).1;
        let mut selection = Selection::default();
        load(&mut selection, page, text);
        for (start, end) in [(start, end), (end, start)] {
            selection.begin(page, Some(start));
            assert!(selection.extend(end));
            let copied = selection.selected_text().unwrap();
            assert!(
                copied.starts_with(markers[0]),
                "page {}: {copied}",
                page + 1
            );
            assert!(copied.contains(markers[1]), "page {}: {copied}", page + 1);
            assert!(copied.ends_with(markers[2]), "page {}: {copied}", page + 1);
            assert!(!copied.contains(markers[3]), "page {}: {copied}", page + 1);
            assert!(
                selection
                    .highlights(page, (595.276, 841.89))
                    .all(|highlight| highlight.width < 260.0),
                "page {}: highlight bridges gutter",
                page + 1
            );
        }
        selection.begin(page, Some(start));
        selection.extend(full_end);
        let copied = selection.selected_text().unwrap();
        let mut offset = 0;
        for marker in markers {
            offset += copied[offset..]
                .find(marker)
                .unwrap_or_else(|| panic!("page {}: misplaced {marker:?}: {copied}", page + 1))
                + marker.len();
        }
    }
}

#[test]
fn field_guide_cross_column_drag_finishes_left_before_starting_right() {
    for (page, left, last_left, right) in [
        (
            7,
            "Route and reference points.",
            "without color.",
            "Reading across columns.",
        ),
        (
            9,
            "Texture in context.",
            "about composition.",
            "Comparing observations.",
        ),
    ] {
        let text =
            crate::pdf::page_text(std::path::Path::new("examples/field-guide.pdf"), page).unwrap();
        let start = phrase_points(&text, left).0;
        let end = phrase_points(&text, right).1;
        let mut selection = Selection::default();
        load(&mut selection, page, text);
        for (start, end) in [(start, end), (end, start)] {
            selection.begin(page, Some(start));
            selection.extend(end);
            let copied = selection.selected_text().unwrap();
            assert!(copied.starts_with(left), "{copied}");
            assert!(copied.contains(last_left), "{copied}");
            assert!(copied.ends_with(right), "{copied}");
        }
    }
}

#[test]
fn field_guide_route_same_column_drag_excludes_neighboring_column() {
    field_guide_same_column_drag(7, "Route and reference points.");
}

#[test]
fn field_guide_texture_same_column_drag_excludes_neighboring_column() {
    field_guide_same_column_drag(9, "Texture in context.");
}

fn field_guide_same_column_drag(page: usize, heading: &str) {
    let text =
        crate::pdf::page_text(std::path::Path::new("examples/field-guide.pdf"), page).unwrap();
    let first_line = text
        .lines
        .iter()
        .find(|range| {
            text.glyphs[(*range).clone()]
                .iter()
                .map(|glyph| glyph.text.as_str())
                .collect::<String>()
                .starts_with(heading)
        })
        .expect("field-guide column heading");
    let first = &text.glyphs[first_line.start];
    // Select the first two physical lines in the left column. Derive the
    // endpoints from actual extraction geometry, not assumed glyph indices.
    let next_y = text
        .glyphs
        .iter()
        .filter(|glyph| glyph.start.x < 295.0 && glyph.start.y < first.start.y - 3.0)
        .map(|glyph| glyph.start.y)
        .max_by(f64::total_cmp)
        .unwrap();
    let last = text
        .glyphs
        .iter()
        .filter(|glyph| glyph.start.x < 295.0 && (glyph.start.y - next_y).abs() < 1.0)
        .max_by(|a, b| a.end.x.total_cmp(&b.end.x))
        .unwrap();
    let start = first.start;
    let end = last.end;
    let mut selection = Selection::default();
    load(&mut selection, page, text);
    for (start, end) in [(start, end), (end, start)] {
        selection.begin(page, Some(start));
        assert!(selection.extend(end));
        selection.finish();
        let copied = selection.selected_text().unwrap();
        let highlights = selection
            .highlights(page, (612.0, 792.0))
            .collect::<Vec<_>>();
        assert!(
            highlights
                .iter()
                .all(|highlight| highlight.left + highlight.width <= 295.0),
            "page {}: same-column drag spills into neighbor: {highlights:?}; copied {copied:?}",
            page + 1
        );
        assert!(copied.starts_with(heading), "{copied:?}");
        assert!(!copied.contains("Reading across columns"), "{copied:?}");
        assert!(!copied.contains("Comparing observations"), "{copied:?}");
        assert_eq!(highlights.len(), 2);
    }
}

fn text(lines: &[(&str, f64)]) -> PageText {
    let mut text = PageText {
        transform: PageTransform::from_coefficients([1.0, 0.0, 0.0, -1.0, 0.0, 100.0]),
        glyphs: Vec::new(),
        lines: Vec::new(),
    };
    for &(line, y) in lines {
        let start = text.glyphs.len();
        for (ix, character) in line.chars().enumerate() {
            text.glyphs.push(TextGlyph {
                text: character.to_string(),
                start: Point {
                    x: ix as f64 * 10.0,
                    y,
                },
                end: Point {
                    x: (ix + 1) as f64 * 10.0,
                    y,
                },
                ink_right: None,
                top: y + 8.0,
                bottom: y - 2.0,
            });
        }
        text.lines.push(start..text.glyphs.len());
    }
    text
}

fn load(selection: &mut Selection, page: usize, text: PageText) {
    let request = selection.request_text(page..page + 1, 0).unwrap();
    assert_eq!(request.page(), page);
    assert!(selection.finish_text(request, Ok(text)));
}

#[test]
fn forward_and_reverse_drags_copy_unicode_lines_and_paragraph_gaps() {
    let mut selection = Selection::default();
    load(
        &mut selection,
        0,
        text(&[("Hé", 80.0), ("世 界", 65.0), ("end", 35.0)]),
    );
    for (start, end) in [
        (Point { x: 0.0, y: 80.0 }, Point { x: 30.0, y: 35.0 }),
        (Point { x: 30.0, y: 35.0 }, Point { x: 0.0, y: 80.0 }),
    ] {
        selection.begin(0, Some(start));
        assert!(selection.extend(end));
        selection.finish();
        assert_eq!(
            selection.selected_text().as_deref(),
            Some("Hé\n世 界\n\nend")
        );
        assert_eq!(selection.highlights(0, (100.0, 100.0)).count(), 3);
        assert!(selection.highlights(1, (100.0, 100.0)).next().is_none());
    }
}

#[test]
fn drag_lifecycle_distinguishes_caret_miss_release_and_clear() {
    let mut selection = Selection::default();
    load(&mut selection, 0, text(&[("abc", 80.0)]));
    selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
    assert_eq!(selection.dragging_page(), Some(0));
    assert_eq!(selection.selected_text(), None);
    assert!(selection.highlights(0, (100.0, 100.0)).next().is_none());
    assert!(selection.extend(Point { x: 20.0, y: 80.0 }));
    assert!(!selection.extend(Point { x: 200.0, y: 80.0 }));
    assert_eq!(selection.selected_text().as_deref(), Some("ab"));
    selection.finish();
    assert_eq!(selection.dragging_page(), None);
    assert!(!selection.extend(Point { x: 30.0, y: 80.0 }));
    assert_eq!(selection.selected_text().as_deref(), Some("ab"));
    selection.clear();
    assert_eq!(selection.page(), None);
    assert_eq!(selection.selected_text(), None);
    assert!(
        selection.page_transform(0).is_some(),
        "clear is not document replacement"
    );

    for (page, at) in [
        (0, None),
        (0, Some(Point { x: 200.0, y: 80.0 })),
        (1, Some(Point { x: 0.0, y: 80.0 })),
    ] {
        selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
        selection.extend(Point { x: 30.0, y: 80.0 });
        selection.begin(page, at);
        assert_eq!(selection.selected_text(), None);
        assert_eq!(selection.dragging_page(), None);
    }
}

#[test]
fn hit_testing_uses_nearest_boundary_with_existing_document_point_tolerance() {
    let mut selection = Selection::default();
    load(&mut selection, 0, text(&[("abc", 80.0)]));
    selection.begin(0, Some(Point { x: -8.0, y: 75.0 }));
    assert!(selection.extend(Point { x: 16.0, y: 91.0 }));
    assert_eq!(selection.selected_text().as_deref(), Some("ab"));
    selection.begin(0, Some(Point { x: -8.1, y: 80.0 }));
    assert_eq!(selection.page(), None);
    selection.begin(0, Some(Point { x: 0.0, y: 74.9 }));
    assert_eq!(selection.page(), None);
}

#[test]
fn highlights_transform_all_corners_and_clip_to_page_bounds() {
    let mut selection = Selection::default();
    let mut page = text(&[("abc", 80.0)]);
    // Crop/rotation projection; this tests rectangle projection, not support
    // for general rotated-text hit testing or reading order.
    page.transform = PageTransform::from_coefficients([0.0, 1.0, 1.0, 0.0, -80.0, -5.0]);
    load(&mut selection, 0, page);
    selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
    selection.extend(Point { x: 30.0, y: 80.0 });
    assert_eq!(
        selection.highlights(0, (6.0, 20.0)).collect::<Vec<_>>(),
        vec![Highlight {
            left: 0.0,
            top: 0.0,
            width: 6.0,
            height: 20.0
        }]
    );
    assert_eq!(selection.selected_text().as_deref(), Some("abc"));
}

#[test]
fn selected_page_survives_eviction_until_selection_is_cleared() {
    let mut selection = Selection::default();
    load(&mut selection, 0, text(&[("abc", 80.0)]));
    selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
    selection.extend(Point { x: 30.0, y: 80.0 });
    selection.finish();
    for page in 1..1000 {
        load(&mut selection, page, text(&[("next", 80.0)]));
        assert!(selection.page_transform(0).is_some());
        if page > 1 {
            assert!(selection.page_transform(page - 1).is_none());
        }
    }
    assert_eq!(selection.selected_text().as_deref(), Some("abc"));
    selection.clear();
    assert_eq!(selection.request_text(999..1000, 0), None);
    assert!(selection.page_transform(0).is_none());
    assert!(selection.page_transform(999).is_some());
}

#[test]
fn one_current_text_request_uses_latest_viewport_after_completion() {
    let mut selection = Selection::default();
    let first = selection.request_text(0..3, 0).unwrap();
    assert_eq!(first.page(), 0);
    assert_eq!(selection.request_text(500..502, 0), None);
    assert!(selection.finish_text(first, Ok(text(&[("old viewport", 80.0)]))));
    let next = selection.request_text(500..502, 0).unwrap();
    assert_eq!(next.page(), 500, "no queued work for pages 1 and 2");
    assert!(selection.page_transform(0).is_none());
    assert_eq!(selection.request_text(501..502, 0), None);
}

#[test]
fn replacement_rejects_old_success_and_failure_without_clearing_new_request() {
    for old_result in [Ok(text(&[("old", 80.0)])), Err("old failure".into())] {
        let mut selection = Selection::default();
        load(&mut selection, 0, text(&[("abc", 80.0)]));
        selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
        selection.extend(Point { x: 30.0, y: 80.0 });
        let old = selection.request_text(1..2, 0).unwrap();
        selection.reset();
        assert_eq!(selection.page(), None);
        assert_eq!(selection.dragging_page(), None);
        assert_eq!(selection.selected_text(), None);
        assert!(selection.page_transform(0).is_none());
        let current = selection.request_text(1..2, 1).unwrap();
        assert!(!selection.finish_text(old, old_result));
        assert!(selection.page_transform(1).is_none());
        assert_eq!(
            selection.request_text(1..2, 1),
            None,
            "new request still owns the slot"
        );
        assert!(selection.finish_text(current, Ok(text(&[("new", 80.0)]))));
        selection.begin(1, Some(Point { x: 0.0, y: 80.0 }));
        selection.extend(Point { x: 30.0, y: 80.0 });
        assert_eq!(selection.selected_text().as_deref(), Some("new"));
    }
}

#[test]
fn empty_and_failed_pages_do_not_spin_but_replacement_allows_retry() {
    let mut selection = Selection::default();
    let empty = selection.request_text(0..2, 0).unwrap();
    assert!(selection.finish_text(empty, Ok(text(&[]))));
    selection.begin(0, Some(Point { x: 0.0, y: 80.0 }));
    assert_eq!(selection.page(), None);
    let failed = selection.request_text(0..2, 0).unwrap();
    assert_eq!(failed.page(), 1);
    assert!(selection.finish_text(failed, Err("no text".into())));
    assert_eq!(selection.request_text(0..2, 0), None);
    selection.reset();
    assert_eq!(selection.request_text(1..2, 1).unwrap().page(), 1);
}
