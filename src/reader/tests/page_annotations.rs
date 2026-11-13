use super::authoring::{draw, select_title};
use super::*;
use crate::pdf::annotations::{Annotation, HighlightColor};
use gpui_kit::test::{TestEventExt, TestQueryExt};

#[gpui_kit::test]
fn mixed_type_overlap_requires_an_explicit_target_and_preserves_text_dragging(
    cx: &mut TestAppContext,
) {
    use crate::pdf::annotations::MarkupKind;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    // Palette routing must work for every new creation Action too.
    for label in [
        "Highlight",
        "Underline",
        "Strikethrough",
        "Squiggly underline",
    ] {
        cx.update_window(handle, |_, window, cx| window.press("cmd-shift-p", cx))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.input(label, cx);
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    let annotations = catalog(&reader, cx);
    assert_eq!(annotations.len(), 4);
    click_annotation(cx, handle, &reader, &annotations[0]);
    reader.read_with(cx, |reader, _| {
        assert!(reader.selected_annotation.is_none());
        assert!(reader.annotation_choice.is_some());
    });
    cx.update_window(handle, |_, window, cx| {
        window.input("Squiggly", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.selected_page_annotation(cx).unwrap().kind(),
            MarkupKind::Squiggly
        );
    });
    cx.update_window(handle, |_, window, cx| window.click("delete-highlight", cx))
        .unwrap();
    cx.run_until_parked();
    let remaining = catalog(&reader, cx);
    assert_eq!(remaining.len(), 3);
    assert!(remaining.iter().all(|a| a.kind() != MarkupKind::Squiggly));
    select_title(cx, handle, &reader, &path);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
        assert!(reader.selected_annotation.is_none());
        assert!(reader.annotation_choice.is_none());
    });
}

#[gpui_kit::test]
fn every_imported_type_uses_page_controls_and_shared_history(cx: &mut TestAppContext) {
    use crate::pdf::annotations::MarkupKind;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    for (kind, subtype) in
        MarkupKind::ALL
            .into_iter()
            .zip(["Highlight", "Underline", "StrikeOut", "Squiggly"])
    {
        let source = directory.path().join(format!("{subtype}.pdf"));
        let mut fixture = lopdf::Document::load("examples/annotations-clean.pdf").unwrap();
        fixture
            .get_dictionary_mut((7, 0))
            .unwrap()
            .set("Subtype", lopdf::Object::Name(subtype.as_bytes().to_vec()));
        fixture.save(&source).unwrap();
        let (handle, reader) = recent_test_window(cx, Some(source.clone()));
        cx.run_until_parked();
        let imported = catalog(&reader, cx)
            .into_iter()
            .find(|a| a.name() == Some("imported-highlight"))
            .unwrap();
        assert_eq!(imported.kind(), kind);
        click_annotation(cx, handle, &reader, &imported);
        reader.read_with(cx, |reader, cx| {
            assert!(reader.annotation_edit_available(cx))
        });
        cx.update_window(handle, |_, window, cx| {
            window.click("recolor-highlight", cx)
        })
        .unwrap();
        cx.run_until_parked();
        click_label(cx, handle, "Green");
        let changed = catalog(&reader, cx)
            .into_iter()
            .find(|a| a.id() == imported.id())
            .unwrap();
        assert_eq!(changed.color(), HighlightColor::Green.color());
        assert_eq!(changed.quads(), imported.quads());
        assert_eq!(changed.author(), imported.author());
        assert_eq!(changed.comment(), imported.comment());
        cx.update_window(handle, |_, window, cx| window.click("delete-highlight", cx))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            assert!(reader.read(cx).focus.is_focused(window));
        })
        .unwrap();
        assert!(catalog(&reader, cx).iter().all(|a| a.id() != imported.id()));
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(UndoAnnotation), cx)
        })
        .unwrap();
        cx.run_until_parked();
        assert!(catalog(&reader, cx).contains(&changed));
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(crate::app::document_window::Save), cx)
        })
        .unwrap();
        cx.run_until_parked();
        let saved = pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 800)
            .unwrap();
        let saved = saved.iter().find(|a| a.name() == imported.name()).unwrap();
        assert_eq!(saved.kind(), kind);
        assert_eq!(saved.quads(), imported.quads());
        assert_eq!(saved.color(), HighlightColor::Green.color());
        assert_eq!(saved.author(), imported.author());
        assert_eq!(saved.comment(), imported.comment());
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx)
        })
        .unwrap();
        cx.run_until_parked();
    }
}

