//! Application-owned actions, independent of their physical keybindings.

gpui_kit::actions!(
    kver,
    [
        AboutKver,
        CheckForUpdates,
        OpenKverWebsite,
        OpenVimTutor,
        Quit
    ]
);

pub(crate) const PROJECT_URL: &str = "https://github.com/JonGretar/Kver";
pub(crate) const OPEN_KVER_WEBSITE_LABEL: &str = "Open Kver Website";
pub(crate) const OPEN_VIM_TUTOR_LABEL: &str = "Open Vim Tutor";

// A platform registers a check when it can discover releases. Installation
// capability is deliberately not a prerequisite for exposing this command.
struct UpdateChecks;
impl gpui_kit::Global for UpdateChecks {}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn register_update_check(
    cx: &mut gpui_kit::App,
    check: impl Fn(&mut gpui_kit::App) + 'static,
) {
    cx.set_global(UpdateChecks);
    cx.on_action(move |_: &CheckForUpdates, cx| {
        cx.defer(super::windows::show_reader);
        check(cx);
    });
}

pub(crate) fn has_update_check(cx: &gpui_kit::App) -> bool {
    cx.has_global::<UpdateChecks>()
}
