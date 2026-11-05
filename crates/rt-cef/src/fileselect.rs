//! Upload fields: Chromium's file dialog, a program such as ranger, yazi or
//! fzf (`fileselect.handler = external`), riptide's fuzzy picker in the
//! prompt area (`prompt`), or a `fileselect` hook in config.lua.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::{Arc, Mutex};

use cef::*;

use rt_core::Command;
use rt_core::engine::Level;
use rt_core::fileselect::{Entry, FileMode, FileRequest, ListRequest, WalkOptions};
use rt_core::prompt::{PromptAnswer, PromptKind, Topic};
use rt_core::settings::Settings;

use crate::{prompts, shell, spawn};

/// `:prompt-fileselect-external`: in the file picker, choose with the
/// field's `fileselect.*.command`; in a file prompt, put the folder a
/// picker chose into it.
pub fn run_command(command: &Command) -> bool {
    if *command != Command::PromptFileselectExternal {
        return false;
    }
    if let Some(Some((id, request))) = shell::with(|s| s.engine.file_prompt()) {
        let template = setting_list(request.mode.command_setting());
        spawn::pick_files(&template, Some(&request), move |paths| {
            if let Some(effects) = shell::with(|s| s.engine.answer_file_prompt(id, paths)) {
                shell::apply(effects);
            }
            shell::refresh_ui();
        });
        return true;
    }
    let template = setting_list("fileselect.folder.command");
    let request = FileRequest {
        mode: FileMode::Folder,
        dir: start_dir(None),
        extensions: Vec::new(),
    };
    spawn::pick_files(&template, Some(&request), |paths| {
        if let Some(folder) = paths.first() {
            let text = format!("{}/", folder.trim_end_matches('/'));
            shell::with(|s| s.engine.set_path_prompt_text(&text));
            shell::refresh_ui();
        }
    });
    true
}

fn setting_list(name: &str) -> Vec<String> {
    shell::with(|s| s.engine.settings().list(name).to_vec()).unwrap_or_default()
}

/// Where a picker starts: `fileselect.directory` if set, else the folder
/// Chromium suggests (the last one files were chosen from, or the one its
/// suggested file is in), else home.
fn start_dir(suggested: Option<&Path>) -> PathBuf {
    let home = std::env::home_dir();
    let configured = shell::with(|s| s.engine.settings().str("fileselect.directory").to_string())
        .unwrap_or_default();
    if !configured.is_empty() {
        let dir = rt_config::downloads::expand_home(&configured, home.as_deref());
        if dir.is_dir() {
            return dir;
        }
    }
    if let Some(path) = suggested.filter(|p| p.is_absolute()) {
        if path.is_dir() {
            return path.to_path_buf();
        }
        if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
            return parent.to_path_buf();
        }
    }
    home.unwrap_or_else(|| PathBuf::from("."))
}

/// A file request from Lua's `rt.ui.pick_files`.
pub fn lua_request(mode: FileMode, dir: Option<&str>, extensions: Vec<String>) -> FileRequest {
    let home = std::env::home_dir();
    let dir = dir
        .map(|d| rt_config::downloads::expand_home(d, home.as_deref()))
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| start_dir(None));
    FileRequest {
        mode,
        dir,
        extensions,
    }
}

/// Ask with the file picker; `chosen` gets the paths, or none if cancelled.
pub fn ask(
    browser: Option<i32>,
    title: String,
    request: FileRequest,
    chosen: impl FnOnce(Vec<String>) + 'static,
) {
    let message = match (request.mode, request.extensions.is_empty()) {
        (FileMode::Folder, _) => "Choose a folder".to_string(),
        (FileMode::Multiple, true) => "Choose files".to_string(),
        (FileMode::Single, true) => "Choose a file".to_string(),
        (_, false) => format!("Choose {}", request.extensions.join(", ")),
    };
    // Withdrawn when the page navigates, like its dialogs: the field is gone.
    prompts::ask(
        browser,
        prompts::Scope::JsDialog,
        Topic::Files,
        title,
        message,
        PromptKind::Files(request),
        move |answer| match answer {
            PromptAnswer::Files(paths) => chosen(paths),
            _ => chosen(Vec::new()),
        },
    );
}

thread_local! {
    static NEXT_TAKEN: Cell<u64> = const { Cell::new(1) };
    /// Upload fields a `fileselect` hook took, until `rt.answer_fileselect`.
    static TAKEN: RefCell<HashMap<u64, FileDialogCallback>> = RefCell::new(HashMap::new());
}

/// `rt.answer_fileselect`: answer an upload field a hook took.
pub fn answer_taken(id: u64, paths: Option<Vec<String>>) {
    let Some(callback) = TAKEN.with(|t| t.borrow_mut().remove(&id)) else {
        return shell::show_message(
            Level::Error,
            format!("rt.answer_fileselect: no upload field is waiting as {id}"),
        );
    };
    answer(&callback, paths.unwrap_or_default());
}

fn answer(callback: &FileDialogCallback, paths: Vec<String>) {
    if paths.is_empty() {
        return callback.cancel();
    }
    let mut list = CefStringList::new();
    for path in &paths {
        list.append(path);
    }
    callback.cont(Some(&mut list));
}

