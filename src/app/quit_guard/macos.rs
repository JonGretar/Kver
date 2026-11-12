//! Cocoa veto before GPUI's non-cancellable `applicationWillTerminate:` path.
//!
//! Keep the SAME delegate object, its ivars, and all inherited GPUI methods.
//! Add only `applicationShouldTerminate:` on an ivar-free runtime subclass.
//! Re-audit this boundary on GPUI upgrades; see the checklist linked by the
//! parent module. If GPUI gains its own veto, refuse installation rather than
//! silently replacing its policy.

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

use async_channel::Sender;
use gpui_kit::App;
use objc2::{
    MainThreadMarker, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};

const CANCEL: usize = 0; // NSTerminateCancel
const NOW: usize = 1; // NSTerminateNow (NSApplicationTerminateReply / NSUInteger)
static REQUESTS: OnceLock<Sender<()>> = OnceLock::new();
static APPROVAL: Approval = Approval::new();

struct Approval {
    approved: AtomicBool,
    requested: AtomicBool,
}

impl Approval {
    const fn new() -> Self {
        Self {
            approved: AtomicBool::new(false),
            requested: AtomicBool::new(false),
        }
    }

    fn approve(&self) {
        self.approved.store(true, Ordering::Release);
    }

    fn invalidate(&self) {
        if self.approved.swap(false, Ordering::AcqRel) {
            self.requested.store(false, Ordering::Release);
        }
    }

    fn consume(&self) -> bool {
        self.approved.swap(false, Ordering::AcqRel)
    }

    fn begin_request(&self) -> bool {
        !self.requested.swap(true, Ordering::AcqRel)
    }

    fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        self.approved.store(false, Ordering::Release);
        self.requested.store(false, Ordering::Release);
    }
}

pub(super) fn invalidate_approval() {
    APPROVAL.invalidate();
}

pub(super) fn cancel_request() {
    APPROVAL.cancel();
}

pub(super) fn approve_next_request() {
    // Headless unit-test Apps do not install a Cocoa adapter.
    if REQUESTS.get().is_some() {
        APPROVAL.approve();
    }
}

extern "C-unwind" fn should_terminate(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> usize {
    if APPROVAL.consume() {
        return NOW;
    }
    // Never borrow GPUI's App from this callback: Cocoa may invoke us while a
    // GPUI update is on the stack. A bounded queue coalesces repeated requests.
    // The receiver invokes the same owner as the Quit menu and Command-Q.
    if let Some(requests) = REQUESTS.get()
        && APPROVAL.begin_request()
        && let Err(async_channel::TrySendError::Closed(())) = requests.try_send(())
    {
        APPROVAL.cancel();
    }
    CANCEL
}

fn subclass(
    original: &'static AnyClass,
    name: &std::ffi::CStr,
) -> Result<&'static AnyClass, String> {
    if original
        .instance_method(sel!(applicationShouldTerminate:))
        .is_some()
    {
        return Err(
            "The native delegate already implements a quit veto; recheck Kver's GPUI integration."
                .into(),
        );
    }
    let class = if let Some(existing) = AnyClass::get(name) {
        existing
    } else {
        let mut builder =
            ClassBuilder::new(name, original).ok_or("Cannot register Kver's native quit guard.")?;
        // SAFETY: Cocoa declares this optional delegate method as
        // (NSApplication *) -> NSApplicationTerminateReply, an NSUInteger.
        // No ivars or other methods are added or overridden.
        unsafe {
            builder.add_method(
                sel!(applicationShouldTerminate:),
                should_terminate
                    as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
            );
        }
        builder.register()
    };
    if class.superclass() != Some(original) || class.instance_size() != original.instance_size() {
        return Err("The native quit guard's delegate layout is incompatible.".into());
    }
    Ok(class)
}

