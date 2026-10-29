//! macOS: Chromium needs the shared `NSApplication` to be one that tracks
//! whether it is inside `sendEvent:` (`CefAppProtocol`), before CEF starts.
//! It also takes the links macOS opens in the default browser.

use std::sync::atomic::{AtomicBool, Ordering};

use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObjectProtocol};
use objc2::{ClassType, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent};

/// Only one application exists, so its state needs no instance variable.
static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

/// `kInternetEventClass` and `kAEGetURL`, both 'GURL'.
const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
/// `keyDirectObject`: the event's URL.
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

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

        #[unsafe(method(finishLaunching))]
        fn finish_launching(&self) {
            // SAFETY: NSApplication's own finishLaunching.
            let () = unsafe { msg_send![super(self), finishLaunching] };
            // AppKit installs its own handler in finishLaunching, so ours goes after it.
            // SAFETY: we are the shared application and live as long as the process.
            unsafe {
                let manager: *mut AnyObject =
                    msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
                let () = msg_send![
                    manager,
                    setEventHandler: self,
                    andSelector: sel!(handleGetURL:withReplyEvent:),
                    forEventClass: GET_URL,
                    andEventID: GET_URL
                ];
            }
        }

        #[unsafe(method(handleGetURL:withReplyEvent:))]
        fn handle_get_url(&self, event: &AnyObject, _reply: &AnyObject) {
            // SAFETY: an NSAppleEventDescriptor whose direct object is the URL string.
            let url = unsafe {
                let param: *mut AnyObject =
                    msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
                if param.is_null() {
                    return;
                }
                let string: *mut AnyObject = msg_send![param, stringValue];
                if string.is_null() {
                    return;
                }
                let utf8: *const std::ffi::c_char = msg_send![string, UTF8String];
                std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
            };
            crate::remote::open_urls(vec![url]);
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
