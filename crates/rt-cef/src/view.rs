//! Commands about how a tab is shown: zoom, fullscreen, mute, devtools,
//! printing, page source, JavaScript evaluation and the message log.

use std::sync::{Arc, RwLock};

use cef::*;
use rt_core::Command;
use rt_core::command::OpenTarget;
use rt_core::engine::Level;

use crate::{eval, shell};

const MESSAGES_TEMPLATE: &str = include_str!("../ui/messages.html");

/// `riptide://messages/`, rebuilt each time `:messages` runs.
static MESSAGES_PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn messages_page() -> Arc<[u8]> {
    let cached = MESSAGES_PAGE.read().ok().and_then(|p| p.clone());
    cached.unwrap_or_else(|| Arc::from(MESSAGES_TEMPLATE.as_bytes()))
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::Zoom { .. }
        | Command::ZoomStep { .. }
        | Command::DevTools
        | Command::Print { .. }
        | Command::Fullscreen
        | Command::ViewSource
        | Command::JsEval { .. }
        | Command::Home
        | Command::TabMute
        | Command::CallMute
        | Command::DevToolsFocus
        | Command::DebugDumpPage { .. }
        | Command::DebugClearSslErrors
        | Command::Messages => {}
        _ => return false,
    }
    if let Command::Messages = command {
        show_messages();
        return true;
    }
    if let Command::DebugClearSslErrors = command {
        if let Some(context) = request_context_get_global_context() {
            context.clear_certificate_exceptions(None);
            shell::show_message(
                Level::Info,
                "Forgot the certificate errors allowed this session",
            );
        }
        return true;
    }
    if let Command::Fullscreen = command {
        if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
            let on = window.is_fullscreen() == 0;
            window.set_fullscreen(on.into());
        }
        return true;
    }
    if let Command::Home = command {
        let home = shell::with(|s| s.engine.settings().str("url.default_page").to_string());
        shell::open(OpenTarget::Current, false, home);
        return true;
    }
    let Some((browser, url)) = shell::with(|s| {
        let tab = s.tabs.current()?;
        Some((tab.browser()?, tab.url.clone()))
    })
    .flatten() else {
        return true;
    };
    let Some(host) = browser.host() else {
        return true;
    };
    match command {
        Command::Zoom { percent } => {
            let default = shell::with(|s| s.engine.settings().int("zoom.default")).unwrap_or(100);
            let percent = percent
                .map(i64::from)
                .or(count.map(i64::from))
                .unwrap_or(default);
            set_zoom(&browser, &host, percent.clamp(5, 1000) as f64);
        }
        Command::ZoomStep { out } => {
            let steps = i64::from(count.unwrap_or(1).max(1));
            let current = rt_core::zoom::to_percent(host.zoom_level());
            let levels = shell::with(|s| {
                rt_core::zoom::levels_from(s.engine.settings().list("zoom.levels"))
            })
            .unwrap_or_default();
            let next = rt_core::zoom::step_in(&levels, current, if *out { -steps } else { steps });
            set_zoom(&browser, &host, next);
        }
        Command::DevToolsFocus => match crate::window::devtools_window(browser.identifier()) {
            Some(window) => window.activate(),
            None => shell::show_message(
                Level::Error,
                "This tab has no developer tools open; use :devtools",
            ),
        },
        Command::DebugDumpPage { path } => {
            let path = expand_home(path);
            eval::eval(
                &browser,
                "document.documentElement.outerHTML",
                move |result| {
                    let saved = result
                        .and_then(|html| std::fs::write(&path, html).map_err(|e| e.to_string()));
                    match saved {
                        Ok(()) => {
                            shell::show_message(Level::Info, format!("Saved the page to {path}"))
                        }
                        Err(e) => {
                            shell::show_message(Level::Error, format!("Can't save the page: {e}"))
                        }
                    }
                    shell::refresh_ui();
                },
            );
        }
        Command::DevTools => {
            if host.has_dev_tools() != 0 {
                host.close_dev_tools();
            } else {
                let info = WindowInfo {
                    runtime_style: RuntimeStyle::CHROME,
                    ..Default::default()
                };
                host.show_dev_tools(Some(&info), None, None, None);
            }
        }
        Command::Print { pdf: None } => host.print(),
        Command::Print { pdf: Some(path) } => {
            let path = expand_home(path);
            let settings = PdfPrintSettings {
                print_background: 1,
                ..Default::default()
            };
            let mut callback = PdfDone::new();
            host.print_to_pdf(
                Some(&CefString::from(path.as_str())),
                Some(&settings),
                Some(&mut callback),
            );
        }
        Command::ViewSource => {
            if url.starts_with("view-source:") {
                shell::show_message(Level::Error, "Already showing the source");
            } else {
                shell::open(OpenTarget::Tab, true, Some(format!("view-source:{url}")));
            }
        }
        Command::JsEval { code, quiet, file } => {
            let script;
            let code = if *file {
                match std::fs::read_to_string(expand_home(code)) {
                    Ok(text) => {
                        script = text;
                        &script
                    }
                    Err(e) => {
                        shell::show_message(Level::Error, format!("Can't read {code}: {e}"));
                        return true;
                    }
                }
            } else {
                code
            };
            let quiet = *quiet;
            let code = format!(
                "(() => {{ const r = eval({}); return r === undefined ? 'undefined' : String(r); }})()",
                serde_json::to_string(code).unwrap_or_default()
            );
            eval::eval(&browser, &code, move |result| {
                let (level, text) = match result {
                    Ok(json) => (
                        Level::Info,
                        serde_json::from_str::<String>(&json).unwrap_or(json),
                    ),
                    Err(e) => (Level::Error, e),
                };
                if !(quiet && level == Level::Info) {
                    shell::show_message(level, text);
                    shell::refresh_ui();
                }
            });
        }
        Command::CallMute => call_mute(),
        Command::TabMute => {
            let muted = host.is_audio_muted() == 0;
            host.set_audio_muted(muted.into());
            shell::with_tab(Some(&mut browser.clone()), |s, index, _| {
                if let Some(tab) = s.tabs.get_mut(index) {
                    tab.muted = muted;
                }
            });
            let what = if muted { "Muted" } else { "Unmuted" };
            shell::show_message(Level::Info, format!("{what} this tab"));
        }
        _ => {}
    }
    true
}

