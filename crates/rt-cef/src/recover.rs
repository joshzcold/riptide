//! `riptide://recover/` (`:recover`): the tabs of each crash kept under a
//! `_crashed-…` session, to reopen some or all of them, or forget a crash.
//! Its buttons come back as `reopen` and `forget` UI messages.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use rt_core::engine::Level;
use rt_storage::recovery::{CRASHED_PREFIX, crashed_when};

use crate::{shell, storage, tabs};

pub const URL: &str = "riptide://recover/";

const TEMPLATE: &str = include_str!("../ui/recover.html");

/// The sessions directory, for the page, which is served on CEF's IO thread.
static DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_dir(data_dir: &Path) {
    let _ = DIR.set(data_dir.join("sessions"));
}

/// The page, read from disk each time it's loaded.
pub fn page() -> Arc<[u8]> {
    let sessions = rt_storage::Sessions::new(DIR.get().map_or(Path::new(""), PathBuf::as_path));
    let mut names: Vec<String> = sessions
        .list()
        .into_iter()
        .filter(|n| n.starts_with(CRASHED_PREFIX))
        .collect();
    names.sort();
    let data: Vec<serde_json::Value> = names
        .into_iter()
        .rev()
        .filter_map(|name| {
            let session = sessions.load(&name).ok()?;
            let windows: Vec<serde_json::Value> = session
                .windows
                .iter()
                .map(|w| {
                    let tabs: Vec<_> = w
                        .tabs
                        .iter()
                        .map(|t| serde_json::json!({ "title": t.title, "url": t.url }))
                        .collect();
                    serde_json::json!({ "tabs": tabs })
                })
                .collect();
            let when = crashed_when(&name).unwrap_or_else(|| name.clone());
            Some(serde_json::json!({ "name": name, "when": when, "windows": windows }))
        })
        .collect();
    // `</` would end the inline <script> early.
    let json = serde_json::to_string(&data)
        .unwrap_or_else(|_| "[]".into())
        .replace("</", "<\\/");
    Arc::from(TEMPLATE.replace("/*RT_DATA*/null", &json).into_bytes())
}

/// The page's reopen button: open the chosen tabs of `session` in this window.
pub fn reopen(session: &str, picked: &[(usize, usize)]) {
    let saved = match storage::load_session(session) {
        Ok(saved) => saved,
        Err(e) => return shell::show_message(Level::Error, e),
    };
    let chosen: Vec<_> = picked
        .iter()
        .filter_map(|&(w, t)| saved.windows.get(w)?.tabs.get(t))
        .collect();
    for (i, tab) in chosen.iter().enumerate() {
        tabs::open_saved(tab, i == 0);
    }
    shell::show_message(
        Level::Info,
        format!(
            "Reopened {} tab{} from {session}",
            chosen.len(),
            if chosen.len() == 1 { "" } else { "s" }
        ),
    );
    shell::refresh_ui();
}

/// The page's forget button: delete `session`.
pub fn forget(session: &str) {
    let Some(dir) = DIR.get() else { return };
    match rt_storage::Sessions::new(dir).delete(session) {
        Ok(()) => shell::show_message(Level::Info, format!("Forgot {session}")),
        Err(e) => shell::show_message(Level::Error, e),
    }
}
