//! Word acquisition through production commands, rendered labels and ordinary
//! selection/copy/markup owners, not a keyboard-specific selection fixture.
use super::*;
use crate::{
    app::keymap::{self, Preset},
    reader::{ReadingMode, VisualSelect},
};

fn draw(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx, handle);
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
    cx.run_until_parked();
}

fn begin(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    press(cx, handle, "v");
    settle(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("hint-input").label(), Some("Choose start: __"));
    })
    .unwrap();
}

fn selected(reader: &Entity<ReaderView>, cx: &TestAppContext) -> Option<String> {
    reader.read_with(cx, |reader, _| reader.selected_text())
}

#[gpui_kit::test]
fn visual_select_ribbon_keeps_close_visible_at_interface_scales(cx: &mut TestAppContext) {
    use gpui_kit::{component::theme::Theme, px, size};

    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    for font_size in [12., 16., 20.] {
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        for width in [360., 720., 1100.] {
            cx.simulate_window_resize(handle, size(px(width), px(800.)));
            settle(cx, handle);
            press(cx, handle, "v");
            settle(cx, handle);
            cx.update_window(handle, |_, window, cx| {
                let ribbon = window.find("hint-input").bounds();
                let close = window.find("cancel-word-hints");
                assert!(close.visible());
                assert!(close.bounds().right() <= ribbon.right());
                assert!(close.bounds().left() >= ribbon.left());
                assert_eq!(ribbon.top(), window.find("page").bounds().top());
                window.click("cancel-word-hints", cx);
            })
            .unwrap();
            settle(cx, handle);
            cx.update_window(handle, |_, window, cx| {
                assert!(window.try_find("hint-input").is_none());
                assert!(reader.read(cx).focus_handle(cx).is_focused(window));
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn visual_select_commits_inclusive_forward_reverse_and_same_word_in_both_layouts(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    settle(cx, handle);
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| reader.set_reading_mode(mode, cx));
        if mode == ReadingMode::SinglePage {
            reader.update(cx, |reader, cx| reader.zoom_in(cx));
        }
        settle(cx, handle);
        for (keys, expected) in [
            ("s a g a", "café world\nSecond naïve"),
            ("g a s a", "café world\nSecond naïve"),
            ("s a s a", "café"),
        ] {
            begin(cx, handle);
            press(cx, handle, keys);
            assert_eq!(selected(&reader, cx).as_deref(), Some(expected));
            reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
            cx.update_window(handle, |_, window, cx| {
                assert!(window.try_find("hint-input").is_none());
                assert!(reader.read(cx).focus_handle(cx).is_focused(window));
                window.press("secondary-c", cx);
                assert_eq!(
                    cx.read_from_clipboard().and_then(|item| item.text()),
                    Some(expected.to_owned()),
                );
                window.press("escape", cx);
            })
            .unwrap();
            assert_eq!(selected(&reader, cx), None);
        }
    }
}

#[gpui_kit::test]
fn visual_select_preserves_prior_selection_and_geometry_until_commit_or_cancel(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    settle(cx, handle);
    super::saving::select_text(cx, handle, &document);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
    let before = reader.read_with(cx, |reader, _| reader.reading_position());
    let bounds = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find("pdf-page-0").bounds()
        })
        .unwrap();
    begin(cx, handle);
    press(cx, handle, "s a");
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("hint-sa").label(), Some("Start: sa"));
        assert_eq!(window.find("hint-input").label(), Some("Choose end: __"));
        assert_eq!(window.find("pdf-page-0").bounds(), bounds);
        let ribbon = window.find("hint-input").bounds();
        let viewport = window.find("page").bounds();
        assert_eq!(ribbon.top(), viewport.top());
        assert_eq!(ribbon.left(), viewport.left());
        assert_eq!(ribbon.right(), viewport.right());
        assert!(window.find("hint-sa").bounds().top() >= ribbon.bottom());
        assert_eq!(
            window.find("cancel-word-hints").label(),
            Some("Cancel word hints")
        );
        assert!(
            window.find("cancel-word-hints").bounds().right() <= ribbon.right(),
            "close stays inside the ribbon"
        );
        window.press("g", cx);
        assert!(
            window.find("hint-sa").visible(),
            "start survives prefix filtering"
        );
        window.press("backspace", cx);
        assert_eq!(window.find("hint-input").label(), Some("Choose end: __"));
        window.press("backspace", cx);
        assert_eq!(window.find("hint-input").label(), Some("Choose start: __"));
        assert_eq!(window.find("hint-sa").label(), Some("sa"));
        window.press("z", cx);
        window.press("z", cx);
        assert_eq!(
            window.find("hint-input").label(),
            Some("No matching hint. Try again.")
        );
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.reading_position()),
        before
    );
    begin(cx, handle);
    press(cx, handle, "s a");
    cx.update_window(handle, |_, window, cx| {
        window.click("cancel-word-hints", cx)
    })
    .unwrap();
    settle(cx, handle);
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("hint-input").is_none());
        assert!(reader.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(window.find("pdf-page-0").bounds(), bounds);
    })
    .unwrap();
    // Ordinary pointer selection can replace an acquired range afterwards.
    begin(cx, handle);
    press(cx, handle, "s a s a");
    super::saving::select_text(cx, handle, &document);
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
}

