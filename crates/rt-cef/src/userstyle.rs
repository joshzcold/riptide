//! Custom CSS: `ui.css` in the config directory styles riptide's own bars and
//! overlay, and `content.user_stylesheets` styles web pages (per site too).
//! Both are read again when their files change.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cef::*;

use crate::shell;

thread_local! {
    /// Files read, by path, with the modification time they were read at.
    static FILES: RefCell<HashMap<PathBuf, (Option<SystemTime>, String)>> = RefCell::new(HashMap::new());
}

/// How often the files are checked for changes.
const CHECK_MS: i64 = 1000;

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A file's text, read again only when it has changed; empty if it's missing.
fn read(path: &Path) -> String {
    let mtime = modified(path);
    if let Some(text) = FILES.with(|f| {
        f.borrow()
            .get(path)
            .filter(|(seen, _)| *seen == mtime)
            .map(|(_, text)| text.clone())
    }) {
        return text;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    FILES.with(|f| {
        f.borrow_mut()
            .insert(path.to_path_buf(), (mtime, text.clone()))
    });
    text
}

fn config_dir() -> Option<PathBuf> {
    shell::with(|s| s.paths.config_dir.clone())
}

fn ui_css_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("ui.css"))
}

/// The text of `ui.css` in `config_dir`, for riptide's own pages. Takes
/// the folder because it's called while the shell is borrowed.
pub fn ui_css(config_dir: &Path) -> String {
    read(&config_dir.join("ui.css"))
}

/// A `content.user_stylesheets` entry as a path: `~/` is the home folder,
/// and a relative path is in the config directory.
fn resolve(entry: &str, config_dir: &Path) -> PathBuf {
    let path = crate::screenshot::expand_home(entry);
    if path.is_relative() {
        config_dir.join(path)
    } else {
        path
    }
}

/// Every stylesheet any site may use, for watching.
fn all_sheets() -> Vec<PathBuf> {
    let Some(config_dir) = config_dir() else {
        return Vec::new();
    };
    shell::with(|s| {
        let settings = s.engine.settings();
        let mut entries: Vec<String> = settings.list("content.user_stylesheets").to_vec();
        for (_, value) in settings.overrides("content.user_stylesheets") {
            if let rt_core::settings::Value::List(items) = value {
                entries.extend(items.iter().cloned());
            }
        }
        entries
    })
    .unwrap_or_default()
    .iter()
    .map(|e| resolve(e, &config_dir))
    .collect()
}

/// The CSS `content.user_stylesheets` gives pages at `url`.
fn css_for(url: &str) -> String {
    let Some(config_dir) = config_dir() else {
        return String::new();
    };
    let entries = shell::with(|s| {
        s.engine
            .settings()
            .list_for("content.user_stylesheets", url)
            .to_vec()
    })
    .unwrap_or_default();
    entries
        .iter()
        .map(|e| read(&resolve(e, &config_dir)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Put the user stylesheets into a page as it starts loading, and again
/// once it has (the first time, the document may not exist yet).
pub fn inject(frame: &Frame) {
    let url = CefString::from(&frame.url()).to_string();
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
        return;
    }
    let css = css_for(&url);
    let css = serde_json::to_string(&css).unwrap_or_default();
    let code = format!(
        "(() => {{ const css = {css}; let s = document.getElementById('__rt_user_css'); \
         if (!s) {{ if (!css) return; s = document.createElement('style'); s.id = '__rt_user_css'; }} \
         s.textContent = css; \
         const put = () => {{ if (!s.isConnected) document.documentElement.appendChild(s); }}; \
         if (document.documentElement) put(); \
         else new MutationObserver((_, o) => {{ if (document.documentElement) {{ o.disconnect(); put(); }} }}) \
           .observe(document, {{ childList: true }}); }})()"
    );
    shell::exec_js(frame, &code);
}

/// Start watching the files.
pub fn start() {
    let mut task = Check::new(RefCell::new(snapshot()));
    post_delayed_task(ThreadId::UI, Some(&mut task), CHECK_MS);
}

/// The files' modification times now: `ui.css` first, then the stylesheets.
fn snapshot() -> Vec<(PathBuf, Option<SystemTime>)> {
    ui_css_path()
        .into_iter()
        .chain(all_sheets())
        .map(|p| {
            let m = modified(&p);
            (p, m)
        })
        .collect()
}

wrap_task! {
    struct Check {
        seen: RefCell<Vec<(PathBuf, Option<SystemTime>)>>,
    }

    impl Task {
        fn execute(&self) {
            let now = snapshot();
            let before = std::mem::replace(&mut *self.seen.borrow_mut(), now.clone());
            if now != before {
                let ui_changed = now.first() != before.first();
                if ui_changed {
                    shell::refresh_ui();
                }
                if now.get(1..) != before.get(1..) {
                    restyle_tabs();
                }
            }
            let mut task = Check::new(RefCell::new(now));
            post_delayed_task(ThreadId::UI, Some(&mut task), CHECK_MS);
        }
    }
}

/// Apply the stylesheets again in every tab.
pub fn restyle_tabs() {
    let frames: Vec<Frame> = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter().filter_map(|t| t.browser()))
            .filter_map(|b| b.main_frame())
            .collect()
    })
    .unwrap_or_default();
    for frame in frames {
        inject(&frame);
    }
}
