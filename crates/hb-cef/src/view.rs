//! Commands about how a tab is shown: zoom, fullscreen, mute, devtools,
//! printing, page source, JavaScript evaluation and the message log.

use std::sync::{Arc, RwLock};

use cef::*;
use hb_core::Command;
use hb_core::command::OpenTarget;
use hb_core::engine::Level;

use crate::{eval, shell};

const MESSAGES_TEMPLATE: &str = include_str!("../ui/messages.html");

/// `hb://messages/`, rebuilt each time `:messages` runs.
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
        | Command::Messages => {}
        _ => return false,
    }
    if let Command::Messages = command {
        show_messages();
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
            let current = hb_core::zoom::to_percent(host.zoom_level());
            let next = hb_core::zoom::step(current, if *out { -steps } else { steps });
            set_zoom(&browser, &host, next);
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
        Command::JsEval { code } => {
            let code = format!(
                "(() => {{ const r = eval({}); return r === undefined ? 'undefined' : String(r); }})()",
                serde_json::to_string(code).unwrap_or_default()
            );
            eval::eval(&browser, &code, |result| {
                let (level, text) = match result {
                    Ok(json) => (
                        Level::Info,
                        serde_json::from_str::<String>(&json).unwrap_or(json),
                    ),
                    Err(e) => (Level::Error, e),
                };
                shell::show_message(level, text);
                shell::refresh_ui();
            });
        }
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
    host.set_zoom_level(hb_core::zoom::to_level(percent));
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
        host.set_zoom_level(hb_core::zoom::to_level(default as f64));
        note_zoom(browser, default as f64);
    } else {
        note_zoom(browser, hb_core::zoom::to_percent(level));
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
                .replace("/*HB_DATA*/null", &json)
                .into_bytes(),
        ));
    }
    shell::open(OpenTarget::Tab, true, Some("hb://messages/".to_string()));
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