fn catalog(reader: &Entity<ReaderView>, cx: &TestAppContext) -> Vec<Annotation> {
    reader.read_with(cx, |reader, cx| {
        reader
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

fn annotation_point(
    reader: &Entity<ReaderView>,
    annotation: &Annotation,
    cx: &TestAppContext,
) -> gpui::Point<gpui::Pixels> {
    reader.read_with(cx, |reader, _| {
        let points = annotation.quads()[0].points();
        let centre = Point {
            x: points.iter().map(|p| p.x).sum::<f64>() / 4.,
            y: points.iter().map(|p| p.y).sum::<f64>() / 4.,
        };
        let screen = reader
            .page_viewport(annotation.page())
            .unwrap()
            .document_to_screen(centre);
        point(px(screen.x as f32), px(screen.y as f32))
    })
}

fn click_annotation(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    reader: &Entity<ReaderView>,
    annotation: &Annotation,
) {
    draw(cx, handle);
    let at = annotation_point(reader, annotation, cx);
    cx.update_window(handle, |_, window, cx| {
        window.pointer_down(at, gpui::MouseButton::Left, cx);
        window.pointer_up(at, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn click_label(cx: &mut TestAppContext, handle: AnyWindowHandle, label: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let at = window.find_by_label(label).bounds().center();
        window.pointer_down(at, gpui::MouseButton::Left, cx);
        window.pointer_up(at, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn imported_page_highlight_recolor_delete_and_save_as_are_shared_but_selection_is_local(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("imported.pdf");
    let output = directory.path().join("recolored.pdf");
    std::fs::copy("examples/annotations-clean.pdf", &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let (first, reader) = recent_test_window(cx, Some(source.clone()));
    let (second, other) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    draw(cx, first);
    draw(cx, second);
    let imported = catalog(&reader, cx)
        .into_iter()
        .find(|a| a.name() == Some("imported-highlight"))
        .unwrap();
    click_annotation(cx, first, &reader, &imported);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_annotation, Some(imported.id()));
        assert!(reader.selected_text().is_none());
    });
    other.read_with(cx, |reader, _| {
        assert!(reader.selected_annotation.is_none())
    });
    cx.update_window(first, |_, window, cx| {
        window.click("recolor-highlight", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
    cx.update_window(first, |_, window, cx| window.click("recolor-highlight", cx))
        .unwrap();
    cx.run_until_parked();
    click_label(cx, first, "Pink");
    let changed = catalog(&other, cx)
        .into_iter()
        .find(|a| a.id() == imported.id())
        .unwrap();
    assert_eq!(changed.color(), HighlightColor::Pink.color());
    assert_eq!(changed.quads(), imported.quads());
    assert_eq!(changed.author(), imported.author());
    assert_eq!(changed.comment(), imported.comment());
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_annotation, Some(imported.id()))
    });
    click_annotation(cx, second, &other, &changed);
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::document_window::SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read(source).unwrap(), original);
    let saved = pdf::Document::open(&output)
        .unwrap()
        .page_annotations(0, 700)
        .unwrap();
    let readback = saved.iter().find(|a| a.name() == imported.name()).unwrap();
    assert_eq!(readback.color(), changed.color());
    assert_eq!(readback.author(), imported.author());
    assert_eq!(readback.quads(), imported.quads());
    draw(cx, second);
    cx.update_window(second, |_, window, cx| window.press("backspace", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(catalog(&reader, cx).iter().all(|a| a.id() != imported.id()));
    for reader in [&reader, &other] {
        reader.read_with(cx, |reader, cx| {
            assert!(
                reader.selected_annotation.is_none(),
                "stale selection must clear in both windows"
            );
            assert!(reader.is_dirty(cx));
        });
    }
    // The removal is visible in the actual shared preview, not just its catalog.
    other.read_with(cx, |reader, _| {
        assert!(
            reader
                .document()
                .unwrap()
                .page_annotations(0, 701)
                .unwrap()
                .iter()
                .all(|a| a.name() != imported.name())
        );
    });
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::document_window::Save), cx)
    })
    .unwrap();
    cx.run_until_parked();
    let remaining = pdf::Document::open(&output)
        .unwrap()
        .page_annotations(0, 702)
        .unwrap();
    assert_eq!(remaining.len(), saved.len() - 1);
    assert!(remaining.iter().all(|a| a.name() != imported.name()));
}

fn overlapping_pdf(path: &Path) {
    use lopdf::{Document, Object};
    let mut pdf = Document::load("examples/annotations-clean.pdf").unwrap();
    let duplicate = pdf.get_object((7, 0)).unwrap().clone();
    let duplicate = pdf.add_object(duplicate);
    let page = pdf.get_dictionary_mut((3, 0)).unwrap();
    page.get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(duplicate));
    pdf.save(path).unwrap();
}

#[gpui_kit::test]
fn overlapping_highlights_require_choice_modal_input_does_not_reach_text_and_stale_choice_is_safe(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("overlap.pdf");
    overlapping_pdf(&source);
    let (handle, reader) = recent_test_window(cx, Some(source.clone()));
    let (second, other) = recent_test_window(cx, Some(source));
    cx.run_until_parked();
    draw(cx, second);
    let imported = catalog(&reader, cx)
        .into_iter()
        .find(|a| a.name() == Some("imported-highlight"))
        .unwrap();
    click_annotation(cx, handle, &reader, &imported);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.annotation_choice.is_some());
        assert!(reader.selected_annotation.is_none());
        assert!(!reader.is_dirty(cx));
    });
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("highlight-overlap-choice").visible());
        window.input("Fixture", cx);
        window.press("down", cx);
        window.press("backspace", cx);
        window.press("escape", cx);
        // Kit's searchable Command clears a nonempty query before dismissal.
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(reader.annotation_choice.is_none());
        assert!(reader.selected_annotation.is_none());
        assert!(reader.selected_text().is_none());
        assert_eq!(reader.page_index(), 0);
        assert!(!reader.is_dirty(cx));
    });
    // Outside click dismisses without beginning a PDF selection.
    click_annotation(cx, handle, &reader, &imported);
    cx.update_window(handle, |_, window, cx| {
        let at = point(px(5.), px(150.));
        window.pointer_down(at, gpui::MouseButton::Left, cx);
        window.pointer_up(at, gpui::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert!(reader.annotation_choice.is_none()));
    click_annotation(cx, handle, &reader, &imported);
    cx.update_window(handle, |_, window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert!(reader.annotation_choice.is_none());
        assert_ne!(
            reader.selected_annotation,
            Some(imported.id()),
            "down chooses the second overlapping entry"
        );
    });
    click_annotation(cx, handle, &reader, &imported);
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    let selected = reader.read_with(cx, |reader, _| reader.selected_annotation.unwrap());
    assert_eq!(selected, imported.id());
    // Open the same choice in another participant, then delete its first target.
    click_annotation(cx, second, &other, &imported);
    cx.update_window(handle, |_, window, cx| window.click("delete-highlight", cx))
        .unwrap();
    cx.run_until_parked();
    let before = catalog(&reader, cx).len();
    cx.update_window(second, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    other.read_with(cx, |reader, _| {
        assert!(reader.annotation_choice.is_none());
        assert!(reader.selected_annotation.is_none());
    });
    cx.update_window(second, |_, window, cx| window.press("backspace", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        catalog(&reader, cx).len(),
        before,
        "stale choice must not retarget the surviving overlap"
    );
}

#[gpui_kit::test]
fn text_drag_over_a_highlight_clears_annotation_target_and_duplicate_ignores_last_color(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    let original = catalog(&reader, cx)[0].clone();
    // Make a distinct highlight and change its color. The first region remains
    // yellow while the session's next-creation color becomes purple.
    draw(cx, handle);
    let text = pdf::page_text(&path, 0).unwrap();
    let (start, end) = reader.read_with(cx, |reader, _| {
        let viewport = reader.page_viewport(0).unwrap();
        let at = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let screen = viewport.document_to_screen(Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            point(px(screen.x as f32), px(screen.y as f32))
        };
        (at(6, 0.1), at(9, 0.8))
    });
    cx.update_window(handle, |_, window, cx| {
        window.drag(start, end, cx);
        window.press("cmd-shift-h", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(RecolorHighlight(HighlightColor::Purple)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    reader.read_with(cx, |reader, _| {
        assert!(reader.selected_annotation.is_none());
        assert!(reader.selected_text().is_some());
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("backspace", cx);
        window.press("cmd-c", cx);
        window.press("cmd-shift-h", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let after = catalog(&reader, cx);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].id(), original.id());
    assert_eq!(
        after[0].color(),
        original.color(),
        "duplicate must not adopt the last-used purple"
    );
    assert_eq!(after[1].color(), HighlightColor::Purple.color());
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_annotation, Some(original.id()));
        assert!(
            reader.selected_text().is_some(),
            "creation retains text for copy"
        );
    });
    // A later distinct creation uses the session-local last-used color.
    let (start, end) = reader.read_with(cx, |reader, _| {
        let viewport = reader.page_viewport(0).unwrap();
        let at = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let screen = viewport.document_to_screen(Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            point(px(screen.x as f32), px(screen.y as f32))
        };
        (at(11, 0.1), at(14, 0.8))
    });
    cx.update_window(handle, |_, window, cx| {
        window.drag(start, end, cx);
        window.press("cmd-shift-h", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let created = catalog(&reader, cx);
    assert_eq!(created.len(), 3);
    assert_eq!(created[2].color(), HighlightColor::Purple.color());
}

#[gpui_kit::test]
fn replacing_document_unmounts_the_owned_overlap_choice_and_old_commands_cannot_edit_the_replacement(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("overlap.pdf");
    overlapping_pdf(&source);
    let (handle, reader) = recent_test_window(cx, Some(source));
    cx.run_until_parked();
    let imported = catalog(&reader, cx)
        .into_iter()
        .find(|a| a.name() == Some("imported-highlight"))
        .unwrap();
    click_annotation(cx, handle, &reader, &imported);
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/selection-sample.pdf"), cx)
    });
    cx.run_until_parked();
    draw(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("highlight-overlap-choice").is_none());
        window.press("enter", cx);
        window.press("backspace", cx);
        window.dispatch_action(Box::new(RecolorHighlight(HighlightColor::Green)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(catalog(&reader, cx).is_empty());
    reader.read_with(cx, |reader, cx| {
        assert!(reader.selected_annotation.is_none());
        assert!(!reader.is_dirty(cx));
    });
}

#[gpui_kit::test]
fn releasing_a_text_drag_outside_the_reader_ends_the_gesture_without_arming_an_edit(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) = recent_test_window(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    let annotation = catalog(&reader, cx)
        .into_iter()
        .find(|a| a.name() == Some("imported-highlight"))
        .unwrap();
    click_annotation(cx, handle, &reader, &annotation);
    let at = annotation_point(&reader, &annotation, cx);
    cx.update_window(handle, |_, window, cx| {
        window.pointer_down(at, gpui::MouseButton::Left, cx);
        window.pointer_move(
            at + point(px(12.), px(0.)),
            Some(gpui::MouseButton::Left),
            cx,
        );
        window.pointer_up(point(px(100.), px(1.)), gpui::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(reader.annotation_press.is_none());
        assert!(reader.selection.dragging_page().is_none());
        assert!(reader.selected_annotation.is_none());
        assert!(!reader.is_dirty(cx));
    });
    click_annotation(cx, handle, &reader, &annotation);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.annotation_edit_available(cx))
    });
}

#[gpui_kit::test]
fn reapplying_highlight_to_duplicate_imported_regions_requires_choice_and_retains_text(
    cx: &mut TestAppContext,
) {
    use crate::pdf::annotations::AnnotationId;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    let first = catalog(&reader, cx)[0].clone();
    let second = Annotation::new(
        AnnotationId::new(first.id().epoch(), first.id().serial() + 1),
        first.page(),
        first.kind(),
        first.quads().to_vec(),
        HighlightColor::Green.color(),
    )
    .with_opacity(first.opacity());
    let directory = tempfile::tempdir().unwrap();
    let imported = directory.path().join("duplicate-regions.pdf");
    let bytes = pdf::Document::open(&path)
        .unwrap()
        .with_annotation_edits(&[], &[first, second])
        .unwrap();
    std::fs::write(&imported, bytes).unwrap();
    reader.update(cx, |reader, cx| reader.open_path(imported.clone(), cx));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &imported);
    let text = reader.read_with(cx, |reader, _| reader.selected_text().unwrap());
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    let before = catalog(&reader, cx);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.annotation_choice.is_some());
        assert!(reader.selected_annotation.is_none());
        assert_eq!(reader.selected_text().as_deref(), Some(text.as_str()));
        assert!(!reader.is_dirty(cx));
    });
    cx.update_window(handle, |_, window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.selected_annotation, Some(before[1].id()));
        assert_eq!(reader.selected_text().as_deref(), Some(text.as_str()));
        assert!(!reader.is_dirty(cx));
    });
    assert_eq!(catalog(&reader, cx), before);
    let session = reader.read_with(cx, |reader, _| reader.session().unwrap());
    session.update(cx, |session, cx| {
        assert!(
            session
                .create_highlight(0, before[0].quads().to_vec(), cx)
                .is_err(),
            "the shared seam also refuses arbitrary duplicate targeting"
        );
        assert!(!session.is_dirty());
    });
}

