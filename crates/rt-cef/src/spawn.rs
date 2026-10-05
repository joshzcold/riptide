//! `:spawn`, userscripts and `:open-editor`. Programs run on a worker thread;
//! their results come back to the UI thread as tasks.
//!
//! Userscripts get qutebrowser's environment (`QUTE_URL`, `QUTE_FIFO`, …),
//! so existing qutebrowser userscripts work. Commands written to `QUTE_FIFO`
//! run when the script exits; it is a plain file on every platform.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::{Arc, Mutex, RwLock};

use cef::*;
use rt_core::Command;
use rt_core::command::OpenTarget;
use rt_core::engine::Level;

use crate::{eval, shell};

const EDITOR_JS: &str = include_str!("../js/editor.js");

/// What to do once a program exits, carried back to the UI thread.
enum Then {
    Report {
        program: String,
        verbose: bool,
        output_messages: bool,
        output_tab: bool,
        fifo: Option<PathBuf>,
    },
    /// Write the edited file back into text field `id` of browser `browser`.
    Edit {
        browser: i32,
        id: u64,
        file: PathBuf,
    },
    /// Open the edited file's text as a URL (`:edit-url`).
    EditUrl {
        file: PathBuf,
        target: OpenTarget,
        related: bool,
    },
    /// Put the edited command line back, or run it (`:cmd-edit`).
    EditCommand { file: PathBuf, run: bool },
    /// Answer file dialog `id` with the paths written to `file`.
    FileSelect { file: PathBuf, id: u32 },
}

thread_local! {
    /// File dialogs waiting for an external picker, by id. CEF callbacks stay
    /// on the UI thread; only the id goes to the worker thread.
    static FILE_DIALOGS: RefCell<HashMap<u32, FileDialogCallback>> = RefCell::new(HashMap::new());
    static NEXT_DIALOG: Cell<u32> = const { Cell::new(1) };
}

/// `fileselect.handler = external`: run `template` (with `{}` replaced by a
/// file to write the chosen paths to) and answer the dialog with them.
pub fn pick_files(template: &[String], callback: FileDialogCallback) {
    let started = temp_dir("fileselect").and_then(|dir| {
        let file = dir.join("chosen.txt");
        std::fs::write(&file, "")?;
        Ok((dir, file))
    });
    let (dir, file) = match started {
        Ok(v) => v,
        Err(e) => {
            callback.cancel();
            return shell::show_message(Level::Error, format!("Can't start the file picker: {e}"));
        }
    };
    let path = file.to_string_lossy();
    let argv: Vec<String> = template.iter().map(|a| a.replace("{}", &path)).collect();
    let Some((program, args)) = argv.split_first() else {
        callback.cancel();
        return shell::show_message(Level::Error, "The fileselect command is empty");
    };
    let id = NEXT_DIALOG.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    FILE_DIALOGS.with(|d| d.borrow_mut().insert(id, callback));
    let mut process = Process::new(program);
    process
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    run_in_background(process, Then::FileSelect { file, id }, Some(dir));
}

fn files_picked(file: &Path, id: u32, done: &Finished) {
    let Some(callback) = FILE_DIALOGS.with(|d| d.borrow_mut().remove(&id)) else {
        return;
    };
    if let Err(e) = &done.status {
        shell::show_message(Level::Error, format!("Can't run the file picker: {e}"));
    }
    let chosen = std::fs::read_to_string(file).unwrap_or_default();
    let mut paths = CefStringList::new();
    let mut any = false;
    for line in chosen.lines().map(str::trim).filter(|l| !l.is_empty()) {
        paths.append(line);
        any = true;
    }
    if any {
        callback.cont(Some(&mut paths));
    } else {
        callback.cancel();
    }
}

struct Finished {
    /// The exit code, or why the program couldn't run.
    status: Result<Option<i32>, String>,
    stdout: String,
    stderr: String,
    /// Removed once the results are handled.
    temp_dir: Option<PathBuf>,
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::Spawn {
            userscript,
            verbose,
            output_messages,
            output,
            hint_url,
            detach,
            argv,
        } => {
            let flags = Flags {
                verbose: *verbose,
                output_messages: *output_messages,
                output_tab: *output,
                detach: *detach,
            };
            if *userscript {
                userscript_start(argv.clone(), flags, count, hint_url.clone());
            } else {
                start(argv.clone(), Vec::new(), flags, None, None);
            }
        }
        Command::OpenEditor => open_editor(),
        Command::EditUrl {
            target,
            related,
            url,
        } => {
            let url = url
                .clone()
                .or_else(|| shell::with(|s| s.tabs.current().map(|t| t.url.clone())).flatten())
                .unwrap_or_default();
            let (target, related) = (*target, *related);
            start_editor(&url, 1, 1, "url", |file| Then::EditUrl {
                file,
                target,
                related,
            });
        }
        Command::CmdEdit { run } => {
            let text = shell::with(|s| s.engine.status().command_line.map(|c| c.text)).flatten();
            let Some(text) = text else {
                shell::show_message(
                    Level::Error,
                    "There's no command line to edit; bind :cmd-edit in command mode",
                );
                return true;
            };
            // The command line comes back when the editor closes.
            if let Some(effects) = shell::with(|s| s.engine.execute_str("mode-leave", None)) {
                shell::apply(effects);
            }
            let run = *run;
            let column = text.chars().count() + 1;
            start_editor(&text, 1, column, "cmd", |file| Then::EditCommand {
                file,
                run,
            });
        }
        _ => return false,
    }
    true
}

