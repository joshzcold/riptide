//! Split panes (`:split`, `:vsplit`): several panes in one window, each with
//! its own tabs. The focused pane's tabs are the window's `tabs`, so tab
//! commands act on it; the other panes keep theirs in `panes`. Each pane's
//! tab views fill a panel of its own, and each split is a panel whose box
//! layout sizes its panes and the handles between them, so a page is only
//! resized when its pane is. `rt_core::split` keeps the layout.

use cef::*;
use rt_core::Command;
use rt_core::command::{OpenTarget, PaneTarget};
use rt_core::engine::Level;
use rt_core::split::{Axis, Layout, PaneId, Tree};
use rt_core::tabs::{Position, SelectOnRemove, TabList};
use rt_storage::SplitState;

use crate::shell::{self, Pane, PaneViews, Shell, Tab, WindowState};

/// Pixels between panes: the handle that drags the border between them.
pub const GAP: i32 = 4;

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
        Command::TabMovePane(target) => move_to_pane(*target),
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

fn window(s: &Shell, id: u32) -> Option<&WindowState> {
    s.windows.iter().find(|w| w.id == id)
}

fn window_mut(s: &mut Shell, id: u32) -> Option<&mut WindowState> {
    s.windows.iter_mut().find(|w| w.id == id)
}

fn select_on_remove(s: &Shell) -> SelectOnRemove {
    SelectOnRemove::from_setting(s.engine.settings().str("tabs.select_on_remove"))
}

/// The pane `target` names, from the focused one.
fn target_pane(w: &WindowState, target: PaneTarget) -> Option<PaneId> {
    match target {
        PaneTarget::Next => Some(w.layout.next()).filter(|&p| p != w.layout.focused()),
        PaneTarget::Direction(direction) => w.layout.neighbour(direction),
    }
}

/// Add `tab` to `tabs` as their current one, pinned among the pinned tabs.
fn insert_current(tabs: &mut TabList<Tab>, tab: Tab, pinned: bool) {
    if pinned {
        let index = tabs.insert(tab, Position::At(tabs.pinned_count()), true);
        tabs.set_pinned(index, true);
    } else {
        tabs.insert(tab, Position::Last, true);
    }
}

/// Split the focused pane; the new pane gets `url` (or the default page) in
/// a new tab, and is focused. With `move_tab` the current tab moves to the
/// new pane instead, and the old pane shows another of its tabs, or `url`
/// in a new one when given or when it has no other.
fn split(axis: Axis, url: Option<String>, move_tab: bool) {
    let Some(window_id) = shell::with(|s| s.id) else {
        return;
    };
    let opened = if move_tab && (url.is_some() || shell::with(|s| s.tabs.len() < 2) == Some(true)) {
        open_in_background(url.clone())
    } else {
        None
    };
    let Some(Some((new, moved))) = shell::with(|s| {
        if s.tabs.is_empty() {
            return None;
        }
        let select = select_on_remove(s);
        let old = s.layout.focused();
        let new = s.layout.split(axis);
        let mut tabs = std::mem::take(&mut s.tabs);
        let moved = if move_tab {
            let current = tabs.current_index();
            let pinned = tabs.is_pinned(current);
            let tab = tabs.remove_selecting(current, select)?;
            if let Some(i) = opened
                .as_ref()
                .and_then(|o| tabs.position(|t| same(&t.view, o)))
            {
                tabs.focus(i);
            }
            let view = tab.view.clone();
            insert_current(&mut s.tabs, tab, pinned);
            Some(view)
        } else {
            None
        };
        s.panes.push(Pane { id: old, tabs });
        Some((new, moved))
    }) else {
        return;
    };
    arrange(window_id);
    match moved {
        Some(view) => {
            if let Some(body) = body(window_id, new) {
                reparent(&view, &body);
            }
            crate::tabs::show_current_tab();
        }
        // Not through the command: with no tab yet, the new pane has no page to run it on.
        None => shell::open(OpenTarget::Tab, false, url),
    }
}

