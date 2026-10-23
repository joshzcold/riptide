//! macOS: Chromium needs the shared `NSApplication` to be one that tracks
//! whether it is inside `sendEvent:` (`CefAppProtocol`), before CEF starts.

use std::sync::atomic::{AtomicBool, Ordering};

use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObjectProtocol};
use objc2::{ClassType, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSEvent};

/// Only one application exists, so its state needs no instance variable.
static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

define_class!(
    #[unsafe(super(NSApplication))]
    #[thread_kind = MainThreadOnly]
    #[name = "RiptideApplication"]
    struct RiptideApplication;

    unsafe impl NSObjectProtocol for RiptideApplication {}

    unsafe impl CrAppProtocol for RiptideApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn is_handling_send_event(&self) -> Bool {
            Bool::new(HANDLING_SEND_EVENT.load(Ordering::Relaxed))
        }
    }

    unsafe impl CrAppControlProtocol for RiptideApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn set_handling_send_event(&self, handling: Bool) {
            HANDLING_SEND_EVENT.store(handling.as_bool(), Ordering::Relaxed);
        }
    }

    unsafe impl CefAppProtocol for RiptideApplication {}

    impl RiptideApplication {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            let was = HANDLING_SEND_EVENT.swap(true, Ordering::Relaxed);
            // SAFETY: NSApplication's own sendEvent: with the event AppKit gave us.
            let () = unsafe { msg_send![super(self), sendEvent: event] };
            HANDLING_SEND_EVENT.store(was, Ordering::Relaxed);
        }
    }
);

/// Make `RiptideApplication` the shared application. Call on the main thread,
/// after the framework is loaded (it defines the protocols) and before CEF starts.
pub fn install() {
    // SAFETY: +sharedApplication on an NSApplication subclass creates NSApp once.
    let app: Retained<NSApplication> =
        unsafe { msg_send![RiptideApplication::class(), sharedApplication] };
    if !app.isKindOfClass(RiptideApplication::class()) {
        tracing::warn!("NSApp was already created; Chromium may misbehave");
    }
}
