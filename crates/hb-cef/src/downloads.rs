//! Downloads: ask where to save (`downloads.location.*`), track progress for
//! the status bar, and cancel or open them by number.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cef::*;
use hb_config::downloads::{current_system_dir, expand_home, sanitize_name, unique_path};
use hb_core::Command;
use hb_core::engine::Level;
use hb_core::prompt::{PromptAnswer, PromptKind, Remember};

use crate::prompts::{self, Scope};
use crate::shell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Done,
    Cancelled,
    Failed,
}

struct Download {
    /// CEF's id for the item.
    id: u32,
    path: PathBuf,
    percent: Option<i32>,
    state: State,
    callback: Option<DownloadItemCallback>,
}

thread_local! {
    static DOWNLOADS: RefCell<Vec<Download>> = const { RefCell::new(Vec::new()) };
}

fn home() -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key).map(PathBuf::from)
}

pub(crate) fn download_dir() -> PathBuf {
    let configured = shell::with(|s| {
        s.engine
            .settings()
            .str("downloads.location.directory")
            .to_string()
    })
    .unwrap_or_default();
    if !configured.is_empty() {
        return expand_home(&configured, home().as_deref());
    }
    current_system_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Status bar summary, e.g. `↓2 41%`.
pub fn summary() -> String {
    DOWNLOADS.with(|d| {
        let d = d.borrow();
        let running: Vec<&Download> = d.iter().filter(|d| d.state == State::Running).collect();
        if running.is_empty() {
            return String::new();
        }
        let known: Vec<i32> = running.iter().filter_map(|d| d.percent).collect();
        match known.is_empty() {
            true => format!("↓{}", running.len()),
            false => format!(
                "↓{} {}%",
                running.len(),
                known.iter().sum::<i32>() / known.len() as i32
            ),
        }
    })
}

/// Start the download at `path`, confirming before overwriting a file.
fn save_to(path: PathBuf, id: u32, callback: BeforeDownloadCallback, browser: Option<i32>) {
    let begin = move |path: PathBuf| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
        DOWNLOADS.with(|d| {
            d.borrow_mut().push(Download {
                id,
                path,
                percent: None,
                state: State::Running,
                callback: None,
            })
        });
        publish();
        shell::refresh_ui();
    };
    if path.exists() {
        let message = format!("{} already exists. Overwrite it?", path.display());
        prompts::ask(
            browser,
            Scope::Other,
            "Download",
            message,
            PromptKind::YesNo {
                default: false,
                remember: Remember::Never,
            },
            move |answer| {
                if matches!(answer, PromptAnswer::Yes { .. }) {
                    begin(path);
                }
            },
        );
    } else {
        begin(path);
    }
}

wrap_download_handler! {
    pub struct HbDownloadHandler {}

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(item), Some(callback)) = (download_item, callback.map(|c| c.clone())) else { return 0 };
            let id = item.id();
            let browser = browser.map(|b| b.identifier());
            let name = sanitize_name(&suggested_name.map(CefString::to_string).unwrap_or_default());
            let dir = download_dir();
            let suggestion = unique_path(&dir, &name, &|p: &Path| p.exists());
            let ask = shell::with(|s| s.engine.settings().bool("downloads.location.prompt")).unwrap_or(true);
            if !ask {
                save_to(suggestion, id, callback, browser);
                return 1;
            }
            let default = suggestion.to_string_lossy().into_owned();
            prompts::ask(browser, Scope::Other, "Save file to", name.clone(), PromptKind::Text { default, masked: false, path: true }, move |answer| {
                let PromptAnswer::Text(text) = answer else { return };
                let mut path = expand_home(text.trim(), home().as_deref());
                if path.is_dir() {
                    path = path.join(&name);
                }
                save_to(path, id, callback, browser);
            });
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else { return };
            let id = item.id();
            let state = if item.is_complete() != 0 {
                State::Done
            } else if item.is_canceled() != 0 {
                State::Cancelled
            } else if item.is_interrupted() != 0 {
                State::Failed
            } else {
                State::Running
            };
            let percent = Some(item.percent_complete()).filter(|p| *p >= 0);
            let finished = DOWNLOADS.with(|d| {
                let mut d = d.borrow_mut();
                let download = d.iter_mut().find(|d| d.id == id)?;
                let changed = download.state != state;
                download.state = state;
                download.percent = percent;
                if state == State::Running {
                    download.callback = callback.map(|c| c.clone());
                } else {
                    download.callback = None;
                }
                changed.then(|| (state, download.path.clone()))
            });
            match finished {
                Some((State::Done, path)) => shell::show_message(Level::Info, format!("Download finished: {}", path.display())),
                Some((State::Cancelled, path)) => shell::show_message(Level::Warning, format!("Download cancelled: {}", path.display())),
                Some((State::Failed, path)) => shell::show_message(Level::Error, format!("Download failed: {}", path.display())),
                _ => {}
            }
            publish();
            shell::refresh_ui();
        }
    }
}

/// Downloads are numbered from 1 in the order they started, like qutebrowser.
fn pick(count: Option<u32>, state: State) -> Result<usize, String> {
    DOWNLOADS.with(|d| {
        let d = d.borrow();
        match count {
            Some(n) => {
                let index = (n as usize)
                    .checked_sub(1)
                    .filter(|&i| i < d.len())
                    .ok_or(format!("There's no download {n}"))?;
                if d[index].state != state {
                    return Err(format!(
                        "Download {n} is not {}",
                        if state == State::Running {
                            "running"
                        } else {
                            "finished"
                        }
                    ));
                }
                Ok(index)
            }
            None => d
                .iter()
                .rposition(|d| d.state == state)
                .ok_or_else(|| match state {
                    State::Running => "No running downloads".to_string(),
                    _ => "No finished downloads".to_string(),
                }),
        }
    })
}