/// Open `url` (or the default page) in a background tab of the focused pane; its view.
fn open_in_background(url: Option<String>) -> Option<BrowserView> {
    let before: Vec<BrowserView> =
        shell::with(|s| s.tabs.iter().map(|t| t.view.clone()).collect()).unwrap_or_default();
    shell::open(OpenTarget::Background, false, url);
    shell::with(|s| {
        s.tabs
            .iter()
            .find(|t| !before.iter().any(|b| same(b, &t.view)))
            .map(|t| t.view.clone())
    })
    .flatten()
}

/// Close the focused pane. Its tabs join the pane that takes its room, which
/// keeps its current tab and takes the focus.
fn close_focused() {
    let closed = shell::with(|s| {
        if !s.layout.is_split() {
            return Err("There's only one pane");
        }
        Ok(close_pane(s))
    });
    match closed {
        Some(Err(message)) => shell::show_message(Level::Error, message),
        Some(Ok(Some((window_id, pane, views)))) => moved_to(window_id, pane, &views),
        _ => {}
    }
}

/// Panes' tabs as one list, in the panes' order on screen (`order`), with
/// `current`'s current tab current.
fn gather(
    mut lists: Vec<(PaneId, TabList<Tab>)>,
    order: &[PaneId],
    current: PaneId,
) -> TabList<Tab> {
    lists.sort_by_key(|(pane, _)| order.iter().position(|p| p == pane));
    let shown = lists
        .iter()
        .find(|(pane, _)| *pane == current)
        .and_then(|(_, tabs)| Some(tabs.current()?.view.clone()));
    let mut out = TabList::default();
    for (_, mut tabs) in lists {
        out.append(tabs.take_all());
    }
    if let Some(i) = shown.and_then(|v| out.position(|t| same(&t.view, &v))) {
        out.focus(i);
    }
    out
}

/// [`close_focused`] on the window's state: the window, the pane now focused
/// and the views that moved to it.
pub fn close_pane(w: &mut WindowState) -> Option<(u32, PaneId, Vec<BrowserView>)> {
    let closing = w.layout.focused();
    let order = w.layout.panes();
    if !w.layout.close(closing) {
        return None;
    }
    let next = w.layout.focused();
    let i = w.panes.iter().position(|p| p.id == next)?;
    let theirs = w.panes.remove(i).tabs;
    let mine = std::mem::take(&mut w.tabs);
    let views = mine.iter().map(|t| t.view.clone()).collect();
    w.tabs = gather(vec![(closing, mine), (next, theirs)], &order, next);
    Some((w.id, next, views))
}

/// Tabs moved to `pane`: put their views in its body and show the panes again.
pub fn moved_to(window_id: u32, pane: PaneId, views: &[BrowserView]) {
    arrange(window_id);
    if let Some(body) = body(window_id, pane) {
        for view in views {
            reparent(view, &body);
        }
    }
    arrange(window_id);
    crate::tabs::show_current_tab();
}

/// Keep only the focused pane, which takes the other panes' tabs.
fn only() {
    let Some((window_id, pane, views)) = shell::with(|s| {
        let order = s.layout.panes();
        let focused = s.layout.focused();
        s.layout.only();
        let others = std::mem::take(&mut s.panes);
        let views: Vec<BrowserView> = others
            .iter()
            .flat_map(|p| p.tabs.iter().map(|t| t.view.clone()))
            .collect();
        let lists = std::iter::once((focused, std::mem::take(&mut s.tabs)))
            .chain(others.into_iter().map(|p| (p.id, p.tabs)))
            .collect();
        s.tabs = gather(lists, &order, focused);
        (s.id, focused, views)
    }) else {
        return;
    };
    moved_to(window_id, pane, &views);
}

fn focus(target: PaneTarget) {
    let switched = shell::with(|s| {
        let pane = target_pane(s, target)?;
        s.focus_pane(pane).then_some(())
    })
    .flatten();
    if switched.is_some() {
        crate::tabs::show_current_tab();
    }
}