#[derive(Clone, Copy)]
struct Flags {
    verbose: bool,
    output_messages: bool,
    output_tab: bool,
    detach: bool,
}

/// A fresh private directory for one run's files.
fn temp_dir(kind: &str) -> std::io::Result<PathBuf> {
    thread_local! {
        static NEXT: Cell<u32> = const { Cell::new(0) };
    }
    let n = NEXT.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    let dir = std::env::temp_dir().join(format!("riptide-{kind}-{}-{n}", std::process::id()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&dir)?;
    Ok(dir)
}

#[derive(serde::Deserialize, Default)]
struct PageDump {
    html: String,
    text: String,
    selection: String,
    #[serde(default)]
    user_agent: String,
}

/// `hint_url` is set for userscripts run from hints: they get the hinted URL
/// as `QUTE_URL` and `QUTE_MODE=hints`, as in qutebrowser.
fn userscript_start(argv: Vec<String>, flags: Flags, count: Option<u32>, hint_url: Option<String>) {
    let Some((paths, url, title, index, browser, download_dir)) = shell::with(|s| {
        let tab = s.tabs.current();
        (
            s.paths.clone(),
            tab.map(|t| t.url.clone()).unwrap_or_default(),
            tab.map(|t| t.title.clone()).unwrap_or_default(),
            s.tabs.current_index() + 1,
            s.current_browser(),
            crate::downloads::download_dir(),
        )
    }) else {
        return;
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut argv = argv;
    argv[0] = rt_config::userscripts::resolve(&argv[0], &paths, home.as_deref())
        .to_string_lossy()
        .into_owned();
    let mode = if hint_url.is_some() {
        "hints"
    } else {
        "command"
    };
    let mut env = vec![
        ("QUTE_MODE", mode.to_string()),
        ("QUTE_URL", hint_url.unwrap_or(url)),
        ("QUTE_TITLE", title),
        ("QUTE_TAB_INDEX", index.to_string()),
        ("QUTE_CONFIG_DIR", paths.config_dir.display().to_string()),
        ("QUTE_DATA_DIR", paths.data_dir.display().to_string()),
        ("QUTE_DOWNLOAD_DIR", download_dir.display().to_string()),
        ("QUTE_VERSION", env!("CARGO_PKG_VERSION").to_string()),
        ("QUTE_COMMANDLINE_TEXT", String::new()),
    ];
    if let Some(count) = count {
        env.push(("QUTE_COUNT", count.to_string()));
    }
    let code = "JSON.stringify({ html: document.documentElement.outerHTML, \
                text: document.body ? document.body.innerText : '', \
                selection: String(getSelection()), user_agent: navigator.userAgent })";
    let launch = move |dump: PageDump| {
        let dir = match temp_dir("userscript") {
            Ok(dir) => dir,
            Err(e) => {
                return shell::show_message(Level::Error, format!("Can't run userscript: {e}"));
            }
        };
        let (html, text, fifo) = (
            dir.join("page.html"),
            dir.join("page.txt"),
            dir.join("fifo"),
        );
        let written = std::fs::write(&html, dump.html)
            .and_then(|()| std::fs::write(&text, dump.text))
            .and_then(|()| std::fs::write(&fifo, ""));
        if let Err(e) = written {
            return shell::show_message(Level::Error, format!("Can't run userscript: {e}"));
        }
        let mut env = env;
        env.push(("QUTE_SELECTED_TEXT", dump.selection));
        env.push(("QUTE_USER_AGENT", dump.user_agent));
        env.push(("QUTE_HTML", html.display().to_string()));
        env.push(("QUTE_TEXT", text.display().to_string()));
        env.push(("QUTE_FIFO", fifo.display().to_string()));
        start(argv, env, flags, Some(fifo), Some(dir));
    };
    match browser {
        Some(browser) => eval::eval(&browser, code, move |result| {
            let dump = result
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            launch(dump);
        }),
        None => launch(PageDump::default()),
    }
}

fn start(
    argv: Vec<String>,
    env: Vec<(&'static str, String)>,
    flags: Flags,
    fifo: Option<PathBuf>,
    temp_dir: Option<PathBuf>,
) {
    let Some((program, args)) = argv.split_first() else {
        return;
    };
    let mut process = Process::new(program);
    process.args(args).envs(env).stdin(Stdio::null());
    if flags.detach {
        process.stdout(Stdio::null()).stderr(Stdio::null());
        match process.spawn() {
            Ok(_) => tracing::debug!(program, "spawned detached"),
            Err(e) => shell::show_message(Level::Error, format!("Can't run {program}: {e}")),
        }
        return;
    }
    process.stdout(Stdio::piped()).stderr(Stdio::piped());
    run_in_background(
        process,
        Then::Report {
            program: program.clone(),
            verbose: flags.verbose,
            output_messages: flags.output_messages,
            output_tab: flags.output_tab,
            fifo,
        },
        temp_dir,
    );
}

fn run_in_background(mut process: Process, then: Then, temp_dir: Option<PathBuf>) {
    std::thread::spawn(move || {
        let finished = match process.output() {
            Ok(out) => Finished {
                status: Ok(out.status.code()),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                temp_dir,
            },
            Err(e) => Finished {
                status: Err(e.to_string()),
                stdout: String::new(),
                stderr: String::new(),
                temp_dir,
            },
        };
        let mut task = Done::new(Arc::new(Mutex::new(Some((then, finished)))));
        post_task(ThreadId::UI, Some(&mut task));
    });
}

fn finished(then: Then, done: Finished) {
    match then {
        Then::Report {
            program,
            verbose,
            output_messages,
            output_tab,
            fifo,
        } => {
            if output_tab {
                show_output(&program, &done);
            }
            report(&program, verbose, output_messages, fifo.as_deref(), &done)
        }
        Then::Edit { browser, id, file } => edited(browser, id, &file, &done),
        Then::EditUrl {
            file,
            target,
            related,
        } => {
            if let Some(text) = editor_result(&file, &done) {
                let url = text.trim();
                if url.is_empty() {
                    shell::show_message(Level::Error, "The URL is empty; nothing opened");
                } else {
                    shell::open(target, related, Some(url.to_string()));
                }
            }
        }
        Then::FileSelect { file, id } => files_picked(&file, id, &done),
        Then::EditCommand { file, run } => {
            if let Some(text) = editor_result(&file, &done) {
                let text = text.trim();
                let line = text.strip_prefix(':').unwrap_or(text);
                let command = if run {
                    line.to_string()
                } else {
                    format!("cmd-set-text :{line}")
                };
                if !line.is_empty()
                    && let Some(effects) = shell::with(|s| s.engine.execute_str(&command, None))
                {
                    shell::apply(effects);
                }
            }
        }
    }
    if let Some(dir) = &done.temp_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
    shell::refresh_ui();
}

fn report(
    program: &str,
    verbose: bool,
    output_messages: bool,
    fifo: Option<&Path>,
    done: &Finished,
) {
    // Commands from the userscript run first, as they would have while it ran.
    if let Some(text) = fifo.and_then(|f| std::fs::read_to_string(f).ok()) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let line = line.strip_prefix(':').unwrap_or(line);
            if let Some(effects) = shell::with(|s| s.engine.execute_str(line, None)) {
                shell::apply(effects);
            }
        }
    }
    let name = Path::new(program)
        .file_name()
        .map_or(program.to_string(), |n| n.to_string_lossy().into_owned());
    match &done.status {
        Err(e) => shell::show_message(Level::Error, format!("Can't run {name}: {e}")),
        Ok(Some(0)) => {
            if output_messages && !done.stdout.trim().is_empty() {
                shell::show_message(Level::Info, one_line(&done.stdout));
            } else if verbose {
                shell::show_message(Level::Info, format!("{name} exited successfully"));
            }
        }
        Ok(code) => {
            let code = code.map_or("a signal".to_string(), |c| format!("status {c}"));
            let detail = one_line(&done.stderr);
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            };
            shell::show_message(Level::Error, format!("{name} exited with {code}{detail}"));
        }
    }
}

