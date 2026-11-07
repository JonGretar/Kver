use super::*;

#[gpui_kit::test]
fn unavailable_palette_commands_do_not_run_and_escape_restores_reader_focus(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, reader) = recent_test_window(cx, None);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-shift-p", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input("Next Page", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.palette_open());
        assert_eq!(reader.page_count(), None);
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    reader.read_with(cx, |reader, cx| {
        assert!(reader.palette_open());
        assert_eq!(reader.palette_query(cx).as_deref(), Some(""));
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert!(!reader.read_with(cx, |reader, _| reader.palette_open()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(reader.read(cx).focus.is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_runs_reader_action_then_returns_focus_to_the_open_sidebar(cx: &mut TestAppContext) {
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
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-p", cx))
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input("Fit Width", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.palette_open());
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
        assert_eq!(reader.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        window.input("KVER", cx);
        assert_eq!(window.find("find-query").value(), Some("KVER"));
    })
    .unwrap();
}