fn set_zoom(browser: &Browser, host: &BrowserHost, percent: f64) {
    host.set_zoom_level(rt_core::zoom::to_level(percent));
    note_zoom(browser, percent);
    shell::show_message(Level::Info, format!("Zoom level: {}%", percent.round()));
}

fn note_zoom(browser: &Browser, percent: f64) {
    shell::with_tab(Some(&mut browser.clone()), |s, index, _| {
        if let Some(tab) = s.tabs.get_mut(index) {
            tab.zoom = percent.round() as u32;
        }
    });
}

/// After a page loads: give sites without a saved zoom `zoom.default`,
/// and note the zoom for the status bar.
pub fn loaded(browser: &Browser) {
    let Some(host) = browser.host() else { return };
    let level = host.zoom_level();
    let default = shell::with(|s| s.engine.settings().int("zoom.default")).unwrap_or(100);
    if level == 0.0 && default != 100 {
        host.set_zoom_level(rt_core::zoom::to_level(default as f64));
        note_zoom(browser, default as f64);
    } else {
        note_zoom(browser, rt_core::zoom::to_percent(level));
    }
}

/// A page went fullscreen (e.g. a video), or left it: the window follows,
/// and the bars hide so only the page shows.
pub fn page_fullscreen(on: bool) {
    let Some((window, bars)) = shell::with(|s| {
        let bars: Vec<BrowserView> = [s.tabbar.clone(), s.statusbar.clone()]
            .into_iter()
            .flatten()
            .collect();
        (s.window.clone(), bars)
    }) else {
        return;
    };
    for bar in bars {
        View::from(&bar).set_visible(i32::from(!on));
    }
    if let Some(window) = window {
        window.set_fullscreen(on.into());
    }
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => path.to_string(),
    }
}

