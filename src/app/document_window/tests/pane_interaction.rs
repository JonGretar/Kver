//! Composed pane interactions use production input, clipboard and shared edits.
use super::*;
use crate::{
    app::keymap::{self, Preset},
    pdf::annotations::{HighlightColor, MarkupKind},
};
use gpui_kit::{
    Bounds, MouseButton, Pixels, point, px, size,
    test::{ClickOptions, TestEventExt, TestQueryExt},
};

fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
}

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, keys: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for key in keys.split_whitespace() {
            window.press(key, cx);
        }
    })
    .unwrap();
    settle(cx, handle);
}

fn prepare(
    cx: &mut TestAppContext,
    path: &str,
) -> (AnyWindowHandle, Entity<DocumentWindow>, Entity<ReaderView>) {
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, document) = open(cx, Some(path.into()));
    cx.simulate_window_resize(handle, size(px(1200.), px(1000.)));
    settle(cx, handle);
    let first = active(&document, cx);
    (handle, document, first)
}

fn active(document: &Entity<DocumentWindow>, cx: &TestAppContext) -> Entity<ReaderView> {
    document.read_with(cx, |d, _| d.reader.clone())
}

fn selected(reader: &Entity<ReaderView>, cx: &TestAppContext) -> Option<String> {
    reader.read_with(cx, |r, _| r.selected_text())
}

fn viewport(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    reader: &Entity<ReaderView>,
) -> Bounds<Pixels> {
    cx.update_window(handle, |_, window, _| {
        window
            .within(("reading-pane", reader.entity_id()))
            .find("page")
            .bounds()
    })
    .unwrap()
}

fn begin(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    press(cx, handle, key);
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("hint-input").label(), Some("Choose start: __"));
    })
    .unwrap();
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui_kit::test]
fn pane_selections_cancel_acquisition_and_copy_without_moving_either_view(cx: &mut TestAppContext) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    begin(cx, handle, "v");
    press(cx, handle, "s a s a");
    assert_eq!(selected(&second, cx).as_deref(), Some("café"));
    press(cx, handle, "ctrl-tab");
    assert_eq!(active(&document, cx), first);
    begin(cx, handle, "shift-v");
    press(cx, handle, "s a s a");
    assert_eq!(selected(&first, cx).as_deref(), Some("Second naïve line"));
    let positions =
        [first.clone(), second.clone()].map(|r| r.read_with(cx, |r, _| r.reading_position()));
    for key in ["v", "shift-v"] {
        begin(cx, handle, key);
        // First endpoint and an unfinished prefix must not replace the old span.
        press(cx, handle, "a a s ctrl-tab");
        assert_eq!(active(&document, cx), second);
        assert_eq!(selected(&first, cx).as_deref(), Some("Second naïve line"));
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("hint-input").is_none());
        })
        .unwrap();
        press(cx, handle, "secondary-c");
        assert_eq!(clipboard(cx).as_deref(), Some("café"));
        press(cx, handle, "ctrl-tab secondary-c");
        assert_eq!(clipboard(cx).as_deref(), Some("Second naïve line"));
    }
    for (reader, position) in [first, second].into_iter().zip(positions) {
        reader.read_with(cx, |r, cx| {
            assert_eq!(r.reading_position(), position);
            assert!(!r.is_dirty(cx));
        });
    }
}

