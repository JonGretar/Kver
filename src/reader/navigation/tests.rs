use super::*;

#[test]
fn long_document_only_lays_out_visible_pages() {
    let pages = vec![(612.0, 792.0); 1000];
    let layout = PageLayout::new(&pages, 1.0, ReadingMode::Continuous, 0);
    assert_eq!(layout.visible_pages(0.0, 700.0), 0..1);
    assert_eq!(layout.visible_pages(layout.page_top(500), 700.0), 500..501);
    assert_eq!(layout.visible_pages(layout.page_top(999), 700.0), 999..1000);
}

#[test]
fn current_page_ignores_a_sliver_but_counts_a_short_final_page() {
    let pages = [(600.0, 1200.0), (300.0, 400.0), (600.0, 180.0)];
    let zoom = 1.0;
    let layout = PageLayout::new(&pages, zoom, ReadingMode::Continuous, 0);
    // Ten points of the previous page are visible above page two.
    assert_eq!(layout.current_page_at(layout.page_top(1) - 10.0, 450.0), 1);
    // The last page is shorter than half a viewport, but at the document
    // end it is still the page the reader has reached.
    let height = 700.0;
    assert_eq!(
        layout.current_page_at(layout.page_top(pages.len()) - height, height),
        2
    );
}

#[test]
fn page_local_position_survives_zoom() {
    let pages = vec![(100.0, 200.0), (150.0, 300.0)];
    let layout = PageLayout::new(&pages, 1.0, ReadingMode::Continuous, 0);
    let position = layout.position_at(216.0 + 75.0);
    assert_eq!(position, Position { page: 1, y: 75.0 });
    let zoomed = PageLayout::new(&pages, 2.0, ReadingMode::Continuous, 0);
    assert_eq!(zoomed.position_at(zoomed.page_top(1) + 150.0), position);
}

#[test]
fn fit_uses_current_page_and_inner_viewport() {
    let viewport = (632.0, 832.0);
    let layout = PageLayout::new(
        &[(600.0, 1200.0), (300.0, 400.0)],
        1.0,
        ReadingMode::Continuous,
        0,
    );
    assert_eq!(layout.fit_scale(ZoomMode::FitWidth, 0, viewport), Some(1.0));
    assert_eq!(
        layout.fit_scale(ZoomMode::FitPage, 0, viewport),
        Some(2.0 / 3.0)
    );
    assert_eq!(layout.fit_scale(ZoomMode::FitWidth, 1, viewport), Some(2.0));
    assert_eq!(layout.fit_scale(ZoomMode::FitPage, 1, viewport), Some(2.0));
    assert_eq!(layout.fit_scale(ZoomMode::Fixed, 1, viewport), None);
    assert_eq!(layout.fit_scale(ZoomMode::FitWidth, 1, (0.0, 800.0)), None);
    assert_eq!(layout.fit_scale(ZoomMode::FitPage, 1, (632.0, 20.0)), None);
    assert_eq!(
        layout.fit_scale(ZoomMode::FitWidth, 0, (332.0, 832.0)),
        Some(0.5)
    );
    assert_eq!(
        layout.fit_scale(ZoomMode::FitPage, 0, (632.0, 632.0)),
        Some(0.5)
    );
}

#[test]
fn empty_layout_has_no_pages_scroll_or_fit_target_in_either_mode() {
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        let layout = PageLayout::new(&[], 1.0, mode, 0);
        assert_eq!(layout.visible_pages(0.0, 700.0), 0..0);
        assert_eq!(layout.spacers(0..0), (0.0, 0.0));
        assert_eq!(layout.clamp_scroll(-10.0, 500.0, 700.0), (0.0, 0.0));
        assert_eq!(layout.position_at(500.0), Position { page: 0, y: 0.0 });
        assert_eq!(
            layout.position_to_save(0.0, 700.0, None),
            Position { page: 0, y: 0.0 }
        );
        assert_eq!(layout.current_page_at(0.0, 700.0), 0);
        assert_eq!(layout.fit_scale(ZoomMode::FitPage, 0, (900.0, 700.0)), None);
        assert_eq!(layout.refit(0.0, 700.0, Some(0)).selected_page, None);
    }
}

