//! Downloads: ask where to save (`downloads.location.*`), track progress for
//! the status bar, and cancel or open them by number.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cef::*;
use rt_config::downloads::{current_system_dir, expand_home, sanitize_name, unique_path};
use rt_core::Command;
use rt_core::engine::Level;
use rt_core::prompt::{PromptAnswer, PromptKind, Remember};

use crate::prompts::{self, Scope};
use crate::shell;
use rt_core::html::escape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Running,
    Done,
    Cancelled,
    Failed,
}

struct Download {
    /// CEF's id for the item.
    id: u32,
    /// Where it came from, for `:download-retry`.
    url: String,
    path: PathBuf,
    percent: Option<i32>,
    state: State,
    callback: Option<DownloadItemCallback>,
    /// `prompt-open-download`: open it when done, with this command or
    /// (`None`) the desktop's default.
    open_when_done: Option<Option<String>>,
}

thread_local! {
    static DOWNLOADS: RefCell<Vec<Download>> = const { RefCell::new(Vec::new()) };
    /// The folder the last download was saved to, for `downloads.location.remember`.
    static LAST_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
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
/// How many downloads are still running, for `confirm_quit`.
pub fn running_count() -> usize {
    DOWNLOADS.with(|d| {
        d.borrow()
            .iter()
            .filter(|d| d.state == State::Running)
            .count()
    })
}

pub fn summary() -> String {
    DOWNLOADS.with(|d| {
        let running: Vec<Option<i32>> = d
            .borrow()
            .iter()
            .filter(|d| d.state == State::Running)
            .map(|d| d.percent)
            .collect();
        summary_of(&running)
    })
}

/// `↓count` and the average of the known percentages, from the running
/// downloads' progress; empty when nothing is running.
fn summary_of(running: &[Option<i32>]) -> String {
    if running.is_empty() {
        return String::new();
    }
    let known: Vec<i32> = running.iter().flatten().copied().collect();
    if known.is_empty() {
        return format!("↓{}", running.len());
    }
    format!(
        "↓{} {}%",
        running.len(),
        known.iter().sum::<i32>() / known.len() as i32
    )
}

/// Start the download at `path`, confirming before overwriting a file.
fn save_to(
    path: PathBuf,
    id: u32,
    url: String,
    callback: BeforeDownloadCallback,
    browser: Option<i32>,
    open_when_done: Option<Option<String>>,
) {
    let begin = move |path: PathBuf| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
        DOWNLOADS.with(|d| {
            d.borrow_mut().push(Download {
                id,
                url,
                path,
                percent: None,
                state: State::Running,
                callback: None,
                open_when_done,
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
            rt_core::prompt::Topic::Download,
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
    pub struct RtDownloadHandler {}

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
            let url = CefString::from(&item.url()).to_string();
            let browser = browser.map(|b| b.identifier());
            let name = sanitize_name(&suggested_name.map(CefString::to_string).unwrap_or_default());
            let (ask, remember, suggest) = shell::with(|s| {
                let settings = s.engine.settings();
                (
                    settings.bool("downloads.location.prompt"),
                    settings.bool("downloads.location.remember"),
                    settings.str("downloads.location.suggestion").to_string(),
                )
            })
            .unwrap_or((true, true, "both".into()));
            let dir = LAST_DIR
                .with(|d| d.borrow().clone())
                .filter(|d| remember && d.is_dir())
                .unwrap_or_else(download_dir);
            let suggestion = unique_path(&dir, &name, &|p: &Path| p.exists());
            if !ask {
                save_to(suggestion, id, url, callback, browser, None);
                return 1;
            }
            let default = match suggest.as_str() {
                "path" => format!("{}/", dir.to_string_lossy().trim_end_matches('/')),
                "filename" => suggestion.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                _ => suggestion.to_string_lossy().into_owned(),
            };
            let prompt_url = url.clone();
            prompts::ask_about(
                browser,
                Scope::Other, rt_core::prompt::Topic::Download,
                "Save file to",
                name.clone(),
                PromptKind::Text { default, masked: false, path: true },
                Some(prompt_url),
                true,
                move |answer| {
                    let (path, open) = match answer {
                        PromptAnswer::Text(text) => {
                            let mut path = expand_home(text.trim(), home().as_deref());
                            // A bare file name goes in the folder the prompt was about.
                            if path.is_relative() {
                                path = dir.join(path);
                            }
                            if path.is_dir() {
                                path = path.join(&name);
                            }
                            if let Some(parent) = path.parent() {
                                LAST_DIR.with(|d| *d.borrow_mut() = Some(parent.to_path_buf()));
                            }
                            (path, None)
                        }
                        // Into a temporary folder, to be opened when it's done.
                        PromptAnswer::OpenDownload { command } => {
                            let folder = std::env::temp_dir().join(format!("riptide-open-{}", std::process::id()));
                            (folder.join(&name), Some(command))
                        }
                        _ => return,
                    };
                    save_to(path, id, url, callback, browser, open);
                },
            );
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
                changed.then(|| (state, download.path.clone(), download.open_when_done.clone()))
            });
            match finished {
                Some((State::Done, path, Some(command))) => {
                    if let Err(e) = open_with(&path, command.as_deref()) {
                        shell::show_message(Level::Error, format!("Can't open {}: {e}", path.display()));
                    }
                }
                Some((State::Done, path, None)) => {
                    shell::show_message(Level::Info, format!("Download finished: {}", path.display()));
                    let delay = shell::with(|s| s.engine.settings().int("downloads.remove_finished")).unwrap_or(-1);
                    if delay >= 0 {
                        let mut task = RemoveFinished::new(id);
                        post_delayed_task(ThreadId::UI, Some(&mut task), delay);
                    }
                }
                Some((State::Cancelled, path, _)) => shell::show_message(Level::Warning, format!("Download cancelled: {}", path.display())),
                Some((State::Failed, path, _)) => shell::show_message(Level::Error, format!("Download failed: {}", path.display())),
                _ => {}
            }
            publish();
            shell::refresh_ui();
        }
    }
}

/// Downloads are numbered from 1 in the order they started, like qutebrowser.
fn pick(count: Option<u32>, state: State) -> Result<usize, String> {
    let what = if state == State::Running {
        "running"
    } else {
        "finished"
    };
    pick_any(count, &[state], what)
}

/// The download numbered `count`, or the newest one, in one of `states`.
fn pick_any(count: Option<u32>, states: &[State], what: &str) -> Result<usize, String> {
    let all: Vec<State> = DOWNLOADS.with(|d| d.borrow().iter().map(|d| d.state).collect());
    pick_from(&all, count, states, what)
}

/// The index of download number `count` (from 1) in `all`, or of the newest
/// one, as long as it's in one of `states`.
fn pick_from(
    all: &[State],
    count: Option<u32>,
    states: &[State],
    what: &str,
) -> Result<usize, String> {
    match count {
        Some(n) => {
            let index = (n as usize)
                .checked_sub(1)
                .filter(|&i| i < all.len())
                .ok_or(format!("There's no download {n}"))?;
            if !states.contains(&all[index]) {
                return Err(format!("Download {n} is not {what}"));
            }
            Ok(index)
        }
        None => all
            .iter()
            .rposition(|s| states.contains(s))
            .ok_or_else(|| format!("No {what} downloads")),
    }
}

/// Open a finished download with `downloads.open_dispatcher`, or the
/// desktop's default application.
fn open_with_system(path: &Path) -> std::io::Result<()> {
    open_with(path, None)
}

/// Open a file with `command` (the path is appended, or replaces `{}`), or
/// with `downloads.open_dispatcher` or the desktop's default when `None`.
fn open_with(path: &Path, command: Option<&str>) -> std::io::Result<()> {
    let dispatcher = match command {
        Some(command) => command.to_string(),
        None => shell::with(|s| {
            s.engine
                .settings()
                .str("downloads.open_dispatcher")
                .to_string()
        })
        .unwrap_or_default(),
    };
    if !dispatcher.trim().is_empty() {
        let file = path.to_string_lossy();
        let mut argv = rt_core::shell_words::split(&dispatcher).map_err(std::io::Error::other)?;
        if argv.iter().any(|a| a.contains("{}")) {
            argv = argv.iter().map(|a| a.replace("{}", &file)).collect();
        } else {
            argv.push(file.into_owned());
        }
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| std::io::Error::other("empty command"))?;
        return std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ());
    }
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

