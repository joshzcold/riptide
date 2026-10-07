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
        | Command::ShareStop
        | Command::Pip
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
        Command::ShareStop => share_stop(),
        Command::Pip => pip(),
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

/// What a `getDisplayMedia` display surface is called in messages.
pub fn share_name(surface: &str) -> &'static str {
    match surface {
        "browser" => "a tab",
        "window" => "a window",
        _ => "your screen",
    }
}

/// `:share-stop`: ask every frame of each sharing tab, in any window, to stop.
fn share_stop() {
    let tabs: Vec<Browser> = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| t.sharing.is_some())
            .filter_map(|t| t.browser())
            .collect()
    })
    .unwrap_or_default();
    if tabs.is_empty() {
        return shell::show_message(Level::Error, "Nothing is being shared");
    }
    for browser in &tabs {
        for frame in crate::hints::all_frames(browser) {
            if let Some(mut message) =
                process_message_create(Some(&CefString::from(crate::renderer::SHARE_STOP_MESSAGE)))
            {
                frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
            }
        }
    }
    shell::show_message(Level::Info, "Stopped sharing");
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
    crate::client::send_when_ready(keys, move || crate::tabs::select(current));
}

/// `:pip`: float the page's main video in a picture-in-picture window, or
/// bring it back. Pages may only do this right after a real key or click,
/// and riptide's own keys never reach the page, so the page gets a script
/// waiting for F24 (which no site uses) and then a real F24.
fn pip() {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    let check = browser.clone();
    crate::eval::eval(&browser, PIP_JS, move |armed| {
        if armed.is_err() {
            return shell::show_message(Level::Error, "This page can't show picture-in-picture");
        }
        let f24 = rt_core::Key::plain(rt_core::KeyCode::F(24));
        crate::client::send_when_ready(vec![f24], move || report_pip(check, 0));
    });
}

/// Waits for an F24 keydown, then floats the largest playing video (or
/// closes picture-in-picture) and leaves the outcome in `window.__rtPip`.
const PIP_JS: &str = r#"(() => {
  window.__rtPip = "waiting";
  window.addEventListener("keydown", async function pip(e) {
    if (e.key !== "F24") return;
    e.preventDefault();
    e.stopImmediatePropagation();
    window.removeEventListener("keydown", pip, true);
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
        window.__rtPip = "closed";
        return;
      }
      const score = (v) => v.videoWidth * v.videoHeight + (v.paused ? 0 : 1e9);
      const videos = [...document.querySelectorAll("video")]
        .filter((v) => v.readyState >= 2 && !v.disablePictureInPicture)
        .sort((a, b) => score(b) - score(a));
      if (videos.length === 0) {
        window.__rtPip = "none";
        return;
      }
      await videos[0].requestPictureInPicture();
      window.__rtPip = "open";
    } catch (err) {
      window.__rtPip = "error " + err.message;
    }
  }, true);
  return "armed";
})()"#;

/// Say how `:pip` went, once the page's script has run.
fn report_pip(browser: Browser, polls: u32) {
    let again = browser.clone();
    crate::eval::eval(&browser, "String(window.__rtPip)", move |result| {
        let message = match result.as_deref() {
            Ok("waiting") if polls < 30 => {
                return crate::client::later(100, move || report_pip(again, polls + 1));
            }
            Ok("open") => (
                Level::Info,
                "Playing in picture-in-picture; :pip again brings it back".to_string(),
            ),
            Ok("closed") => (Level::Info, "Picture-in-picture closed".to_string()),
            Ok("none") => (
                Level::Error,
                "There's no video playing on this page".to_string(),
            ),
            Ok(other) if other.starts_with("error ") => (
                Level::Error,
                format!("Picture-in-picture failed: {}", &other[6..]),
            ),
            _ => (
                Level::Error,
                "The page didn't answer the picture-in-picture request".to_string(),
            ),
        };
        shell::show_message(message.0, message.1);
    });
}
