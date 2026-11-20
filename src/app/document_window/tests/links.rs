use super::*;
use crate::pdf;
use gpui_kit::{Modifiers, px, size};

fn hint_window(
    cx: &mut TestAppContext,
    annotation: u32,
) -> (tempfile::TempDir, AnyWindowHandle, Entity<DocumentWindow>) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let mut pdf = lopdf::Document::load_mem(&pdf::linked_fixture()).unwrap();
    pdf.get_object_mut((3, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(
            "Annots",
            lopdf::Object::Array(vec![lopdf::Object::Reference((annotation, 0))]),
        );
    pdf.save(&source).unwrap();
    std::fs::copy("examples/readme-sample.pdf", root.path().join("other.pdf")).unwrap();
    setup(cx);
    let (window, document) = open(cx, Some(source));
    settle(cx, window);
    (root, window, document)
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    }
}

fn choose_link(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, frame, cx| {
        for stroke in key.split_whitespace() {
            frame.press(stroke, cx);
        }
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        assert!(frame.find("hint-aa").visible());
        assert_eq!(frame.find("hint-input").label(), Some("Choose link: __"));
        frame.press("a", cx);
        frame.press("a", cx);
        assert!(frame.try_find("hint-input").is_none());
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn link_hints_follow_textless_rotated_pdf_and_record_history(cx: &mut TestAppContext) {
    let (_root, window, document) = hint_window(cx, 5);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    choose_link(cx, window, "f");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.scale(), 1.5);
        assert!((reader.reading_position().1 - 500.).abs() < 1.);
    });
    document.read_with(cx, |document, _| assert!(document.jumplist.can_back()));
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(NavigateBack), cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
}

