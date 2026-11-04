//! macOS application chrome; reader operations remain window-local Actions.

use gpui_kit as gpui;
use gpui_kit::{App, KeyBinding, Menu, MenuItem};

use crate::reader::{Copy, OpenPdf};
use crate::reader_windows::{NewWindow, NextWindow, PreviousWindow};

gpui::actions!(kver, [Quit]);

pub fn install(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus([
        Menu::new("Kver").items([MenuItem::action("Quit Kver", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New Window", NewWindow),
            MenuItem::action("Open PDF…", OpenPdf),
        ]),
        Menu::new("Edit").items([MenuItem::action("Copy", Copy)]),
        Menu::new("Window").items([
            MenuItem::action("Next Window", NextWindow),
            MenuItem::action("Previous Window", PreviousWindow),
        ]),
    ]);
}
