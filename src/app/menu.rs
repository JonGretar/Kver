//! macOS application chrome; reader operations remain window-local Actions.

use std::cell::Cell;

use gpui_kit::{AnyWindowHandle, App, AppContext as _};
#[cfg(target_os = "macos")]
use gpui_kit::{Global, Menu, MenuItem};

use super::document_window::OpenPdf;
#[cfg(target_os = "macos")]
use super::document_window::{NavigateBack, NavigateForward, Save, SaveAs};
#[cfg(target_os = "macos")]
use super::windows::{CloseWindow, NewWindow, NextWindow, PreviousWindow};
#[cfg(target_os = "macos")]
use crate::reader::{
    Continuous, Copy, Highlight, NextPage, PreviousPage, ShowAnnotations, ShowContents, ShowFind,
    SinglePage, ToggleSidebar, UseDefaultReadingMode,
};
#[cfg(target_os = "macos")]
use crate::reading_history::ShowRecent;
#[cfg(target_os = "macos")]
use crate::settings_ui::ShowSettings;

pub use super::commands::{AboutKver, CheckForUpdates, Quit};

/// Commands displayed in the native menu. The keymap checks that overriding
/// a modified accelerator cannot promote a reader-only bare key into it.
pub(crate) const ACCELERATOR_COMMANDS: &[&str] = &[
    "about-kver",
    "check-for-updates",
    "show-settings",
    "quit",
    "new-window",
    "open-pdf",
    "save-as",
    "save",
    "show-recent",
    "copy",
    "highlight",
    "undo-annotation",
    "redo-annotation",
    "show-find",
    "navigate-back",
    "navigate-forward",
    "close-window",
    "next-window",
    "previous-window",
    "show-contents",
    "toggle-sidebar",
    "single-page",
    "continuous",
    "use-default-reading-mode",
    "next-page",
    "previous-page",
];

#[cfg(target_os = "macos")]
struct MenuState {
    updates_available: bool,
    can_back: Cell<bool>,
    can_forward: Cell<bool>,
    highlight_available: Cell<bool>,
    save_available: Cell<bool>,
    save_as_available: Cell<bool>,
    annotation_undo_available: Cell<bool>,
    annotation_redo_available: Cell<bool>,
}
#[cfg(target_os = "macos")]
impl Global for MenuState {}

/// Install handlers and native menus after the application keymap.
pub fn install(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let updates_available = crate::updater::install(cx);
    #[cfg(target_os = "macos")]
    cx.set_global(MenuState {
        updates_available,
        can_back: Cell::new(false),
        can_forward: Cell::new(false),
        highlight_available: Cell::new(false),
        save_available: Cell::new(false),
        save_as_available: Cell::new(false),
        annotation_undo_available: Cell::new(false),
        annotation_redo_available: Cell::new(false),
    });
    let about_window = Cell::<Option<AnyWindowHandle>>::new(None);
    cx.on_action(move |_: &AboutKver, cx| {
        if let Some(handle) = about_window.get()
            && (cx.active_window() == Some(handle)
                || cx
                    .update_window(handle, |_, window, _| window.activate_window())
                    .is_ok())
        {
            return;
        }
        match crate::about::open(cx) {
            Ok(handle) => about_window.set(Some(handle)),
            Err(error) => eprintln!("Cannot open About Kver: {error}"),
        }
    });
    cx.on_action(|_: &Quit, cx| super::windows::request_quit(cx));
    cx.on_action(|_: &OpenPdf, cx| {
        if cx.windows().is_empty() {
            match super::windows::open(cx, None) {
                Ok(reader) => reader.update(cx, |reader, cx| reader.open_dialog(cx)),
                Err(error) => eprintln!("Cannot open window: {error}"),
            }
        }
    });
    #[cfg(target_os = "macos")]
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            refresh_navigation(cx, false, false);
        }
    })
    .detach();
    refresh_navigation(cx, false, false);
}

