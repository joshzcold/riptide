//! Download a URL through Chromium's network stack, for files the browser
//! needs itself (Greasemonkey `@require` libraries).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use cef::*;

type Done = Box<dyn FnOnce(Result<Vec<u8>, String>)>;

thread_local! {
    static NEXT_ID: Cell<u32> = const { Cell::new(1) };
    /// Requests in flight, kept alive until they finish, with what to call then.
    static PENDING: RefCell<HashMap<u32, (Urlrequest, Done)>> = RefCell::new(HashMap::new());
}

/// GET `url` and call `done` with the body, or why it failed.
pub fn get(url: &str, done: impl FnOnce(Result<Vec<u8>, String>) + 'static) {
    let Some(mut request) = request_create() else {
        return done(Err("could not create a request".into()));
    };
    request.set_url(Some(&CefString::from(url)));
    request.set_method(Some(&CefString::from("GET")));
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let mut client = FetchClient::new(id, RefCell::new(Vec::new()));
    match urlrequest_create(Some(&mut request), Some(&mut client), None) {
        Some(request) => PENDING.with(|p| {
            p.borrow_mut().insert(id, (request, Box::new(done)));
        }),
        None => done(Err("could not start the request".into())),
    }
}

wrap_task! {
    struct Finished {
        id: u32,
        result: RefCell<Option<Result<Vec<u8>, String>>>,
    }

    impl Task {
        fn execute(&self) {
            let pending = PENDING.with(|p| p.borrow_mut().remove(&self.id));
            if let (Some((_, done)), Some(result)) = (pending, self.result.borrow_mut().take()) {
                done(result);
            }
        }
    }
}

wrap_urlrequest_client! {
    struct FetchClient {
        id: u32,
        data: RefCell<Vec<u8>>,
    }

    impl UrlrequestClient {
        fn on_download_data(&self, _request: Option<&mut Urlrequest>, data: *const u8, data_length: usize) {
            if !data.is_null() {
                // SAFETY: CEF passes `data_length` readable bytes at `data`.
                let bytes = unsafe { std::slice::from_raw_parts(data, data_length) };
                self.data.borrow_mut().extend_from_slice(bytes);
            }
        }

        fn on_request_complete(&self, request: Option<&mut Urlrequest>) {
            let status = request.as_ref().and_then(|r| r.response()).map(|r| r.status()).unwrap_or(0);
            let ok = request.is_some_and(|r| r.request_status() == UrlrequestStatus::SUCCESS) && status == 200;
            let result = if ok {
                Ok(std::mem::take(&mut *self.data.borrow_mut()))
            } else {
                Err(format!("HTTP status {status}"))
            };
            // Out of CEF's callback before the caller's code runs.
            let mut task = Finished::new(self.id, RefCell::new(Some(result)));
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}