wrap_task! {
    struct RemoveFinished {
        id: u32,
    }

    impl Task {
        fn execute(&self) {
            DOWNLOADS.with(|d| d.borrow_mut().retain(|d| !(d.id == self.id && d.state == State::Done)));
            publish();
            shell::refresh_ui();
        }
    }
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
        Command::DownloadRetry => match pick_any(
            count,
            &[State::Failed, State::Cancelled],
            "failed or cancelled",
        ) {
            Ok(index) => {
                let url = DOWNLOADS.with(|d| d.borrow_mut().remove(index).url);
                start(&url);
            }
            Err(e) => shell::show_message(Level::Error, e),
        },
        Command::DownloadRemove { all: true } => {
            DOWNLOADS.with(|d| d.borrow_mut().retain(|d| d.state == State::Running));
            publish();
        }
        Command::DownloadRemove { all: false } => {
            let any = [State::Running, State::Done, State::Cancelled, State::Failed];
            match pick_any(count, &any, "listed") {
                Ok(index) => {
                    let download = DOWNLOADS.with(|d| d.borrow_mut().remove(index));
                    if let Some(callback) = download.callback {
                        callback.cancel();
                    }
                    publish();
                }
                Err(e) => shell::show_message(Level::Error, e),
            }
        }
        Command::DownloadDelete => match pick(count, State::Done) {
            Ok(index) => {
                let path = DOWNLOADS.with(|d| d.borrow()[index].path.clone());
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        DOWNLOADS.with(|d| d.borrow_mut().remove(index));
                        shell::show_message(Level::Info, format!("Deleted {}", path.display()));
                        publish();
                    }
                    Err(e) => shell::show_message(
                        Level::Error,
                        format!("Could not delete {}: {e}", path.display()),
                    ),
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
                rt_core::command::OpenTarget::Tab,
                true,
                Some("riptide://downloads/".to_string()),
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

/// `riptide://downloads/`, rebuilt on the UI thread whenever a download changes
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

#[cfg(test)]
mod tests {
    use super::State::*;
    use super::*;

    #[test]
    fn a_count_picks_that_download_if_its_in_the_right_state() {
        let all = [Done, Running, Failed];
        assert_eq!(pick_from(&all, Some(2), &[Running], "running"), Ok(1));
        assert_eq!(
            pick_from(&all, Some(1), &[Running], "running"),
            Err("Download 1 is not running".into())
        );
        assert_eq!(
            pick_from(&all, Some(4), &[Running], "running"),
            Err("There's no download 4".into())
        );
        assert_eq!(
            pick_from(&all, Some(0), &[Running], "running"),
            Err("There's no download 0".into())
        );
    }

    #[test]
    fn the_summary_counts_running_downloads_and_averages_known_progress() {
        assert_eq!(summary_of(&[]), "");
        assert_eq!(summary_of(&[None, None]), "↓2");
        assert_eq!(summary_of(&[Some(40), None, Some(60)]), "↓3 50%");
        assert_eq!(summary_of(&[Some(41)]), "↓1 41%");
    }

    #[test]
    fn no_count_picks_the_newest_in_the_right_state() {
        let all = [Done, Running, Done, Cancelled];
        assert_eq!(pick_from(&all, None, &[Done], "finished"), Ok(2));
        assert_eq!(pick_from(&all, None, &[Done, Cancelled], "finished"), Ok(3));
        assert_eq!(
            pick_from(&all, None, &[Failed], "failed"),
            Err("No failed downloads".into())
        );
        assert_eq!(
            pick_from(&[], None, &[Done], "finished"),
            Err("No finished downloads".into())
        );
    }
}
