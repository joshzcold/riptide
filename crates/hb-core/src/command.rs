use thiserror::Error;

use crate::hints::{HintGroup, HintRequest, HintTarget};
use crate::mode::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Current,
    Tab,
    Background,
    Window,
    Private,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readline {
    BackwardChar,
    ForwardChar,
    BeginningOfLine,
    EndOfLine,
    BackwardDeleteChar,
    DeleteChar,
    UnixLineDiscard,
    KillLine,
    Rubout,
    /// Delete back to the previous path separator, for file prompts.
    FilenameRubout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabTarget {
    /// 1-based; negative counts from the end.
    Number(i64),
    /// The previously focused tab.
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMoveTarget {
    /// `+` or `-`, multiplied by the count.
    Relative(i64),
    /// 1-based; negative counts from the end.
    Absolute(i64),
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YankWhat {
    Url,
    Title,
    Domain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Prev,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Open {
        target: OpenTarget,
        /// Position a new tab next to the current one rather than at the end.
        related: bool,
        url: Option<String>,
    },
    Back,
    Forward,
    Reload {
        force: bool,
    },
    Stop,
    Scroll(Direction),
    ScrollPage {
        x: f64,
        y: f64,
    },
    ScrollToPerc {
        perc: Option<f64>,
        horizontal: bool,
    },
    ModeEnter(Mode),
    ModeLeave,
    CmdSetText {
        text: String,
        append_space: bool,
    },
    CommandAccept,
    CommandHistoryPrev,
    CommandHistoryNext,
    Readline(Readline),
    ClearKeychain,
    /// `force` also closes pinned tabs.
    TabClose {
        force: bool,
    },
    /// Toggle pinning; the count picks the tab.
    TabPin,
    TabNext,
    TabPrev,
    TabFocus(Option<TabTarget>),
    TabMove(Option<TabMoveTarget>),
    /// Close other tabs; `force` closes pinned ones too.
    TabOnly {
        force: bool,
    },
    Undo,
    Hint(HintRequest),
    Yank(YankWhat),
    /// `:set [name[?|!]] [value]`
    Set {
        name: Option<String>,
        value: Option<String>,
    },
    Bind {
        mode: Mode,
        keys: Option<String>,
        command: Option<String>,
    },
    Unbind {
        mode: Mode,
        keys: String,
    },
    ConfigSource,
    /// Open the help page, optionally at a topic, in a new tab with `tab`.
    Help {
        tab: bool,
        topic: Option<String>,
    },
    Version,
    /// Open the bundled changelog, in a new tab with `tab`.
    Changelog {
        tab: bool,
    },
    CompletionFocus(FocusDirection),
    /// Answer the active prompt; `value` is yes/no for y/n questions.
    PromptAccept {
        value: Option<bool>,
        save: bool,
    },
    QuickmarkAdd {
        url: String,
        name: String,
    },
    QuickmarkLoad {
        target: OpenTarget,
        name: String,
    },
    /// Without a name, deletes the current page's quickmark.
    QuickmarkDel {
        name: Option<String>,
    },
    /// Defaults to the current page's URL and title.
    BookmarkAdd {
        url: Option<String>,
        title: Option<String>,
    },
    BookmarkLoad {
        target: OpenTarget,
        url: String,
    },
    BookmarkDel {
        url: Option<String>,
    },
    SessionSave {
        name: Option<String>,
    },
    SessionLoad {
        name: String,
    },
    SessionDelete {
        name: String,
    },
    HistoryClear {
        force: bool,
    },
    /// Download the filter lists and rebuild the content blocker.
    AdblockUpdate,
    /// Download a URL, or the current page.
    Download {
        url: Option<String>,
    },
    /// The count picks a download by number; default: the newest running one.
    DownloadCancel,
    /// Open a finished download with the system's default application.
    DownloadOpen,
    /// Forget finished, failed and cancelled downloads.
    DownloadClear,
    Quit {
        /// Save the open tabs as the default session first.
        save: bool,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("{0}: no such command")]
    Unknown(String),
    #[error("{command}: {message}")]
    BadArgs { command: String, message: String },
}

pub struct CommandSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// Hidden commands are bound to keys but not offered in completion.
    pub hidden: bool,
}

const fn spec(name: &'static str, description: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        description,
        hidden: false,
    }
}

const fn hidden(name: &'static str, description: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        description,
        hidden: true,
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    spec("open", "Open a URL or search for text"),
    spec("back", "Go back in history"),
    spec("forward", "Go forward in history"),
    spec("reload", "Reload the current page"),
    spec("stop", "Stop loading the current page"),
    spec("scroll", "Scroll in a direction"),
    spec("scroll-page", "Scroll by a multiple of the page size"),
    spec("scroll-to-perc", "Scroll to a percentage of the page"),
    spec("mode-enter", "Enter a key mode"),
    spec("mode-leave", "Leave the current mode"),
    spec("cmd-set-text", "Preset the command line text"),
    spec(
        "tab-close",
        "Close the current tab (--force: even if pinned)",
    ),
    spec(
        "tab-pin",
        "Pin or unpin the current tab (count: tab number)",
    ),
    spec("tab-next", "Switch to the next tab"),
    spec("tab-prev", "Switch to the previous tab"),
    spec("tab-focus", "Select a tab by number, or 'last'"),
    spec(
        "tab-move",
        "Move the current tab: +, -, start, end or a number",
    ),
    spec("tab-only", "Close all tabs except the current one"),
    spec("undo", "Re-open the last closed tab"),
    spec(
        "hint",
        "Label elements to follow: [--rapid] [group] [target] [fill text]",
    ),
    spec(
        "yank",
        "Copy the page's url, title or domain to the clipboard",
    ),
    spec(
        "set",
        "Show or change an option: :set name [value], :set name! toggles",
    ),
    spec(
        "bind",
        "Show or set a key binding: :bind [--mode m] keys [command]",
    ),
    spec("unbind", "Remove a key binding: :unbind [--mode m] keys"),
    spec("config-source", "Reload the configuration files"),
    spec(
        "help",
        "Show help: :help [-t] [:command | setting | section]",
    ),
    spec("version", "Show version, paths and loaded config files"),
    spec(
        "changelog",
        "Show what changed in each version: :changelog [-t]",
    ),
    spec(
        "quickmark-add",
        "Save a quickmark: :quickmark-add <url> <name>",
    ),
    spec(
        "quickmark-load",
        "Open a quickmark: :quickmark-load [-t|-b] <name>",
    ),
    spec(
        "quickmark-del",
        "Delete a quickmark (default: the current page's)",
    ),
    spec("bookmark-add", "Bookmark a URL (default: the current page)"),
    spec(
        "bookmark-load",
        "Open a bookmark: :bookmark-load [-t|-b] <url>",
    ),
    spec(
        "bookmark-del",
        "Delete a bookmark (default: the current page)",
    ),
    spec("session-save", "Save the open tabs: :session-save [name]"),
    spec("session-load", "Replace the open tabs with a saved session"),
    spec("session-delete", "Delete a saved session"),
    spec(
        "history-clear",
        "Delete all browsing history (needs --force)",
    ),
    spec(
        "adblock-update",
        "Download the filter lists in content.blocking.adblock.lists",
    ),
    spec("download", "Download a URL (default: the current page)"),
    spec("download-cancel", "Cancel a download (count: its number)"),
    spec(
        "download-open",
        "Open a finished download (count: its number)",
    ),
    spec("download-clear", "Remove finished downloads from the list"),
    spec(
        "quit",
        "Quit the browser; --save keeps the tabs as the default session",
    ),
    hidden(
        "completion-item-focus",
        "Select the next or previous completion",
    ),
    hidden("prompt-accept", "Answer the prompt: [--save] [yes|no]"),
    hidden("command-accept", "Execute the command line"),
    hidden(
        "command-history-prev",
        "Previous command line history entry",
    ),
    hidden("command-history-next", "Next command line history entry"),
    hidden("clear-keychain", "Clear the pending key sequence and count"),
    hidden("rl-backward-char", "Move cursor one character left"),
    hidden("rl-forward-char", "Move cursor one character right"),
    hidden(
        "rl-beginning-of-line",
        "Move cursor to the start of the line",
    ),
    hidden("rl-end-of-line", "Move cursor to the end of the line"),
    hidden(
        "rl-backward-delete-char",
        "Delete the character before the cursor",
    ),
    hidden("rl-delete-char", "Delete the character under the cursor"),
    hidden(
        "rl-unix-line-discard",
        "Delete from the cursor to the start of the line",
    ),
    hidden(
        "rl-kill-line",
        "Delete from the cursor to the end of the line",
    ),
    hidden("rl-rubout", "Delete the word before the cursor"),
    hidden(
        "rl-filename-rubout",
        "Delete the path component before the cursor",
    ),
];

/// Parse a full command line, which may chain commands with `;;`.
pub fn parse_line(line: &str) -> Result<Vec<Command>, CommandError> {
    line.split(";;")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse)
        .collect()
}

