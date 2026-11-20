//! Document-window lifecycle and focus; each window owns an independent reader.

use std::path::PathBuf;

use gpui_kit as gpui;
use gpui_kit::component::TitleBar;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Global, WeakEntity, Window,
    WindowBounds, WindowOptions, px, size,
};

use super::document_window::DocumentWindow;
use crate::reader::DocumentSource;
use crate::{settings::OpenFiles, settings_ui};

gpui::actions!(kver, [CloseWindow, NewWindow, NextWindow, PreviousWindow]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OpenMode {
    #[default]
    Default,
    New,
    Reuse,
}

impl OpenMode {
    pub(crate) fn resolve(self, cx: &mut App) -> OpenFiles {
        match self {
            Self::Default => settings_ui::state(cx).read(cx).open_files(),
            Self::New => OpenFiles::NewWindow,
            Self::Reuse => OpenFiles::Reuse,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpenTarget {
    File(PathBuf),
    Url(url::Url),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenRequest {
    pub(crate) target: Option<OpenTarget>,
    pub(crate) mode: OpenMode,
}

#[derive(Default)]
struct DocumentWindows(Vec<(AnyWindowHandle, WeakEntity<DocumentWindow>)>);
impl Global for DocumentWindows {}

pub(crate) fn register_document(
    handle: AnyWindowHandle,
    document: WeakEntity<DocumentWindow>,
    cx: &mut App,
) {
    if !cx.has_global::<DocumentWindows>() {
        cx.set_global(DocumentWindows::default());
    }
    let windows = &mut cx.global_mut::<DocumentWindows>().0;
    windows.retain(|(_, document)| document.upgrade().is_some());
    windows.push((handle, document));
}

pub(crate) fn note_activation(handle: AnyWindowHandle, cx: &mut App) {
    if !cx.has_global::<DocumentWindows>() {
        return;
    }
    let windows = cx.global_mut::<DocumentWindows>();
    if let Some(ix) = windows.0.iter().position(|(window, _)| *window == handle) {
        let document = windows.0.remove(ix);
        windows.0.push(document);
    }
}

fn most_recent(cx: &App) -> Option<(AnyWindowHandle, Entity<DocumentWindow>)> {
    cx.try_global::<DocumentWindows>()?
        .0
        .iter()
        .rev()
        .filter(|(handle, _)| cx.windows().contains(handle))
        .find_map(|(handle, document)| document.upgrade().map(|document| (*handle, document)))
}

pub(crate) fn open_request(
    cx: &mut App,
    initial: &WeakEntity<DocumentWindow>,
    request: OpenRequest,
) {
    let reuse = request.mode.resolve(cx) == OpenFiles::Reuse;
    if let Some(target) = request.target {
        let existing = if reuse {
            most_recent(cx).map(|(_, document)| document)
        } else {
            initial
                .upgrade()
                .filter(|document| request.mode != OpenMode::New && document.read(cx).is_empty(cx))
        };
        match existing.map_or_else(|| open(cx, None), Ok) {
            Ok(document) => {
                let handle = document.read(cx).window_handle();
                let _ = cx.update_window(handle, |_, window, cx| {
                    window.activate_window();
                    document.update(cx, |document, cx| match target {
                        OpenTarget::File(path) => document.open_path(path, cx),
                        OpenTarget::Url(url) => {
                            document.open_remote_pdf(url, OpenMode::Reuse, false, cx)
                        }
                    });
                });
            }
            Err(error) => eprintln!("Cannot open window: {error}"),
        }
    } else if request.mode != OpenMode::New
        && let Some((handle, _)) = most_recent(cx)
    {
        if let Err(error) = cx.update_window(handle, |_, window, _| window.activate_window()) {
            eprintln!("Cannot activate window: {error}");
        }
    } else if let Err(error) = open(cx, None) {
        eprintln!("Cannot open window: {error}");
    }
    cx.activate(true);
}

pub(crate) fn request_quit(cx: &mut App) {
    // Menu/keyboard actions may still be dispatching inside the target window.
    // GPUI temporarily removes that window from its registry during dispatch;
    // only discover/update it after the current effect returns it to the app.
    cx.defer(|cx| {
        let dirty = cx.try_global::<DocumentWindows>().and_then(|windows| {
            windows.0.iter().find_map(|(handle, document)| {
                let document = document.upgrade()?;
                let reader = document.read(cx).reader().read(cx);
                (reader.is_dirty(cx) || reader.is_saving(cx)).then_some((*handle, document))
            })
        });
        if let Some((handle, document)) = dirty {
            if let Err(error) = cx.update_window(handle, |_, window, cx| {
                window.activate_window();
                document.update(cx, |document, cx| document.request_quit(window, cx));
            }) {
                eprintln!("Cannot show the unsaved-work decision: {error}");
            }
        } else {
            super::quit_guard::finish_quit(cx);
        }
    });
}

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
    open_request(
        cx,
        initial,
        OpenRequest {
            target: Some(OpenTarget::File(path)),
            mode: OpenMode::Default,
        },
    );
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
    open_source(cx, path.map(DocumentSource::File), None)
}

pub(crate) fn open_link(
    cx: &mut App,
    path: PathBuf,
    page: usize,
) -> Result<Entity<DocumentWindow>, String> {
    open_source(cx, Some(DocumentSource::File(path)), Some(page))
}

pub(super) fn open_source(
    cx: &mut App,
    source: Option<DocumentSource>,
    page: Option<usize>,
) -> Result<Entity<DocumentWindow>, String> {
    let bounds = Bounds::centered(None, size(px(920.0), px(800.0)), cx);
    let (handle, reader) = gpui_kit::open_window(
        WindowOptions {
            #[cfg(target_os = "linux")]
            app_id: Some(crate::app::LINUX_APP_ID.into()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(360.0), px(240.0))),
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| cx.new(|cx| DocumentWindow::new_with_page(source, page, window, cx)),
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

    #[gpui_kit::test]
    fn cli_reuses_most_recent_reader_even_when_an_auxiliary_window_is_active(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let first = cx.update(|cx| open(cx, Some("examples/readme-sample.pdf".into())).unwrap());
        let first_handle = cx.update(|cx| cx.windows()[0]);
        let second = cx.update(|cx| open(cx, Some("examples/field-guide.pdf".into())).unwrap());
        cx.run_until_parked();
        cx.update_window(first_handle, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            crate::settings_ui::install_with_paths(cx, None);
            cx.dispatch_action(&crate::settings_ui::ShowSettings);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_ne!(cx.active_window(), Some(first_handle));
            open_request(
                cx,
                &first.downgrade(),
                OpenRequest {
                    target: Some(OpenTarget::File("examples/selection-sample.pdf".into())),
                    mode: OpenMode::Reuse,
                },
            );
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(cx.windows().len(), 3);
            assert_eq!(
                first.read(cx).reader().read(cx).path(),
                Some(PathBuf::from("examples/selection-sample.pdf").as_path())
            );
            assert_eq!(
                second.read(cx).reader().read(cx).path(),
                Some(PathBuf::from("examples/field-guide.pdf").as_path())
            );
        });
    }

    #[gpui_kit::test]
    fn external_opens_follow_new_window_preference_and_allow_explicit_reuse(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::settings::AppPaths::for_locations(
            root.path().join("config.toml"),
            root.path().join("data"),
        );
        crate::settings::save(
            paths.config(),
            crate::settings::SettingsChange::OpenFiles(OpenFiles::NewWindow),
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::settings_ui::install_with_paths(cx, Some(paths));
        });
        cx.run_until_parked();
        let initial = cx.update(|cx| open(cx, Some("examples/readme-sample.pdf".into())).unwrap());
        cx.update(|cx| {
            open_request(
                cx,
                &initial.downgrade(),
                OpenRequest {
                    target: Some(OpenTarget::File("examples/field-guide.pdf".into())),
                    mode: OpenMode::Default,
                },
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(cx.windows().len(), 2);
            assert_eq!(
                initial.read(cx).reader().read(cx).path(),
                Some(PathBuf::from("examples/readme-sample.pdf").as_path())
            );
            open_request(
                cx,
                &initial.downgrade(),
                OpenRequest {
                    target: Some(OpenTarget::File("examples/readme-sample.pdf".into())),
                    mode: OpenMode::New,
                },
            );
            assert_eq!(cx.windows().len(), 3);
            open_request(
                cx,
                &initial.downgrade(),
                OpenRequest {
                    target: Some(OpenTarget::File("examples/selection-sample.pdf".into())),
                    mode: OpenMode::Reuse,
                },
            );
            assert_eq!(cx.windows().len(), 3);
            assert_eq!(
                most_recent(cx).unwrap().1.read(cx).reader().read(cx).path(),
                Some(PathBuf::from("examples/selection-sample.pdf").as_path())
            );
        });
    }

    #[gpui_kit::test]
    fn cli_no_path_activates_reader_and_reuse_recovers_after_it_closes(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let initial = cx.update(|cx| open(cx, None).unwrap().downgrade());
        cx.update(|cx| {
            open_request(
                cx,
                &initial,
                OpenRequest {
                    target: None,
                    mode: OpenMode::Default,
                },
            );
            assert_eq!(cx.windows().len(), 1);
            open_request(
                cx,
                &initial,
                OpenRequest {
                    target: None,
                    mode: OpenMode::New,
                },
            );
            assert_eq!(cx.windows().len(), 2);
            for handle in cx.windows() {
                cx.update_window(handle, |_, window, _| window.remove_window())
                    .unwrap();
            }
        });
        cx.run_until_parked();
        cx.update(|cx| {
            open_request(
                cx,
                &initial,
                OpenRequest {
                    target: Some(OpenTarget::File("examples/readme-sample.pdf".into())),
                    mode: OpenMode::Reuse,
                },
            );
            assert_eq!(cx.windows().len(), 1);
        });
    }

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
    fn platform_opens_reuse_the_most_recent_reader_by_default(cx: &mut TestAppContext) {
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
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
        reader.read_with(cx, |reader, _| {
            assert_eq!(
                reader.path(),
                Some(PathBuf::from("examples/missing.pdf").as_path())
            );
            assert_eq!(reader.page_count(), None);
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
