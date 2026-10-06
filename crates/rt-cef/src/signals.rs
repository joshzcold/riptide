//! Signals that end the browser. Chromium quits gracefully on SIGINT and
//! SIGHUP, closing every page just as `:quit` does, so riptide would take
//! Ctrl-C or a closed terminal for the user quitting and drop the tabs saved
//! for crash recovery. This notes the signal, then hands it to Chromium.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

use libc::{c_int, c_void, siginfo_t};

const SIGNALS: [c_int; 3] = [libc::SIGINT, libc::SIGHUP, libc::SIGTERM];

static RECEIVED: AtomicBool = AtomicBool::new(false);
/// The handler and flags each signal had before [`install`], by index in [`SIGNALS`].
static PREVIOUS: [AtomicUsize; 3] = [const { AtomicUsize::new(libc::SIG_DFL) }; 3];
static PREVIOUS_FLAGS: [AtomicI32; 3] = [const { AtomicI32::new(0) }; 3];

/// Only does what's safe in a signal handler: atomics, then the old handler.
extern "C" fn note(signal: c_int, info: *mut siginfo_t, context: *mut c_void) {
    RECEIVED.store(true, Ordering::SeqCst);
    let Some(i) = SIGNALS.iter().position(|&s| s == signal) else {
        return;
    };
    let handler = PREVIOUS[i].load(Ordering::SeqCst);
    let flags = PREVIOUS_FLAGS[i].load(Ordering::SeqCst);
    // SAFETY: `handler` is what sigaction reported for this signal, called
    // the way its flags say it expects.
    unsafe {
        match handler {
            libc::SIG_IGN => {}
            libc::SIG_DFL => {
                libc::signal(signal, libc::SIG_DFL);
                libc::raise(signal);
            }
            _ if flags & libc::SA_SIGINFO != 0 => {
                let previous: extern "C" fn(c_int, *mut siginfo_t, *mut c_void) =
                    std::mem::transmute(handler);
                previous(signal, info, context);
            }
            _ => {
                let previous: extern "C" fn(c_int) = std::mem::transmute(handler);
                previous(signal);
            }
        }
    }
}

/// Wrap the handlers CEF installed. Call after `cef::initialize`, in the
/// browser process.
pub fn install() {
    for (i, &signal) in SIGNALS.iter().enumerate() {
        // SAFETY: plain sigaction calls with zeroed structs filled in here.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, std::ptr::null(), &mut old) != 0 {
                continue;
            }
            PREVIOUS[i].store(old.sa_sigaction, Ordering::SeqCst);
            PREVIOUS_FLAGS[i].store(old.sa_flags, Ordering::SeqCst);
            let mut new: libc::sigaction = std::mem::zeroed();
            new.sa_sigaction = note as *const () as libc::sighandler_t;
            new.sa_flags = libc::SA_SIGINFO | (old.sa_flags & libc::SA_RESTART);
            libc::sigemptyset(&mut new.sa_mask);
            libc::sigaction(signal, &new, std::ptr::null_mut());
        }
    }
}

/// A signal asked the browser to stop.
pub fn received() -> bool {
    RECEIVED.load(Ordering::SeqCst)
}
