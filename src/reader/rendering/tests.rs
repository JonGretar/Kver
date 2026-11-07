use super::*;
use crate::reader::ReadingMode;

fn bitmap(width: u32, height: u32) -> RenderedBitmap {
    RenderedBitmap {
        bgra: vec![0; width as usize * height as usize * 4],
        page_count: 1,
        width,
        height,
        page_width: width as f32,
        page_height: height as f32,
    }
}

fn request(
    rasters: &mut PageRasters,
    layout: &PageLayout<'_>,
    top: f32,
    height: f32,
    density: f32,
) -> RasterRequest {
    match rasters.schedule(layout, top, height, density) {
        Schedule::Render(request) => request,
        Schedule::Ready => panic!("expected a raster request, got Ready"),
        Schedule::Unchanged => panic!("expected a raster request, got Unchanged"),
    }
}

#[test]
fn display_density_changes_bitmap_resolution_not_logical_zoom() {
    assert_eq!(raster_scale(1.0, 2.0, (612.0, 792.0)), 2.0);
    assert_eq!(raster_scale(1.0, 1.0, (612.0, 792.0)), 1.0);
}

#[test]
fn high_zoom_is_bounded_for_a4_scans() {
    let scale = raster_scale(4.0, 2.0, (595.44, 842.04));
    assert!(scale < 8.0);
    assert!(scale > 7.0);
    assert!(595.44 * 842.04 * scale * scale <= 32_000_000.0);
}

