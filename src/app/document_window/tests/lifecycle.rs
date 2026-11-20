//! Lifecycle evidence uses production windows/sessions and disposable PDF files.
use super::*;
use crate::reader::{ReaderView, ReadingMode};
use gpui_kit::{px, size};

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn active(document: &Entity<DocumentWindow>, cx: &TestAppContext) -> Entity<ReaderView> {
    document.read_with(cx, |d, _| d.reader.clone())
}

fn split(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    document: &Entity<DocumentWindow>,
) -> Entity<ReaderView> {
    cx.simulate_window_resize(handle, size(px(1100.), px(850.)));
    press(cx, handle, "secondary-\\");
    document.read_with(cx, |d, _| assert!(d.other_pane.is_some()));
    active(document, cx)
}

#[gpui_kit::test]
fn dirty_replacement_cancel_and_failed_discard_preserve_both_panes_and_edits(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let original_bytes = std::fs::read(&source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, handle, &document);
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    second.update(cx, |reader, cx| reader.zoom_in(cx));
    let before = [first.clone(), second.clone()].map(|r| r.read_with(cx, |r, _| r.jump_location()));
    for decision in ["unsaved-cancel", "unsaved-discard"] {
        document.update(cx, |d, cx| d.open_path(root.path().join("missing.pdf"), cx));
        cx.run_until_parked();
        click(cx, handle, decision);
        document.read_with(cx, |d, cx| {
            assert_eq!(d.reader, second);
            assert_eq!(d.other_pane.as_ref().unwrap().reader, first);
            assert!(d.reader.read(cx).is_dirty(cx));
            assert!(d.reader.read(cx).has_annotation_undo(cx));
        });
        for (reader, expected) in [first.clone(), second.clone()].into_iter().zip(&before) {
            reader.read_with(cx, |r, _| assert_eq!(&r.jump_location(), expected));
        }
        assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
    }
    document.update(cx, |d, cx| {
        d.open_path("examples/field-guide.pdf".into(), cx)
    });
    cx.run_until_parked();
    click(cx, handle, "unsaved-discard");
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_ne!(d.reader, second);
        assert_eq!(d.reader.read(cx).page_index(), 0);
        assert!(!d.reader.read(cx).is_dirty(cx));
    });
    first.read_with(cx, |r, _| assert!(r.session().is_none()));
    second.read_with(cx, |r, _| assert!(r.session().is_none()));
    assert_eq!(std::fs::read(source).unwrap(), original_bytes);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn save_as_and_undo_remain_shared_across_split_and_other_window(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let destination = root.path().join("copy.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    let (_, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    saving::highlight(cx, handle, &document);
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    press(cx, handle, "secondary-z");
    for reader in [first.clone(), second.clone(), active(&other, cx)] {
        reader.read_with(cx, |r, cx| {
            assert!(!r.is_dirty(cx));
            assert!(r.has_annotation_redo(cx));
        });
    }
    press(cx, handle, REDO_KEY);
    press(cx, handle, "secondary-s");
    assert_eq!(
        crate::pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 99)
            .unwrap()
            .len(),
        1
    );
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(SaveAs), cx)
    })
    .unwrap();
    cx.run_until_parked();
    // A window-owned Save As remains valid when its initiating pane is removed.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ClosePane), cx)
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, _| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader, first);
    });
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    let destination = std::fs::canonicalize(destination).unwrap();
    for reader in [first, active(&other, cx)] {
        reader.read_with(cx, |r, cx| {
            assert_eq!(r.path(), Some(destination.as_path()));
            assert!(!r.is_dirty(cx));
            assert!(r.has_annotation_undo(cx));
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
fn explicit_new_window_preserves_dirty_source_split(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    saving::highlight(cx, handle, &document);
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    cx.update(|cx| {
        windows::open_request(
            cx,
            &document.downgrade(),
            windows::OpenRequest {
                target: Some(windows::OpenTarget::File("examples/field-guide.pdf".into())),
                mode: windows::OpenMode::New,
            },
        )
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    document.read_with(cx, |d, cx| {
        assert_eq!(d.reader, second);
        assert_eq!(d.other_pane.as_ref().unwrap().reader, first);
        assert!(d.reader.read(cx).is_dirty(cx));
        assert!(d.pending_leave.is_none());
    });
}

#[gpui_kit::test]
fn external_reload_resolves_each_split_anchor_and_clamps_removed_pages(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    let (_, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    first.update(cx, |r, cx| {
        r.set_reading_mode(ReadingMode::SinglePage, cx);
        r.select_page(3, cx);
    });
    second.update(cx, |r, cx| {
        r.set_reading_mode(ReadingMode::SinglePage, cx);
        r.select_page(10, cx);
        r.zoom_in(cx);
    });
    cx.run_until_parked();
    let readers = [first, second, active(&other, cx)];
    let before = readers.each_ref().map(|r| {
        r.read_with(cx, |r, _| {
            (r.reading_position(), r.scale(), r.document_generation())
        })
    });
    let session = readers[0].read_with(cx, |r, _| r.session().unwrap());
    let bytes = [
        std::fs::read(&source).unwrap(),
        b"\n% external rewrite\n".to_vec(),
    ]
    .concat();
    std::fs::write(&source, bytes).unwrap();
    session.update(cx, |s, cx| s.check_external(true, false, cx));
    cx.run_until_parked();
    for (reader, (position, scale, generation)) in readers.iter().zip(before) {
        reader.read_with(cx, |r, cx| {
            assert_eq!(r.reading_position(), position);
            assert_eq!(r.scale(), scale);
            assert!(r.document_generation() > generation);
            assert!(!r.is_dirty(cx));
            assert!(r.selected_text().is_none());
        });
    }
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    session.update(cx, |s, cx| s.check_external(true, false, cx));
    cx.run_until_parked();
    for reader in readers {
        reader.read_with(cx, |r, _| {
            assert_eq!(r.page_count(), Some(1));
            assert_eq!(r.reading_position().0, 0);
        });
    }
    document.read_with(cx, |d, _| assert!(d.other_pane.is_some()));
}

#[gpui_kit::test]
fn surviving_pane_is_the_only_reopen_position_even_with_retained_removed_reader(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::keymap::install(cx);
        crate::reading_history::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let source = std::fs::canonicalize("examples/readme-sample.pdf").unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let first = active(&document, cx);
    first.update(cx, |r, cx| r.select_page(8, cx));
    let second = split(cx, handle, &document);
    second.update(cx, |r, cx| r.select_page(20, cx));
    press(cx, handle, "ctrl-tab");
    let expected = first.read_with(cx, |r, _| r.reading_position());
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(KeepOnlyPane), cx)
    })
    .unwrap();
    cx.run_until_parked();
    // Simulate obsolete work retaining its old entity after removal.
    second.update(cx, |r, cx| r.select_page(40, cx));
    cx.run_until_parked();
    let (_, reopened) = open(cx, Some(source));
    cx.run_until_parked();
    reopened.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).reading_position(), expected);
    });
}

