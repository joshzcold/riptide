//! Questions the browser asks the user: JavaScript dialogs, logins, download
//! locations, permissions. They queue and are answered one at a time.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptKind {
    /// Free text, e.g. a download path or `window.prompt()`. `path` turns on
    /// file name completion with Tab.
    Text {
        default: String,
        masked: bool,
        path: bool,
    },
    /// y/n; `default` is what Return means.
    YesNo { default: bool, remember: Remember },
    /// Information only; any accept dismisses it.
    Alert,
}

/// Whether and where a yes/no answer is kept, which decides the key hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remember {
    /// A one-off question (`confirm()`, overwrite a file).
    Never,
    /// `y`/`n` answer once; `A`/`N` save the answer for the site.
    Always,
    /// Chromium remembers `y` per site itself; `A`/`N` also save the answer
    /// for the site, and `n` only means "not now".
    Site,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub kind: PromptKind,
    /// What `prompt-yank` copies, e.g. a download's URL.
    pub url: Option<String>,
    /// A download's save prompt, which `prompt-open-download` can answer.
    pub download: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptAnswer {
    Text(String),
    /// `remember` is set by `A`/`N`, e.g. to keep a permission decision.
    Yes {
        remember: bool,
    },
    No {
        remember: bool,
    },
    Ok,
    Cancelled,
    /// `prompt-open-download`: save to a temporary folder and open the file
    /// with `command` (or the desktop's default) once it's done.
    OpenDownload {
        command: Option<String>,
    },
}

/// What the overlay draws for the active prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PromptView {
    pub title: String,
    pub message: String,
    pub kind: &'static str,
    /// The text being typed, masked for passwords.
    pub input: String,
    pub cursor: usize,
    pub hint: String,
    /// How many more prompts are waiting.
    pub queued: usize,
}

impl Prompt {
    pub fn hint(&self) -> String {
        match self.kind {
            PromptKind::Text { path: true, .. } => {
                "Tab: complete, Return: accept, Escape: cancel".into()
            }
            PromptKind::Text { .. } => "Return: accept, Escape: cancel".into(),
            PromptKind::YesNo { default, remember } => {
                let default = if default { "yes" } else { "no" };
                match remember {
                    Remember::Never => format!("y: yes, n: no, Return: {default}, Escape: cancel"),
                    Remember::Always => format!(
                        "y: yes, n: no, A: always, N: never (saved for this site), Return: {default}, Escape: cancel"
                    ),
                    Remember::Site => {
                        "y: allow, A: always allow, n: not now, N: always block (saved for this site), Escape: not now"
                            .to_string()
                    }
                }
            }
            PromptKind::Alert => "Return or Escape: close".into(),
        }
    }
}
