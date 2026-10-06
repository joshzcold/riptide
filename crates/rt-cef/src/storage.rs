//! History, quickmarks, bookmarks and sessions for the browser process.
//! Kept apart from the shell so the engine's completion source can read it
//! while the shell is borrowed.

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use cef::*;
use rt_config::Paths;
use rt_core::Command;
use rt_core::completion::{Completion, CompletionKind};
use rt_core::engine::Level;
use rt_storage::{Session, Storage, matches};

use crate::{shell, tabs};

/// The session `:session-save`, `:quit --save` and `auto_save.session` use.
pub const DEFAULT_SESSION: &str = "default";

thread_local! {
    static STORAGE: RefCell<Option<Storage>> = const { RefCell::new(None) };
    /// Settings the completion source needs; it can't read the engine's while
    /// the engine is asking it for completions.
    static COMPLETION: RefCell<CompletionSettings> = RefCell::new(CompletionSettings::default());
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

/// `riptide://bookmarks/`, rebuilt each time `:bookmark-list` runs.
static BOOKMARKS_PAGE: std::sync::RwLock<Option<std::sync::Arc<[u8]>>> =
    std::sync::RwLock::new(None);

pub fn bookmarks_page() -> std::sync::Arc<[u8]> {
    BOOKMARKS_PAGE
        .read()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(|| std::sync::Arc::from(&b""[..]))
}

fn publish_bookmarks() {
    let escape = rt_core::html::escape;
    let row = |name: &str, url: &str| {
        format!(
            "<li><a href=\"{}\">{}</a> <span class=url>{}</span></li>",
            escape(url),
            escape(name),
            escape(url)
        )
    };
    let (quickmarks, bookmarks) = with(|s| {
        let q: String = s.quickmarks.iter().map(|(n, u)| row(n, u)).collect();
        let b: String = s
            .bookmarks
            .iter()
            .map(|(u, t)| row(if t.is_empty() { u } else { t }, u))
            .collect();
        (q, b)
    })
    .unwrap_or_default();
    let section = |title: &str, items: String| {
        if items.is_empty() {
            format!("<h2>{title}</h2><p class=none>None yet.</p>")
        } else {
            format!("<h2>{title}</h2><ul>{items}</ul>")
        }
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>Bookmarks</title><style>\
         :root {{ color-scheme: light dark; }} body {{ margin: 1.5rem; font: 14px/1.6 system-ui, sans-serif; }} \
         ul {{ list-style: none; padding: 0; }} .url, .none {{ color: gray; font-size: .9em; }}</style></head>\
         <body><h1>Bookmarks</h1>{}{}</body></html>",
        section("Quickmarks", quickmarks),
        section("Bookmarks", bookmarks)
    );
    if let Ok(mut page) = BOOKMARKS_PAGE.write() {
        *page = Some(std::sync::Arc::from(html.into_bytes()));
    }
}

/// `:completion-item-del`: delete what a completion item stands for.
pub fn delete_completion(item: &rt_core::completion::Completion) {
    let command = match item.category {
        "History" => {
            match with(|s| s.history.as_ref().map(|h| h.delete_url(&item.name))).flatten() {
                Some(Ok(_)) => {
                    shell::show_message(Level::Info, format!("Deleted {} from history", item.name))
                }
                Some(Err(e)) => {
                    shell::show_message(Level::Error, format!("Could not delete from history: {e}"))
                }
                None => {}
            }
            return;
        }
        "Tabs" => return crate::tabs::close_label(&item.name),
        // `:open` lists quickmarks by URL with the name as the description.
        "Quickmarks" => {
            let is_name =
                with(|s| s.quickmarks.iter().any(|(name, _)| *name == item.name)).unwrap_or(false);
            let name = if is_name {
                &item.name
            } else {
                &item.description
            };
            Command::QuickmarkDel {
                name: Some(name.clone()),
            }
        }
        "Bookmarks" => Command::BookmarkDel {
            url: Some(item.name.clone()),
        },
        "Sessions" => Command::SessionDelete {
            name: item.name.clone(),
        },
        _ => return,
    };
    run_command(&command);
}

#[derive(Default)]
struct CompletionSettings {
    history_limit: usize,
    categories: Vec<String>,
    exclude: Vec<String>,
    engines: std::collections::BTreeMap<String, String>,
}

/// Mirror the settings the completion source reads.
pub fn sync_settings(settings: &rt_core::settings::Settings) {
    COMPLETION.with(|c| {
        *c.borrow_mut() = CompletionSettings {
            history_limit: settings.int("completion.web_history.max_items").max(0) as usize,
            categories: settings.list("completion.open_categories").to_vec(),
            exclude: settings.list("completion.web_history.exclude").to_vec(),
            engines: settings
                .map("url.searchengines")
                .cloned()
                .unwrap_or_default(),
        }
    });
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
        time: None,
        detail: None,
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
    if matches!(kind, CompletionKind::Tab | CompletionKind::OtherTab) {
        return crate::tabs::completions(pattern, kind == CompletionKind::OtherTab);
    }
    if kind == CompletionKind::Url {
        let categories = COMPLETION.with(|c| c.borrow().categories.clone());
        return categories
            .iter()
            .flat_map(|c| open_category(c, pattern))
            .collect();
    }
    with(|s| {
        let mut items = Vec::new();
        if kind == CompletionKind::Quickmark {
            for (name, url) in s
                .quickmarks
                .iter()
                .filter(|(n, u)| matches(pattern, &[n, u]))
            {
                items.push(item("Quickmarks", name, url));
            }
        }
        if kind == CompletionKind::Bookmark {
            for (url, title) in s
                .bookmarks
                .iter()
                .filter(|(u, t)| matches(pattern, &[u, t]))
            {
                items.push(item("Bookmarks", url, title));
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

/// One `completion.open_categories` entry for `:open`.
fn open_category(category: &str, pattern: &str) -> Vec<Completion> {
    match category {
        "searchengines" => {
            let engines = COMPLETION.with(|c| c.borrow().engines.clone());
            let word = pattern.split_whitespace().next().unwrap_or("");
            // Only while the first word could still be an engine's name.
            if pattern.contains(char::is_whitespace) {
                return Vec::new();
            }
            engines
                .iter()
                .filter(|(name, _)| *name != "DEFAULT" && name.starts_with(word))
                .map(|(name, url)| item("Search engines", name, url))
                .collect()
        }
        "quickmarks" => with(|s| {
            s.quickmarks
                .iter()
                .filter(|(n, u)| matches(pattern, &[n, u]))
                .map(|(name, url)| item("Quickmarks", url, name))
                .collect()
        })
        .unwrap_or_default(),
        "bookmarks" => with(|s| {
            s.bookmarks
                .iter()
                .filter(|(u, t)| matches(pattern, &[u, t]))
                .map(|(url, title)| item("Bookmarks", url, title))
                .collect()
        })
        .unwrap_or_default(),
        "history" => {
            let (limit, exclude) = COMPLETION.with(|c| {
                let c = c.borrow();
                (c.history_limit, c.exclude.clone())
            });
            if limit == 0 {
                return Vec::new();
            }
            with(
                |s| match s.history.as_ref().map(|h| h.search(pattern, limit)) {
                    Some(Ok(entries)) => entries
                        .iter()
                        .filter(|e| !exclude.iter().any(|glob| rt_core::url::glob(glob, &e.url)))
                        .map(|e| Completion {
                            time: Some(e.last_visit),
                            ..item("History", &e.url, &e.title)
                        })
                        .collect(),
                    Some(Err(e)) => {
                        tracing::warn!(%e, "history search failed");
                        Vec::new()
                    }
                    None => Vec::new(),
                },
            )
            .unwrap_or_default()
        }
        "filesystem" => files(pattern),
        _ => Vec::new(),
    }
}

/// Files and folders for a pattern that looks like a path (`/…`, `~/…`, `file://…`).
fn files(pattern: &str) -> Vec<Completion> {
    let path = pattern.strip_prefix("file://").unwrap_or(pattern);
    if !(path.starts_with('/') || path.starts_with("~/")) {
        return Vec::new();
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let expanded = match (path.strip_prefix("~/"), &home) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    };
    let (dir, prefix) = match expanded.rfind('/') {
        Some(i) => (&expanded[..=i], &expanded[i + 1..]),
        None => return Vec::new(),
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<(String, bool)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            (name.starts_with(prefix) && !name.starts_with('.')).then_some((name, is_dir))
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .take(50)
        .map(|(name, is_dir)| {
            let full = format!("{dir}{name}{}", if is_dir { "/" } else { "" });
            item("Filesystem", &full, if is_dir { "folder" } else { "" })
        })
        .collect()
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

thread_local! {
    /// The session last loaded with `:session-load`, for `session.default_name`.
    static LOADED_SESSION: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The session to save to and restore from when none is named:
/// `session.default_name`, else the last one loaded, else `default`.
pub fn default_session() -> String {
    let configured = shell::with(|s| s.engine.settings().str("session.default_name").to_string())
        .unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    LOADED_SESSION
        .with(|l| l.borrow().clone())
        .unwrap_or_else(|| DEFAULT_SESSION.to_string())
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

/// `:save`: write each of `what` (everything when empty) now.
fn save(what: &[String]) {
    let all = what.is_empty();
    let wants = |name: &str| all || what.iter().any(|w| w == name);
    let mut errors = Vec::new();
    if wants("config")
        && let Some(Err(e)) = shell::with(|s| s.autoconfig.as_ref().map(|a| a.save())).flatten()
    {
        errors.push(format!("config: {e}"));
    }
    if wants("quickmarks")
        && let Some(Err(e)) = with(|s| s.quickmarks.save())
    {
        errors.push(format!("quickmarks: {e}"));
    }
    if wants("bookmarks")
        && let Some(Err(e)) = with(|s| s.bookmarks.save())
    {
        errors.push(format!("bookmarks: {e}"));
    }
    if wants("cookies")
        && let Some(manager) = cookie_manager_get_global_manager(None)
    {
        manager.flush_store(None);
    }
    if wants("session")
        && let Err(e) = save_session(&default_session())
    {
        errors.push(format!("session: {e}"));
    }
    if errors.is_empty() {
        let saved = if all {
            "everything".to_string()
        } else {
            what.join(", ")
        };
        shell::show_message(Level::Info, format!("Saved {saved}"));
    } else {
        shell::show_message(
            Level::Error,
            format!("Could not save {}", errors.join("; ")),
        );
    }
}

/// Carry out storage commands; returns false for anything else.
pub fn run_command(command: &Command) -> bool {
    match command {
        Command::QuickmarkSave => match with(|s| s.quickmarks.save()) {
            Some(Ok(())) => shell::show_message(Level::Info, "Saved the quickmarks"),
            Some(Err(e)) => {
                shell::show_message(Level::Error, format!("Can't save the quickmarks: {e}"))
            }
            None => {}
        },
        Command::MarksReload => {
            let Some(config_dir) = shell::with(|s| s.paths.config_dir.clone()) else {
                return true;
            };
            match with(|s| s.reload_marks(&config_dir))
                .unwrap_or_default()
                .first()
            {
                None => shell::show_message(Level::Info, "Read the quickmarks and bookmarks again"),
                Some(e) => shell::show_message(Level::Error, e.clone()),
            }
        }
        Command::BookmarkList { tab } => {
            publish_bookmarks();
            let target = if *tab {
                rt_core::command::OpenTarget::Tab
            } else {
                rt_core::command::OpenTarget::Current
            };
            shell::open(target, true, Some("riptide://bookmarks/".to_string()));
        }
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
            let name = name.clone().unwrap_or_else(default_session);
            match save_session(&name) {
                Ok(()) => shell::show_message(Level::Info, format!("Saved session {name}")),
                Err(e) => shell::show_message(Level::Error, e),
            }
        }
        Command::SessionLoad { name } => match load_session(name) {
            Ok(session) => {
                // Internal sessions (_autosave, _restart) don't become the default.
                if !name.starts_with('_') {
                    LOADED_SESSION.with(|l| *l.borrow_mut() = Some(name.clone()));
                }
                tabs::restore(&session);
            }
            Err(e) => shell::show_message(Level::Error, e),
        },
        Command::Save { what } => save(what),
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
        Command::HistoryImport { path } => {
            let path = match path {
                Some(path) => std::path::PathBuf::from(path),
                None => match qutebrowser_history() {
                    Some(path) => path,
                    None => {
                        shell::show_message(
                            Level::Error,
                            "Where is qutebrowser's history? :history-import <path>",
                        );
                        return true;
                    }
                },
            };
            let result = with(|s| s.history.as_mut().map(|h| h.import_qutebrowser(&path)));
            match result.flatten() {
                Some(Ok(n)) => shell::show_message(
                    Level::Info,
                    format!("Imported {n} visits from {}", path.display()),
                ),
                Some(Err(e)) => shell::show_message(
                    Level::Error,
                    format!("Could not import {}: {e}", path.display()),
                ),
                None => shell::show_message(Level::Error, "History is unavailable"),
            }
        }
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
pub const AUTOSAVE_SESSION: &str = rt_storage::recovery::AUTOSAVE;
/// Restored tabs that last this long without a crash are no longer suspected
/// of causing one.
const RECOVERY_PROBATION_MS: i64 = 60_000;

/// Start the crash-recovery saves.
pub fn start_autosave() {
    let mut task = Autosave::new();
    let interval = shell::with(|s| s.engine.settings().int("auto_save.interval")).unwrap_or(0);
    post_delayed_task(ThreadId::UI, Some(&mut task), interval.max(1000));
}

/// A crash left tabs behind: the last run's autosave, moved aside to its
/// `_crashed-…` name, and its tabs.
pub fn take_crashed() -> Option<(String, Session)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = match with(|s| s.sessions.take_crashed(now))? {
        Ok(name) => name?,
        Err(e) => {
            tracing::warn!("can't keep the crashed session: {e}");
            return None;
        }
    };
    let session = load_session(&name).ok()?;
    Some((name, session))
}

/// Whether the last start reopened crashed tabs less than
/// [`RECOVERY_PROBATION_MS`] before it crashed again (or exited).
pub fn was_recovering() -> bool {
    with(|s| s.sessions.was_recovering()).unwrap_or(false)
}

pub fn set_recovering(on: bool) {
    with(|s| s.sessions.set_recovering(on));
}

/// Mark the reopened crashed tabs as suspects until they have run for a
/// while; a clean exit clears the mark too (see `rt_cef::run`).
pub fn start_recovery_probation() {
    set_recovering(true);
    let mut task = EndProbation::new();
    post_delayed_task(ThreadId::UI, Some(&mut task), RECOVERY_PROBATION_MS);
}

wrap_task! {
    struct EndProbation {}

    impl Task {
        fn execute(&self) {
            set_recovering(false);
        }
    }
}

wrap_task! {
    struct Autosave {}

    impl Task {
        fn execute(&self) {
            let interval = shell::with(|s| s.engine.settings().int("auto_save.interval")).unwrap_or(0);
            if interval > 0 {
                crate::history::read_scroll_positions();
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

/// Where qutebrowser keeps `history.sqlite` on this platform.
fn qutebrowser_history() -> Option<std::path::PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
    };
    let dir = if cfg!(windows) {
        env("APPDATA")?.join("qutebrowser").join("data")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/qutebrowser")
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))?
            .join("qutebrowser")
    };
    Some(dir.join("history.sqlite"))
}
