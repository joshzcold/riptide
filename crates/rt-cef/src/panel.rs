//! `rt.ui.panel`: lines a plugin or `config.lua` keeps beside or below the
//! page, such as a tab tree or notes. Each panel is a view in the window's
//! layout showing `riptide://ui/panel.html`, one per side of a window.

use std::cell::{Cell, RefCell};

use cef::*;
use serde_json::json;

use rt_config::lua::{self, PanelSpec};
use rt_core::key::{Key, KeyCode};

use crate::client::Role;
use crate::shell;

pub const URL: &str = "riptide://ui/panel.html";

struct Docked {
    id: u32,
    window: u32,
    source: String,
    spec: PanelSpec,
    keys: Vec<Key>,
    view: BrowserView,
    /// The line its cursor is on, from 1; 0 with no lines.
    cursor: u32,
}

thread_local! {
    static PANELS: RefCell<Vec<Docked>> = const { RefCell::new(Vec::new()) };
    /// The panel whose keys work, until Escape.
    static FOCUSED: Cell<Option<u32>> = const { Cell::new(None) };
}

fn origin(source: &str) -> &str {
    if source.is_empty() {
        "config.lua"
    } else {
        source
    }
}

fn parse_keys(spec: &PanelSpec) -> Vec<Key> {
    spec.keys
        .iter()
        .filter_map(|k| Key::parse_sequence(k).ok()?.first().copied())
        .collect()
}

fn clamp_cursor(cursor: u32, lines: usize) -> u32 {
    let lines = u32::try_from(lines).unwrap_or(u32::MAX);
    cursor.clamp(lines.min(1), lines)
}

/// Show panel `id`, or redraw it with `spec`. A new panel on a side that
/// already has one replaces it.
pub fn show(id: u32, source: String, spec: PanelSpec) {
    prune();
    let moved = PANELS.with(|p| {
        let mut all = p.borrow_mut();
        let panel = all.iter_mut().find(|p| p.id == id)?;
        let moved = panel.spec.side != spec.side || panel.spec.size != spec.size;
        panel.keys = parse_keys(&spec);
        panel.cursor = clamp_cursor(panel.cursor, spec.lines.len());
        panel.spec = spec.clone();
        Some((moved, panel.view.clone(), panel.window))
    });
    if let Some((moved, view, window)) = moved {
        if moved {
            detach(&view);
            attach(&view, window, &spec.side);
        }
        return render(id);
    }
    let Some(window) = shell::with(|s| s.id) else {
        return;
    };
    let taken: Vec<u32> = PANELS.with(|p| {
        p.borrow()
            .iter()
            .filter(|p| p.window == window && p.spec.side == spec.side)
            .map(|p| p.id)
            .collect()
    });
    for old in taken {
        close(old, false);
    }
    let Some(view) = crate::window::create_browser_view(Role::Panel, URL) else {
        return;
    };
    View::from(&view).set_focusable(0);
    let side = spec.side.clone();
    PANELS.with(|p| {
        p.borrow_mut().push(Docked {
            id,
            window,
            source,
            keys: parse_keys(&spec),
            cursor: clamp_cursor(1, spec.lines.len()),
            spec,
            view: view.clone(),
        })
    });
    attach(&view, window, &side);
}

