//! Content blocking. Requests are checked on CEF's IO thread against the
//! `rt-adblock` engine; `:adblock-update` downloads the filter lists through
//! Chromium's own network stack and rebuilds the engine on a worker thread.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use cef::*;
use rt_adblock::{Blocker, Store};
use rt_core::Command;
use rt_core::engine::Level;
use rt_core::settings::Settings;

use crate::{eval, shell};

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
                    matches!(value, rt_core::settings::Value::Bool(true)),
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
/// The engine, if blocking is on for `page` (per-site setting, whitelist).
fn blocker_for(page: &str) -> Option<Arc<Blocker>> {
    let state = STATE.read().ok()?;
    let enabled = state
        .enabled_for
        .iter()
        .rev()
        .find(|(pattern, _)| rt_core::url::pattern_matches(pattern, page))
        .map_or(state.enabled, |(_, on)| *on);
    let host = rt_core::url::host(page);
    if !enabled || rt_adblock::whitelisted(host, &state.whitelist) {
        return None;
    }
    state.blocker.clone()
}

/// Requests blocked on each tab's current page, by browser id. Counted on the
/// IO thread, shown by the `blocked` status bar widget.
static BLOCKED: std::sync::Mutex<std::collections::BTreeMap<i32, u32>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());
/// A status bar redraw is already on its way, so a burst of blocks makes one.
static REDRAW_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One more request blocked on `browser`'s page (any thread).
pub fn count_blocked(browser: i32) {
    if let Ok(mut blocked) = BLOCKED.lock() {
        *blocked.entry(browser).or_default() += 1;
    }
    if !REDRAW_PENDING.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let mut task = RedrawBlocked::new();
        post_delayed_task(ThreadId::UI, Some(&mut task), 250);
    }
}

/// A new page started loading in `browser`, or the tab closed.
pub fn reset_blocked(browser: i32) {
    if let Ok(mut blocked) = BLOCKED.lock() {
        blocked.remove(&browser);
    }
}

/// How many requests were blocked on `browser`'s page.
pub fn blocked(browser: i32) -> u32 {
    BLOCKED
        .lock()
        .map(|b| b.get(&browser).copied().unwrap_or(0))
        .unwrap_or(0)
}

wrap_task! {
    struct RedrawBlocked {}

    impl Task {
        fn execute(&self) {
            REDRAW_PENDING.store(false, std::sync::atomic::Ordering::Relaxed);
            shell::refresh_ui();
        }
    }
}

/// What to do with a request (on the IO thread). Top-level pages are never
/// blocked, so a mistaken rule can't make a site unreachable, but their
/// tracking parameters are taken out (`$removeparam`).
pub fn check(url: &str, page: &str, resource: ResourceType) -> rt_adblock::Verdict {
    use rt_adblock::Verdict;
    let top = matches!(
        resource,
        ResourceType::MAIN_FRAME | ResourceType::NAVIGATION_PRELOAD_MAIN_FRAME
    );
    if top {
        return match blocker_for(url).map(|b| b.check(url, url, "document")) {
            Some(Verdict::Rewrite(to)) => Verdict::Rewrite(to),
            _ => Verdict::Allow,
        };
    }
    let Some(kind) = kind(resource) else {
        return Verdict::Allow;
    };
    let Some(blocker) = blocker_for(page) else {
        return Verdict::Allow;
    };
    let verdict = blocker.check(url, page, kind);
    if verdict != Verdict::Allow {
        tracing::debug!(url, page, kind, ?verdict, "adblock");
    }
    verdict
}

/// `$redirect` stand-ins waiting for their request's resource handler, by
/// request id (both run on the IO thread).
static STAND_INS: std::sync::Mutex<std::collections::BTreeMap<u64, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub fn hold_stand_in(request: u64, data_url: String) {
    if let Ok(mut held) = STAND_INS.lock() {
        held.insert(request, data_url);
    }
}

/// The stand-in for `request`, decoded from its `data:` URL: the bytes and
/// their MIME type.
pub fn take_stand_in(request: u64) -> Option<(Vec<u8>, &'static str)> {
    let data_url = STAND_INS.lock().ok()?.remove(&request)?;
    let (head, body) = data_url.strip_prefix("data:")?.split_once(',')?;
    let mime = head.trim_end_matches(";base64");
    let bytes = if head.ends_with(";base64") {
        rt_core::extensions::base64_decode(body)?
    } else {
        body.as_bytes().to_vec()
    };
    Some((bytes, crate::scheme::static_mime(mime)))
}