pub(super) fn install(cx: &mut App) -> Result<(), String> {
    let _main_thread = MainThreadMarker::new()
        .ok_or("Native quit protection must be installed on the main thread.")?;
    if REQUESTS.get().is_some() {
        return Err("Native quit protection was already installed.".into());
    }
    let application_class = AnyClass::get(c"NSApplication")
        .ok_or("NSApplication is unavailable for native quit protection.")?;
    // SAFETY: Installed on the main thread after GPUI has created NSApplication
    // and its delegate. Both selectors are public Cocoa accessors; Retained
    // keeps the existing objects live while we inspect/update their class.
    let (application, delegate) = unsafe {
        let application: Option<Retained<AnyObject>> =
            msg_send![application_class, sharedApplication];
        let application = application.ok_or("The native application is unavailable.")?;
        let delegate: Option<Retained<AnyObject>> = msg_send![&*application, delegate];
        (
            application,
            delegate.ok_or("The native application delegate is unavailable.")?,
        )
    };
    let original = delegate.class();
    if original.name() != c"GPUIApplicationDelegate" {
        return Err(format!(
            "Unexpected native delegate {:?}; recheck Kver's GPUI quit protection.",
            original.name()
        ));
    }
    let class = subclass(original, c"KverQuitGuardDelegate")?;
    let (requests, incoming) = async_channel::bounded(1);
    REQUESTS
        .set(requests)
        .map_err(|_| "Native quit protection was already installed.")?;
    // SAFETY: Main-thread-only, with no intervening callbacks or concurrent
    // class changes. `class` is a subclass of this object's current class and
    // adds no ivars (sizes checked above). Its sole method uses Cocoa's exact
    // ABI. Delegate identity/layout and all GPUI callbacks remain unchanged.
    unsafe {
        let previous = AnyObject::set_class(&delegate, class);
        debug_assert_eq!(previous, original);
        // NSApplication caches optional delegate capabilities. Re-register the
        // same object so it discovers the newly supplied veto selector.
        let _: () = msg_send![&*application, setDelegate: std::ptr::null_mut::<AnyObject>()];
        let _: () = msg_send![&*application, setDelegate: &*delegate];
    }
    cx.spawn(async move |cx| {
        while incoming.recv().await.is_ok() {
            if APPROVAL.is_requested() {
                cx.update(super::super::windows::request_quit);
            }
        }
    })
    .detach();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_is_one_shot_and_new_work_revokes_it() {
        let approval = Approval::new();
        assert!(!approval.consume());
        approval.approve();
        assert!(approval.consume());
        assert!(!approval.consume());
        approval.approve();
        approval.invalidate();
        assert!(!approval.consume());
        assert!(approval.begin_request());
        assert!(!approval.begin_request());
        approval.cancel();
        assert!(!approval.is_requested());
        assert!(approval.begin_request());
        approval.approve();
        approval.invalidate();
        assert!(
            approval.begin_request(),
            "new work must rearm a queued native quit"
        );
    }

    #[test]
    fn runtime_subclass_preserves_delegate_identity_ivars_and_inherited_behavior() {
        let mut builder = ClassBuilder::new(
            c"KverQuitGuardTestDelegate",
            AnyClass::get(c"NSObject").unwrap(),
        )
        .unwrap();
        builder.add_ivar::<usize>(c"marker");
        extern "C-unwind" fn inherited(_: *mut AnyObject, _: Sel) -> usize {
            42
        }
        // SAFETY: No arguments and an NSUInteger return, as declared here.
        unsafe {
            builder.add_method(
                sel!(kverInheritedBehavior),
                inherited as extern "C-unwind" fn(*mut AnyObject, Sel) -> usize,
            );
        }
        let original = builder.register();
        let class = subclass(original, c"KverQuitGuardTestSubclass").unwrap();
        // SAFETY: This test owns its NSObject-derived instance exclusively.
        // Both classes have the same layout; the subclass adds only the veto.
        unsafe {
            let delegate: Retained<AnyObject> = msg_send![original, new];
            let identity = Retained::as_ptr(&delegate);
            original
                .instance_variable(c"marker")
                .unwrap()
                .load_ptr::<usize>(&delegate)
                .write(17);
            assert_eq!(AnyObject::set_class(&delegate, class), original);
            assert_eq!(Retained::as_ptr(&delegate), identity);
            assert_eq!(
                *class
                    .instance_variable(c"marker")
                    .unwrap()
                    .load_ptr::<usize>(&delegate),
                17
            );
            let inherited: usize = msg_send![&*delegate, kverInheritedBehavior];
            assert_eq!(inherited, 42);
            let reply: usize = msg_send![&*delegate, applicationShouldTerminate: std::ptr::null_mut::<AnyObject>()];
            assert_eq!(reply, CANCEL);
        }
    }

    #[test]
    fn a_framework_owned_veto_is_not_overridden() {
        let original = AnyClass::get(c"NSObject").unwrap();
        let guarded = subclass(original, c"KverQuitGuardExistingVetoTest").unwrap();
        assert!(subclass(guarded, c"KverQuitGuardMustNotOverrideTest").is_err());
    }
}
