//! Shared edit history exercised through mounted production document windows.
use super::saving::select_text;
use super::*;
use crate::{
    pdf::annotations::{Annotation, HighlightColor},
    reader::{DeleteHighlight, Highlight, RecolorHighlight, RedoAnnotation, UndoAnnotation},
};
use gpui_kit::Action;

fn catalog(document: &Entity<DocumentWindow>, cx: &TestAppContext) -> Vec<Annotation> {
    document.read_with(cx, |document, cx| {
        document
            .reader
            .read(cx)
            .session()
            .unwrap()
            .read(cx)
            .annotations()
            .unwrap()
            .unwrap()
            .as_ref()
            .clone()
    })
}

fn action(cx: &mut TestAppContext, handle: AnyWindowHandle, action: impl Action) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(action), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn dirty(cx: &mut TestAppContext, handle: AnyWindowHandle, expected: bool) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.try_find("document-unsaved").is_some(), expected);
    })
    .unwrap();
}

fn persisted(path: &std::path::Path) -> Vec<Annotation> {
    crate::pdf::Document::open(path)
        .unwrap()
        .page_annotations(0, 100)
        .unwrap()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn annotation_history_is_shared_save_retained_and_content_relative(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("history.pdf");
    let independent = directory.path().join("independent.pdf");
    for path in [&source, &independent] {
        std::fs::copy("examples/selection-sample.pdf", path).unwrap();
    }
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    let (third, unrelated) = open(cx, Some(independent));
    cx.run_until_parked();
    select_text(cx, first, &document);
    press(cx, first, "cmd-shift-h");
    let created = catalog(&document, cx);
    assert_eq!(created.len(), 1);
    // Identical selection selects the existing annotation without another edit.
    press(cx, first, "cmd-shift-h");
    assert_eq!(catalog(&other, cx), created);
    press(cx, second, "cmd-z");
    assert!(catalog(&document, cx).is_empty());
    for handle in [first, second] {
        dirty(cx, handle, false);
    }
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.reader.read(cx).selected_text().as_deref(),
            Some("Alpha")
        );
    });
    other.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).selected_text().is_none());
    });
    press(cx, first, "cmd-shift-z");
    assert_eq!(catalog(&other, cx), created);
    press(cx, second, "cmd-s");
    assert_eq!(persisted(&source).len(), 1);
    // Undoing a saved creation is dirty; redoing it returns to the saved state.
    press(cx, first, "cmd-z");
    for handle in [first, second] {
        dirty(cx, handle, true);
    }
    // Undoing a saved edit is still protected by the native quit decision path.
    cx.update(windows::request_quit);
    cx.run_until_parked();
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("unsaved-save").visible());
        window.click("unsaved-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(persisted(&source).len(), 1);
    press(cx, second, "cmd-shift-z");
    for handle in [first, second] {
        dirty(cx, handle, false);
    }

    // Re-arm only this window's target. History does not restore old selection.
    action(cx, first, Highlight);
    action(cx, first, RecolorHighlight(HighlightColor::Green));
    assert_eq!(
        catalog(&other, cx)[0].color(),
        HighlightColor::Green.color()
    );
    action(cx, first, RecolorHighlight(HighlightColor::Yellow));
    // Equality is not limited to revisiting the saved history cursor.
    assert_eq!(catalog(&document, cx), created);
    dirty(cx, second, false);
    action(cx, second, UndoAnnotation);
    dirty(cx, first, true);
    action(cx, first, RedoAnnotation);
    dirty(cx, second, false);
    action(cx, first, DeleteHighlight);
    assert!(catalog(&other, cx).is_empty());
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&document, cx), created);
    dirty(cx, first, false);
    action(cx, first, RedoAnnotation);
    press(cx, second, "cmd-s");
    assert!(persisted(&source).is_empty());
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&other, cx), created);
    dirty(cx, second, true);
    // A real new edit branches; the old deletion cannot be redone.
    action(cx, first, Highlight);
    action(cx, first, RecolorHighlight(HighlightColor::Purple));
    let branched = catalog(&other, cx);
    action(cx, second, RedoAnnotation);
    assert_eq!(catalog(&document, cx), branched);

    // A participant can close without discarding shared history or edits.
    press(cx, first, "cmd-w");
    drop(document);
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&other, cx), created);
    action(cx, second, RedoAnnotation);
    assert_eq!(catalog(&other, cx), branched);
    // A separate document has no history even though its bytes were identical.
    action(cx, third, UndoAnnotation);
    assert!(catalog(&unrelated, cx).is_empty());
    dirty(cx, third, false);
    other.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.scale(), 1.);
        assert!(reader.selected_text().is_none());
        assert_eq!(reader.session().unwrap().read(cx).participant_count(), 1);
    });
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn annotation_history_during_save_uses_the_committed_snapshot_not_the_current_revision(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("async.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    select_text(cx, first, &document);
    press(cx, first, "cmd-shift-h");
    let created = catalog(&document, cx);
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    // Hold foreground completion queued while the real writer commits a
    // captured snapshot. No fake completion or test-only production API.
    session.update(cx, |session, cx| {
        session.start_save(cx).unwrap();
        session.undo_annotation(cx);
        assert!(session.is_saving());
    });
    cx.run_until_parked();
    assert_eq!(persisted(&source).len(), 1);
    assert!(catalog(&other, cx).is_empty());
    other.read_with(cx, |document, cx| {
        assert!(
            document
                .reader
                .read(cx)
                .document()
                .unwrap()
                .page_annotations(0, 200)
                .unwrap()
                .is_empty(),
            "an older save completion must not resurrect its captured preview"
        );
    });
    for handle in [first, second] {
        dirty(cx, handle, true);
    }
    action(cx, second, RedoAnnotation);
    assert_eq!(catalog(&other, cx), created);
    for handle in [first, second] {
        dirty(cx, handle, false);
    }

    // Save a deletion, then undo AND redo while writing. A newer revision
    // with equal content must be clean after that older snapshot commits.
    action(cx, first, Highlight);
    action(cx, first, DeleteHighlight);
    session.update(cx, |session, cx| {
        session.start_save(cx).unwrap();
        session.undo_annotation(cx);
        session.redo_annotation(cx);
    });
    cx.run_until_parked();
    assert!(persisted(&source).is_empty());
    for handle in [first, second] {
        dirty(cx, handle, false);
    }
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&other, cx), created);
    dirty(cx, second, true);
    press(cx, second, "cmd-s");
    assert_eq!(persisted(&source).len(), 1);
    dirty(cx, first, false);
    action(cx, first, RedoAnnotation);
    assert!(catalog(&other, cx).is_empty());
    dirty(cx, first, true);
}

