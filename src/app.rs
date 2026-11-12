//! Application composition: platform entry points, shared state, and windows.
//!
//! Features own actions and behavior. Startup installs the application keymap
//! before native menus snapshot its shortcuts.

pub mod commands;
pub mod document_window;
pub mod keymap;
pub mod menu;
pub(crate) mod quit_guard;
pub(crate) mod shortcut_label;
pub(crate) mod theme;
pub mod windows;

use std::path::PathBuf;

use gpui_kit::{App, platform};

use crate::{assets::ReaderAssets, reading_history, settings::AppPaths, settings_ui};

/// Launch Kver with an optional initial PDF.
pub fn run(path: Option<PathBuf>) {
    // Register before launching so Finder's initial open event is not lost.
    let (requests, incoming) = async_channel::unbounded();
    let application = platform::application().with_assets(ReaderAssets);
    application.on_open_urls(move |urls| {
        for url in urls {
            if let Some(path) = local_path(&url) {
                let _ = requests.try_send(path);
            }
        }
    });
    application.on_reopen(windows::reopen);
    application.run(move |cx: &mut App| {
        initialize(cx, AppPaths::resolve());
        if let Err(error) = quit_guard::install(cx) {
            eprintln!("Cannot protect unsaved work during native Quit: {error}");
            cx.quit();
            return;
        }

        let initial = match windows::open(cx, path) {
            Ok(reader) => reader.downgrade(),
            Err(error) => {
                eprintln!("Cannot open window: {error}");
                cx.quit();
                return;
            }
        };
        cx.spawn(async move |cx| {
            while let Ok(path) = incoming.recv().await {
                cx.update(|cx| windows::open_requested_path(cx, &initial, path));
            }
        })
        .detach();
        cx.activate(true);
    });
}

fn initialize(cx: &mut App, paths: Option<AppPaths>) {
    gpui_kit::init(cx);
    theme::install(cx);
    gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
    keymap::install_with_paths(cx, keymap::Preset::default(), paths.clone());
    windows::install(cx);
    settings_ui::install_with_paths(cx, paths.clone());
    reading_history::install(cx, paths);
    menu::install(cx);
}

fn local_path(url: &str) -> Option<PathBuf> {
    url::Url::parse(url).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests;
