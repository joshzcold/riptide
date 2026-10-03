//! `:spawn`, userscripts and `:open-editor`. Programs run on a worker thread;
//! their results come back to the UI thread as tasks.
//!
//! Userscripts get qutebrowser's environment (`QUTE_URL`, `QUTE_FIFO`, …),
//! so existing qutebrowser userscripts work. Commands written to `QUTE_FIFO`
//! run when the script exits; it is a plain file on every platform.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::{Arc, Mutex};

use cef::*;
use hb_core::Command;
use hb_core::engine::Level;

use crate::{eval, shell};

const EDITOR_JS: &str = include_str!("../js/editor.js");

/// What to do once a program exits, carried back to the UI thread.
enum Then {
    Report {
        program: String,
        verbose: bool,
        output_messages: bool,
        fifo: Option<PathBuf>,
    },
    /// Write the edited file back into text field `id` of browser `browser`.
    Edit {
        browser: i32,
        id: u64,
        file: PathBuf,
    },
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
            detach,
            argv,
        } => {
            let flags = Flags {
                verbose: *verbose,
                output_messages: *output_messages,
                detach: *detach,
            };
            if *userscript {
                userscript_start(argv.clone(), flags, count);
            } else {
                start(argv.clone(), Vec::new(), flags, None, None);
            }
        }
        Command::OpenEditor => open_editor(),
        _ => return false,
    }
    true
}

#[derive(Clone, Copy)]
struct Flags {
    verbose: bool,
    output_messages: bool,
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
    let dir =
        std::env::temp_dir().join(format!("hackers-browser-{kind}-{}-{n}", std::process::id()));
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
}

fn userscript_start(argv: Vec<String>, flags: Flags, count: Option<u32>) {
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
    argv[0] = hb_config::userscripts::resolve(&argv[0], &paths, home.as_deref())
        .to_string_lossy()
        .into_owned();
    let mut env = vec![
        ("QUTE_MODE", "command".to_string()),
        ("QUTE_URL", url),
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
                selection: String(getSelection()) })";
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
            fifo,
        } => report(&program, verbose, output_messages, fifo.as_deref(), &done),
        Then::Edit { browser, id, file } => edited(browser, id, &file, &done),
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
    let Some((browser, template)) = shell::with(|s| {
        (
            s.current_browser(),
            s.engine.settings().list("editor.command").to_vec(),
        )
    }) else {
        return;
    };
    let Some(browser) = browser else { return };
    let target = browser.clone();
    let code = format!("{EDITOR_JS}\nwindow.__hbEditor.take()");
    eval::eval(&browser, &code, move |result| {
        let field = result
            .ok()
            .and_then(|json| serde_json::from_str::<Option<Field>>(&json).ok())
            .flatten();
        let Some(field) = field else {
            shell::show_message(Level::Error, "No text field is focused");
            return shell::refresh_ui();
        };
        let started = temp_dir("editor").and_then(|dir| {
            let file = dir.join("field.txt");
            std::fs::write(&file, &field.text)?;
            Ok((dir, file))
        });
        let (dir, file) = match started {
            Ok(v) => v,
            Err(e) => {
                shell::show_message(Level::Error, format!("Can't start the editor: {e}"));
                return shell::refresh_ui();
            }
        };
        let argv = hb_core::settings::editor_argv(
            &template,
            &file.to_string_lossy(),
            field.line,
            field.column,
        );
        let Some((program, args)) = argv.split_first() else {
            return;
        };
        let mut process = Process::new(program);
        process
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        run_in_background(
            process,
            Then::Edit {
                browser: target.identifier(),
                id: field.id,
                file,
            },
            Some(dir),
        );
    });
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
    match &done.status {
        Ok(Some(0)) => {}
        Ok(code) => {
            let code = code.map_or("a signal".to_string(), |c| format!("status {c}"));
            return shell::show_message(
                Level::Error,
                format!("The editor exited with {code}; text not changed"),
            );
        }
        Err(e) => return shell::show_message(Level::Error, format!("Can't run the editor: {e}")),
    }
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => {
            return shell::show_message(Level::Error, format!("Can't read the edited text: {e}"));
        }
    };
    // Editors add a final newline; a one-line field shouldn't get it.
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    let code = format!(
        "{EDITOR_JS}\nwindow.__hbEditor.put({id}, {})",
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
