use serde::Serialize;

use crate::cmdline::{History, LineEditor};
use crate::command::{self, COMMANDS, Command};
use crate::key::{Key, KeyCode, format_sequence};
use crate::keymap::{Keymap, Lookup};
use crate::mode::Mode;

/// Something the browser layer has to act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Run {
        command: Command,
        count: Option<u32>,
    },
    ModeChanged {
        from: Mode,
        to: Mode,
    },
}

#[derive(Debug, Default, PartialEq)]
pub struct KeyOutcome {
    /// True when the key must not reach the web page.
    pub consumed: bool,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Message {
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandLineView {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub name: &'static str,
    pub description: &'static str,
}

/// Everything the status bar needs to draw itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatusView {
    pub mode: Mode,
    pub command_line: Option<CommandLineView>,
    pub keystring: String,
    pub message: Option<Message>,
}

/// The modal key-handling state machine. It has no knowledge of CEF.
pub struct Engine {
    keymap: Keymap,
    mode: Mode,
    pending: Vec<Key>,
    count: Option<u32>,
    cmdline: LineEditor,
    history: History,
    message: Option<Message>,
    message_generation: u64,
    url: String,
    dirty: bool,
}

impl Engine {
    pub fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            cmdline: LineEditor::default(),
            history: History::default(),
            message: None,
            message_generation: 0,
            url: String::new(),
            dirty: true,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Returns whether the status view changed since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn status(&self) -> StatusView {
        let mut keystring = self.count.map(|c| c.to_string()).unwrap_or_default();
        keystring.push_str(&format_sequence(&self.pending));
        StatusView {
            mode: self.mode,
            command_line: (self.mode == Mode::Command).then(|| CommandLineView {
                text: self.cmdline.text().to_string(),
                cursor: self.cmdline.cursor(),
            }),
            keystring,
            message: self.message.clone(),
        }
    }

    /// Command-name completion while the first word is being typed.
    pub fn completions(&self) -> Vec<Completion> {
        if self.mode != Mode::Command {
            return Vec::new();
        }
        let Some(typed) = self.cmdline.text().strip_prefix(':') else {
            return Vec::new();
        };
        if typed.contains(char::is_whitespace) {
            return Vec::new();
        }
        COMMANDS
            .iter()
            .filter(|c| !c.hidden && c.name.starts_with(typed))
            .map(|c| Completion {
                name: c.name,
                description: c.description,
            })
            .collect()
    }

    pub fn set_url(&mut self, url: &str) {
        self.url = url.to_string();
    }

    /// Returns an id for [`Engine::expire_message`], so a timer started for
    /// one message cannot clear a newer one.
    pub fn show_message(&mut self, level: Level, text: impl Into<String>) -> u64 {
        self.message = Some(Message {
            level,
            text: text.into(),
        });
        self.message_generation += 1;
        self.dirty = true;
        self.message_generation
    }

    pub fn message_generation(&self) -> u64 {
        self.message_generation
    }

    pub fn expire_message(&mut self, generation: u64) {
        if generation == self.message_generation && self.message.take().is_some() {
            self.dirty = true;
        }
    }

    /// Called when keyboard focus moves onto or off an editable element.
    pub fn focus_changed(&mut self, editable: bool) -> Vec<Effect> {
        let mut effects = Vec::new();
        match (editable, self.mode) {
            (true, Mode::Normal) => self.set_mode(Mode::Insert, &mut effects),
            (false, Mode::Insert) => self.set_mode(Mode::Normal, &mut effects),
            _ => {}
        }
        effects
    }

    pub fn load_started(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.message.take().is_some() {
            self.dirty = true;
        }
        if self.mode == Mode::Insert {
            self.set_mode(Mode::Normal, &mut effects);
        }
        effects
    }

    pub fn handle_key(&mut self, key: Key) -> KeyOutcome {
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Command => self.handle_command(key),
            Mode::Insert | Mode::Passthrough => self.handle_passthrough(key),
        }
    }

    /// Run a command string such as `scroll down ;; reload`.
    pub fn execute_str(&mut self, line: &str, count: Option<u32>) -> Vec<Effect> {
        let line = line.replace("{url}", &self.url);
        let mut effects = Vec::new();
        match command::parse_line(&line) {
            Ok(commands) => {
                for cmd in commands {
                    self.execute(cmd, count, &mut effects);
                }
            }
            Err(e) => {
                self.show_message(Level::Error, e.to_string());
            }
        }
        effects
    }

