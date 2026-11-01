//! macOS application chrome; reader operations remain window-local Actions.

use gpui::{App, KeyBinding, Menu, MenuItem};

use crate::reader::OpenPdf;

gpui::actions!(kver, [Quit]);

pub fn install(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus([
        Menu::new("Kver").items([MenuItem::action("Quit Kver", Quit)]),
        Menu::new("File").items([MenuItem::action("Open PDF…", OpenPdf)]),
    ]);
}
