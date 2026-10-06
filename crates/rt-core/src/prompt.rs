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

/// What a prompt is about, for styling (`prompt-<topic>` classes in `ui.css`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    /// The page's `alert()`, `confirm()`, `prompt()` or leave-page dialog.
    Dialog,
    Permission,
    /// A site's or proxy's login.
    Login,
    Download,
    Certificate,
    /// riptide asking before doing something, e.g. quitting.
    Confirm,
}

impl Topic {
    pub fn name(self) -> &'static str {
        match self {
            Topic::Dialog => "dialog",
            Topic::Permission => "permission",
            Topic::Login => "login",
            Topic::Download => "download",
            Topic::Certificate => "certificate",
            Topic::Confirm => "confirm",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub kind: PromptKind,
    pub topic: Topic,
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
    /// [`Topic::name`].
    pub topic: &'static str,
    /// The text being typed, masked for passwords.
    pub input: String,
    pub cursor: usize,
    /// The options on one line, for the docked prompt.
    pub hint: String,
    /// The same, one per line or button in a floating prompt.
    pub options: Vec<PromptOption>,
    /// How many more prompts are waiting.
    pub queued: usize,
}

/// One way to answer a prompt: a key, and what it does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PromptOption {
    /// The key a button presses, e.g. `y` or `<Return>`.
    pub key: String,
    /// The keys as shown, e.g. `Return or Escape`.
    pub keys: String,
    pub label: String,
}

fn option(key: &str, keys: &str, label: &str) -> PromptOption {
    PromptOption {
        key: key.into(),
        keys: keys.into(),
        label: label.into(),
    }
}

impl Prompt {
    /// The ways to answer this prompt, in the order they're shown.
    pub fn options(&self) -> Vec<PromptOption> {
        let mut options = match self.kind {
            PromptKind::Text { path, .. } => {
                let mut options = Vec::new();
                if path {
                    options.push(option("<Tab>", "Tab", "complete"));
                }
                options.push(option("<Return>", "Return", "accept"));
                options.push(option("<Escape>", "Escape", "cancel"));
                options
            }
            PromptKind::YesNo { default, remember } => {
                let default = if default { "yes" } else { "no" };
                match remember {
                    Remember::Never => vec![
                        option("y", "y", "yes"),
                        option("n", "n", "no"),
                        option("<Return>", "Return", default),
                        option("<Escape>", "Escape", "cancel"),
                    ],
                    Remember::Always => vec![
                        option("y", "y", "yes"),
                        option("n", "n", "no"),
                        option("A", "A", "always"),
                        option("N", "N", "never (saved for this site)"),
                        option("<Return>", "Return", default),
                        option("<Escape>", "Escape", "cancel"),
                    ],
                    Remember::Site => vec![
                        option("y", "y", "allow"),
                        option("A", "A", "always allow"),
                        option("n", "n", "not now"),
                        option("N", "N", "always block (saved for this site)"),
                        option("<Escape>", "Escape", "not now"),
                    ],
                }
            }
            PromptKind::Alert => vec![option("<Return>", "Return or Escape", "close")],
        };
        if self.download {
            options.insert(
                options.len() - 1,
                option("<Ctrl-x>", "Ctrl-x", "open instead of saving"),
            );
        }
        options
    }

    /// [`Prompt::options`] on one line: `y: allow, n: not now, …`.
    pub fn hint(&self) -> String {
        self.options()
            .iter()
            .map(|o| format!("{}: {}", o.keys, o.label))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
