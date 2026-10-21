//! Split panes (`:split`, `:vsplit`): several tabs on screen in one window.
//! The focused pane always shows the current tab, so everything that acts on
//! the current tab acts on the focused pane; each other pane remembers its
//! tab's view. `rt_core::split` keeps the layout.

use cef::*;
use rt_core::command::{OpenTarget, PaneTarget};
use rt_core::engine::Level;
use rt_core::split::Axis;
use rt_core::{Command, Effect};

use crate::shell::{self, WindowState};

/// Pixels between panes, where the page area's background shows.
const GAP: i32 = 2;

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::Split { vertical, url } => split(
            if *vertical {
                Axis::Vertical
            } else {
                Axis::Horizontal
            },
            url.clone(),
        ),
        Command::SplitClose => close_focused(),
        Command::SplitOnly => only(),
        Command::SplitFocus(target) => focus(*target),
        Command::SplitEqual => {
            shell::with(|s| s.layout.equalize());
            arrange_active();
        }
        _ => return false,
    }
    shell::refresh_ui();
    true
}

fn same(a: &BrowserView, b: &BrowserView) -> bool {
    View::from(a).is_same(Some(&mut View::from(b))) != 0
}

fn index_of(w: &WindowState, view: &BrowserView) -> Option<usize> {
    w.tabs.iter().position(|t| same(&t.view, view))
}

/// Split the focused pane; the new pane gets `url` (or the default page) in
/// a new tab, and the old one keeps its tab.
fn split(axis: Axis, url: Option<String>) {
    let split = shell::with(|s| {
        let view = s.tabs.current()?.view.clone();
        let old = s.layout.focused();
        s.layout.split(axis);
        s.pane_tabs.push((old, view));
        Some(())
    })
    .flatten();
    if split.is_none() {
        return;
    }
    shell::apply(vec![Effect::Run {
        command: Command::Open {
            target: OpenTarget::Tab,
            related: false,
            url,
        },
        count: None,
    }]);
}

/// Close the focused pane and focus its neighbour. Its tab stays open.
fn close_focused() {
    let next = shell::with(|s| {
        if !s.layout.is_split() {
            return Err("There's only one pane");
        }
        let focused = s.layout.focused();
        s.layout.close(focused);
        let next = s.layout.focused();
        let view = s
            .pane_tabs
            .iter()
            .position(|(p, _)| *p == next)
            .map(|i| s.pane_tabs.remove(i).1);
        Ok((s.id, view.and_then(|v| index_of(s, &v))))
    });
    match next {
        Some(Err(message)) => shell::show_message(Level::Error, message),
        Some(Ok((_, Some(index)))) => crate::tabs::select(index),
        Some(Ok((window, None))) => crate::tabs::show_current_in(window, true),
        None => {}
    }
}

/// Keep only the focused pane; the other panes' tabs stay open.
fn only() {
    if let Some(window) = shell::with(|s| {
        s.layout.only();
        s.pane_tabs.clear();
        s.id
    }) {
        crate::tabs::show_current_in(window, true);
    }
}

fn focus(target: PaneTarget) {
    let index = shell::with(|s| {
        let pane = match target {
            PaneTarget::Next => s.layout.next(),
            PaneTarget::Direction(direction) => s.layout.neighbour(direction)?,
        };
        let view = s.pane_tabs.iter().find(|(p, _)| *p == pane)?.1.clone();
        index_of(s, &view)
    })
    .flatten();
    if let Some(index) = index {
        crate::tabs::select(index);
    }
}

/// The tab at `index` is about to become current. If another pane shows
/// it, that pane takes the focus, and the focused pane keeps its tab.
pub fn before_switch(w: &mut WindowState, index: usize) {
    if !w.layout.is_split() || index == w.tabs.current_index() {
        return;
    }
    let Some(target) = w.tabs.get(index).map(|t| t.view.clone()) else {
        return;
    };
    let Some(i) = w.pane_tabs.iter().position(|(_, v)| same(v, &target)) else {
        return;
    };
    let (pane, _) = w.pane_tabs.remove(i);
    if let Some(current) = w.tabs.current().map(|t| t.view.clone()) {
        w.pane_tabs.push((w.layout.focused(), current));
    }
    w.layout.focus(pane);
}

/// After tabs closed or the current tab changed: every pane but the focused
/// one shows a tab of its own that isn't current. A pane whose tab went gets
/// one that isn't on screen; with none left, the pane closes.
pub fn reconcile(w: &mut WindowState) {
    if !w.layout.is_split() {
        w.pane_tabs.clear();
        return;
    }
    let focused = w.layout.focused();
    let views: Vec<BrowserView> = w.tabs.iter().map(|t| t.view.clone()).collect();
    let current = w.tabs.current().map(|t| t.view.clone());
    let mut kept: Vec<(u32, BrowserView)> = Vec::new();
    for pane in w.layout.panes().into_iter().filter(|&p| p != focused) {
        let on_screen = |v: &BrowserView, kept: &[(u32, BrowserView)]| {
            current.as_ref().is_some_and(|c| same(c, v)) || kept.iter().any(|(_, k)| same(k, v))
        };
        let own = w
            .pane_tabs
            .iter()
            .find(|(p, _)| *p == pane)
            .map(|(_, v)| v.clone())
            .filter(|v| views.iter().any(|t| same(t, v)) && !on_screen(v, &kept));
        match own.or_else(|| views.iter().find(|v| !on_screen(v, &kept)).cloned()) {
            Some(view) => kept.push((pane, view)),
            None => {
                w.layout.close(pane);
            }
        }
    }
    w.pane_tabs = kept;
}

