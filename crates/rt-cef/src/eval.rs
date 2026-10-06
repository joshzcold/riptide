//! Browser side of the eval channel: run JavaScript in a page and get its
//! string result back through the renderer process.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use cef::*;

use crate::renderer::EVAL_MESSAGE;

type Callback = Box<dyn FnOnce(Result<String, String>)>;

thread_local! {
    static NEXT_ID: Cell<i32> = const { Cell::new(1) };
    // Kept apart from the shell so callbacks can borrow it freely.
    static PENDING: RefCell<HashMap<i32, Callback>> = RefCell::new(HashMap::new());
}

/// Evaluate `code` in the browser's main frame and call `done` with the
/// result. The script's completion value must be a string (use JSON).
pub fn eval(browser: &Browser, code: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    match browser.main_frame() {
        Some(frame) => eval_frame(&frame, code, done),
        None => done(Err("page has no main frame".into())),
    }
}

/// [`eval`] in one frame, which may be a cross-origin iframe.
pub fn eval_frame(frame: &Frame, code: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let Some(mut message) = process_message_create(Some(&CefString::from(EVAL_MESSAGE))) else {
        done(Err("could not create process message".into()));
        return;
    };
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1).max(1));
        id
    });
    if let Some(args) = message.argument_list() {
        args.set_int(0, id);
        args.set_string(1, Some(&CefString::from(code)));
    }
    PENDING.with(|p| p.borrow_mut().insert(id, Box::new(done)));
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

/// Deliver a reply from the renderer; unknown ids (e.g. after a reload) are ignored.
pub fn complete(id: i32, ok: bool, text: String) {
    let Some(done) = PENDING.with(|p| p.borrow_mut().remove(&id)) else {
        return;
    };
    done(if ok { Ok(text) } else { Err(text) });
}