/// Offer `request` to the `fileselect` hooks: true if one took it.
fn offer_to_lua(callback: &FileDialogCallback, request: &FileRequest, title: &str) -> bool {
    let id = NEXT_TAKEN.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let (dir, extensions, id_text) = (
        request.dir.display().to_string(),
        request.extensions.join(","),
        id.to_string(),
    );
    let fields = [
        ("mode", request.mode.name()),
        ("dir", dir.as_str()),
        ("extensions", extensions.as_str()),
        ("title", title),
        ("id", id_text.as_str()),
    ];
    // Stored first: a hook may answer before it returns.
    TAKEN.with(|t| t.borrow_mut().insert(id, callback.clone()));
    let taken = crate::lua::fileselect(&fields);
    if !taken {
        TAKEN.with(|t| t.borrow_mut().remove(&id));
    }
    taken
}

/// List `wanted.root` for the file picker off the UI thread, with
/// `fileselect.prompt.command` or by walking it.
pub fn list(wanted: ListRequest, settings: &Settings) {
    let command = settings.list("fileselect.prompt.command").to_vec();
    let max = settings.int("fileselect.prompt.max_files").max(1) as usize;
    let options = WalkOptions {
        hidden: settings.bool("fileselect.prompt.hidden"),
        ignore: settings.list("fileselect.prompt.ignore").to_vec(),
        max,
        folders_only: wanted.request.mode == FileMode::Folder,
    };
    std::thread::spawn(move || {
        let root = &wanted.root;
        let (entries, error) = if !root.is_dir() {
            (
                Vec::new(),
                Some(format!("{} isn't a folder", root.display())),
            )
        } else if command.is_empty() {
            (rt_core::fileselect::walk(root, &options), None)
        } else {
            run_listing(&command, &wanted, max)
        };
        let mut task = Listed::new(Arc::new(Mutex::new(Some((wanted, entries, error)))));
        post_task(ThreadId::UI, Some(&mut task));
    });
}

/// Run `fileselect.prompt.command` in the folder and read its paths.
fn run_listing(
    template: &[String],
    wanted: &ListRequest,
    max: usize,
) -> (Vec<Entry>, Option<String>) {
    let mut request = wanted.request.clone();
    request.dir = wanted.root.clone();
    let argv = request.expand(template, "");
    let Some((program, args)) = argv.split_first() else {
        return (Vec::new(), None);
    };
    let output = Process::new(program)
        .args(args)
        .current_dir(&wanted.root)
        .envs(request.env())
        .stdin(Stdio::null())
        .output();
    match output {
        Err(e) => (Vec::new(), Some(format!("Can't run {program}: {e}"))),
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let entries = rt_core::fileselect::parse_listing(&stdout, &wanted.root, max);
            let error = (!out.status.success() && entries.is_empty()).then(|| {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let why = stderr.lines().next().unwrap_or("").trim();
                format!("{program} failed: {why}")
            });
            (entries, error)
        }
    }
}

type Listing = (ListRequest, Vec<Entry>, Option<String>);

wrap_task! {
    struct Listed {
        // Posted from a worker thread; runs once on the UI thread.
        result: Arc<Mutex<Option<Listing>>>,
    }

    impl Task {
        fn execute(&self) {
            let taken = self.result.lock().ok().and_then(|mut r| r.take());
            if let Some((wanted, entries, error)) = taken {
                shell::with(|s| {
                    s.engine
                        .set_file_listing(wanted.prompt, &wanted.root, entries, error)
                });
                shell::refresh_ui();
            }
        }
    }
}

wrap_dialog_handler! {
    pub struct RtDialogHandler {}

    impl DialogHandler {
        fn on_file_dialog(
            &self,
            browser: Option<&mut Browser>,
            mode: FileDialogMode,
            title: Option<&CefString>,
            default_file_path: Option<&CefString>,
            accept_filters: Option<&mut CefStringList>,
            accept_extensions: Option<&mut CefStringList>,
            _accept_descriptions: Option<&mut CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> ::std::os::raw::c_int {
            let mode = if mode == FileDialogMode::OPEN_MULTIPLE {
                FileMode::Multiple
            } else if mode == FileDialogMode::OPEN_FOLDER {
                FileMode::Folder
            } else if mode == FileDialogMode::OPEN {
                FileMode::Single
            } else {
                // Save dialogs keep Chromium's own; downloads ask in the prompt instead.
                return 0;
            };
            let Some(callback) = callback.map(|c| c.clone()) else {
                return 0;
            };
            let strings = |list: Option<&mut CefStringList>| -> Vec<String> {
                list.map(crate::favicons::read_list).unwrap_or_default()
            };
            let suggested = default_file_path
                .map(|p| p.to_string())
                .filter(|p| !p.is_empty())
                .map(PathBuf::from);
            let request = FileRequest {
                mode,
                dir: start_dir(suggested.as_deref()),
                extensions: rt_core::fileselect::accepted_extensions(
                    &strings(accept_filters),
                    &strings(accept_extensions),
                ),
            };
            let title = title
                .map(|t| t.to_string())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "Upload".to_string());
            if offer_to_lua(&callback, &request, &title) {
                return 1;
            }
            let handler = shell::with(|s| s.engine.settings().str("fileselect.handler").to_string())
                .unwrap_or_default();
            match handler.as_str() {
                "external" => {
                    let template = setting_list(mode.command_setting());
                    spawn::pick_files(&template, Some(&request), move |paths| answer(&callback, paths));
                }
                "prompt" => {
                    let browser = browser.map(|b| b.identifier());
                    ask(browser, title, request, move |paths| answer(&callback, paths));
                }
                _ => return 0,
            }
            1
        }
    }
}
