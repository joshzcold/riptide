//! `rt.ui.float`: boxes of text that `config.lua` and plugins draw over the
//! page. Each float is its own overlay view showing `riptide://ui/float.html`,
//! so it can sit beside a prompt or the completion list.

use std::cell::RefCell;

use cef::*;
use serde_json::json;

use rt_config::lua::{self, FloatSpec};
use rt_core::engine::Level;
use rt_core::key::Key;

use crate::client::Role;
use crate::shell;

pub const URL: &str = "riptide://ui/float.html";

/// Floats open at once, across windows; each is a browser view.
const MAX_FLOATS: usize = 8;

/// Space between a float and the page area's edges.
const MARGIN: i32 = 16;

struct Float {
    id: u32,
    /// The window it's drawn in.
    window: u32,
    source: String,
    spec: FloatSpec,
    keys: Vec<Key>,
    view: BrowserView,
    overlay: Option<OverlayController>,
    /// What its page measured, once it has drawn.
    size: Option<(i32, i32)>,
    /// Bumped by each update, so an older timeout doesn't close it.
    generation: u32,
}

thread_local! {
    static FLOATS: RefCell<Vec<Float>> = const { RefCell::new(Vec::new()) };
}

/// Show float `id`, or redraw it with `spec`.
pub fn show(id: u32, source: String, spec: FloatSpec) {
    prune();
    let keys: Vec<Key> = spec
        .keys
        .iter()
        .filter_map(|k| Key::parse_sequence(k).ok()?.first().copied())
        .collect();
    let existing = FLOATS.with(|f| {
        let mut all = f.borrow_mut();
        let float = all.iter_mut().find(|f| f.id == id)?;
        float.spec = spec.clone();
        float.keys = keys.clone();
        float.generation += 1;
        Some(float.generation)
    });
    if let Some(generation) = existing {
        arm_timeout(id, generation, spec.timeout);
        return render(id);
    }
    if FLOATS.with(|f| f.borrow().len()) >= MAX_FLOATS {
        return shell::show_message(
            Level::Error,
            format!("{}: too many floats open", label(&source)),
        );
    }
    let Some((window_id, window)) = shell::with(|s| Some((s.id, s.window.clone()?))).flatten()
    else {
        return;
    };
    let Some(view) = crate::window::create_browser_view(Role::Float, URL) else {
        return;
    };
    View::from(&view).set_focusable(0);
    let mut as_view = View::from(&view);
    let overlay = window.add_overlay_view(Some(&mut as_view), DockingMode::CUSTOM, 0);
    if let Some(overlay) = &overlay {
        // Shown once its page has measured itself.
        overlay.set_visible(0);
    }
    let timeout = spec.timeout;
    FLOATS.with(|f| {
        f.borrow_mut().push(Float {
            id,
            window: window_id,
            source,
            spec,
            keys,
            view,
            overlay,
            size: None,
            generation: 0,
        })
    });
    arm_timeout(id, 0, timeout);
}

/// What errors from `source`'s Lua are reported as.
fn origin(source: &str) -> &str {
    if source.is_empty() {
        "config.lua"
    } else {
        source
    }
}

fn label(source: &str) -> String {
    if source.is_empty() {
        "config.lua".into()
    } else {
        format!("Plugin {source}")
    }
}

/// Close float `id`. `by_lua` when its own `close` asked, so Lua already knows.
pub fn close(id: u32, by_lua: bool) {
    let Some(float) = FLOATS.with(|f| {
        let mut all = f.borrow_mut();
        let at = all.iter().position(|f| f.id == id)?;
        Some(all.remove(at))
    }) else {
        return;
    };
    if let Some(overlay) = float.overlay.as_ref().filter(|o| o.is_valid() != 0) {
        overlay.destroy();
    }
    if !by_lua {
        crate::lua::carry_out_for(
            origin(&float.source),
            lua::float_closed(id, &crate::lua::current_context()),
        );
    }
}

/// The floats open, for the e2e tests: id, source, title, text, and whether it's placed.
pub fn test_state() -> serde_json::Value {
    FLOATS.with(|f| {
        f.borrow()
            .iter()
            .map(|f| {
                let text: Vec<String> = f
                    .spec
                    .lines
                    .iter()
                    .map(|line| line.iter().map(|(t, _)| t.as_str()).collect())
                    .collect();
                json!({
                    "id": f.id,
                    "source": f.source,
                    "title": f.spec.title,
                    "text": text.join("\n"),
                    "placed": f.size.is_some(),
                })
            })
            .collect()
    })
}

/// Forget the floats of windows that have closed (their views went with them).
fn prune() {
    let open: Vec<u32> =
        shell::with(|s| s.windows.iter().map(|w| w.id).collect()).unwrap_or_default();
    let gone: Vec<u32> = FLOATS.with(|f| {
        f.borrow()
            .iter()
            .filter(|f| !open.contains(&f.window))
            .map(|f| f.id)
            .collect()
    });
    for id in gone {
        close(id, false);
    }
}

/// Window `window` is closing: close its floats, without their `on_close`.
pub fn close_window(window: u32) {
    let ids: Vec<u32> = FLOATS.with(|f| {
        f.borrow()
            .iter()
            .filter(|f| f.window == window)
            .map(|f| f.id)
            .collect()
    });
    for id in ids {
        close(id, true);
    }
}

