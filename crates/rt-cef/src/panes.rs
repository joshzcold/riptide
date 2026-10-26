//! Split panes (`:split`, `:vsplit`): several tabs on screen in one window.
//! The focused pane always shows the current tab, so everything that acts on
//! the current tab acts on the focused pane; each other pane remembers its
//! tab's view. `rt_core::split` keeps the layout.

use cef::*;
use rt_core::command::{OpenTarget, PaneTarget};
use rt_core::engine::Level;
use rt_core::split::{Axis, Layout, Tree};
use rt_core::{Command, Effect};
use rt_storage::SplitState;

use crate::shell::{self, WindowState};

/// Pixels between panes, where the page area's background shows and the
/// borders can be dragged.
const GAP: i32 = 4;

const HANDLE_URL: &str = "riptide://ui/split.html";

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::Split {
            vertical,
            url,
            move_tab,
        } => split(
            if *vertical {
                Axis::Vertical
            } else {
                Axis::Horizontal
            },
            url.clone(),
            *move_tab,
        ),
        Command::SplitClose => close_focused(),
        Command::SplitOnly => only(),
        Command::SplitFocus(target) => focus(*target),
        Command::SplitEqual => {
            shell::with(|s| s.layout.equalize());
            arrange_active();
        }
        Command::SplitResize { width, percent } => {
            let axis = if *width {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            let resized = shell::with(|s| s.layout.resize(axis, f64::from(*percent) / 100.0));
            if resized == Some(false) {
                let way = if *width {
                    "side by side"
                } else {
                    "one above the other"
                };
                shell::show_message(
                    Level::Error,
                    format!("The focused pane has no neighbour {way}"),
                );
            }
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
/// a new tab, and the old one keeps its tab. With `move_tab` the current tab
/// goes to the new pane instead, and the old one shows the tab used before
/// it, or `url` in a new tab when given or when every other tab is on screen.
fn split(axis: Axis, url: Option<String>, move_tab: bool) {
    let split = shell::with(|s| {
        let view = s.tabs.current()?.view.clone();
        let earlier = (move_tab && url.is_none())
            .then(|| {
                s.tabs
                    .recent()
                    .filter_map(|i| s.tabs.get(i))
                    .find(|t| !in_other_pane(s, &t.view))
                    .map(|t| t.view.clone())
            })
            .flatten();
        let old = s.layout.focused();
        s.layout.split(axis);
        s.pane_tabs
            .push((old, earlier.clone().unwrap_or_else(|| view.clone())));
        Some((s.id, old, view, earlier.is_some()))
    })
    .flatten();
    let Some((window, old, moved, placed)) = split else {
        return;
    };
    if placed {
        crate::tabs::show_current_in(window, true);
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
    if move_tab {
        // The new tab takes the old pane, and the moved one is current in the new pane.
        let index = shell::with(|s| {
            let opened = s.tabs.current()?.view.clone();
            let pane = s.pane_tabs.iter_mut().find(|(p, _)| *p == old)?;
            pane.1 = opened;
            index_of(s, &moved)
        })
        .flatten();
        if let Some(index) = index {
            crate::tabs::select(index);
        }
    }
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
    arrange_in(window_id, None);
}

thread_local! {
    /// Placements put off because the page area had no size yet.
    static WAITING: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// [`arrange`] in a page area of `size`, or the size it has now. One that
/// has no size yet is tried again shortly, so the first tab isn't left 0×0.
fn arrange_in(window_id: u32, size: Option<Rect>) {
    let size = size.or_else(|| {
        shell::with(|s| {
            let w = s.windows.iter().find(|w| w.id == window_id)?;
            w.content.as_ref().map(|c| View::from(c).bounds())
        })
        .flatten()
    });
    let Some(size) = size else { return };
    if size.width <= 0 || size.height <= 0 {
        if WAITING.get() < 200 {
            WAITING.set(WAITING.get() + 1);
            crate::client::later(50, move || arrange(window_id));
        }
        return;
    }
    WAITING.set(0);
    let Some(Some((content, tabs, notice, panes, focused, pane_notices, current_index))) =
        shell::with(|s| {
            let w = s.windows.iter().find(|w| w.id == window_id)?;
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
            let pane_notices: Vec<(BrowserView, Rect)> = w
                .pane_notices
                .iter()
                .filter_map(|(tab, notice)| {
                    let pane = w.pane_tabs.iter().find(|(_, v)| same(v, tab))?.0;
                    Some((notice.clone(), rect(pane)?))
                })
                .collect();
            Some((
                w.content.clone()?,
                tabs,
                w.crash_notice.clone(),
                w.layout.is_split(),
                focused,
                pane_notices,
                current,
            ))
        })
    else {
        return;
    };
    // The gaps between panes show the page area's background, and the
    // focused pane's edges a border behind it.
    let (gap, accent) = split_colors();
    View::from(&content).set_background_color(if panes { gap } else { 0 });
    if let Some(border) = panes.then(|| border(window_id, &content)).flatten() {
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
        border.set_visible(1);
    } else if let Some(Some(border)) = shell::with(|s| {
        s.windows
            .iter()
            .find(|w| w.id == window_id)
            .and_then(|w| w.pane_border.clone())
    }) {
        View::from(&border).set_visible(0);
    }
    tracing::debug!(
        window = window_id,
        width = size.width,
        height = size.height,
        split = panes,
        "placing tabs"
    );
    for (i, (tab, rect)) in tabs.iter().enumerate() {
        let view = View::from(tab);
        let was_empty = view.bounds().width <= 0 || view.bounds().height <= 0;
        view.set_bounds(Some(rect));
        // A view can't take keyboard focus while it has no size, so the
        // current tab asks again once it has one.
        if was_empty && i == current_index && rect.width > 0 && rect.height > 0 {
            focus_current(window_id, tab);
        }
    }
    if let Some(notice) = notice {
        View::from(&notice).set_bounds(Some(&focused));
    }
    for (notice, rect) in &pane_notices {
        View::from(notice).set_bounds(Some(rect));
    }
    place_handles(window_id, &content, &size, &focused, gap, accent);
}

/// Put a handle over each border between panes, made or removed as the
/// borders come and go, and above the tab views so it gets the mouse.
fn place_handles(
    window_id: u32,
    content: &Panel,
    size: &Rect,
    focused: &Rect,
    gap: u32,
    accent: u32,
) {
    let Some(Some((borders, mut handles))) = shell::with(|s| {
        let w = s.windows.iter_mut().find(|w| w.id == window_id)?;
        let borders = if w.layout.is_split() {
            w.layout.borders(size.width, size.height, GAP)
        } else {
            Vec::new()
        };
        Some((borders, std::mem::take(&mut w.pane_handles)))
    }) else {
        return;
    };
    let hex = |argb: u32| format!("{:06x}", argb & 0x00ff_ffff);
    let mut kept = Vec::new();
    for border in &borders {
        let b = border.bounds;
        let rect = Rect {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
        };
        // A border along the focused pane's edge shows the focus color.
        let touches = rect.x < focused.x + focused.width + GAP
            && focused.x < rect.x + rect.width + GAP
            && rect.y < focused.y + focused.height + GAP
            && focused.y < rect.y + rect.height + GAP;
        let color = if touches { accent } else { gap };
        let axis = if border.axis == Axis::Vertical {
            "v"
        } else {
            "h"
        };
        let view = match handles.iter().position(|(id, _)| *id == border.id) {
            Some(i) => {
                let (_, view) = handles.remove(i);
                if let Some(frame) = view.browser().and_then(|b| b.main_frame()) {
                    shell::exec_js(
                        &frame,
                        &format!("window.rtColor && rtColor('#{}')", hex(color)),
                    );
                }
                view
            }
            None => {
                let url = format!("{HANDLE_URL}#{axis}/{}/{}", border.id, hex(color));
                let Some(view) =
                    crate::window::create_browser_view(crate::client::Role::SplitHandle, &url)
                else {
                    continue;
                };
                View::from(&view).set_focusable(0);
                content.add_child_view(Some(&mut View::from(&view)));
                view
            }
        };
        View::from(&view).set_bounds(Some(&rect));
        View::from(&view).set_visible(1);
        content.reorder_child_view(Some(&mut View::from(&view)), -1);
        kept.push((border.id.clone(), view));
    }
    for (_, gone) in handles {
        content.remove_child_view(Some(&mut View::from(&gone)));
    }
    shell::with(|s| {
        if let Some(w) = s.windows.iter_mut().find(|w| w.id == window_id) {
            w.pane_handles = kept;
        }
    });
}

/// A border's handle was dragged `delta` pixels right or down.
pub fn drag(border: &str, delta: i32) {
    let moved = shell::with(|s| {
        let size = s.content.as_ref().map(|c| View::from(c).bounds())?;
        let extent = s
            .layout
            .borders(size.width, size.height, GAP)
            .into_iter()
            .find(|b| b.id == border)?
            .extent;
        s.layout.move_border(border, delta, extent).then_some(())
    })
    .flatten();
    if moved.is_some() {
        arrange_active();
    }
}

/// The drag ended: the keys go back to the focused pane's page.
pub fn drop_border() {
    if let Some(Some(view)) = shell::with(|s| s.tabs.current().map(|t| t.view.clone())) {
        crate::panes::focus_view(&view);
    }
}

/// Cover each other pane's crashed tab with a crash notice of its own, as
/// the focused pane's is (`tabs::show_current_in`), and drop notices whose
/// tab is back or off screen.
pub fn show_crashes(window_id: u32, content: &Panel) {
    let Some(Some((crashed, mut notices))) = shell::with(|s| {
        let w = s.windows.iter_mut().find(|w| w.id == window_id)?;
        let crashed: Vec<(BrowserView, String)> = w
            .tabs
            .iter()
            .filter(|t| in_other_pane(w, &t.view))
            .filter_map(|t| {
                Some((
                    t.view.clone(),
                    crate::ui::crashed_url(&t.url, t.crashed.as_deref()?),
                ))
            })
            .collect();
        Some((crashed, std::mem::take(&mut w.pane_notices)))
    }) else {
        return;
    };
    let mut kept = Vec::new();
    for (tab, url) in crashed {
        View::from(&tab).set_visible(0);
        let notice = match notices.iter().position(|(t, _)| same(t, &tab)) {
            Some(i) => {
                let (_, notice) = notices.remove(i);
                if let Some(frame) = notice.browser().and_then(|b| b.main_frame()) {
                    frame.load_url(Some(&CefString::from(url.as_str())));
                }
                notice
            }
            None => {
                let Some(notice) =
                    crate::window::create_browser_view(crate::client::Role::Crashed, &url)
                else {
                    continue;
                };
                content.add_child_view(Some(&mut View::from(&notice)));
                notice
            }
        };
        View::from(&notice).set_visible(1);
        kept.push((tab, notice));
    }
    for (_, gone) in notices {
        content.remove_child_view(Some(&mut View::from(&gone)));
    }
    shell::with(|s| {
        if let Some(w) = s.windows.iter_mut().find(|w| w.id == window_id) {
            w.pane_notices = kept;
        }
    });
}

/// Where the focused pane is in window `window_id`, in window coordinates,
/// for things that belong over its page, such as an extension's popup.
pub fn focused_area(window_id: u32) -> Option<Rect> {
    let (content, pane) = shell::with(|s| {
        let w = s.windows.iter().find(|w| w.id == window_id)?;
        let content = w.content.clone()?;
        let size = View::from(&content).bounds();
        let pane = w
            .layout
            .bounds(size.width, size.height, GAP)
            .into_iter()
            .find(|(p, _)| *p == w.layout.focused())
            .map(|(_, b)| b)?;
        Some((content, pane))
    })
    .flatten()?;
    let mut origin = Point {
        x: pane.x,
        y: pane.y,
    };
    View::from(&content).convert_point_to_window(Some(&mut origin));
    Some(Rect {
        x: origin.x,
        y: origin.y,
        width: pane.width,
        height: pane.height,
    })
}

/// Focus the current tab's `view`, unless the window isn't the active one
/// or a question or the command line has the keys.
fn focus_current(window_id: u32, view: &BrowserView) {
    let ours = shell::with(|s| {
        s.id == window_id
            && !matches!(
                s.engine.mode(),
                rt_core::Mode::Prompt | rt_core::Mode::YesNo | rt_core::Mode::Command
            )
    })
    .unwrap_or(false);
    if ours {
        tracing::debug!(
            window = window_id,
            "focusing the current tab now that it has a size"
        );
        focus_view(view);
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
        fn on_layout_changed(&self, _view: Option<&mut View>, new_bounds: Option<&Rect>) {
            arrange_in(self.window, new_bounds.cloned());
        }
    }

    impl PanelDelegate {}
}

thread_local! {
    /// Browsers riptide focused itself, and when, until Chromium reports it.
    static OURS: std::cell::RefCell<Vec<(i32, std::time::Instant)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The tab Chromium last reported focused.
    static LAST_FOCUSED: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

/// Focus `view` for riptide. Chromium reports it like a click, and a click in
/// another pane switches panes, so its report is passed over (see [`RtFocusHandler`]).
pub fn focus_view(view: &BrowserView) {
    // A tab that has the focus already isn't reported again, and a request
    // left waiting would swallow the next click on it. (`View::has_focus`
    // would tell, but crashes on a call window's Chrome-style view.)
    if let Some(id) = view.browser().map(|b| b.identifier())
        && id != LAST_FOCUSED.get()
    {
        OURS.with(|o| o.borrow_mut().push((id, std::time::Instant::now())));
    }
    View::from(view).request_focus();
}

/// Whether `browser` got focus because riptide asked; each request answers once.
fn focused_by_us(browser: i32) -> bool {
    OURS.with(|o| {
        let mut o = o.borrow_mut();
        o.retain(|(_, at)| at.elapsed() < std::time::Duration::from_secs(1));
        let found = o.iter().position(|(id, _)| *id == browser);
        found.map(|i| o.remove(i)).is_some()
    })
}

thread_local! {
    /// Tabs focused only to take keys riptide sends, such as a call's mute key
    /// in another window, and when: for a moment their focus doesn't switch windows.
    static LENT: std::cell::RefCell<Vec<(i32, std::time::Instant)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Keep `browser`'s focus over the next moment from switching windows.
pub fn lend_focus(browser: i32) {
    LENT.with(|l| l.borrow_mut().push((browser, std::time::Instant::now())));
}

/// Whether `browser` has the focus only to take riptide's keys (see [`lend_focus`]).
pub fn focus_lent(browser: i32) -> bool {
    LENT.with(|l| {
        let mut l = l.borrow_mut();
        l.retain(|(_, at)| at.elapsed() < std::time::Duration::from_secs(2));
        l.iter().any(|(id, _)| *id == browser)
    })
}

// A click in another pane focuses that pane. Focus riptide gives a tab
// (`focus_view`) reports here too, and acting on it could hand focus back
// and forth between two panes for good.
wrap_focus_handler! {
    pub struct RtFocusHandler;

    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) {
            let Some(id) = browser.map(|b| b.identifier()) else { return };
            LAST_FOCUSED.set(id);
            if focused_by_us(id) {
                tracing::debug!(browser = id, "tab got the focus riptide gave it");
                return;
            }
            tracing::debug!(browser = id, "tab got focus");
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
            if focus_lent(self.browser) {
                return;
            }
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

/// The window's panes to save, each with its tab's saved index (`saved`
/// maps a tab's index to it); `None` with one pane.
pub fn saved(w: &WindowState, saved: impl Fn(usize) -> Option<usize>) -> Option<SplitState> {
    let focused = w.layout.focused();
    let tree = w.layout.tree(|pane| {
        let index = if pane == focused {
            w.tabs.current_index()
        } else {
            let view = &w.pane_tabs.iter().find(|(p, _)| *p == pane)?.1;
            index_of(w, view)?
        };
        saved(index)
    })?;
    Some(to_state(tree))
}

fn to_state(tree: Tree<usize>) -> SplitState {
    match tree {
        Tree::Pane(tab) => SplitState {
            tab: Some(tab),
            ..Default::default()
        },
        Tree::Split { axis, children } => SplitState {
            vertical: axis == Axis::Vertical,
            sizes: children
                .iter()
                .map(|(_, share)| (share * 1000.0).round() as u32)
                .collect(),
            panes: children.into_iter().map(|(c, _)| to_state(c)).collect(),
            tab: None,
        },
    }
}

fn from_state(state: &SplitState, index: &impl Fn(usize) -> Option<usize>) -> Option<Tree<usize>> {
    if let Some(tab) = state.tab {
        return index(tab).map(Tree::Pane);
    }
    // Sizes that don't match the panes are ignored, and the panes share the room.
    let sized = state.sizes.len() == state.panes.len();
    let mut children: Vec<(Tree<usize>, f64)> = state
        .panes
        .iter()
        .enumerate()
        .filter_map(|(i, pane)| {
            let share = if sized {
                f64::from(state.sizes[i])
            } else {
                1.0
            };
            Some((from_state(pane, index)?, share))
        })
        .collect();
    match children.len() {
        0 => None,
        1 => children.pop().map(|(tree, _)| tree),
        _ => Some(Tree::Split {
            axis: if state.vertical {
                Axis::Vertical
            } else {
                Axis::Horizontal
            },
            children,
        }),
    }
}

/// Put a saved window's panes back over its restored tabs: `index` maps a
/// saved tab index to the window's, and the pane with `active` is focused.
pub fn restore(
    w: &mut WindowState,
    state: &SplitState,
    index: impl Fn(usize) -> Option<usize>,
    active: usize,
) {
    w.layout = Layout::new();
    w.pane_tabs.clear();
    let Some(tree) = from_state(state, &index) else {
        return;
    };
    let (layout, panes) = Layout::from_tree(tree, |&tab| tab == active);
    let focused = layout.focused();
    w.pane_tabs = panes
        .into_iter()
        .filter(|(pane, _)| *pane != focused)
        .filter_map(|(pane, tab)| Some((pane, w.tabs.get(tab)?.view.clone())))
        .collect();
    w.layout = layout;
}

/// Whether a saved layout has a pane for the saved tab `tab`.
pub fn shows(state: &SplitState, tab: usize) -> bool {
    state.tab == Some(tab) || state.panes.iter().any(|p| shows(p, tab))
}

/// One pane again, as in a new window.
pub fn reset(w: &mut WindowState) {
    w.layout = Layout::new();
    w.pane_tabs.clear();
}
