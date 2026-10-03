//! Tab operations. Index bookkeeping lives in `hb_core::tabs::TabList`; this
//! module keeps the CEF views in the content panel in step with it.

use cef::*;
use hb_core::Command;
use hb_core::command::{TabMoveTarget, TabTarget};
use hb_core::engine::Level;
use hb_core::tabs::{Position, resolve_index};

use crate::client::Role;
use crate::shell::{self, Tab};
use crate::window;

const MAX_CLOSED_TABS: usize = 100;

/// Carry out tab commands; returns false for commands that are not about tabs.
pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    let n = i64::from(count.unwrap_or(1).max(1));
    match command {
        Command::TabNext => focus_offset(n),
        Command::TabPrev => focus_offset(-n),
        Command::TabFocus(target) => match (count, target) {
            (Some(c), _) => focus_number(i64::from(c)),
            (None, None) => focus_offset(1),
            (None, Some(TabTarget::Number(k))) => focus_number(*k),
            (None, Some(TabTarget::Last)) => match shell::with(|s| s.tabs.previous()).flatten() {
                Some(index) => focus(index),
                None => shell::show_message(Level::Error, "There is no previous tab"),
            },
        },
        Command::TabClose { force } => {
            if let Some(index) = shell::with(|s| s.tabs.current_index()) {
                close_unless_pinned(index, *force);
            }
        }
        Command::TabOnly { force } => {
            let (current, len, pinned) =
                shell::with(|s| (s.tabs.current_index(), s.tabs.len(), s.tabs.pinned_count()))
                    .unwrap_or_default();
            // Pinned tabs come first, so closing from the end keeps indices valid.
            for index in (0..len)
                .rev()
                .filter(|&i| i != current && (*force || i >= pinned))
            {
                close(index);
            }
        }
        Command::TabPin => {
            let target = match count {
                Some(c) => shell::with(|s| resolve_index(i64::from(c), s.tabs.len())).flatten(),
                None => shell::with(|s| s.tabs.current_index()),
            };
            match target {
                Some(index) => toggle_pin(index),
                None => shell::show_message(Level::Error, format!("There's no tab with index {n}")),
            }
        }
        Command::TabMove(target) => move_current(*target, count),
        Command::Undo => match shell::with(|s| s.closed.pop()).flatten() {
            Some((index, url)) => open(&url, Position::At(index), true),
            None => shell::show_message(Level::Error, "No closed tabs to restore"),
        },
        _ => return false,
    }
    shell::refresh_ui();
    true
}

/// Close a tab from a command or a middle-click; pinned tabs need `force`.
pub fn close_unless_pinned(index: usize, force: bool) {
    if !force && shell::with(|s| s.tabs.is_pinned(index)).unwrap_or(false) {
        return shell::show_message(
            Level::Error,
            "Tab is pinned! Use :tab-close --force to close it",
        );
    }
    close(index);
}

fn toggle_pin(index: usize) {
    shell::with(|s| {
        let pin = !s.tabs.is_pinned(index);
        s.tabs.set_pinned(index, pin);
    });
}

/// Mouse-wheel and drag in the tab bar.
pub fn cycle(forward: bool) {
    let enabled =
        shell::with(|s| s.engine.settings().bool("tabs.mousewheel_switching")).unwrap_or(false);
    if let Some(index) = enabled
        .then(|| shell::with(|s| s.tabs.offset(if forward { 1 } else { -1 })))
        .flatten()
    {
        select(index);
    }
}

pub fn move_tab(from: usize, to: usize) {
    shell::with(|s| s.tabs.move_tab(from, to));
    if let Some(index) = shell::with(|s| s.tabs.current_index()) {
        select(index);
    }
}

/// Open `url` in a new tab.
pub fn open(url: &str, position: Position, focus: bool) {
    match window::create_browser_view(Role::Tab, url) {
        Some(view) => add_view(view, position, focus),
        None => shell::show_message(Level::Error, "Could not create a browser view"),
    }
}

/// Adopt a browser view (new or a CEF popup) as a tab.
pub fn add_view(view: BrowserView, position: Position, focus_tab: bool) {
    let Some(content) = shell::with(|s| s.content.clone()).flatten() else {
        return;
    };
    let mut child = View::from(&view);
    child.set_visible(0);
    content.add_child_view(Some(&mut child));
    let Some(index) = shell::with(|s| s.tabs.insert(Tab::new(view), position, false)) else {
        return;
    };
    let first = shell::with(|s| s.tabs.len() == 1).unwrap_or(false);
    if focus_tab || first {
        switch_to(index, true);
    }
}

fn focus(index: usize) {
    switch_to(index, false);
}

/// A tab picked with the mouse. Always re-focuses the page, since the click
/// moved keyboard focus to the tab bar.
pub fn select(index: usize) {
    if shell::with(|s| index < s.tabs.len()).unwrap_or(false) {
        switch_to(index, true);
        shell::refresh_ui();
    }
}

fn focus_offset(n: i64) {
    if let Some(index) = shell::with(|s| s.tabs.offset(n)) {
        focus(index);
    }
}