#[gpui_kit::test]
fn a_window_opened_after_unsaved_creation_immediately_displays_the_shared_edited_pdf(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("late-participant.pdf");
    std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
    let (first, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, first, &reader, &path);
    cx.update_window(first, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    let created = catalog(&reader, cx);
    assert_eq!(created.len(), 1);
    let (second, other) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    draw(cx, second);
    other.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx));
        assert!(reader.selected_text().is_none());
        assert_eq!(
            reader
                .document()
                .unwrap()
                .page_annotations(0, 900)
                .unwrap()
                .len(),
            1,
            "late participant must render unsaved markup before any further edit"
        );
    });
}

#[gpui_kit::test]
#[ignore = "Kver #142 / Hayro #1575: invalid Annots entries hide subsequent appearances"]
fn newly_created_highlight_has_visible_color_in_the_production_pdf_preview_before_save(
    cx: &mut TestAppContext,
) {
    assert_new_highlight_color(cx, "examples/annotations.pdf", 8);
}

#[gpui_kit::test]
fn newly_created_highlight_has_visible_color_in_a_clean_pdf_before_save(cx: &mut TestAppContext) {
    assert_new_highlight_color(cx, "examples/annotations-clean.pdf", 6);
}

#[gpui_kit::test]
fn new_line_types_have_visible_distinct_appearances_in_the_production_preview_before_save(
    cx: &mut TestAppContext,
) {
    use crate::pdf::annotations::MarkupKind;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    for (kind, action) in [
        (
            MarkupKind::Underline,
            Box::new(Underline) as Box<dyn gpui_kit::Action>,
        ),
        (MarkupKind::StrikeOut, Box::new(StrikeOut)),
        (MarkupKind::Squiggly, Box::new(Squiggly)),
    ] {
        let path = directory.path().join(format!("{kind:?}.pdf"));
        std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
        let (handle, reader) = recent_test_window(cx, Some(path.clone()));
        cx.run_until_parked();
        select_title(cx, handle, &reader, &path);
        cx.update_window(handle, |_, window, cx| window.dispatch_action(action, cx))
            .unwrap();
        cx.run_until_parked();
        let document = reader.read_with(cx, |reader, _| reader.document().unwrap());
        let annotation = &document.page_annotations(0, 901).unwrap()[0];
        assert_eq!(annotation.kind(), kind);
        let [left, top, right, bottom] = document.annotation_bounds(annotation).unwrap();
        let bitmap = document.render_bitmap(0, 2.).unwrap();
        let mut red = 0;
        let mut vertical = Vec::new();
        for y in (top * 2.) as u32..(bottom * 2.).ceil() as u32 {
            for x in (left * 2.) as u32..(right * 2.).ceil() as u32 {
                let offset = ((y * bitmap.width + x) * 4) as usize;
                let b = i16::from(bitmap.bgra[offset]);
                let g = i16::from(bitmap.bgra[offset + 1]);
                let r = i16::from(bitmap.bgra[offset + 2]);
                if r > 150 && r > b + 50 && r > g + 50 {
                    red += 1;
                    vertical.push((y as f64 / 2. - top) / (bottom - top));
                }
            }
        }
        assert!(red > 25, "{kind:?} has no visible red appearance");
        let band = if kind == MarkupKind::StrikeOut {
            0.4..0.6
        } else {
            0.7..1.
        };
        assert!(
            vertical.iter().all(|y| band.contains(y)),
            "{kind:?} painted at the wrong text height: {vertical:?}"
        );
        assert!(red < 500, "{kind:?} painted a highlight-like fill");
    }
}

fn assert_new_highlight_color(cx: &mut TestAppContext, path: &str, expected_count: usize) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from(path);
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    let document = reader.read_with(cx, |reader, _| reader.document().unwrap());
    let annotations = document.page_annotations(0, 901).unwrap();
    assert_eq!(
        annotations.len(),
        expected_count,
        "preview must include the new highlight"
    );
    let selected = reader.read_with(cx, |reader, cx| {
        reader.selected_page_annotation(cx).unwrap()
    });
    let [left, top, right, bottom] = document.annotation_bounds(&selected).unwrap();
    let bitmap = document.render_bitmap(0, 1.).unwrap();
    let mut yellow = 0;
    for y in top as u32..bottom as u32 {
        for x in left as u32..right as u32 {
            let offset = ((y * bitmap.width + x) * 4) as usize;
            let b = i16::from(bitmap.bgra[offset]);
            let g = i16::from(bitmap.bgra[offset + 1]);
            let r = i16::from(bitmap.bgra[offset + 2]);
            yellow += usize::from(r > b + 30 && g > b + 30 && r > 200 && g > 180);
        }
    }
    assert!(
        yellow > 50,
        "new highlight has no visible yellow pixels in the edited preview: {yellow}"
    );
}
