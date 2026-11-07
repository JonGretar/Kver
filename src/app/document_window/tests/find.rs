//! Find and sidebar workflows through the composed document window.

use super::super::sidebar::SidebarPanel;
use super::*;

#[gpui_kit::test]
fn reader_and_palette_match_actions_reach_the_window_owned_find(cx: &mut TestAppContext) {
    setup(cx);
    let (window, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-f", cx);
        frame.render_frame(cx);
        frame.input("This is page", cx);
        frame.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("escape", cx);
        frame.render_frame(cx);
        frame.press("cmd-g", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.sidebar_panel(cx), None);
        assert_eq!(document.reader().read(cx).page_index(), 1);
    });
    cx.update_window(window, |_, frame, cx| {
        frame.press("cmd-shift-p", cx);
        frame.render_frame(cx);
        frame.input("Next match", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(!document.palette_open());
        assert_eq!(document.reader().read(cx).page_index(), 2);
        assert_eq!(document.sidebar_panel(cx), None);
    });
}

#[gpui_kit::test]
fn reader_sidebar_opens_and_remembers_its_panel(cx: &mut TestAppContext) {
    setup(cx);
    let (window, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("toggle-sidebar", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.sidebar_panel(cx), Some(SidebarPanel::Contents));
    });
    cx.simulate_keystrokes(window, "cmd-f");
    document.read_with(cx, |document, cx| {
        assert_eq!(document.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
    cx.simulate_keystrokes(window, "cmd-b cmd-b");
    document.read_with(cx, |document, cx| {
        assert_eq!(document.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
}

#[gpui_kit::test]
fn find_query_survives_closing_and_reopening_the_sidebar(cx: &mut TestAppContext) {
    setup(cx);
    let (window, _) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.simulate_keystrokes(window, "cmd-f");
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("find-query").focused(), Some(true));
        frame.input("KVER", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "cmd-b cmd-b");
    cx.update_window(window, |_, frame, cx| {
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
}

fn find_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<ReaderView>) {
    setup(cx);
    let (window, reader) = reader_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
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
            reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
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
