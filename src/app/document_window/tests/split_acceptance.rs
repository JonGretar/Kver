//! One combined journey across shared tools, per-pane state and document edits.

use super::*;
use gpui_kit::{MouseButton, point, px, size, test::TestEventExt};

fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
}

fn action(cx: &mut TestAppContext, handle: AnyWindowHandle, action: impl Action) {
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(action), cx)
    })
    .unwrap();
    settle(cx, handle);
}

fn focus(cx: &mut TestAppContext, handle: AnyWindowHandle, reader: &Entity<ReaderView>) {
    cx.update_window(handle, |_, window, cx| {
        window.click_at(
            ("reading-pane", reader.entity_id()),
            point(px(5.), px(5.)),
            cx,
        );
    })
    .unwrap();
    settle(cx, handle);
}

#[gpui_kit::test]
fn save_as_preserves_single_and_split_window_navigation_trails(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let saved = root.path().join("renamed.pdf");
    std::fs::copy("examples/readme-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    let (other_handle, other) = open(cx, Some(source));
    cx.simulate_window_resize(handle, size(px(1200.), px(900.)));
    settle(cx, handle);
    let first = document.read_with(cx, |d, _| d.reader.clone());
    first.update(cx, |r, cx| {
        r.navigate_outline(crate::pdf::OutlineDestination::at_page(10), cx)
    });
    action(cx, handle, SplitSideBySide);
    let second = document.read_with(cx, |d, _| d.reader.clone());
    second.update(cx, |r, cx| {
        r.navigate_outline(crate::pdf::OutlineDestination::at_page(20), cx)
    });
    other
        .read_with(cx, |d, _| d.reader.clone())
        .update(cx, |r, cx| {
            r.navigate_outline(crate::pdf::OutlineDestination::at_page(30), cx);
        });
    settle(cx, handle);
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    action(cx, handle, SaveAs);
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    settle(cx, handle);
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, second);
        assert!(d.jumplist.can_back(), "Save As must not clear navigation");
    });
    action(cx, handle, NavigateBack);
    assert_eq!(second.read_with(cx, |r, _| r.page_index()), 10);
    action(cx, handle, SwitchPane);
    action(cx, handle, NavigateBack);
    assert_eq!(first.read_with(cx, |r, _| r.page_index()), 0);
    action(cx, other_handle, NavigateBack);
    other.read_with(cx, |d, cx| assert_eq!(d.reader.read(cx).page_index(), 0));
    action(cx, handle, NavigateForward);
    assert_eq!(first.read_with(cx, |r, _| r.page_index()), 10);
}

#[gpui_kit::test]
fn shared_find_selection_ruler_edit_history_and_save_as_compose_in_one_split(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("guide.pdf");
    let saved = root.path().join("marked-guide.pdf");
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.simulate_window_resize(handle, size(px(1280.), px(950.)));
    settle(cx, handle);
    let first = document.read_with(cx, |d, _| d.reader.clone());
    action(cx, handle, FitWidth);
    action(cx, handle, SplitSideBySide);
    let second = document.read_with(cx, |d, _| d.reader.clone());
    action(cx, handle, ShowFind);
    cx.update_window(handle, |_, window, cx| window.input("Landscape", cx))
        .unwrap();
    settle(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.click(("find-result", 2usize), cx)
    })
    .unwrap();
    settle(cx, handle);
    assert_eq!(second.read_with(cx, |r, _| r.page_index()), 2);
    assert_eq!(first.read_with(cx, |r, _| r.page_index()), 0);
    focus(cx, handle, &second);
    action(cx, handle, crate::reader::ToggleReadingRuler);
    let second_position = second.read_with(cx, |r, _| r.reading_position());
    focus(cx, handle, &first);
    let text = crate::pdf::page_text(&source, 0).unwrap();
    let glyph = &text.glyphs[0];
    let at = text.transform.document_to_page(crate::coordinates::Point {
        x: (glyph.start.x + glyph.end.x) / 2.,
        y: (glyph.top + glyph.bottom) / 2.,
    });
    cx.update_window(handle, |_, window, cx| {
        let page = window
            .within(("reading-pane", first.entity_id()))
            .find("pdf-page-0")
            .bounds();
        let scale = first.read(cx).scale();
        let at = page.origin + point(px(at.x as f32 * scale), px(at.y as f32 * scale));
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
        window
            .within(("reading-pane", first.entity_id()))
            .click_with_options(
                "pdf-page-0",
                gpui_kit::test::ClickOptions::new()
                    .with_offset(at - page.origin)
                    .with_count(2),
                cx,
            );
    })
    .unwrap();
    settle(cx, handle);
    let selected = first.read_with(cx, |r, _| r.selected_text().expect("word selected"));
    action(cx, handle, crate::reader::Highlight);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| assert!(r.is_dirty(cx)));
    }
    focus(cx, handle, &second);
    cx.update_window(handle, |_, window, cx| window.press("secondary-z", cx))
        .unwrap();
    settle(cx, handle);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| assert!(!r.is_dirty(cx)));
    }
    assert_eq!(
        first.read_with(cx, |r, _| r.selected_text()),
        Some(selected)
    );
    assert_eq!(
        second.read_with(cx, |r, _| r.reading_position()),
        second_position
    );
    cx.update_window(handle, |_, window, cx| window.press(REDO_KEY, cx))
        .unwrap();
    settle(cx, handle);
    action(cx, handle, SaveAs);
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    settle(cx, handle);
    let saved = std::fs::canonicalize(saved).unwrap();
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| {
            assert_eq!(r.path(), Some(saved.as_path()));
            assert!(!r.is_dirty(cx));
            assert!(r.has_annotation_undo(cx));
        });
    }
    assert_eq!(
        crate::pdf::Document::open(&saved)
            .unwrap()
            .page_annotations(0, 1)
            .unwrap()
            .len(),
        1
    );
    action(cx, handle, ShowFind);
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("find-query").value(), Some("Landscape"));
        window.press("escape", cx);
    })
    .unwrap();
    settle(cx, handle);
    action(cx, handle, NavigateBack);
    assert_eq!(second.read_with(cx, |r, _| r.page_index()), 0);
    assert_eq!(first.read_with(cx, |r, _| r.page_index()), 0);
    action(cx, handle, NavigateForward);
    assert_eq!(second.read_with(cx, |r, _| r.page_index()), 2);
    action(cx, handle, ClosePane);
    assert_eq!(document.read_with(cx, |d, _| d.reader.clone()), first);
    cx.update_window(handle, |_, window, cx| {
        assert!(first.read(cx).focus_handle(cx).is_focused(window));
    })
    .unwrap();
    let (_, reopened) = open(cx, Some(saved));
    settle(cx, handle);
    reopened.read_with(cx, |d, _| assert!(d.other_pane.is_none()));
}
