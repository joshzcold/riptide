//! Tab operations. Index bookkeeping lives in `rt_core::tabs::TabList`; this
//! module keeps the CEF views in the content panel in step with it.

use cef::*;
use rt_core::Command;
use rt_core::command::{TabMoveTarget, TabTarget};
use rt_core::engine::Level;
use rt_core::tabs::{Position, SelectOnRemove, resolve_index};

use crate::client::Role;
use crate::shell::{self, Tab};
use crate::window;

/// Carry out tab commands; returns false for commands that are not about tabs.
pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    let n = i64::from(count.unwrap_or(1).max(1));
    match command {
        Command::TabSelect { target } => select_tab(target),
        Command::TabClone { background, window } => clone_tab(*background, *window),
        Command::TabGive { window } => give_tab(*window),
        Command::TabTake { target } => take_tab(target),
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
        .then(|| {
            shell::with(|s| {
                s.tabs.offset_wrapping(
                    if forward { 1 } else { -1 },
                    s.engine.settings().bool("tabs.wrap"),
                )
            })
        })
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
        Some(view) => {
            // Known before the page commits, so closing it early can still be undone.
            if let Some(index) = add_view(view, position, focus) {
                shell::with(|s| s.tabs.get_mut(index).map(|tab| tab.url = url.to_string()));
            }
            crate::lua::emit("tab_opened", &[("url", url)]);
        }
        None => shell::show_message(Level::Error, "Could not create a browser view"),
    }
}

/// Adopt a browser view (new or a CEF popup) as a tab; returns its index.
pub fn add_view(view: BrowserView, position: Position, focus_tab: bool) -> Option<usize> {
    let content = shell::with(|s| s.content.clone()).flatten()?;
    let mut child = View::from(&view);
    child.set_visible(0);
    content.add_child_view(Some(&mut child));
    let index = shell::with(|s| s.tabs.insert(Tab::new(view), position, false))?;
    let first = shell::with(|s| s.tabs.len() == 1).unwrap_or(false);
    if focus_tab || first {
        switch_to(index, true);
    }
    Some(index)
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
    if let Some(index) = shell::with(|s| {
        s.tabs
            .offset_wrapping(n, s.engine.settings().bool("tabs.wrap"))
    }) {
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
        load_pending(view);
    }
    shell::apply(effects);
}

/// A lazily restored tab is shown for the first time: load its page.
fn load_pending(view: &BrowserView) {
    let Some(frame) = view.browser().and_then(|b| b.main_frame()) else {
        return;
    };
    let url = shell::with(|s| s.tabs.current_mut()?.pending.take()).flatten();
    if let Some(url) = url {
        frame.load_url(Some(&CefString::from(url.as_str())));
    }
}

/// Close the tab a `window/tab` label (from tab completion) names, in
/// whichever window it is.
pub fn close_label(label: &str) {
    let Some((window, tab)) = label.split_once('/').and_then(|(w, t)| {
        Some((
            w.trim().parse::<usize>().ok()?,
            t.trim().parse::<usize>().ok()?,
        ))
    }) else {
        return;
    };
    let Some(Some(previous)) = shell::with(|s| {
        let index = open_window_index(s, window)?;
        Some(std::mem::replace(&mut s.active, index))
    }) else {
        return;
    };
    if let Some(index) = tab.checked_sub(1) {
        close(index);
    }
    shell::with(|s| {
        if previous < s.windows.len() {
            s.active = previous;
        }
    });
}

