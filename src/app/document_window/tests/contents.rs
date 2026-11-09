//! Contents navigation through the composed document window.

use super::super::sidebar::SidebarPanel;
use super::*;
use crate::app::keymap::install as bind_reader_keys;

#[gpui_kit::test]
fn outline_pointer_and_keyboard_activation_share_document_navigation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) = reader_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-t", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(("contents-entry", 0usize))
                .label()
                .is_some_and(|label| label.ends_with("Landscape"))
        );
        window.click(("contents-entry", 0usize), cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 2);
    reader.update(cx, |reader, cx| reader.previous_page(cx));
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 1);
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index()), 2);
}

#[gpui_kit::test]
fn document_replacement_discards_outline_and_keeps_the_sidebar_open(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, document) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-t", cx);
    })
    .unwrap();
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("contents-empty").visible());
        let empty = window.find("contents-empty").bounds();
        let sidebar = window.find("reader-sidebar").bounds();
        assert!(empty.left() >= sidebar.left() + window.rem_size());
        assert!(empty.right() <= sidebar.right() - window.rem_size());
        assert!(window.try_find(("contents-entry", 0usize)).is_none());
        window.press("enter", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            document.read(cx).sidebar_panel(cx),
            Some(SidebarPanel::Contents)
        );
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.page_count(), Some(100));
    });
}

#[gpui_kit::test]
fn outline_completion_does_not_steal_find_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, _) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-t", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| window.press("cmd-f", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        window.input("landscape", cx);
        assert_eq!(window.find("find-query").value(), Some("landscape"));
    })
    .unwrap();
}