fn focus_number(number: i64) {
    let len = shell::with(|s| s.tabs.len()).unwrap_or(0);
    match resolve_index(number, len) {
        Some(index) => focus(index),
        None => shell::show_message(Level::Error, format!("There's no tab with index {number}")),
    }
}

/// Make `index` the visible, focused tab. `force` re-shows it even if it is already current.
fn switch_to(index: usize, force: bool) {
    let Some(Some((views, effects))) = shell::with(|s| {
        let leaving = s.tabs.current_index();
        let mode = s.engine.mode();
        if let Some(tab) = s.tabs.get_mut(leaving) {
            tab.mode = mode;
        }
        if !s.tabs.focus(index) && !force {
            return None;
        }
        let current = s.tabs.current()?;
        let url = current.url.clone();
        s.engine.set_url(&url);
        let views: Vec<(BrowserView, bool)> = s
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (t.view.clone(), i == s.tabs.current_index()))
            .collect();
        let left_in = s.tabs.current().map(|t| t.mode);
        Some((views, s.engine.tab_switched(left_in)))
    }) else {
        return;
    };
    for (view, visible) in &views {
        View::from(view).set_visible((*visible).into());
    }
    if let Some((view, _)) = views.iter().find(|(_, visible)| *visible) {
        View::from(view).request_focus();
    }
    shell::apply(effects);
}

/// Close a tab. Closing the last one follows `tabs.last_close`.
pub fn close(index: usize) {
    if shell::with(|s| s.tabs.len() <= 1).unwrap_or(true) {
        return last_close();
    }
    let Some(Some((tab, was_current))) = shell::with(|s| {
        let was_current = index == s.tabs.current_index();
        let tab = s.tabs.remove(index)?;
        if !tab.url.is_empty() {
            s.closed.push((index, tab.url.clone()));
            if s.closed.len() > MAX_CLOSED_TABS {
                s.closed.remove(0);
            }
        }
        Some((tab, was_current))
    }) else {
        return;
    };
    if let Some(content) = shell::with(|s| s.content.clone()).flatten() {
        content.remove_child_view(Some(&mut View::from(&tab.view)));
    }
    if let Some(browser) = tab.browser() {
        crate::prompts::withdraw_for_browser(browser.identifier(), None);
    }
    // Dropping the last reference closes the browser, which re-enters the shell.
    drop(tab);
    if was_current && let Some(index) = shell::with(|s| s.tabs.current_index()) {
        switch_to(index, true);
    }
}

/// Replace the open tabs with a session's (only its first window, for now).
pub fn restore(session: &hb_storage::Session) {
    let Some(window) = session.windows.first().filter(|w| !w.tabs.is_empty()) else {
        return shell::show_message(Level::Error, "That session has no tabs");
    };
    let old = shell::with(|s| s.tabs.len()).unwrap_or(0);
    for tab in &window.tabs {
        open(&tab.url, Position::Last, false);
    }
    for _ in 0..old {
        close(0);
    }
    // Saved sessions list pinned tabs first, so pinning in order keeps it.
    shell::with(|s| {
        for (index, tab) in window.tabs.iter().enumerate() {
            if tab.pinned {
                s.tabs.set_pinned(index, true);
            }
        }
    });
    switch_to(window.active.min(window.tabs.len() - 1), true);
}

wrap_task! {
    pub struct CloseTab {
        index: usize,
    }

    impl Task {
        fn execute(&self) {
            close(self.index);
            shell::refresh_ui();
        }
    }
}

fn last_close() {
    let Some((action, url, window, browser)) = shell::with(|s| {
        let settings = s.engine.settings();
        let url = match settings.str("tabs.last_close") {
            "blank" => Some("about:blank".to_string()),
            "startpage" => Some(
                settings
                    .list("url.start_pages")
                    .first()
                    .cloned()
                    .unwrap_or_else(|| s.default_page()),
            ),
            "default-page" => Some(s.default_page()),
            _ => None,
        };
        (
            settings.str("tabs.last_close").to_string(),
            url,
            s.window.clone(),
            s.current_browser(),
        )
    }) else {
        return;
    };
    match (action.as_str(), url, browser) {
        ("close", _, _) => {
            if let Some(window) = window {
                window.close();
            }
        }
        (_, Some(url), Some(browser)) => {
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        _ => {}
    }
}

fn move_current(target: Option<TabMoveTarget>, count: Option<u32>) {
    shell::with(|s| {
        let len = s.tabs.len();
        let n = i64::from(count.unwrap_or(1).max(1));
        let to = match target {
            None => count
                .and_then(|c| resolve_index(i64::from(c), len))
                .unwrap_or(0),
            Some(TabMoveTarget::Relative(d)) => s.tabs.offset(d * n),
            Some(TabMoveTarget::Absolute(k)) => match resolve_index(k, len) {
                Some(i) => i,
                None => {
                    s.engine
                        .show_message(Level::Error, format!("There's no tab with index {k}"));
                    return;
                }
            },
            Some(TabMoveTarget::Start) => 0,
            Some(TabMoveTarget::End) => len.saturating_sub(1),
        };
        s.tabs.move_current(to);
    });
}
