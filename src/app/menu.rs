//! macOS application chrome; reader operations remain window-local Actions.

use std::cell::Cell;

use gpui_kit::{AnyWindowHandle, App, AppContext as _};
#[cfg(target_os = "macos")]
use gpui_kit::{Menu, MenuItem};

use super::document_window::OpenPdf;
#[cfg(target_os = "macos")]
use super::windows::{CloseWindow, NewWindow, NextWindow, PreviousWindow};
#[cfg(target_os = "macos")]
use crate::reader::{
    Continuous, Copy, NextPage, PreviousPage, ShowContents, ShowFind, SinglePage, ToggleSidebar,
    UseDefaultReadingMode,
};
#[cfg(target_os = "macos")]
use crate::reading_history::ShowRecent;
#[cfg(target_os = "macos")]
use crate::settings_ui::ShowSettings;

pub use super::commands::{AboutKver, CheckForUpdates, Quit};

/// Install handlers and native menus after the application keymap.
pub fn install(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let updates_available = crate::updater::install(cx);
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
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &OpenPdf, cx| {
        if cx.windows().is_empty() {
            match super::windows::open(cx, None) {
                Ok(reader) => reader.update(cx, |reader, cx| reader.open_dialog(cx)),
                Err(error) => eprintln!("Cannot open window: {error}"),
            }
        }
    });
    #[cfg(target_os = "macos")]
    cx.set_menus([
        Menu::new("Kver").items([
            MenuItem::action("About Kver", AboutKver),
            MenuItem::action("Check for Updates…", CheckForUpdates).disabled(!updates_available),
            MenuItem::separator(),
            MenuItem::action("Settings…", ShowSettings),
            MenuItem::separator(),
            MenuItem::action("Quit Kver", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Window", NewWindow),
            MenuItem::action("Open PDF…", OpenPdf),
            MenuItem::action("Open Recent…", ShowRecent),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Copy", Copy),
            MenuItem::action("Find", ShowFind),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Close Window", CloseWindow),
            MenuItem::action("Next Window", NextWindow),
            MenuItem::action("Previous Window", PreviousWindow),
        ]),
        Menu::new("View").items([
            MenuItem::action("Find", ShowFind),
            MenuItem::action("Contents", ShowContents),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Single Page", SinglePage),
            MenuItem::action("Continuous", Continuous),
            MenuItem::action("Use Default Reading Mode", UseDefaultReadingMode),
            MenuItem::action("Next Page", NextPage),
            MenuItem::action("Previous Page", PreviousPage),
        ]),
    ]);
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
}