/// Program output for the one-line status bar.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(serde::Deserialize)]
struct Field {
    id: u64,
    text: String,
    line: usize,
    column: usize,
}

fn open_editor() {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    let target = browser.clone();
    let code = format!("{EDITOR_JS}\nwindow.__rtEditor.take()");
    eval::eval(&browser, &code, move |result| {
        let field = result
            .ok()
            .and_then(|json| serde_json::from_str::<Option<Field>>(&json).ok())
            .flatten();
        let Some(field) = field else {
            shell::show_message(Level::Error, "No text field is focused");
            return shell::refresh_ui();
        };
        let (browser, id) = (target.identifier(), field.id);
        start_editor(&field.text, field.line, field.column, "field", |file| {
            Then::Edit { browser, id, file }
        });
    });
}

/// Open `text` in `editor.command`, with the cursor at `line`:`column`.
/// `then` says what to do with the file once the editor exits.
fn start_editor(
    text: &str,
    line: usize,
    column: usize,
    kind: &str,
    then: impl FnOnce(PathBuf) -> Then,
) {
    let template =
        shell::with(|s| s.engine.settings().list("editor.command").to_vec()).unwrap_or_default();
    let started = temp_dir(kind).and_then(|dir| {
        let file = dir.join(format!("{kind}.txt"));
        std::fs::write(&file, text)?;
        Ok((dir, file))
    });
    let (dir, file) = match started {
        Ok(v) => v,
        Err(e) => return shell::show_message(Level::Error, format!("Can't start the editor: {e}")),
    };
    let argv = rt_core::settings::editor_argv(&template, &file.to_string_lossy(), line, column);
    let Some((program, args)) = argv.split_first() else {
        return shell::show_message(Level::Error, "editor.command is empty");
    };
    let mut process = Process::new(program);
    process
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    run_in_background(process, then(file), Some(dir));
}