#[gpui_kit::test]
fn link_hints_create_a_split_and_modified_click_reuses_the_other_reader(cx: &mut TestAppContext) {
    let (_root, window, document) = hint_window(cx, 5);
    cx.update(|cx| crate::app::keymap::reload(cx, None, crate::app::keymap::Preset::Vim).unwrap());
    cx.simulate_window_resize(window, size(px(1200.), px(1000.)));
    settle(cx, window);
    let first = document.read_with(cx, |document, _| document.reader().clone());

    choose_link(cx, window, "g w f");
    settle(cx, window);
    let second = document.read_with(cx, |document, _| {
        assert_ne!(document.reader(), &first);
        assert_eq!(document.other_pane.as_ref().unwrap().reader, first);
        document.reader().clone()
    });
    first.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.scale(), 1.5);
        assert!((reader.reading_position().1 - 500.).abs() < 1.);
    });
    cx.update_window(window, |_, frame, cx| {
        assert!(second.read(cx).focus_handle(cx).is_focused(frame));
        let left = frame.find(("reading-pane", first.entity_id())).bounds();
        let right = frame.find(("reading-pane", second.entity_id())).bounds();
        assert!(left.right() <= right.left());
        frame.dispatch_action(Box::new(SplitStacked), cx);
    })
    .unwrap();
    second.update(cx, |reader, cx| {
        reader.select_page(0, cx);
        reader.fit_width(cx);
    });
    settle(cx, window);
    let second_bounds = cx
        .update_window(window, |_, frame, cx| {
            let top = frame.find(("reading-pane", first.entity_id())).bounds();
            let bottom = frame.find(("reading-pane", second.entity_id())).bounds();
            assert!(top.bottom() <= bottom.top());
            frame
                .within(("reading-pane", second.entity_id()))
                .click_with_modifiers("pdf-link-0-0", Modifiers::secondary_key(cx), cx);
            bottom
        })
        .unwrap();
    settle(cx, window);
    document.read_with(cx, |document, _| {
        assert_eq!(document.reader(), &first);
        assert_eq!(document.other_pane.as_ref().unwrap().reader, second);
    });
    assert_eq!(first.read_with(cx, |reader, _| reader.page_index()), 1);
    assert_eq!(second.read_with(cx, |reader, _| reader.page_index()), 0);
    cx.update_window(window, |_, frame, cx| {
        assert!(first.read(cx).focus_handle(cx).is_focused(frame));
        assert_eq!(
            frame.find(("reading-pane", second.entity_id())).bounds(),
            second_bounds
        );
    })
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn link_hints_open_same_and_other_documents_in_new_windows(cx: &mut TestAppContext) {
    for (annotation, key) in [(5, "shift-f"), (17, "f"), (17, "shift-f")] {
        let (_root, window, document) = hint_window(cx, annotation);
        let count = cx.update(|cx| cx.windows().len());
        let reader = document.read_with(cx, |document, _| document.reader().clone());
        choose_link(cx, window, key);
        assert_eq!(cx.update(|cx| cx.windows().len()), count + 1);
        assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
        let destination_window = cx.update(|cx| *cx.windows().last().unwrap());
        settle(cx, destination_window);
        cx.update_window(destination_window, |_, frame, _| {
            if annotation == 5 {
                assert_eq!(frame.find("reader-zoom").label(), Some("150%"));
            }
            assert!(
                frame
                    .find("reader-status")
                    .label()
                    .unwrap()
                    .starts_with("Page 2 of ")
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn web_links_open_browser_from_both_hint_commands_and_pointer(cx: &mut TestAppContext) {
    let (_root, window, _document) = hint_window(cx, 11);
    for key in ["f", "shift-f"] {
        choose_link(cx, window, key);
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.org/"));
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    }
    cx.update_window(window, |_, frame, cx| {
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.org/"));
}

#[gpui_kit::test]
fn link_hints_cancel_and_empty_pages_do_not_navigate(cx: &mut TestAppContext) {
    let (_root, window, document) = hint_window(cx, 5);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.update_window(window, |_, frame, cx| frame.press("f", cx))
        .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        frame.press("z", cx);
        frame.press("z", cx);
        assert_eq!(
            frame.find("hint-input").label(),
            Some("No matching hint. Try again.")
        );
        frame.press("escape", cx);
        assert!(frame.try_find("hint-input").is_none());
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| frame.press("f", cx))
        .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        assert_eq!(
            frame.find("hint-input").label(),
            Some("No visible links to follow")
        );
        frame.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn link_hints_invalidate_on_view_changes_and_document_replacement(cx: &mut TestAppContext) {
    let (_root, window, document) = hint_window(cx, 5);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    for change in 0..4 {
        cx.update_window(window, |_, frame, cx| frame.press("f", cx))
            .unwrap();
        // Also exercise cancellation before background hint preparation completes.
        if change != 3 {
            settle(cx, window);
        }
        match change {
            0 => reader.update(cx, |reader, cx| reader.scroll_by(10., cx)),
            1 => reader.update(cx, |reader, cx| reader.zoom_in(cx)),
            2 => cx.simulate_window_resize(
                window,
                gpui_kit::size(gpui_kit::px(720.), gpui_kit::px(500.)),
            ),
            _ => document.update(cx, |document, cx| {
                document.open_path("examples/readme-sample.pdf".into(), cx)
            }),
        }
        settle(cx, window);
        cx.update_window(window, |_, frame, _| {
            assert!(frame.try_find("hint-input").is_none());
            assert!(frame.try_find("hint-aa").is_none());
        })
        .unwrap();
    }
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn follow_link_palette_command_takes_focus_from_find_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (_root, window, document) = hint_window(cx, 5);
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(ShowFind), cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| {
        assert_eq!(frame.find("find-query").focused(), Some(true));
        frame.press("f", cx);
        assert!(frame.try_find("hint-input").is_none());
        frame.press("secondary-shift-p", cx);
        frame.input("Follow link in new window", cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    settle(cx, window);
    assert!(!document.read_with(cx, |document, _| document.palette_open()));
    cx.update_window(window, |_, frame, cx| {
        assert!(frame.find("hint-aa").visible());
        frame.press("escape", cx);
        assert!(frame.try_find("hint-input").is_none());
        assert!(
            document
                .read(cx)
                .reader()
                .read(cx)
                .focus_handle(cx)
                .is_focused(frame)
        );
    })
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

fn linked_window(
    cx: &mut TestAppContext,
    with_target: bool,
) -> (
    tempfile::TempDir,
    AnyWindowHandle,
    Entity<DocumentWindow>,
    PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let target = root.path().join("other.pdf");
    std::fs::write(&source, pdf::linked_fixture()).unwrap();
    if with_target {
        std::fs::copy("examples/readme-sample.pdf", &target).unwrap();
    }
    setup(cx);
    let (window, document) = open(cx, Some(source));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    (root, window, document, target)
}

#[gpui_kit::test]
fn another_pdf_link_opens_independent_window_at_target_page(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(
            frame.find("pdf-link-0-9").label(),
            Some("Open other.pdf in a new window")
        );
        frame.click("pdf-link-0-9", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.document_generation(), generation);
        assert!(reader.path().unwrap().ends_with("source.pdf"));
    });
    let new_window = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != source_window)
            .unwrap()
    });
    cx.update_window(new_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("reader-status").label(), Some("Page 2 of 100"));
        assert!(frame.find("document-window").visible());
    })
    .unwrap();
    assert!(target.exists());
}

#[gpui_kit::test]
fn missing_link_target_reports_error_without_opening_window(cx: &mut TestAppContext) {
    let (_root, source_window, document, _) = linked_window(cx, false);
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-9", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    document.read_with(cx, |document, cx| {
        let reader = document.reader().read(cx);
        assert_eq!(reader.page_index(), 0);
        assert!(
            reader.status().contains("Cannot open linked PDF"),
            "{}",
            reader.status()
        );
        assert!(reader.status().contains("other.pdf"), "{}", reader.status());
    });
}

#[gpui_kit::test]
fn obsolete_link_completion_cannot_open_a_window_or_change_replacement(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    document.update(cx, |document, cx| {
        document.open_linked_document(generation, target.clone(), 1, source_window, cx);
        document.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        document.finish_linked_open(
            LinkedOpen {
                request: 1,
                generation,
                path: target,
                page: 1,
                source_window,
            },
            Ok(()),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.path(),
            Some(std::path::Path::new("examples/field-guide.pdf"))
        );
        assert_eq!(reader.page_index(), 0);
    });
}

#[gpui_kit::test]
fn earlier_link_open_cannot_override_a_newer_request(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    document.update(cx, |document, cx| {
        document.linked_open_request = 2;
        let request = |request| LinkedOpen {
            request,
            generation,
            path: target.clone(),
            page: 1,
            source_window,
        };
        document.finish_linked_open(request(1), Ok(()), cx);
        assert_eq!(cx.windows().len(), 1);
        document.finish_linked_open(request(2), Ok(()), cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn checked_in_link_guide_opens_the_illustrated_field_guide(cx: &mut TestAppContext) {
    setup(cx);
    let (source_window, source) = open(cx, Some(PathBuf::from("examples/link-guide.pdf")));
    let source_reader = source.read_with(cx, |source, _| source.reader().clone());
    cx.run_until_parked();
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    source_reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    let field_guide = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != source_window)
            .unwrap()
    });
    cx.update_window(field_guide, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("reader-status").label(), Some("Page 8 of 24"));
    })
    .unwrap();
}
