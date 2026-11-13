use super::*;
use gpui_kit::px;

pub(super) fn select_text(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    document: &Entity<DocumentWindow>,
) {
    let path = document.read_with(cx, |document, cx| {
        document.reader.read(cx).path().unwrap().to_path_buf()
    });
    let text = crate::pdf::page_text(&path, 0).unwrap();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let page = window.find("pdf-page-0").bounds();
        let at = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let point = text.transform.document_to_page(crate::coordinates::Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            page.origin + gpui_kit::point(px(point.x as f32), px(point.y as f32))
        };
        window.drag(at(0, 0.1), at(4, 0.8), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

pub(super) fn highlight(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    document: &Entity<DocumentWindow>,
) {
    select_text(cx, handle, document);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).is_dirty(cx))
    });
}

#[gpui_kit::test]
fn replacement_cancel_and_failed_save_stop_the_continuation_then_discard_replaces(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("readonly-source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let mut permissions = std::fs::metadata(&source).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&source, permissions).unwrap();
    let next = PathBuf::from("examples/field-guide.pdf");
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    let generation = document.read_with(cx, |document, cx| {
        document.reader.read(cx).document_generation()
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(generation, Some(next.clone()), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).path(), Some(source.as_path()));
        assert!(document.reader.read(cx).is_dirty(cx));
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(generation, Some(next.clone()), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-save", cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(source.clone()));
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).path(), Some(source.as_path()));
        assert!(document.reader.read(cx).is_dirty(cx));
        assert!(document.pending_leave.is_none());
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(generation, Some(next.clone()), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-discard", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader.read(cx).path(), Some(next.as_path()));
        assert!(!document.reader.read(cx).is_dirty(cx));
    });
}

