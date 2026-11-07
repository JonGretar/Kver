//! Find and sidebar workflows through the composed reader window.

use super::*;

#[gpui_kit::test]
fn single_page_find_jump_and_window_modes_are_local(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let first = cx.add_window(|frame, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            frame.scale_factor(),
            cx,
        )
    });
    let second = cx.add_window(|frame, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            frame.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(first.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    first
        .update(cx, |reader, frame, cx| {
            reader.set_reading_mode(ReadingMode::SinglePage, cx);
            reader.show_sidebar(SidebarPanel::Find, frame, cx);
        })
        .unwrap();
    cx.update_window(first.into(), |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("This is page 3 of 100", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(first.into(), |_, frame, cx| {
        frame.render_frame(cx);
        assert!(
            frame
                .find(("find-result", 0usize))
                .label()
                .unwrap()
                .starts_with("Page 3:")
        );
        frame.click(("find-result", 0usize), cx);
    })
    .unwrap();
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 2);
            assert_eq!(reader.reading_position().0, 2);
            assert_eq!(reader.visible_pages(0.0, 700.0), 2..3);
            assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        })
        .unwrap();
    second
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 0);
            assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        })
        .unwrap();
    first
        .update(cx, |reader, _, cx| {
            reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        })
        .unwrap();
    cx.run_until_parked();
    let destination = pdf::document_outline(Path::new("examples/field-guide.pdf")).unwrap()[0]
        .destination()
        .unwrap();
    first
        .update(cx, |reader, _, cx| {
            // A new document starts with the configured default view.
            assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
            reader.set_reading_mode(ReadingMode::SinglePage, cx);
            reader.navigate_outline(destination, cx);
        })
        .unwrap();
    first
        .read_with(cx, |reader, _| {
            assert_eq!(reader.page_index(), 2);
            assert_eq!(reader.reading_position().0, 2);
            assert_eq!(reader.visible_pages(0.0, 700.0), 2..3);
        })
        .unwrap();
    cx.run_until_parked();
    first
        .read_with(cx, |reader, _| {
            assert!(reader.images.keys().all(|page| *page == 2));
            assert!(reader.retained_bitmap_bytes() <= BITMAP_BUDGET);
        })
        .unwrap();
}

#[gpui_kit::test]
fn reader_sidebar_opens_and_remembers_its_panel(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let window = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| {
            frame.render_frame(cx);
            frame.click("toggle-sidebar", cx);
        })
        .unwrap();
    });
    window
        .read_with(cx, |reader, cx| {
            assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Contents));
        })
        .unwrap();

    cx.simulate_keystrokes(window.into(), "cmd-f");
    window
        .read_with(cx, |reader, cx| {
            assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Find));
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "cmd-b cmd-b");
    window
        .read_with(cx, |reader, cx| {
            assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Find));
        })
        .unwrap();
}

#[gpui_kit::test]
fn find_query_survives_closing_and_reopening_the_sidebar(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let window = cx.add_window(|window, cx| {
        Preview::new(
            Some(PathBuf::from("examples/readme-sample.pdf")),
            window.scale_factor(),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| frame.render_frame(cx))
            .unwrap();
    });
    cx.simulate_keystrokes(window.into(), "cmd-f");
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| {
            frame.render_frame(cx);
            assert_eq!(frame.find("find-query").focused(), Some(true));
            frame.input("KVER", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(window.into(), "cmd-b cmd-b");
    cx.update(|cx| {
        cx.update_window(window.into(), |_, frame, cx| {
            frame.render_frame(cx);
            assert_eq!(frame.find("find-query").value(), Some("KVER"));
            assert_eq!(frame.find("find-query").focused(), Some(true));
            assert!(
                frame
                    .find("find-status")
                    .label()
                    .unwrap()
                    .contains("matches")
            );
        })
        .unwrap();
    });
}

fn find_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Preview>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-f", cx);
    })
    .unwrap();
    (window, reader)
}

#[gpui_kit::test]
fn find_keyboard_pointer_and_highlight_share_selection(cx: &mut TestAppContext) {
    let (window, reader) = find_window(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.input("This is page", cx);
        // Enter can arrive before a batch has produced any results.
        frame.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(
            frame
                .find("find-status")
                .label()
                .unwrap()
                .starts_with("1 /")
        );
        assert!(frame.find("find-highlight").visible());
        frame.press("cmd-g", cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 1);
    cx.update_window(window, |_, frame, cx| frame.press("cmd-shift-g", cx))
        .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click(("find-result", 2usize), cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 2);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(
            frame
                .find("find-status")
                .label()
                .unwrap()
                .starts_with("3 /")
        );
        assert!(frame.find("find-highlight").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn replacing_query_and_document_discards_old_find_work(cx: &mut TestAppContext) {
    let (window, reader) = find_window(cx);
    cx.update_window(window, |_, frame, cx| frame.input("This is page", cx))
        .unwrap();
    cx.update_window(window, |_, frame, cx| {
        frame.press("cmd-a", cx);
        frame.input("not-a-phrase-in-this-pdf", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(
            frame.find("find-status").label(),
            Some("No matches in mapped text")
        );
        assert!(frame.try_find(("find-result", 0usize)).is_none());
        frame.press("cmd-a", cx);
        frame.input("This is page 99 of 100", cx);
        frame.press("enter", cx);
        reader.update(cx, |reader, cx| {
            reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("find-query").value(), Some(""));
        assert!(frame.try_find(("find-result", 0usize)).is_none());
        assert!(frame.try_find("find-highlight").is_none());
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 0);
}