/// Parse one command, with or without a leading `:`.
pub fn parse(input: &str) -> Result<Command, CommandError> {
    let input = input.trim().trim_start_matches(':');
    let (name, rest) = match input.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (input, ""),
    };
    let mut args = Args::new(name, rest);
    let cmd = match name {
        "open" => {
            let (target, related) = args.open_target();
            let url = args.rest();
            Command::Open {
                target,
                related,
                url: (!url.is_empty()).then(|| url.to_string()),
            }
        }
        "back" => Command::Back,
        "forward" => Command::Forward,
        "reload" => Command::Reload {
            force: args.flag(&["-f", "--force"]).is_some(),
        },
        "stop" => Command::Stop,
        "scroll" => {
            let dir = match args.required("direction")? {
                "up" => Direction::Up,
                "down" => Direction::Down,
                "left" => Direction::Left,
                "right" => Direction::Right,
                "top" => Direction::Top,
                "bottom" => Direction::Bottom,
                other => return Err(args.error(format!("invalid direction: {other}"))),
            };
            Command::Scroll(dir)
        }
        "scroll-page" => {
            let x = args.number("x")?;
            let y = args.number("y")?;
            Command::ScrollPage { x, y }
        }
        "scroll-to-perc" => {
            let horizontal = args.flag(&["-x", "--horizontal"]).is_some();
            let perc = match args.optional() {
                Some(_) => Some(args.parse_last_number("perc")?),
                None => None,
            };
            Command::ScrollToPerc { perc, horizontal }
        }
        "mode-enter" => {
            let mode = args.required("mode")?;
            let mode = mode.parse::<Mode>().map_err(|e| args.error(e))?;
            Command::ModeEnter(mode)
        }
        "mode-leave" => Command::ModeLeave,
        "cmd-set-text" => {
            let append_space = args.flag(&["-s", "--space"]).is_some();
            let text = args.rest();
            if text.is_empty() {
                return Err(args.error("missing argument: text"));
            }
            Command::CmdSetText {
                text: text.to_string(),
                append_space,
            }
        }
        "command-accept" => Command::CommandAccept,
        "command-history-prev" => Command::CommandHistoryPrev,
        "command-history-next" => Command::CommandHistoryNext,
        "clear-keychain" => Command::ClearKeychain,
        "tab-close" => Command::TabClose {
            force: args.flag(&["-f", "--force"]).is_some(),
        },
        "tab-pin" => Command::TabPin,
        "tab-next" => Command::TabNext,
        "tab-prev" => Command::TabPrev,
        "tab-focus" => Command::TabFocus(match args.optional() {
            None => None,
            Some("last") => Some(TabTarget::Last),
            Some(_) => Some(TabTarget::Number(args.parse_last_int("index")?)),
        }),
        "tab-move" => Command::TabMove(match args.optional() {
            None => None,
            Some("+") => Some(TabMoveTarget::Relative(1)),
            Some("-") => Some(TabMoveTarget::Relative(-1)),
            Some("start") => Some(TabMoveTarget::Start),
            Some("end") => Some(TabMoveTarget::End),
            Some(_) => Some(TabMoveTarget::Absolute(args.parse_last_int("index")?)),
        }),
        "tab-only" => Command::TabOnly {
            force: args.flag(&["-f", "--force"]).is_some(),
        },
        "undo" => Command::Undo,
        "hint" => {
            let rapid = args.flag(&["-r", "--rapid"]).is_some();
            let group = match args.optional() {
                Some(g) => g.parse::<HintGroup>().map_err(|e| args.error(e))?,
                None => HintGroup::All,
            };
            let target = match args.optional() {
                Some(t) => t.parse::<HintTarget>().map_err(|e| args.error(e))?,
                None => HintTarget::Normal,
            };
            let fill = args.rest();
            if target == HintTarget::Fill && fill.is_empty() {
                return Err(args.error("the fill target needs command text"));
            }
            Command::Hint(HintRequest {
                group,
                target,
                rapid,
                fill: (!fill.is_empty()).then(|| fill.to_string()),
            })
        }
        "yank" => Command::Yank(match args.optional() {
            None | Some("url") => YankWhat::Url,
            Some("title") => YankWhat::Title,
            Some("domain") => YankWhat::Domain,
            Some(other) => return Err(args.error(format!("cannot yank {other:?}"))),
        }),
        "set" => {
            let name = args.optional().map(String::from);
            let value = args.rest();
            Command::Set {
                name,
                value: (!value.is_empty()).then(|| value.to_string()),
            }
        }
        "bind" => {
            let mode = args.mode()?;
            let keys = args.optional().map(String::from);
            let command = args.rest();
            Command::Bind {
                mode,
                keys,
                command: (!command.is_empty()).then(|| command.to_string()),
            }
        }
        "unbind" => {
            let mode = args.mode()?;
            Command::Unbind {
                mode,
                keys: args.required("keys")?.to_string(),
            }
        }
        "config-source" => Command::ConfigSource,
        "help" => {
            let tab = args.flag(&["-t", "--tab"]).is_some();
            let topic = args.rest();
            Command::Help {
                tab,
                topic: (!topic.is_empty()).then(|| topic.to_string()),
            }
        }
        "version" => Command::Version,
        "changelog" => Command::Changelog {
            tab: args.flag(&["-t", "--tab"]).is_some(),
        },
        "quickmark-add" => {
            let url = args.required("url")?.to_string();
            let name = args.rest();
            if name.is_empty() {
                return Err(args.error("missing argument: name"));
            }
            Command::QuickmarkAdd {
                url,
                name: name.to_string(),
            }
        }
        "quickmark-load" => {
            let (target, _) = args.open_target();
            let name = args.rest();
            if name.is_empty() {
                return Err(args.error("missing argument: name"));
            }
            Command::QuickmarkLoad {
                target,
                name: name.to_string(),
            }
        }
        "quickmark-del" => {
            let name = args.rest();
            Command::QuickmarkDel {
                name: (!name.is_empty()).then(|| name.to_string()),
            }
        }
        "bookmark-add" => {
            let url = args.optional().map(String::from);
            let title = args.rest();
            Command::BookmarkAdd {
                url,
                title: (!title.is_empty()).then(|| title.to_string()),
            }
        }
        "bookmark-load" => {
            let (target, _) = args.open_target();
            Command::BookmarkLoad {
                target,
                url: args.required("url")?.to_string(),
            }
        }
        "bookmark-del" => Command::BookmarkDel {
            url: args.optional().map(String::from),
        },
        "session-save" => {
            let name = args.rest();
            Command::SessionSave {
                name: (!name.is_empty()).then(|| name.to_string()),
            }
        }
        which @ ("session-load" | "session-delete") => {
            let name = args.rest();
            if name.is_empty() {
                return Err(args.error("missing argument: name"));
            }
            let name = name.to_string();
            if which == "session-load" {
                Command::SessionLoad { name }
            } else {
                Command::SessionDelete { name }
            }
        }
        "download" => Command::Download {
            url: args.optional().map(String::from),
        },
        "download-cancel" => Command::DownloadCancel,
        "download-open" => Command::DownloadOpen,
        "download-clear" => Command::DownloadClear,
        "adblock-update" => Command::AdblockUpdate,
        "history-clear" => Command::HistoryClear {
            force: args.flag(&["-f", "--force"]).is_some(),
        },
        "completion-item-focus" => Command::CompletionFocus(match args.required("direction")? {
            "next" => FocusDirection::Next,
            "prev" => FocusDirection::Prev,
            other => return Err(args.error(format!("expected next or prev, got {other:?}"))),
        }),
        "prompt-accept" => {
            let save = args.flag(&["-s", "--save"]).is_some();
            let value = match args.optional() {
                None => None,
                Some("yes") => Some(true),
                Some("no") => Some(false),
                Some(other) => return Err(args.error(format!("expected yes or no, got {other:?}"))),
            };
            Command::PromptAccept { value, save }
        }
        "quit" => Command::Quit {
            save: args.flag(&["-s", "--save"]).is_some(),
        },
        name => match parse_readline(name) {
            Some(rl) => Command::Readline(rl),
            None => return Err(CommandError::Unknown(name.to_string())),
        },
    };
    args.finish()?;
    Ok(cmd)
}

