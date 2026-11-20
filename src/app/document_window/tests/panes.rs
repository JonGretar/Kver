use super::*;
use crate::reader::{ReadingMode, ZoomMode};
use gpui_kit::{
    InputEvent as _, Modifiers, PinchEvent, ScrollDelta, ScrollEvent, TouchPhase, point, px, size,
    test::TestEventExt as _,
};

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, keys: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for key in keys.split_whitespace() {
            window.press(key, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
}

fn prepare(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<DocumentWindow>) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.simulate_window_resize(handle, size(px(1100.), px(850.)));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    (handle, document)
}

fn active(document: &Entity<DocumentWindow>, cx: &TestAppContext) -> Entity<ReaderView> {
    document.read_with(cx, |d, _| d.reader.clone())
}

#[gpui_kit::test]
fn split_copies_location_then_navigation_layout_and_history_are_independent(
    cx: &mut TestAppContext,
) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    first.update(cx, |reader, cx| {
        reader.select_page(8, cx);
        reader.zoom_in(cx);
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
    });
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    assert_ne!(first, second);
    document.read_with(cx, |d, cx| {
        let a = first.read(cx);
        let b = second.read(cx);
        assert_eq!(a.session(), b.session());
        assert_eq!(a.jump_location(), b.jump_location());
        assert!(!a.is_dirty(cx));
        assert!(d.jumplist.entries.is_empty());
    });
    press(cx, handle, "right secondary-=");
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 8);
        assert_eq!(reader.scale(), 1.25);
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
    });
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 9));
    press(cx, handle, "ctrl-tab");
    assert_eq!(active(&document, cx), first);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Shared toolbar reflects the active pane, not the most recently moved one.
        assert!(
            window
                .find("reading-mode")
                .label()
                .unwrap()
                .starts_with("Single page")
        );
        window.dispatch_action(Box::new(Continuous), cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous)
    });
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage)
    });
    // Semantic navigation records only in its originating pane.
    first.update(cx, |reader, cx| {
        reader.navigate_outline(crate::pdf::OutlineDestination::at_page(20), cx)
    });
    cx.run_until_parked();
    document.read_with(cx, |d, _| assert!(d.jumplist.can_back()));
    press(cx, handle, "ctrl-tab");
    document.read_with(cx, |d, _| assert!(d.jumplist.entries.is_empty()));
    press(cx, handle, "ctrl-tab");
    document.read_with(cx, |d, _| assert_eq!(d.jumplist.entries.len(), 2));
}

#[gpui_kit::test]
fn orientations_divider_close_and_keep_preserve_stable_readers(cx: &mut TestAppContext) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    for stacked in [false, true, false] {
        if stacked {
            press(cx, handle, "secondary-shift-\\");
        } else {
            press(cx, handle, "secondary-\\");
        }
        assert_eq!(active(&document, cx), second);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let a = window.find(("reading-pane", first.entity_id())).bounds();
            let b = window.find(("reading-pane", second.entity_id())).bounds();
            if stacked {
                assert!((a.size.height - b.size.height).abs() < px(1.));
                assert_eq!(a.bottom(), b.top());
            } else {
                assert!((a.size.width - b.size.width).abs() < px(1.));
                assert_eq!(a.right(), b.left());
            }
            let from = window.find("pane-divider").bounds().center();
            let to = if stacked {
                from + point(px(0.), px(100.))
            } else {
                from + point(px(100.), px(0.))
            };
            window.drag(from, to, cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ClosePane), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(active(&document, cx), first);
    document.read_with(cx, |d, _| assert!(d.other_pane.is_none()));
    press(cx, handle, "secondary-shift-\\");
    let survivor = active(&document, cx);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(KeepOnlyPane), cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(active(&document, cx), survivor);
    for action in [
        Box::new(ClosePane) as Box<dyn Action>,
        Box::new(KeepOnlyPane),
    ] {
        cx.update_window(handle, |_, window, cx| window.dispatch_action(action, cx))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    }
}

