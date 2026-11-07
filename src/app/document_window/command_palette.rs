//! Window-local command palette. The owner supplies availability, not a reader
//! reference; commands and dismissal follow the normal GPUI action path.

use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Entity, FocusHandle, IntoElement, SharedString,
    Window, div, prelude::*,
};
use gpui_kit::{
    base::{Disableable, FocusTrapElement, TestSupportExt as _},
    component::{
        ActiveTheme,
        command::{Command, CommandItem, CommandState},
    },
};

use super::sidebar::{ShowContents, ShowFind, ToggleSidebar};
use crate::{
    app::windows::{NewWindow, NextWindow, PreviousWindow},
    reader::{
        Continuous, Copy, FitPage, FitWidth, NextMatch, NextPage, OpenPdf, PreviousMatch,
        PreviousPage, ReadingMode, ShowDeleteMark, ShowJumpMark, ShowMarks, ShowSetMark,
        SinglePage, UseDefaultReadingMode, ZoomIn, ZoomOut,
    },
    reading_history::ShowRecent,
    settings_ui::ShowSettings,
};

gpui_kit::actions!(kver, [ShowCommandPalette, DismissCommandPalette]);

/// Read-only presentation input, rebuilt by the owner from current state.
#[non_exhaustive]
pub(crate) struct CommandAvailability {
    pub(crate) page_ix: usize,
    pub(crate) page_count: usize,
    pub(crate) mode_override: Option<ReadingMode>,
    pub(crate) has_selection: bool,
    pub(crate) has_matches: bool,
}

#[derive(Clone)]
pub(crate) struct CommandPalette {
    command: Entity<CommandState>,
    focus: FocusHandle,
}

impl CommandPalette {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let command = cx.new(|cx| CommandState::new(window, cx));
        command.update(cx, |command, cx| command.focus(window, cx));
        Self {
            command,
            focus: cx.focus_handle(),
        }
    }

    pub(crate) fn query(&self, cx: &App) -> SharedString {
        self.command.read(cx).query(cx)
    }

    #[cfg(test)]
    pub(crate) fn has_selection(&self, cx: &App) -> bool {
        self.command.read(cx).selected_index().is_some()
    }

    /// Keep backdrop and focus-trapped panel as siblings in the host's overlay
    /// layer. No extra container changes sizing, hit testing, or action ancestry.
    pub(crate) fn render(&self, available: CommandAvailability, cx: &App) -> [AnyElement; 2] {
        let has_document = available.page_count > 0;
        let items = vec![
            command("Open PDF…", OpenPdf, true),
            command("Open Recent…", ShowRecent, true),
            command("Open marks…", ShowMarks, has_document),
            command("Set local mark…", ShowSetMark, has_document),
            command("Jump to local mark…", ShowJumpMark, has_document),
            command("Delete local mark…", ShowDeleteMark, has_document),
            command("Settings…", ShowSettings, true),
            command("Find", ShowFind, has_document),
            command("Next match", NextMatch, available.has_matches),
            command("Previous match", PreviousMatch, available.has_matches),
            command("Contents", ShowContents, has_document),
            command("Toggle Sidebar", ToggleSidebar, has_document),
            command("New Window", NewWindow, true),
            command("Next Window", NextWindow, cx.windows().len() >= 2),
            command("Previous Window", PreviousWindow, cx.windows().len() >= 2),
            command(
                "Next Page",
                NextPage,
                has_document && available.page_ix + 1 < available.page_count,
            ),
            command(
                "Previous Page",
                PreviousPage,
                has_document && available.page_ix > 0,
            ),
            command(
                "Single page",
                SinglePage,
                has_document && available.mode_override != Some(ReadingMode::SinglePage),
            ),
            command(
                "Continuous",
                Continuous,
                has_document && available.mode_override != Some(ReadingMode::Continuous),
            ),
            command(
                "Use default reading mode",
                UseDefaultReadingMode,
                has_document && available.mode_override.is_some(),
            ),
            command("Zoom In", ZoomIn, has_document),
            command("Zoom Out", ZoomOut, has_document),
            command("Fit Width", FitWidth, has_document),
            command("Fit Page", FitPage, has_document),
            command("Copy", Copy, available.has_selection),
        ];
        [
            div()
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, cx| {
                    dismiss(window, cx);
                })
                .into_any_element(),
            div()
                .id("palette-panel")
                .test_support()
                .role(gpui_kit::Role::Dialog)
                .aria_label("Commands")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div().w_full().max_w(gpui_kit::rems(24.)).child(
                        Command::new(&self.command)
                            .placeholder("Search actions…")
                            .items(items)
                            .on_confirm(|_, window, cx| dismiss(window, cx))
                            .on_cancel(dismiss),
                    ),
                )
                .focus_trap("reader-command-palette", &self.focus)
                .into_any_element(),
        ]
    }
}

fn command(label: &'static str, action: impl Action, enabled: bool) -> CommandItem {
    CommandItem::new()
        .label(label)
        .action(Box::new(action))
        .disabled(!enabled)
}

fn dismiss(window: &mut Window, cx: &mut App) {
    window.dispatch_action(Box::new(DismissCommandPalette), cx);
}