fn parse_readline(name: &str) -> Option<Readline> {
    Some(match name.strip_prefix("rl-")? {
        "backward-char" => Readline::BackwardChar,
        "forward-char" => Readline::ForwardChar,
        "beginning-of-line" => Readline::BeginningOfLine,
        "end-of-line" => Readline::EndOfLine,
        "backward-delete-char" => Readline::BackwardDeleteChar,
        "delete-char" => Readline::DeleteChar,
        "unix-line-discard" => Readline::UnixLineDiscard,
        "kill-line" => Readline::KillLine,
        "rubout" => Readline::Rubout,
        "filename-rubout" => Readline::FilenameRubout,
        _ => return None,
    })
}

struct Args<'a> {
    command: &'a str,
    rest: &'a str,
    last: Option<&'a str>,
}

impl<'a> Args<'a> {
    fn new(command: &'a str, rest: &'a str) -> Self {
        Self {
            command,
            rest,
            last: None,
        }
    }

    fn error(&self, message: impl Into<String>) -> CommandError {
        CommandError::BadArgs {
            command: self.command.to_string(),
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.rest.split_whitespace().next()
    }

    fn advance(&mut self) -> Option<&'a str> {
        let token = self.peek()?;
        self.rest = self.rest.trim_start()[token.len()..].trim_start();
        self.last = Some(token);
        Some(token)
    }

