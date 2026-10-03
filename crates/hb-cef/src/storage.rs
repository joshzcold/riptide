//! History, quickmarks, bookmarks and sessions for the browser process.
//! Kept apart from the shell so the engine's completion source can read it
//! while the shell is borrowed.

use std::cell::{Cell, RefCell};
use std::time::{SystemTime, UNIX_EPOCH};

use cef::*;
use hb_config::Paths;
use hb_core::Command;
use hb_core::completion::{Completion, CompletionKind};
use hb_core::engine::Level;
use hb_storage::{Session, Storage, matches};

use crate::{shell, tabs};

/// The session `:session-save`, `:quit --save` and `auto_save.session` use.
pub const DEFAULT_SESSION: &str = "default";

thread_local! {
    static STORAGE: RefCell<Option<Storage>> = const { RefCell::new(None) };
    static HISTORY_LIMIT: Cell<usize> = const { Cell::new(100) };
}

/// Open storage; returns errors to report (the browser works without it).
pub fn open(paths: &Paths) -> Vec<String> {
    let (storage, errors) = Storage::open(&paths.config_dir, &paths.data_dir);
    STORAGE.with(|s| *s.borrow_mut() = Some(storage));
    errors
}

pub fn with<R>(f: impl FnOnce(&mut Storage) -> R) -> Option<R> {
    STORAGE.with(|s| s.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Mirror `completion.web_history.max_items`, which the source can't read itself.
pub fn set_history_limit(limit: i64) {
    HISTORY_LIMIT.with(|l| l.set(limit.max(0) as usize));
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn record_visit(url: &str, title: &str) {
    with(|s| {
        if let Some(history) = &s.history
            && let Err(e) = history.add_visit(url, title, now())
        {
            tracing::warn!(%e, "could not record history");
        }
    });
}

pub fn set_title(url: &str, title: &str) {
    with(|s| {
        if let Some(history) = &s.history {
            let _ = history.set_title(url, title);
        }
    });
}

fn item(category: &'static str, name: &str, description: &str) -> Completion {
    Completion {
        category,
        name: name.to_string(),
        description: description.to_string(),
    }
}

/// The engine's completion source.
pub fn complete(kind: CompletionKind, pattern: &str) -> Vec<Completion> {
    if kind == CompletionKind::Spelling {
        return crate::spell::completions(pattern);
    }
    if kind == CompletionKind::Tab {
        return crate::tabs::completions(pattern);
    }
    with(|s| {
        let mut items = Vec::new();
        if matches!(kind, CompletionKind::Url | CompletionKind::Quickmark) {
            for (name, url) in s
                .quickmarks
                .iter()
                .filter(|(n, u)| matches(pattern, &[n, u]))
            {
                // `:open` inserts the URL; `:quickmark-load` inserts the name.
                items.push(match kind {
                    CompletionKind::Url => item("Quickmarks", url, name),
                    _ => item("Quickmarks", name, url),
                });
            }
        }
        if matches!(kind, CompletionKind::Url | CompletionKind::Bookmark) {
            for (url, title) in s
                .bookmarks
                .iter()
                .filter(|(u, t)| matches(pattern, &[u, t]))
            {
                items.push(item("Bookmarks", url, title));
            }
        }
        if kind == CompletionKind::Url
            && let Some(history) = &s.history
        {
            let limit = HISTORY_LIMIT.with(Cell::get);
            if limit > 0 {
                match history.search(pattern, limit) {
                    Ok(entries) => {
                        items.extend(entries.iter().map(|e| item("History", &e.url, &e.title)))
                    }
                    Err(e) => tracing::warn!(%e, "history search failed"),
                }
            }
        }
        if kind == CompletionKind::Session {
            for name in s.sessions.list().iter().filter(|n| matches(pattern, &[n])) {
                items.push(item("Sessions", name, ""));
            }
        }
        items
    })
    .unwrap_or_default()
}

fn current_page() -> Option<(String, String)> {
    shell::with(|s| s.tabs.current().map(|t| (t.url.clone(), t.title.clone()))).flatten()
}

fn report(result: std::io::Result<String>) {
    match result {
        Ok(text) => shell::show_message(Level::Info, text),
        Err(e) => shell::show_message(Level::Error, e.to_string()),
    }
}

/// Save the open tabs as `name`.
pub fn save_session(name: &str) -> Result<(), String> {
    let session = shell::current_session();
    with(|s| s.sessions.save(name, &session))
        .unwrap_or_else(|| Err("storage is unavailable".into()))
}

pub fn load_session(name: &str) -> Result<Session, String> {
    with(|s| s.sessions.load(name)).unwrap_or_else(|| Err("storage is unavailable".into()))
}

/// Carry out storage commands; returns false for anything else.
pub fn run_command(command: &Command) -> bool {
    match command {
        Command::QuickmarkAdd { url, name } => {
            let result = with(|s| s.quickmarks.add(name, url)).unwrap_or(Ok(false));
            report(result.map(|replaced| {
                let verb = if replaced { "Updated" } else { "Added" };
                format!("{verb} quickmark {name}")
            }));
        }
        Command::QuickmarkLoad { target, name } => {
            match with(|s| s.quickmarks.get(name).map(String::from)).flatten() {
                Some(url) => shell::open(*target, false, Some(url)),
                None => {
                    shell::show_message(Level::Error, format!("Quickmark '{name}' doesn't exist"))
                }
            }
        }
        Command::QuickmarkDel { name } => {
            let name = match name {
                Some(name) => Some(name.clone()),
                None => current_page().and_then(|(url, _)| {
                    with(|s| s.quickmarks.name_for(&url).map(String::from)).flatten()
                }),
            };
            let Some(name) = name else {
                shell::show_message(Level::Error, "This page has no quickmark");
                return true;
            };
            match with(|s| s.quickmarks.remove(&name)) {
                Some(Ok(true)) => {
                    shell::show_message(Level::Info, format!("Deleted quickmark {name}"))
                }
                Some(Ok(false)) => {
                    shell::show_message(Level::Error, format!("Quickmark '{name}' doesn't exist"))
                }
                Some(Err(e)) => shell::show_message(Level::Error, e.to_string()),
                None => {}
            }
        }
        Command::BookmarkAdd { url, title } => {
            let page = current_page().unwrap_or_default();
            let url = url.clone().unwrap_or(page.0);
            let title = title.clone().unwrap_or(page.1);
            if url.is_empty() {
                shell::show_message(Level::Error, "Nothing to bookmark");
                return true;
            }
            let result = with(|s| s.bookmarks.add(&url, &title)).unwrap_or(Ok(false));
            match result {
                Ok(true) => shell::show_message(Level::Info, format!("Bookmarked {url}")),
                Ok(false) => {
                    shell::show_message(Level::Warning, format!("{url} is already bookmarked"))
                }
                Err(e) => shell::show_message(Level::Error, e.to_string()),
            }
        }
        Command::BookmarkLoad { target, url } => shell::open(*target, false, Some(url.clone())),
        Command::BookmarkDel { url } => {
            let Some(url) = url.clone().or_else(|| current_page().map(|p| p.0)) else {
                return true;
            };
            match with(|s| s.bookmarks.remove(&url)) {
                Some(Ok(true)) => {
                    shell::show_message(Level::Info, format!("Removed bookmark {url}"))
                }
                Some(Ok(false)) => {
                    shell::show_message(Level::Error, format!("{url} is not bookmarked"))
                }
                Some(Err(e)) => shell::show_message(Level::Error, e.to_string()),
                None => {}
            }
        }
        Command::SessionSave { name } => {
            let name = name.as_deref().unwrap_or(DEFAULT_SESSION);
            match save_session(name) {
                Ok(()) => shell::show_message(Level::Info, format!("Saved session {name}")),
                Err(e) => shell::show_message(Level::Error, e),
            }
        }
        Command::SessionLoad { name } => match load_session(name) {
            Ok(session) => tabs::restore(&session),
            Err(e) => shell::show_message(Level::Error, e),
        },
        Command::SessionDelete { name } => {
            match with(|s| s.sessions.delete(name))
                .unwrap_or_else(|| Err("storage is unavailable".into()))
            {
                Ok(()) => shell::show_message(Level::Info, format!("Deleted session {name}")),
                Err(e) => shell::show_message(Level::Error, e),
            }
        }
        Command::HistoryClear { force: false } => shell::show_message(
            Level::Warning,
            "This deletes all browsing history. Run :history-clear --force to confirm.",
        ),
        Command::HistoryClear { force: true } => {
            let result = with(|s| s.history.as_ref().map(|h| h.clear()));
            match result.flatten() {
                Some(Ok(())) => shell::show_message(Level::Info, "History cleared"),
                Some(Err(e)) => {
                    shell::show_message(Level::Error, format!("Could not clear history: {e}"))
                }
                None => shell::show_message(Level::Error, "History is unavailable"),
            }
        }
        _ => return false,
    }
    true
}

/// Saved every `auto_save.interval` ms and deleted on a clean exit, so it is
/// only there at startup after a crash.
pub const AUTOSAVE_SESSION: &str = "_autosave";

/// Start the crash-recovery saves.
pub fn start_autosave() {
    let mut task = Autosave::new();
    let interval = shell::with(|s| s.engine.settings().int("auto_save.interval")).unwrap_or(0);
    post_delayed_task(ThreadId::UI, Some(&mut task), interval.max(1000));
}

/// A crash left tabs behind: the autosave from the last run, if any.
pub fn crashed_session() -> Option<Session> {
    let exists = with(|s| s.sessions.exists(AUTOSAVE_SESSION)).unwrap_or(false);
    exists
        .then(|| load_session(AUTOSAVE_SESSION).ok())
        .flatten()
}

wrap_task! {
    struct Autosave {}

    impl Task {
        fn execute(&self) {
            let interval = shell::with(|s| s.engine.settings().int("auto_save.interval")).unwrap_or(0);
            if interval > 0 {
                let session = shell::current_session();
                if !session.windows.is_empty()
                    && let Some(Err(e)) = with(|s| s.sessions.save(AUTOSAVE_SESSION, &session))
                {
                    tracing::warn!("crash-recovery save failed: {e}");
                }
            }
            // Check again later even when off, in case it is turned on.
            let mut task = Autosave::new();
            post_delayed_task(ThreadId::UI, Some(&mut task), interval.max(1000));
        }
    }
}

/// The latest `limit` history entries as JSON objects, newest first.
pub fn recent_history(limit: usize) -> Vec<serde_json::Value> {
    with(|s| {
        s.history
            .as_ref()
            .and_then(|h| h.search("", limit).ok())
            .unwrap_or_default()
    })
    .unwrap_or_default()
    .into_iter()
    .map(|e| serde_json::json!({ "url": e.url, "title": e.title, "last_visit": e.last_visit }))
    .collect()
}