/// Scriptlets (`##+js(...)`) must run before the page's own scripts. Each
/// tab gets a site's scriptlets once, through DevTools, which runs them in
/// every new document whose host is that site's, before anything else.
pub fn before_navigation(browser: &Browser, url: &str) {
    let Some(blocker) = blocker_for(url) else {
        return;
    };
    let host = rt_core::url::host(url).to_string();
    if host.is_empty() {
        return;
    }
    let key = (browser.identifier(), host.clone());
    if SCRIPTLETS_SENT.with(|s| s.borrow().contains(&key)) {
        return;
    }
    let script = blocker.cosmetic(url).script;
    SCRIPTLETS_SENT.with(|s| s.borrow_mut().insert(key));
    if script.is_empty() {
        return;
    }
    let host_json = serde_json::to_string(&host).unwrap_or_default();
    let source = format!(
        "if (location.hostname === {host_json}) {{ (function () {{ const scriptletGlobals = {{}};\n{script}\n}})(); }}"
    );
    let message = serde_json::json!({
        "id": 1,
        "method": "Page.addScriptToEvaluateOnNewDocument",
        "params": { "source": source },
    })
    .to_string();
    if let Some(host) = browser.host() {
        // The registration only takes with the Page domain on.
        let enable = serde_json::json!({ "id": 1, "method": "Page.enable" }).to_string();
        host.send_dev_tools_message(Some(enable.as_bytes()));
        host.send_dev_tools_message(Some(message.as_bytes()));
    }
}

thread_local! {
    /// Which tabs (browser ids) have been given which hosts' scriptlets.
    static SCRIPTLETS_SENT: RefCell<std::collections::HashSet<(i32, String)>> =
        RefCell::new(std::collections::HashSet::new());
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

/// Every class and id in the page, for the generic hiding rules.
const COLLECT_JS: &str = "JSON.stringify((() => {
  const classes = new Set(), ids = new Set();
  for (const el of document.querySelectorAll('[class],[id]')) {
    if (el.id) ids.add(el.id);
    for (const c of el.classList) classes.add(c);
    if (classes.size + ids.size > 20000) break;
  }
  return { classes: [...classes], ids: [...ids] };
})())";

#[derive(serde::Deserialize)]
struct ClassesAndIds {
    classes: Vec<String>,
    ids: Vec<String>,
}

/// Hide ads with the lists' element-hiding rules once a page has loaded:
/// its site-specific rules, then the generic ones for the classes and ids
/// it uses.
pub fn apply_cosmetic(browser: &Browser, url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    let Some(blocker) = blocker_for(url) else {
        return;
    };
    let cosmetic = Arc::new(blocker.cosmetic(url));
    inject_css(browser, &cosmetic.css);
    if !cosmetic.generic {
        return;
    }
    hide_generic(browser, url, blocker.clone(), cosmetic.clone());
    // Ads often arrive after the load; look again a little later.
    for delay in [2000, 6000] {
        let mut task = Recheck::new(
            browser.clone(),
            url.to_string(),
            blocker.clone(),
            cosmetic.clone(),
        );
        post_delayed_task(ThreadId::UI, Some(&mut task), delay);
    }
}

fn hide_generic(
    browser: &Browser,
    url: &str,
    blocker: Arc<Blocker>,
    cosmetic: Arc<rt_adblock::Cosmetic>,
) {
    // Stop if the tab moved on to another page meanwhile.
    let current = browser
        .main_frame()
        .map(|f| CefString::from(&f.url()).to_string());
    if current.as_deref() != Some(url) {
        return;
    }
    let target = browser.clone();
    eval::eval(browser, COLLECT_JS, move |result| {
        let Some(found) = result
            .ok()
            .and_then(|json| serde_json::from_str::<ClassesAndIds>(&json).ok())
        else {
            return;
        };
        inject_css(
            &target,
            &blocker.generic_css(&found.classes, &found.ids, &cosmetic),
        );
    });
}

wrap_task! {
    struct Recheck {
        browser: Browser,
        url: String,
        blocker: Arc<Blocker>,
        cosmetic: Arc<rt_adblock::Cosmetic>,
    }

    impl Task {
        fn execute(&self) {
            hide_generic(&self.browser, &self.url, self.blocker.clone(), self.cosmetic.clone());
        }
    }
}

/// Add CSS rules to the page's riptide style element (skipping ones it has).
pub(crate) fn inject_css(browser: &Browser, css: &str) {
    if css.is_empty() {
        return;
    }
    let css = serde_json::to_string(css).unwrap_or_default();
    // Rules already in the style are skipped, so re-checks don't pile up.
    let code = format!(
        "(() => {{ let s = document.getElementById('__rt_cosmetic'); \
         if (!s) {{ s = document.createElement('style'); s.id = '__rt_cosmetic'; \
         (document.head || document.documentElement).appendChild(s); }} \
         const have = new Set(s.textContent.split('\\n')); \
         const add = {css}.split('\\n').filter((r) => r && !have.has(r)); \
         if (add.length) s.textContent += add.join('\\n') + '\\n'; return 'null'; }})()"
    );
    eval::eval(browser, &code, |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_pages_are_never_checked() {
        assert_eq!(kind(ResourceType::MAIN_FRAME), None);
        assert_eq!(kind(ResourceType::NAVIGATION_PRELOAD_MAIN_FRAME), None);
    }

    #[test]
    fn requests_map_to_filter_list_types() {
        assert_eq!(kind(ResourceType::SUB_FRAME), Some("sub_frame"));
        assert_eq!(kind(ResourceType::SCRIPT), Some("script"));
        assert_eq!(kind(ResourceType::WORKER), Some("script"));
        assert_eq!(kind(ResourceType::IMAGE), Some("image"));
        assert_eq!(kind(ResourceType::FAVICON), Some("image"));
        assert_eq!(kind(ResourceType::XHR), Some("xmlhttprequest"));
        assert_eq!(kind(ResourceType::PREFETCH), Some("other"));
    }
}