/// `:tab-move-pane`: the current tab moves to the pane `target` names and
/// is focused there. A pane left without tabs closes.
fn move_to_pane(target: PaneTarget) {
    let moved = shell::with(|s| {
        let pane = target_pane(s, target)?;
        if s.tabs.is_empty() {
            return None;
        }
        let select = select_on_remove(s);
        let current = s.tabs.current_index();
        let pinned = s.tabs.is_pinned(current);
        let tab = s.tabs.remove_selecting(current, select)?;
        let view = tab.view.clone();
        let source = s.layout.focused();
        let emptied = s.tabs.is_empty();
        s.focus_pane(pane);
        insert_current(&mut s.tabs, tab, pinned);
        if emptied {
            s.layout.close(source);
            s.panes.retain(|p| p.id != source);
        }
        Some((s.id, pane, view))
    })
    .flatten();
    match moved {
        Some((window_id, pane, view)) => moved_to(window_id, pane, &[view]),
        None => shell::show_message(Level::Error, "There's no pane there"),
    }
}

/// The focused pane's last tab closed: the pane closes too, and its
/// neighbour takes the focus. False when it's the only pane.
pub fn close_emptied_pane() -> bool {
    match shell::with(|s| s.layout.is_split().then(|| close_pane(s)).flatten()).flatten() {
        Some((window_id, pane, views)) => {
            moved_to(window_id, pane, &views);
            true
        }
        None => false,
    }
}

/// A click on a tab in another pane: that pane takes the focus. True if it did.
pub fn focus_clicked(browser: i32) -> bool {
    let switched = shell::with(|s| {
        let is = |t: &Tab| t.browser().is_some_and(|b| b.identifier() == browser);
        let (pane, index) = s
            .panes
            .iter()
            .find_map(|p| Some((p.id, p.tabs.position(|t| is(t))?)))?;
        s.focus_pane(pane);
        s.tabs.focus(index);
        Some(())
    })
    .flatten();
    if switched.is_some() {
        crate::tabs::show_current_tab();
    }
    switched.is_some()
}

/// Put `view` in `body`, out of whichever panel had it.
fn reparent(view: &BrowserView, body: &Panel) {
    let mut child = View::from(view);
    if let Some(parent) = child.parent_view() {
        if parent.is_same(Some(&mut View::from(body))) != 0 {
            return;
        }
        if let Some(parent) = parent.as_panel() {
            parent.remove_child_view(Some(&mut child));
        }
    }
    body.add_child_view(Some(&mut child));
}

/// Take `view` out of its parent, if it has one.
fn detach(view: &View) {
    if let Some(parent) = view.parent_view().and_then(|p| p.as_panel()) {
        parent.remove_child_view(Some(&mut view.clone()));
    }
}

/// Pane `pane`'s body in window `window_id`, made if it's new.
pub fn body(window_id: u32, pane: PaneId) -> Option<Panel> {
    let existing =
        shell::with(|s| Some(window(s, window_id)?.views_of(pane)?.body.clone())).flatten();
    if existing.is_some() {
        return existing;
    }
    let body = panel_create(Some(&mut BodyDelegate::new(window_id, pane)))?;
    shell::with(|s| {
        if let Some(w) = window_mut(s, window_id) {
            w.pane_views.push(PaneViews {
                pane,
                body: body.clone(),
                notice: None,
            });
        }
    });
    Some(body)
}

/// The focused pane's body in the active window, where a new tab goes.
pub fn focused_body() -> Option<Panel> {
    let (window_id, pane) = shell::with(|s| (s.id, s.layout.focused()))?;
    body(window_id, pane)
}

/// The layout's shape, without the sizes: the panels are made again when it changes.
fn shape(tree: &Tree<PaneId>) -> String {
    match tree {
        Tree::Pane(pane) => pane.to_string(),
        Tree::Split { axis, children } => {
            let children: Vec<String> = children.iter().map(|(c, _)| shape(c)).collect();
            format!("{:?}({})", axis, children.join(","))
        }
    }
}

/// A share of a split as a box layout's flex.
fn flex(share: f64) -> i32 {
    ((share * 10_000.0).round() as i32).max(1)
}

