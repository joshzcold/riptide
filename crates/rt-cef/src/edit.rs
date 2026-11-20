//! `riptide://edit/<path>` (`:edit`, and `file://` text files with
//! `editor.file_urls`): CodeMirror over a local file. The page is served on
//! the IO thread with the file's text; saves come back as `save` UI messages.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::UNIX_EPOCH;

use cef::*;
use rt_core::command::{Command, OpenTarget};
use rt_core::edit;
use rt_core::engine::Level;
use serde_json::json;

use crate::shell;

const TEMPLATE: &str = include_str!("../ui/editor.html");
const BUNDLE: &str = include_str!("../ui/editor.bundle.js");

/// The page around its data: built once, since the bundle is large.
static PARTS: LazyLock<(String, String)> = LazyLock::new(|| {
    let page = TEMPLATE.replacen("/*RT_BUNDLE*/", BUNDLE, 1);
    let (before, after) = page
        .split_once("/*RT_DATA*/null")
        .expect("editor.html has a data placeholder");
    (before.to_string(), after.to_string())
});

/// `editor.vim`, kept up to date by the UI thread because pages are served on the IO thread.
static VIM: AtomicBool = AtomicBool::new(true);

pub fn set_vim(on: bool) {
    VIM.store(on, Ordering::Relaxed);
}

/// What tells a save that the file changed since the page read it.
fn version(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!(
        "{}.{:09}-{}",
        modified.as_secs(),
        modified.subsec_nanos(),
        meta.len()
    ))
}

/// The file's text, or why the page can't show it.
fn read(path: &Path) -> Result<Option<String>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
            return match parent {
                Some(dir) if !dir.is_dir() => Err(format!("{} doesn't exist", dir.display())),
                _ => Ok(None),
            };
        }
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    if meta.is_dir() {
        return Err(format!("{} is a folder", path.display()));
    }
    if meta.len() > edit::MAX_BYTES {
        return Err(format!(
            "{} is too large to edit ({} MB; the limit is {} MB)",
            path.display(),
            meta.len() / 1024 / 1024,
            edit::MAX_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} isn't UTF-8 text", path.display()))
}

/// The page for `riptide://edit<path>`.
pub fn page(url_path: &str) -> Arc<[u8]> {
    let url = format!("riptide://edit{url_path}");
    let data = match edit::path_from_url(&url) {
        Some(path) => {
            let file = Path::new(&path);
            let (text, error) = match read(file) {
                Ok(text) => (text, None),
                Err(e) => (None, Some(e)),
            };
            json!({
                "path": path,
                "exists": text.is_some(),
                "text": text.unwrap_or_default(),
                "version": version(file),
                "error": error,
                "vim": VIM.load(Ordering::Relaxed),
            })
        }
        None => json!({ "path": url, "text": "", "error": "Not a local file path" }),
    };
    // `</` would end the inline <script> early.
    let data = data.to_string().replace("</", "<\\/");
    let (before, after) = &*PARTS;
    Arc::from(format!("{before}{data}{after}").into_bytes())
}

/// `editor.file_urls`: the editor page for a `file://` URL that should open there.
pub fn redirect(url: &str) -> Option<String> {
    if !url.starts_with("file:") {
        return None;
    }
    if !shell::with(|s| s.engine.settings().bool("editor.file_urls")).unwrap_or(false) {
        return None;
    }
    let path = edit::path_from_url(url)?;
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > edit::MAX_BYTES {
        return None;
    }
    let mut head = vec![0; edit::SNIFF_BYTES];
    let read = std::io::Read::read(&mut std::fs::File::open(&path).ok()?, &mut head).ok()?;
    edit::opens_in_editor(&path, &head[..read]).then(|| edit::edit_url(url))?
}

// Loads the editor page in place of a `file://` navigation, which can't load
// it itself: only riptide pages may navigate to riptide:// pages.
wrap_task! {
    pub struct Load {
        frame: Frame,
        url: String,
    }

    impl Task {
        fn execute(&self) {
            self.frame.load_url(Some(&CefString::from(self.url.as_str())));
        }
    }
}

pub fn run_command(command: &Command) -> bool {
    let Command::Edit { path } = command else {
        return false;
    };
    match path {
        Some(path) => match edit::url_for_path(path, std::env::home_dir().as_deref()) {
            Some(url) => shell::open(OpenTarget::Tab, true, Some(url)),
            None => shell::show_message(
                Level::Error,
                format!("{path}: give an absolute path or one starting with ~/"),
            ),
        },
        None => {
            let frame = shell::with(|s| s.current_browser())
                .flatten()
                .and_then(|b| b.main_frame());
            let edit = frame
                .as_ref()
                .and_then(|f| edit::edit_url(&CefString::from(&f.url()).to_string()));
            match (frame, edit) {
                (Some(frame), Some(edit)) => frame.load_url(Some(&CefString::from(edit.as_str()))),
                _ => {
                    shell::show_message(Level::Error, "This isn't a local file; give :edit a path")
                }
            }
        }
    }
    true
}

/// The current tab's main frame, if it's the editor page for `path`. Saving
/// is input, so the page that sent it is the current tab.
fn sender(path: &str) -> Option<Frame> {
    let browser = shell::with(|s| s.current_browser()).flatten()?;
    let frame = browser.main_frame()?;
    let url = CefString::from(&frame.url()).to_string();
    (edit::path_from_url(&url).as_deref() == Some(path)).then_some(frame)
}

/// A `save` message from an editor page.
pub fn save(path: &str, text: &str, expected: Option<&str>, force: bool) {
    let frame = sender(path);
    let reply = |code: String| {
        if let Some(frame) = &frame {
            shell::exec_js(frame, &code);
        }
    };
    let quoted = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let file = Path::new(path);
    if !force && version(file).as_deref() != expected {
        return reply(format!(
            "rtEditFailed({})",
            quoted("The file changed on disk since it was opened; :w! overwrites it")
        ));
    }
    if let Err(e) = std::fs::write(file, text) {
        return reply(format!(
            "rtEditFailed({})",
            quoted(&format!("Can't write {path}: {e}"))
        ));
    }
    tracing::info!(path, bytes = text.len(), "editor page wrote a file");
    reply(format!(
        "rtEditSaved({}, {})",
        quoted(&version(file).unwrap_or_default()),
        text.len()
    ));
}

/// A `quit` message: close the editor's tab.
pub fn quit() {
    if let Some(index) = shell::with(|s| s.tabs.current_index()) {
        crate::tabs::close_unless_pinned(index, false);
        shell::refresh_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_can_be_inlined() {
        // Either would end or confuse the inline <script> the bundle goes in.
        assert!(!BUNDLE.contains("</script"));
        assert!(!BUNDLE.contains("<script"));
        assert!(PARTS.0.contains("RtEditor"));
    }

    #[test]
    fn missing_files_are_new_and_folders_are_refused() {
        let dir = std::env::temp_dir().join(format!("rt-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read(&dir.join("new.txt")), Ok(None));
        assert!(read(&dir.join("no/such/dir/new.txt")).is_err());
        assert!(read(&dir).is_err());
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        assert_eq!(read(&dir.join("a.txt")), Ok(Some("hi".into())));
        std::fs::write(dir.join("b.bin"), [0xff, 0xfe, 0]).unwrap();
        assert!(read(&dir.join("b.bin")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
