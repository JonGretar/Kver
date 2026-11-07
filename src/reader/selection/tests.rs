use super::*;

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