#[test]
fn nearest_visible_page_runs_first_and_next_job_uses_latest_viewport() {
    let layout = PageLayout::new(&[(100.0, 100.0); 4], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let first = request(&mut rasters, &layout, 90.0, 140.0, 1.0);
    assert_eq!(first.page(), 1);
    assert!(matches!(
        rasters.schedule(&layout, 348.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
    assert!(matches!(
        rasters.finish(first, Ok(bitmap(100, 100)), 3..4),
        Completion::Accepted
    ));
    assert!(
        rasters.image(1).is_none(),
        "offscreen completion must not retain pixels"
    );
    assert_eq!(rasters.retained_bytes(), 0);
    assert_eq!(request(&mut rasters, &layout, 348.0, 100.0, 1.0).page(), 3);
}

#[test]
fn scrolling_evicts_old_bitmaps_even_while_a_job_is_current() {
    let layout = PageLayout::new(&[(100.0, 100.0); 3], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let first = request(&mut rasters, &layout, 0.0, 220.0, 1.0);
    assert!(matches!(
        rasters.finish(first, Ok(bitmap(100, 100)), 0..2),
        Completion::Accepted
    ));
    assert_eq!(rasters.retained_bytes(), 40_000);
    let second = request(&mut rasters, &layout, 0.0, 220.0, 1.0);
    assert_eq!(second.page(), 1);
    assert!(matches!(
        rasters.schedule(&layout, 232.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
    assert!(rasters.image(0).is_none());
    assert_eq!(rasters.retained_bytes(), 0);
    assert!(matches!(
        rasters.finish(second, Err("offscreen failure".into()), 2..3),
        Completion::Accepted
    ));
    assert_eq!(request(&mut rasters, &layout, 232.0, 100.0, 1.0).page(), 2);
}

#[test]
fn invalidation_rejects_old_pixels_and_errors_without_retiring_new_job() {
    for (zoom, density) in [(1.0, 1.0), (2.0, 1.0), (1.0, 2.0)] {
        for old_result in [Ok(bitmap(2, 2)), Err("old failure".into())] {
            let layout = PageLayout::new(&[(2.0, 2.0)], 1.0, ReadingMode::Continuous, 0);
            let mut rasters = PageRasters::default();
            let old = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
            rasters.invalidate();
            let changed = PageLayout::new(&[(2.0, 2.0)], zoom, ReadingMode::Continuous, 0);
            let current = request(&mut rasters, &changed, 0.0, 100.0, density);
            assert_ne!(old, current);
            assert_eq!(current.scale(), zoom * density);
            assert!(matches!(
                rasters.finish(old, old_result, 0..1),
                Completion::Obsolete
            ));
            assert!(rasters.image(0).is_none());
            assert!(matches!(
                rasters.schedule(&changed, 0.0, 100.0, density),
                Schedule::Unchanged
            ));
            let size = (2.0 * current.scale()) as u32;
            assert!(matches!(
                rasters.finish(current, Ok(bitmap(size, size)), 0..1),
                Completion::Accepted
            ));
            assert_eq!(rasters.retained_bytes(), (size * size * 4) as usize);
            rasters.invalidate();
            assert!(rasters.image(0).is_none());
            assert_eq!(rasters.retained_bytes(), 0);
        }
    }
}

#[test]
fn completed_ticket_cannot_finish_a_later_request_for_the_same_page() {
    let layout = PageLayout::new(&[(100.0, 100.0); 2], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let old = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    assert!(matches!(
        rasters.finish(old, Ok(bitmap(100, 100)), 0..1),
        Completion::Accepted
    ));
    let other = request(&mut rasters, &layout, 116.0, 100.0, 1.0);
    assert!(matches!(
        rasters.finish(other, Ok(bitmap(100, 100)), 1..2),
        Completion::Accepted
    ));
    let current = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    assert_eq!(current.page(), old.page());
    assert_eq!(current.scale(), old.scale());
    assert_ne!(current, old);
    assert!(matches!(
        rasters.finish(old, Err("duplicate old reply".into()), 0..1),
        Completion::Obsolete
    ));
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
    assert!(matches!(
        rasters.finish(current, Ok(bitmap(100, 100)), 0..1),
        Completion::Accepted
    ));
}

#[test]
fn visible_failures_do_not_spin_and_explicit_turn_keeps_retry_policy() {
    let layout = PageLayout::new(&[(100.0, 100.0); 2], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let failed = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    assert!(
        matches!(rasters.finish(failed, Err("cannot render".into()), 0..1),
        Completion::Failed(error) if error == "cannot render")
    );
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
    rasters.on_page_selected(0);
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
    // Selecting a neighbor permits another try even if the failed page stays visible.
    rasters.on_page_selected(1);
    assert_eq!(request(&mut rasters, &layout, 0.0, 220.0, 1.0).page(), 0);
}

#[test]
fn leaving_viewport_or_changing_render_inputs_allows_failed_page_retry() {
    let layout = PageLayout::new(&[(100.0, 100.0); 2], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let first = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    assert!(matches!(
        rasters.finish(first, Err("failure".into()), 0..1),
        Completion::Failed(_)
    ));
    let other = request(&mut rasters, &layout, 116.0, 100.0, 1.0);
    assert!(matches!(
        rasters.finish(other, Ok(bitmap(100, 100)), 1..2),
        Completion::Accepted
    ));
    let retry = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    assert_eq!(retry.page(), 0);
    assert!(matches!(
        rasters.finish(retry, Err("failure".into()), 0..1),
        Completion::Failed(_)
    ));
    rasters.invalidate();
    let new_density = request(&mut rasters, &layout, 0.0, 100.0, 2.0);
    assert_eq!(new_density.page(), 0);
    assert_eq!(new_density.scale(), 2.0);
}

#[test]
fn malformed_pixels_report_error_without_installing_an_image() {
    let layout = PageLayout::new(&[(100.0, 100.0)], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    let current = request(&mut rasters, &layout, 0.0, 100.0, 1.0);
    let mut invalid = bitmap(1, 1);
    invalid.bgra.clear();
    assert!(matches!(rasters.finish(current, Ok(invalid), 0..1),
        Completion::Failed(error) if error == "Cannot display page image: invalid pixel dimensions"));
    assert!(rasters.image(0).is_none());
    assert_eq!(rasters.retained_bytes(), 0);
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 100.0, 1.0),
        Schedule::Unchanged
    ));
}

#[test]
fn budget_defers_another_bitmap_until_visible_cache_has_room() {
    let layout = PageLayout::new(
        &[(2048.0, 1024.0), (8192.0, 8192.0)],
        1.0,
        ReadingMode::Continuous,
        0,
    );
    let mut rasters = PageRasters::default();
    let first = request(&mut rasters, &layout, 0.0, 10000.0, 1.0);
    assert!(matches!(
        rasters.finish(first, Ok(bitmap(2048, 1024)), 0..2),
        Completion::Accepted
    ));
    assert_eq!(rasters.retained_bytes(), 8 * 1024 * 1024);
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 10000.0, 1.0),
        Schedule::Ready
    ));
    assert!(rasters.image(0).is_some());
    assert!(rasters.image(1).is_none());
    assert!(rasters.retained_bytes() <= BITMAP_BUDGET);
    let second = request(&mut rasters, &layout, 1040.0, 9000.0, 1.0);
    assert_eq!(second.page(), 1);
    assert_eq!(rasters.retained_bytes(), 0);
}

#[test]
fn sustained_single_page_turns_keep_only_current_page_pixels() {
    let pages = vec![(1.0, 1.0); 1000];
    let mut rasters = PageRasters::default();
    for page in 0..pages.len() {
        let layout = PageLayout::new(&pages, 1.0, ReadingMode::SinglePage, page);
        let current = request(&mut rasters, &layout, 0.0, 700.0, 1.0);
        assert_eq!(current.page(), page);
        assert!(matches!(
            rasters.finish(current, Ok(bitmap(1, 1)), page..page + 1),
            Completion::Accepted
        ));
        assert_eq!(rasters.retained_bytes(), 4);
        assert!(rasters.image(page).is_some());
        if page > 0 {
            assert!(rasters.image(page - 1).is_none());
        }
        assert!(matches!(
            rasters.schedule(&layout, 0.0, 700.0, 1.0),
            Schedule::Ready
        ));
    }
}

#[test]
fn empty_layout_schedules_no_work() {
    let layout = PageLayout::new(&[], 1.0, ReadingMode::Continuous, 0);
    let mut rasters = PageRasters::default();
    assert!(matches!(
        rasters.schedule(&layout, 0.0, 700.0, 1.0),
        Schedule::Ready
    ));
    assert_eq!(rasters.retained_bytes(), 0);
}
