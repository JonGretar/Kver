//! External rewrites pass through the real shared session and document windows.
use super::*;
use crate::pdf::annotations::Annotation;
use gpui_kit::test::{TestEventExt, TestQueryExt};
use std::{path::Path, time::Duration};

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

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
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

fn poll(cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
}

/// External readers may remove /NM and rebuild all PDF object identifiers.
/// Change content too: object numbers or attached note text are not identity.
fn rewrite(path: &Path) {
    let mut pdf = lopdf::Document::load(path).unwrap();
    for object in pdf.objects.values_mut() {
        if let Ok(dictionary) = object.as_dict_mut()
            && dictionary
                .get(b"Subtype")
                .is_ok_and(|subtype| subtype.as_name().is_ok_and(|name| name == b"Highlight"))
        {
            dictionary.remove(b"NM");
            dictionary.set(
                "Contents",
                lopdf::Object::string_literal("external attached comment"),
            );
            dictionary.set("C", vec![0.into(), 0.into(), 1.into()]);
        }
    }
    pdf.renumber_objects_with(500);
    pdf.save(path).unwrap();
}

#[gpui_kit::test]
fn clean_reload_refreshes_identifiers_and_preserves_each_windows_position(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("external.pdf");
    std::fs::copy("examples/annotations-clean.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    // Create history, then save it: a clean session can still have undo.
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let old = catalog(&document, cx);
    let target = old.iter().find(|a| !a.is_readonly()).unwrap().id();
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    session.update(cx, |session, cx| {
        session
            .edit_annotation(
                target,
                Some(crate::pdf::annotations::HighlightColor::Green),
                cx,
            )
            .unwrap();
        session.start_save(cx).unwrap();
    });
    cx.run_until_parked();
    other.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).has_annotation_undo(cx))
    });
    let reader = other.read_with(cx, |document, _| document.reader.clone());
    reader.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.zoom_in(cx);
    });
    for handle in [first, second] {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    cx.run_until_parked();
    let before = [document.clone(), other.clone()].map(|document| {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            (
                reader.reading_position(),
                reader.scale(),
                reader.document_generation(),
            )
        })
    });
    let old_target = old.iter().find(|a| !a.is_readonly()).unwrap().id();
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(format!(
                "annotation-{}-{}",
                old_target.epoch(),
                old_target.serial()
            ))
            .click("recolor-annotation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    rewrite(&source);
    poll(cx);
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        if let Some(choice) = window.try_find_by_label("Pink") {
            // A queued old menu must not target the annotation now occupying
            // the same imported slot after the external rewrite.
            window.pointer_down(choice.bounds().center(), gpui_kit::MouseButton::Left, cx);
            window.pointer_up(choice.bounds().center(), gpui_kit::MouseButton::Left, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
    let new = catalog(&other, cx);
    assert!(!new.is_empty());
    assert!(new.iter().all(|a| old.iter().all(|old| a.id() != old.id())));
    for (document, (position, scale, generation)) in
        [document.clone(), other.clone()].into_iter().zip(before)
    {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert_eq!(
                reader.reading_position(),
                position,
                "scale {scale}, generation {generation}"
            );
            assert_eq!(reader.scale(), scale);
            assert!(reader.document_generation() > generation);
            assert!(!reader.is_dirty(cx));
            assert!(!reader.has_annotation_undo(cx));
            assert!(!reader.has_annotation_redo(cx));
            assert!(reader.selected_text().is_none());
        });
    }
    for handle in [first, second] {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("document-unsaved").is_none());
        })
        .unwrap();
    }
    // The external writer removed names and renumbered every object. A real
    // list command must target the new baseline, not the original source slot.
    let target = new
        .iter()
        .find(|a| a.kind() == crate::pdf::annotations::MarkupKind::Highlight && !a.is_readonly())
        .unwrap();
    let row = format!(
        "annotation-{}-{}",
        target.id().epoch(),
        target.id().serial()
    );
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        window.within(row).click("delete-annotation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    press(cx, first, "cmd-s");
    let readback = crate::pdf::Document::open(&source)
        .unwrap()
        .page_annotations(target.page(), 99)
        .unwrap();
    assert_eq!(
        readback.len(),
        new.iter().filter(|a| a.page() == target.page()).count() - 1
    );
    assert!(readback.iter().all(|a| a.quads() != target.quads()));
}

