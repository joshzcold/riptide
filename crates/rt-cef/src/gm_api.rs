//! The Greasemonkey APIs a script must `@grant`, browser side:
//! `GM_xmlhttpRequest` and `GM_openInTab`. Every request is checked here,
//! against the script the renderer's native function was made for.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use cef::*;
use rt_core::engine::Level;
use serde::Deserialize;

use crate::renderer::GM_XHR_DONE_MESSAGE;
use crate::shell;

#[derive(Deserialize)]
struct XhrRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    data: Option<String>,
    /// Milliseconds; 0 for none.
    #[serde(default)]
    timeout: i64,
}

thread_local! {
    static NEXT_ID: Cell<u32> = const { Cell::new(1) };
    /// Requests in flight, kept alive until they finish, with the frame to answer.
    static PENDING: RefCell<HashMap<u32, (Urlrequest, Frame, i32)>> = RefCell::new(HashMap::new());
}

fn page_url(frame: &Frame) -> String {
    CefString::from(&frame.url()).to_string()
}

/// `GM_xmlhttpRequest` request `id` from `script`, running in `frame`.
pub fn xhr(frame: Frame, script: &str, id: i32, json: &str) {
    let page = page_url(&frame);
    let request: XhrRequest = match serde_json::from_str(json) {
        Ok(request) => request,
        Err(e) => return reply(&frame, id, &failure(&format!("bad request: {e}"))),
    };
    let allowed = crate::greasemonkey::script(script).is_some_and(|s| {
        s.applies_to(&page) && s.grants("GM_xmlhttpRequest") && s.may_connect(&page, &request.url)
    });
    if !allowed {
        let message = format!(
            "{script}: GM_xmlhttpRequest to {} isn't allowed by its @grant and @connect",
            request.url
        );
        shell::show_message(Level::Warning, message);
        return reply(&frame, id, &failure("not allowed"));
    }
    let Some(mut cef_request) = request_create() else {
        return reply(&frame, id, &failure("could not create a request"));
    };
    cef_request.set_url(Some(&CefString::from(request.url.as_str())));
    cef_request.set_method(Some(&CefString::from(
        request.method.to_uppercase().as_str(),
    )));
    for (name, value) in &request.headers {
        cef_request.set_header_by_name(
            Some(&CefString::from(name.as_str())),
            Some(&CefString::from(value.as_str())),
            1,
        );
    }
    if let Some(data) = &request.data
        && let (Some(mut post), Some(mut element)) =
            (post_data_create(), post_data_element_create())
    {
        element.set_to_bytes(data.len(), data.as_ptr());
        post.add_element(Some(&mut element));
        cef_request.set_post_data(Some(&mut post));
    }
    let key = NEXT_ID.with(|n| {
        let key = n.get();
        n.set(key + 1);
        key
    });
    let mut client = XhrClient::new(key, RefCell::new(Vec::new()));
    // The frame's request context, so a private window's cookies stay private.
    match frame.create_urlrequest(Some(&mut cef_request), Some(&mut client)) {
        Some(started) => {
            PENDING.with(|p| p.borrow_mut().insert(key, (started, frame, id)));
            if request.timeout > 0 {
                let mut task = TimeOut::new(key);
                post_delayed_task(ThreadId::UI, Some(&mut task), request.timeout);
            }
        }
        None => reply(&frame, id, &failure("could not start the request")),
    }
}

/// `GM_openInTab(url)` from `script`, running in `frame`.
pub fn open_in_tab(frame: &Frame, script: &str, url: &str, background: bool) {
    let page = page_url(frame);
    let allowed = crate::greasemonkey::script(script)
        .is_some_and(|s| s.applies_to(&page) && s.grants("GM_openInTab"));
    if !allowed || !(url.starts_with("http://") || url.starts_with("https://")) {
        shell::show_message(
            Level::Warning,
            format!("{script}: GM_openInTab({url}) isn't allowed"),
        );
        return;
    }
    let position =
        shell::with(|s| s.new_tab_position(true)).unwrap_or(rt_core::tabs::Position::Next);
    crate::tabs::open(url, position, !background);
}

fn failure(error: &str) -> serde_json::Value {
    serde_json::json!({ "status": 0, "statusText": "", "responseText": "", "responseHeaders": "", "finalUrl": "", "error": error })
}

fn reply(frame: &Frame, id: i32, response: &serde_json::Value) {
    if frame.is_valid() == 0 {
        return;
    }
    let Some(mut message) = process_message_create(Some(&CefString::from(GM_XHR_DONE_MESSAGE)))
    else {
        return;
    };
    if let Some(args) = message.argument_list() {
        args.set_int(0, id);
        args.set_string(1, Some(&CefString::from(response.to_string().as_str())));
    }
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

/// Answer request `key` once, whichever comes first: the response or its timeout.
fn finish(key: u32, response: serde_json::Value) {
    if let Some((_, frame, id)) = PENDING.with(|p| p.borrow_mut().remove(&key)) {
        reply(&frame, id, &response);
    }
}

wrap_task! {
    struct TimeOut {
        key: u32,
    }

    impl Task {
        fn execute(&self) {
            let request = PENDING.with(|p| p.borrow().get(&self.key).map(|(r, _, _)| r.clone()));
            if let Some(request) = request {
                finish(self.key, failure("timeout"));
                request.cancel();
            }
        }
    }
}

wrap_task! {
    struct Finished {
        key: u32,
        response: RefCell<Option<serde_json::Value>>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(response) = self.response.borrow_mut().take() {
                finish(self.key, response);
            }
        }
    }
}

wrap_urlrequest_client! {
    struct XhrClient {
        key: u32,
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
            let Some(request) = request else { return };
            let response = request.response();
            let response = if request.request_status() == UrlrequestStatus::SUCCESS
                && let Some(response) = response
            {
                let mut headers = CefStringMultimap::new();
                response.header_map(Some(&mut headers));
                let headers: String = headers
                    .into_iter()
                    .flat_map(|(name, values)| values.into_iter().map(move |v| format!("{name}: {v}\r\n")))
                    .collect();
                let body = std::mem::take(&mut *self.data.borrow_mut());
                serde_json::json!({
                    "status": response.status(),
                    "statusText": CefString::from(&response.status_text()).to_string(),
                    "responseText": String::from_utf8_lossy(&body),
                    "responseHeaders": headers,
                    "finalUrl": CefString::from(&response.url()).to_string(),
                })
            } else {
                failure("network error")
            };
            // Out of CEF's callback before answering.
            let mut task = Finished::new(self.key, RefCell::new(Some(response)));
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}