#[gpui_kit::test]
fn pane_switch_override_cancels_hints_without_enabling_reader_typing_commands(
    cx: &mut TestAppContext,
) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    cx.update(|cx| {
        keymap::reload(
            cx,
            Some(
                "[[keyboard.bindings]]\nkeys = 'ctrl-tab'\ncontext = 'reader'\ncommand = 'copy'\n\
                 [[keyboard.bindings]]\nkeys = 'ctrl-alt-w'\ncontext = 'reader'\ncommand = 'switch-pane'\n"
                    .into(),
            ),
            Preset::Vim,
        )
        .unwrap();
    });
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    begin(cx, handle, "v");
    press(cx, handle, "s");
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("hint-input").label(), Some("Choose start: s_"));
    })
    .unwrap();
    press(cx, handle, "ctrl-tab");
    assert_eq!(active(&document, cx), second);
    press(cx, handle, "ctrl-alt-w");
    assert_eq!(active(&document, cx), first);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn pane_markup_controls_and_document_undo_share_edits_not_selection(cx: &mut TestAppContext) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    press(cx, handle, "g w s");
    let second = active(&document, cx);
    begin(cx, handle, "v");
    press(cx, handle, "s a s a ctrl-tab");
    begin(cx, handle, "v");
    press(cx, handle, "a a a a");
    for (kind, keys) in MarkupKind::ALL
        .into_iter()
        .zip(["a h", "a u", "a s", "a w"])
    {
        let a = viewport(cx, handle, &first);
        let b = viewport(cx, handle, &second);
        press(cx, handle, keys);
        let annotation = first
            .read_with(cx, |r, cx| r.selected_page_annotation(cx))
            .expect("new markup selected in originating pane");
        assert_eq!(annotation.kind(), kind);
        assert_eq!(annotation.quads().len(), 1);
        first.read_with(cx, |r, _| {
            let excerpts = r
                .document()
                .unwrap()
                .annotation_excerpts(0, std::slice::from_ref(&annotation))
                .unwrap();
            assert_eq!(excerpts[0].1.as_deref(), Some("Alpha"));
        });
        assert_eq!(selected(&second, cx).as_deref(), Some("café"));
        assert_eq!(viewport(cx, handle, &first), a);
        assert_eq!(viewport(cx, handle, &second), b);
        cx.update_window(handle, |_, window, _| {
            assert_eq!(window.find_all("selected-highlight-controls").len(), 1);
        })
        .unwrap();
        press(cx, handle, "ctrl-tab");
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("selected-highlight-controls").is_none());
        })
        .unwrap();
        press(cx, handle, "u");
        for reader in [&first, &second] {
            reader.read_with(cx, |r, cx| {
                let session = r.session().unwrap().read(cx);
                assert!(session.annotations().unwrap().unwrap().is_empty());
                assert!(!r.is_dirty(cx));
            });
        }
        press(cx, handle, "ctrl-r");
        for reader in [&first, &second] {
            reader.read_with(cx, |r, cx| {
                let session = r.session().unwrap().read(cx);
                assert_eq!(session.annotations().unwrap().unwrap()[0].kind(), kind);
                assert!(r.is_dirty(cx));
            });
        }
        // Undo from the other pane again; the pre-existing selection survives.
        press(cx, handle, "u ctrl-tab");
        begin(cx, handle, "v");
        press(cx, handle, "a a a a");
    }
}

#[gpui_kit::test]
fn pane_rulers_pointer_selections_and_layered_escape_are_independent(cx: &mut TestAppContext) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    let text =
        crate::pdf::page_text(std::path::Path::new("examples/selection-sample.pdf"), 0).unwrap();
    let place = |cx: &mut TestAppContext, reader: &Entity<ReaderView>, line: usize| {
        let scale = reader.read_with(cx, |r, _| r.scale());
        let glyph = &text.glyphs[text.lines[line].start];
        let at = text.transform.document_to_page(crate::coordinates::Point {
            x: (glyph.start.x + glyph.end.x) / 2.,
            y: (glyph.top + glyph.bottom) / 2.,
        });
        cx.update_window(handle, |_, window, cx| {
            window
                .within(("reading-pane", reader.entity_id()))
                .click_with_options(
                    "pdf-page-0",
                    ClickOptions::new()
                        .with_offset(point(px(at.x as f32 * scale), px(at.y as f32 * scale)))
                        .with_modifiers(gpui_kit::Modifiers::alt()),
                    cx,
                );
        })
        .unwrap();
        settle(cx, handle);
    };
    place(cx, &first, 0);
    assert_eq!(active(&document, cx), first);
    place(cx, &second, 1);
    assert_eq!(active(&document, cx), second);
    let markers = cx
        .update_window(handle, |_, window, _| {
            [&first, &second].map(|r| {
                window
                    .within(("reading-pane", r.entity_id()))
                    .find("reading-ruler-marker")
                    .bounds()
            })
        })
        .unwrap();
    for _ in 0..2 {
        press(cx, handle, "ctrl-tab");
        cx.update_window(handle, |_, window, _| {
            for (reader, marker) in [&first, &second].into_iter().zip(markers) {
                assert_eq!(
                    window
                        .within(("reading-pane", reader.entity_id()))
                        .find("reading-ruler-marker")
                        .bounds(),
                    marker
                );
            }
        })
        .unwrap();
    }
    // Ordinary double click activates the pointer pane and suppresses only its ruler.
    cx.update_window(handle, |_, window, cx| {
        let page = window
            .within(("reading-pane", first.entity_id()))
            .find("pdf-page-0")
            .bounds();
        let scale = first.read(cx).scale();
        let glyph = &text.glyphs[0];
        let at = text.transform.document_to_page(crate::coordinates::Point {
            x: (glyph.start.x + glyph.end.x) / 2.,
            y: (glyph.top + glyph.bottom) / 2.,
        });
        let at = page.origin + point(px(at.x as f32 * scale), px(at.y as f32 * scale));
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
        window
            .within(("reading-pane", first.entity_id()))
            .click_with_options(
                "pdf-page-0",
                ClickOptions::new()
                    .with_offset(at - page.origin)
                    .with_count(2),
                cx,
            );
    })
    .unwrap();
    settle(cx, handle);
    assert_eq!(active(&document, cx), first);
    assert_eq!(selected(&first, cx).as_deref(), Some("Alpha"));
    press(cx, handle, "secondary-c");
    assert_eq!(clipboard(cx).as_deref(), Some("Alpha"));
    begin(cx, handle, "v");
    press(cx, handle, "a a escape");
    assert_eq!(selected(&first, cx).as_deref(), Some("Alpha"));
    press(cx, handle, "escape");
    cx.update_window(handle, |_, window, _| {
        assert_eq!(
            window
                .within(("reading-pane", first.entity_id()))
                .find("reading-ruler-marker")
                .bounds(),
            markers[0]
        );
    })
    .unwrap();
    press(cx, handle, "escape");
    cx.update_window(handle, |_, window, _| {
        assert!(
            window
                .within(("reading-pane", first.entity_id()))
                .try_find("reading-ruler-marker")
                .is_none()
        );
        assert!(
            window
                .within(("reading-pane", second.entity_id()))
                .find("reading-ruler-marker")
                .visible()
        );
    })
    .unwrap();
    first.read_with(cx, |r, cx| assert!(!r.is_dirty(cx)));
}

