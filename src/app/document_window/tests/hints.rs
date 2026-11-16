use super::*;
use crate::reader::{HintEvent, ReadingMode};
use std::{cell::RefCell, rc::Rc};

fn draw(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

fn begin(cx: &mut TestAppContext, handle: AnyWindowHandle, reader: &Entity<ReaderView>) {
    cx.update_window(handle, |_, window, cx| {
        reader.update(cx, |reader, cx| {
            reader.begin_word_hints(|_| true, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    draw(cx, handle);
    cx.run_until_parked();
    draw(cx, handle);
}

#[gpui_kit::test]
fn hints_choose_without_selection_and_preserve_snapshot_in_both_layouts(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let chosen = Rc::new(RefCell::new(Vec::new()));
    document.update(cx, |_, cx| {
        let chosen = chosen.clone();
        cx.subscribe(&reader, move |_, _, event: &HintEvent, _| {
            if let HintEvent::Chosen { snapshot, index } = event {
                chosen.borrow_mut().push((snapshot.clone(), *index));
            }
        })
        .detach();
    });
    for mode in [ReadingMode::Continuous, ReadingMode::SinglePage] {
        reader.update(cx, |reader, cx| reader.set_reading_mode(mode, cx));
        draw(cx, handle);
        begin(cx, handle, &reader);
        cx.update_window(handle, |_, window, cx| {
            assert!(window.find("hint-aa").visible());
            window.press("a", cx);
            assert_eq!(
                window.find("hint-input").label(),
                Some("Type two letters: a_")
            );
            window.press("backspace", cx);
            assert_eq!(
                window.find("hint-input").label(),
                Some("Type two letters: __")
            );
            window.press("z", cx);
            window.press("z", cx);
            assert_eq!(
                window.find("hint-input").label(),
                Some("No matching hint. Try again.")
            );
            window.press("s", cx);
            window.press("a", cx);
            assert!(window.try_find("hint-input").is_none());
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            reader.read_with(cx, |reader, _| reader.selected_text()),
            None
        );
    }
    let events = chosen.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].1, 1);
    assert_eq!(events[0].0.labels()[1].letters(), "sa");
    assert_eq!(
        events[0].0.labels()[1].target().id().glyphs(),
        events[1].0.labels()[1].target().id().glyphs()
    );
}

#[gpui_kit::test]
fn hints_cancel_on_input_focus_scroll_zoom_resize_and_do_not_reopen(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    draw(cx, handle);
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
    begin(cx, handle, &reader);
    reader.update(cx, |reader, cx| reader.scroll_by(10., cx));
    draw(cx, handle);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
    begin(cx, handle, &reader);
    reader.update(cx, |reader, cx| reader.zoom_in(cx));
    draw(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
    begin(cx, handle, &reader);
    cx.simulate_window_resize(
        handle,
        gpui_kit::size(gpui_kit::px(720.), gpui_kit::px(500.)),
    );
    draw(cx, handle);
    cx.run_until_parked();
    draw(cx, handle);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
    // Find owns focus. Hints cannot start from there or consume its letters.
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowFind), cx)
    })
    .unwrap();
    cx.run_until_parked();
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn hint_focus_yields_to_find_palette_and_recent_dialog(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    draw(cx, handle);
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        window.press("secondary-f", cx);
        window.input("az", cx);
        assert_eq!(window.find("find-query").value(), Some("az"));
        assert_eq!(window.find("find-query").focused(), Some(true));
        assert!(window.try_find("hint-input").is_none());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowCommandPalette), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.input("az", cx);
        assert!(window.try_find("hint-input").is_none());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowRecent), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.input("book", cx);
        assert_eq!(window.find("recent-panel").label(), Some("Open Recent"));
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn caller_subset_spans_visible_pages_and_ui_scale_keeps_labels_on_page(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some("examples/readme-sample.pdf".into()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        reader.zoom_out(cx);
        reader.zoom_out(cx);
    });
    draw(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        reader.update(cx, |reader, cx| {
            reader.begin_word_hints(|word| word.glyphs().start == 0, window, cx)
        });
    })
    .unwrap();
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx, handle);
    }
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("hint-aa").visible());
        assert!(window.find("hint-sa").visible());
        assert!(window.try_find("hint-da").is_none());
        window.press("escape", cx);
    })
    .unwrap();
    // The inset previously stayed at 16 logical points while p_4 scaled.
    cx.update(|cx| {
        gpui_kit::component::theme::Theme::update(cx, |theme| theme.font_size = gpui_kit::px(20.))
    });
    draw(cx, handle);
    let (other_handle, other_reader) =
        reader_window(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    draw(cx, other_handle);
    cx.update_window(other_handle, |_, window, cx| {
        other_reader.update(cx, |reader, cx| {
            reader.begin_word_hints(|word| word.glyphs().start == 0, window, cx)
        });
    })
    .unwrap();
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx, other_handle);
    }
    cx.update_window(other_handle, |_, window, _| {
        let page = window.find("pdf-page-0").bounds();
        let label = window.find("hint-aa").bounds();
        assert!(page.contains(&label.origin));
        assert!(page.contains(&label.bottom_right()));
    })
    .unwrap();
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("hint-input").is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn sparse_words_remain_targetable_with_larger_ui_text(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
        gpui_kit::component::theme::Theme::update(cx, |theme| theme.font_size = gpui_kit::px(20.))
    });
    let (handle, reader) = reader_window(cx, Some("examples/selection-sample.pdf".into()));
    cx.run_until_parked();
    draw(cx, handle);
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, _| {
        assert!(window.find("hint-aa").visible());
        assert!(window.find("hint-la").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn enlarged_dense_page_allows_legible_visible_word_hints(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        reader.select_page(7, cx);
        for _ in 0..4 {
            reader.zoom_in(cx);
        }
    });
    for width in [1536., 920.] {
        cx.simulate_window_resize(
            handle,
            gpui_kit::size(gpui_kit::px(width), gpui_kit::px(800.)),
        );
        for _ in 0..3 {
            cx.run_until_parked();
            draw(cx, handle);
        }
        begin(cx, handle, &reader);
        cx.update_window(handle, |_, window, cx| {
            assert_eq!(
                window.find("hint-input").label(),
                Some("Type two letters: __")
            );
            assert!(window.find("hint-aa").visible());
            window.press("escape", cx);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn dense_page_filters_and_chooses_without_reassigning_labels(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let chosen = Rc::new(RefCell::new(None));
    document.update(cx, |_, cx| {
        let chosen = chosen.clone();
        cx.subscribe(&reader, move |_, _, event: &HintEvent, _| {
            if let HintEvent::Chosen { snapshot, index } = event {
                *chosen.borrow_mut() = Some((snapshot.clone(), *index));
            }
        })
        .detach();
    });
    reader.update(cx, |reader, cx| reader.select_page(7, cx));
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx, handle);
    }
    begin(cx, handle, &reader);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("hint-aa").visible());
        assert!(window.find("hint-sa").visible());
        window.press("s", cx);
        assert!(window.try_find("hint-aa").is_none());
        assert!(window.find("hint-sa").visible());
        window.press("a", cx);
        assert!(window.try_find("hint-input").is_none());
    })
    .unwrap();
    let chosen = chosen.borrow();
    let (snapshot, index) = chosen.as_ref().expect("chosen dense-page target");
    assert!(snapshot.labels().len() > 26);
    assert_eq!(*index, 1);
    assert_eq!(snapshot.labels()[*index].letters(), "sa");
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.selected_text()),
        None
    );
}