    fn handle_normal(&mut self, key: Key) -> KeyOutcome {
        if self.message.take().is_some() {
            self.dirty = true;
        }
        if self.pending.is_empty()
            && let Some(d) = key.digit()
            && (d != 0 || self.count.is_some())
        {
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d));
            self.dirty = true;
            return consumed(Vec::new());
        }
        self.pending.push(key);
        self.dirty = true;
        match self.keymap.lookup(Mode::Normal, &self.pending) {
            Lookup::Exact(cmd) => {
                let cmd = cmd.to_string();
                let count = self.count.take();
                self.pending.clear();
                consumed(self.execute_str(&cmd, count))
            }
            Lookup::Partial => consumed(Vec::new()),
            Lookup::None => {
                let had_prefix = self.pending.len() > 1 || self.count.is_some();
                self.pending.clear();
                self.count = None;
                KeyOutcome {
                    consumed: had_prefix || !is_forwardable(key),
                    effects: Vec::new(),
                }
            }
        }
    }

    fn handle_command(&mut self, key: Key) -> KeyOutcome {
        if let Lookup::Exact(cmd) = self.keymap.lookup(Mode::Command, &[key]) {
            let cmd = cmd.to_string();
            return consumed(self.execute_str(&cmd, None));
        }
        if let Some(c) = key.text() {
            self.cmdline.insert(c);
            self.history.reset();
            self.dirty = true;
        }
        consumed(Vec::new())
    }

    fn handle_passthrough(&mut self, key: Key) -> KeyOutcome {
        match self.keymap.lookup(self.mode, &[key]) {
            Lookup::Exact(cmd) => {
                let cmd = cmd.to_string();
                consumed(self.execute_str(&cmd, None))
            }
            _ => KeyOutcome::default(),
        }
    }

    fn execute(&mut self, cmd: Command, count: Option<u32>, effects: &mut Vec<Effect>) {
        match cmd {
            Command::ModeEnter(mode) => self.set_mode(mode, effects),
            Command::ModeLeave => self.set_mode(Mode::Normal, effects),
            Command::CmdSetText { text, append_space } => {
                let text = if append_space {
                    format!("{text} ")
                } else {
                    text
                };
                self.cmdline.set(&text);
                self.set_mode(Mode::Command, effects);
                self.dirty = true;
            }
            Command::CommandAccept => {
                let text = self.cmdline.text().to_string();
                self.history.push(&text);
                self.set_mode(Mode::Normal, effects);
                let line = text.strip_prefix(':').unwrap_or(&text);
                effects.extend(self.execute_str(line, None));
            }
            Command::CommandHistoryPrev => {
                if let Some(entry) = self.history.older(self.cmdline.text()) {
                    let entry = entry.to_string();
                    self.cmdline.set(&entry);
                    self.dirty = true;
                }
            }
            Command::CommandHistoryNext => {
                if let Some(entry) = self.history.newer() {
                    self.cmdline.set(&entry);
                    self.dirty = true;
                }
            }
            Command::Readline(action) => {
                self.cmdline.apply(action);
                self.history.reset();
                self.dirty = true;
                if self.mode == Mode::Command && self.cmdline.text().is_empty() {
                    self.set_mode(Mode::Normal, effects);
                }
            }
            Command::ClearKeychain => {
                self.pending.clear();
                self.count = None;
                self.dirty = true;
            }
            command => effects.push(Effect::Run { command, count }),
        }
    }

    fn set_mode(&mut self, mode: Mode, effects: &mut Vec<Effect>) {
        if mode == self.mode {
            return;
        }
        if self.mode == Mode::Command {
            self.cmdline.clear();
            self.history.reset();
        }
        if mode == Mode::Command && self.cmdline.text().is_empty() {
            self.cmdline.set(":");
        }
        self.pending.clear();
        self.count = None;
        effects.push(Effect::ModeChanged {
            from: self.mode,
            to: mode,
        });
        self.mode = mode;
        self.dirty = true;
    }
}

fn consumed(effects: Vec<Effect>) -> KeyOutcome {
    KeyOutcome {
        consumed: true,
        effects,
    }
}