/// The config was loaded again: its floats' functions are gone.
pub fn close_all() {
    let ids: Vec<u32> = FLOATS.with(|f| f.borrow().iter().map(|f| f.id).collect());
    for id in ids {
        close(id, true);
    }
}

/// A float's page finished loading: draw it.
pub fn ready(browser: i32) {
    if let Some(id) = FLOATS.with(|f| {
        f.borrow()
            .iter()
            .find(|f| f.view.browser().is_some_and(|b| b.identifier() == browser))
            .map(|f| f.id)
    }) {
        render(id);
    }
}

fn render(id: u32) {
    let Some((frame, source, spec)) = FLOATS.with(|f| {
        let all = f.borrow();
        let float = all.iter().find(|f| f.id == id)?;
        let frame = float.view.browser()?.main_frame()?;
        Some((frame, float.source.clone(), float.spec.clone()))
    }) else {
        return;
    };
    let Some((theme, css)) = shell::with(|s| {
        (
            json!(rt_core::theme::ui_vars_previewing(
                s.engine.settings(),
                None
            )),
            crate::userstyle::ui_css(&s.paths.config_dir),
        )
    }) else {
        return;
    };
    let state = json!({
        "id": id,
        "theme": theme,
        "css": css,
        "source": source,
        "title": spec.title,
        "lines": spec.lines,
        "width": spec.width,
        "keys": !spec.keys.is_empty(),
    });
    shell::exec_js(&frame, &format!("rtFloat({state})"));
}

/// Float `id`'s page measured its content.
pub fn set_size(id: u32, width: u32, height: u32) {
    let changed = FLOATS.with(|f| {
        let mut all = f.borrow_mut();
        let float = all.iter_mut().find(|f| f.id == id)?;
        let size = Some((width as i32, height as i32));
        (float.size != size).then(|| float.size = size)
    });
    if changed.is_some() {
        reposition();
    }
}

/// Place every float in its window's page area, e.g. after a resize.
pub fn reposition() {
    let placed: Vec<(OverlayController, Rect)> = FLOATS.with(|f| {
        f.borrow()
            .iter()
            .filter_map(|float| {
                let (width, height) = float.size?;
                let area = shell::with(|s| {
                    let window = s.windows.iter().find(|w| w.id == float.window)?;
                    Some(View::from(window.row.as_ref()?).bounds())
                })
                .flatten()?;
                Some((
                    float.overlay.clone()?,
                    bounds(&float.spec.position, area, width, height),
                ))
            })
            .collect()
    });
    for (overlay, rect) in placed {
        overlay.set_bounds(Some(&rect));
        overlay.set_visible(1);
    }
}

fn bounds(position: &str, area: Rect, width: i32, height: i32) -> Rect {
    let width = width.min(area.width - 2 * MARGIN).max(1);
    let height = height.min(area.height - 2 * MARGIN).max(1);
    let centre = area.x + (area.width - width) / 2;
    let right = area.x + area.width - width - MARGIN;
    let (x, y) = match position {
        "top" => (centre, area.y + MARGIN),
        "bottom" => (centre, area.y + area.height - height - MARGIN),
        "top-right" => (right, area.y + MARGIN),
        "bottom-right" => (right, area.y + area.height - height - MARGIN),
        _ => (centre, area.y + (area.height - height) / 3),
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// In normal mode with nothing typed, the newest float with keys in the
/// focused window gets its keys, and Escape closes it. True if it took `key`.
pub fn forward_key(key: &Key) -> bool {
    let Some(window) = shell::with(|s| {
        (s.engine.mode() == rt_core::Mode::Normal && s.engine.status().keystring.is_empty())
            .then_some(s.id)
    })
    .flatten() else {
        return false;
    };
    let target = FLOATS.with(|f| {
        let all = f.borrow();
        let float = all
            .iter()
            .rev()
            .find(|f| f.window == window && !f.keys.is_empty())?;
        if key.code == rt_core::key::KeyCode::Escape && key.mods.is_empty() {
            return Some((float.id, float.source.clone(), None));
        }
        let at = float.keys.iter().position(|k| k == key)?;
        Some((
            float.id,
            float.source.clone(),
            Some(float.spec.keys[at].clone()),
        ))
    });
    match target {
        None => false,
        Some((id, _, None)) => {
            close(id, false);
            true
        }
        Some((id, source, Some(name))) => {
            let context = crate::lua::current_context();
            crate::lua::carry_out_for(origin(&source), lua::float_key(id, &name, &context));
            true
        }
    }
}

fn arm_timeout(id: u32, generation: u32, timeout: Option<u32>) {
    if let Some(ms) = timeout.filter(|ms| *ms > 0) {
        let mut task = FloatTimeout::new(id, generation);
        post_delayed_task(ThreadId::UI, Some(&mut task), i64::from(ms));
    }
}

wrap_task! {
    struct FloatTimeout {
        id: u32,
        generation: u32,
    }

    impl Task {
        fn execute(&self) {
            let current = FLOATS.with(|f| {
                f.borrow().iter().any(|f| f.id == self.id && f.generation == self.generation)
            });
            if current {
                close(self.id, false);
            }
        }
    }
}
