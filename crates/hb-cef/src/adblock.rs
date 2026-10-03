//! Content blocking. Requests are checked on CEF's IO thread against the
//! `hb-adblock` engine; `:adblock-update` downloads the filter lists through
//! Chromium's own network stack and rebuilds the engine on a worker thread.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use cef::*;
use hb_adblock::{Blocker, Store};
use hb_core::Command;
use hb_core::engine::Level;
use hb_core::settings::Settings;

use crate::shell;

struct State {
    blocker: Option<Arc<Blocker>>,
    enabled: bool,
    /// Per-site `content.blocking.enabled`, as `(pattern, enabled)`; the last match wins.
    enabled_for: Vec<(String, bool)>,
    whitelist: Vec<String>,
}

static STATE: RwLock<State> = RwLock::new(State {
    blocker: None,
    enabled: true,
    enabled_for: Vec::new(),
    whitelist: Vec::new(),
});

thread_local! {
    /// Downloads in flight, kept alive until they finish.
    static UPDATE: RefCell<Option<Update>> = const { RefCell::new(None) };
}

struct Update {
    requests: Vec<Urlrequest>,
    pending: usize,
    failed: Vec<String>,
}

/// Copy the settings the IO thread needs. Runs on the UI thread.
pub fn sync_settings(settings: &Settings) {
    if let Ok(mut state) = STATE.write() {
        state.enabled = settings.bool("content.blocking.enabled");
        state.enabled_for = settings
            .overrides("content.blocking.enabled")
            .into_iter()
            .map(|(pattern, value)| {
                (
                    pattern,
                    matches!(value, hb_core::settings::Value::Bool(true)),
                )
            })
            .collect();
        state.whitelist = settings.list("content.blocking.whitelist").to_vec();
    }
}

/// Load the cached engine (or compile the saved lists) in the background.
pub fn load(data_dir: PathBuf, lists: Vec<String>) {
    std::thread::spawn(move || {
        let blocker = Store::new(&data_dir).load(&lists);
        let loaded = blocker.is_some();
        if let Ok(mut state) = STATE.write() {
            state.blocker = blocker.map(Arc::new);
        }
        if !loaded {
            shell::post_message(
                Level::Info,
                "Content blocking has no filter lists yet; run :adblock-update".into(),
            );
        }
    });
}

/// Whether to cancel a request. `page` is the URL of the tab's top frame.
pub fn should_block(url: &str, page: &str, resource: ResourceType) -> bool {
    let Some(kind) = kind(resource) else {
        return false;
    };
    let Ok(state) = STATE.read() else {
        return false;
    };
    let enabled = state
        .enabled_for
        .iter()
        .rev()
        .find(|(pattern, _)| hb_core::url::pattern_matches(pattern, page))
        .map_or(state.enabled, |(_, on)| *on);
    let Some(blocker) = state.blocker.as_ref().filter(|_| enabled) else {
        return false;
    };
    let host = hb_core::url::host(page);
    if hb_adblock::whitelisted(host, &state.whitelist) {
        return false;
    }
    let blocked = blocker.should_block(url, page, kind);
    if blocked {
        tracing::debug!(url, page, kind, "blocked");
    }
    blocked
}

/// The adblock request type. Top-level pages are never blocked, so a
/// mistaken rule can't make a site unreachable.
fn kind(resource: ResourceType) -> Option<&'static str> {
    Some(match resource {
        ResourceType::MAIN_FRAME | ResourceType::NAVIGATION_PRELOAD_MAIN_FRAME => return None,
        ResourceType::SUB_FRAME | ResourceType::NAVIGATION_PRELOAD_SUB_FRAME => "sub_frame",
        ResourceType::STYLESHEET => "stylesheet",
        ResourceType::SCRIPT | ResourceType::WORKER | ResourceType::SHARED_WORKER => "script",
        ResourceType::IMAGE | ResourceType::FAVICON => "image",
        ResourceType::FONT_RESOURCE => "font",
        ResourceType::OBJECT | ResourceType::PLUGIN_RESOURCE => "object",
        ResourceType::MEDIA => "media",
        ResourceType::XHR => "xmlhttprequest",
        ResourceType::PING => "ping",
        ResourceType::CSP_REPORT => "csp_report",
        _ => "other",
    })
}

