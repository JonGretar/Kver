//! Reader window lifecycle and focus; document state remains in each Preview.

use std::path::PathBuf;

use gpui::{App, AppContext as _, Bounds, Window, WindowBounds, WindowOptions, px, size};

use crate::reader::Preview;

gpui::actions!(kver, [NewWindow, NextWindow, PreviousWindow]);

pub fn open(cx: &mut App, path: Option<PathBuf>) -> Result<(), String> {
    let bounds = Bounds::centered(None, size(px(920.0), px(800.0)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Preview::new(path, window.scale_factor(), cx)),
        )
        .map_err(|error| error.to_string())?;
    handle
        .update(cx, |_, window, _| window.activate_window())
        .map_err(|error| error.to_string())?;
    Ok(())
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