fn show_messages() {
    let log = shell::with(|s| s.engine.message_log()).unwrap_or_default();
    // `</` would end the inline <script> early.
    let json = serde_json::to_string(&log)
        .unwrap_or_else(|_| "[]".into())
        .replace("</", "<\\/");
    if let Ok(mut page) = MESSAGES_PAGE.write() {
        *page = Some(Arc::from(
            MESSAGES_TEMPLATE
                .replace("/*RT_DATA*/null", &json)
                .into_bytes(),
        ));
    }
    shell::open(
        OpenTarget::Tab,
        true,
        Some("riptide://messages/".to_string()),
    );
}

wrap_pdf_print_callback! {
    struct PdfDone {}

    impl PdfPrintCallback {
        fn on_pdf_print_finished(&self, path: Option<&CefString>, ok: ::std::os::raw::c_int) {
            let path = path.map(CefString::to_string).unwrap_or_default();
            if ok != 0 {
                shell::show_message(Level::Info, format!("Saved {path}"));
            } else {
                shell::show_message(Level::Error, format!("Could not save {path}"));
            }
            shell::refresh_ui();
        }
    }
}

/// `:call-mute`: the call is the tab using a microphone. Its site's own
/// mute key keeps the site's mute button in step. Chromium only takes keys
/// in the tab that's showing, so a call in another tab of this window is
/// shown just long enough for the key, then the tab you were on comes back.
fn call_mute() {
    let call = shell::with(|s| {
        let (window, index) = s
            .windows
            .iter()
            .enumerate()
            .find_map(|(w, state)| Some((w, state.tabs.iter().position(|t| t.media.1)?)))?;
        let tab = s.windows[window].tabs.get(index)?;
        let keys = s
            .engine
            .settings()
            .map("content.call_mute_keys")
            .cloned()
            .unwrap_or_default();
        let key = keys
            .iter()
            .find(|(pattern, _)| rt_core::url::pattern_matches(pattern, &tab.url))
            .map(|(_, key)| key.clone());
        let here = window == s.active;
        let current = s.windows[window].tabs.current_index();
        Some((tab.url.clone(), key, here, index, current))
    })
    .flatten();
    let Some((url, key, here, index, current)) = call else {
        return shell::show_message(Level::Error, "No tab is using a microphone");
    };
    let site = rt_core::url::host(&url).to_string();
    let Some(key) = key else {
        return shell::show_message(
            Level::Error,
            format!("No mute key known for {site}; add one to content.call_mute_keys"),
        );
    };
    let keys = match rt_core::Key::parse_sequence(&key) {
        Ok(keys) => keys,
        Err(e) => {
            return shell::show_message(
                Level::Error,
                format!("content.call_mute_keys for {site}: {e}"),
            );
        }
    };
    if !here {
        return shell::show_message(
            Level::Error,
            format!("The call ({site}) is in another window; press {key} there"),
        );
    }
    shell::show_message(
        Level::Info,
        format!("Pressed {key} in {site} to mute or unmute"),
    );
    if index != current {
        crate::tabs::select(index);
    }
    // The first key a page gets starts something in Chromium that loses
    // keys for a few hundred milliseconds, so a page that hasn't had one
    // gets a bare Shift first and the mute key a moment later.
    let warm = shell::with(|s| s.current_browser())
        .flatten()
        .is_some_and(|b| crate::client::had_keys(b.identifier()));
    if warm {
        return press_in_call(keys, current);
    }
    crate::client::prime_page();
    let mut task = PressInCall::new(std::cell::RefCell::new(keys), current);
    post_delayed_task(ThreadId::UI, Some(&mut task), FIRST_KEY_DELAY_MS);
}

/// How long after a page's first key the next one gets through, with room
/// to spare on a busy machine (about 400 ms was needed under Xvfb).
const FIRST_KEY_DELAY_MS: i64 = 700;

fn press_in_call(keys: Vec<rt_core::Key>, back_to: usize) {
    for key in keys {
        crate::client::send_to_page(key);
    }
    crate::tabs::select(back_to);
}

wrap_task! {
    struct PressInCall {
        keys: std::cell::RefCell<Vec<rt_core::Key>>,
        back_to: usize,
    }

    impl Task {
        fn execute(&self) {
            press_in_call(self.keys.take(), self.back_to);
        }
    }
}