/// The edited text, or `None` (with a message) if the editor failed.
fn editor_result(file: &Path, done: &Finished) -> Option<String> {
    match &done.status {
        Ok(Some(0)) => {}
        Ok(code) => {
            let code = code.map_or("a signal".to_string(), |c| format!("status {c}"));
            shell::show_message(
                Level::Error,
                format!("The editor exited with {code}; nothing changed"),
            );
            return None;
        }
        Err(e) => {
            shell::show_message(Level::Error, format!("Can't run the editor: {e}"));
            return None;
        }
    }
    match std::fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) => {
            shell::show_message(Level::Error, format!("Can't read the edited text: {e}"));
            None
        }
    }
}

fn edited(browser: i32, id: u64, file: &Path, done: &Finished) {
    let browser = shell::with(|s| {
        s.tabs
            .iter()
            .filter_map(|t| t.browser())
            .find(|b| b.identifier() == browser)
    })
    .flatten();
    let Some(browser) = browser else {
        return shell::show_message(Level::Error, "The tab with the text field is gone");
    };
    let Some(text) = editor_result(file, done) else {
        return;
    };
    // Editors add a final newline; a one-line field shouldn't get it.
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    let code = format!(
        "{EDITOR_JS}\nwindow.__rtEditor.put({id}, {})",
        serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into())
    );
    eval::eval(&browser, &code, |result| {
        if result.as_deref() != Ok("true") {
            shell::show_message(Level::Error, "The text field is gone");
            shell::refresh_ui();
        }
    });
}

wrap_task! {
    struct Done {
        // Posted from a worker thread; runs once on the UI thread.
        result: Arc<Mutex<Option<(Then, Finished)>>>,
    }

    impl Task {
        fn execute(&self) {
            let taken = self.result.lock().ok().and_then(|mut r| r.take());
            if let Some((then, done)) = taken {
                finished(then, done);
            }
        }
    }
}

/// `riptide://process/`: the output of the last `:spawn -o`.
static OUTPUT_PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn output_page() -> Arc<[u8]> {
    OUTPUT_PAGE
        .read()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(|| {
            Arc::from(&b"<!doctype html><title>Process output</title><p>Nothing has run yet."[..])
        })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn show_output(program: &str, done: &Finished) {
    let name = Path::new(program)
        .file_name()
        .map_or(program.to_string(), |n| n.to_string_lossy().into_owned());
    let status = match &done.status {
        Ok(Some(code)) => format!("exited with status {code}"),
        Ok(None) => "was killed by a signal".to_string(),
        Err(e) => format!("couldn't run: {e}"),
    };
    let stderr = if done.stderr.is_empty() {
        String::new()
    } else {
        format!("<h2>stderr</h2><pre>{}</pre>", escape(&done.stderr))
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>{name} output</title><style>\
         :root {{ color-scheme: light dark; }} body {{ margin: 1.5rem; font: 14px/1.5 system-ui, sans-serif; }} \
         pre {{ font: 13px/1.4 \"DejaVu Sans Mono\", monospace; white-space: pre-wrap; }} \
         .status {{ color: gray; }}</style></head><body><h1>{name}</h1><p class=status>{status}</p>\
         <pre>{}</pre>{stderr}</body></html>",
        escape(&done.stdout),
        name = escape(&name),
        status = escape(&status),
    );
    if let Ok(mut page) = OUTPUT_PAGE.write() {
        *page = Some(Arc::from(html.into_bytes()));
    }
    shell::open(
        rt_core::command::OpenTarget::Tab,
        true,
        Some("riptide://process/".to_string()),
    );
}
