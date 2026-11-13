use super::saving::{highlight, select_text};
use super::*;

fn indicator(cx: &mut TestAppContext, handle: AnyWindowHandle, filename: &str, unsaved: bool) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("document-name").label(), Some(filename));
        if unsaved {
            let status = window.find("document-unsaved");
            assert!(status.visible());
            assert_eq!(status.label(), Some("Unsaved changes"));
        } else {
            assert!(window.try_find("document-unsaved").is_none());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn unsaved_title_follows_save_and_save_as_across_participants(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.pdf");
    let copy = directory.path().join("annotated.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, _) = open(cx, Some(source.clone()));
    let (unrelated, _) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    for window in [first, second] {
        indicator(cx, window, "original.pdf", false);
    }
    // Selection, zoom and navigation are not document edits.
    select_text(cx, first, &document);
    cx.update_window(first, |_, window, cx| window.press("cmd-=", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).scale() > 1.)
    });
    indicator(cx, first, "original.pdf", false);
    cx.update_window(first, |_, window, cx| window.press("cmd--", cx))
        .unwrap();
    cx.run_until_parked();
    highlight(cx, first, &document);
    for window in [first, second] {
        indicator(cx, window, "original.pdf", true);
    }
    indicator(cx, unrelated, "field-guide.pdf", false);
    // A newly opened participant must also present the current shared state.
    let (joined, _) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    indicator(cx, joined, "original.pdf", true);
    cx.update_window(second, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
    for window in [first, second, joined] {
        indicator(cx, window, "original.pdf", false);
    }
    let saved = std::fs::read(&source).unwrap();
    // Save As is valid even for a clean PDF and updates every participant's name.
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(copy.clone()));
    cx.run_until_parked();
    for window in [first, second, joined] {
        indicator(cx, window, "annotated.pdf", false);
    }
    assert_eq!(std::fs::read(source).unwrap(), saved);
    assert!(copy.exists());
    indicator(cx, unrelated, "field-guide.pdf", false);
}

#[gpui_kit::test]
fn unsaved_title_survives_cancel_failure_and_document_replacement(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    indicator(cx, handle, "original.pdf", true);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    // A directory cannot be committed as a PDF destination.
    cx.simulate_new_path_selection(|_| Some(directory.path().to_path_buf()));
    cx.run_until_parked();
    indicator(cx, handle, "original.pdf", true);
    // An external change produces a real current-file save failure.
    let external = [
        std::fs::read(&source).unwrap(),
        b"\n% external edit\n".to_vec(),
    ]
    .concat();
    std::fs::write(&source, &external).unwrap();
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    indicator(cx, handle, "original.pdf", true);
    assert_eq!(std::fs::read(&source).unwrap(), external);
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    cx.run_until_parked();
    let generation = document.read_with(cx, |document, cx| {
        document.reader.read(cx).document_generation()
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(
            generation,
            Some(PathBuf::from("examples/field-guide.pdf")),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    indicator(cx, handle, "original.pdf", true);
    document.update(cx, |document, cx| {
        document.finish_file_choice(
            generation,
            Some(PathBuf::from("examples/field-guide.pdf")),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-discard", cx))
        .unwrap();
    cx.run_until_parked();
    indicator(cx, handle, "field-guide.pdf", false);
}

fn late_edits(cx: &mut TestAppContext, save_as: bool) {
    use crate::{
        coordinates::Point, pdf::annotations::Quad, reader::document_session::Destination,
    };
    use std::sync::Arc;
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.pdf");
    let copy = directory.path().join("copy.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, _) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    highlight(cx, first, &document);
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    // Keep completion queued while inspecting the production windows. Inject
    // only the late mutation at the shared owner, not a second dirty flag or
    // fake save result; the actual writer still commits the older snapshot.
    cx.update(|cx| {
        session.update(cx, |session, cx| {
            if save_as {
                session
                    .start_save_as(Arc::new(Destination::inspect(copy.clone()).unwrap()), cx)
                    .unwrap();
            } else {
                session.start_save(cx).unwrap();
            }
            session
                .create_highlight(
                    0,
                    vec![
                        Quad::new([
                            Point { x: 48., y: 714. },
                            Point { x: 170., y: 714. },
                            Point { x: 48., y: 700. },
                            Point { x: 170., y: 700. },
                        ])
                        .unwrap(),
                    ],
                    cx,
                )
                .unwrap();
        });
        for handle in [first, second] {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.find("document-unsaved").visible());
                assert!(
                    window
                        .find("reader-status")
                        .label()
                        .unwrap()
                        .contains("Saving")
                );
            })
            .unwrap();
        }
    });
    cx.run_until_parked();
    let saved_path = if save_as { &copy } else { &source };
    let filename = saved_path.file_name().unwrap().to_str().unwrap();
    for window in [first, second] {
        indicator(cx, window, filename, true);
    }
    assert_eq!(
        crate::pdf::Document::open(saved_path)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    cx.update_window(second, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    for window in [first, second] {
        indicator(cx, window, filename, false);
    }
    assert_eq!(
        crate::pdf::Document::open(saved_path)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        2
    );
}

#[gpui_kit::test]
fn unsaved_title_keeps_late_edits_visible_after_save_as(cx: &mut TestAppContext) {
    late_edits(cx, true);
}

#[gpui_kit::test]
fn unsaved_title_keeps_late_edits_visible_after_current_save(cx: &mut TestAppContext) {
    late_edits(cx, false);
}

#[gpui_kit::test]
fn unsaved_title_detaches_from_a_replaced_shared_document(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("shared.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (first, document) = open(cx, Some(source.clone()));
    let (second, _) = open(cx, Some(source));
    cx.run_until_parked();
    highlight(cx, first, &document);
    let generation = document.read_with(cx, |document, cx| {
        document.reader.read(cx).document_generation()
    });
    // Replacing one participant does not discard the other participant's edits.
    document.update(cx, |document, cx| {
        document.finish_file_choice(
            generation,
            Some(PathBuf::from("examples/field-guide.pdf")),
            cx,
        )
    });
    cx.run_until_parked();
    indicator(cx, first, "field-guide.pdf", false);
    indicator(cx, second, "shared.pdf", true);
    cx.update_window(second, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    indicator(cx, first, "field-guide.pdf", false);
    indicator(cx, second, "shared.pdf", false);
}

#[gpui_kit::test]
fn unsaved_title_reserves_status_space_at_narrow_widths_and_larger_text(cx: &mut TestAppContext) {
    use gpui_kit::{component::theme::Theme, px, size};
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let name = format!("{}.pdf", "long-filename-".repeat(12));
    let path = directory.path().join(&name);
    std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
    let (handle, document) = open(cx, Some(path));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    for font_size in [12., 16., 20.] {
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        for width in [360., 600., 920., 1200.] {
            cx.simulate_window_resize(handle, size(px(width), px(800.)));
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let name = window.find("document-name").bounds();
                let status = window.find("document-unsaved").bounds();
                let open = window.find("open-pdf").bounds();
                let title = window.find("document-title").bounds();
                assert!(name.size.width > px(0.), "width={width}, font={font_size}");
                assert!(status.left() > name.right());
                assert_eq!(name.center().y, status.center().y);
                assert!(status.right() < open.left());
                assert!(status.size.width > px(0.));
                assert!(status.top() >= px(0.));
                assert!(status.bottom() <= window.find("title-bar").bounds().bottom());
                assert!(((name.left() + status.right()) / 2. - title.center().x).abs() <= px(0.5));
            })
            .unwrap();
        }
    }
}
