//! Document-window lifecycle and focus; each window owns an independent reader.

use std::path::PathBuf;

use gpui_kit as gpui;
use gpui_kit::component::TitleBar;
use gpui_kit::{
    App, AppContext as _, Bounds, Entity, WeakEntity, Window, WindowBounds, WindowOptions, px, size,
};

use super::document_window::DocumentWindow;
use crate::settings_ui;

gpui::actions!(kver, [CloseWindow, NewWindow, NextWindow, PreviousWindow]);

pub fn install(cx: &mut App) {
    cx.on_action(|_: &NewWindow, cx| {
        if let Err(error) = open(cx, None) {
            eprintln!("Cannot open window: {error}");
        }
    });
    cx.on_window_closed(|cx, _| {
        if should_quit(
            cx.windows().len(),
            settings_ui::state(cx)
                .read(cx)
                .quit_when_last_window_closes(),
        ) {
            cx.quit();
        }
    })
    .detach();
}

fn should_quit(remaining_windows: usize, quit_when_last_window_closes: bool) -> bool {
    // Only macOS has a Dock/reopen path and a persistent app menu when
    // all windows have closed. Do not strand the process elsewhere.
    remaining_windows == 0 && (quit_when_last_window_closes || !cfg!(target_os = "macos"))
}

/// Platform file-open requests can arrive after the launch window has closed.
pub fn open_requested_path(cx: &mut App, initial: &WeakEntity<DocumentWindow>, path: PathBuf) {
    if let Some(document) = initial.upgrade()
        && document.read(cx).reader().read(cx).path().is_none()
    {
        document.update(cx, |document, cx| document.open_path(path, cx));
    } else if let Err(error) = open(cx, Some(path)) {
        eprintln!("Cannot open window: {error}");
    }
    cx.activate(true);
}

/// Dock activation while windowless opens one empty reader.
pub fn reopen(cx: &mut App) {
    if cx.windows().is_empty()
        && let Err(error) = open(cx, None)
    {
        eprintln!("Cannot reopen window: {error}");
    }
}

pub fn open(cx: &mut App, path: Option<PathBuf>) -> Result<Entity<DocumentWindow>, String> {
    let bounds = Bounds::centered(None, size(px(920.0), px(800.0)), cx);
    let (handle, reader) = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(360.0), px(240.0))),
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| cx.new(|cx| DocumentWindow::new(path, window, cx)),
    )
    .map_err(|error| error.to_string())?;
    cx.update_window(handle, |_, window, _| window.activate_window())
        .map_err(|error| error.to_string())?;
    Ok(reader)
}

/// Cycle in stable window order, wrapping in either direction.
pub fn cycle(window: &mut Window, cx: &mut App, step: isize) {
    let windows = cx.windows();
    let Some(index) = windows
        .iter()
        .position(|handle| *handle == window.window_handle())
    else {
        return;
    };
    if windows.len() < 2 {
        return;
    }
    let destination = windows[(index as isize + step).rem_euclid(windows.len() as isize) as usize];
    if let Err(error) = cx.update_window(destination, |_, window, _| window.activate_window()) {
        eprintln!("Cannot activate window: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[test]
    fn last_close_policy_distinguishes_multiple_windows_and_platform_reopen() {
        assert!(!should_quit(2, true));
        assert!(!should_quit(1, true));
        assert!(should_quit(0, true));
        assert_eq!(should_quit(0, false), !cfg!(target_os = "macos"));
    }

    #[gpui_kit::test]
    fn new_window_action_works_without_an_active_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx);
            cx.dispatch_action(&NewWindow);
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        cx.update(|cx| cx.dispatch_action(&NewWindow));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    }

    #[gpui_kit::test]
    fn platform_opens_reuse_only_the_empty_initial_document_window(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let initial = cx.update(|cx| open(cx, None).unwrap());
        let reader = initial.read_with(cx, |document, _| document.reader().clone());
        let first = PathBuf::from("examples/readme-sample.pdf");
        cx.update(|cx| open_requested_path(cx, &initial.downgrade(), first.clone()));
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.path(), Some(first.as_path()))
        });
        cx.update(|cx| {
            open_requested_path(
                cx,
                &initial.downgrade(),
                PathBuf::from("examples/missing.pdf"),
            );
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 2);
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.path(), Some(first.as_path()));
            assert_eq!(reader.page_count(), Some(100));
        });
    }

    #[gpui_kit::test]
    fn platform_reopen_and_file_request_recover_from_zero_windows(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let initial = cx.update(|cx| open(cx, None).unwrap().downgrade());
        cx.update(|cx| {
            let handle = cx.windows()[0];
            cx.update_window(handle, |_, window, _| window.remove_window())
                .unwrap();
        });
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().is_empty()));
        assert!(initial.upgrade().is_none());

        let path = std::path::PathBuf::from("examples/readme-sample.pdf");
        cx.update(|cx| open_requested_path(cx, &initial, path));
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        cx.update(reopen);
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        cx.update(|cx| {
            let handle = cx.windows()[0];
            cx.update_window(handle, |_, window, _| window.remove_window())
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(reopen);
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    }
}