#[gpui_kit::test]
fn visual_select_palette_and_custom_command_work_without_standard_bare_letter(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    settle(cx, handle);
    press(cx, handle, "v");
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("hint-input").is_none());
        window.press("secondary-shift-p", cx);
        window.input("Visual select", cx);
    })
    .unwrap();
    cx.run_until_parked();
    press(cx, handle, "enter");
    settle(cx, handle);
    press(cx, handle, "s a s a");
    assert_eq!(selected(&reader, cx).as_deref(), Some("café"));
    cx.update(|cx| {
        keymap::reload(cx, Some(
        "[[keyboard.bindings]]\nkeys = 'q'\ncontext = 'reader'\ncommand = 'visual-select'\n".into()
    ), Preset::Standard).unwrap()
    });
    press(cx, handle, "q");
    settle(cx, handle);
    press(cx, handle, "a a a a");
    assert_eq!(selected(&reader, cx).as_deref(), Some("Alpha"));
    press(cx, handle, "secondary-f");
    cx.update_window(handle, |_, window, cx| {
        window.input("qv", cx);
        assert_eq!(window.find("find-query").value(), Some("qv"));
        assert!(window.try_find("hint-input").is_none());
        window.dispatch_action(Box::new(VisualSelect), cx);
    })
    .unwrap();
    settle(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn visual_select_ranges_feed_every_existing_markup_kind(cx: &mut TestAppContext) {
    use crate::pdf::annotations::MarkupKind;
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    settle(cx, handle);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let text =
        crate::pdf::page_text(std::path::Path::new("examples/selection-sample.pdf"), 0).unwrap();
    cx.update_window(handle, |_, window, cx| {
        let page = window.find("pdf-page-0").bounds();
        let at = |line, end| {
            let unit = text.word_units(line)[2].clone();
            let glyph = &text.glyphs[if end { unit.end - 1 } else { unit.start }];
            let p = text.transform.document_to_page(crate::coordinates::Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * if end { 0.9 } else { 0.1 },
                y: (glyph.top + glyph.bottom) / 2.,
            });
            page.origin + gpui_kit::point(gpui_kit::px(p.x as f32), gpui_kit::px(p.y as f32))
        };
        window.drag(at(0, false), at(1, true), cx);
    })
    .unwrap();
    assert_eq!(
        selected(&reader, cx).as_deref(),
        Some("café world\nSecond naïve")
    );
    press(cx, handle, "a h");
    let pointer_quads = reader.read_with(cx, |reader, cx| {
        reader
            .session()
            .unwrap()
            .read(cx)
            .annotations()
            .unwrap()
            .unwrap()[0]
            .quads()
            .to_vec()
    });
    press(cx, handle, "secondary-z");
    for (kind, key) in MarkupKind::ALL
        .into_iter()
        .zip(["a h", "a u", "a s", "a w"])
    {
        begin(cx, handle);
        press(cx, handle, "s a g a");
        press(cx, handle, key);
        reader.read_with(cx, |reader, cx| {
            let annotations = reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap();
            let annotation = annotations.iter().find(|a| a.kind() == kind).unwrap();
            assert_eq!(annotation.page(), 0);
            assert_eq!(annotation.quads(), pointer_quads.as_slice());
            assert_eq!(
                reader.selected_text().as_deref(),
                Some("café world\nSecond naïve")
            );
        });
    }
}