#[gpui_kit::test]
fn dirty_conflict_cancel_picker_failure_and_discard_keep_both_versions_safe(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("external.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, first, &document);
    let local = catalog(&document, cx);
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let external = std::fs::read(&source).unwrap();
    // Save's commit boundary remains protective even before the polling deadline.
    press(cx, first, "cmd-s");
    let position = document.read_with(cx, |document, cx| {
        document.reader.read(cx).reading_position()
    });
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        window.press("down", cx);
        window.input("j", cx);
        window.click("pdf-page-0", cx);
        assert!(
            window.find("save-conflict-copy").visible(),
            "backdrop must not dismiss a destructive decision"
        );
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!(reader.reading_position(), position);
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
    });
    click(cx, first, "save-conflict-cancel");
    assert_eq!(catalog(&other, cx), local);
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-copy");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(catalog(&other, cx), local);
    // Refuse a destination belonging to an independent session.
    let occupied = directory.path().join("occupied.pdf");
    std::fs::copy("examples/selection-sample.pdf", &occupied).unwrap();
    let (_, _unrelated) = open(cx, Some(occupied.clone()));
    cx.run_until_parked();
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-copy");
    cx.simulate_new_path_selection(|_| Some(occupied.clone()));
    cx.run_until_parked();
    click(cx, first, "overwrite-confirm");
    assert_eq!(catalog(&other, cx), local);
    assert_eq!(std::fs::read(&source).unwrap(), external);
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-reload");
    for document in [&document, &other] {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert!(!reader.is_dirty(cx));
            assert!(!reader.has_annotation_undo(cx));
            assert!(reader.page_count().unwrap() > 1);
        });
    }
    assert_eq!(std::fs::read(&source).unwrap(), external);
    assert!(catalog(&other, cx).is_empty());
    press(cx, second, "cmd-z");
    assert!(catalog(&other, cx).is_empty());
}

#[gpui_kit::test]
fn conflict_save_as_moves_all_windows_without_overwriting_external_pdf(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let destination = directory.path().join("copy.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (_, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, first, &document);
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let external = std::fs::read(&source).unwrap();
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-copy");
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read(&source).unwrap(), external);
    for document in [&document, &other] {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert_eq!(
                reader.path(),
                Some(std::fs::canonicalize(&destination).unwrap().as_path())
            );
            assert!(!reader.is_dirty(cx));
            assert!(reader.has_annotation_undo(cx));
        });
    }
    assert_eq!(
        crate::pdf::Document::open(&destination)
            .unwrap()
            .page_annotations(0, 99)
            .unwrap()
            .len(),
        1
    );
}

#[gpui_kit::test]
fn polling_offers_one_conflict_and_failed_reload_preserves_edits(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, first, &document);
    cx.update_window(first, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    let local = catalog(&other, cx);
    std::fs::write(&source, b"unfinished external PDF").unwrap();
    poll(cx);
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("save-conflict-copy").is_none());
    })
    .unwrap();
    click(cx, first, "save-conflict-reload");
    assert_eq!(catalog(&other, cx), local);
    for document in [&document, &other] {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert!(reader.is_dirty(cx));
            assert!(reader.has_annotation_undo(cx));
            assert!(reader.status().contains("previous PDF remains available"));
        });
    }
    assert_eq!(std::fs::read(&source).unwrap(), b"unfinished external PDF");
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-reload");
    assert!(!other.read_with(cx, |document, cx| document.reader.read(cx).is_dirty(cx)));
}

#[gpui_kit::test]
fn queued_reload_cannot_discard_newer_edits_or_replace_another_document(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (_, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, first, &document);
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    let local = catalog(&document, cx);
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    cx.update(|cx| {
        session.update(cx, |session, cx| session.check_external(true, true, cx));
        cx.update_window(first, |_, window, cx| {
            window.dispatch_action(Box::new(crate::reader::UndoAnnotation), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    other.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!(
            reader.page_count(),
            Some(1),
            "obsolete discard must not install"
        );
        assert!(
            reader.has_annotation_redo(cx),
            "reload must not clear newer history"
        );
    });
    press(cx, first, "cmd-shift-z");
    assert_eq!(catalog(&other, cx), local);
    cx.update(|cx| {
        session.update(cx, |session, cx| session.check_external(true, false, cx));
        document.read(cx).reader.clone().update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("examples/annotations-clean.pdf"), cx);
        });
    });
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).page_count(), Some(2));
        assert!(!document.reader.read(cx).is_dirty(cx));
    });
    assert_eq!(catalog(&other, cx), local);
}