#[gpui_kit::test]
fn pointer_scroll_pinch_and_wheel_do_not_change_keyboard_pane(cx: &mut TestAppContext) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let pointer = window
            .within(("reading-pane", first.entity_id()))
            .find("page")
            .bounds()
            .center();
        window.pointer_move(pointer, None, cx);
        window.dispatch_event(
            ScrollEvent {
                position: pointer,
                delta: ScrollDelta::Pixels(point(px(0.), px(-90.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            PinchEvent {
                position: pointer,
                delta: 0.25,
                phase: TouchPhase::Moved,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            ScrollEvent {
                position: pointer,
                delta: ScrollDelta::Lines(point(0., 1.)),
                modifiers: Modifiers {
                    platform: cfg!(target_os = "macos"),
                    control: !cfg!(target_os = "macos"),
                    ..Default::default()
                },
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(active(&document, cx), second);
    first.read_with(cx, |reader, _| {
        assert!((reader.scale() - 1.5625).abs() < 0.001)
    });
    second.read_with(cx, |reader, _| assert_eq!(reader.scale(), 1.));
    press(cx, handle, "right");
    first.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    cx.update_window(handle, |_, window, cx| {
        window.click_at(
            ("reading-pane", first.entity_id()),
            point(px(0.5), px(0.5)),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(active(&document, cx), first);
    press(cx, handle, "right");
    first.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
}

#[gpui_kit::test]
fn resize_refits_each_pane_and_never_changes_orientation_or_discards_a_pane(
    cx: &mut TestAppContext,
) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    first.update(cx, |reader, cx| reader.fit_width(cx));
    let fixed = second.read_with(cx, |reader, _| reader.scale());
    for font_size in [16., 24., 16.] {
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        cx.simulate_window_resize(handle, size(px(650.), px(600.)));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let a = window.find(("reading-pane", first.entity_id())).bounds();
            let b = window.find(("reading-pane", second.entity_id())).bounds();
            assert!(a.size.width > px(0.) && b.size.width > px(0.));
            assert_eq!(a.right(), b.left());
            assert_eq!(a.top(), b.top());
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(active(&document, cx), second);
        second.read_with(cx, |reader, _| assert_eq!(reader.scale(), fixed));
        first.read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth)
        });
    }
}

#[gpui_kit::test]
fn failed_replacement_keeps_both_success_collapses_and_removed_replies_are_inert(
    cx: &mut TestAppContext,
) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/missing.pdf"), cx)
    });
    cx.run_until_parked();
    assert_eq!(active(&document, cx), second);
    document.read_with(cx, |d, _| assert!(d.other_pane.is_some()));
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    cx.run_until_parked();
    let replacement = active(&document, cx);
    assert_ne!(replacement, first);
    assert_ne!(replacement, second);
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert_eq!(
            d.reader.read(cx).path(),
            Some(std::path::Path::new("examples/field-guide.pdf"))
        );
    });
    // Retained external handles cannot send old document/jump work into the
    // replacement window once the removed pane's subscriptions are dropped.
    second.update(cx, |reader, cx| {
        reader.navigate_outline(crate::pdf::OutlineDestination::at_page(42), cx)
    });
    cx.run_until_parked();
    document.read_with(cx, |d, _| assert!(d.jumplist.entries.is_empty()));
}

#[gpui_kit::test]
fn dirty_split_close_and_replacement_still_require_an_explicit_decision(cx: &mut TestAppContext) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("annotated.pdf");
    std::fs::copy("examples/selection-sample.pdf", &path).unwrap();
    let (handle, document) = open(cx, Some(path.clone()));
    cx.run_until_parked();
    super::saving::highlight(cx, handle, &document);
    press(cx, handle, "secondary-\\");
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_some());
        assert!(d.reader.read(cx).is_dirty(cx));
        assert_eq!(
            d.reader
                .read(cx)
                .session()
                .unwrap()
                .read(cx)
                .participant_count(),
            2
        );
    });
    press(cx, handle, "secondary-w");
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_some());
        assert!(d.reader.read(cx).is_dirty(cx));
    });
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("unsaved-cancel", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_some());
        assert_eq!(d.reader.read(cx).path(), Some(path.as_path()));
    });
    // Removing a view has no document-wide discard or save side effect.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ClosePane), cx)
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_none());
        assert!(d.reader.read(cx).is_dirty(cx));
    });
    press(cx, handle, "secondary-s");
    document.read_with(cx, |d, cx| assert!(!d.reader.read(cx).is_dirty(cx)));
}

