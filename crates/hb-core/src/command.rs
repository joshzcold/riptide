use thiserror::Error;

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
    TabClose,
    TabNext,
    TabPrev,
    TabFocus(Option<TabTarget>),
    TabMove(Option<TabMoveTarget>),
    TabOnly,
    Undo,
    Quit,
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
    spec("tab-close", "Close the current tab"),
    spec("tab-next", "Switch to the next tab"),
    spec("tab-prev", "Switch to the previous tab"),
    spec("tab-focus", "Select a tab by number, or 'last'"),
    spec(
        "tab-move",
        "Move the current tab: +, -, start, end or a number",
    ),
    spec("tab-only", "Close all tabs except the current one"),
    spec("undo", "Re-open the last closed tab"),
    spec("quit", "Quit the browser"),
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
            let mut target = OpenTarget::Current;
            let mut related = false;
            while let Some(flag) = args.flag(&[
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
                target = match flag {
                    "-r" | "--related" => {
                        related = true;
                        continue;
                    }
                    "-t" | "--tab" => OpenTarget::Tab,
                    "-b" | "--bg" => OpenTarget::Background,
                    "-w" | "--window" => OpenTarget::Window,
                    _ => OpenTarget::Private,
                };
            }
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
        "tab-close" => Command::TabClose,
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
        "tab-only" => Command::TabOnly,
        "undo" => Command::Undo,
        "quit" | "q" | "qa" | "wq" => Command::Quit,
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
    fn chains_commands() {
        assert_eq!(
            parse_line("back ;; reload -f").unwrap(),
            vec![Command::Back, Command::Reload { force: true }]
        );
    }

    #[test]
    fn every_spec_parses() {
        let needs_args = ["scroll", "scroll-page", "mode-enter", "cmd-set-text"];
        for spec in COMMANDS.iter().filter(|s| !needs_args.contains(&s.name)) {
            assert!(parse(spec.name).is_ok(), "{} failed to parse", spec.name);
        }
    }
}
