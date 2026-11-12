//! Production pointer/action/file-picker workflow, with real persisted PDFs.
use super::*;
use crate::app::document_window::SaveAs;

pub(super) fn draw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn select_title(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    reader: &Entity<ReaderView>,
    path: &Path,
) {
    let text = pdf::page_text(path, 0).unwrap();
    draw(cx, window);
    let (start, end) = reader.read_with(cx, |reader, _| {
        let viewport = reader.page_viewport(0).unwrap();
        let screen = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let screen = viewport.document_to_screen(Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            point(px(screen.x as f32), px(screen.y as f32))
        };
        (screen(0, 0.1), screen(4, 0.8))
    });
    cx.update_window(window, |_, frame, cx| frame.drag(start, end, cx))
        .unwrap();
}

#[gpui_kit::test]
fn selected_text_highlight_save_as_and_fresh_reopen_share_one_session(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let output = directory.path().join("highlighted.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    let (first, reader) = recent_test_window(cx, Some(source.clone()));
    let (second, other) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    draw(cx, first);
    draw(cx, second);
    select_title(cx, first, &reader, &source);
    let selected = reader.read_with(cx, |reader, _| reader.selected_text().unwrap());
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        assert_ne!(window.find("highlight-selection").disabled(), Some(true));
        assert!(
            window.find("highlight-selection").bounds().right() <= window.bounds().size.width,
            "Highlight is clipped outside its window: {:?}",
            window.find("highlight-selection").bounds()
        );
        window.click("highlight-selection", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let quads = reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.selected_text().as_deref(), Some(selected.as_str()));
        assert!(reader.is_dirty(cx));
        let session = reader.session().unwrap();
        let catalog = session.read(cx).annotations().unwrap().unwrap();
        assert_eq!(catalog.len(), 1);
        catalog[0].quads().to_vec()
    });
    other.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx));
        assert!(reader.selected_text().is_none());
        assert_eq!(reader.page_index(), 0);
    });
    cx.update_window(first, |_, window, cx| {
        window.click("highlight-selection", cx);
        window.dispatch_action(Box::new(SaveAs), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    let output = std::fs::canonicalize(output).unwrap();
    for reader in [&reader, &other] {
        reader.read_with(cx, |reader, cx| {
            assert_eq!(reader.path(), Some(output.as_path()));
            assert!(!reader.is_dirty(cx));
            assert!(!reader.is_saving(cx));
            let session = reader.session().unwrap();
            assert_eq!(session.read(cx).annotations().unwrap().unwrap().len(), 1);
            assert_eq!(
                session.read(cx).unsaved_annotations().count(),
                0,
                "saved AP must not also have an overlay"
            );
        });
    }
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let saved = std::fs::read(&output).unwrap();
    assert!(saved.starts_with(&original));
    // Parsing actual saved bytes is required, not just reopening a session alias.
    let document = pdf::Document::open(&output).unwrap();
    let annotations = document.page_annotations(0, 999).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].quads().len(), quads.len());
    for (saved, selected) in annotations[0].quads().iter().zip(&quads) {
        for (saved, selected) in saved.points().into_iter().zip(selected.points()) {
            assert!((saved.x - selected.x).abs() < 0.001);
            assert!((saved.y - selected.y).abs() < 0.001);
        }
    }
    assert_eq!(
        annotations[0].kind(),
        pdf::annotations::MarkupKind::Highlight
    );
    for handle in [first, second] {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx)
        })
        .unwrap();
        cx.run_until_parked();
    }
    let (reopened, reopened_reader) = recent_test_window(cx, Some(output.clone()));
    cx.run_until_parked();
    draw(cx, reopened);
    reopened_reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.path(), Some(output.as_path()));
        assert!(!reader.is_dirty(cx));
        assert_eq!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            1
        );
    });
}

#[gpui_kit::test]
fn save_picker_cancel_and_last_participant_cancel_retain_selection_and_edits(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (window, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, window, &reader, &path);
    cx.update_window(window, |_, window, cx| {
        window.click("highlight-selection", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert!(
            reader.is_dirty(cx),
            "Highlight did not create an edit: {}",
            reader.status()
        );
    });
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("unsaved-save").visible());
        window.click("unsaved-cancel", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx));
        assert!(reader.selected_text().is_some());
    });
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("unsaved-discard", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.windows().is_empty()));
}

#[gpui_kit::test]
fn closing_one_participant_keeps_shared_edits_and_the_last_close_can_save_elsewhere(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("readonly.pdf");
    let output = directory.path().join("saved-on-close.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o444)).unwrap();
    }
    let (first, reader) = recent_test_window(cx, Some(source.clone()));
    let (second, other) = recent_test_window(cx, Some(source.clone()));
    let (unrelated, unrelated_reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    select_title(cx, first, &reader, &source);
    cx.update_window(first, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    other.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx));
        assert!(reader.selected_text().is_none());
        assert_eq!(reader.session().unwrap().read(cx).participant_count(), 1);
    });
    unrelated_reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
    draw(cx, unrelated);
    draw(cx, second);
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| window.click("unsaved-save", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        2,
        "picker cancellation must cancel closing"
    );
    other.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| window.click("unsaved-save", cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert_eq!(
        pdf::Document::open(&output)
            .unwrap()
            .page_annotations(0, 10)
            .unwrap()
            .len(),
        1
    );
    unrelated_reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
}

#[gpui_kit::test]
fn overwrite_cancellation_retains_edits_and_confirmation_saves_the_pdf(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    let output = directory.path().join("existing.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    std::fs::write(&output, b"existing file").unwrap();
    let (handle, reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &source);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    for confirm in [false, true] {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(SaveAs), cx)
        })
        .unwrap();
        cx.run_until_parked();
        cx.simulate_new_path_selection(|_| Some(output.clone()));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("overwrite-confirm").visible());
            if confirm {
                window.press("enter", cx);
            } else {
                window.click("overwrite-cancel", cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        reader.read_with(cx, |reader, cx| assert_eq!(reader.is_dirty(cx), !confirm));
        if !confirm {
            assert_eq!(std::fs::read(&output).unwrap(), b"existing file");
        }
    }
    assert_eq!(
        pdf::Document::open(&output)
            .unwrap()
            .page_annotations(0, 10)
            .unwrap()
            .len(),
        1
    );
}

#[gpui_kit::test]
fn selection_context_menu_and_palette_reuse_highlight_without_clearing_text(
    cx: &mut TestAppContext,
) {
    use gpui_kit::test::TestEventExt as _;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/selection-sample.pdf");
    let (handle, reader) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &path);
    let text = reader.read_with(cx, |reader, _| reader.selected_text().unwrap());
    cx.update_window(handle, |_, window, cx| window.right_click("page", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let target = window
            .within("popup-menu")
            .find_by_label("Highlight")
            .bounds()
            .center();
        window.pointer_down(target, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(target, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.selected_text().as_deref(), Some(text.as_str()));
        assert!(reader.is_dirty(cx));
        assert_eq!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            1
        );
    });
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-p", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.input("Highlight", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.selected_text().as_deref(), Some(text.as_str()));
        assert_eq!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            1
        );
    });
}
