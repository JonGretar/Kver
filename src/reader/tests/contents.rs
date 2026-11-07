//! Contents navigation through the composed document window.

use super::*;

#[gpui_kit::test]
fn outline_pointer_and_keyboard_activation_share_document_navigation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-t", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(("contents-entry", 0usize)).label(),
            Some("1 Landscape")
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
    let (handle, reader) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
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
        assert!(window.try_find(("contents-entry", 0usize)).is_none());
        window.press("enter", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Contents));
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
    let (handle, _) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
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
