//! Hint mode glue: asks the page for hintable elements, draws labels, and
//! performs the chosen action. Label logic lives in `rt_core::hints`.

use cef::*;
use rt_core::engine::Level;
use rt_core::hints::{HintItem, HintRequest, HintTarget};
use rt_core::tabs::Position;
use serde::Deserialize;

use crate::{clipboard, eval, shell, tabs};

const HINTS_JS: &str = include_str!("../js/hints.js");

#[derive(Deserialize)]
struct Item {
    url: Option<String>,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

/// `:hint` — collect elements in the current tab, then enter hint mode.
pub fn request(request: HintRequest) {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    let groups =
        shell::with(|s| rt_core::settings::hint_selectors(s.engine.settings())).unwrap_or_default();
    let Some(selectors) = groups.get(&request.group) else {
        let names: Vec<&str> = groups.keys().map(String::as_str).collect();
        return shell::show_message(
            Level::Error,
            format!(
                "No hint group {:?}; hints.selectors has {}",
                request.group,
                names.join(", ")
            ),
        );
    };
    let selectors = serde_json::to_string(selectors).unwrap_or_default();
    let code = format!("{HINTS_JS}; window.__rtHints.collect({selectors})");
    let id = browser.identifier();
    eval::eval(&browser, &code, move |result| {
        let items: Vec<Item> = match result.map(|json| serde_json::from_str(&json)) {
            Ok(Ok(items)) => items,
            Ok(Err(e)) => return shell::show_message(Level::Error, format!("Hints failed: {e}")),
            Err(e) => return shell::show_message(Level::Error, format!("Hints failed: {e}")),
        };
        let items = items
            .into_iter()
            .map(|i| HintItem {
                url: i.url,
                text: i.text,
            })
            .collect();
        load_dictionary();
        let effects = shell::with(|s| {
            // Ignore stale replies if the user switched tabs meanwhile.
            if s.current_browser().map(|b| b.identifier()) != Some(id) {
                return Vec::new();
            }
            s.hint_browser = Some(id);
            s.engine.start_hints(request, items)
        });
        shell::apply(effects.unwrap_or_default());
    });
}

thread_local! {
    /// The `hints.dictionary` last read, by path.
    static DICTIONARY: std::cell::RefCell<Option<(String, std::rc::Rc<[String]>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Read `hints.dictionary` for word hints, once per path.
fn load_dictionary() {
    let Some(path) = shell::with(|s| {
        let settings = s.engine.settings();
        (settings.str("hints.mode") == "word").then(|| settings.str("hints.dictionary").to_string())
    })
    .flatten() else {
        return;
    };
    let words = DICTIONARY.with(|d| {
        let mut d = d.borrow_mut();
        if let Some((loaded, words)) = d.as_ref()
            && *loaded == path
        {
            return words.clone();
        }
        let path_buf = crate::screenshot::expand_home(&path);
        let words: std::rc::Rc<[String]> = match std::fs::read_to_string(&path_buf) {
            Ok(text) => rt_core::hints::dictionary_words(&text).into(),
            Err(e) => {
                tracing::warn!(%e, path, "can't read hints.dictionary");
                Vec::new().into()
            }
        };
        *d = Some((path, words.clone()));
        words
    });
    shell::with(|s| s.engine.set_hint_words(words));
}

fn hint_browser() -> Option<Browser> {
    shell::with(|s| {
        let id = s.hint_browser?;
        s.tabs
            .iter()
            .filter_map(|t| t.browser())
            .find(|b| b.identifier() == id)
    })
    .flatten()
}

fn call(method: &str, arg: &str) {
    if let Some(browser) = hint_browser() {
        eval::eval(
            &browser,
            &format!("window.__rtHints.{method}({arg})"),
            |_| {},
        );
    }
}

fn related_position() -> Position {
    shell::with(|s| s.new_tab_position(true)).unwrap_or(Position::Next)
}

pub fn show(labels: &[String]) {
    let upper = shell::with(|s| s.engine.settings().bool("hints.uppercase")).unwrap_or(false);
    let labels = serde_json::to_string(labels).unwrap_or_default();
    call("show", &format!("{labels}, {upper}"));
}

pub fn filter(typed: &str) {
    let hide = shell::with(|s| {
        let rapid = s.engine.hint_session().is_some_and(|h| h.request.rapid);
        !rapid || s.engine.settings().bool("hints.hide_unmatched_rapid_hints")
    })
    .unwrap_or(true);
    let typed = serde_json::to_string(typed).unwrap_or_default();
    call("filter", &format!("{typed}, {hide}"));
}

pub fn clear() {
    call("clear", "");
}

pub fn follow(index: usize, url: Option<String>, target: HintTarget) {
    let Some(browser) = hint_browser() else {
        return;
    };
    match (target, url) {
        (HintTarget::Tab, Some(url)) => tabs::open(&url, related_position(), true),
        (HintTarget::TabBg, Some(url)) => tabs::open(&url, related_position(), false),
        (HintTarget::Current, Some(url)) => {
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        (HintTarget::Yank, Some(url)) => clipboard::yank(&url, "URL"),
        (HintTarget::Download, Some(url)) => crate::downloads::start(&url),
        (HintTarget::Download, None) => {
            shell::show_message(Level::Error, "That element has no URL")
        }
        (HintTarget::Yank, None) => shell::show_message(Level::Error, "That element has no URL"),
        (HintTarget::Hover, _) => mouse_at(&browser, index, false),
        // Elements without a URL (buttons, inputs) are clicked instead.
        _ => mouse_at(&browser, index, true),
    }
}

/// Move the mouse to the element and optionally click it. Real input events
/// give the page a user gesture, so popups, focus and frameworks behave.
fn mouse_at(browser: &Browser, index: usize, click: bool) {
    let target = browser.clone();
    eval::eval(
        browser,
        &format!("window.__rtHints.point({index})"),
        move |result| {
            let Some(point) = result
                .ok()
                .and_then(|json| serde_json::from_str::<Option<Point>>(&json).ok())
                .flatten()
            else {
                return shell::show_message(Level::Error, "The element is gone");
            };
            let Some(host) = target.host() else { return };
            let zoom = 1.2_f64.powf(host.zoom_level());
            let event = MouseEvent {
                x: (point.x * zoom).round() as i32,
                y: (point.y * zoom).round() as i32,
                modifiers: 0,
            };
            host.send_mouse_move_event(Some(&event), 0);
            if click {
                host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 0, 1);
                host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 1, 1);
            }
        },
    );
}