/// Make window `window_id`'s panels match its layout: a body for each pane
/// in the page area, or while it's split, a panel for each split holding its
/// panes and the handles between them.
pub fn arrange(window_id: u32) {
    let Some(Some((content, panes, focused, tree, built))) = shell::with(|s| {
        let w = window(s, window_id)?;
        Some((
            w.content.clone()?,
            w.layout.panes(),
            w.layout.focused(),
            w.layout
                .tree(Some)
                .unwrap_or(Tree::Pane(w.layout.focused())),
            w.split_shape.clone(),
        ))
    }) else {
        return;
    };
    for &pane in &panes {
        body(window_id, pane);
    }
    // A closed pane's body goes. Its tabs moved already; anything left joins the focused pane.
    let gone = shell::with(|s| {
        let w = window_mut(s, window_id)?;
        let (kept, gone): (Vec<PaneViews>, Vec<PaneViews>) = std::mem::take(&mut w.pane_views)
            .into_iter()
            .partition(|v| panes.contains(&v.pane));
        w.pane_views = kept;
        Some(gone)
    })
    .flatten()
    .unwrap_or_default();
    if !gone.is_empty()
        && let Some(target) = body(window_id, focused)
    {
        for views in gone {
            let notice = views.notice.as_ref().map(View::from);
            while let Some(mut child) = views.body.child_view_at(0) {
                views.body.remove_child_view(Some(&mut child));
                if notice
                    .as_ref()
                    .is_none_or(|n| n.is_same(Some(&mut child.clone())) == 0)
                {
                    target.add_child_view(Some(&mut child));
                }
            }
            detach(&View::from(&views.body));
        }
    }
    let shape = shape(&tree);
    if shape != built {
        rebuild(window_id, &content, &tree);
        shell::with(|s| {
            if let Some(w) = window_mut(s, window_id) {
                w.split_shape = shape;
            }
        });
    } else {
        resize_splits(window_id, &tree);
    }
    color_handles(window_id);
}

/// Make the split panels again for `tree`, keeping the bodies and the handles
/// whose borders are still there.
fn rebuild(window_id: u32, content: &Panel, tree: &Tree<PaneId>) {
    let Some((bodies, splits, mut handles)) = shell::with(|s| {
        let w = window_mut(s, window_id)?;
        let bodies: Vec<Panel> = w.pane_views.iter().map(|v| v.body.clone()).collect();
        Some((
            bodies,
            std::mem::take(&mut w.split_panels),
            std::mem::take(&mut w.pane_handles),
        ))
    })
    .flatten() else {
        return;
    };
    for body in &bodies {
        detach(&View::from(body));
    }
    for (_, handle, _) in &handles {
        detach(&View::from(handle));
    }
    for (_, panel) in &splits {
        detach(&View::from(panel));
    }
    let mut panels = Vec::new();
    let mut kept = Vec::new();
    let root = build(
        window_id,
        tree,
        &mut Vec::new(),
        &mut panels,
        &mut handles,
        &mut kept,
    );
    if let Some(mut root) = root {
        content.add_child_view(Some(&mut root));
    }
    shell::with(|s| {
        if let Some(w) = window_mut(s, window_id) {
            w.split_panels = panels;
            w.pane_handles = kept;
        }
    });
    // Handles whose borders went close here, outside the shell.
    drop(handles);
}

/// The view for `tree`: a pane's body, or a panel for a split.
fn build(
    window_id: u32,
    tree: &Tree<PaneId>,
    path: &mut Vec<usize>,
    panels: &mut Vec<(String, Panel)>,
    old_handles: &mut Vec<(String, BrowserView, u32)>,
    handles: &mut Vec<(String, BrowserView, u32)>,
) -> Option<View> {
    let (axis, children) = match tree {
        Tree::Pane(pane) if path.is_empty() => {
            return body(window_id, *pane).map(|b| View::from(&b));
        }
        Tree::Pane(pane) => return framed(&body(window_id, *pane)?),
        Tree::Split { axis, children } => (*axis, children),
    };
    let panel = panel_create(None)?;
    let layout = panel.set_to_box_layout(Some(&BoxLayoutSettings {
        horizontal: (axis == Axis::Vertical).into(),
        cross_axis_alignment: AxisAlignment::STRETCH,
        ..Default::default()
    }))?;
    for (i, (child, share)) in children.iter().enumerate() {
        path.push(i);
        let child = build(window_id, child, path, panels, old_handles, handles);
        path.pop();
        if let Some(mut child) = child {
            panel.add_child_view(Some(&mut child));
            layout.set_flex_for_view(Some(&mut child), flex(*share));
        }
        if i + 1 == children.len() {
            continue;
        }
        let id = rt_core::split::border_id(path, i);
        let handle = match old_handles.iter().position(|(h, _, _)| *h == id) {
            Some(at) => Some(old_handles.remove(at)),
            None => new_handle(&id, axis),
        };
        if let Some((id, view, color)) = handle {
            let mut handle = View::from(&view);
            panel.add_child_view(Some(&mut handle));
            layout.set_flex_for_view(Some(&mut handle), 0);
            handles.push((id, view, color));
        }
    }
    panels.push((path_key(path), panel.clone()));
    Some(View::from(&panel))
}