#[test]
fn single_page_layout_bounds_scroll_to_only_the_selected_page() {
    let pages = [(360.0, 300.0), (560.0, 1500.0), (940.0, 500.0)];
    for (page, expected_maximum) in [(0, 0.0), (1, 2348.0), (2, 348.0)] {
        let layout = PageLayout::new(&pages, 2.0, ReadingMode::SinglePage, page);
        assert_eq!(layout.visible_pages(10000.0, 700.0), page..page + 1);
        assert_eq!(layout.spacers(page..page + 1), (0.0, 0.0));
        assert_eq!(layout.page_top(page), 0.0);
        assert_eq!(layout.current_page_at(10000.0, 700.0), page);
        assert_eq!(layout.clamp_scroll(-5.0, -5.0, 700.0), (0.0, 0.0));
        assert_eq!(
            layout.clamp_scroll(50.0, 10000.0, 700.0),
            (50.0, expected_maximum)
        );
        assert_eq!(layout.position_at(200.0), Position { page, y: 100.0 });
        assert_eq!(
            layout.position_at(10000.0),
            Position {
                page,
                y: pages[page].1
            }
        );
    }
}

#[test]
fn virtualized_spacers_preserve_mixed_page_document_extent() {
    let pages = [(384.0, 504.0), (378.0, 288.0), (352.0, 294.0)];
    for zoom in [0.25, 1.0, 1.25, 4.0] {
        let layout = PageLayout::new(&pages, zoom, ReadingMode::Continuous, 0);
        let total: f32 = pages.iter().map(|(_, height)| height * zoom + GAP).sum();
        for page in 0..pages.len() {
            let range = layout.visible_pages(layout.page_top(page), 20.0);
            let (before, after) = layout.spacers(range.clone());
            let materialized: f32 = pages[range]
                .iter()
                .map(|(_, height)| height * zoom + GAP)
                .sum();
            assert_eq!(before + materialized + after, total);
        }
        assert_eq!(
            layout.clamp_scroll(0.0, total, 200.0).1,
            (total - 200.0).max(0.0)
        );
    }
}

#[test]
fn gaps_belong_to_the_preceding_page_and_positions_clamp_to_page_height() {
    let pages = [(100.0, 200.0), (150.0, 300.0)];
    let layout = PageLayout::new(&pages, 2.0, ReadingMode::Continuous, 0);
    assert_eq!(layout.position_at(-100.0), Position { page: 0, y: 0.0 });
    assert_eq!(layout.position_at(408.0), Position { page: 0, y: 200.0 });
    assert_eq!(layout.position_at(416.0), Position { page: 1, y: 0.0 });
    assert_eq!(layout.position_at(10000.0), Position { page: 1, y: 300.0 });
    assert_eq!(
        layout.visible_pages(405.0, 5.0),
        2..2,
        "a viewport wholly inside a gap has no page"
    );
}

#[test]
fn refitting_uses_center_for_new_scroll_target_and_top_for_explicit_turn() {
    let pages = [(600.0, 1000.0), (300.0, 800.0), (600.0, 180.0)];
    let layout = PageLayout::new(&pages, 1.0, ReadingMode::Continuous, 0);
    let pinned = layout.refit(900.0, 400.0, Some(0));
    assert_eq!(pinned.page, 0);
    assert_eq!(pinned.selected_page, Some(0));
    assert_eq!(pinned.anchor, 0.0);
    assert_eq!(pinned.position, Position { page: 0, y: 900.0 });

    for selected in [None, Some(2)] {
        let scrolled = layout.refit(900.0, 400.0, selected);
        assert_eq!(scrolled.page, 1);
        assert_eq!(
            scrolled.selected_page, None,
            "discard a pin outside the viewport"
        );
        assert_eq!(scrolled.anchor, 200.0);
        assert_eq!(scrolled.position, Position { page: 1, y: 84.0 });
        let zoomed = PageLayout::new(&pages, 1.5, ReadingMode::Continuous, scrolled.page);
        let top = zoomed.offset_for(scrolled.position, scrolled.anchor);
        assert_eq!(zoomed.position_at(top + 200.0), scrolled.position);
    }
    let turned = layout.refit(900.0, 400.0, Some(1));
    assert_eq!(turned.page, 1);
    assert_eq!(turned.anchor, 0.0);
    let unchanged =
        PageLayout::new(&pages, 1.0, ReadingMode::Continuous, 1).refit(900.0, 400.0, None);
    assert_eq!(unchanged.anchor, 0.0);
}