#[gpui_kit::test]
fn split_close_defers_to_other_window_and_quit_prompts_once_for_shared_session(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    let (other_handle, other) = open(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    saving::highlight(cx, handle, &document);
    split(cx, handle, &document);
    let session = active(&document, cx).read_with(cx, |r, _| r.session().unwrap());
    assert_eq!(session.read_with(cx, |s, _| s.participant_count()), 3);
    cx.update(windows::request_quit);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all("unsaved-save").len(), 1);
    })
    .unwrap();
    cx.update_window(other_handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("unsaved-save").is_none());
    })
    .unwrap();
    click(cx, handle, "unsaved-cancel");
    // Closing the split leaves another participant's edits alive without asking.
    press(cx, handle, "secondary-w");
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    assert_eq!(session.read_with(cx, |s, _| s.participant_count()), 1);
    assert!(active(&other, cx).read_with(cx, |r, cx| r.is_dirty(cx)));
    press(cx, other_handle, "secondary-w");
    click(cx, other_handle, "unsaved-cancel");
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    // Discard during quit clears this shared owner once, not once per view.
    cx.update(windows::request_quit);
    cx.run_until_parked();
    click(cx, other_handle, "unsaved-discard");
    assert!(!session.read_with(cx, |s, _| s.is_dirty()));
    other.read_with(cx, |d, _| assert!(d.pending_leave.is_none()));
}

#[gpui_kit::test]
fn private_split_reopen_uses_survivor_without_creating_database(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::keymap::install(cx);
        crate::reading_history::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let source = std::fs::canonicalize("examples/readme-sample.pdf").unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let first = active(&document, cx);
    first.update(cx, |r, cx| r.select_page(8, cx));
    let second = split(cx, handle, &document);
    second.update(cx, |r, cx| r.select_page(20, cx));
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ClosePane), cx)
    })
    .unwrap();
    cx.run_until_parked();
    let expected = first.read_with(cx, |r, _| r.reading_position());
    let (_, reopened) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    reopened.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).reading_position(), expected);
    });
    assert!(!paths.local_data().join("reading.sqlite3").exists());
    let clear = cx.update(|cx| crate::reading_history::history(cx).unwrap().clear());
    cx.spawn(async move |_| clear.await.unwrap()).detach();
    cx.run_until_parked();
    // A removed pane's late movement must not resurrect a cleared place.
    second.update(cx, |r, cx| r.select_page(40, cx));
    let (_, forgotten) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    forgotten.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).page_index(), 0);
    });
    // A service restart models a fresh private process: no split or second
    // location is stored on disk.
    cx.update(|cx| crate::reading_history::install(cx, Some(paths.clone())));
    let (_, fresh) = open(cx, Some(source));
    cx.run_until_parked();
    fresh.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).page_index(), 0);
    });
    assert!(!paths.local_data().join("reading.sqlite3").exists());
}

