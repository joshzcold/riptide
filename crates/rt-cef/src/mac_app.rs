//! macOS: Chromium needs the shared `NSApplication` to be one that tracks
//! whether it is inside `sendEvent:` (`CefAppProtocol`), before CEF starts.
//! It also takes the links macOS opens in the default browser, and has the
//! standard Edit menu that macOS's editing shortcuts go through.

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
            install_menu(self);
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

fn ns_string(text: &str) -> *mut AnyObject {
    let text = std::ffi::CString::new(text).unwrap_or_default();
    // SAFETY: a NUL-terminated UTF-8 string, copied by NSString.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

/// The standard Edit menu, so Cmd+Z, Cmd+X, Cmd+C, Cmd+V and Cmd+A reach the
/// focused page through AppKit's responder chain.
fn install_menu(app: &RiptideApplication) {
    // SAFETY: plain NSMenu and NSMenuItem construction on the main thread; the
    // menus are retained by their parents and the bar by NSApp.
    unsafe {
        let new_menu = |title: &str| -> *mut AnyObject {
            let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
            msg_send![menu, initWithTitle: ns_string(title)]
        };
        let add_submenu = |bar: *mut AnyObject, menu: *mut AnyObject| {
            let item: *mut AnyObject = msg_send![class!(NSMenuItem), new];
            let () = msg_send![item, setSubmenu: menu];
            let () = msg_send![bar, addItem: item];
        };
        let bar = new_menu("");
        // macOS treats the first menu as the application menu.
        add_submenu(bar, new_menu("riptide"));
        let edit = new_menu("Edit");
        for (title, action, key) in [
            ("Undo", sel!(undo:), "z"),
            ("Redo", sel!(redo:), "Z"),
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
            ("Select All", sel!(selectAll:), "a"),
        ] {
            let _: *mut AnyObject = msg_send![
                edit,
                addItemWithTitle: ns_string(title),
                action: action,
                keyEquivalent: ns_string(key)
            ];
        }
        add_submenu(bar, edit);
        let () = msg_send![app, setMainMenu: bar];
    }
}

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
