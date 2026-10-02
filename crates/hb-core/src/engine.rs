use serde::Serialize;

use std::collections::VecDeque;

use crate::cmdline::{History, LineEditor};
use crate::command::{self, Command, FocusDirection};
use crate::completion::{self, Completion, CompletionKind, CompletionView};
use crate::config::ConfigOp;
use crate::hints::{HintInput, HintItem, HintRequest, HintSession, HintTarget};
use crate::key::{Key, KeyCode, format_sequence};
use crate::keymap::{Keymap, Lookup};
use crate::mode::Mode;
use crate::prompt::{Prompt, PromptAnswer, PromptKind, PromptView};
use crate::settings::{self, Settings, Value};

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
    /// Draw these hint labels, one per item, in page order.
    ShowHints {
        labels: Vec<String>,
    },
    /// Only show labels starting with `typed`.
    FilterHints {
        typed: String,
    },
    /// Act on the chosen element.
    FollowHint {
        index: usize,
        url: Option<String>,
        target: HintTarget,
    },
    /// A `:set`/`:bind`/`:unbind` succeeded; the host persists it.
    ConfigChanged(ConfigOp),
    /// The user answered (or cancelled) the prompt with this id.
    PromptAnswered {
        id: u64,
        answer: PromptAnswer,
    },
}

/// Completion results for one command line text, plus Tab-cycling state.
struct CompletionState {
    /// The text the items were computed for.
    base: String,
    /// The text after inserting the selected item, so cycling doesn't re-query.
    inserted: Option<String>,
    view: CompletionView,
}

/// Aliases may refer to other aliases, but not endlessly.
const MAX_ALIAS_DEPTH: usize = 10;

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
    settings: Settings,
    mode: Mode,
    pending: Vec<Key>,
    count: Option<u32>,
    cmdline: LineEditor,
    history: History,
    message: Option<Message>,
    message_generation: u64,
    url: String,
    clipboard: Option<Box<dyn Fn() -> Option<String>>>,
    completion_source: Option<completion::Source>,
    completion: Option<CompletionState>,
    hints: Option<HintSession>,
    prompts: VecDeque<Prompt>,
    prompt_editor: LineEditor,
    /// The mode to return to once the prompt queue is empty.
    mode_before_prompt: Mode,
    dirty: bool,
}

impl Engine {
    pub fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            settings: Settings::default(),
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            cmdline: LineEditor::default(),
            history: History::default(),
            message: None,
            message_generation: 0,
            url: String::new(),
            clipboard: None,
            completion_source: None,
            completion: None,
            hints: None,
            prompts: VecDeque::new(),
            prompt_editor: LineEditor::default(),
            mode_before_prompt: Mode::Normal,
            dirty: true,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Back to the built-in settings and bindings, before re-reading config.
    pub fn reset_config(&mut self) {
        self.keymap = Keymap::defaults();
        self.settings = Settings::default();
    }

