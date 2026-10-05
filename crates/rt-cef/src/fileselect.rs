//! `fileselect.handler = external`: upload fields open a program such as
//! ranger or yazi instead of Chromium's file dialog.

use cef::*;

use crate::{shell, spawn};

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
            spawn::pick_files(&template, callback);
            1
        }
    }
}
