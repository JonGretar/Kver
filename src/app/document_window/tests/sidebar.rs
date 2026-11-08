//! Sidebar layout must still drive the PDF viewport after host extraction.

use super::super::sidebar::SidebarPanel;
use super::*;
use crate::{app::keymap::install as bind_reader_keys, reader::ZoomMode};
use gpui_kit::{point, px, test::TestEventExt as _};

#[gpui_kit::test]
fn sidebar_spans_the_reading_chrome_but_not_the_title_bar(cx: &mut TestAppContext) {
    setup(cx);
    let (window, _) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-f", cx);
        frame.render_frame(cx);
        let workspace = frame.find("reader-workspace").bounds();
        let sidebar = frame.find("reader-sidebar").bounds();
        let toolbar_control = frame.find("toggle-sidebar").bounds();
        let page = frame.find("page").bounds();
        let status = frame.find("reader-status").bounds();
        assert_eq!(sidebar.origin.y, workspace.origin.y);
        assert_eq!(sidebar.size.height, workspace.size.height);
        assert!(toolbar_control.origin.y >= sidebar.origin.y);
        assert!(frame.find("document-name").bounds().bottom() <= sidebar.origin.y);
        assert!(page.origin.y >= toolbar_control.bottom());
        assert!(page.bottom() <= status.origin.y);
        assert!(status.bottom() <= sidebar.bottom());
    })
    .unwrap();
}

#[gpui_kit::test]
fn resizing_sidebar_refits_pdf_and_preserves_panel_across_documents(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-f", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-alt-w", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let before = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find("reader-sidebar").bounds()
        })
        .unwrap();
    let initial_scale = reader.read_with(cx, |reader, _| reader.scale());
    let start = point(before.right() - px(1.), before.origin.y + px(80.));
    cx.update_window(handle, |_, window, cx| {
        window.pointer_down(start, gpui_kit::MouseButton::Left, cx);
        window.pointer_move(
            start + point(px(12.), px(0.)),
            Some(gpui_kit::MouseButton::Left),
            cx,
        );
        window.pointer_move(
            start + point(px(64.), px(0.)),
            Some(gpui_kit::MouseButton::Left),
            cx,
        );
        window.pointer_up(
            start + point(px(64.), px(0.)),
            gpui_kit::MouseButton::Left,
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    let resized = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find("reader-sidebar").bounds()
        })
        .unwrap();
    assert!(resized.size.width > before.size.width + px(32.));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
        assert!(reader.scale() < initial_scale);
    });
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("reader-sidebar").bounds().size.width,
            resized.size.width
        );
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
}