#[gpui_kit::test]
fn visual_select_keeps_original_page_local_end_labels_across_visible_pages(
    cx: &mut TestAppContext,
) {
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some("examples/readme-sample.pdf".into()));
    settle(cx, handle);
    reader.update(cx, |reader, cx| {
        reader.zoom_out(cx);
        reader.zoom_out(cx);
    });
    settle(cx, handle);
    begin(cx, handle);
    let (first, second) = cx
        .update_window(handle, |_, window, _| {
            let pages = [
                window.find("pdf-page-0").bounds(),
                window.find("pdf-page-1").bounds(),
            ];
            let mut choices = [Vec::new(), Vec::new()];
            for a in 'a'..='z' {
                for b in 'a'..='z' {
                    let letters = format!("{a}{b}");
                    if let Some(label) = window.try_find(format!("hint-{letters}")) {
                        for (page, choices) in pages.iter().zip(choices.iter_mut()) {
                            if page.contains(&label.bounds().center()) {
                                choices.push(letters.clone());
                            }
                        }
                    }
                }
            }
            assert!(!choices[0].is_empty() && !choices[1].is_empty());
            (choices[0].clone(), choices[1].clone())
        })
        .unwrap();
    let pair = |letters: &str| {
        letters
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    press(cx, handle, &pair(&second[0]));
    cx.update_window(handle, |_, window, _| {
        for label in &first {
            assert!(window.try_find(format!("hint-{label}")).is_none());
        }
        for label in &second {
            assert!(window.find(format!("hint-{label}")).visible());
        }
    })
    .unwrap();
    press(cx, handle, "backspace");
    cx.update_window(handle, |_, window, _| {
        for label in first.iter().chain(second.iter()) {
            assert!(window.find(format!("hint-{label}")).visible());
        }
    })
    .unwrap();
    press(
        cx,
        handle,
        &format!("{} {}", pair(&second[0]), pair(&second[0])),
    );
    assert!(selected(&reader, cx).is_some());
    press(cx, handle, "a h");
    reader.read_with(cx, |reader, cx| {
        let annotations = reader
            .session()
            .unwrap()
            .read(cx)
            .annotations()
            .unwrap()
            .unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].page(), 1);
    });
}

#[gpui_kit::test]
fn visual_select_ruler_focus_invalidation_and_windows_do_not_leak(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    let (other, other_reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    settle(cx, handle);
    settle(cx, other);
    press(cx, handle, "r");
    settle(cx, handle);
    let marker = cx
        .update_window(handle, |_, window, _| {
            window.find("reading-ruler-marker").bounds()
        })
        .unwrap();
    begin(cx, handle);
    press(cx, handle, "s a");
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("reading-ruler-marker").is_none());
    })
    .unwrap();
    cx.update_window(other, |_, window, _| {
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
    press(cx, handle, "escape");
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("reading-ruler-marker").bounds(), marker);
    })
    .unwrap();
    begin(cx, handle);
    press(cx, handle, "s a s a");
    cx.update_window(handle, |_, window, _| {
        assert!(
            window.try_find("reading-ruler-marker").is_none(),
            "ordinary selection hides ruler"
        );
    })
    .unwrap();
    press(cx, handle, "escape");
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("reading-ruler-marker").bounds(), marker);
    })
    .unwrap();
    for action in [
        Box::new(crate::reader::ZoomIn) as Box<dyn gpui_kit::Action>,
        Box::new(crate::reader::SinglePage),
        Box::new(crate::reader::ScrollDown),
    ] {
        begin(cx, handle);
        press(cx, handle, "s a");
        cx.update_window(handle, |_, window, cx| window.dispatch_action(action, cx))
            .unwrap();
        settle(cx, handle);
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("hint-input").is_none())
        })
        .unwrap();
    }
    begin(cx, handle);
    press(cx, handle, "s a");
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowFind), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.input("vasa", cx);
        assert_eq!(window.find("find-query").value(), Some("vasa"));
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
    assert_eq!(selected(&reader, cx), None);
    assert_eq!(selected(&other_reader, cx), None);
}