/// Height of the strip along a pane's top that shows which pane has the focus.
const FRAME: i32 = 2;

/// A panel holding a split's pane `body` below its focus strip.
fn framed(body: &Panel) -> Option<View> {
    let frame = panel_create(None)?;
    let layout = frame.set_to_box_layout(Some(&BoxLayoutSettings {
        horizontal: 0,
        cross_axis_alignment: AxisAlignment::STRETCH,
        inside_border_insets: Insets {
            top: FRAME,
            ..Default::default()
        },
        ..Default::default()
    }))?;
    let mut body = View::from(body);
    frame.add_child_view(Some(&mut body));
    layout.set_flex_for_view(Some(&mut body), 1);
    Some(View::from(&frame))
}

fn path_key(path: &[usize]) -> String {
    path.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// A handle for the border `id` between panes split along `axis`.
fn new_handle(id: &str, axis: Axis) -> Option<(String, BrowserView, u32)> {
    let (gap, _) = split_colors();
    let axis = if axis == Axis::Vertical { "v" } else { "h" };
    let url = format!("{HANDLE_URL}#{axis}/{id}/{:06x}", gap & 0x00ff_ffff);
    let view = crate::window::create_browser_view(crate::client::Role::SplitHandle, &url)?;
    View::from(&view).set_focusable(0);
    Some((id.to_string(), view, gap))
}

/// The layout kept its shape but its sizes changed: give each split's panes their new flex.
fn resize_splits(window_id: u32, tree: &Tree<PaneId>) {
    let Some(panels) = shell::with(|s| Some(window(s, window_id)?.split_panels.clone())).flatten()
    else {
        return;
    };
    fn walk(tree: &Tree<PaneId>, path: &mut Vec<usize>, panels: &[(String, Panel)]) {
        let Tree::Split { children, .. } = tree else {
            return;
        };
        let key = path_key(path);
        if let Some((_, panel)) = panels.iter().find(|(k, _)| *k == key)
            && let Some(layout) = panel.get_layout().and_then(|l| l.as_box_layout())
        {
            for (i, (_, share)) in children.iter().enumerate() {
                if let Some(mut child) = panel.child_view_at(i32::try_from(2 * i).unwrap_or(0)) {
                    layout.set_flex_for_view(Some(&mut child), flex(*share));
                }
            }
            View::from(panel).invalidate_layout();
        }
        for (i, (child, _)) in children.iter().enumerate() {
            path.push(i);
            walk(child, path, panels);
            path.pop();
        }
    }
    walk(tree, &mut Vec::new(), &panels);
}

/// The focused pane's strip shows the focus color; the other strips and the handles, the gaps' color.
fn color_handles(window_id: u32) {
    let (gap, accent) = split_colors();
    let Some(Some((frames, handles))) = shell::with(|s| {
        let w = window(s, window_id)?;
        if w.layout.panes().len() < 2 {
            return None;
        }
        let frames: Vec<(View, bool)> = w
            .pane_views
            .iter()
            .filter_map(|v| {
                let frame = View::from(&v.body).parent_view()?;
                Some((frame, v.pane == w.layout.focused()))
            })
            .collect();
        Some((frames, w.pane_handles.clone()))
    }) else {
        return;
    };
    for (frame, focused) in &frames {
        frame.set_background_color(if *focused { accent } else { gap });
    }
    let mut colors = Vec::new();
    for (_, view, was) in &handles {
        let color = gap;
        if color != *was
            && let Some(frame) = view.browser().and_then(|b| b.main_frame())
        {
            shell::exec_js(
                &frame,
                &format!("window.rtColor && rtColor('#{:06x}')", color & 0x00ff_ffff),
            );
        }
        colors.push(color);
    }
    shell::with(|s| {
        if let Some(w) = window_mut(s, window_id) {
            for ((_, _, color), new) in w.pane_handles.iter_mut().zip(colors) {
                *color = new;
            }
        }
    });
}

/// Show each pane's current tab and hide its others; a tab whose renderer
/// died is replaced by its pane's crash notice. With `focus`, the focused
/// pane's page (or notice) takes the keyboard focus.
pub fn show(window_id: u32, focus: bool) {
    arrange(window_id);
    // Per pane: its id, body, tabs (view, current), crash notice URL and notice.
    type Shown = (
        PaneId,
        Panel,
        Vec<(BrowserView, bool)>,
        Option<String>,
        Option<BrowserView>,
    );
    let Some(Some((focused, mut panes))) = shell::with(|s| {
        let w = window(s, window_id)?;
        let describe = |pane: PaneId, tabs: &TabList<Tab>| -> Option<Shown> {
            let views = w.views_of(pane)?;
            let current = tabs.current_index();
            let crashed = tabs
                .current()
                .and_then(|t| Some(crate::ui::crashed_url(&t.url, t.crashed.as_deref()?)));
            Some((
                pane,
                views.body.clone(),
                tabs.iter()
                    .enumerate()
                    .map(|(i, t)| (t.view.clone(), i == current))
                    .collect(),
                crashed,
                views.notice.clone(),
            ))
        };
        let focused = w.layout.focused();
        let panes: Vec<Shown> = std::iter::once(describe(focused, &w.tabs))
            .chain(w.panes.iter().map(|p| describe(p.id, &p.tabs)))
            .flatten()
            .collect();
        Some((focused, panes))
    }) else {
        return;
    };
    for (pane, body, _, crashed, notice) in &mut panes {
        match (crashed.as_deref(), notice.as_ref()) {
            (Some(url), None) => {
                *notice = crate::window::create_browser_view(crate::client::Role::Crashed, url);
                if let Some(view) = notice.as_ref() {
                    body.add_child_view(Some(&mut View::from(view)));
                    let (pane, view) = (*pane, view.clone());
                    shell::with(|s| {
                        let views = window_mut(s, window_id)?
                            .pane_views
                            .iter_mut()
                            .find(|v| v.pane == pane)?;
                        views.notice = Some(view);
                        Some(())
                    });
                }
            }
            // Only the fragment changes, so the notice redraws without reloading.
            (Some(url), Some(view)) => {
                if let Some(frame) = view.browser().and_then(|b| b.main_frame()) {
                    frame.load_url(Some(&CefString::from(url)));
                }
            }
            (None, _) => {}
        }
    }
    // Show, then focus, then hide: hiding the focused view makes Chromium move
    // the focus itself, which would read as a click in that pane.
    for (_, _, tabs, crashed, notice) in &panes {
        for (view, current) in tabs {
            if *current && crashed.is_none() {
                View::from(view).set_visible(1);
                // A lazily restored tab loads once it's on screen.
                crate::tabs::load_pending(view);
            }
        }
        if let Some(notice) = notice.as_ref().filter(|_| crashed.is_some()) {
            View::from(notice).set_visible(1);
        }
    }
    if focus && let Some((_, _, tabs, crashed, notice)) = panes.iter().find(|p| p.0 == focused) {
        let shown = match (crashed, notice) {
            (Some(_), Some(notice)) => Some(notice),
            _ => tabs.iter().find(|(_, current)| *current).map(|(v, _)| v),
        };
        if let Some(view) = shown {
            focus_view(view);
        }
    }
    for (_, _, tabs, crashed, notice) in &panes {
        for (view, current) in tabs {
            if !*current || crashed.is_some() {
                View::from(view).set_visible(0);
            }
        }
        if let Some(notice) = notice.as_ref().filter(|_| crashed.is_none()) {
            View::from(notice).set_visible(0);
        }
    }
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
        focus_view(&view);
    }
}