pub fn run_command(command: &Command) -> bool {
    if !matches!(command, Command::AdblockUpdate) {
        return false;
    }
    if UPDATE.with(|u| u.borrow().is_some()) {
        shell::show_message(Level::Info, "Filter lists are already being updated");
        return true;
    }
    let Some((data_dir, lists)) = shell::with(|s| {
        (
            s.paths.data_dir.clone(),
            s.engine
                .settings()
                .list("content.blocking.adblock.lists")
                .to_vec(),
        )
    }) else {
        return true;
    };
    if lists.is_empty() {
        shell::show_message(Level::Error, "content.blocking.adblock.lists is empty");
        return true;
    }
    let store = Store::new(&data_dir);
    let mut update = Update {
        requests: Vec::new(),
        pending: 0,
        failed: Vec::new(),
    };
    for url in &lists {
        if let Some(path) = url.strip_prefix("file://") {
            let saved = std::fs::read_to_string(path).and_then(|text| store.save_list(url, &text));
            if let Err(e) = saved {
                update.failed.push(format!("{url} ({e})"));
            }
            continue;
        }
        let Some(mut request) = request_create() else {
            continue;
        };
        request.set_url(Some(&CefString::from(url.as_str())));
        request.set_method(Some(&CefString::from("GET")));
        let mut client = ListClient::new(url.clone(), RefCell::new(Vec::new()));
        match urlrequest_create(Some(&mut request), Some(&mut client), None) {
            Some(r) => {
                update.requests.push(r);
                update.pending += 1;
            }
            None => update.failed.push(url.clone()),
        }
    }
    let pending = update.pending;
    UPDATE.with(|u| *u.borrow_mut() = Some(update));
    shell::show_message(
        Level::Info,
        format!("Downloading {} filter list(s)…", lists.len()),
    );
    if pending == 0 {
        finish();
    }
    true
}

/// One list finished downloading (or failed). Runs on the UI thread.
fn list_done(url: &str, text: Option<String>) {
    let Some(data_dir) = shell::with(|s| s.paths.data_dir.clone()) else {
        return;
    };
    let error = match text {
        Some(text) => Store::new(&data_dir)
            .save_list(url, &text)
            .err()
            .map(|e| e.to_string()),
        None => Some("download failed".into()),
    };
    let done = UPDATE.with(|u| {
        let mut u = u.borrow_mut();
        let Some(update) = u.as_mut() else {
            return false;
        };
        if let Some(e) = error {
            update.failed.push(format!("{url} ({e})"));
        }
        update.pending = update.pending.saturating_sub(1);
        update.pending == 0
    });
    if done {
        finish();
    }
}

/// All downloads are done: compile on a worker thread and swap the engine in.
fn finish() {
    let Some(update) = UPDATE.with(|u| u.borrow_mut().take()) else {
        return;
    };
    let Some((data_dir, lists)) = shell::with(|s| {
        (
            s.paths.data_dir.clone(),
            s.engine
                .settings()
                .list("content.blocking.adblock.lists")
                .to_vec(),
        )
    }) else {
        return;
    };
    std::thread::spawn(move || {
        let compiled = Store::new(&data_dir).compile(&lists);
        let message = match compiled {
            Some((blocker, rules)) => {
                if let Ok(mut state) = STATE.write() {
                    state.blocker = Some(Arc::new(blocker));
                }
                let ok = lists.len() - update.failed.len();
                match update.failed.as_slice() {
                    [] => (
                        Level::Info,
                        format!("Content blocking: {rules} rules from {ok} list(s)"),
                    ),
                    failed => (
                        Level::Warning,
                        format!(
                            "Content blocking: {rules} rules; failed: {}",
                            failed.join(", ")
                        ),
                    ),
                }
            }
            None => (
                Level::Error,
                format!(
                    "No filter lists could be downloaded: {}",
                    update.failed.join(", ")
                ),
            ),
        };
        shell::post_message(message.0, message.1);
    });
}

wrap_task! {
    struct ListDone {
        url: String,
        text: RefCell<Option<String>>,
    }

    impl Task {
        fn execute(&self) {
            list_done(&self.url, self.text.borrow_mut().take());
            shell::refresh_ui();
        }
    }
}

wrap_urlrequest_client! {
    struct ListClient {
        url: String,
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
            let text = ok.then(|| String::from_utf8_lossy(&self.data.borrow()).into_owned());
            if !ok {
                tracing::warn!(url = %self.url, status, "filter list download failed");
            }
            // The client runs on the UI thread already, but posting keeps
            // `list_done` out of CEF's callback.
            let mut task = ListDone::new(self.url.clone(), RefCell::new(text));
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}
