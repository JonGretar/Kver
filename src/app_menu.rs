//! macOS application chrome; reader operations remain window-local Actions.

use std::cell::Cell;

use gpui_kit as gpui;
use gpui_kit::{AnyWindowHandle, App, AppContext as _, KeyBinding, Menu, MenuItem};

use crate::reader::{Copy, OpenPdf, ShowContents, ShowFind, ToggleSidebar};
use crate::reader_windows::{NewWindow, NextWindow, PreviousWindow};

gpui::actions!(kver, [AboutKver, Quit]);

pub fn install(cx: &mut App) {
    crate::about::bind_keys(cx);
    let about_window = Cell::<Option<AnyWindowHandle>>::new(None);
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
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
    cx.set_menus([
        Menu::new("Kver").items([
            MenuItem::action("About Kver", AboutKver),
            MenuItem::separator(),
            MenuItem::action("Quit Kver", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Window", NewWindow),
            MenuItem::action("Open PDF…", OpenPdf),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Copy", Copy),
            MenuItem::action("Find", ShowFind),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Next Window", NextWindow),
            MenuItem::action("Previous Window", PreviousWindow),
        ]),
        Menu::new("View").items([
            MenuItem::action("Find", ShowFind),
            MenuItem::action("Contents", ShowContents),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
        ]),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{InputEvent as _, KeyUpEvent, Keystroke, TestAppContext};

    #[gpui_kit::test]
    fn about_kver_opens_one_window_and_can_reopen_after_dismissal(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
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
}