/// Where the focused pane is in window `window_id`, in window coordinates,
/// for things that belong over its page, such as an extension's popup.
pub fn focused_area(window_id: u32) -> Option<Rect> {
    let body = shell::with(|s| {
        let w = window(s, window_id)?;
        Some(w.views_of(w.layout.focused())?.body.clone())
    })
    .flatten()?;
    let view = View::from(&body);
    let size = view.bounds();
    let mut origin = Point { x: 0, y: 0 };
    view.convert_point_to_window(Some(&mut origin));
    Some(Rect {
        x: origin.x,
        y: origin.y,
        width: size.width,
        height: size.height,
    })
}

/// Each pane's place in the page area, for the test channel: its id, whether
/// it's focused, and its bounds.
pub fn pane_bounds(w: &WindowState) -> Vec<(PaneId, bool, Rect)> {
    let Some(content) = w.content.as_ref().map(View::from) else {
        return Vec::new();
    };
    let origin = content.bounds_in_screen();
    w.pane_views
        .iter()
        .filter(|v| w.layout.contains(v.pane))
        .map(|v| {
            let b = View::from(&v.body).bounds_in_screen();
            (
                v.pane,
                v.pane == w.layout.focused(),
                Rect {
                    x: b.x - origin.x,
                    y: b.y - origin.y,
                    width: b.width,
                    height: b.height,
                },
            )
        })
        .collect()
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

// A pane's body: its tab views fill it, and its crash notice when its tab crashed.
wrap_panel_delegate! {
    pub struct BodyDelegate {
        window: u32,
        pane: PaneId,
    }

    impl ViewDelegate {
        fn on_layout_changed(&self, view: Option<&mut View>, new_bounds: Option<&Rect>) {
            let (Some(view), Some(bounds)) = (view, new_bounds) else { return };
            if bounds.width <= 0 || bounds.height <= 0 {
                return;
            }
            // Hidden tabs aren't laid out; sized here, a background tab lays
            // its page out at the size it'll be shown at.
            if let Some(panel) = view.as_panel() {
                let size = Rect { x: 0, y: 0, width: bounds.width, height: bounds.height };
                for i in 0..panel.child_view_count() {
                    let Some(child) = panel.child_view_at(i32::try_from(i).unwrap_or(0)) else { continue };
                    if child.is_visible() == 0 {
                        child.set_bounds(Some(&size));
                    }
                }
            }
            // A view can't take keyboard focus while it has no size, so the
            // first tab asks again once it has one.
            let (window, pane) = (self.window, self.pane);
            let first = shell::with(|s| {
                let w = window_mut(s, window)?;
                if w.focused_with_size || w.layout.focused() != pane {
                    return None;
                }
                w.focused_with_size = true;
                Some(w.tabs.current()?.view.clone())
            })
            .flatten();
            if let Some(tab) = first {
                focus_current(window, &tab);
            }
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
            if focus_clicked(self.browser) {
                shell::refresh_ui();
            }
        }
    }
}

/// The window's panes to save, each with its tabs: `saved` maps a pane and
/// a tab's index among its tabs to its index in the saved window. `None`
/// with one pane.
pub fn saved(
    w: &WindowState,
    saved: impl Fn(PaneId, usize) -> Option<usize>,
) -> Option<SplitState> {
    let focused = w.layout.focused();
    let tree = w.layout.tree(|pane| {
        let tabs = if pane == focused {
            &w.tabs
        } else {
            &w.panes.iter().find(|p| p.id == pane)?.tabs
        };
        let current = saved(pane, tabs.current_index())?;
        let all = (0..tabs.len()).filter_map(|i| saved(pane, i)).collect();
        Some((current, all))
    })?;
    Some(to_state(tree))
}

/// A saved pane: the window's index of the tab it shows, and of all its tabs.
type SavedPane = (usize, Vec<usize>);

fn to_state(tree: Tree<SavedPane>) -> SplitState {
    match tree {
        Tree::Pane((tab, tabs)) => SplitState {
            tab: Some(tab),
            tabs,
            ..Default::default()
        },
        Tree::Split { axis, children } => SplitState {
            vertical: axis == Axis::Vertical,
            sizes: children
                .iter()
                .map(|(_, share)| (share * 1000.0).round() as u32)
                .collect(),
            panes: children.into_iter().map(|(c, _)| to_state(c)).collect(),
            ..Default::default()
        },
    }
}

/// A saved layout with each pane's shown tab and all its tabs, mapped by `index`.
fn from_state(
    state: &SplitState,
    index: &impl Fn(usize) -> Option<usize>,
) -> Option<Tree<SavedPane>> {
    if let Some(tab) = state.tab {
        let shown = index(tab)?;
        let mut tabs: Vec<usize> = state.tabs.iter().filter_map(|&t| index(t)).collect();
        if !tabs.contains(&shown) {
            tabs.push(shown);
        }
        return Some(Tree::Pane((shown, tabs)));
    }
    // Sizes that don't match the panes are ignored, and the panes share the room.
    let sized = state.sizes.len() == state.panes.len();
    let mut children: Vec<(Tree<SavedPane>, f64)> = state
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

/// Put a saved window's panes back. Its tabs are all open in the focused
/// pane, and `index` maps a saved tab's index to theirs. Each pane takes its
/// tabs; the pane showing `active` keeps the tabs no pane names.
pub fn restore(
    window_id: u32,
    state: &SplitState,
    index: impl Fn(usize) -> Option<usize>,
    active: usize,
) {
    let Some(tree) = from_state(state, &index) else {
        return;
    };
    let moved = shell::with(|s| {
        let w = window_mut(s, window_id)?;
        let (mut layout, panes) = Layout::from_tree(tree, |(tab, _)| *tab == active);
        let focused = layout.focused();
        let mut slots: Vec<Option<(Tab, bool)>> = w.tabs.take_all().into_iter().map(Some).collect();
        let mut take = |indices: &[usize], current: usize| {
            let mut tabs = TabList::default();
            let mut shown = None;
            for &i in indices {
                if let Some(tab) = slots.get_mut(i).and_then(Option::take) {
                    if i == current {
                        shown = Some(tabs.len());
                    }
                    tabs.append(vec![tab]);
                }
            }
            if let Some(shown) = shown {
                tabs.focus(shown);
            }
            tabs
        };
        let mut others = Vec::new();
        let mut moved = Vec::new();
        for (pane, (current, indices)) in &panes {
            if *pane == focused {
                continue;
            }
            let tabs = take(indices, *current);
            if tabs.is_empty() {
                layout.close(*pane);
                continue;
            }
            moved.extend(tabs.iter().map(|t| (*pane, t.view.clone())));
            others.push(Pane { id: *pane, tabs });
        }
        let rest: Vec<usize> = (0..slots.len()).collect();
        let mut tabs = TabList::default();
        let mut shown = None;
        for i in rest {
            if let Some(tab) = slots[i].take() {
                if i == active {
                    shown = Some(tabs.len());
                }
                tabs.append(vec![tab]);
            }
        }
        if let Some(shown) = shown {
            tabs.focus(shown);
        }
        w.tabs = tabs;
        w.panes = others;
        w.layout = layout;
        Some(moved)
    })
    .flatten()
    .unwrap_or_default();
    arrange(window_id);
    for (pane, view) in &moved {
        if let Some(body) = body(window_id, *pane) {
            reparent(view, &body);
        }
    }
    arrange(window_id);
}

/// Whether a saved layout has a pane showing the saved tab `tab`.
pub fn shows(state: &SplitState, tab: usize) -> bool {
    state.tab == Some(tab) || state.panes.iter().any(|p| shows(p, tab))
}

/// One pane again, as in a new window, with every tab in it.
pub fn reset(w: &mut WindowState) {
    for mut pane in std::mem::take(&mut w.panes) {
        let tabs = pane.tabs.take_all();
        w.tabs.append(tabs);
    }
    w.layout.only();
}
