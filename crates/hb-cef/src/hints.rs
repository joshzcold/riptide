//! Hint mode glue: asks the page for hintable elements, draws labels, and
//! performs the chosen action. Label logic lives in `hb_core::hints`.

use cef::*;
use hb_core::engine::Level;
use hb_core::hints::{HintItem, HintRequest, HintTarget};
use hb_core::tabs::Position;
use serde::Deserialize;

use crate::{clipboard, eval, shell, tabs};

const HINTS_JS: &str = include_str!("../js/hints.js");

#[derive(Deserialize)]
struct Item {
    url: Option<String>,
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
    let group = serde_json::to_string(request.group.name()).unwrap_or_default();
    let code = format!("{HINTS_JS}; window.__hbHints.collect({group})");
    let id = browser.identifier();
    eval::eval(&browser, &code, move |result| {
        let items: Vec<Item> = match result.map(|json| serde_json::from_str(&json)) {
            Ok(Ok(items)) => items,
            Ok(Err(e)) => return shell::show_message(Level::Error, format!("Hints failed: {e}")),
            Err(e) => return shell::show_message(Level::Error, format!("Hints failed: {e}")),
        };
        let items = items.into_iter().map(|i| HintItem { url: i.url }).collect();
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
            &format!("window.__hbHints.{method}({arg})"),
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
    call("filter", &serde_json::to_string(typed).unwrap_or_default());
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
        &format!("window.__hbHints.point({index})"),
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
