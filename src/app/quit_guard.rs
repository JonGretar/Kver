//! Route native termination through Kver's existing unsaved-work decisions.
//!
//! No GPUI fork or patch. The macOS adapter depends on the pinned framework's
//! native delegate layout; dependency upgrades must follow the checklist in
//! `docs/releasing.md#native-quit-checks-after-dependency-upgrades`.

use gpui_kit::App;

#[cfg(target_os = "macos")]
mod macos;

pub(super) fn install(cx: &mut App) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::install(cx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cx;
        Ok(())
    }
}

/// A new edit or write invalidates any approval waiting in the native event
/// queue. The final termination callback must never consume an obsolete permit.
pub(crate) fn invalidate_approval() {
    #[cfg(target_os = "macos")]
    macos::invalidate_approval();
}

pub(crate) fn cancel_request() {
    #[cfg(target_os = "macos")]
    macos::cancel_request();
}

pub(super) fn finish_quit(cx: &mut App) {
    #[cfg(target_os = "macos")]
    macos::approve_next_request();
    cx.quit();
}