/// Put `view` beside the page area of `window`, or below it.
fn attach(view: &BrowserView, window: u32, side: &str) {
    let Some((top, row, content)) = shell::with(|s| {
        let w = s.windows.iter().find(|w| w.id == window)?;
        Some((w.window.clone()?, w.row.clone()?, w.content.clone()?))
    })
    .flatten() else {
        return;
    };
    let index_of = |count: usize, at: &dyn Fn(i32) -> Option<View>, target: &mut View| {
        (0..count as i32).find(|&i| at(i).is_some_and(|v| v.is_same(Some(target)) != 0))
    };
    let mut panel = View::from(view);
    match side {
        "bottom" => {
            let mut row_view = View::from(&row);
            let at = index_of(
                top.child_view_count(),
                &|i| top.child_view_at(i),
                &mut row_view,
            );
            top.add_child_view_at(Some(&mut panel), at.map_or(0, |i| i + 1));
        }
        side => {
            let mut content_view = View::from(&content);
            let at = index_of(
                row.child_view_count(),
                &|i| row.child_view_at(i),
                &mut content_view,
            )
            .unwrap_or(0);
            let at = if side == "right" { at + 1 } else { at };
            row.add_child_view_at(Some(&mut panel), at);
        }
    }
    top.layout();
}

fn detach(view: &BrowserView) {
    let mut panel = View::from(view);
    if let Some(parent) = panel.parent_view().and_then(|p| p.as_panel()) {
        parent.remove_child_view(Some(&mut panel));
        let window = parent.window();
        if let Some(window) = window {
            window.layout();
        }
    }
}

/// The size a panel's view asks the layout for: its `size` across, stretched along.
pub fn size_for(view: &mut View) -> Option<Size> {
    PANELS.with(|p| {
        p.borrow().iter().find_map(|panel| {
            if View::from(&panel.view).is_same(Some(view)) == 0 {
                return None;
            }
            let size = i32::try_from(panel.spec.size).unwrap_or(300);
            Some(if panel.spec.side == "bottom" {
                Size {
                    width: 1,
                    height: size,
                }
            } else {
                Size {
                    width: size,
                    height: 1,
                }
            })
        })
    })
}

/// Close panel `id`. `by_lua` when its own `close` asked, so Lua already knows.
pub fn close(id: u32, by_lua: bool) {
    let Some(panel) = PANELS.with(|p| {
        let mut all = p.borrow_mut();
        let at = all.iter().position(|p| p.id == id)?;
        Some(all.remove(at))
    }) else {
        return;
    };
    if FOCUSED.get() == Some(id) {
        FOCUSED.set(None);
    }
    detach(&panel.view);
    if !by_lua {
        crate::lua::carry_out_for(
            origin(&panel.source),
            lua::panel_closed(id, &crate::lua::current_context()),
        );
    }
}

/// The config was loaded again: its panels' functions are gone.
pub fn close_all() {
    let ids: Vec<u32> = PANELS.with(|p| p.borrow().iter().map(|p| p.id).collect());
    for id in ids {
        close(id, true);
    }
}

/// Forget the panels of windows that have closed.
fn prune() {
    let open: Vec<u32> =
        shell::with(|s| s.windows.iter().map(|w| w.id).collect()).unwrap_or_default();
    let gone: Vec<u32> = PANELS.with(|p| {
        p.borrow()
            .iter()
            .filter(|p| !open.contains(&p.window))
            .map(|p| p.id)
            .collect()
    });
    for id in gone {
        close(id, false);
    }
}

/// `:panel-focus`: the next panel in the focused window, then none.
pub fn focus_next() {
    prune();
    let Some(window) = shell::with(|s| s.id) else {
        return;
    };
    let ids: Vec<u32> = PANELS.with(|p| {
        p.borrow()
            .iter()
            .filter(|p| p.window == window)
            .map(|p| p.id)
            .collect()
    });
    let next = match FOCUSED.get().and_then(|f| ids.iter().position(|&i| i == f)) {
        Some(at) => ids.get(at + 1).copied(),
        None => ids.first().copied(),
    };
    if next.is_none() && ids.is_empty() {
        shell::show_message(rt_core::engine::Level::Info, "No panel is open");
    }
    set_focus(next);
}

/// A panel's `focus()`, or a click in it.
pub fn focus(id: u32) {
    set_focus(Some(id));
}

fn set_focus(id: Option<u32>) {
    let before = FOCUSED.replace(id);
    for changed in [before, id].into_iter().flatten() {
        render(changed);
    }
}