    /// Consume the next token if it is one of `names`.
    fn flag(&mut self, names: &[&str]) -> Option<&'a str> {
        let token = self.peek()?;
        names.contains(&token).then(|| self.advance()).flatten()
    }

    fn optional(&mut self) -> Option<&'a str> {
        self.advance()
    }

    fn required(&mut self, name: &str) -> Result<&'a str, CommandError> {
        self.advance()
            .ok_or_else(|| self.error(format!("missing argument: {name}")))
    }

    fn number(&mut self, name: &str) -> Result<f64, CommandError> {
        self.required(name)?;
        self.parse_last_number(name)
    }

    fn parse_last_number(&self, name: &str) -> Result<f64, CommandError> {
        let token = self.last.unwrap_or_default();
        token
            .parse::<f64>()
            .map_err(|_| self.error(format!("{name} must be a number, got {token:?}")))
    }

    /// `-t`/`-b`/`-w`/`-p`/`-r` flags, ending at `--`. Returns the target and `related`.
    fn open_target(&mut self) -> (OpenTarget, bool) {
        let mut target = OpenTarget::Current;
        let mut related = false;
        while let Some(flag) = self.flag(&[
            "--",
            "-r",
            "--related",
            "-t",
            "--tab",
            "-b",
            "--bg",
            "-w",
            "--window",
            "-p",
            "--private",
        ]) {
            match flag {
                "--" => break,
                "-r" | "--related" => related = true,
                "-t" | "--tab" => target = OpenTarget::Tab,
                "-b" | "--bg" => target = OpenTarget::Background,
                "-w" | "--window" => target = OpenTarget::Window,
                _ => target = OpenTarget::Private,
            }
        }
        (target, related)
    }

    /// An optional `--mode m` / `-m m` flag, defaulting to normal mode.
    fn mode(&mut self) -> Result<Mode, CommandError> {
        if self.flag(&["-m", "--mode"]).is_none() {
            return Ok(Mode::Normal);
        }
        let name = self.required("mode")?;
        name.parse::<Mode>().map_err(|e| self.error(e))
    }

    fn parse_last_int(&self, name: &str) -> Result<i64, CommandError> {
        let token = self.last.unwrap_or_default();
        token
            .parse::<i64>()
            .map_err(|_| self.error(format!("{name} must be an integer, got {token:?}")))
    }

    /// Take the remaining raw text, for commands like `open` whose last argument may contain spaces.
    fn rest(&mut self) -> &'a str {
        std::mem::take(&mut self.rest).trim()
    }

    fn finish(&self) -> Result<(), CommandError> {
        match self.peek() {
            Some(extra) => Err(self.error(format!("unexpected argument: {extra}"))),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_with_flags_and_spaces() {
        assert_eq!(
            parse(":open -t rust lang book").unwrap(),
            Command::Open {
                target: OpenTarget::Tab,
                related: false,
                url: Some("rust lang book".into())
            }
        );
        assert_eq!(
            parse("open").unwrap(),
            Command::Open {
                target: OpenTarget::Current,
                related: false,
                url: None
            }
        );
    }

    #[test]
    fn parses_numeric_args() {
        assert_eq!(
            parse("scroll-page 0 -0.5").unwrap(),
            Command::ScrollPage { x: 0.0, y: -0.5 }
        );
        assert_eq!(
            parse("scroll-to-perc --horizontal 100").unwrap(),
            Command::ScrollToPerc {
                perc: Some(100.0),
                horizontal: true
            }
        );
        assert_eq!(
            parse("scroll-to-perc").unwrap(),
            Command::ScrollToPerc {
                perc: None,
                horizontal: false
            }
        );
    }

    #[test]
    fn parses_cmd_set_text() {
        assert_eq!(
            parse("cmd-set-text -s :open").unwrap(),
            Command::CmdSetText {
                text: ":open".into(),
                append_space: true
            }
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(
            parse("frobnicate"),
            Err(CommandError::Unknown("frobnicate".into()))
        );
        assert!(matches!(
            parse("scroll sideways"),
            Err(CommandError::BadArgs { .. })
        ));
        assert!(matches!(
            parse("scroll-page 0 lots"),
            Err(CommandError::BadArgs { .. })
        ));
        assert!(matches!(
            parse("back extra"),
            Err(CommandError::BadArgs { .. })
        ));
        assert!(matches!(
            parse("mode-enter sideways"),
            Err(CommandError::BadArgs { .. })
        ));
    }

    #[test]
    fn parses_tab_commands() {
        assert_eq!(
            parse("open -t -r https://x.org").unwrap(),
            Command::Open {
                target: OpenTarget::Tab,
                related: true,
                url: Some("https://x.org".into())
            }
        );
        assert_eq!(parse("tab-focus").unwrap(), Command::TabFocus(None));
        assert_eq!(
            parse("tab-focus -1").unwrap(),
            Command::TabFocus(Some(TabTarget::Number(-1)))
        );
        assert_eq!(
            parse("tab-focus last").unwrap(),
            Command::TabFocus(Some(TabTarget::Last))
        );
        assert_eq!(
            parse("tab-move +").unwrap(),
            Command::TabMove(Some(TabMoveTarget::Relative(1)))
        );
        assert_eq!(
            parse("tab-move 2").unwrap(),
            Command::TabMove(Some(TabMoveTarget::Absolute(2)))
        );
        assert_eq!(
            parse("tab-move end").unwrap(),
            Command::TabMove(Some(TabMoveTarget::End))
        );
        assert!(matches!(
            parse("tab-focus first"),
            Err(CommandError::BadArgs { .. })
        ));
        assert!(matches!(
            parse("tab-move 1.5"),
            Err(CommandError::BadArgs { .. })
        ));
    }

    #[test]
    fn parses_hint_and_yank() {
        assert_eq!(
            parse("hint").unwrap(),
            Command::Hint(HintRequest {
                group: HintGroup::All,
                target: HintTarget::Normal,
                rapid: false,
                fill: None
            })
        );
        assert_eq!(
            parse("hint --rapid links tab-bg").unwrap(),
            Command::Hint(HintRequest {
                group: HintGroup::Links,
                target: HintTarget::TabBg,
                rapid: true,
                fill: None
            })
        );
        assert_eq!(
            parse("hint links fill :open -t {hint-url}").unwrap(),
            Command::Hint(HintRequest {
                group: HintGroup::Links,
                target: HintTarget::Fill,
                rapid: false,
                fill: Some(":open -t {hint-url}".into())
            })
        );
        assert!(matches!(
            parse("hint links fill"),
            Err(CommandError::BadArgs { .. })
        ));
        assert_eq!(
            parse("open -t -- -t is text").unwrap(),
            Command::Open {
                target: OpenTarget::Tab,
                related: false,
                url: Some("-t is text".into())
            }
        );
        assert!(matches!(
            parse("hint everything"),
            Err(CommandError::BadArgs { .. })
        ));
        assert_eq!(parse("yank title").unwrap(), Command::Yank(YankWhat::Title));
        assert_eq!(parse("yank").unwrap(), Command::Yank(YankWhat::Url));
    }

    #[test]
    fn parses_config_commands() {
        assert_eq!(
            parse("set url.start_pages [\"a\", \"b\"]").unwrap(),
            Command::Set {
                name: Some("url.start_pages".into()),
                value: Some("[\"a\", \"b\"]".into())
            }
        );
        assert_eq!(
            parse("set").unwrap(),
            Command::Set {
                name: None,
                value: None
            }
        );
        assert_eq!(
            parse("bind --mode insert <Ctrl-e> open-editor ;; x").unwrap(),
            Command::Bind {
                mode: Mode::Insert,
                keys: Some("<Ctrl-e>".into()),
                command: Some("open-editor ;; x".into())
            }
        );
        assert_eq!(
            parse("unbind d").unwrap(),
            Command::Unbind {
                mode: Mode::Normal,
                keys: "d".into()
            }
        );
        assert!(matches!(
            parse("bind --mode sideways x quit"),
            Err(CommandError::BadArgs { .. })
        ));
    }

    #[test]
    fn parses_storage_commands() {
        assert_eq!(
            parse("quickmark-add https://x.org/ my site").unwrap(),
            Command::QuickmarkAdd {
                url: "https://x.org/".into(),
                name: "my site".into()
            }
        );
        assert_eq!(
            parse("quickmark-load -t my site").unwrap(),
            Command::QuickmarkLoad {
                target: OpenTarget::Tab,
                name: "my site".into()
            }
        );
        assert_eq!(
            parse("bookmark-add").unwrap(),
            Command::BookmarkAdd {
                url: None,
                title: None
            }
        );
        assert_eq!(
            parse("bookmark-add https://x.org/ The X").unwrap(),
            Command::BookmarkAdd {
                url: Some("https://x.org/".into()),
                title: Some("The X".into())
            }
        );
        assert_eq!(
            parse("session-save").unwrap(),
            Command::SessionSave { name: None }
        );
        assert_eq!(
            parse("session-load work").unwrap(),
            Command::SessionLoad {
                name: "work".into()
            }
        );
        assert_eq!(parse("quit --save").unwrap(), Command::Quit { save: true });
        assert_eq!(
            parse("history-clear").unwrap(),
            Command::HistoryClear { force: false }
        );
        assert!(matches!(
            parse("quickmark-add https://x.org/"),
            Err(CommandError::BadArgs { .. })
        ));
        assert!(matches!(
            parse("completion-item-focus up"),
            Err(CommandError::BadArgs { .. })
        ));
    }

    #[test]
    fn chains_commands() {
        assert_eq!(
            parse_line("back ;; reload -f").unwrap(),
            vec![Command::Back, Command::Reload { force: true }]
        );
    }

    #[test]
    fn every_spec_parses() {
        let needs_args = [
            "scroll",
            "scroll-page",
            "mode-enter",
            "cmd-set-text",
            "unbind",
            "quickmark-add",
            "quickmark-load",
            "bookmark-load",
            "session-load",
            "session-delete",
            "completion-item-focus",
            "prompt-accept",
        ];
        for spec in COMMANDS.iter().filter(|s| !needs_args.contains(&s.name)) {
            assert!(parse(spec.name).is_ok(), "{} failed to parse", spec.name);
        }
    }
}