#[gpui_kit::test]
fn annotation_shortcuts_respect_find_input_undo_and_list_focus(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    select_text(cx, handle, &document);
    press(cx, handle, "cmd-shift-h");
    let created = catalog(&document, cx);
    press(cx, handle, "cmd-f");
    cx.update_window(handle, |_, window, cx| window.input("Alpha", cx))
        .unwrap();
    cx.run_until_parked();
    press(cx, handle, "cmd-z");
    assert_eq!(
        catalog(&document, cx),
        created,
        "Find input must own text undo"
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        assert_ne!(window.find("find-query").value(), Some("Alpha"));
    })
    .unwrap();
    action(cx, handle, ShowAnnotations);
    press(cx, handle, "cmd-z");
    assert!(catalog(&document, cx).is_empty());
    // Empty-list focus survives; redo is still routed to this shared session.
    press(cx, handle, "cmd-shift-z");
    assert_eq!(catalog(&document, cx), created);
    press(cx, handle, "down");
    press(cx, handle, "cmd-c");
    cx.update(|cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Alpha")
        );
    });
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn imported_annotation_history_restores_geometry_metadata_and_local_reading_state(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{point, px, test::TestEventExt};
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("imported.pdf");
    std::fs::copy("examples/annotations-clean.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let original = catalog(&document, cx);
    let imported = original
        .iter()
        .find(|a| a.name() == Some("imported-highlight"))
        .unwrap();
    // Select the real imported markup with pointer input on the PDF page.
    let at = document.read_with(cx, |document, cx| {
        let pdf = document.reader.read(cx).document().unwrap();
        let points = imported.quads()[0].points();
        pdf.page_transform(0)
            .unwrap()
            .document_to_page(crate::coordinates::Point {
                x: points.iter().map(|p| p.x).sum::<f64>() / 4.,
                y: points.iter().map(|p| p.y).sum::<f64>() / 4.,
            })
    });
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        let at =
            window.find("pdf-page-0").bounds().origin + point(px(at.x as f32), px(at.y as f32));
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    action(cx, second, crate::reader::NextPage);
    action(cx, second, crate::reader::ZoomIn);
    let position = other.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        (reader.page_index(), reader.scale())
    });
    assert_eq!(position.0, 1);
    assert!(position.1 > 1.);
    action(cx, first, RecolorHighlight(HighlightColor::Blue));
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&document, cx), original);
    dirty(cx, second, false);
    action(cx, second, RedoAnnotation);
    press(cx, first, "cmd-s");
    let saved = persisted(&source);
    let readback = saved.iter().find(|a| a.name() == imported.name()).unwrap();
    assert_eq!(readback.color(), HighlightColor::Blue.color());
    assert_eq!(readback.quads(), imported.quads());
    assert_eq!(readback.author(), imported.author());
    assert_eq!(readback.comment(), imported.comment());
    action(cx, first, DeleteHighlight);
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&document, cx).len(), original.len());
    dirty(cx, first, false);
    action(cx, second, UndoAnnotation);
    assert_eq!(catalog(&document, cx), original);
    dirty(cx, first, true);
    press(cx, first, "cmd-s");
    let restored = persisted(&source);
    let restored = restored
        .iter()
        .find(|a| a.name() == imported.name())
        .unwrap();
    assert_eq!(restored.color(), imported.color());
    assert_eq!(restored.quads(), imported.quads());
    assert_eq!(restored.author(), imported.author());
    assert_eq!(restored.comment(), imported.comment());
    other.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!((reader.page_index(), reader.scale()), position);
        assert!(reader.selected_text().is_none());
    });
    // A save while undone must retain redo, including the original deletion.
    action(cx, second, RedoAnnotation);
    action(cx, first, RedoAnnotation);
    assert!(catalog(&other, cx).iter().all(|a| a.id() != imported.id()));
    press(cx, second, "cmd-s");
    assert!(
        persisted(&source)
            .iter()
            .all(|a| a.name() != imported.name())
    );
}