/// qutebrowser's `input.forward_unbound_keys = auto`: unbound keys reach the
/// page unless they are plain alphanumerics.
fn is_forwardable(key: Key) -> bool {
    !matches!(key.code, KeyCode::Char(c) if c.is_alphanumeric() && key.mods.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Direction, OpenTarget};

    fn engine() -> Engine {
        Engine::new(Keymap::defaults())
    }

    fn press(e: &mut Engine, keys: &str) -> Vec<KeyOutcome> {
        Key::parse_sequence(keys)
            .unwrap()
            .into_iter()
            .map(|k| e.handle_key(k))
            .collect()
    }

    fn runs(outcomes: &[KeyOutcome]) -> Vec<(Command, Option<u32>)> {
        outcomes
            .iter()
            .flat_map(|o| &o.effects)
            .filter_map(|e| match e {
                Effect::Run { command, count } => Some((command.clone(), *count)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn single_key_binding_runs_command() {
        let mut e = engine();
        let out = press(&mut e, "j");
        assert!(out[0].consumed);
        assert_eq!(runs(&out), vec![(Command::Scroll(Direction::Down), None)]);
    }

    #[test]
    fn count_and_multi_key_sequence() {
        let mut e = engine();
        let out = press(&mut e, "5g");
        assert_eq!(e.status().keystring, "5g");
        assert!(runs(&out).is_empty());
        let out = press(&mut e, "g");
        assert_eq!(
            runs(&out),
            vec![(
                Command::ScrollToPerc {
                    perc: Some(0.0),
                    horizontal: false
                },
                Some(5)
            )]
        );
        assert_eq!(e.status().keystring, "");
    }

    #[test]
    fn zero_without_count_is_a_binding() {
        let mut e = engine();
        let out = press(&mut e, "0");
        assert_eq!(
            runs(&out),
            vec![(
                Command::ScrollToPerc {
                    perc: Some(0.0),
                    horizontal: true
                },
                None
            )]
        );
        let out = press(&mut e, "10j");
        assert_eq!(
            runs(&out),
            vec![(Command::Scroll(Direction::Down), Some(10))]
        );
    }

    #[test]
    fn unbound_keys_forwarding() {
        let mut e = engine();
        assert!(press(&mut e, "x")[0].consumed);
        assert!(!press(&mut e, "<Space>")[0].consumed);
        assert!(!press(&mut e, "<F1>")[0].consumed);
        // A broken sequence is swallowed rather than leaking its last key to the page.
        let out = press(&mut e, "g<Space>");
        assert!(out[1].consumed);
    }

    #[test]
    fn insert_mode_passes_keys_until_escape() {
        let mut e = engine();
        press(&mut e, "i");
        assert_eq!(e.mode(), Mode::Insert);
        assert!(!press(&mut e, "j")[0].consumed);
        assert!(press(&mut e, "<Escape>")[0].consumed);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn passthrough_needs_shift_escape() {
        let mut e = engine();
        press(&mut e, "<Ctrl-v>");
        assert_eq!(e.mode(), Mode::Passthrough);
        assert!(!press(&mut e, "<Escape>")[0].consumed);
        press(&mut e, "<Shift-Escape>");
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn command_line_open() {
        let mut e = engine();
        press(&mut e, "o");
        assert_eq!(e.mode(), Mode::Command);
        assert_eq!(e.status().command_line.unwrap().text, ":open ");
        let out = press(&mut e, "example.com<Return>");
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(
            runs(&out),
            vec![(
                Command::Open {
                    target: OpenTarget::Current,
                    url: Some("example.com".into())
                },
                None
            )]
        );
    }

    #[test]
    fn url_substitution() {
        let mut e = engine();
        e.set_url("https://example.com/");
        press(&mut e, "go");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open https://example.com/"
        );
    }

    #[test]
    fn backspace_on_empty_leaves_command_mode() {
        let mut e = engine();
        press(&mut e, ":");
        press(&mut e, "<Backspace>");
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn bad_command_shows_error() {
        let mut e = engine();
        press(&mut e, ":nope<Return>");
        let msg = e.status().message.unwrap();
        assert_eq!(msg.level, Level::Error);
        assert!(msg.text.contains("no such command"));
    }

    #[test]
    fn stale_message_timer_keeps_newer_message() {
        let mut e = engine();
        let first = e.show_message(Level::Info, "one");
        e.show_message(Level::Info, "two");
        e.expire_message(first);
        assert_eq!(e.status().message.unwrap().text, "two");
        e.expire_message(e.message_generation());
        assert!(e.status().message.is_none());
    }

    #[test]
    fn history_recall() {
        let mut e = engine();
        press(&mut e, ":back<Return>");
        press(&mut e, ":<Up>");
        assert_eq!(e.status().command_line.unwrap().text, ":back");
    }

    #[test]
    fn completion_filters_by_prefix() {
        let mut e = engine();
        press(&mut e, ":scr");
        let names: Vec<_> = e.completions().iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["scroll", "scroll-page", "scroll-to-perc"]);
        press(&mut e, "oll ");
        assert!(e.completions().is_empty());
    }

    #[test]
    fn auto_insert_mode_on_focus() {
        let mut e = engine();
        e.focus_changed(true);
        assert_eq!(e.mode(), Mode::Insert);
        e.focus_changed(false);
        assert_eq!(e.mode(), Mode::Normal);
        press(&mut e, ":");
        e.focus_changed(true);
        assert_eq!(e.mode(), Mode::Command);
    }
}
