//! Main-thread-only Sparkle boundary. Sparkle owns scheduling, consent, network
//! work, signature verification, installation, and relaunch UI, not the reader.
//!
//! Load only our bundled framework: unbundled development/test executables have
//! no feed/key and do not load Sparkle or make update requests.

use gpui_kit::App;
use objc2::{
    MainThreadMarker, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{NSBundle, NSError, NSString, ns_string};

use crate::app_menu::CheckForUpdates;

struct Updater {
    controller: Retained<AnyObject>,
    _framework: Retained<NSBundle>,
    _main_thread: MainThreadMarker,
}

impl Updater {
    fn new() -> Result<Option<Self>, String> {
        let main_thread = MainThreadMarker::new().ok_or("Updater requires the main thread.")?;
        let app = NSBundle::mainBundle();
        let feed = app.objectForInfoDictionaryKey(ns_string!("SUFeedURL"));
        let key = app.objectForInfoDictionaryKey(ns_string!("SUPublicEDKey"));
        match (feed, key) {
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => {}
            _ => return Err("The update feed or verification key is missing.".into()),
        }
        let frameworks = app
            .privateFrameworksPath()
            .ok_or("The app has no Frameworks directory.")?;
        let path = NSString::from_str(&format!("{frameworks}/Sparkle.framework"));
        let framework = NSBundle::bundleWithPath(&path).ok_or("Sparkle.framework is missing.")?;

        // SAFETY: GPUI calls this on the main thread. The only code loaded is the
        // framework sealed inside our app bundle, never a downloaded/search-path
        // library. Selectors and ownership match Sparkle 2's public headers.
        unsafe {
            framework
                .loadAndReturnError()
                .map_err(|error| error.to_string())?;
            let class = AnyClass::get(c"SPUStandardUpdaterController")
                .ok_or("Sparkle's updater controller is unavailable.")?;
            let allocated: Allocated<AnyObject> = msg_send![class, alloc];
            let controller: Option<Retained<AnyObject>> = msg_send![
                allocated,
                initWithStartingUpdater: false,
                updaterDelegate: None::<&AnyObject>,
                userDriverDelegate: None::<&AnyObject>
            ];
            let controller = controller.ok_or("Cannot create the updater.")?;
            let updater: Retained<AnyObject> = msg_send![&*controller, updater];
            let mut error: Option<Retained<NSError>> = None;
            let started: Bool = msg_send![&*updater, startUpdater: &mut error];
            if !started.as_bool() {
                return Err(error.map_or_else(
                    || "Cannot start the updater.".into(),
                    |error| error.to_string(),
                ));
            }
            Ok(Some(Self {
                controller,
                _framework: framework,
                _main_thread: main_thread,
            }))
        }
    }

    fn check(&self) {
        // SAFETY: the controller stays retained by GPUI's application action
        // handler, which runs on the main thread. Sparkle's session gate prevents
        // duplicate checks while its dialogs or downloads are active.
        unsafe {
            let updater: Retained<AnyObject> = msg_send![&*self.controller, updater];
            let available: Bool = msg_send![&*updater, canCheckForUpdates];
            if available.as_bool() {
                let _: () = msg_send![&*self.controller, checkForUpdates: None::<&AnyObject>];
            }
        }
    }
}

/// Retain one updater for the application lifetime; false disables the menu.
pub(crate) fn install(cx: &mut App) -> bool {
    match Updater::new() {
        Ok(Some(updater)) => {
            cx.on_action(move |_: &CheckForUpdates, _| updater.check());
            true
        }
        Ok(None) => false,
        Err(error) => {
            eprintln!("Updates unavailable: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbundled_executables_have_no_update_configuration() {
        let app = NSBundle::mainBundle();
        assert!(
            app.objectForInfoDictionaryKey(ns_string!("SUFeedURL"))
                .is_none()
        );
        assert!(
            app.objectForInfoDictionaryKey(ns_string!("SUPublicEDKey"))
                .is_none()
        );
    }
}