/// A click on line `line` of panel `id`: focus it there.
pub fn click(id: u32, line: u32) {
    let known = PANELS.with(|p| {
        let mut all = p.borrow_mut();
        let panel = all.iter_mut().find(|p| p.id == id)?;
        panel.cursor = clamp_cursor(line, panel.spec.lines.len());
        Some(())
    });
    if known.is_some() {
        set_focus(Some(id));
        render(id);
    }
}

/// A panel's page finished loading: draw it.
pub fn ready(browser: i32) {
    if let Some(id) = PANELS.with(|p| {
        p.borrow()
            .iter()
            .find(|p| p.view.browser().is_some_and(|b| b.identifier() == browser))
            .map(|p| p.id)
    }) {
        render(id);
    }
}

fn render(id: u32) {
    let Some((frame, source, spec, cursor)) = PANELS.with(|p| {
        let all = p.borrow();
        let panel = all.iter().find(|p| p.id == id)?;
        let frame = panel.view.browser()?.main_frame()?;
        Some((
            frame,
            panel.source.clone(),
            panel.spec.clone(),
            panel.cursor,
        ))
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
        "cursor": cursor,
        "focused": FOCUSED.get() == Some(id),
        "keys": !spec.keys.is_empty(),
    });
    shell::exec_js(&frame, &format!("rtPanel({state})"));
}

/// In normal mode with nothing typed, the focused panel in the focused
/// window gets keys: `j`/`k` (and the arrows) move its cursor, its own keys
/// call its functions, and Escape goes back to the page.
pub fn forward_key(key: &Key) -> bool {
    let Some(id) = FOCUSED.get() else {
        return false;
    };
    let Some(window) = shell::with(|s| {
        (s.engine.mode() == rt_core::Mode::Normal && s.engine.status().keystring.is_empty())
            .then_some(s.id)
    })
    .flatten() else {
        return false;
    };
    enum Then {
        Unfocus,
        Moved,
        Call(String, String, u32),
    }
    let then = PANELS.with(|p| {
        let mut all = p.borrow_mut();
        let panel = all.iter_mut().find(|p| p.id == id && p.window == window)?;
        let lines = panel.spec.lines.len();
        if let Some(at) = panel.keys.iter().position(|k| k == key) {
            return Some(Then::Call(
                panel.source.clone(),
                panel.spec.keys[at].clone(),
                panel.cursor,
            ));
        }
        if !key.mods.is_empty() {
            return None;
        }
        match key.code {
            KeyCode::Escape => Some(Then::Unfocus),
            KeyCode::Char('j') | KeyCode::Down => {
                panel.cursor = clamp_cursor(panel.cursor + 1, lines);
                Some(Then::Moved)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                panel.cursor = clamp_cursor(panel.cursor.saturating_sub(1), lines);
                Some(Then::Moved)
            }
            _ => None,
        }
    });
    match then {
        None => false,
        Some(Then::Unfocus) => {
            set_focus(None);
            true
        }
        Some(Then::Moved) => {
            render(id);
            true
        }
        Some(Then::Call(source, name, line)) => {
            let context = crate::lua::current_context();
            crate::lua::carry_out_for(origin(&source), lua::panel_key(id, &name, line, &context));
            true
        }
    }
}

/// The panels open, for the e2e tests.
pub fn test_state() -> serde_json::Value {
    PANELS.with(|p| {
        p.borrow()
            .iter()
            .map(|p| {
                let bounds = View::from(&p.view).bounds();
                json!({
                    "id": p.id,
                    "source": p.source,
                    "side": p.spec.side,
                    "title": p.spec.title,
                    "lines": p.spec.lines.len(),
                    "cursor": p.cursor,
                    "focused": FOCUSED.get() == Some(p.id),
                    "width": bounds.width,
                    "height": bounds.height,
                })
            })
            .collect()
    })
}