#[test]
fn saved_place_preserves_explicit_turns_and_short_final_page() {
    let pages = [(600.0, 1000.0), (300.0, 800.0), (600.0, 180.0)];
    let layout = PageLayout::new(&pages, 1.0, ReadingMode::Continuous, 2);
    let (_, top) = layout.clamp_scroll(0.0, 10000.0, 700.0);
    assert_eq!(layout.position_at(top), Position { page: 1, y: 312.0 });
    assert_eq!(
        layout.position_to_save(top, 700.0, Some(2)),
        Position { page: 2, y: 0.0 }
    );
    assert_eq!(
        layout.position_to_save(top, 700.0, None),
        Position { page: 2, y: 0.0 }
    );
    assert_eq!(
        layout.position_to_save(top, 700.0, Some(1)),
        Position { page: 1, y: 312.0 }
    );
    let single = PageLayout::new(&pages, 2.0, ReadingMode::SinglePage, 1);
    assert_eq!(
        single.position_to_save(200.0, 700.0, Some(1)),
        Position { page: 1, y: 100.0 }
    );
}

#[test]
fn page_viewport_shares_centering_insets_zoom_and_scroll_with_layout() {
    let pages = [(100.0, 200.0), (200.0, 400.0)];
    let origin = Point { x: 20.0, y: 30.0 };
    let scroll = Point { x: 5.0, y: 40.0 };
    for (mode, page, expected) in [
        (ReadingMode::Continuous, 0, Point { x: 65.0, y: 6.0 }),
        (ReadingMode::Continuous, 1, Point { x: 31.0, y: 422.0 }),
        (ReadingMode::SinglePage, 1, Point { x: 31.0, y: 6.0 }),
    ] {
        let layout = PageLayout::new(&pages, 2.0, mode, page);
        let transform =
            PageTransform::from_coefficients([1.0, 0.0, 0.0, -1.0, 0.0, pages[page].1 as f64]);
        let viewport = layout
            .page_viewport(page, transform, origin, 300.0, scroll)
            .unwrap();
        let top_left = Point {
            x: 0.0,
            y: pages[page].1 as f64,
        };
        assert_eq!(viewport.document_to_screen(top_left), expected);
        assert_eq!(viewport.screen_to_document(expected), Some(top_left));
        assert!(
            layout
                .page_viewport(2, transform, origin, 300.0, scroll)
                .is_none()
        );
    }
}

#[test]
fn fit_scales_keep_existing_limits_in_both_reading_modes() {
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        let layout = PageLayout::new(&[(10.0, 10.0), (10000.0, 10000.0)], 1.0, mode, 0);
        for zoom_mode in [ZoomMode::FitWidth, ZoomMode::FitPage] {
            assert_eq!(layout.fit_scale(zoom_mode, 0, (1000.0, 1000.0)), Some(4.0));
            assert_eq!(layout.fit_scale(zoom_mode, 1, (100.0, 100.0)), Some(0.25));
            assert_eq!(layout.fit_scale(zoom_mode, 0, (32.0, 100.0)), None);
            assert_eq!(layout.fit_scale(zoom_mode, 2, (100.0, 100.0)), None);
        }
    }
}