#[gpui_kit::test]
fn visual_select_follows_the_existing_column_order(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some("examples/reading-order.pdf".into()));
    settle(cx, handle);
    reader.update(cx, |reader, cx| reader.fit_page(cx));
    settle(cx, handle);
    begin(cx, handle);
    // Locate the rendered choices from the real PDF's word geometry. Do not
    // derive labels from a second copy of the assignment algorithm.
    let text =
        crate::pdf::page_text(std::path::Path::new("examples/reading-order.pdf"), 0).unwrap();
    let scale = reader.read_with(cx, |reader, _| reader.scale());
    let labels = cx
        .update_window(handle, |_, window, _| {
            let page = window.find("pdf-page-0").bounds();
            ["Left column ends.", "Right column begins."].map(|needle| {
                let line = text
                    .lines
                    .iter()
                    .find(|line| {
                        text.glyphs[(*line).clone()]
                            .iter()
                            .map(|g| g.text.as_str())
                            .collect::<String>()
                            .starts_with(needle)
                    })
                    .expect("column boundary fixture");
                let glyph = &text.glyphs[line.start];
                let at = text.transform.document_to_page(crate::coordinates::Point {
                    x: glyph.start.x,
                    y: glyph.top,
                });
                let at = page.origin + point(px(at.x as f32 * scale), px(at.y as f32 * scale));
                let mut nearest = None;
                for a in 'a'..='z' {
                    for b in 'a'..='z' {
                        let pair = format!("{a}{b}");
                        if let Some(label) = window.try_find(format!("hint-{pair}")) {
                            let bounds = label.bounds();
                            let distance = f32::from(bounds.left() - at.x).abs()
                                + f32::from(bounds.bottom() - at.y).abs();
                            if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
                                nearest = Some((distance, pair));
                            }
                        }
                    }
                }
                let (distance, pair) = nearest.expect("visible column boundary hint");
                assert!(
                    distance < 12.,
                    "boundary word must have its own rendered hint: {distance}"
                );
                pair
            })
        })
        .unwrap();
    let keys = labels
        .iter()
        .flat_map(|pair| pair.chars())
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    press(cx, handle, &keys);
    let selection = selected(&reader, cx).expect("column range");
    assert_eq!(selection, "Left column ends.\n\nRight");
}

#[gpui_kit::test]
fn positioned_word_gaps_are_shared_by_double_click_hints_copy_and_find(cx: &mut TestAppContext) {
    use gpui_kit::test::ClickOptions;
    use lopdf::{Document, Object, Stream, dictionary};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("positioned-words.pdf");
    let mut pdf = Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let font = pdf.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let content = pdf.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 16 Tf 54 700 Td [(Alpha) -180 (beta) -180 (gamma)] TJ ET".to_vec(),
    ));
    let page = pdf.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content,
    });
    pdf.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let root = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    pdf.trailer.set("Root", root);
    pdf.save(&path).unwrap();

    setup(cx);
    cx.update(|cx| keymap::reload(cx, None, Preset::Vim).unwrap());
    let (handle, reader) = reader_window(cx, Some(path.clone()));
    settle(cx, handle);
    let text = crate::pdf::page_text(&path, 0).unwrap();
    let beta = &text.glyphs[text.word_units(0)[2].start];
    let at = text.transform.document_to_page(crate::coordinates::Point {
        x: beta.start.x + (beta.end.x - beta.start.x) * 0.5,
        y: (beta.top + beta.bottom) * 0.5,
    });
    cx.update_window(handle, |_, window, cx| {
        window.click_with_options(
            "pdf-page-0",
            ClickOptions::new()
                .with_offset(gpui_kit::point(
                    gpui_kit::px(at.x as f32),
                    gpui_kit::px(at.y as f32),
                ))
                .with_count(2),
            cx,
        );
    })
    .unwrap();
    assert_eq!(selected(&reader, cx).as_deref(), Some("beta"));
    begin(cx, handle);
    press(cx, handle, "s a d a secondary-c");
    assert_eq!(selected(&reader, cx).as_deref(), Some("beta gamma"));
    cx.update(|cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("beta gamma")
        )
    });
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowFind), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.input("Alpha beta", cx))
        .unwrap();
    settle(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("find-status").label(), Some("1 match"));
        assert_eq!(
            window.find(("find-result", 0usize)).label(),
            Some("Page 1: Alpha beta gamma")
        );
    })
    .unwrap();
}