/// Whether `view` is on screen in a pane other than the focused one.
pub fn in_other_pane(w: &WindowState, view: &BrowserView) -> bool {
    w.pane_tabs.iter().any(|(_, v)| same(v, view))
}

/// Place window `window_id`'s tab views: the current one (and the crash
/// notice) in the focused pane, the other panes' tabs in theirs, and every
/// hidden tab over the whole page area, ready to be shown.
pub fn arrange(window_id: u32) {
    let Some(Some((content, tabs, notice, panes, focused))) = shell::with(|s| {
        let w = s.windows.iter().find(|w| w.id == window_id)?;
        let size = w.content.as_ref().map(|c| View::from(c).bounds())?;
        let bounds = w.layout.bounds(size.width, size.height, GAP);
        let rect = |pane| {
            bounds.iter().find(|(p, _)| *p == pane).map(|(_, b)| Rect {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            })
        };
        let full = Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        };
        let focused = rect(w.layout.focused()).unwrap_or(full.clone());
        let current = w.tabs.current_index();
        let tabs: Vec<(BrowserView, Rect)> = w
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let place = if i == current {
                    focused.clone()
                } else {
                    w.pane_tabs
                        .iter()
                        .find(|(_, v)| same(v, &t.view))
                        .and_then(|(p, _)| rect(*p))
                        .unwrap_or_else(|| full.clone())
                };
                (t.view.clone(), place)
            })
            .collect();
        Some((
            w.content.clone()?,
            tabs,
            w.crash_notice.clone(),
            w.layout.is_split(),
            focused,
        ))
    }) else {
        return;
    };
    // The gaps between panes show the page area's background, and the
    // focused pane's edges a border behind it.
    let (gap, accent) = split_colors();
    View::from(&content).set_background_color(if panes { gap } else { 0 });
    if let Some(border) = border(window_id, &content) {
        let size = View::from(&content).bounds();
        let around = Rect {
            x: (focused.x - GAP).max(0),
            y: (focused.y - GAP).max(0),
            width: 0,
            height: 0,
        };
        let around = Rect {
            width: (focused.x + focused.width + GAP).min(size.width) - around.x,
            height: (focused.y + focused.height + GAP).min(size.height) - around.y,
            ..around
        };
        let border = View::from(&border);
        border.set_background_color(accent);
        border.set_bounds(Some(&around));
        border.set_visible(panes.into());
    }
    for (view, rect) in &tabs {
        View::from(view).set_bounds(Some(rect));
    }
    if let Some(notice) = notice {
        View::from(&notice).set_bounds(Some(&focused));
    }
}

/// The window's pane border, below every tab view, made on first use.
fn border(window_id: u32, content: &Panel) -> Option<Panel> {
    let existing = shell::with(|s| {
        s.windows
            .iter()
            .find(|w| w.id == window_id)
            .and_then(|w| w.pane_border.clone())
    })
    .flatten();
    if existing.is_some() {
        return existing;
    }
    let panel = panel_create(None)?;
    content.add_child_view_at(Some(&mut View::from(&panel)), 0);
    shell::with(|s| {
        if let Some(w) = s.windows.iter_mut().find(|w| w.id == window_id) {
            w.pane_border = Some(panel.clone());
        }
    });
    Some(panel)
}

fn arrange_active() {
    if let Some(window) = shell::with(|s| s.id) {
        arrange(window);
    }
}

/// The theme's colors for the gaps between panes and the focused pane's border.
fn split_colors() -> (u32, u32) {
    let color = |name: &str, fallback: u32| {
        shell::with(|s| {
            let colors = rt_core::theme::resolve(s.engine.settings());
            rt_core::theme::argb(colors.get(name)?)
        })
        .flatten()
        .unwrap_or(fallback)
    };
    (color("surface2", 0xff33_3333), color("accent", 0xff5f_87d7))
}

// The page area, which places its tab views itself (see `arrange`).
wrap_panel_delegate! {
    pub struct ContentDelegate {
        window: u32,
    }

    impl ViewDelegate {
        fn on_layout_changed(&self, _view: Option<&mut View>, _new_bounds: Option<&Rect>) {
            arrange(self.window);
        }
    }

    impl PanelDelegate {}
}

// A click in another pane focuses that pane.
wrap_focus_handler! {
    pub struct RtFocusHandler;

    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) {
            let Some(id) = browser.map(|b| b.identifier()) else { return };
            let mut task = FocusPane::new(id);
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}

wrap_task! {
    struct FocusPane {
        browser: i32,
    }

    impl Task {
        fn execute(&self) {
            shell::activate_browser(self.browser);
            let index = shell::with(|s| {
                let index = s
                    .tabs
                    .iter()
                    .position(|t| t.browser().is_some_and(|b| b.identifier() == self.browser))?;
                in_other_pane(s, &s.tabs.get(index)?.view).then_some(index)
            })
            .flatten();
            if let Some(index) = index {
                crate::tabs::select(index);
                shell::refresh_ui();
            }
        }
    }
}