    /// Apply a change from a config file. Values must already be validated.
    pub fn apply_config(&mut self, op: &ConfigOp) -> Result<(), String> {
        match op {
            ConfigOp::Set { name, value } => self.settings.set(name, value.clone()),
            ConfigOp::Bind {
                mode,
                keys,
                command,
            } => {
                self.check_command(command)
                    .map_err(|e| format!("{keys}: {e}"))?;
                self.keymap
                    .bind(*mode, keys, command)
                    .map_err(|e| e.to_string())
            }
            ConfigOp::Unbind { mode, keys } => self
                .keymap
                .unbind(*mode, keys)
                .map(|_| ())
                .map_err(|e| e.to_string()),
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
        if let Some(hints) = &self.hints {
            keystring.push_str(&hints.typed);
        }
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

    /// Completions for the command line, recomputed only when the text changes.
    pub fn completions(&mut self) -> CompletionView {
        if self.mode != Mode::Command {
            self.completion = None;
            return CompletionView::default();
        }
        let text = self.cmdline.text();
        let fresh = match &self.completion {
            Some(state) => state.base != text && state.inserted.as_deref() != Some(text),
            None => true,
        };
        if fresh {
            let items = completion::compute(text, self.completion_source.as_ref());
            self.completion = Some(CompletionState {
                base: text.to_string(),
                inserted: None,
                view: CompletionView {
                    items,
                    selected: None,
                },
            });
        }
        self.completion
            .as_ref()
            .map(|s| s.view.clone())
            .unwrap_or_default()
    }

    /// Lets `:open`, `:quickmark-load` and friends complete from storage.
    pub fn set_completion_source(
        &mut self,
        source: impl Fn(CompletionKind, &str) -> Vec<Completion> + 'static,
    ) {
        self.completion_source = Some(Box::new(source));
    }

    fn focus_completion(&mut self, forward: bool) {
        self.completions();
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        let len = state.view.items.len();
        if len == 0 {
            return;
        }
        let next = match (state.view.selected, forward) {
            (None, true) => 0,
            (None, false) => len - 1,
            (Some(i), true) => (i + 1) % len,
            (Some(i), false) => (i + len - 1) % len,
        };
        state.view.selected = Some(next);
        let text = completion::insert(&state.base, &state.view.items[next]);
        state.inserted = Some(text.clone());
        self.cmdline.set(&text);
        self.dirty = true;
    }

    /// Queue a question; it shows once the ones before it are answered.
    pub fn push_prompt(&mut self, prompt: Prompt) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.prompts.push_back(prompt);
        self.dirty = true;
        if self.prompts.len() == 1 {
            self.activate_prompt(&mut effects);
        }
        effects
    }

    /// Withdraw a prompt that no longer applies (its tab closed or navigated).
    pub fn cancel_prompt(&mut self, id: u64) -> Vec<Effect> {
        let mut effects = Vec::new();
        let was_active = self.prompts.front().is_some_and(|p| p.id == id);
        self.prompts.retain(|p| p.id != id);
        self.dirty = true;
        if was_active {
            self.activate_prompt(&mut effects);
        }
        effects
    }

    pub fn prompt_view(&self) -> Option<PromptView> {
        let prompt = self.prompts.front()?;
        let (kind, input) = match &prompt.kind {
            PromptKind::Text { masked: true, .. } => (
                "text",
                "*".repeat(self.prompt_editor.text().chars().count()),
            ),
            PromptKind::Text { .. } => ("text", self.prompt_editor.text().to_string()),
            PromptKind::YesNo { .. } => ("yesno", String::new()),
            PromptKind::Alert => ("alert", String::new()),
        };
        Some(PromptView {
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            kind,
            input,
            cursor: self.prompt_editor.cursor(),
            hint: prompt.hint(),
            queued: self.prompts.len() - 1,
        })
    }

    /// Show the front of the queue, or go back to the earlier mode if empty.
    fn activate_prompt(&mut self, effects: &mut Vec<Effect>) {
        let Some(prompt) = self.prompts.front() else {
            let mode = self.mode_before_prompt;
            self.set_mode(mode, effects);
            return;
        };
        let (mode, text) = match &prompt.kind {
            PromptKind::Text { default, .. } => (Mode::Prompt, default.clone()),
            _ => (Mode::YesNo, String::new()),
        };
        if !matches!(self.mode, Mode::Prompt | Mode::YesNo) {
            self.mode_before_prompt = match self.mode {
                Mode::Command | Mode::Hint => Mode::Normal,
                other => other,
            };
        }
        self.prompt_editor.set(&text);
        self.set_mode(mode, effects);
        self.dirty = true;
    }

    fn answer_prompt(&mut self, answer: PromptAnswer, effects: &mut Vec<Effect>) {
        let Some(prompt) = self.prompts.pop_front() else {
            return;
        };
        effects.push(Effect::PromptAnswered {
            id: prompt.id,
            answer,
        });
        self.activate_prompt(effects);
    }

    fn accept_prompt(&mut self, value: Option<bool>, save: bool, effects: &mut Vec<Effect>) {
        let Some(prompt) = self.prompts.front() else {
            return;
        };
        let answer = match prompt.kind {
            PromptKind::Text { .. } => PromptAnswer::Text(self.prompt_editor.text().to_string()),
            PromptKind::YesNo { default, .. } => match value.unwrap_or(default) {
                true => PromptAnswer::Yes { remember: save },
                false => PromptAnswer::No { remember: save },
            },
            PromptKind::Alert => PromptAnswer::Ok,
        };
        self.answer_prompt(answer, effects);
    }

    /// Lets `{clipboard}` in commands read the system clipboard.
    pub fn set_clipboard_reader(&mut self, reader: impl Fn() -> Option<String> + 'static) {
        self.clipboard = Some(Box::new(reader));
    }

    /// Begin hint mode once the page has reported its hintable elements.
    pub fn start_hints(&mut self, request: HintRequest, items: Vec<HintItem>) -> Vec<Effect> {
        let mut effects = Vec::new();
        if items.is_empty() {
            self.show_message(Level::Info, "No elements found");
            return effects;
        }
        let session = HintSession::new(request, items, self.settings.str("hints.chars"));
        effects.push(Effect::ShowHints {
            labels: session.labels.clone(),
        });
        self.hints = Some(session);
        self.set_mode(Mode::Hint, &mut effects);
        effects
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
            (true, Mode::Normal) if self.settings.bool("input.insert_mode.auto_enter") => {
                self.set_mode(Mode::Insert, &mut effects)
            }
            (false, Mode::Insert) if self.settings.bool("input.insert_mode.auto_leave") => {
                self.set_mode(Mode::Normal, &mut effects)
            }
            _ => {}
        }
        effects
    }

