//! Application composition: platform entry points, shared state, and windows.
//!
//! Features own actions and behavior. Startup installs the application keymap
//! before native menus snapshot its shortcuts.

pub mod commands;
pub mod document_window;
pub mod keymap;
pub mod menu;
mod onboarding;
pub(crate) mod quit_guard;
pub(crate) mod shortcut_label;
pub(crate) mod theme;
pub mod windows;

use std::path::PathBuf;

use gpui_kit::{App, AppContext as _, platform};

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
    application.on_reopen(|cx| {
        // Dock activation during onboarding must not bypass first-run setup.
        if cx.has_global::<StartupReady>() {
            windows::reopen(cx);
        } else {
            cx.activate(true);
        }
    });
    application.run(move |cx: &mut App| {
        initialize_ui(cx);
        cx.spawn(async move |cx| {
            let paths = AppPaths::resolve();
            let config = paths.as_ref().map(|paths| paths.config().to_path_buf());
            let first_run = cx
                .background_spawn(async move {
                    config
                        .map(|path| crate::settings::create_if_missing(&path))
                        .transpose()
                })
                .await;
            let mut startup_error = None;
            match (first_run, &paths) {
                (Ok(Some(true)), Some(paths)) => {
                    let result = cx.update(|cx| onboarding::open(paths.config().to_path_buf(), cx));
                    match result {
                        Ok((_, finished)) => {
                            let _ = finished.recv().await;
                        }
                        Err(error) => startup_error = Some(format!("Cannot open Welcome: {error}")),
                    }
                }
                (Err(error), _) => startup_error = Some(error),
                _ => {}
            }
            let initial = cx.update(|cx| start_reader(cx, paths, path, startup_error));
            let Some(initial) = initial else { return };
            while let Ok(path) = incoming.recv().await {
                cx.update(|cx| windows::open_requested_path(cx, &initial, path));
            }
        })
        .detach();
        cx.activate(true);
    });
}

fn start_reader(
    cx: &mut App,
    paths: Option<AppPaths>,
    path: Option<PathBuf>,
    startup_error: Option<String>,
) -> Option<gpui_kit::WeakEntity<document_window::DocumentWindow>> {
    gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
    initialize_services(cx, paths);
    cx.set_global(StartupReady);
    if let Err(error) = quit_guard::install(cx) {
        eprintln!("Cannot protect unsaved work during native Quit: {error}");
        cx.quit();
        return None;
    }

    let initial = match windows::open(cx, path) {
        Ok(reader) => reader.downgrade(),
        Err(error) => {
            eprintln!("Cannot open window: {error}");
            cx.quit();
            return None;
        }
    };
    if let Some(error) = startup_error {
        use gpui_kit::component::{WindowExt as _, notification::Notification};
        if let Some(handle) = cx.active_window() {
            let _ = cx.update_window(handle, |_, window, cx| {
                window.push_notification(
                    Notification::warning(format!(
                        "{error}. Check the configuration directory permissions and try again."
                    )),
                    cx,
                );
            });
        }
    }
    Some(initial)
}

struct StartupReady;
impl gpui_kit::Global for StartupReady {}

#[cfg(test)]
fn initialize(cx: &mut App, paths: Option<AppPaths>) {
    initialize_ui(cx);
    initialize_services(cx, paths);
}

fn initialize_ui(cx: &mut App) {
    gpui_kit::init(cx);
    theme::install(cx);
    gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
}

fn initialize_services(cx: &mut App, paths: Option<AppPaths>) {
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
