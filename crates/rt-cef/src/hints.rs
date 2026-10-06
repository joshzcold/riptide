//! Hint mode glue: asks the page for hintable elements, draws labels, and
//! performs the chosen action. Label logic lives in `rt_core::hints`.

use std::cell::RefCell;
use std::rc::Rc;

use cef::*;
use rt_core::engine::Level;
use rt_core::hints::{ChildFrame, FrameReport, HintItem, HintRequest, HintTarget, Placement};
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

/// What `__rtHints.collect` reports from one frame.
#[derive(Deserialize)]
struct Collected {
    root: bool,
    items: Vec<Item>,
    #[serde(default)]
    frames: Vec<CrossFrame>,
}

#[derive(Deserialize)]
struct CrossFrame {
    url: String,
    name: String,
    rect: Option<Point>,
}

/// A frame with hints: its items are `start..start + len` of the session's.
struct HintFrame {
    frame: Frame,
    /// Where its viewport is in the top page; `None` if that couldn't be worked out.
    offset: Option<(f64, f64)>,
    start: usize,
    len: usize,
}

thread_local! {
    /// The frames of the current hint session, in label order.
    static FRAMES: RefCell<Vec<HintFrame>> = const { RefCell::new(Vec::new()) };
}

/// Replies still to come from the frames of one `:hint`.
struct Gathering {
    browser: i32,
    request: Option<HintRequest>,
    pending: usize,
    replies: Vec<(Frame, Collected)>,
}

/// How long to wait for every frame before hinting with what has answered.
const COLLECT_TIMEOUT_MS: i64 = 1500;

/// `:hint` — collect elements in every frame of the current tab, then enter
/// hint mode. Cross-origin iframes collect their own; the rest are searched
/// from the frame above them.
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
    let frames = all_frames(&browser);
    let gathering = Rc::new(RefCell::new(Gathering {
        browser: browser.identifier(),
        request: Some(request),
        pending: frames.len(),
        replies: Vec::new(),
    }));
    for frame in frames {
        let state = gathering.clone();
        let target = frame.clone();
        eval::eval_frame(&frame, &code, move |result| {
            let reply = result
                .ok()
                .and_then(|json| serde_json::from_str::<Collected>(&json).ok());
            let finished = {
                let mut g = state.borrow_mut();
                if let Some(reply) = reply {
                    g.replies.push((target, reply));
                }
                g.pending = g.pending.saturating_sub(1);
                g.pending == 0
            };
            if finished {
                finish(&state);
            }
        });
    }
    // A frame that never answers (busy, or gone) doesn't hold the rest up.
    let mut task = FinishGathering::new(RefCell::new(Some(gathering)));
    post_delayed_task(ThreadId::UI, Some(&mut task), COLLECT_TIMEOUT_MS);
}

/// The browser's frames, the main frame first.
fn all_frames(browser: &Browser) -> Vec<Frame> {
    let mut ids = CefStringList::new();
    browser.frame_identifiers(Some(&mut ids));
    let mut frames: Vec<Frame> = ids
        .into_iter()
        .filter_map(|id| browser.frame_by_identifier(Some(&CefString::from(id.as_str()))))
        .filter(|f| f.is_valid() != 0)
        .collect();
    frames.sort_by_key(|f| f.is_main() == 0);
    frames
}

wrap_task! {
    struct FinishGathering {
        gathering: RefCell<Option<Rc<RefCell<Gathering>>>>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(state) = self.gathering.borrow_mut().take() {
                finish(&state);
            }
        }
    }
}

fn frame_id(frame: &Frame) -> String {
    CefString::from(&frame.identifier()).to_string()
}