    /// Switching tabs drops back to normal mode, like qutebrowser's
    /// `tabs.mode_on_change = normal`.
    pub fn tab_switched(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let persist = self.settings.str("tabs.mode_on_change") == "persist";
        // Hints belong to the old tab's page, so they always end.
        if self.mode == Mode::Hint
            || (!persist && matches!(self.mode, Mode::Insert | Mode::Passthrough))
        {
            self.set_mode(Mode::Normal, &mut effects);
        }
        effects
    }

    pub fn load_started(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.message.take().is_some() {
            self.dirty = true;
        }
        let leave_insert =
            self.mode == Mode::Insert && self.settings.bool("input.insert_mode.leave_on_load");
        if leave_insert || self.mode == Mode::Hint {
            self.set_mode(Mode::Normal, &mut effects);
        }
        effects
    }

    pub fn handle_key(&mut self, key: Key) -> KeyOutcome {
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Command => self.handle_command(key),
            Mode::Insert | Mode::Passthrough => self.handle_passthrough(key),
            Mode::Hint => self.handle_hint(key),
            Mode::Prompt | Mode::YesNo => self.handle_prompt(key),
        }
    }

    /// Run a command string such as `scroll down ;; reload`. Variables are
    /// filled in after splitting on `;;`, so their contents cannot add commands.
    pub fn execute_str(&mut self, line: &str, count: Option<u32>) -> Vec<Effect> {
        self.execute_line(line, count, 0)
    }

    fn execute_line(&mut self, line: &str, count: Option<u32>, depth: usize) -> Vec<Effect> {
        let mut effects = Vec::new();
        for piece in line.split(";;").map(str::trim).filter(|p| !p.is_empty()) {
            // Aliases are the user's own commands, so they may contain `;;`;
            // expand them before variables are filled in.
            if let Some(expanded) = self.expand_alias(piece) {
                if depth >= MAX_ALIAS_DEPTH {
                    self.show_message(
                        Level::Error,
                        "Alias expansion is too deep (recursive alias?)",
                    );
                    break;
                }
                effects.extend(self.execute_line(&expanded, count, depth + 1));
                continue;
            }
            let result = self
                .substitute(piece)
                .and_then(|piece| command::parse(&piece).map_err(|e| e.to_string()));
            match result {
                Ok(cmd) => self.execute(cmd, count, &mut effects),
                Err(e) => {
                    self.show_message(Level::Error, e);
                    break;
                }
            }
        }
        effects
    }

    fn expand_alias(&self, piece: &str) -> Option<String> {
        let piece = piece.trim_start_matches(':');
        let (name, rest) = piece.split_once(char::is_whitespace).unwrap_or((piece, ""));
        let target = self.settings.map("aliases")?.get(name)?;
        Some(format!("{target} {rest}").trim().to_string())
    }

    fn substitute(&self, piece: &str) -> Result<String, String> {
        let mut piece = piece.replace("{url}", &self.url);
        if piece.contains("{clipboard}") {
            let text = self
                .clipboard
                .as_ref()
                .and_then(|read| read())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .ok_or("Clipboard is empty")?;
            piece = piece.replace("{clipboard}", &text);
        }
        Ok(piece)
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
                let forward = match self.settings.str("input.forward_unbound_keys") {
                    "all" => true,
                    "none" => false,
                    _ => is_forwardable(key),
                };
                KeyOutcome {
                    consumed: had_prefix || !forward,
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

    fn handle_hint(&mut self, key: Key) -> KeyOutcome {
        if let Lookup::Exact(cmd) = self.keymap.lookup(Mode::Hint, &[key]) {
            let cmd = cmd.to_string();
            return consumed(self.execute_str(&cmd, None));
        }
        let mut effects = Vec::new();
        let Some(session) = self.hints.as_mut() else {
            return consumed(effects);
        };
        let input = match (key.code, key.text()) {
            (KeyCode::Backspace, _) => {
                session.pop();
                HintInput::Filtered
            }
            (_, Some(c)) => session.push(c),
            _ => return consumed(effects),
        };
        self.dirty = true;
        match input {
            HintInput::NoMatch => {}
            HintInput::Filtered => effects.push(Effect::FilterHints {
                typed: session.typed.clone(),
            }),
            HintInput::Chosen(index) => {
                let url = session.items[index].url.clone();
                let request = session.request.clone();
                if request.rapid {
                    effects.push(Effect::FilterHints {
                        typed: String::new(),
                    });
                } else {
                    self.set_mode(Mode::Normal, &mut effects);
                }
                match (request.target, request.fill) {
                    (HintTarget::Fill, Some(fill)) => {
                        let text = fill.replace("{hint-url}", url.as_deref().unwrap_or_default());
                        self.execute(
                            Command::CmdSetText {
                                text,
                                append_space: false,
                            },
                            None,
                            &mut effects,
                        );
                    }
                    (target, _) => effects.push(Effect::FollowHint { index, url, target }),
                }
            }
        }
        consumed(effects)
    }

    /// Prompts swallow every key so nothing reaches the page meanwhile.
    fn handle_prompt(&mut self, key: Key) -> KeyOutcome {
        if let Lookup::Exact(cmd) = self.keymap.lookup(self.mode, &[key]) {
            let cmd = cmd.to_string();
            return consumed(self.execute_str(&cmd, None));
        }
        if self.mode == Mode::Prompt
            && let Some(c) = key.text()
        {
            self.prompt_editor.insert(c);
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
            Command::ModeLeave if matches!(self.mode, Mode::Prompt | Mode::YesNo) => {
                self.answer_prompt(PromptAnswer::Cancelled, effects)
            }
            Command::ModeLeave => self.set_mode(Mode::Normal, effects),
            Command::PromptAccept { value, save } => self.accept_prompt(value, save, effects),
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
            Command::Readline(action) if self.mode == Mode::Prompt => {
                self.prompt_editor.apply(action);
                self.dirty = true;
            }
            Command::Readline(action) => {
                self.cmdline.apply(action);
                self.history.reset();
                self.dirty = true;
                if self.mode == Mode::Command && self.cmdline.text().is_empty() {
                    self.set_mode(Mode::Normal, effects);
                }
            }
            Command::Set { name, value } => self.set_command(name, value, effects),
            Command::Bind {
                mode,
                keys,
                command,
            } => self.bind_command(mode, keys, command, effects),
            Command::Unbind { mode, keys } => match self.keymap.unbind(mode, &keys) {
                Ok(true) => effects.push(Effect::ConfigChanged(ConfigOp::Unbind { mode, keys })),
                Ok(false) => {
                    self.show_message(Level::Error, format!("{keys} is not bound in {mode} mode"));
                }
                Err(e) => {
                    self.show_message(Level::Error, e.to_string());
                }
            },
            Command::CompletionFocus(direction) => {
                self.focus_completion(direction == FocusDirection::Next)
            }
            Command::ClearKeychain => {
                self.pending.clear();
                self.count = None;
                self.dirty = true;
            }
            command => effects.push(Effect::Run { command, count }),
        }
    }

    fn set_command(
        &mut self,
        name: Option<String>,
        value: Option<String>,
        effects: &mut Vec<Effect>,
    ) {
        let Some(name) = name else {
            self.show_message(
                Level::Error,
                "Usage: :set <option> [value]  (:set <option>! toggles)",
            );
            return;
        };
        let (name, toggle) = match (name.strip_suffix('!'), name.strip_suffix('?')) {
            (Some(n), _) => (n.to_string(), true),
            (_, Some(n)) => (n.to_string(), false),
            _ => (name, false),
        };
        let Some(def) = settings::find(&name) else {
            self.show_message(Level::Error, format!("No option {name:?}"));
            return;
        };
        let value = match (toggle, value) {
            (true, _) => match self.settings.get(&name) {
                Some(Value::Bool(b)) => Ok(Value::Bool(!b)),
                _ => Err(format!("{name} is not a true/false option")),
            },
            (false, Some(text)) => def.parse(&text),
            (false, None) => {
                let current = self
                    .settings
                    .get(&name)
                    .map(ToString::to_string)
                    .unwrap_or_default();
                self.show_message(Level::Info, format!("{name} = {current}"));
                return;
            }
        };
        match value {
            Ok(value) => {
                let _ = self.settings.set(&name, value.clone());
                effects.push(Effect::ConfigChanged(ConfigOp::Set { name, value }));
                self.dirty = true;
            }
            Err(e) => {
                self.show_message(Level::Error, e);
            }
        }
    }

    fn bind_command(
        &mut self,
        mode: Mode,
        keys: Option<String>,
        command: Option<String>,
        effects: &mut Vec<Effect>,
    ) {
        let Some(keys) = keys else {
            self.show_message(Level::Error, "Usage: :bind [--mode m] <keys> [command]");
            return;
        };
        let seq = match crate::key::Key::parse_sequence(&keys) {
            Ok(seq) => seq,
            Err(e) => {
                self.show_message(Level::Error, e.to_string());
                return;
            }
        };
        let Some(command) = command else {
            let text = match self.keymap.lookup(mode, &seq) {
                Lookup::Exact(cmd) => format!("{keys} is bound to '{cmd}' in {mode} mode"),
                _ => format!("{keys} is unbound in {mode} mode"),
            };
            self.show_message(Level::Info, text);
            return;
        };
        if let Err(e) = self.check_command(&command) {
            self.show_message(Level::Error, e);
            return;
        }
        match self.keymap.bind(mode, &keys, &command) {
            Ok(()) => effects.push(Effect::ConfigChanged(ConfigOp::Bind {
                mode,
                keys,
                command,
            })),
            Err(e) => {
                self.show_message(Level::Error, e.to_string());
            }
        }
    }

    /// Reject bindings to unknown commands up front instead of at key press.
    pub fn check_command(&self, line: &str) -> Result<(), String> {
        for piece in line.split(";;").map(str::trim).filter(|p| !p.is_empty()) {
            if self.expand_alias(piece).is_some() {
                continue;
            }
            if let Err(e @ command::CommandError::Unknown(_)) = command::parse(piece) {
                return Err(e.to_string());
            }
        }
        Ok(())
    }

    fn set_mode(&mut self, mode: Mode, effects: &mut Vec<Effect>) {
        if mode == self.mode {
            return;
        }
        if self.mode == Mode::Command {
            self.cmdline.clear();
            self.history.reset();
        }
        if self.mode == Mode::Hint {
            self.hints = None;
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
    use crate::command::{Direction, OpenTarget, TabTarget};
    use crate::completion::{Completion, CompletionKind};
    use crate::hints::{HintItem, HintRequest, HintTarget};
    use crate::prompt::{Prompt, PromptAnswer, PromptKind, Remember};
    use crate::settings::Value;

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
                    related: false,
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
    fn set_completes_setting_names() {
        let mut e = engine();
        press(&mut e, ":set tabs.new");
        let view = e.completions();
        let names: Vec<_> = view
            .items
            .iter()
            .map(|c| (c.category, c.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Settings", "tabs.new_position.related"),
                ("Settings", "tabs.new_position.unrelated")
            ]
        );
        press(&mut e, "_position.related ");
        assert!(e.completions().items.is_empty());
    }

    #[test]
    fn tab_cycles_completions_and_inserts_them() {
        let mut e = engine();
        e.set_completion_source(|kind, pattern| {
            assert_eq!(kind, CompletionKind::Url);
            ["https://a.org/", "https://b.org/"]
                .iter()
                .filter(|u| u.contains(pattern))
                .map(|u| Completion {
                    category: "History",
                    name: u.to_string(),
                    description: String::new(),
                })
                .collect()
        });
        press(&mut e, "O");
        assert_eq!(e.completions().items.len(), 2);
        press(&mut e, "<Tab>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open -t https://a.org/"
        );
        assert_eq!(e.completions().selected, Some(0));
        // The list stays put while cycling, even though the text changed.
        press(&mut e, "<Tab>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open -t https://b.org/"
        );
        press(&mut e, "<Shift-Tab>");
        assert_eq!(e.completions().selected, Some(0));
        // Typing starts a new query from the edited text.
        press(&mut e, "<Ctrl-u>:open b");
        let view = e.completions();
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.selected, None);
    }

    #[test]
    fn storage_bindings() {
        let mut e = engine();
        e.set_url("https://x.org/");
        press(&mut e, "m");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":quickmark-add https://x.org/ "
        );
        press(&mut e, "<Escape>");
        assert_eq!(
            runs(&press(&mut e, "M")),
            vec![(
                Command::BookmarkAdd {
                    url: None,
                    title: None
                },
                None
            )]
        );
        press(&mut e, "B");
        assert_eq!(e.status().command_line.unwrap().text, ":quickmark-load -t ");
        press(&mut e, "<Escape>");
        assert_eq!(
            runs(&press(&mut e, ":wq<Return>")),
            vec![(Command::Quit { save: true }, None)]
        );
    }

    #[test]
    fn completion_filters_by_prefix() {
        let mut e = engine();
        press(&mut e, ":scr");
        let names: Vec<_> = e
            .completions()
            .items
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["scroll", "scroll-page", "scroll-to-perc"]);
        press(&mut e, "oll ");
        assert!(e.completions().items.is_empty());
    }

    #[test]
    fn tab_bindings() {
        let mut e = engine();
        assert_eq!(
            runs(&press(&mut e, "<Alt-3>")),
            vec![(Command::TabFocus(Some(TabTarget::Number(3))), None)]
        );
        assert_eq!(
            runs(&press(&mut e, "3J")),
            vec![(Command::TabNext, Some(3))]
        );
        assert_eq!(
            runs(&press(&mut e, "<Ctrl-T>")),
            vec![(Command::Undo, None)]
        );
        e.set_url("https://x.org/");
        press(&mut e, "gO");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open -t -r https://x.org/"
        );
    }

    #[test]
    fn tab_switch_leaves_insert_mode() {
        let mut e = engine();
        press(&mut e, "i");
        e.tab_switched();
        assert_eq!(e.mode(), Mode::Normal);
        press(&mut e, ":");
        e.tab_switched();
        assert_eq!(e.mode(), Mode::Command);
    }

    fn hint_request(target: HintTarget, rapid: bool, fill: Option<&str>) -> HintRequest {
        HintRequest {
            group: crate::hints::HintGroup::All,
            target,
            rapid,
            fill: fill.map(String::from),
        }
    }

    fn items(n: usize) -> Vec<HintItem> {
        (0..n)
            .map(|i| HintItem {
                url: Some(format!("https://example.com/{i}")),
            })
            .collect()
    }

    fn all_effects(outcomes: &[KeyOutcome]) -> Vec<Effect> {
        outcomes.iter().flat_map(|o| o.effects.clone()).collect()
    }

    #[test]
    fn f_requests_hints_and_choosing_follows() {
        let mut e = engine();
        let out = press(&mut e, "f");
        assert_eq!(
            runs(&out),
            vec![(
                Command::Hint(hint_request(HintTarget::Normal, false, None)),
                None
            )]
        );
        let effects = e.start_hints(hint_request(HintTarget::Normal, false, None), items(3));
        assert_eq!(
            effects[0],
            Effect::ShowHints {
                labels: vec!["a".into(), "s".into(), "d".into()]
            }
        );
        assert_eq!(e.mode(), Mode::Hint);
        let effects = all_effects(&press(&mut e, "s"));
        assert_eq!(e.mode(), Mode::Normal);
        assert!(effects.contains(&Effect::FollowHint {
            index: 1,
            url: Some("https://example.com/1".into()),
            target: HintTarget::Normal
        }));
    }

    #[test]
    fn hint_typing_filters_and_escape_cancels() {
        let mut e = engine();
        e.start_hints(hint_request(HintTarget::Tab, false, None), items(20));
        let label = e
            .hints
            .as_ref()
            .unwrap()
            .labels
            .iter()
            .find(|l| l.len() == 2)
            .unwrap()
            .clone();
        let first = label.chars().next().unwrap().to_string();
        let effects = all_effects(&press(&mut e, &first));
        assert_eq!(
            effects,
            vec![Effect::FilterHints {
                typed: first.clone()
            }]
        );
        assert_eq!(e.status().keystring, first);
        // A key no label continues with is ignored.
        assert!(all_effects(&press(&mut e, "z")).is_empty());
        press(&mut e, "<Escape>");
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.hints.is_none());
    }

    #[test]
    fn rapid_hints_stay_active() {
        let mut e = engine();
        e.start_hints(hint_request(HintTarget::TabBg, true, None), items(3));
        let effects = all_effects(&press(&mut e, "a"));
        assert_eq!(e.mode(), Mode::Hint);
        assert!(effects.contains(&Effect::FilterHints {
            typed: String::new()
        }));
        assert!(matches!(
            effects.last(),
            Some(Effect::FollowHint { index: 0, .. })
        ));
    }

    #[test]
    fn fill_target_sets_command_line() {
        let mut e = engine();
        e.start_hints(
            hint_request(HintTarget::Fill, false, Some(":open -t {hint-url}")),
            items(3),
        );
        press(&mut e, "d");
        assert_eq!(e.mode(), Mode::Command);
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open -t https://example.com/2"
        );
    }

    #[test]
    fn no_elements_shows_message() {
        let mut e = engine();
        assert!(
            e.start_hints(hint_request(HintTarget::Normal, false, None), Vec::new())
                .is_empty()
        );
        assert_eq!(e.mode(), Mode::Normal);
        assert_eq!(e.status().message.unwrap().text, "No elements found");
    }

    #[test]
    fn clipboard_substitution_cannot_chain_commands() {
        let mut e = engine();
        e.set_clipboard_reader(|| Some("evil ;; quit".into()));
        let out = press(&mut e, "pp");
        assert_eq!(
            runs(&out),
            vec![(
                Command::Open {
                    target: OpenTarget::Current,
                    related: false,
                    url: Some("evil ;; quit".into())
                },
                None
            )]
        );
    }

    #[test]
    fn empty_clipboard_is_an_error() {
        let mut e = engine();
        e.set_clipboard_reader(|| Some("  ".into()));
        assert!(runs(&press(&mut e, "pp")).is_empty());
        assert_eq!(e.status().message.unwrap().text, "Clipboard is empty");
    }

    fn config_changes(outcomes: &[KeyOutcome]) -> Vec<ConfigOp> {
        all_effects(outcomes)
            .into_iter()
            .filter_map(|e| match e {
                Effect::ConfigChanged(op) => Some(op),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn set_shows_changes_and_toggles() {
        let mut e = engine();
        let out = press(&mut e, ":set hints.chars qwer<Return>");
        assert_eq!(
            config_changes(&out),
            vec![ConfigOp::Set {
                name: "hints.chars".into(),
                value: Value::Str("qwer".into())
            }]
        );
        assert_eq!(e.settings().str("hints.chars"), "qwer");
        press(&mut e, ":set hints.uppercase!<Return>");
        assert!(e.settings().bool("hints.uppercase"));
        press(&mut e, ":set hints.chars<Return>");
        assert_eq!(e.status().message.unwrap().text, "hints.chars = qwer");
        press(&mut e, ":set hints.chars x<Return>");
        assert!(e.status().message.unwrap().text.contains("two distinct"));
        press(&mut e, ":set nope 1<Return>");
        assert_eq!(e.status().message.unwrap().text, "No option \"nope\"");
    }

    #[test]
    fn bind_and_unbind_commands() {
        let mut e = engine();
        let out = press(&mut e, ":bind X reload<Return>");
        assert_eq!(config_changes(&out).len(), 1);
        assert_eq!(
            runs(&press(&mut e, "X")),
            vec![(Command::Reload { force: false }, None)]
        );
        press(&mut e, ":bind X frobnicate<Return>");
        assert!(e.status().message.unwrap().text.contains("no such command"));
        press(&mut e, ":bind X<Return>");
        assert_eq!(
            e.status().message.unwrap().text,
            "X is bound to 'reload' in normal mode"
        );
        let out = press(&mut e, ":unbind X<Return>");
        assert_eq!(
            config_changes(&out),
            vec![ConfigOp::Unbind {
                mode: Mode::Normal,
                keys: "X".into()
            }]
        );
        press(&mut e, ":unbind X<Return>");
        assert_eq!(
            e.status().message.unwrap().text,
            "X is not bound in normal mode"
        );
    }

    #[test]
    fn aliases_expand_and_stop_recursing() {
        let mut e = engine();
        assert_eq!(
            runs(&press(&mut e, ":q<Return>")),
            vec![(Command::Quit { save: false }, None)]
        );
        let aliases = Value::Map(
            [
                ("br".to_string(), "back ;; reload".to_string()),
                ("loop".to_string(), "loop".to_string()),
            ]
            .into(),
        );
        e.apply_config(&ConfigOp::Set {
            name: "aliases".into(),
            value: aliases,
        })
        .unwrap();
        assert_eq!(
            runs(&press(&mut e, ":br<Return>")),
            vec![
                (Command::Back, None),
                (Command::Reload { force: false }, None)
            ]
        );
        assert!(runs(&press(&mut e, ":loop<Return>")).is_empty());
        assert!(e.status().message.unwrap().text.contains("too deep"));
    }

    #[test]
    fn input_settings() {
        let mut e = engine();
        e.apply_config(&ConfigOp::Set {
            name: "input.forward_unbound_keys".into(),
            value: Value::Str("all".into()),
        })
        .unwrap();
        assert!(!press(&mut e, "x")[0].consumed);
        e.apply_config(&ConfigOp::Set {
            name: "input.insert_mode.auto_enter".into(),
            value: Value::Bool(false),
        })
        .unwrap();
        e.focus_changed(true);
        assert_eq!(e.mode(), Mode::Normal);
        e.reset_config();
        assert!(press(&mut e, "x")[0].consumed);
    }

    #[test]
    fn hint_chars_setting_is_used() {
        let mut e = engine();
        e.apply_config(&ConfigOp::Set {
            name: "hints.chars".into(),
            value: Value::Str("xy".into()),
        })
        .unwrap();
        let effects = e.start_hints(hint_request(HintTarget::Normal, false, None), items(2));
        assert_eq!(
            effects[0],
            Effect::ShowHints {
                labels: vec!["x".into(), "y".into()]
            }
        );
    }

    fn prompt(id: u64, kind: PromptKind) -> Prompt {
        Prompt {
            id,
            title: "t".into(),
            message: "m".into(),
            kind,
        }
    }

    fn answers(outcomes: &[KeyOutcome]) -> Vec<(u64, PromptAnswer)> {
        all_effects(outcomes)
            .into_iter()
            .filter_map(|e| match e {
                Effect::PromptAnswered { id, answer } => Some((id, answer)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn text_prompt_edits_and_accepts() {
        let mut e = engine();
        press(&mut e, "i");
        e.push_prompt(prompt(
            1,
            PromptKind::Text {
                default: "/tmp/file".into(),
                masked: false,
            },
        ));
        assert_eq!(e.mode(), Mode::Prompt);
        // Keys edit the answer; none reach the page.
        let out = press(&mut e, "<Ctrl-w>x.txt");
        assert!(out.iter().all(|o| o.consumed));
        assert_eq!(e.prompt_view().unwrap().input, "/tmp/x.txt");
        assert_eq!(
            answers(&press(&mut e, "<Return>")),
            vec![(1, PromptAnswer::Text("/tmp/x.txt".into()))]
        );
        // Back to the mode from before the prompt.
        assert_eq!(e.mode(), Mode::Insert);
    }

    #[test]
    fn password_prompts_are_masked() {
        let mut e = engine();
        e.push_prompt(prompt(
            1,
            PromptKind::Text {
                default: String::new(),
                masked: true,
            },
        ));
        press(&mut e, "hunter2");
        assert_eq!(e.prompt_view().unwrap().input, "*******");
        assert_eq!(
            answers(&press(&mut e, "<Return>")),
            vec![(1, PromptAnswer::Text("hunter2".into()))]
        );
    }

    #[test]
    fn yes_no_keys() {
        let mut e = engine();
        for id in 1..=5 {
            e.push_prompt(prompt(
                id,
                PromptKind::YesNo {
                    default: false,
                    remember: Remember::Session,
                },
            ));
        }
        assert_eq!(e.prompt_view().unwrap().queued, 4);
        assert_eq!(
            answers(&press(&mut e, "y")),
            vec![(1, PromptAnswer::Yes { remember: false })]
        );
        assert_eq!(
            answers(&press(&mut e, "N")),
            vec![(2, PromptAnswer::No { remember: true })]
        );
        assert_eq!(
            answers(&press(&mut e, "<Return>")),
            vec![(3, PromptAnswer::No { remember: false })]
        );
        assert_eq!(
            answers(&press(&mut e, "<Escape>")),
            vec![(4, PromptAnswer::Cancelled)]
        );
        assert!(answers(&press(&mut e, "j")).is_empty());
        assert_eq!(
            answers(&press(&mut e, "A")),
            vec![(5, PromptAnswer::Yes { remember: true })]
        );
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn cancelling_the_active_prompt_shows_the_next() {
        let mut e = engine();
        e.push_prompt(prompt(1, PromptKind::Alert));
        e.push_prompt(prompt(
            2,
            PromptKind::Text {
                default: "x".into(),
                masked: false,
            },
        ));
        assert_eq!(e.mode(), Mode::YesNo);
        e.cancel_prompt(1);
        assert_eq!(e.mode(), Mode::Prompt);
        assert_eq!(e.prompt_view().unwrap().input, "x");
        e.cancel_prompt(2);
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.prompt_view().is_none());
    }

    #[test]
    fn prompts_interrupt_the_command_line() {
        let mut e = engine();
        press(&mut e, ":open x");
        e.push_prompt(prompt(1, PromptKind::Alert));
        assert_eq!(
            answers(&press(&mut e, "<Return>")),
            vec![(1, PromptAnswer::Ok)]
        );
        assert_eq!(e.mode(), Mode::Normal);
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