#[gpui_kit::test]
fn linked_pdf_new_window_leaves_source_split_and_locations_untouched(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    std::fs::write(&source, crate::pdf::linked_fixture()).unwrap();
    let target = root.path().join("other.pdf");
    std::fs::copy("examples/readme-sample.pdf", &target).unwrap();
    let (handle, document) = open(cx, Some(source));
    cx.run_until_parked();
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    second.update(cx, |r, cx| r.zoom_in(cx));
    let before = [first.clone(), second.clone()].map(|r| r.read_with(cx, |r, _| r.jump_location()));
    // Use the production linked-document event seam after PDF link decoding.
    document.update(cx, |d, cx| {
        d.open_linked_document(
            d.reader.read(cx).document_generation(),
            target.clone(),
            5,
            handle,
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    document.read_with(cx, |d, _| {
        assert_eq!(d.reader, second);
        assert_eq!(d.other_pane.as_ref().unwrap().reader, first);
    });
    for (reader, expected) in [first, second].into_iter().zip(before) {
        reader.read_with(cx, |r, _| assert_eq!(r.jump_location(), expected));
    }
    let destination = cx.update(|cx| *cx.windows().last().unwrap());
    cx.update_window(destination, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("reader-status")
                .label()
                .unwrap()
                .starts_with("Page 6 of ")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn successful_replacement_uses_one_ordinary_restored_position(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let target = std::fs::canonicalize("examples/readme-sample.pdf").unwrap();
    let store = crate::reading_history::RecentStore::new(&paths);
    store.record(&target).unwrap();
    store
        .save_position(
            &target,
            crate::reading_history::ReadingPosition { page: 18, y: 0. },
        )
        .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::keymap::install(cx);
        crate::reading_history::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (handle, document) = open(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    split(cx, handle, &document);
    // Native-picker completion enters the same ordinary file-opening policy.
    document.update(cx, |d, cx| {
        d.finish_file_choice(
            d.reader.read(cx).document_generation(),
            Some(target.clone()),
            cx,
        );
    });
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(d.reader.read(cx).path(), Some(target.as_path()));
        assert_eq!(d.reader.read(cx).reading_position().0, 18);
        assert_eq!(d.reader.read(cx).scale(), 1.);
    });
}

#[gpui_kit::test]
fn newer_shared_revision_invalidates_discard_approval_before_staged_adoption(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, document) = open(cx, Some(source));
    cx.run_until_parked();
    saving::highlight(cx, handle, &document);
    let first = active(&document, cx);
    let second = split(cx, handle, &document);
    document.update(cx, |d, cx| {
        d.open_path("examples/field-guide.pdf".into(), cx)
    });
    cx.run_until_parked();
    let _subscription = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("unsaved-discard", cx);
            document.update(cx, |d, cx| {
                let pending = d.replacement.as_ref().unwrap().reader.clone();
                cx.subscribe(&pending, |d, _, event, cx| {
                    if matches!(event, DocumentEvent::Opened { result: Ok(()), .. }) {
                        // This is the real loaded-but-not-adopted interval. Undo
                        // and redo preserve the edit but advance its shared revision.
                        let session = d.reader.read(cx).session().unwrap();
                        session.update(cx, |session, cx| {
                            session.undo_annotation(cx);
                            session.redo_annotation(cx);
                        });
                    }
                })
            })
        })
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert_eq!(d.reader, second);
        assert_eq!(d.other_pane.as_ref().unwrap().reader, first);
        assert!(d.replacement.is_none());
        assert!(d.reader.read(cx).is_dirty(cx));
        assert!(d.reader.read(cx).has_annotation_undo(cx));
    });
}

#[gpui_kit::test]
fn picker_started_during_staging_cannot_save_or_block_the_replacement(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("obsolete.pdf");
    let (handle, document) = open(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    split(cx, handle, &document);
    cx.update_window(handle, |_, window, cx| {
        document.update(cx, |d, cx| {
            d.open_path("examples/readme-sample.pdf".into(), cx)
        });
        window.dispatch_action(Box::new(SaveAs), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    document.read_with(cx, |d, _| {
        assert!(d.other_pane.is_none());
        assert!(!d.save_prompt_open);
    });
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    assert!(!destination.exists());
    document.read_with(cx, |d, cx| {
        assert_eq!(
            d.reader.read(cx).path(),
            Some(std::path::Path::new("examples/readme-sample.pdf"))
        );
        assert!(d.pending_leave.is_none());
        assert!(!d.save_prompt_open);
    });
}