#[gpui_kit::test]
fn reader_only_bindings_and_palette_preserve_shared_find_query(cx: &mut TestAppContext) {
    let (handle, document) = prepare(cx);
    press(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.input("shore", cx);
        window.press("secondary-\\", cx);
        window.press("ctrl-tab", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, _| assert!(d.other_pane.is_none()));
    // Palette explicitly invokes the command even when opened from an input.
    press(cx, handle, "secondary-shift-p");
    cx.update_window(handle, |_, window, cx| {
        window.input("Split side by side", cx)
    })
    .unwrap();
    cx.run_until_parked();
    press(cx, handle, "enter");
    document.read_with(cx, |d, _| assert!(d.other_pane.is_some()));
    press(cx, handle, "ctrl-tab secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").value(), Some("shore"));
    })
    .unwrap();
    cx.update(|cx| crate::app::keymap::select_preset(cx, crate::app::keymap::Preset::Vim).unwrap());
    press(cx, handle, "escape");
    // Vim chords use GPUI's pending sequence handling, not a second command bus.
    press(cx, handle, "g w s g w w g w o");
    document.read_with(cx, |d, _| assert!(d.other_pane.is_none()));
}

#[gpui_kit::test]
fn fit_modes_preserve_document_anchors_across_divider_and_orientation_changes(
    cx: &mut TestAppContext,
) {
    let (handle, document) = prepare(cx);
    let first = active(&document, cx);
    first.update(cx, |reader, cx| {
        reader.select_page(10, cx);
        reader.scroll_by(120., cx);
        reader.fit_width(cx);
    });
    press(cx, handle, "secondary-\\");
    let second = active(&document, cx);
    second.update(cx, |reader, cx| {
        reader.select_page(30, cx);
        reader.scroll_by(70., cx);
        reader.fit_page(cx);
    });
    cx.run_until_parked();
    let a = first.read_with(cx, |reader, _| reader.reading_position());
    let b = second.read_with(cx, |reader, _| reader.reading_position());
    press(cx, handle, "secondary-shift-\\");
    for (reader, expected, mode) in [
        (&first, a, ZoomMode::FitWidth),
        (&second, b, ZoomMode::FitPage),
    ] {
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.zoom_mode(), mode);
            let actual = reader.reading_position();
            assert_eq!(actual.0, expected.0);
            assert!(
                (actual.1 - expected.1).abs() < 1.,
                "{expected:?} -> {actual:?}"
            );
        });
    }
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find("pane-divider").bounds().center();
        window.drag(from, from + point(px(0.), px(80.)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    for (reader, expected) in [(&first, a), (&second, b)] {
        reader.read_with(cx, |reader, _| {
            let actual = reader.reading_position();
            assert_eq!(actual.0, expected.0);
            assert!(
                (actual.1 - expected.1).abs() < 1.,
                "{expected:?} -> {actual:?}"
            );
        });
    }
}

#[gpui_kit::test]
fn hidden_same_session_replacement_cannot_bypass_last_window_unsaved_protection(
    cx: &mut TestAppContext,
) {
    use crate::{coordinates::Point, pdf::annotations::Quad};
    let (handle, document) = prepare(cx);
    press(cx, handle, "secondary-\\");
    let pending = document.update(cx, |d, cx| {
        let path = d.reader.read(cx).path().unwrap().to_path_buf();
        d.stage_replacement(DocumentSource::File(path), cx);
        d.replacement.as_ref().unwrap().reader.clone()
    });
    // Act in the real interval after the staged reader has joined the session
    // but before the window's deferred adoption. No worker or session is mocked.
    let _subscription = cx
        .update_window(handle, |_, window, cx| {
            document.update(cx, |_, cx| {
                cx.subscribe_in(&pending, window, |d, _, event, window, cx| {
                    if !matches!(event, DocumentEvent::Opened { result: Ok(()), .. }) {
                        return;
                    }
                    let session = d.reader.read(cx).session().unwrap();
                    assert_eq!(session.read(cx).participant_count(), 3);
                    session.update(cx, |session, cx| {
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
                    d.request_leave(Leave::Close, window, cx);
                })
            })
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("unsaved-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(d.other_pane.is_some());
        assert!(d.reader.read(cx).is_dirty(cx));
        assert!(d.replacement.is_none());
    });
}
