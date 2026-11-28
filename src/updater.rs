//! Sparkle owns scheduling, verification and installation. Its bundled user
//! driver translates main-thread callbacks into Kver ribbon notices.
//!
//! Load only our bundled framework: unbundled development/test executables have
//! no feed/key and do not load Sparkle or make update requests.

use crate::app::ribbon::{self, Choice, Notice};
use block2::RcBlock;
use gpui_kit::App;
use objc2::{
    MainThreadMarker, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{NSBundle, NSError, NSString, ns_string};
use std::rc::Rc;

struct Updater {
    engine: Retained<AnyObject>,
    driver: Retained<AnyObject>,
    _driver_bundle: Retained<NSBundle>,
    _framework: Retained<NSBundle>,
    _main_thread: MainThreadMarker,
}

struct Presentation {
    message: String,
    label: String,
    notes: String,
    generation: usize,
}

impl Presentation {
    fn into_notice(self, driver: Retained<AnyObject>) -> Option<Notice> {
        if self.message.is_empty() {
            return None;
        }
        let generation = self.generation;
        Some(Notice {
            message: self.message,
            action: (!self.label.is_empty()).then_some(self.label),
            link: url::Url::parse(&self.notes)
                .ok()
                .filter(|url| url.scheme() == "https")
                .map(|url| ("Release notes".into(), url.into())),
            respond: Rc::new(move |choice, _| {
                // SAFETY: main thread, retained KverUpdateDriver; the driver
                // rejects stale replies before invoking a Sparkle callback.
                unsafe {
                    let _: () = msg_send![&*driver, respond: generation, activate: choice == Choice::Activate];
                }
            }),
        })
    }
}

impl Updater {
    fn new(send: async_channel::Sender<Presentation>) -> Result<Option<Self>, String> {
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
            let bundle_path = app
                .builtInPlugInsPath()
                .ok_or("Missing updater driver directory.")?;
            let bundle = NSBundle::bundleWithPath(&NSString::from_str(&format!(
                "{bundle_path}/KverUpdateDriver.bundle"
            )))
            .ok_or("Kver update driver is missing.")?;
            bundle.loadAndReturnError().map_err(|e| e.to_string())?;
            let class = AnyClass::get(c"KverUpdateDriver").ok_or("Missing Kver update driver.")?;
            let driver: Retained<AnyObject> = msg_send![class, new];
            // The driver copies this block. Queue owned values instead of
            // re-entering GPUI from a synchronous Sparkle reply.
            let publish = RcBlock::new(
                move |message: *const NSString,
                      label: *const NSString,
                      notes: *const NSString,
                      generation: usize| {
                    let _ = send.try_send(Presentation {
                        message: (&*message).to_string(),
                        label: (&*label).to_string(),
                        notes: (&*notes).to_string(),
                        generation,
                    });
                },
            );
            let _: () = msg_send![&*driver, setPublish: &*publish];
            let class = AnyClass::get(c"SPUUpdater").ok_or("Missing Sparkle updater.")?;
            let allocated: Allocated<AnyObject> = msg_send![class, alloc];
            let engine: Option<Retained<AnyObject>> = msg_send![
                allocated,
                initWithHostBundle: &*app,
                applicationBundle: &*app,
                userDriver: &*driver,
                delegate: None::<&AnyObject>
            ];
            let engine = engine.ok_or("Cannot create the updater.")?;
            let mut error: Option<Retained<NSError>> = None;
            let started: Bool = msg_send![&*engine, startUpdater: &mut error];
            if !started.as_bool() {
                return Err(error.map_or_else(
                    || "Cannot start the updater.".into(),
                    |error| error.to_string(),
                ));
            }
            Ok(Some(Self {
                engine,
                driver,
                _driver_bundle: bundle,
                _framework: framework,
                _main_thread: main_thread,
            }))
        }
    }

    fn check(&self) {
        // SAFETY: retained on GPUI's main thread. Reveal an existing session
        // rather than starting a second check while a reply is outstanding.
        unsafe {
            let shown: Bool = msg_send![&*self.driver, reveal];
            let available: Bool = msg_send![&*self.engine, canCheckForUpdates];
            if !shown.as_bool() && available.as_bool() {
                let _: () = msg_send![&*self.engine, checkForUpdates];
            }
        }
    }
}

/// Only register the application command after successfully starting Sparkle.
pub(crate) fn install(cx: &mut App) {
    let (send, receive) = async_channel::unbounded();
    match Updater::new(send) {
        Ok(Some(updater)) => {
            let driver = updater.driver.clone();
            cx.spawn(async move |cx| {
                while let Ok(event) = receive.recv().await {
                    cx.update(|cx| ribbon::present(event.into_notice(driver.clone()), cx));
                }
            })
            .detach();
            crate::app::commands::register_update_check(cx, move |_| updater.check());
        }
        Ok(None) => (),
        Err(error) => {
            eprintln!("Updates unavailable: {error}");
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
