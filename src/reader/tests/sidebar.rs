//! Sidebar layout must still drive the PDF viewport after host extraction.

use super::*;

#[gpui_kit::test]
fn resizing_sidebar_refits_pdf_and_preserves_panel_across_documents(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
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
        window.pointer_down(start, gpui::MouseButton::Left, cx);
        window.pointer_move(
            start + point(px(12.), px(0.)),
            Some(gpui::MouseButton::Left),
            cx,
        );
        window.pointer_move(
            start + point(px(64.), px(0.)),
            Some(gpui::MouseButton::Left),
            cx,
        );
        window.pointer_up(start + point(px(64.), px(0.)), gpui::MouseButton::Left, cx);
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
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
}
