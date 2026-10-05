//! `fileselect.handler = external`: upload fields open a program such as
//! ranger or yazi instead of Chromium's file dialog.

use cef::*;

use rt_core::Command;

use crate::{shell, spawn};

/// `:prompt-fileselect-external`: put the folder a picker chose into the
/// file prompt.
pub fn run_command(command: &Command) -> bool {
    if *command != Command::PromptFileselectExternal {
        return false;
    }
    let template = shell::with(|s| {
        s.engine
            .settings()
            .list("fileselect.folder.command")
            .to_vec()
    })
    .unwrap_or_default();
    spawn::pick_files(&template, |paths| {
        if let Some(folder) = paths.first() {
            let text = format!("{}/", folder.trim_end_matches('/'));
            shell::with(|s| s.engine.set_path_prompt_text(&text));
            shell::refresh_ui();
        }
    });
    true
}

wrap_dialog_handler! {
    pub struct RtDialogHandler {}

    impl DialogHandler {
        fn on_file_dialog(
            &self,
            _browser: Option<&mut Browser>,
            mode: FileDialogMode,
            _title: Option<&CefString>,
            _default_file_path: Option<&CefString>,
            _accept_filters: Option<&mut CefStringList>,
            _accept_extensions: Option<&mut CefStringList>,
            _accept_descriptions: Option<&mut CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> ::std::os::raw::c_int {
            let setting = if mode == FileDialogMode::OPEN_MULTIPLE {
                "fileselect.multiple_files.command"
            } else if mode == FileDialogMode::OPEN_FOLDER {
                "fileselect.folder.command"
            } else if mode == FileDialogMode::OPEN {
                "fileselect.single_file.command"
            } else {
                // Save dialogs keep Chromium's own; downloads ask in the prompt instead.
                return 0;
            };
            let template = shell::with(|s| {
                let settings = s.engine.settings();
                (settings.str("fileselect.handler") == "external").then(|| settings.list(setting).to_vec())
            })
            .flatten();
            let (Some(template), Some(callback)) = (template, callback.map(|c| c.clone())) else {
                return 0;
            };
            spawn::pick_files(&template, move |paths| {
                if paths.is_empty() {
                    return callback.cancel();
                }
                let mut list = CefStringList::new();
                for path in &paths {
                    list.append(path);
                }
                callback.cont(Some(&mut list));
            });
            1
        }
    }
}