/// Close a tab. Closing the last one follows `tabs.last_close`.
pub fn close(index: usize) {
    if shell::with(|s| s.tabs.len() <= 1).unwrap_or(true) {
        return last_close();
    }
    let Some(Some((tab, was_current))) = shell::with(|s| {
        let was_current = index == s.tabs.current_index();
        let select = SelectOnRemove::from_setting(s.engine.settings().str("tabs.select_on_remove"));
        let undo = s.engine.settings().int("tabs.undo_stack_size").max(0) as usize;
        let tab = s.tabs.remove_selecting(index, select)?;
        if !tab.url.is_empty() {
            s.closed.push((index, tab.url.clone()));
            if s.closed.len() > undo {
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
/// Replace this window's tabs with the session's first window, and open
/// its other windows.
pub fn restore(session: &rt_storage::Session) {
    let Some(window) = session.windows.first().filter(|w| !w.tabs.is_empty()) else {
        return shell::show_message(Level::Error, "That session has no tabs");
    };
    restore_window(window);
    for other in session
        .windows
        .iter()
        .skip(1)
        .filter(|w| !w.tabs.is_empty())
    {
        crate::window::create_from_session(other.clone());
    }
}

/// Replace the active window's tabs with a saved window's.
pub fn restore_window(window: &rt_storage::WindowState) {
    if window.tabs.is_empty() {
        return;
    }
    let old = shell::with(|s| s.tabs.len()).unwrap_or(0);
    let lazy = shell::with(|s| s.engine.settings().bool("session.lazy_restore")).unwrap_or(false);
    let active = window.active.min(window.tabs.len() - 1);
    for (i, tab) in window.tabs.iter().enumerate() {
        if !lazy || i == active {
            open(&tab.url, Position::Last, false);
            continue;
        }
        open("about:blank", Position::Last, false);
        shell::with(|s| {
            let last = s.tabs.len().checked_sub(1)?;
            let placeholder = s.tabs.get_mut(last)?;
            placeholder.url = tab.url.clone();
            placeholder.title = tab.title.clone();
            placeholder.pending = Some(tab.url.clone());
            Some(())
        });
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

/// An open tab, for `:tab-select` completion: window and tab numbers
/// (from 1), title and URL.
pub struct OpenTab {
    pub window: usize,
    pub tab: usize,
    pub title: String,
    pub url: String,
}

thread_local! {
    /// Taken on every redraw: completion runs inside the shell borrow and
    /// can't look at the windows itself.
    static OPEN_TABS: std::cell::RefCell<Vec<OpenTab>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn remember_open_tabs(s: &shell::Shell) {
    let tabs = s
        .windows
        .iter()
        .filter(|w| w.window.is_some())
        .enumerate()
        .flat_map(|(w, state)| {
            state.tabs.iter().enumerate().map(move |(t, tab)| OpenTab {
                window: w + 1,
                tab: t + 1,
                title: tab.title.clone(),
                url: tab.url.clone(),
            })
        })
        .collect();
    OPEN_TABS.with(|o| *o.borrow_mut() = tabs);
}

/// Completions for `:tab-select`: every word typed must appear in the
/// position, title or URL.
pub fn completions(pattern: &str) -> Vec<rt_core::completion::Completion> {
    let words: Vec<String> = pattern.split_whitespace().map(str::to_lowercase).collect();
    OPEN_TABS.with(|o| {
        o.borrow()
            .iter()
            .filter(|t| {
                let hay = format!("{}/{} {} {}", t.window, t.tab, t.title, t.url).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|t| rt_core::completion::Completion {
                time: None,
                category: "Tabs",
                name: format!("{}/{}", t.window, t.tab),
                description: if t.title.is_empty() {
                    t.url.clone()
                } else {
                    format!("{} — {}", t.title, t.url)
                },
            })
            .collect()
    })
}

/// `:tab-select`: `window/tab` from completion, or the first tab whose title
/// or URL contains every word of `target`.
fn select_tab(target: &str) {
    let found = OPEN_TABS.with(|o| {
        let tabs = o.borrow();
        let position = target.split_once('/').and_then(|(w, t)| {
            Some((
                w.trim().parse::<usize>().ok()?,
                t.trim().parse::<usize>().ok()?,
            ))
        });
        match position {
            Some(position) => tabs
                .iter()
                .find(|t| (t.window, t.tab) == position)
                .map(|t| (t.window, t.tab)),
            None => {
                let words: Vec<String> = target.split_whitespace().map(str::to_lowercase).collect();
                tabs.iter()
                    .find(|t| {
                        let hay = format!("{} {}", t.title, t.url).to_lowercase();
                        !words.is_empty() && words.iter().all(|w| hay.contains(w.as_str()))
                    })
                    .map(|t| (t.window, t.tab))
            }
        }
    });
    let Some((window, tab)) = found else {
        return shell::show_message(Level::Error, format!("No tab matches {target:?}"));
    };
    // Window numbers count open windows only, like the completion.
    let target_window = shell::with(|s| {
        let index = s
            .windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.window.is_some())
            .nth(window - 1)
            .map(|(i, _)| i)?;
        s.active = index;
        s.window.clone()
    })
    .flatten();
    if let Some(window) = target_window {
        window.activate();
    }
    switch_to(tab - 1, true);
    shell::refresh_ui();
}

/// The shell index of open window `number` (from 1, as in completion).
fn open_window_index(s: &shell::Shell, number: usize) -> Option<usize> {
    s.windows
        .iter()
        .enumerate()
        .filter(|(_, w)| w.window.is_some())
        .nth(number.checked_sub(1)?)
        .map(|(i, _)| i)
}

fn current_url() -> Option<(String, bool)> {
    shell::with(|s| Some((s.tabs.current()?.url.clone(), s.private))).flatten()
}

/// Tabs can't carry their back/forward history to another view, so these
/// reopen the page and close the original.
fn clone_tab(background: bool, window: bool) {
    let Some((url, private)) = current_url() else {
        return;
    };
    if window {
        crate::window::create(vec![url], Vec::new(), private);
    } else {
        open(&url, Position::Next, !background);
    }
}

/// Close the current tab after it moved; a window left empty closes.
fn close_moved(index: usize) {
    let last = shell::with(|s| s.tabs.len() <= 1).unwrap_or(false);
    if last {
        if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
            window.close();
        }
    } else {
        close(index);
    }
}

fn give_tab(window: Option<usize>) {
    let Some((url, private)) = current_url() else {
        return;
    };
    let Some((index, source)) = shell::with(|s| (s.tabs.current_index(), s.active)) else {
        return;
    };
    match window {
        None => {
            crate::window::create(vec![url], Vec::new(), private);
            // The new window may have become the active one already.
            shell::with(|s| s.active = source);
        }
        Some(number) => {
            let target = shell::with(|s| open_window_index(s, number)).flatten();
            let Some(target) = target.filter(|t| *t != source) else {
                return shell::show_message(
                    Level::Error,
                    format!("There's no other window {number}"),
                );
            };
            shell::with(|s| s.active = target);
            open(&url, Position::Last, true);
            if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
                window.activate();
            }
            shell::with(|s| s.active = source);
        }
    }
    close_moved(index);
    shell::refresh_ui();
}

fn take_tab(target: &str) {
    let position = target.split_once('/').and_then(|(w, t)| {
        Some((
            w.trim().parse::<usize>().ok()?,
            t.trim().parse::<usize>().ok()?,
        ))
    });
    let Some((window, tab)) = position else {
        return shell::show_message(Level::Error, "Usage: :tab-take <window/tab>, e.g. 2/1");
    };
    let found = shell::with(|s| {
        let source = open_window_index(s, window)?;
        let url = s.windows[source].tabs.get(tab.checked_sub(1)?)?.url.clone();
        (source != s.active).then_some((source, url, s.active))
    })
    .flatten();
    let Some((source, url, here)) = found else {
        return shell::show_message(
            Level::Error,
            format!("There's no tab {target} in another window"),
        );
    };
    open(&url, Position::Last, true);
    shell::with(|s| s.active = source);
    close_moved(tab - 1);
    shell::with(|s| s.active = here.min(s.windows.len() - 1));
    shell::refresh_ui();
}
