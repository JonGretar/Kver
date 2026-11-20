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
