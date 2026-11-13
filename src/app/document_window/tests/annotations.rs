//! Text-markup browsing and editing through the production document window.
use super::*;
use crate::pdf::{self, annotations::HighlightColor};
use gpui_kit::test::{TestEventExt, TestQueryExt};

fn row(epoch: u64, serial: u64) -> String {
    format!("annotation-{epoch}-{serial}")
}

fn show(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn list_edits_all_imported_types_and_missing_text_then_saves_preserved_metadata(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("list-edits.pdf");
    std::fs::copy("examples/annotations-clean.pdf", &source).unwrap();
    let original = pdf::Document::open(&source)
        .unwrap()
        .page_annotations(0, 1)
        .unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    let (second, other) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    show(cx, second);
    cx.update_window(second, |_, window, cx| window.click(row(epoch, 3), cx))
        .unwrap();
    cx.run_until_parked();
    show(cx, handle);
    for serial in [3, 1, 2, 0, 4] {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let entry = window.find(row(epoch, serial));
            let color = window.within(row(epoch, serial)).find("recolor-annotation");
            let delete = window.within(row(epoch, serial)).find("delete-annotation");
            assert_eq!(color.label(), Some("Change annotation color"));
            assert_eq!(delete.label(), Some("Delete annotation"));
            assert!(color.bounds().top() < entry.bounds().center().y);
            assert_eq!(color.bounds().top(), delete.bounds().top());
            assert!(color.bounds().right() <= delete.bounds().left());
            if serial == 4 {
                assert!(
                    window
                        .find(row(epoch, serial))
                        .label()
                        .unwrap()
                        .starts_with("No marked text available")
                );
                assert_eq!(window.find("copy-annotation").disabled(), Some(true));
            }
            // Commands target this row without first selecting/navigating it.
            window
                .within(row(epoch, serial))
                .click("recolor-annotation", cx);
        })
        .unwrap();
        cx.run_until_parked();
        // Exercise the actual menu's keyboard and pointer paths, not callbacks.
        cx.update_window(handle, |_, window, cx| {
            if serial == 1 {
                window.press("down", cx);
                window.press("down", cx);
                window.press("down", cx);
                window.press("down", cx);
                window.press("enter", cx);
            } else {
                let at = window.find_by_label("Pink").bounds().center();
                window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        reader.read_with(cx, |r, cx| {
            let entries = r
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap();
            assert_eq!(
                entries
                    .iter()
                    .find(|a| a.id().serial() == serial)
                    .unwrap()
                    .color(),
                HighlightColor::Pink.color()
            );
        });
        // List edits use shared history for all four kinds and missing excerpts.
        cx.update_window(second, |_, window, cx| window.press("cmd-z", cx))
            .unwrap();
        cx.run_until_parked();
        reader.read_with(cx, |r, cx| {
            let entries = r
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap();
            let restored = entries.iter().find(|a| a.id().serial() == serial).unwrap();
            let before = original.iter().find(|a| a.id().serial() == serial).unwrap();
            assert_eq!(restored.color(), before.color());
            assert_eq!(restored.kind(), before.kind());
            assert_eq!(restored.quads(), before.quads());
            assert_eq!(restored.author(), before.author());
            assert_eq!(restored.comment(), before.comment());
        });
        cx.update_window(handle, |_, window, cx| window.press("cmd-shift-z", cx))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(second, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find(row(epoch, serial))
                    .label()
                    .unwrap()
                    .contains("#FF73A6")
            );
            if serial == 3 {
                assert!(
                    window
                        .find(row(epoch, 3))
                        .label()
                        .unwrap()
                        .contains("By Fixture author")
                );
            }
        })
        .unwrap();
    }
    other.read_with(cx, |d, cx| {
        assert_eq!(d.reader.read(cx).page_index(), 0);
        assert!(d.reader.read(cx).is_dirty(cx));
    });
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    let saved = pdf::Document::open(&source)
        .unwrap()
        .page_annotations(0, 2)
        .unwrap();
    assert_eq!(saved.len(), original.len());
    for (before, after) in original.iter().zip(&saved) {
        assert_eq!(after.kind(), before.kind());
        assert_eq!(after.quads(), before.quads());
        assert_eq!(after.author(), before.author());
        assert_eq!(after.comment(), before.comment());
        assert_eq!(after.name(), before.name());
        assert_eq!(after.opacity(), before.opacity());
        assert_eq!(after.color(), HighlightColor::Pink.color());
    }
    // Deleting the selected row from one window clears identification in the
    // other window too. Removed row controls must not remain usable.
    cx.update_window(handle, |_, window, cx| window.click(row(epoch, 3), cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        // Reach the destructive control through real Tab navigation.
        for _ in 0..32 {
            if window
                .within(row(epoch, 3))
                .find("delete-annotation")
                .focused()
                == Some(true)
            {
                break;
            }
            window.press("tab", cx);
        }
        assert_eq!(
            window
                .within(row(epoch, 3))
                .find("delete-annotation")
                .focused(),
            Some(true)
        );
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row(epoch, 3)).is_none());
        assert!(window.try_find("annotation-identification").is_none());
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        window.input("jk", cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    let saved = pdf::Document::open(&source)
        .unwrap()
        .page_annotations(0, 3)
        .unwrap();
    assert_eq!(saved.len(), original.len() - 1);
    assert!(saved.iter().all(|a| a.name() != Some("imported-highlight")));
    for serial in [1, 2, 0, 4] {
        cx.update_window(handle, |_, window, cx| {
            window
                .within(row(epoch, serial))
                .click("delete-annotation", cx)
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.press("cmd-z", cx))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(second, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find(row(epoch, serial)).visible());
            assert!(
                window
                    .find(row(epoch, serial))
                    .label()
                    .unwrap()
                    .contains("#FF73A6")
            );
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| window.press("cmd-shift-z", cx))
            .unwrap();
        cx.run_until_parked();
    }
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    let saved = pdf::Document::open(&source).unwrap();
    assert!(saved.page_annotations(0, 4).unwrap().is_empty());
    assert_eq!(saved.page_annotations(1, 4).unwrap().len(), 1);
}

#[gpui_kit::test]
fn deleting_final_list_row_repairs_focus_and_replacement_rejects_open_color_menu(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(|cx| crate::app::keymap::select_preset(cx, crate::app::keymap::Preset::Vim).unwrap());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("single.pdf");
    let mut fixture = lopdf::Document::load("examples/annotations-clean.pdf").unwrap();
    for (page_ix, id) in fixture.get_pages() {
        fixture.get_dictionary_mut(id).unwrap().set(
            "Annots",
            if page_ix == 1 {
                vec![lopdf::Object::Reference((7, 0))]
            } else {
                vec![]
            },
        );
    }
    fixture.save(&source).unwrap();
    let (handle, document) = open(cx, Some(source.clone()));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    show(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.within(row(epoch, 0)).click("delete-annotation", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("annotations-status").label(),
            Some("No text markup in this PDF")
        );
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        window.input("nGjk", cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.press("cmd-s", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 9)
            .unwrap()
            .is_empty()
    );
    reader.read_with(cx, |r, _| assert_eq!(r.page_index(), 0));
    // Save retains deletion history. Undo restores an unselected row without
    // moving List focus; normal arrows can select it again.
    cx.update_window(handle, |_, window, cx| window.press("cmd-z", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(row(epoch, 0)).visible());
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        assert!(window.try_find("annotation-identification").is_none());
        assert!(window.find("document-unsaved").visible());
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("annotation-identification").visible());
        window.press("cmd-shift-z", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        assert!(window.try_find("document-unsaved").is_none());
    })
    .unwrap();
    reader.update(cx, |r, cx| {
        r.open_path(PathBuf::from("examples/annotations-clean.pdf"), cx)
    });
    cx.run_until_parked();
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.within(row(epoch, 3)).click("recolor-annotation", cx)
    })
    .unwrap();
    cx.run_until_parked();
    reader.update(cx, |r, cx| {
        r.open_path(PathBuf::from("examples/readme-sample.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        if window.try_find_by_label("Pink").is_some() {
            let at = window.find_by_label("Pink").bounds().center();
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |r, cx| assert!(!r.is_dirty(cx)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        window.input("nGjk", cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |r, _| assert_eq!(r.page_index(), 0));
}

#[gpui_kit::test]
fn deleting_an_unselected_row_elsewhere_repairs_its_focused_button_without_changing_selection(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let path = PathBuf::from("examples/annotations-clean.pdf");
    let (first, document) = open(cx, Some(path.clone()));
    let (second, _) = open(cx, Some(path));
    cx.run_until_parked();
    let epoch = document.read_with(cx, |d, cx| {
        d.reader.read(cx).session().unwrap().read(cx).epoch()
    });
    show(cx, first);
    show(cx, second);
    cx.update_window(second, |_, window, cx| window.click(row(epoch, 0), cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| {
        for _ in 0..32 {
            if window
                .within(row(epoch, 1))
                .find("delete-annotation")
                .focused()
                == Some(true)
            {
                break;
            }
            window.press("tab", cx);
        }
        assert_eq!(
            window
                .within(row(epoch, 1))
                .find("delete-annotation")
                .focused(),
            Some(true)
        );
    })
    .unwrap();
    cx.update_window(first, |_, window, cx| {
        window.within(row(epoch, 3)).click("delete-annotation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row(epoch, 3)).is_none());
        // A preceding deletion shifts virtual row positions, not button identity.
        assert_eq!(
            window
                .within(row(epoch, 1))
                .find("delete-annotation")
                .focused(),
            Some(true)
        );
    })
    .unwrap();
    cx.update_window(first, |_, window, cx| {
        window.within(row(epoch, 1)).click("delete-annotation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row(epoch, 1)).is_none());
        assert_eq!(window.find("annotations-panel").focused(), Some(true));
        window.press("cmd-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Squiggly passage")
        );
        assert!(window.find("annotation-identification").visible());
        window.press("up", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn imported_annotations_browse_copy_and_identify_all_four_types(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let expected = [
        (3, "Highlight passage; Highlight"),
        (1, "Underline passage; Underline"),
        (2, "Strikeout passage; Strikethrough"),
        (0, "Squiggly passage; Squiggly underline"),
    ];
    let mut previous_bottom = gpui_kit::px(0.);
    for (serial, prefix) in expected {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let target = window.find(row(epoch, serial));
            assert!(
                target.label().unwrap().starts_with(prefix),
                "{:?}",
                target.label()
            );
            if serial == 3 {
                assert!(target.label().unwrap().ends_with("; By Fixture author"));
            } else {
                assert!(!target.label().unwrap().contains("; By "));
            }
            assert!(target.bounds().top() >= previous_bottom);
            previous_bottom = target.bounds().bottom();
            window.click(row(epoch, serial), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("annotation-identification").visible());
            window.press("cmd-c", cx);
            let text = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(!text.contains("Attached comment"));
            assert_eq!(text, prefix.split(';').next().unwrap());
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        window.click(row(epoch, 5), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(row(epoch, 5))
                .label()
                .unwrap()
                .starts_with("No marked text available")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("untouched".into()));
        window.press("cmd-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("untouched")
        );
        window.press("down", cx); // malformed quad, usable Rect
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(row(epoch, 6))
                .label()
                .unwrap()
                .contains("Malformed")
        );
        assert!(window.find("annotation-identification").visible());
        assert_eq!(
            window
                .within(row(epoch, 6))
                .find("recolor-annotation")
                .disabled(),
            Some(true)
        );
        assert_eq!(
            window
                .within(row(epoch, 6))
                .find("delete-annotation")
                .disabled(),
            Some(true)
        );
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 0);
}

#[gpui_kit::test]
fn replacing_document_while_annotations_load_never_restores_old_rows_or_copy(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    reader.update(cx, |r, cx| {
        r.open_path(PathBuf::from("examples/readme-sample.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("annotations-status").label(),
            Some("No text markup in this PDF")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 0);
}

#[gpui_kit::test]
fn a_growing_annotation_list_virtualizes_rows_and_navigates_offscreen_entries(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations-many.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(row(epoch, 0)).visible());
        assert!(window.try_find(row(epoch, 239)).is_none());
        window.scroll(
            "annotations-panel",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                gpui_kit::px(0.),
                gpui_kit::px(-30_000.),
            )),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row(epoch, 0)).is_none());
        window.click(row(epoch, 239), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("copy-annotation", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Highlight passage")
        );
        assert!(window.find("annotation-identification").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn rotated_annotation_navigation_and_copy_remain_window_local(cx: &mut TestAppContext) {
    setup(cx);
    let path = PathBuf::from("examples/annotations.pdf");
    let (handle, document) = open(cx, Some(path.clone()));
    let (_, other) = open(cx, Some(path));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(row(epoch, 1 << 32), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let before = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("annotation-identification").visible());
            let before = window.find("annotation-identification").bounds();
            window.press("cmd-c", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Rotated passage")
            );
            window.dispatch_action(Box::new(ZoomIn), cx);
            before
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let after = window.find("annotation-identification").bounds();
        assert!(after.size.height > before.size.height);
        assert!(after.size.width > before.size.width);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 1);
    other.read_with(cx, |d, cx| assert_eq!(d.reader().read(cx).page_index(), 0));
}

#[gpui_kit::test]
fn replacing_a_document_with_an_unreadable_pdf_does_not_leave_annotations_loading(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/missing.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("annotations-status")
                .label()
                .unwrap()
                .starts_with("Couldn’t load")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
    })
    .unwrap();
}