#[gpui_kit::test]
fn quit_command_protects_dirty_sessions_even_when_another_window_is_active(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(menu::install);
    let (dirty_window, dirty_document) =
        open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    let (clean_window, _) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    highlight(cx, dirty_window, &dirty_document);
    cx.update_window(clean_window, |_, window, cx| {
        window.activate_window();
        window.dispatch_action(Box::new(menu::Quit), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(dirty_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("unsaved-save").visible());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    dirty_document.read_with(cx, |document, cx| {
        assert!(document.reader.read(cx).is_dirty(cx));
        assert!(document.pending_leave.is_none());
    });
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn ordinary_save_keeps_the_active_find_query_results_and_focus(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("find-save.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    cx.update_window(handle, |_, window, cx| window.press("cmd-f", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.input("Alpha", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| assert!(document.has_matches(cx)));
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    let focus = document.read_with(cx, |document, cx| {
        assert!(!document.reader.read(cx).is_dirty(cx));
        assert!(
            document.has_matches(cx),
            "Save must not discard live Find results"
        );
        let find = document.sidebar.as_ref().unwrap().read(cx).find().read(cx);
        find.query_focus(cx)
    });
    cx.update_window(handle, |_, window, _| assert!(focus.is_focused(window)))
        .unwrap();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn save_in_an_existing_quit_dialog_waits_for_another_windows_current_save(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("shared-save.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    let (_, _other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    cx.update(windows::request_quit);
    cx.run_until_parked();
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    cx.update(|cx| {
        session.update(cx, |session, cx| session.start_save(cx).unwrap());
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("unsaved-discard").disabled(), Some(true));
            window.press("enter", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(!document.reader.read(cx).is_dirty(cx));
        assert!(
            document.pending_leave.is_none(),
            "a completed save must not leave a hidden quit continuation"
        );
    });
    assert!(!cx.did_prompt_for_new_path());
    assert_eq!(
        crate::pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
}

fn quit_from_focused_dirty_window(cx: &mut TestAppContext, keyboard: bool) {
    setup(cx);
    cx.update(menu::install);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    cx.update_window(handle, |_, window, _| window.activate_window())
        .unwrap();
    for _ in 0..2 {
        if keyboard {
            cx.update_window(handle, |_, window, cx| window.press("cmd-q", cx))
                .unwrap();
        } else {
            // Native menu actions enter App::dispatch_action and route through
            // the active window, not directly to the native quit adapter.
            cx.update(|cx| cx.dispatch_action(&menu::Quit));
        }
        cx.run_until_parked();
        document.read_with(cx, |document, _| {
            assert!(
                matches!(document.pending_leave, Some(Leave::Quit)),
                "Quit did not open a decision for its own focused dirty window"
            );
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("unsaved-save").visible());
            window.click("unsaved-cancel", cx);
        })
        .unwrap();
        cx.run_until_parked();
        document.read_with(cx, |document, cx| {
            assert!(document.reader.read(cx).is_dirty(cx));
            assert!(document.pending_leave.is_none());
        });
    }
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn command_q_protects_its_focused_dirty_window(cx: &mut TestAppContext) {
    quit_from_focused_dirty_window(cx, true);
}

#[gpui_kit::test]
fn quit_menu_protects_its_focused_dirty_window(cx: &mut TestAppContext) {
    quit_from_focused_dirty_window(cx, false);
}

#[gpui_kit::test]
fn native_quit_during_save_prompts_reuses_the_flow_and_cancellation_rearms_quit(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(menu::install);
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("existing.pdf");
    std::fs::write(&destination, b"existing").unwrap();
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    for overwrite in [false, true] {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(SaveAs), cx);
        })
        .unwrap();
        cx.run_until_parked();
        if overwrite {
            cx.simulate_new_path_selection(|_| Some(destination.clone()));
            cx.run_until_parked();
        }
        // Native termination reaches this owner independently of modal focus.
        cx.update(windows::request_quit);
        cx.run_until_parked();
        document.read_with(cx, |document, _| {
            assert!(
                matches!(document.pending_leave, Some(Leave::Quit)),
                "overwrite={overwrite}"
            );
            assert!(document.save_prompt_open);
        });
        if overwrite {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("unsaved-save").is_none());
                assert!(window.find("overwrite-confirm").visible());
                window.press("escape", cx);
            })
            .unwrap();
        } else {
            cx.simulate_new_path_selection(|_| None);
        }
        cx.run_until_parked();
        document.read_with(cx, |document, cx| {
            assert!(document.pending_leave.is_none());
            assert!(!document.save_prompt_open);
            assert!(document.reader.read(cx).is_dirty(cx));
        });
        cx.update(windows::request_quit);
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("unsaved-save").visible());
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    assert_eq!(std::fs::read(destination).unwrap(), b"existing");
}

#[gpui_kit::test]
fn quit_waits_for_an_existing_background_save_and_save_failure_cancels_it(cx: &mut TestAppContext) {
    use crate::reader::document_session::Destination;
    use std::sync::Arc;
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("removed");
    std::fs::create_dir(&parent).unwrap();
    let refused = Arc::new(Destination::inspect(parent.join("copy.pdf")).unwrap());
    std::fs::remove_dir(parent).unwrap();
    let successful = directory.path().join("successful.pdf");
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    highlight(cx, handle, &document);
    let session = document.read_with(cx, |document, cx| {
        document.reader.read(cx).session().unwrap()
    });
    for (destination, succeeds) in [
        (refused, false),
        (
            Arc::new(Destination::inspect(successful.clone()).unwrap()),
            true,
        ),
    ] {
        cx.update(|cx| {
            session.update(cx, |session, cx| {
                session.start_save_as(destination, cx).unwrap()
            });
            windows::request_quit(cx);
        });
        document.read_with(cx, |document, _| {
            assert!(matches!(document.pending_leave, Some(Leave::Quit)))
        });
        cx.run_until_parked();
        document.read_with(cx, |document, cx| {
            assert!(document.pending_leave.is_none());
            assert_eq!(document.reader.read(cx).is_dirty(cx), !succeeds);
            assert!(!document.reader.read(cx).is_saving(cx));
        });
    }
    assert_eq!(
        crate::pdf::Document::open(&successful)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
}

#[gpui_kit::test]
fn signed_pdf_disables_highlight_without_disabling_selection(cx: &mut TestAppContext) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("signed.pdf");
    let mut pdf = lopdf::Document::load("examples/selection-sample.pdf").unwrap();
    pdf.add_object(lopdf::dictionary! { "Type" => "Sig", "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()] });
    pdf.save(&source).unwrap();
    let (handle, document) = open(cx, Some(source));
    cx.run_until_parked();
    select_text(cx, handle, &document);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("highlight-selection").disabled(), Some(true));
        window.press("cmd-shift-h", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        let session = reader.session().unwrap();
        assert!(
            session
                .read(cx)
                .editing_rejection()
                .unwrap()
                .contains("signed")
        );
        assert!(!reader.save_available(cx));
        assert!(!reader.is_dirty(cx));
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
    });
}

#[gpui_kit::test]
fn readable_encrypted_pdf_remains_noneditable_even_with_an_empty_user_password(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("encrypted.pdf");
    let mut pdf = lopdf::Document::load("examples/selection-sample.pdf").unwrap();
    pdf.trailer.set(
        "ID",
        vec![
            lopdf::Object::string_literal(b"0123456789abcdef"),
            lopdf::Object::string_literal(b"fedcba9876543210"),
        ],
    );
    let encryption = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
        document: &pdf,
        owner_password: "fixture-owner",
        user_password: "",
        key_length: 128,
        permissions: lopdf::Permissions::all(),
    })
    .unwrap();
    pdf.encrypt(&encryption).unwrap();
    pdf.save(&source).unwrap();
    let (handle, document) = open(cx, Some(source));
    cx.run_until_parked();
    select_text(cx, handle, &document);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("highlight-selection").disabled(), Some(true));
        window.press("cmd-shift-h", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!(reader.page_count(), Some(1));
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
        assert!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .editing_rejection()
                .unwrap()
                .contains("encrypted")
        );
        assert!(!reader.save_available(cx));
        assert!(!reader.is_dirty(cx));
    });
}