#[gpui_kit::test]
fn pane_annotation_targets_and_context_commands_follow_pointer_and_active_state(
    cx: &mut TestAppContext,
) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    begin(cx, handle, "v");
    press(cx, handle, "s a s a a u ctrl-tab");
    begin(cx, handle, "v");
    press(cx, handle, "a a a a a h");
    let first_id = first.read_with(cx, |r, cx| r.selected_page_annotation(cx).unwrap().id());
    let second_id = second.read_with(cx, |r, cx| r.selected_page_annotation(cx).unwrap().id());
    let positions = [&first, &second].map(|r| r.read_with(cx, |r, _| r.reading_position()));
    press(cx, handle, "ctrl-tab");
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find_all("selected-highlight-controls").len(), 1);
        assert_eq!(
            window.find("selected-annotation-type").label(),
            Some("Underline")
        );
    })
    .unwrap();
    // Right-clicking the inactive pane activates it without replacing its span.
    cx.update_window(handle, |_, window, cx| {
        window
            .within(("reading-pane", first.entity_id()))
            .right_click("page", cx);
    })
    .unwrap();
    settle(cx, handle);
    assert_eq!(active(&document, cx), first);
    cx.update_window(handle, |_, window, cx| {
        let at = window.find_by_label("Recolor Pink").bounds().center();
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, handle);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| {
            let session = r.session().unwrap().read(cx);
            let annotations = session.annotations().unwrap().unwrap();
            assert_eq!(
                annotations
                    .iter()
                    .find(|a| a.id() == first_id)
                    .unwrap()
                    .color(),
                HighlightColor::Pink.color()
            );
            assert_ne!(
                annotations
                    .iter()
                    .find(|a| a.id() == second_id)
                    .unwrap()
                    .color(),
                HighlightColor::Pink.color()
            );
        });
    }
    for (reader, position) in [&first, &second].into_iter().zip(positions) {
        reader.read_with(cx, |r, _| assert_eq!(r.reading_position(), position));
    }
    press(cx, handle, "ctrl-tab");
    cx.update_window(handle, |_, window, cx| window.click("delete-highlight", cx))
        .unwrap();
    settle(cx, handle);
    first.read_with(cx, |r, cx| {
        assert_eq!(r.selected_page_annotation(cx).unwrap().id(), first_id);
    });
    assert!(second.read_with(cx, |r, cx| r.selected_page_annotation(cx).is_none()));
    press(cx, handle, "ctrl-tab u");
    first.read_with(cx, |r, cx| {
        assert_eq!(
            r.session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            2
        );
    });
    // Removing a view is not discarding the shared document.
    press(cx, handle, "g w c");
    assert_eq!(active(&document, cx), second);
    second.read_with(cx, |r, cx| {
        assert!(r.is_dirty(cx));
        assert_eq!(
            r.session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            2
        );
    });
}

