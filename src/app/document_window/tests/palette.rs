use super::super::sidebar::SidebarPanel;
use super::*;
use crate::{app::keymap::install as bind_reader_keys, reader::ZoomMode};

#[gpui_kit::test]
fn vim_palette_and_find_keys_stay_in_the_reader_context(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
        crate::app::keymap::select_preset(cx, crate::app::keymap::Preset::Vim).unwrap();
    });
    let (window, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press(":", cx);
    })
    .unwrap();
    assert!(document.read_with(cx, |document, _| document.palette_open()));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("escape", cx);
    })
    .unwrap();
    assert!(!document.read_with(cx, |document, _| document.palette_open()));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("/", cx);
        frame.render_frame(cx);
        assert_eq!(frame.find("find-query").focused(), Some(true));
        frame.press(":", cx);
        assert!(!document.read(cx).palette_open());
    })
    .unwrap();
}

#[gpui_kit::test]
fn unavailable_palette_commands_do_not_run_and_escape_restores_reader_focus(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, document) = open(cx, None);
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
    document.read_with(cx, |document, cx| {
        assert!(!document.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    document.read_with(cx, |document, cx| {
        assert!(document.palette_open());
        assert_eq!(document.reader().read(cx).page_count(), None);
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    document.read_with(cx, |document, cx| {
        assert!(document.palette_open());
        assert_eq!(document.palette_query(cx).as_deref(), Some(""));
    });
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert!(!document.read_with(cx, |document, _| document.palette_open()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            document
                .read(cx)
                .reader()
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_runs_reader_action_then_returns_focus_to_the_open_sidebar(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
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
    document.read_with(cx, |document, cx| {
        assert!(!document.palette_open());
        let reader = document.reader().read(cx);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
        assert_eq!(document.sidebar_panel(cx), Some(SidebarPanel::Find));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        window.input("KVER", cx);
        assert_eq!(window.find("find-query").value(), Some("KVER"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_and_reader_keys_dispatch_once_to_the_owning_reader(cx: &mut TestAppContext) {
    setup(cx);
    let (first_window, first) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let (_, second) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(first_window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.press("cmd-shift-p", cx);
        window.render_frame(cx);
        window.input("Next Page", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(first_window, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 1);
        assert!(!document.palette_open());
    });
    second.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 0)
    });
    cx.update_window(first_window, |_, window, cx| {
        window.render_frame(cx);
        window.press("right", cx);
    })
    .unwrap();
    first.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 2)
    });
    cx.update_window(first_window, |_, window, cx| {
        window.press("cmd-shift-p", cx);
        window.render_frame(cx);
        window.input("n", cx);
        window.press("down", cx);
    })
    .unwrap();
    first.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_index(), 2);
        assert_eq!(document.palette_query(cx).as_deref(), Some("n"));
    });
}

#[gpui_kit::test]
fn window_overlays_keep_full_window_bounds_above_the_reader_workspace(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    setup(cx);
    cx.update(|cx| {
        crate::reading_history::install(
            cx,
            Some(crate::settings::AppPaths::for_locations(
                root.path().join("config.toml"),
                root.path().join("data"),
            )),
        )
    });
    let (handle, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let workspace = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("document-name").label(),
                Some("readme-sample.pdf")
            );
            let root = window.find("document-window").bounds();
            let workspace = window.find("reader-workspace").bounds();
            assert!(workspace.origin.y > root.origin.y);
            window.press("cmd-shift-p", cx);
            window.render_frame(cx);
            assert_eq!(window.find("reader-command-palette").bounds(), root);
            assert_eq!(window.find("reader-workspace").bounds(), workspace);
            workspace
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-m", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("reader-marks-palette").bounds(),
            window.find("document-window").bounds()
        );
        assert_eq!(window.find("reader-workspace").bounds(), workspace);
    })
    .unwrap();
    assert!(!document.read_with(cx, |document, _| document.palette_open()));
    cx.simulate_window_resize(
        handle,
        gpui_kit::size(gpui_kit::px(620.), gpui_kit::px(600.)),
    );
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("reader-marks-palette").bounds(),
            window.find("document-window").bounds()
        );
        window.press("escape", cx);
    })
    .unwrap();
    document.update(cx, |document, cx| {
        document.open_path(PathBuf::from("examples/missing.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("document-name").label(), Some("missing.pdf"));
    })
    .unwrap();
}