/// Place the frames, drop hidden ones, and start hint mode with every
/// frame's items in order. Runs once, whichever comes first: the last
/// reply or the timeout.
fn finish(state: &Rc<RefCell<Gathering>>) {
    let (browser, request, replies) = {
        let mut g = state.borrow_mut();
        let Some(request) = g.request.take() else {
            return;
        };
        (g.browser, request, std::mem::take(&mut g.replies))
    };
    let mut replies: Vec<(Frame, Collected)> =
        replies.into_iter().filter(|(_, c)| c.root).collect();
    replies.sort_by_key(|(f, _)| f.is_main() == 0);
    let roots: Vec<String> = replies.iter().map(|(f, _)| frame_id(f)).collect();
    let reports: Vec<FrameReport> = replies
        .iter()
        .map(|(frame, collected)| {
            // The nearest frame above that collected for itself.
            let mut ancestor = None;
            let mut parent = if frame.is_main() != 0 {
                None
            } else {
                frame.parent()
            };
            while let Some(p) = parent {
                let id = frame_id(&p);
                if roots.contains(&id) {
                    ancestor = Some(id);
                    break;
                }
                parent = p.parent();
            }
            FrameReport {
                id: frame_id(frame),
                ancestor,
                name: CefString::from(&frame.name()).to_string(),
                url: CefString::from(&frame.url()).to_string(),
                children: collected
                    .frames
                    .iter()
                    .map(|c| ChildFrame {
                        url: c.url.clone(),
                        name: c.name.clone(),
                        at: c.rect.as_ref().map(|r| (r.x, r.y)),
                    })
                    .collect(),
            }
        })
        .collect();
    let placements = rt_core::hints::place_frames(&reports);
    let mut items = Vec::new();
    let mut frames = Vec::new();
    for ((frame, collected), placement) in replies.into_iter().zip(placements) {
        let offset = match placement {
            Placement::Hidden => continue,
            Placement::At(x, y) => Some((x, y)),
            Placement::Unknown => None,
        };
        if collected.items.is_empty() {
            continue;
        }
        frames.push(HintFrame {
            frame,
            offset,
            start: items.len(),
            len: collected.items.len(),
        });
        items.extend(collected.items.into_iter().map(|i| HintItem {
            url: i.url,
            text: i.text,
        }));
    }
    load_dictionary();
    let effects = shell::with(|s| {
        // Ignore stale replies if the user switched tabs meanwhile.
        if s.current_browser().map(|b| b.identifier()) != Some(browser) {
            return Vec::new();
        }
        s.hint_browser = Some(browser);
        FRAMES.with(|f| *f.borrow_mut() = frames);
        s.engine.start_hints(request, items)
    });
    shell::apply(effects.unwrap_or_default());
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

/// Run `__rtHints.method(arg)` in every frame of the hint session.
fn call(method: &str, arg: &str) {
    let frames: Vec<Frame> = FRAMES.with(|f| f.borrow().iter().map(|h| h.frame.clone()).collect());
    for frame in frames.iter().filter(|f| f.is_valid() != 0) {
        eval::eval_frame(frame, &format!("window.__rtHints.{method}({arg})"), |_| {});
    }
}

/// A frame, where it is in the top page (if known), and an index within it.
type FrameHint = (Frame, Option<(f64, f64)>, usize);

/// The frame showing hint `index`, its offset, and the index within it.
fn frame_of(index: usize) -> Option<FrameHint> {
    FRAMES.with(|f| {
        f.borrow()
            .iter()
            .find(|h| (h.start..h.start + h.len).contains(&index))
            .map(|h| (h.frame.clone(), h.offset, index - h.start))
    })
}

fn related_position() -> Position {
    shell::with(|s| s.new_tab_position(true)).unwrap_or(Position::Next)
}

/// Each frame draws its own share of the labels.
pub fn show(labels: &[String]) {
    let (upper, theme) = shell::with(|s| {
        let vars = rt_core::theme::ui_vars(s.engine.settings());
        let theme: serde_json::Map<String, serde_json::Value> = vars
            .into_iter()
            .filter(|(name, _)| name.starts_with("hints-") || name == "font-hints")
            .map(|(name, value)| (name, value.into()))
            .collect();
        (
            s.engine.settings().bool("hints.uppercase"),
            serde_json::Value::Object(theme),
        )
    })
    .unwrap_or((false, serde_json::Value::Null));
    let css = serde_json::to_string(&crate::userstyle::hints_css()).unwrap_or_default();
    let frames: Vec<(Frame, usize, usize)> = FRAMES.with(|f| {
        f.borrow()
            .iter()
            .map(|h| (h.frame.clone(), h.start, h.len))
            .collect()
    });
    for (frame, start, len) in frames {
        let Some(slice) = labels.get(start..start + len) else {
            continue;
        };
        let slice = serde_json::to_string(slice).unwrap_or_default();
        eval::eval_frame(
            &frame,
            &format!("window.__rtHints.show({slice}, {upper}, {theme}, {css})"),
            |_| {},
        );
    }
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
/// Inside a frame whose place on screen isn't known, the element is clicked
/// by script instead.
fn mouse_at(browser: &Browser, index: usize, click: bool) {
    let Some((frame, offset, local)) = frame_of(index) else {
        return shell::show_message(Level::Error, "The element is gone");
    };
    let Some((dx, dy)) = offset else {
        if click {
            eval::eval_frame(
                &frame,
                &format!("window.__rtHints.activate({local})"),
                |result| {
                    if result.as_deref() == Ok("gone") {
                        shell::show_message(Level::Error, "The element is gone");
                    }
                },
            );
        }
        return;
    };
    let target = browser.clone();
    eval::eval_frame(
        &frame,
        &format!("window.__rtHints.point({local})"),
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
                x: ((point.x + dx) * zoom).round() as i32,
                y: ((point.y + dy) * zoom).round() as i32,
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