#[gpui_kit::test]
fn pane_hints_clip_to_own_viewport_and_invalidate_on_geometry_changes(cx: &mut TestAppContext) {
    let (handle, document, first) = prepare(cx, "examples/selection-sample.pdf");
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    for orientation in ["g w v", "g w s"] {
        press(cx, handle, orientation);
        // Explicit high zoom and horizontal clipping in the active pane only.
        press(cx, handle, "= = = = = right");
        assert!(second.read_with(cx, |r, _| r.scale()) > 2.);
        for key in ["v", "shift-v"] {
            begin(cx, handle, key);
            cx.update_window(handle, |_, window, _| {
                let pane = window
                    .within(("reading-pane", second.entity_id()))
                    .find("page")
                    .bounds();
                let ribbon = window.find("hint-input").bounds();
                assert_eq!(ribbon.top(), pane.top());
                let label = window.find("hint-aa").bounds();
                assert!(label.left() >= pane.left());
                assert!(label.right() <= pane.right());
                assert!(label.top() >= ribbon.bottom());
                assert!(label.bottom() <= pane.bottom());
                assert!(
                    window
                        .within(("reading-pane", first.entity_id()))
                        .try_find("hint-aa")
                        .is_none()
                );
            })
            .unwrap();
            press(cx, handle, "a a a a");
            assert!(
                selected(&second, cx).is_some(),
                "high-zoom hints commit an ordinary selection"
            );
        }
        for change in 0..4 {
            begin(cx, handle, "v");
            match change {
                0 => cx
                    .update_window(handle, |_, window, cx| {
                        window.dispatch_action(Box::new(crate::reader::ZoomIn), cx);
                    })
                    .unwrap(),
                1 => cx
                    .update_window(handle, |_, window, cx| {
                        let from = window.find("pane-divider").bounds().center();
                        window.drag(from, from + point(px(20.), px(20.)), cx);
                    })
                    .unwrap(),
                2 => cx.simulate_window_resize(handle, size(px(1180.), px(980.))),
                _ => cx
                    .update_window(handle, |_, window, cx| {
                        let action: Box<dyn Action> = if orientation == "g w v" {
                            Box::new(SplitStacked)
                        } else {
                            Box::new(SplitSideBySide)
                        };
                        window.dispatch_action(action, cx);
                    })
                    .unwrap(),
            }
            settle(cx, handle);
            cx.update_window(handle, |_, window, _| {
                assert!(window.try_find("hint-input").is_none());
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn pane_link_hints_follow_only_their_own_visible_links(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let mut pdf = lopdf::Document::load_mem(&crate::pdf::linked_fixture()).unwrap();
    pdf.get_object_mut((3, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(
            "Annots",
            lopdf::Object::Array(vec![lopdf::Object::Reference((5, 0))]),
        );
    pdf.save(&source).unwrap();
    let (handle, document, first) = prepare(cx, source.to_str().unwrap());
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    for arrangement in ["g w v", "g w s"] {
        press(cx, handle, arrangement);
        for key in ["f", "shift-f"] {
            press(cx, handle, key);
            cx.update_window(handle, |_, window, _| {
                assert_eq!(window.find("hint-input").label(), Some("Choose link: __"));
                let pane = window
                    .within(("reading-pane", second.entity_id()))
                    .find("page")
                    .bounds();
                let label = window.find("hint-aa").bounds();
                assert!(label.left() >= pane.left() && label.right() <= pane.right());
                assert!(label.top() >= pane.top() && label.bottom() <= pane.bottom());
                assert!(
                    window
                        .within(("reading-pane", first.entity_id()))
                        .try_find("hint-aa")
                        .is_none()
                );
            })
            .unwrap();
            press(cx, handle, "a ctrl-tab");
            assert_eq!(active(&document, cx), first);
            press(cx, handle, "ctrl-tab");
            cx.update_window(handle, |_, window, _| {
                assert!(window.try_find("hint-input").is_none());
            })
            .unwrap();
            second.read_with(cx, |r, _| assert_eq!(r.page_index(), 0));
        }
    }
    press(cx, handle, "f");
    press(cx, handle, "a a");
    first.read_with(cx, |r, _| assert_eq!(r.page_index(), 0));
    second.read_with(cx, |r, _| assert_eq!(r.page_index(), 1));
    press(cx, handle, "ctrl-tab shift-f");
    press(cx, handle, "a a");
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    first.read_with(cx, |r, _| assert_eq!(r.page_index(), 0));
    second.read_with(cx, |r, _| assert_eq!(r.page_index(), 1));
}

#[gpui_kit::test]
fn pointer_link_activates_inactive_pane_before_emitting_destination(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let mut pdf = lopdf::Document::load_mem(&crate::pdf::linked_fixture()).unwrap();
    pdf.get_object_mut((3, 0))
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(
            "Annots",
            lopdf::Object::Array(vec![lopdf::Object::Reference((17, 0))]),
        );
    pdf.save(&source).unwrap();
    std::fs::copy("examples/readme-sample.pdf", root.path().join("other.pdf")).unwrap();
    let (handle, document, first) = prepare(cx, source.to_str().unwrap());
    press(cx, handle, "g w v");
    let second = active(&document, cx);
    cx.update_window(handle, |_, window, cx| {
        window
            .within(("reading-pane", first.entity_id()))
            .click("pdf-link-0-0", cx);
    })
    .unwrap();
    settle(cx, handle);
    assert_eq!(active(&document, cx), first);
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| {
            assert_eq!(r.page_index(), 0);
            assert!(!r.is_dirty(cx));
        });
    }
}