#[gpui_kit::test]
fn no_op_mutations_preserve_redo_and_create_delete_returns_to_clean(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    select_text(cx, handle, &document);
    press(cx, handle, "cmd-shift-h");
    action(cx, handle, RecolorHighlight(HighlightColor::Green));
    action(cx, handle, UndoAnnotation);
    let before = catalog(&document, cx);
    action(cx, handle, Highlight); // duplicate must neither edit nor branch
    action(cx, handle, RecolorHighlight(HighlightColor::Yellow)); // same color
    action(cx, handle, RedoAnnotation);
    assert_eq!(
        catalog(&document, cx)[0].color(),
        HighlightColor::Green.color()
    );
    action(cx, handle, UndoAnnotation);
    assert_eq!(catalog(&document, cx), before);
    action(cx, handle, DeleteHighlight);
    assert!(catalog(&document, cx).is_empty());
    dirty(cx, handle, false);
    action(cx, handle, UndoAnnotation);
    assert_eq!(catalog(&document, cx), before);
    dirty(cx, handle, true);
}

#[gpui_kit::test]
fn palette_annotation_history_commands_work_from_either_preset(cx: &mut TestAppContext) {
    setup(cx);
    for preset in [
        crate::app::keymap::Preset::Standard,
        crate::app::keymap::Preset::Vim,
    ] {
        cx.update(|cx| crate::app::keymap::select_preset(cx, preset).unwrap());
        let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
        cx.run_until_parked();
        select_text(cx, handle, &document);
        press(cx, handle, "cmd-shift-h");
        let created = catalog(&document, cx);
        for (query, expected) in [("Undo", 0), ("Redo", 1)] {
            press(cx, handle, "cmd-shift-p");
            cx.update_window(handle, |_, window, cx| window.input(query, cx))
                .unwrap();
            cx.run_until_parked();
            press(cx, handle, "enter");
            assert!(!document.read_with(cx, |document, _| document.palette_open()));
            assert_eq!(catalog(&document, cx).len(), expected);
        }
        assert_eq!(catalog(&document, cx), created);
        press(cx, handle, "cmd-z");
        assert!(catalog(&document, cx).is_empty());
        press(cx, handle, "cmd-w");
        drop(document);
    }
}

#[gpui_kit::test]
fn save_as_and_failed_writes_preserve_annotation_history(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let source = root.join("source.pdf");
    let copy = root.join("copy.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let source_bytes = std::fs::read(&source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    select_text(cx, first, &document);
    press(cx, first, "cmd-shift-h");
    let created = catalog(&document, cx);
    // Cancellation must preserve both history and the original baseline.
    action(cx, first, SaveAs);
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    action(cx, second, UndoAnnotation);
    dirty(cx, first, false);
    action(cx, first, RedoAnnotation);
    action(cx, first, SaveAs);
    cx.simulate_new_path_selection(|_| Some(copy.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
    assert_eq!(persisted(&copy).len(), 1);
    action(cx, second, UndoAnnotation);
    for handle in [first, second] {
        dirty(cx, handle, true);
    }
    action(cx, first, RedoAnnotation);
    for handle in [first, second] {
        dirty(cx, handle, false);
    }
    assert_eq!(catalog(&other, cx), created);
    other.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).path(), Some(copy.as_path()));
    });
    action(cx, first, Highlight);
    action(cx, first, RecolorHighlight(HighlightColor::Pink));
    let external = [
        std::fs::read(&copy).unwrap(),
        b"\n% external change\n".to_vec(),
    ]
    .concat();
    std::fs::write(&copy, &external).unwrap();
    press(cx, second, "cmd-s");
    assert_eq!(std::fs::read(&copy).unwrap(), external);
    dirty(cx, first, true);
    press(cx, second, "escape"); // dismiss conflict feedback
    action(cx, first, UndoAnnotation);
    assert_eq!(catalog(&other, cx), created);
    dirty(cx, second, false);
    action(cx, second, RedoAnnotation);
    assert_eq!(catalog(&other, cx)[0].color(), HighlightColor::Pink.color());
    dirty(cx, first, true);
}