/// Open a file with the desktop's default application.
fn open_with_system(path: &Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(path).spawn().map(|_| ())
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::Download { url } => {
            let target = shell::with(|s| {
                let url = url
                    .as_ref()
                    .map(|u| s.fuzzy_url(u))
                    .or_else(|| s.tabs.current().map(|t| t.url.clone()));
                Some((url?, s.current_browser()?.host()?))
            });
            match target.flatten() {
                Some((url, host)) => host.start_download(Some(&CefString::from(url.as_str()))),
                None => shell::show_message(Level::Error, "Nothing to download"),
            }
        }
        Command::DownloadCancel => match pick(count, State::Running) {
            Ok(index) => {
                let callback = DOWNLOADS.with(|d| d.borrow()[index].callback.clone());
                match callback {
                    Some(callback) => callback.cancel(),
                    None => {
                        shell::show_message(Level::Error, "That download can't be cancelled yet")
                    }
                }
            }
            Err(e) => shell::show_message(Level::Error, e),
        },
        Command::DownloadOpen => match pick(count, State::Done) {
            Ok(index) => {
                let path = DOWNLOADS.with(|d| d.borrow()[index].path.clone());
                if let Err(e) = open_with_system(&path) {
                    shell::show_message(
                        Level::Error,
                        format!("Could not open {}: {e}", path.display()),
                    );
                }
            }
            Err(e) => shell::show_message(Level::Error, e),
        },
        Command::DownloadClear => {
            DOWNLOADS.with(|d| d.borrow_mut().retain(|d| d.state == State::Running));
            publish();
        }
        Command::Downloads => {
            publish();
            shell::open(
                hb_core::command::OpenTarget::Tab,
                true,
                Some("hb://downloads/".to_string()),
            );
        }
        _ => return false,
    }
    shell::refresh_ui();
    true
}

/// `;d` and similar: download a URL from the current page.
pub fn start(url: &str) {
    if let Some(host) = shell::with(|s| s.current_browser()?.host()).flatten() {
        host.start_download(Some(&CefString::from(url)));
    }
}

/// `hb://downloads/`, rebuilt on the UI thread whenever a download changes
/// and served from the IO thread.
static PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn page() -> Arc<[u8]> {
    PAGE.read().ok().and_then(|p| p.clone()).unwrap_or_else(|| {
        publish();
        PAGE.read()
            .ok()
            .and_then(|p| p.clone())
            .unwrap_or_else(|| Arc::from(&b""[..]))
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn publish() {
    let (rows, running) = DOWNLOADS.with(|d| {
        let d = d.borrow();
        let rows: String = d
            .iter()
            .enumerate()
            .map(|(i, dl)| {
                let name = dl.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = dl.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                let state = match (dl.state, dl.percent) {
                    (State::Running, Some(p)) => format!("{p}%"),
                    (State::Running, None) => "downloading".into(),
                    (State::Done, _) => "done".into(),
                    (State::Cancelled, _) => "cancelled".into(),
                    (State::Failed, _) => "failed".into(),
                };
                format!(
                    "<tr><td class=n>{}</td><td>{}<div class=dir>{}</div></td><td class=state>{}</td></tr>",
                    i + 1,
                    escape(&name),
                    escape(&dir),
                    escape(&state)
                )
            })
            .collect();
        (rows, d.iter().any(|dl| dl.state == State::Running))
    });
    let body = if rows.is_empty() {
        "<p class=empty>No downloads this session.</p>".to_string()
    } else {
        format!("<table>{rows}</table>")
    };
    // While something downloads, the page reloads itself to show progress.
    let refresh = if running {
        "<meta http-equiv=refresh content=1>"
    } else {
        ""
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>Downloads</title>{refresh}<style>{STYLE}</style></head>\
         <body><main><h1>Downloads</h1>{body}<p class=help>:download-cancel and :download-open take the number as a count, e.g. 2:download-open. \
         :download-clear forgets finished ones.</p></main></body></html>"
    );
    if let Ok(mut page) = PAGE.write() {
        *page = Some(Arc::from(html.into_bytes()));
    }
}

const STYLE: &str = "
  :root { color-scheme: light dark; --bg: #fbfbf9; --fg: #1d1f21; --muted: #5f6368; --line: #e2e2dc; }
  @media (prefers-color-scheme: dark) { :root { --bg: #17181a; --fg: #e6e6e3; --muted: #9a9ea6; --line: #2c2e32; } }
  body { margin: 0; background: var(--bg); color: var(--fg); font: 14px/1.5 system-ui, sans-serif; }
  main { max-width: 50rem; padding: 1.5rem; }
  h1 { font-size: 1.3rem; margin: 0 0 1rem; }
  table { border-collapse: collapse; width: 100%; }
  td { padding: .4rem .6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
  .n { color: var(--muted); width: 2rem; }
  .dir { color: var(--muted); font-size: .85em; }
  .state { text-align: right; white-space: nowrap; }
  .empty, .help { color: var(--muted); }
";
