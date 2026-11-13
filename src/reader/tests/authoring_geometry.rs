//! Actual selection-to-PDF geometry, not hand-written writer rectangles.
use super::authoring::{draw, select_title};
use super::*;
use crate::app::document_window::SaveAs;

fn save(cx: &mut TestAppContext, handle: AnyWindowHandle, output: &Path) {
    cx.update_window(handle, |_, window, cx| {
        for action in [
            Box::new(Highlight) as Box<dyn gpui_kit::Action>,
            Box::new(Underline),
            Box::new(StrikeOut),
            Box::new(Squiggly),
        ] {
            window.dispatch_action(action, cx);
        }
        window.dispatch_action(Box::new(SaveAs), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.to_path_buf()));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn cropped_rotated_production_selections_reopen_at_the_same_document_quads(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    for rotation in [0, 90, 180, 270] {
        let source = directory.path().join(format!("source-{rotation}.pdf"));
        let output = directory.path().join(format!("saved-{rotation}.pdf"));
        let mut fixture = lopdf::Document::load("examples/selection-sample.pdf").unwrap();
        let page = fixture.get_pages()[&1];
        let dictionary = fixture.get_dictionary_mut(page).unwrap();
        dictionary.set(
            "CropBox",
            vec![20.into(), 30.into(), 590.into(), 780.into()],
        );
        dictionary.set("Rotate", rotation);
        fixture.save(&source).unwrap();
        let (window, reader) = recent_test_window(cx, Some(source.clone()));
        cx.run_until_parked();
        draw(cx, window);
        cx.update_window(window, |_, window, cx| {
            window.dispatch_action(Box::new(SinglePage), cx);
            window.dispatch_action(Box::new(FitPage), cx);
        })
        .unwrap();
        cx.run_until_parked();
        select_title(cx, window, &reader, &source);
        let selected = reader.read_with(cx, |reader, _| {
            assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
            reader.selection.document_quads()
        });
        save(cx, window, &output);
        let reopened = pdf::Document::open(&output).unwrap();
        let annotations = reopened.page_annotations(0, 77).unwrap();
        assert_eq!(annotations.len(), 4, "rotation {rotation}");
        for (annotation, kind) in annotations.iter().zip(pdf::annotations::MarkupKind::ALL) {
            assert_eq!(annotation.kind(), kind);
            assert_eq!(annotation.quads().len(), selected.len());
            for (saved, selected) in annotation.quads().iter().zip(&selected) {
                for (saved, selected) in saved.points().into_iter().zip(selected) {
                    assert!((saved.x - selected.x).abs() < 0.001);
                    assert!((saved.y - selected.y).abs() < 0.001);
                }
            }
        }
        reader.read_with(cx, |reader, cx| {
            assert!(!reader.is_dirty(cx));
            assert_eq!(reader.selected_text().as_deref(), Some("Alpha"));
        });
        cx.update_window(window, |_, window, cx| {
            window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn wrapped_column_selection_saves_distinct_regions_after_zoom_and_scrolling(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("columns.pdf");
    let output = directory.path().join("marked-columns.pdf");
    std::fs::copy("examples/field-guide.pdf", &source).unwrap();
    let text = pdf::page_text(&source, 7).unwrap();
    let phrase = |prefix: &str| {
        let range = text
            .lines
            .iter()
            .find(|range| {
                text.glyphs[(*range).clone()]
                    .iter()
                    .map(|glyph| glyph.text.as_str())
                    .collect::<String>()
                    .starts_with(prefix)
            })
            .unwrap();
        let mut bytes = 0;
        let last = text.glyphs[range.clone()]
            .iter()
            .find(|glyph| {
                bytes += glyph.text.len();
                bytes >= prefix.len()
            })
            .unwrap();
        (text.glyphs[range.start].start, last.end)
    };
    let start = phrase("Route and reference points.").0;
    let end = phrase("Reading across columns.").1;
    let (window, reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    draw(cx, window);
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.select_page(7, cx);
        reader.fit_page(cx);
        reader.zoom_in(cx);
        reader.scroll_by(12., cx);
    });
    cx.run_until_parked();
    draw(cx, window);
    let (start, end) = reader.read_with(cx, |reader, _| {
        let viewport = reader.page_viewport(7).unwrap();
        let screen = |point| {
            let point = viewport.document_to_screen(point);
            gpui_kit::point(px(point.x as f32), px(point.y as f32))
        };
        (screen(start), screen(end))
    });
    cx.update_window(window, |_, window, cx| window.drag(start, end, cx))
        .unwrap();
    let quads = reader.read_with(cx, |reader, _| {
        let copied = reader.selected_text().unwrap();
        assert!(copied.starts_with("Route and reference points."));
        assert!(copied.ends_with("Reading across columns."), "{copied}");
        let quads = reader.selection.document_quads();
        assert!(quads.len() > 2);
        for [left, right, _, _] in &quads {
            assert!(
                right.x - left.x < 260.,
                "a stored quad spans the column gutter"
            );
        }
        quads
    });
    save(cx, window, &output);
    let reopened = pdf::Document::open(&output).unwrap();
    let annotations = reopened.page_annotations(7, 77).unwrap();
    assert_eq!(annotations.len(), 4);
    for (annotation, kind) in annotations.iter().zip(pdf::annotations::MarkupKind::ALL) {
        assert_eq!(annotation.kind(), kind);
        assert_eq!(annotation.quads().len(), quads.len());
        for (saved, selected) in annotation.quads().iter().zip(&quads) {
            for (saved, selected) in saved.points().into_iter().zip(selected) {
                assert!((saved.x - selected.x).abs() < 0.001);
                assert!((saved.y - selected.y).abs() < 0.001);
            }
        }
    }
}