#[gpui_kit::test]
fn clean_reload_invalidates_queued_native_destination_and_clamps_removed_pages(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let destination = directory.path().join("obsolete-choice.pdf");
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(SinglePage), cx);
        window.dispatch_action(Box::new(crate::reader::GoToDocumentEnd), cx);
    })
    .unwrap();
    cx.run_until_parked();
    other.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).reading_position().0 > 0);
    });
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    poll(cx);
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    assert!(
        !destination.exists(),
        "obsolete native picker reply must not save a reloaded baseline"
    );
    for document in [&document, &other] {
        document.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert_eq!(reader.page_count(), Some(1));
            assert_eq!(reader.reading_position().0, 0);
            assert_eq!(reader.path(), Some(source.as_path()));
            assert!(!reader.is_dirty(cx));
            assert!(!document.save_prompt_open);
        });
    }
}

#[gpui_kit::test]
fn reload_resets_all_markup_history_but_retains_per_type_colors_across_shared_save_as(
    cx: &mut TestAppContext,
) {
    use crate::pdf::annotations::{HighlightColor, MarkupKind};
    use crate::reader::{Highlight, RecolorHighlight, Squiggly, StrikeOut, Underline};

    fn create(cx: &mut TestAppContext, handle: AnyWindowHandle, kind: MarkupKind) {
        let action: Box<dyn gpui_kit::Action> = match kind {
            MarkupKind::Highlight => Box::new(Highlight),
            MarkupKind::Underline => Box::new(Underline),
            MarkupKind::StrikeOut => Box::new(StrikeOut),
            MarkupKind::Squiggly => Box::new(Squiggly),
        };
        cx.update_window(handle, |_, window, cx| window.dispatch_action(action, cx))
            .unwrap();
        cx.run_until_parked();
    }

    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("all-types.pdf");
    let destination = directory.path().join("all-types-copy.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let choices = [
        (MarkupKind::Highlight, HighlightColor::Green),
        (MarkupKind::Underline, HighlightColor::Blue),
        (MarkupKind::StrikeOut, HighlightColor::Pink),
        (MarkupKind::Squiggly, HighlightColor::Yellow),
    ];
    saving::select_text(cx, first, &document);
    for (kind, color) in choices {
        create(cx, first, kind);
        cx.update_window(first, |_, window, cx| {
            window.dispatch_action(Box::new(RecolorHighlight(color)), cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    let before = catalog(&document, cx);
    assert_eq!(before.len(), 4);
    assert_eq!(catalog(&other, cx), before);
    rewrite(&source);
    press(cx, first, "cmd-s");
    click(cx, first, "save-conflict-reload");
    for participant in [&document, &other] {
        participant.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert!(!reader.has_annotation_undo(cx));
            assert!(!reader.has_annotation_redo(cx));
            assert!(!reader.is_dirty(cx));
        });
        assert!(catalog(participant, cx).is_empty());
    }
    saving::select_text(cx, second, &other);
    for (kind, color) in choices {
        create(cx, second, kind);
        let annotations = catalog(&document, cx);
        let created = annotations.iter().find(|a| a.kind() == kind).unwrap();
        assert_eq!(created.color(), color.color(), "reload lost {kind:?} color");
        assert!(before.iter().all(|old| old.id() != created.id()));
    }
    let recreated = catalog(&other, cx);
    press(cx, first, "cmd-z");
    assert_eq!(catalog(&other, cx).len(), 3);
    press(cx, second, "cmd-shift-z");
    assert_eq!(catalog(&document, cx), recreated);

    // The conflict-copy pipeline must serialize all kinds against the fresh
    // editing source and move every participant only after commit succeeds.
    rewrite(&source);
    let external_bytes = std::fs::read(&source).unwrap();
    press(cx, second, "cmd-s");
    click(cx, second, "save-conflict-copy");
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    let saved = crate::pdf::Document::open(&destination)
        .unwrap()
        .page_annotations(0, 99)
        .unwrap();
    assert_eq!(saved.len(), 4);
    for (kind, color) in choices {
        assert_eq!(
            saved.iter().find(|a| a.kind() == kind).unwrap().color(),
            color.color()
        );
    }
    assert_eq!(std::fs::read(&source).unwrap(), external_bytes);
    for participant in [&document, &other] {
        participant.read_with(cx, |document, cx| {
            let reader = document.reader.read(cx);
            assert_eq!(
                reader.path(),
                Some(std::fs::canonicalize(&destination).unwrap().as_path())
            );
            assert!(!reader.is_dirty(cx));
            assert!(reader.has_annotation_undo(cx));
        });
    }
}