/// Native menus snapshot both the resolved keymap and the active window's
/// availability. Rebuild when either the jumplist or its input context changes.
pub(crate) fn refresh_navigation(cx: &App, can_back: bool, can_forward: bool) {
    #[cfg(target_os = "macos")]
    if let Some(state) = cx.try_global::<MenuState>() {
        state.can_back.set(can_back);
        state.can_forward.set(can_forward);
        cx.set_menus([
            Menu::new("Kver").items([
                MenuItem::action("About Kver", AboutKver),
                MenuItem::action("Check for Updates…", CheckForUpdates)
                    .disabled(!state.updates_available),
                MenuItem::separator(),
                MenuItem::action("Settings…", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("Quit Kver", Quit),
            ]),
            Menu::new("File").items([
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("Open PDF…", OpenPdf),
                MenuItem::action("Open Recent…", ShowRecent),
                MenuItem::action("Save", Save).disabled(!state.save_available.get()),
                MenuItem::action("Save As…", SaveAs).disabled(!state.save_as_available.get()),
            ]),
            Menu::new("Edit").items([
                MenuItem::action("Undo", crate::reader::UndoAnnotation)
                    .disabled(!state.annotation_undo_available.get()),
                MenuItem::action("Redo", crate::reader::RedoAnnotation)
                    .disabled(!state.annotation_redo_available.get()),
                MenuItem::separator(),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Highlight", Highlight).disabled(!state.highlight_available.get()),
                MenuItem::action("Find", ShowFind),
            ]),
            Menu::new("View").items([
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::separator(),
                MenuItem::action("Single Page", SinglePage),
                MenuItem::action("Continuous", Continuous),
                MenuItem::action("Use Default Reading Mode", UseDefaultReadingMode),
            ]),
            Menu::new("Navigate").items([
                MenuItem::action("Contents", ShowContents),
                MenuItem::action("Annotations", ShowAnnotations),
                MenuItem::separator(),
                MenuItem::action("Forward", NavigateForward).disabled(!can_forward),
                MenuItem::action("Back", NavigateBack).disabled(!can_back),
                MenuItem::separator(),
                MenuItem::action("Next Page", NextPage),
                MenuItem::action("Previous Page", PreviousPage),
            ]),
            Menu::new("Window").items([
                MenuItem::action("Close Window", CloseWindow),
                MenuItem::action("Next Window", NextWindow),
                MenuItem::action("Previous Window", PreviousWindow),
            ]),
        ]);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (cx, can_back, can_forward);
}

/// Re-snapshot accelerators after a keymap change without discarding the
/// active window's navigation availability.
pub(crate) fn refresh_shortcuts(cx: &App) {
    #[cfg(target_os = "macos")]
    if let Some(state) = cx.try_global::<MenuState>() {
        refresh_navigation(cx, state.can_back.get(), state.can_forward.get());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}

pub(crate) fn set_authoring_availability(cx: &App, highlight: bool, save: bool, save_as: bool) {
    #[cfg(target_os = "macos")]
    if let Some(state) = cx.try_global::<MenuState>() {
        state.highlight_available.set(highlight);
        state.save_available.set(save);
        state.save_as_available.set(save_as);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (cx, highlight, save, save_as);
}

pub(crate) fn set_annotation_history_availability(cx: &App, undo: bool, redo: bool) {
    #[cfg(target_os = "macos")]
    if let Some(state) = cx.try_global::<MenuState>() {
        state.annotation_undo_available.set(undo);
        state.annotation_redo_available.set(redo);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (cx, undo, redo);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader_windows::CloseWindow;
    use gpui_kit::{
        InputEvent as _, KeyUpEvent, Keystroke, TestAppContext, test::TestWindowExt as _,
    };

    #[gpui_kit::test]
    fn about_kver_opens_one_window_and_can_reopen_after_dismissal(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            crate::reader_windows::install(cx);
            install(cx);
            cx.dispatch_action(&AboutKver);
        });
        let about = cx.update(|cx| {
            assert_eq!(cx.windows().len(), 1);
            cx.windows()[0]
        });
        assert_eq!(cx.opened_url(), None);

        cx.update(|cx| cx.dispatch_action(&AboutKver));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);

        cx.simulate_keystrokes(about, "shift-tab enter");
        cx.update(|cx| {
            cx.update_window(about, |_, window, cx| {
                window.dispatch_event(
                    KeyUpEvent {
                        keystroke: Keystroke::parse("enter").unwrap(),
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .unwrap();
        });
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://github.com/JonGretar/Kver")
        );
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);

        cx.simulate_keystrokes(about, "escape");
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().is_empty()));

        // A real About window is reopened from a remaining reader window.
        let reader = cx.add_window(|_, _| gpui_kit::Empty);
        cx.update(|cx| {
            reader
                .update(cx, |_, window, _| window.activate_window())
                .unwrap();
        });
        cx.update(|cx| cx.dispatch_action(&AboutKver));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    }

    #[gpui_kit::test]
    fn cmd_w_and_close_window_action_close_only_the_active_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            crate::reader_windows::install(cx);
            install(cx);
        });
        let reader = cx.add_window(|window, cx| {
            super::super::document_window::DocumentWindow::new(None, window, cx)
        });
        cx.update(|cx| cx.dispatch_action(&AboutKver));
        let about = cx.update(|cx| {
            assert_eq!(cx.windows().len(), 2);
            *cx.windows()
                .iter()
                .find(|&&handle| handle != reader.into())
                .unwrap()
        });

        cx.update(|cx| {
            cx.update_window(about, |_, window, cx| window.render_frame(cx))
                .unwrap();
            assert_eq!(cx.active_window(), Some(about));
        });
        cx.simulate_keystrokes(about, "cmd-w");
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows()), vec![reader.into()]);

        cx.update(|cx| {
            cx.update_window(reader.into(), |_, window, _| window.activate_window())
                .unwrap();
            cx.dispatch_action(&AboutKver);
        });
        let about = cx.update(|cx| {
            *cx.windows()
                .iter()
                .find(|&&handle| handle != reader.into())
                .unwrap()
        });
        cx.update(|cx| {
            cx.update_window(about, |_, window, cx| {
                window.render_frame(cx);
                window.dispatch_action(Box::new(CloseWindow), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| cx.windows()), vec![reader.into()]);

        cx.update(|cx| {
            cx.update_window(reader.into(), |_, window, cx| {
                window.activate_window();
                window.render_frame(cx);
            })
            .unwrap();
        });
        cx.simulate_keystrokes(reader.into(), "cmd-w");
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().is_empty()));
    }

    #[cfg(target_os = "macos")]
    #[gpui_kit::test]
    fn keymap_refresh_preserves_navigation_menu_availability(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            install(cx);
            refresh_navigation(cx, true, false);
            crate::app::keymap::reload(
                cx,
                Some("[[keyboard.bindings]]\nkeys = \"ctrl-e\"\ncontext = \"reader\"\ncommand = \"navigate-back\"\n".into()),
                crate::app::keymap::Preset::Standard,
            )
            .unwrap();
            let state = cx.global::<MenuState>();
            assert!(state.can_back.get());
            assert!(!state.can_forward.get());
            let binding = cx
                .key_bindings()
                .borrow()
                .bindings_for_action(&NavigateBack)
                .last()
                .unwrap()
                .clone();
            assert_eq!(
                binding.match_keystrokes(&[Keystroke::parse("ctrl-e").unwrap()]),
                Some(false)
            );
        });
    }
}
