//! Search lifecycle tests need only Find, not a reader or a sidebar.

use super::*;
use gpui_kit::{TestAppContext, test::TestWindowExt};

fn sample_hit() -> FindMatch {
    let text = pdf::page_text(std::path::Path::new("examples/readme-sample.pdf"), 0).unwrap();
    find_search::matches_on_page(&text, 0, "This is page 1 of 100").remove(0)
}

#[gpui_kit::test]
fn obsolete_batches_and_navigation_cannot_cross_query_or_document_revisions(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(FindPanel::new);
    window
        .update(cx, |find, _, cx| {
            find.set_document(0, Some("examples/readme-sample.pdf".into()), 100, cx);
            find.query_changed("This is page 1 of 100".into(), cx);
            let first_revision = find.revision;
            let old_navigation = FindNavigation {
                document_generation: 0,
                revision: first_revision,
                hit: sample_hit(),
            };
            find.query_changed("This is page 2 of 100".into(), cx);
            assert!(!find.is_current(&old_navigation));
            assert!(!find.apply_batch(
                0,
                first_revision,
                vec![PageMatches::Mapped(vec![sample_hit()])],
                cx,
            ));
            assert_eq!(find.scanned, 0);
            assert!(!find.has_matches());
            assert!(find.highlight().is_none());

            let current_revision = find.revision;
            let navigation = FindNavigation {
                document_generation: 0,
                revision: current_revision,
                hit: sample_hit(),
            };
            assert!(find.is_current(&navigation));
            find.set_document(1, None, 0, cx);
            assert!(!find.is_current(&navigation));
            assert!(find.task.is_none());
            assert!(!find.searching);
            assert!(!find.apply_batch(
                0,
                current_revision,
                vec![PageMatches::Mapped(vec![sample_hit()])],
                cx,
            ));
            assert_eq!(find.scanned, 0);
            assert!(!find.has_matches());
        })
        .unwrap();
}

#[gpui_kit::test]
fn results_are_bounded_and_pending_enter_activates_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(FindPanel::new);
    window
        .update(cx, |find, _, cx| {
            find.set_document(0, Some("examples/readme-sample.pdf".into()), 100, cx);
            find.query_changed("This is page".into(), cx);
            find.step_match(1, cx);
            assert!(find.pending_next);
            assert!(!find.apply_batch(
                0,
                find.revision,
                vec![PageMatches::Mapped(vec![sample_hit(); MAX_MATCHES + 1])],
                cx,
            ));
            assert_eq!(find.results.len(), MAX_MATCHES);
            assert!(find.truncated);
            assert!(!find.searching);
            assert!(!find.pending_next);
            assert_eq!(find.active, Some(0));
            assert_eq!(find.highlight().unwrap().1, 1);
            find.step_match(-1, cx);
            assert_eq!(find.active, Some(MAX_MATCHES - 1));
            find.step_match(1, cx);
            assert_eq!(find.active, Some(0));
        })
        .unwrap();
}

#[gpui_kit::test]
fn unmapped_and_failed_pages_keep_distinct_progress(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(FindPanel::new);
    window
        .update(cx, |find, _, cx| {
            find.set_document(0, Some("examples/readme-sample.pdf".into()), 3, cx);
            find.query_changed("absent phrase".into(), cx);
            assert!(!find.apply_batch(
                0,
                find.revision,
                vec![
                    PageMatches::Unmapped,
                    PageMatches::Failed,
                    PageMatches::Mapped(Vec::new()),
                ],
                cx,
            ));
            assert_eq!(find.scanned, 3);
            assert_eq!(find.mapped_pages, 1);
            assert_eq!(find.failed_pages, 1);
            assert!(!find.has_matches());
            assert!(!find.truncated);
            assert!(!find.searching);
        })
        .unwrap();
}

#[gpui_kit::test]
fn closing_find_drops_its_pending_search_owner(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(FindPanel::new);
    let weak = window
        .update(cx, |find, _, cx| {
            find.set_document(0, Some("examples/readme-sample.pdf".into()), 100, cx);
            find.query_changed("This is page".into(), cx);
            assert!(find.task.is_some());
            cx.weak_entity()
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}

#[gpui_kit::test]
fn document_replacement_clears_query_when_hidden_find_is_focused_again(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(FindPanel::new);
    window
        .update(cx, |find, frame, cx| {
            find.set_document(0, Some("examples/readme-sample.pdf".into()), 100, cx);
            find.focus(frame, cx);
        })
        .unwrap();
    cx.update_window(window.into(), |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("old query", cx);
    })
    .unwrap();
    // No render occurs between replacement and focus, as with a hidden panel.
    window
        .update(cx, |find, frame, cx| {
            find.set_document(1, None, 0, cx);
            find.set_document(1, Some("examples/field-guide.pdf".into()), 5, cx);
            find.focus(frame, cx);
            assert_eq!(find.query(cx), "");
            assert!(!find.pending_query_reset);
            assert!(!find.has_matches());
        })
        .unwrap();
}
