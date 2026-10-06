use serde::Serialize;

use std::collections::{HashMap, VecDeque};

use crate::cmdline::{History, LineEditor};
use crate::command::{self, Command, FocusDirection};
use crate::completion::{self, Completion, CompletionKind, CompletionView};
use crate::config::ConfigOp;
use crate::hints::{AutoFollow, HintInput, HintItem, HintRequest, HintSession, HintTarget};
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
    /// Delete what this completion stands for (`:completion-item-del`).
    DeleteCompletion(Completion),
    /// Put text on the clipboard, or the primary selection.
    YankText {
        text: String,
        primary: bool,
    },
    /// A `:set`/`:bind`/`:unbind` succeeded; the host persists it.
    ConfigChanged(ConfigOp),
    /// The user answered (or cancelled) the prompt with this id.
    PromptAnswered {
        id: u64,
        answer: PromptAnswer,
    },
    /// A replayed macro key the engine doesn't handle: send it to the page.
    PassKey(Key),
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
    /// Numbers each partly typed key chain, for `input.partial_timeout`.
    partial_generation: u64,
    cmdline: LineEditor,
    history: History,
    /// Messages on screen, newest last, each with the generation that
    /// expires it.
    messages: Vec<(u64, Message)>,
    message_generation: u64,
    url: String,
    clipboard: Option<Box<dyn Fn() -> Option<String>>>,
    primary: Option<Box<dyn Fn() -> Option<String>>>,
    completion_source: Option<completion::Source>,
    completion: Option<CompletionState>,
    hints: Option<HintSession>,
    /// Words for `hints.mode = word`, from `hints.dictionary`.
    hint_words: std::rc::Rc<[String]>,
    prompts: VecDeque<Prompt>,
    prompt_editor: LineEditor,
    /// The mode to return to once the prompt queue is empty.
    mode_before_prompt: Mode,
    dirty: bool,
    macros: Macros,
    /// Commands defined in `config.lua`, as `(name, description)`.
    user_commands: Vec<(String, String)>,
    /// Every message of this session, oldest first, for `:messages`.
    message_log: VecDeque<LoggedMessage>,
    /// What `.` runs: the last command line or normal-mode binding.
    last_command: Option<(String, Option<u32>)>,
    /// Tab was pressed, for `completion.show = auto`.
    completion_opened: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LoggedMessage {
    /// Seconds since the Unix epoch.
    pub time: u64,
    pub level: Level,
    pub text: String,
}

/// How many messages `:messages` keeps.
const MESSAGE_LOG_LEN: usize = 1000;

/// Commands `.` doesn't repeat: they only start something interactive.
fn repeatable(line: &str) -> bool {
    let name = line
        .trim_start_matches(':')
        .split_whitespace()
        .next()
        .unwrap_or("");
    !matches!(
        name,
        "" | "repeat-command" | "mode-enter" | "mode-leave" | "set-cmd-text" | "cmd-set-text"
    )
}

/// Keyboard macros (`q` / `@`).
#[derive(Default)]
struct Macros {
    recording: Option<(char, Vec<Key>)>,
    registers: HashMap<char, Vec<Key>>,
    last_run: Option<char>,
    /// Nesting depth of replays, to stop a macro that runs itself.
    replaying: usize,
    /// Keys in the binding that ran the current command, so stopping a
    /// recording can drop them.
    binding_len: usize,
    /// Where the current command line started in the recording.
    cmdline_start: Option<usize>,
    /// The count given to `@`, waiting for the register key.
    count: Option<u32>,
}

/// How deeply macros may run other macros.
const MAX_MACRO_DEPTH: usize = 10;

/// Messages shown at once: the newest in the status bar, the rest above it.
const MAX_MESSAGES: usize = 5;

impl Engine {
    pub fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            settings: Settings::default(),
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            partial_generation: 0,
            cmdline: LineEditor::default(),
            history: History::default(),
            messages: Vec::new(),
            message_generation: 0,
            url: String::new(),
            clipboard: None,
            primary: None,
            completion_source: None,
            completion: None,
            hints: None,
            hint_words: std::rc::Rc::from(Vec::new()),
            prompts: VecDeque::new(),
            prompt_editor: LineEditor::default(),
            mode_before_prompt: Mode::Normal,
            dirty: true,
            macros: Macros::default(),
            message_log: VecDeque::new(),
            last_command: None,
            completion_opened: false,
            user_commands: Vec::new(),
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
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
            ConfigOp::SetFor {
                pattern,
                name,
                value,
            } => self.settings.set_for(pattern, name, value.clone()),
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
            ConfigOp::Unset { name } => self.settings.unset(name),
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
        let mut keystring = match &self.macros.recording {
            Some((register, _)) => format!("recording @{register} "),
            None => String::new(),
        };
        keystring.push_str(&self.count.map(|c| c.to_string()).unwrap_or_default());
        keystring.push_str(&format_sequence(&self.pending));
        if let Some(hints) = &self.hints {
            keystring.push_str(&hints.filter);
            keystring.push_str(&hints.typed);
        }
        StatusView {
            mode: self.mode,
            command_line: (self.mode == Mode::Command).then(|| CommandLineView {
                text: self.cmdline.text().to_string(),
                cursor: self.cmdline.cursor(),
            }),
            keystring,
            message: self.messages.last().map(|(_, m)| m.clone()),
        }
    }

    /// The hints on screen, while in hint mode.
    pub fn hint_session(&self) -> Option<&HintSession> {
        self.hints.as_ref()
    }

    /// What the pending keys can still become, for the key hint popup:
    /// the keys typed so far and `(the rest, command)` for each binding,
    /// minus chains matching `keyhint.blacklist`.
    pub fn keyhints(&self) -> Option<(String, Vec<(String, String)>)> {
        if self.pending.is_empty() || !matches!(self.mode, Mode::Normal | Mode::Caret) {
            return None;
        }
        let prefix = format_sequence(&self.pending);
        let blacklist = self.settings.list("keyhint.blacklist");
        let mut items: Vec<_> = self
            .keymap
            .continuations(self.mode, &self.pending)
            .into_iter()
            .filter(|(rest, _)| {
                let chain = format!("{prefix}{rest}");
                !blacklist.iter().any(|glob| crate::url::glob(glob, &chain))
            })
            .collect();
        // Letters first (a, A, b, B…), then digits, then symbols and named keys.
        items.sort_by_key(|(rest, _)| {
            let first = rest.chars().next().unwrap_or(' ');
            let group = if first.is_ascii_alphabetic() {
                0
            } else if first.is_ascii_digit() {
                1
            } else {
                2
            };
            (group, rest.to_lowercase(), first.is_ascii_uppercase())
        });
        (!items.is_empty()).then_some((prefix, items))
    }

    /// Completions for the command line, recomputed only when the text changes.
    pub fn completions(&mut self) -> CompletionView {
        if self.mode != Mode::Command {
            self.completion = None;
            self.completion_opened = false;
            return CompletionView::default();
        }
        let shown = match self.settings.str("completion.show") {
            "never" => false,
            "auto" => self.completion_opened,
            _ => true,
        };
        // completion.min_chars counts what's typed after the command name.
        let typed = self
            .cmdline
            .text()
            .trim_start_matches(':')
            .split_once(char::is_whitespace)
            .map_or(usize::MAX, |(_, rest)| rest.trim_start().chars().count());
        if !shown || typed < self.settings.int("completion.min_chars").max(0) as usize {
            self.completion = None;
            return CompletionView::default();
        }
        let text = self.cmdline.text();
        let fresh = match &self.completion {
            Some(state) => state.base != text && state.inserted.as_deref() != Some(text),
            None => true,
        };
        if fresh {
            let mut items =
                completion::compute(text, self.completion_source.as_ref(), &self.settings);
            // Commands from config.lua complete next to the built-in ones.
            if let Some(typed) = text
                .strip_prefix(':')
                .filter(|t| !t.contains(char::is_whitespace))
            {
                items.extend(
                    self.user_commands
                        .iter()
                        .filter(|(n, _)| n.starts_with(typed))
                        .map(|(n, d)| completion::Completion {
                            category: "Commands",
                            name: n.clone(),
                            description: d.clone(),
                            time: None,
                            detail: None,
                        }),
                );
            }
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

    fn path_prompt(&self) -> bool {
        matches!(
            self.prompts.front().map(|p| &p.kind),
            Some(PromptKind::Text { path: true, .. })
        )
    }

    /// Replace the text of a file prompt, as `:prompt-fileselect-external`
    /// does once the picker returns. Ignored if that prompt is gone.
    pub fn set_path_prompt_text(&mut self, text: &str) {
        if self.path_prompt() {
            self.prompt_editor.set(text);
            self.dirty = true;
        }
    }

    /// Lets `:open`, `:quickmark-load` and friends complete from storage.
    pub fn set_completion_source(
        &mut self,
        source: impl Fn(CompletionKind, &str) -> Vec<Completion> + 'static,
    ) {
        self.completion_source = Some(Box::new(source));
    }

    /// Take the selected item out of the list and ask the host to delete
    /// it. The next item takes its place, as if Tab had chosen it.
    fn delete_completion(&mut self, effects: &mut Vec<Effect>) {
        const DELETABLE: &[&str] = &["History", "Quickmarks", "Bookmarks", "Sessions", "Tabs"];
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        let Some(index) = state.view.selected else {
            self.show_message(Level::Error, "No completion is selected");
            return;
        };
        let item = state.view.items[index].clone();
        if !DELETABLE.contains(&item.category) {
            let category = item.category.to_lowercase();
            self.show_message(
                Level::Error,
                format!("Can't delete {category} from completion"),
            );
            return;
        }
        state.view.items.remove(index);
        let text = if state.view.items.is_empty() {
            state.view.selected = None;
            state.inserted = None;
            state.base.clone()
        } else {
            let next = index.min(state.view.items.len() - 1);
            state.view.selected = Some(next);
            let text = completion::insert(&state.base, &state.view.items[next]);
            state.inserted = Some(text.clone());
            text
        };
        self.cmdline.set(&text);
        self.dirty = true;
        effects.push(Effect::DeleteCompletion(item));
    }

    fn focus_completion(&mut self, forward: bool) {
        self.completion_opened = true;
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
        // completion.quick: a lone item that starts a new part (a command
        // name, a setting) is taken, and the next part completes.
        if len == 1 && text.ends_with(' ') && self.settings.bool("completion.quick") {
            self.completion = None;
        }
        self.cmdline.set(&text);
        self.dirty = true;
    }

    /// Whether the command line changed since completions were last computed,
    /// so the host can hold them back for `completion.delay`.
    pub fn completion_stale(&self) -> bool {
        self.mode == Mode::Command
            && self.completion.as_ref().is_some_and(|state| {
                let text = self.cmdline.text();
                state.base != text && state.inserted.as_deref() != Some(text)
            })
    }

    /// `completion.use_best_match`: the first command, alias or user
    /// command that starts with an unknown command name.
    fn best_match(&self, line: &str) -> Option<String> {
        let (name, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let aliases = self.settings.map("aliases");
        let known = command::COMMANDS.iter().any(|c| c.name == name)
            || aliases.is_some_and(|a| a.contains_key(name))
            || self.user_commands.iter().any(|(n, _)| n == name);
        if name.is_empty() || known {
            return None;
        }
        let found = command::COMMANDS
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| c.name.to_string())
            .chain(aliases.into_iter().flat_map(|a| a.keys().cloned()))
            .chain(self.user_commands.iter().map(|(n, _)| n.clone()))
            .filter(|n| n.starts_with(name))
            .min()?;
        Some(format!("{found} {rest}").trim_end().to_string())
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

    /// Lets `{primary}` read the primary selection (X11); defaults to the clipboard.
    pub fn set_primary_reader(&mut self, reader: impl Fn() -> Option<String> + 'static) {
        self.primary = Some(Box::new(reader));
    }

    /// The words `hints.mode = word` labels with.
    pub fn set_hint_words(&mut self, words: std::rc::Rc<[String]>) {
        self.hint_words = words;
    }

    /// Begin hint mode once the page has reported its hintable elements.
    pub fn start_hints(&mut self, request: HintRequest, items: Vec<HintItem>) -> Vec<Effect> {
        let mut effects = Vec::new();
        if items.is_empty() {
            self.show_message(Level::Info, "No elements found");
            return effects;
        }
        let mode = self.settings.str("hints.mode").to_string();
        if mode == "word" && self.hint_words.is_empty() {
            self.show_message(
                Level::Warning,
                "hints.dictionary has no usable words; using letter hints",
            );
        }
        let mut session = if mode == "number" {
            HintSession::new_numbers(request, items)
        } else if mode == "word" && !self.hint_words.is_empty() {
            HintSession::new_words(request, items, &self.hint_words)
        } else {
            HintSession::new_with(
                request,
                items,
                self.settings.str("hints.chars"),
                self.settings.int("hints.min_chars").max(1) as u32,
                self.settings.bool("hints.scatter"),
            )
        };
        session.auto_follow = AutoFollow::from_setting(self.settings.str("hints.auto_follow"));
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
        let text = text.into();
        self.message_generation += 1;
        if self.message_log.len() == MESSAGE_LOG_LEN {
            self.message_log.pop_front();
        }
        self.message_log.push_back(LoggedMessage {
            time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            level,
            text: text.clone(),
        });
        self.messages
            .push((self.message_generation, Message { level, text }));
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
        self.dirty = true;
        self.message_generation
    }

    pub fn message_log(&self) -> Vec<LoggedMessage> {
        self.message_log.iter().cloned().collect()
    }

    /// Messages still on screen besides the newest, oldest first.
    pub fn earlier_messages(&self) -> Vec<Message> {
        let n = self.messages.len().saturating_sub(1);
        self.messages[..n].iter().map(|(_, m)| m.clone()).collect()
    }

    pub fn message_generation(&self) -> u64 {
        self.message_generation
    }

    pub fn expire_message(&mut self, generation: u64) {
        let before = self.messages.len();
        self.messages.retain(|(g, _)| *g != generation);
        if self.messages.len() != before {
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
    /// The current tab changed. `left_in` is the mode the new tab was in when
    /// the user last left it, for `tabs.mode_on_change = restore`.
    pub fn tab_switched(&mut self, left_in: Option<Mode>) -> Vec<Effect> {
        let mut effects = Vec::new();
        let typing = |m: Mode| matches!(m, Mode::Insert | Mode::Passthrough);
        let target = match self.settings.str("tabs.mode_on_change") {
            "persist" if typing(self.mode) => self.mode,
            "restore" => left_in.filter(|m| typing(*m)).unwrap_or(Mode::Normal),
            _ => Mode::Normal,
        };
        // Hints, the caret and the like belong to the old tab's page, so they
        // end. The command line and prompts belong to the window and stay.
        let page_bound = matches!(
            self.mode,
            Mode::Hint
                | Mode::Caret
                | Mode::SetMark
                | Mode::JumpMark
                | Mode::RecordMacro
                | Mode::RunMacro
        );
        if self.mode != target
            && (page_bound || typing(self.mode) || (self.mode == Mode::Normal && typing(target)))
        {
            self.set_mode(target, &mut effects);
        }
        effects.extend(self.apply_mode_override());
        effects
    }

    /// A key chain or count is half typed: its generation, for
    /// [`Engine::expire_partial`] once `input.partial_timeout` runs out.
    pub fn partial_keys(&self) -> Option<u64> {
        (!self.pending.is_empty() || self.count.is_some()).then_some(self.partial_generation)
    }

    /// Forget the half-typed keys, if nothing was typed since `generation`.
    pub fn expire_partial(&mut self, generation: u64) {
        if generation == self.partial_generation && self.partial_keys().is_some() {
            self.pending.clear();
            self.count = None;
            self.dirty = true;
        }
    }

    /// `input.mode_override`: the mode the current page's site asks for.
    /// The command line, prompts and hints aren't interrupted.
    pub fn apply_mode_override(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let wanted = match self.settings.str_for("input.mode_override", &self.url) {
            "normal" => Mode::Normal,
            "insert" => Mode::Insert,
            "passthrough" => Mode::Passthrough,
            _ => return effects,
        };
        if self.mode != wanted
            && matches!(self.mode, Mode::Normal | Mode::Insert | Mode::Passthrough)
        {
            self.set_mode(wanted, &mut effects);
        }
        effects
    }

    pub fn load_started(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if !std::mem::take(&mut self.messages).is_empty() {
            self.dirty = true;
        }
        let leave_insert =
            self.mode == Mode::Insert && self.settings.bool("input.insert_mode.leave_on_load");
        let leave_hints = self.mode == Mode::Hint && self.settings.bool("hints.leave_on_load");
        if leave_insert || leave_hints {
            self.set_mode(Mode::Normal, &mut effects);
        }
        effects
    }

    pub fn handle_key(&mut self, key: Key) -> KeyOutcome {
        let key = self.map_key(key);
        if self.macros.replaying == 0
            && let Some((_, keys)) = &mut self.macros.recording
        {
            keys.push(key);
        }
        self.dispatch_key(key)
    }

    /// `bindings.key_mappings`: the key this one stands for.
    fn map_key(&self, key: Key) -> Key {
        let Some(mappings) = self.settings.map("bindings.key_mappings") else {
            return key;
        };
        let single = |s: &str| match Key::parse_sequence(s).as_deref() {
            Ok([k]) => Some(*k),
            _ => None,
        };
        mappings
            .iter()
            .find(|(from, _)| single(from) == Some(key))
            .and_then(|(_, to)| single(to))
            .unwrap_or(key)
    }

    fn dispatch_key(&mut self, key: Key) -> KeyOutcome {
        match self.mode {
            Mode::Normal | Mode::Caret => self.handle_bound(self.mode, key),
            Mode::Command => self.handle_command(key),
            Mode::Insert | Mode::Passthrough => self.handle_passthrough(key),
            Mode::Hint => self.handle_hint(key),
            Mode::Prompt | Mode::YesNo => self.handle_prompt(key),
            Mode::SetMark | Mode::JumpMark => self.handle_mark(key),
            Mode::RecordMacro | Mode::RunMacro => self.handle_macro_register(key),
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
                .and_then(|piece| match self.user_command(&piece) {
                    Some(command) => Ok(command),
                    None => command::parse(&piece).map_err(|e| e.to_string()),
                });
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
        if piece.contains("{primary}") {
            let text = self
                .primary
                .as_ref()
                .or(self.clipboard.as_ref())
                .and_then(|read| read())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .ok_or("Primary selection is empty")?;
            piece = piece.replace("{primary}", &text);
        }
        Ok(piece)
    }

    /// Normal and caret mode: counts, multi-key bindings, and what to do
    /// with keys nothing is bound to.
    fn handle_bound(&mut self, mode: Mode, key: Key) -> KeyOutcome {
        if !std::mem::take(&mut self.messages).is_empty() {
            self.dirty = true;
        }
        if self.pending.is_empty()
            && self.settings.bool("input.match_counts")
            && let Some(d) = key.digit()
            && (d != 0 || self.count.is_some())
        {
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d));
            self.dirty = true;
            return consumed(Vec::new());
        }
        self.pending.push(key);
        self.partial_generation += 1;
        self.dirty = true;
        match self.keymap.lookup(mode, &self.pending) {
            Lookup::Exact(cmd) => {
                let cmd = cmd.to_string();
                let count = self.count.take();
                self.macros.binding_len =
                    self.pending.len() + count.map_or(0, |c| c.to_string().len());
                self.pending.clear();
                if mode == Mode::Normal && repeatable(&cmd) {
                    self.last_command = Some((cmd.clone(), count));
                }
                consumed(self.execute_str(&cmd, count))
            }
            Lookup::Partial => consumed(Vec::new()),
            Lookup::None => {
                let had_prefix = self.pending.len() > 1 || self.count.is_some();
                self.pending.clear();
                self.count = None;
                // Caret mode keeps every key; the page would scroll or type.
                let forward = mode == Mode::Normal
                    && match self.settings.str("input.forward_unbound_keys") {
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
        let before = self.cmdline.text().to_string();
        let mut effects = if let Lookup::Exact(cmd) = self.keymap.lookup(Mode::Command, &[key]) {
            let cmd = cmd.to_string();
            self.execute_str(&cmd, None)
        } else {
            if let Some(c) = key.text() {
                self.cmdline.insert(c);
                self.history.reset();
                self.dirty = true;
            }
            Vec::new()
        };
        // Search as you type.
        if self.mode == Mode::Command
            && self.settings.bool("search.incremental")
            && self.cmdline.text() != before
            && let Some((reverse, needle)) = search_text(self.cmdline.text())
        {
            let search = Command::Search {
                text: needle.to_string(),
                reverse,
                incremental: true,
            };
            effects.push(Effect::Run {
                command: search,
                count: None,
            });
        }
        consumed(effects)
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
            (KeyCode::Backspace, _) => session.pop(),
            (_, Some(c)) => session.push(c),
            _ => return consumed(effects),
        };
        self.dirty = true;
        match input {
            HintInput::NoMatch => {}
            HintInput::Filtered => effects.push(Effect::FilterHints {
                typed: session.typed.clone(),
            }),
            HintInput::Relabeled => effects.push(Effect::ShowHints {
                labels: session.labels.clone(),
            }),
            HintInput::Ready(_) => effects.push(Effect::FilterHints {
                typed: session.typed.clone(),
            }),
            HintInput::Chosen(index) => self.follow_hint(index, &mut effects),
        }
        consumed(effects)
    }

    /// Act on hint `index`, as typing its label or Return (`hint-follow`) does.
    fn follow_hint(&mut self, index: usize, effects: &mut Vec<Effect>) {
        let Some(session) = self.hints.as_ref() else {
            return;
        };
        let url = session.items[index].url.clone();
        let request = session.request.clone();
        if request.rapid {
            effects.push(Effect::FilterHints {
                typed: String::new(),
            });
        } else {
            self.set_mode(Mode::Normal, effects);
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
                    effects,
                );
            }
            (HintTarget::Spawn, Some(line)) => {
                let url = url.unwrap_or_default();
                let quoted = crate::shell_words::quote(&url);
                let line = if line.contains("{hint-url}") {
                    line.replace("{hint-url}", &quoted)
                } else {
                    format!("{line} {quoted}")
                };
                effects.extend(self.execute_str(&format!("spawn {line}"), None));
            }
            (HintTarget::Userscript, Some(line)) => match crate::shell_words::split(&line) {
                Ok(argv) if !argv.is_empty() => effects.push(Effect::Run {
                    command: Command::Spawn {
                        userscript: true,
                        verbose: false,
                        output_messages: false,
                        output: false,
                        hint_url: Some(url.unwrap_or_default()),
                        detach: false,
                        argv,
                    },
                    count: None,
                }),
                _ => {
                    self.show_message(Level::Error, "hint userscript: bad script name");
                }
            },
            (target, _) => effects.push(Effect::FollowHint { index, url, target }),
        }
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

    /// The key after `` ` `` or `'` names the mark; anything else cancels.
    fn handle_mark(&mut self, key: Key) -> KeyOutcome {
        if let Lookup::Exact(cmd) = self.keymap.lookup(self.mode, &[key]) {
            let cmd = cmd.to_string();
            return consumed(self.execute_str(&cmd, None));
        }
        let command = match self.mode {
            Mode::SetMark => "set-mark",
            _ => "jump-mark",
        };
        let mut effects = Vec::new();
        self.set_mode(Mode::Normal, &mut effects);
        if let Some(c) = key.text().filter(|c| !c.is_whitespace()) {
            self.execute(
                Command::Mark {
                    set: command == "set-mark",
                    key: c,
                },
                None,
                &mut effects,
            );
        }
        consumed(effects)
    }

    /// The key after `q` or `@` names the register; anything else cancels.
    fn handle_macro_register(&mut self, key: Key) -> KeyOutcome {
        if let Lookup::Exact(cmd) = self.keymap.lookup(self.mode, &[key]) {
            let cmd = cmd.to_string();
            return consumed(self.execute_str(&cmd, None));
        }
        let record = self.mode == Mode::RecordMacro;
        let mut effects = Vec::new();
        self.set_mode(Mode::Normal, &mut effects);
        if let Some(c) = key.text().filter(|c| !c.is_whitespace()) {
            if record {
                // The register key isn't part of the macro.
                if let Some((_, keys)) = &mut self.macros.recording {
                    keys.pop();
                }
                self.execute(
                    Command::MacroRecord { register: Some(c) },
                    None,
                    &mut effects,
                );
            } else {
                let count = self.macros.count.take();
                self.execute(Command::MacroRun { register: Some(c) }, count, &mut effects);
            }
        }
        consumed(effects)
    }

    fn macro_record(&mut self, register: Option<char>, effects: &mut Vec<Effect>) {
        if let Some((register, mut keys)) = self.macros.recording.take() {
            // Drop the keys that asked to stop: the binding, or the command line.
            let keep = match self.macros.cmdline_start.take() {
                Some(start) if self.mode == Mode::Command => start,
                _ => keys.len().saturating_sub(self.macros.binding_len),
            };
            keys.truncate(keep);
            self.macros.registers.insert(register, keys);
            self.show_message(Level::Info, format!("Recorded macro {register}"));
            self.dirty = true;
            return;
        }
        match register {
            Some(register) => {
                self.macros.recording = Some((register, Vec::new()));
                self.dirty = true;
            }
            None => self.set_mode(Mode::RecordMacro, effects),
        }
    }

    fn macro_run(&mut self, register: Option<char>, count: Option<u32>, effects: &mut Vec<Effect>) {
        let Some(register) = register else {
            self.macros.count = count;
            return self.set_mode(Mode::RunMacro, effects);
        };
        let register = match register {
            '@' => match self.macros.last_run {
                Some(last) => last,
                None => {
                    self.show_message(Level::Error, "No macro has run yet");
                    return;
                }
            },
            r => r,
        };
        let Some(keys) = self.macros.registers.get(&register).cloned() else {
            self.show_message(Level::Error, format!("Macro {register} is empty"));
            return;
        };
        if self.macros.replaying >= MAX_MACRO_DEPTH {
            self.show_message(
                Level::Error,
                "Macros nest too deeply (does one run itself?)",
            );
            return;
        }
        self.macros.last_run = Some(register);
        self.macros.replaying += 1;
        for _ in 0..count.unwrap_or(1).max(1) {
            for &key in &keys {
                let outcome = self.dispatch_key(key);
                effects.extend(outcome.effects);
                if !outcome.consumed {
                    effects.push(Effect::PassKey(key));
                }
            }
        }
        self.macros.replaying -= 1;
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
            Command::MacroRecord { register } => self.macro_record(register, effects),
            Command::MacroRun { register } => self.macro_run(register, count, effects),
            Command::ModeLeave if matches!(self.mode, Mode::Prompt | Mode::YesNo) => {
                self.answer_prompt(PromptAnswer::Cancelled, effects)
            }
            Command::ModeLeave => {
                // Escape from an incremental search takes its highlights away.
                if self.mode == Mode::Command
                    && search_text(self.cmdline.text()).is_some()
                    && self.settings.bool("search.incremental")
                {
                    let clear = Command::Search {
                        text: String::new(),
                        reverse: false,
                        incremental: true,
                    };
                    effects.push(Effect::Run {
                        command: clear,
                        count: None,
                    });
                }
                self.set_mode(Mode::Normal, effects)
            }
            Command::PromptAccept { value, save } => self.accept_prompt(value, save, effects),
            Command::PromptFileselectExternal if self.path_prompt() => {
                effects.push(Effect::Run {
                    command: Command::PromptFileselectExternal,
                    count: None,
                });
            }
            Command::PromptFileselectExternal => {
                self.show_message(Level::Error, "This prompt doesn't ask for a file");
            }
            Command::PromptComplete => {
                let path = matches!(
                    self.prompts.front().map(|p| &p.kind),
                    Some(PromptKind::Text { path: true, .. })
                );
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
                if path
                    && let Some(text) =
                        crate::path_complete::complete(self.prompt_editor.text(), home.as_deref())
                {
                    self.prompt_editor.set(&text);
                    self.dirty = true;
                }
            }
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
                self.history
                    .truncate(self.settings.int("completion.cmd_history_max_items").max(0) as usize);
                self.set_mode(Mode::Normal, effects);
                if let Some((reverse, needle)) = search_text(&text) {
                    let search = Command::Search {
                        text: needle.to_string(),
                        reverse,
                        incremental: false,
                    };
                    return effects.push(Effect::Run {
                        command: search,
                        count: None,
                    });
                }
                let mut line = text.strip_prefix(':').unwrap_or(&text).to_string();
                if self.settings.bool("completion.use_best_match")
                    && let Some(best) = self.best_match(&line)
                {
                    line = best;
                }
                let line = line.as_str();
                if repeatable(line) {
                    self.last_command = Some((line.to_string(), None));
                }
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
            Command::RlPaste { primary } => {
                let reader = if primary {
                    self.primary.as_ref().or(self.clipboard.as_ref())
                } else {
                    self.clipboard.as_ref()
                };
                let Some(text) = reader.and_then(|read| read()) else {
                    return;
                };
                if self.mode == Mode::Prompt {
                    self.prompt_editor.insert_str(&text);
                } else {
                    self.cmdline.insert_str(&text);
                    self.history.reset();
                }
                self.dirty = true;
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
            Command::Set {
                name,
                value,
                pattern,
            } => self.set_command(name, value, pattern, effects),
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
            Command::Message { level, text } => {
                self.show_message(level, text);
            }
            Command::ClearMessages => {
                if !std::mem::take(&mut self.messages).is_empty() {
                    self.dirty = true;
                }
            }
            Command::Nop => {}
            Command::ConfigCycle { name, values } => {
                let current = self.settings.get(&name).map(ToString::to_string);
                if values.is_empty() {
                    self.set_command(Some(format!("{name}!")), None, None, effects);
                } else if values.len() == 1 {
                    self.show_message(Level::Error, "config-cycle needs two or more values");
                } else {
                    let next = current
                        .and_then(|c| values.iter().position(|v| *v == c))
                        .map_or(0, |i| (i + 1) % values.len());
                    self.set_command(Some(name), Some(values[next].clone()), None, effects);
                }
            }
            Command::ConfigListAdd { name, value } => {
                if let Some(Value::List(mut list)) = self.setting_of_kind(&name, "list") {
                    list.push(value);
                    self.set_value(&name, Value::List(list), effects);
                }
            }
            Command::ConfigListRemove { name, value } => {
                if let Some(Value::List(mut list)) = self.setting_of_kind(&name, "list") {
                    let before = list.len();
                    list.retain(|v| *v != value);
                    if list.len() == before {
                        self.show_message(Level::Error, format!("{value:?} isn't in {name}"));
                    } else {
                        self.set_value(&name, Value::List(list), effects);
                    }
                }
            }
            Command::ConfigDictAdd {
                name,
                key,
                value,
                replace,
            } => {
                if let Some(Value::Map(mut map)) = self.setting_of_kind(&name, "map") {
                    if map.contains_key(&key) && !replace {
                        self.show_message(
                            Level::Error,
                            format!("{name} already has {key:?}; use --replace to change it"),
                        );
                    } else {
                        map.insert(key, value);
                        self.set_value(&name, Value::Map(map), effects);
                    }
                }
            }
            Command::ConfigDictRemove { name, key } => {
                if let Some(Value::Map(mut map)) = self.setting_of_kind(&name, "map") {
                    if map.remove(&key).is_none() {
                        self.show_message(Level::Error, format!("{name} has no {key:?}"));
                    } else {
                        self.set_value(&name, Value::Map(map), effects);
                    }
                }
            }
            Command::ConfigClear => {
                let changed = self.settings.changed();
                for (name, _) in &changed {
                    let _ = self.settings.unset(name);
                    effects.push(Effect::ConfigChanged(ConfigOp::Unset {
                        name: name.to_string(),
                    }));
                }
                self.show_message(
                    Level::Info,
                    format!("Put {} setting(s) back to their defaults", changed.len()),
                );
            }
            Command::ConfigUnset { name } => match self.settings.unset(&name) {
                Ok(()) => {
                    let value = self.settings.get(&name).map(ToString::to_string);
                    self.show_message(
                        Level::Info,
                        format!("{name} = {}", value.unwrap_or_default()),
                    );
                    effects.push(Effect::ConfigChanged(ConfigOp::Unset { name }));
                }
                Err(e) => {
                    self.show_message(Level::Error, e);
                }
            },
            Command::RepeatCommand => match self.last_command.clone() {
                Some((line, last_count)) => {
                    effects.extend(self.execute_str(&line, count.or(last_count)))
                }
                None => {
                    self.show_message(Level::Error, "No command to repeat yet");
                }
            },
            Command::CompletionItemDel => self.delete_completion(effects),
            Command::HintFollow { label } => {
                let index = self.hints.as_ref().and_then(|session| match &label {
                    Some(label) => session.labels.iter().position(|l| l == label),
                    None => session.ready,
                });
                match index {
                    Some(index) => self.follow_hint(index, effects),
                    None if self.hints.is_some() => {
                        let what = label
                            .map_or("No hint is ready; type a label".to_string(), |l| {
                                format!("No hint labelled {l:?}")
                            });
                        self.show_message(Level::Error, what);
                    }
                    None => {}
                }
            }
            Command::CompletionItemYank { sel } => {
                let selected = self.completion.as_ref().and_then(|s| {
                    let i = s.view.selected?;
                    s.view.items.get(i).map(|item| item.name.clone())
                });
                match selected {
                    Some(text) => effects.push(Effect::YankText { text, primary: sel }),
                    None => {
                        self.show_message(Level::Error, "No completion is selected");
                    }
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

    /// A setting's current value if it's a `list` or a `map`, as `kind` says;
    /// otherwise shows why not.
    fn setting_of_kind(&mut self, name: &str, kind: &str) -> Option<Value> {
        let value = self.settings.get(name).cloned();
        let fits = matches!(
            (&value, kind),
            (Some(Value::List(_)), "list") | (Some(Value::Map(_)), "map")
        );
        if !fits {
            let what = if value.is_none() {
                "no option".to_string()
            } else {
                format!("not a {kind} setting")
            };
            self.show_message(Level::Error, format!("{name}: {what}"));
            return None;
        }
        value
    }

    /// Set `name` to `value`, checked like `:set`, and persist it.
    fn set_value(&mut self, name: &str, value: Value, effects: &mut Vec<Effect>) {
        let checked = settings::find(name)
            .ok_or_else(|| format!("No option {name:?}"))
            .and_then(|def| def.from_json(&value.to_json()));
        match checked {
            Ok(value) => {
                let _ = self.settings.set(name, value.clone());
                self.show_message(Level::Info, format!("{name} = {value}"));
                effects.push(Effect::ConfigChanged(ConfigOp::Set {
                    name: name.to_string(),
                    value,
                }));
            }
            Err(e) => {
                self.show_message(Level::Error, e);
            }
        }
    }

    fn set_command(
        &mut self,
        name: Option<String>,
        value: Option<String>,
        pattern: Option<String>,
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
        match (value, pattern) {
            (Ok(value), Some(pattern)) => {
                match self.settings.set_for(&pattern, &name, value.clone()) {
                    Ok(()) => effects.push(Effect::ConfigChanged(ConfigOp::SetFor {
                        pattern,
                        name,
                        value,
                    })),
                    Err(e) => {
                        self.show_message(Level::Error, e);
                    }
                }
                self.dirty = true;
            }
            (Ok(value), None) => {
                let _ = self.settings.set(&name, value.clone());
                effects.push(Effect::ConfigChanged(ConfigOp::Set { name, value }));
                self.dirty = true;
            }
            (Err(e), _) => {
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
            if self.expand_alias(piece).is_some() || self.user_command(piece).is_some() {
                continue;
            }
            if let Err(e @ command::CommandError::Unknown(_)) = command::parse(piece) {
                return Err(e.to_string());
            }
        }
        Ok(())
    }

    /// Commands defined in `config.lua`; set before the config is applied.
    pub fn set_user_commands(&mut self, commands: Vec<(String, String)>) {
        self.user_commands = commands;
    }

    pub fn user_commands(&self) -> &[(String, String)] {
        &self.user_commands
    }

    /// `piece` as a call to a command defined in `config.lua`.
    fn user_command(&self, piece: &str) -> Option<Command> {
        let piece = piece.trim().trim_start_matches(':');
        let (name, args) = piece.split_once(char::is_whitespace).unwrap_or((piece, ""));
        self.user_commands
            .iter()
            .any(|(n, _)| n == name)
            .then(|| Command::User {
                name: name.to_string(),
                args: args.trim().to_string(),
            })
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

/// A command line that is a search: `/text` (forward) or `?text` (backward).
fn search_text(line: &str) -> Option<(bool, &str)> {
    if let Some(text) = line.strip_prefix('/') {
        Some((false, text))
    } else {
        line.strip_prefix('?').map(|text| (true, text))
    }
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

    fn passed_keys(outcomes: &[KeyOutcome]) -> Vec<Key> {
        outcomes
            .iter()
            .flat_map(|o| &o.effects)
            .filter_map(|e| match e {
                Effect::PassKey(k) => Some(*k),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn macros_record_and_replay() {
        let mut e = engine();
        press(&mut e, "qa");
        assert!(e.status().keystring.starts_with("recording @a"));
        press(&mut e, "jjq");
        assert!(!e.status().keystring.starts_with("recording"));
        let down = (Command::Scroll(Direction::Down), None);
        assert_eq!(runs(&press(&mut e, "@a")), vec![down.clone(); 2]);
        assert_eq!(runs(&press(&mut e, "2@a")), vec![down.clone(); 4]);
        assert_eq!(runs(&press(&mut e, "@@")), vec![down.clone(); 2]);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn macro_keys_the_engine_ignores_go_to_the_page() {
        let mut e = engine();
        press(&mut e, "qbix<Escape>q");
        let out = press(&mut e, "@b");
        assert_eq!(passed_keys(&out), vec![Key::char('x')]);
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn stopping_from_the_command_line_drops_it() {
        let mut e = engine();
        press(&mut e, "qcj:macro-record<Return>");
        let down = (Command::Scroll(Direction::Down), None);
        assert_eq!(runs(&press(&mut e, "@c")), vec![down]);
    }

    #[test]
    fn a_macro_that_runs_itself_stops() {
        let mut e = engine();
        press(&mut e, "qd@dq");
        press(&mut e, "@d");
        let message = e.status().message.map(|m| m.text).unwrap_or_default();
        assert!(message.contains("too deeply"), "{message}");
    }

    #[test]
    fn slash_searches_incrementally_and_on_return() {
        let search = |text: &str, reverse, incremental| Command::Search {
            text: text.into(),
            reverse,
            incremental,
        };
        let mut e = engine();
        let typed = press(&mut e, "/ab");
        assert_eq!(e.mode(), Mode::Command);
        assert_eq!(
            runs(&typed),
            vec![
                (search("a", false, true), None),
                (search("ab", false, true), None)
            ]
        );
        assert_eq!(
            runs(&press(&mut e, "<Return>")),
            vec![(search("ab", false, false), None)]
        );
        assert_eq!(
            runs(&press(&mut e, "n")),
            vec![(Command::SearchNext { prev: false }, None)]
        );
        assert_eq!(
            runs(&press(&mut e, "2N")),
            vec![(Command::SearchNext { prev: true }, Some(2))]
        );
        let out = press(&mut e, "?x<Escape>");
        assert_eq!(runs(&out).last(), Some(&(search("", false, true), None)));
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn caret_mode_moves_selects_and_yanks() {
        use crate::command::{CaretMove, YankWhat};
        let mut e = engine();
        press(&mut e, "v");
        assert_eq!(e.mode(), Mode::Caret);
        assert_eq!(
            runs(&press(&mut e, "3w")),
            vec![(Command::CaretMove(CaretMove::NextWord), Some(3))]
        );
        assert_eq!(
            runs(&press(&mut e, "0")),
            vec![(Command::CaretMove(CaretMove::StartOfLine), None)]
        );
        assert_eq!(
            runs(&press(&mut e, "v")),
            vec![(Command::SelectionToggle { line: false }, None)]
        );
        let out = press(&mut e, "x");
        assert!(out[0].consumed, "unbound keys stay out of the page");
        assert_eq!(
            runs(&press(&mut e, "y")),
            vec![(Command::Yank(YankWhat::Selection), None)]
        );
        press(&mut e, "<Escape>");
        assert_eq!(e.mode(), Mode::Normal);
        let out = press(&mut e, "V");
        assert_eq!(e.mode(), Mode::Caret);
        assert_eq!(
            runs(&out),
            vec![(Command::SelectionToggle { line: true }, None)]
        );
    }

    #[test]
    fn tab_switches_follow_mode_on_change() {
        let mut e = engine();
        e.set_mode(Mode::Insert, &mut Vec::new());
        e.tab_switched(Some(Mode::Normal));
        assert_eq!(e.mode(), Mode::Normal, "normal is the default");
        e.apply_config(&ConfigOp::Set {
            name: "tabs.mode_on_change".into(),
            value: Value::Str("restore".into()),
        })
        .unwrap();
        e.tab_switched(Some(Mode::Insert));
        assert_eq!(
            e.mode(),
            Mode::Insert,
            "restore brings the tab's insert mode back"
        );
        e.tab_switched(Some(Mode::Normal));
        assert_eq!(e.mode(), Mode::Normal);
        e.tab_switched(Some(Mode::Hint));
        assert_eq!(e.mode(), Mode::Normal, "transient modes aren't restored");
        e.apply_config(&ConfigOp::Set {
            name: "tabs.mode_on_change".into(),
            value: Value::Str("persist".into()),
        })
        .unwrap();
        e.set_mode(Mode::Insert, &mut Vec::new());
        e.tab_switched(Some(Mode::Normal));
        assert_eq!(e.mode(), Mode::Insert, "persist keeps insert mode");
    }

    #[test]
    fn user_commands_parse_bind_and_complete() {
        let mut e = engine();
        e.set_user_commands(vec![("wiki".into(), "Look it up".into())]);
        assert_eq!(
            runs(&[consumed(e.execute_str("wiki rust lang", None))]),
            vec![(
                Command::User {
                    name: "wiki".into(),
                    args: "rust lang".into()
                },
                None
            )]
        );
        e.apply_config(&ConfigOp::Bind {
            mode: Mode::Normal,
            keys: "gw".into(),
            command: "wiki x".into(),
        })
        .unwrap();
        press(&mut e, ":wi");
        assert!(e.completions().items.iter().any(|c| c.name == "wiki"));
        assert!(e.check_command("nope").is_err());
    }

    #[test]
    fn messages_stack_and_expire_on_their_own() {
        let mut e = engine();
        let first = e.show_message(Level::Info, "first");
        let second = e.show_message(Level::Error, "second");
        assert_eq!(e.status().message.unwrap().text, "second");
        assert_eq!(
            e.earlier_messages()
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>(),
            ["first"]
        );
        e.expire_message(second);
        assert_eq!(e.status().message.unwrap().text, "first");
        assert!(e.earlier_messages().is_empty());
        e.expire_message(first);
        assert!(e.status().message.is_none());
        e.show_message(Level::Info, "a");
        e.show_message(Level::Info, "b");
        press(&mut e, "j");
        assert!(e.status().message.is_none(), "a key clears them");
    }

    #[test]
    fn marks_take_the_next_key() {
        let mut e = engine();
        let out = press(&mut e, "`a");
        assert_eq!(
            runs(&out),
            vec![(
                Command::Mark {
                    set: true,
                    key: 'a'
                },
                None
            )]
        );
        assert_eq!(e.mode(), Mode::Normal);
        let out = press(&mut e, "'A");
        assert_eq!(
            runs(&out),
            vec![(
                Command::Mark {
                    set: false,
                    key: 'A'
                },
                None
            )]
        );
        let out = press(&mut e, "'<Escape>j");
        assert_eq!(runs(&out), vec![(Command::Scroll(Direction::Down), None)]);
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
        assert!(!press(&mut e, "<F2>")[0].consumed);
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
        assert!(
            view.items.iter().all(|c| c.detail.is_some()),
            "current values show"
        );
        press(&mut e, "_position.related ");
        let view = e.completions();
        let values: Vec<_> = view
            .items
            .iter()
            .map(|c| (c.category, c.name.as_str(), c.description.as_str()))
            .collect();
        assert!(
            values.contains(&("Values", "next", "current, default")),
            "{values:?}"
        );
        assert!(values.iter().all(|v| v.0 == "Values"));
        press(&mut e, "pr<Tab>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":set tabs.new_position.related prev"
        );
        press(&mut e, "<Escape>");
        press(&mut e, ":set -u example.com hints.mo<Tab>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":set -u example.com hints.mode "
        );
    }

    #[test]
    fn ctrl_d_deletes_the_selected_completion() {
        let mut e = engine();
        e.set_completion_source(|_, _| {
            ["https://a.org/", "https://b.org/", "https://c.org/"]
                .iter()
                .map(|u| Completion {
                    time: None,
                    detail: None,
                    category: "History",
                    name: u.to_string(),
                    description: String::new(),
                })
                .collect()
        });
        press(&mut e, "o");
        let out = press(&mut e, "<Ctrl-d>");
        assert!(all_effects(&out).is_empty(), "nothing is selected yet");
        press(&mut e, "<Tab><Tab>");
        let out = press(&mut e, "<Ctrl-d>");
        let deleted: Vec<_> = all_effects(&out)
            .into_iter()
            .filter_map(|e| match e {
                Effect::DeleteCompletion(item) => Some(item.name),
                _ => None,
            })
            .collect();
        assert_eq!(deleted, ["https://b.org/"]);
        // The next item takes the deleted one's place, without asking the source again.
        let view = e.completions();
        assert_eq!(view.items.len(), 2);
        assert_eq!(view.selected, Some(1));
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open https://c.org/"
        );
        let out = press(&mut e, "<Ctrl-c>");
        assert!(all_effects(&out).contains(&Effect::YankText {
            text: "https://c.org/".into(),
            primary: false
        }));
    }

    #[test]
    fn commands_cant_be_deleted_from_completion() {
        let mut e = engine();
        press(&mut e, ":scr<Tab>");
        let out = press(&mut e, "<Ctrl-d>");
        assert!(all_effects(&out).is_empty());
        assert!(e.status().message.unwrap().text.starts_with("Can't delete"));
        assert_eq!(e.completions().items.len(), 6);
    }

    fn set(e: &mut Engine, name: &str, value: Value) {
        e.apply_config(&ConfigOp::Set {
            name: name.into(),
            value,
        })
        .unwrap();
    }

    #[test]
    fn completion_show_and_min_chars() {
        let mut e = engine();
        set(&mut e, "completion.show", Value::Str("auto".into()));
        press(&mut e, ":scr");
        assert!(e.completions().items.is_empty(), "auto waits for Tab");
        press(&mut e, "<Tab>");
        assert!(!e.completions().items.is_empty());
        press(&mut e, "<Escape>");
        set(&mut e, "completion.show", Value::Str("never".into()));
        press(&mut e, ":scr<Tab>");
        assert!(e.completions().items.is_empty());
        press(&mut e, "<Escape>");
        set(&mut e, "completion.show", Value::Str("always".into()));
        set(&mut e, "completion.min_chars", Value::Int(3));
        press(&mut e, ":set hi");
        assert!(
            e.completions().items.is_empty(),
            "two characters is too few"
        );
        press(&mut e, "n");
        assert!(!e.completions().items.is_empty());
        press(&mut e, "<Escape>");
        press(&mut e, ":scr");
        assert!(
            !e.completions().items.is_empty(),
            "command names don't wait"
        );
    }

    #[test]
    fn completion_quick_takes_a_lone_item_and_moves_on() {
        let mut e = engine();
        press(&mut e, ":tab-onl<Tab>");
        assert_eq!(e.status().command_line.unwrap().text, ":tab-only ");
        assert_eq!(
            e.completions().selected,
            None,
            "the next part completes afresh"
        );
        press(&mut e, "<Escape>");
        set(&mut e, "completion.quick", Value::Bool(false));
        press(&mut e, ":tab-onl<Tab>");
        assert_eq!(e.completions().selected, Some(0));
    }

    #[test]
    fn use_best_match_runs_the_first_command_a_prefix_matches() {
        let mut e = engine();
        press(&mut e, ":relo<Return>");
        assert!(e.status().message.unwrap().text.contains("no such command"));
        set(&mut e, "completion.use_best_match", Value::Bool(true));
        assert!(matches!(
            runs(&press(&mut e, ":relo<Return>")).as_slice(),
            [(Command::Reload { .. }, None)]
        ));
        // A known command name is left alone.
        assert!(matches!(
            runs(&press(&mut e, ":stop<Return>")).as_slice(),
            [(Command::Stop, None)]
        ));
    }

    #[test]
    fn ctrl_v_and_shift_insert_paste_into_the_command_line() {
        let mut e = engine();
        e.set_clipboard_reader(|| Some("https://a.org/x\n".into()));
        e.set_primary_reader(|| Some("two\nlines".into()));
        press(&mut e, ":open <Ctrl-v>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open https://a.org/x"
        );
        press(&mut e, " <Shift-Insert>");
        assert_eq!(
            e.status().command_line.unwrap().text,
            ":open https://a.org/x two lines"
        );
        press(&mut e, "<Escape>");
        e.push_prompt(prompt(
            1,
            PromptKind::Text {
                default: String::new(),
                masked: false,
                path: false,
            },
        ));
        press(&mut e, "<Ctrl-v>");
        assert_eq!(e.prompt_view().unwrap().input, "https://a.org/x");
    }

    #[test]
    fn match_counts_and_partial_keys() {
        let mut e = engine();
        press(&mut e, "3g");
        let generation = e.partial_keys().expect("3g is half typed");
        press(&mut e, "g");
        assert_eq!(e.partial_keys(), None, "gg finished the chain");
        press(&mut e, "g");
        let stale = generation;
        let current = e.partial_keys().unwrap();
        e.expire_partial(stale);
        assert!(
            e.partial_keys().is_some(),
            "an older timer doesn't clear newer keys"
        );
        e.expire_partial(current);
        assert_eq!(e.partial_keys(), None);
        assert_eq!(e.status().keystring, "");
        set(&mut e, "input.match_counts", Value::Bool(false));
        e.keymap.bind(Mode::Normal, "1", "tab-focus 1").unwrap();
        assert!(matches!(
            runs(&press(&mut e, "1")).as_slice(),
            [(Command::TabFocus(_), None)]
        ));
    }

    #[test]
    fn mode_override_follows_the_page() {
        let mut e = engine();
        e.settings
            .set_for(
                "term.example",
                "input.mode_override",
                Value::Str("passthrough".into()),
            )
            .unwrap();
        e.set_url("https://term.example/");
        e.apply_mode_override();
        assert_eq!(e.mode(), Mode::Passthrough);
        e.set_url("https://other.example/");
        e.tab_switched(Some(Mode::Normal));
        assert_eq!(e.mode(), Mode::Normal);
        e.set_url("https://term.example/");
        press(&mut e, ":");
        e.apply_mode_override();
        assert_eq!(
            e.mode(),
            Mode::Command,
            "the command line isn't interrupted"
        );
    }

    #[test]
    fn command_history_is_capped() {
        let mut e = engine();
        set(&mut e, "completion.cmd_history_max_items", Value::Int(2));
        for line in [":reload<Return>", ":stop<Return>", ":back<Return>"] {
            press(&mut e, line);
        }
        press(&mut e, ":<Up><Up><Up>");
        assert_eq!(e.status().command_line.unwrap().text, ":stop");
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
                    time: None,
                    detail: None,
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
        assert_eq!(
            names,
            vec![
                "scroll",
                "scroll-page",
                "scroll-to-perc",
                "screenshot",
                "scroll-px",
                "scroll-to-anchor"
            ]
        );
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
        e.tab_switched(None);
        assert_eq!(e.mode(), Mode::Normal);
        press(&mut e, ":");
        e.tab_switched(None);
        assert_eq!(e.mode(), Mode::Command);
    }

    fn hint_request(target: HintTarget, rapid: bool, fill: Option<&str>) -> HintRequest {
        HintRequest {
            group: "all".into(),
            target,
            rapid,
            fill: fill.map(String::from),
        }
    }

    fn items(n: usize) -> Vec<HintItem> {
        (0..n)
            .map(|i| HintItem {
                text: String::new(),
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
    fn word_hints_need_a_dictionary() {
        let mut e = engine();
        set(&mut e, "hints.mode", Value::Str("word".into()));
        e.start_hints(hint_request(HintTarget::Current, false, None), items(2));
        assert_eq!(e.hint_session().unwrap().labels, ["a", "s"]);
        assert!(
            e.status()
                .message
                .unwrap()
                .text
                .contains("hints.dictionary")
        );
        press(&mut e, "<Escape>");
        e.set_hint_words(std::rc::Rc::from(vec!["go".to_string(), "up".to_string()]));
        e.start_hints(hint_request(HintTarget::Current, false, None), items(2));
        assert_eq!(e.hint_session().unwrap().labels, ["go", "up"]);
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

    #[test]
    fn keyhints_list_what_pending_keys_can_become() {
        let mut e = engine();
        assert!(e.keyhints().is_none());
        press(&mut e, "g");
        let (prefix, items) = e.keyhints().unwrap();
        assert_eq!(prefix, "g");
        assert!(items.contains(&("g".to_string(), "scroll-to-perc 0".to_string())));
        assert!(items.iter().all(|(rest, _)| !rest.is_empty()));
        let order: Vec<&str> = items.iter().map(|(rest, _)| rest.as_str()).collect();
        let pos = |k| order.iter().position(|r| *r == k).unwrap();
        assert!(
            pos("g") < pos("t")
                && pos("t") < pos("T")
                && pos("T") < pos("0")
                && pos("0") < pos("$")
        );
        press(&mut e, "<Escape>");
        e.apply_config(&ConfigOp::Set {
            name: "keyhint.blacklist".into(),
            value: Value::List(vec!["gg".into(), "g*".into()]),
        })
        .unwrap();
        press(&mut e, "g");
        assert!(e.keyhints().is_none(), "every g chain is blacklisted");
    }

    #[test]
    fn key_mappings_apply_in_every_mode() {
        let mut e = engine();
        press(&mut e, "i");
        assert_eq!(e.mode(), Mode::Insert);
        press(&mut e, "<Ctrl-[>");
        assert_eq!(e.mode(), Mode::Normal, "Ctrl-[ is Escape");
        press(&mut e, ":open x");
        let out = press(&mut e, "<Ctrl-m>");
        assert_eq!(e.mode(), Mode::Normal, "Ctrl-m is Return");
        assert!(!runs(&out).is_empty());
        e.apply_config(&ConfigOp::Set {
            name: "bindings.key_mappings".into(),
            value: Value::Map([("x".to_string(), "j".to_string())].into()),
        })
        .unwrap();
        assert_eq!(runs(&press(&mut e, "x")), runs(&press(&mut e, "j")));
        assert!(
            settings::find("bindings.key_mappings")
                .unwrap()
                .from_json(&serde_json::json!({"ab": "c"}))
                .is_err()
        );
    }

    #[test]
    fn dot_repeats_the_last_command() {
        let mut e = engine();
        assert!(runs(&press(&mut e, ".")).is_empty());
        assert_eq!(e.status().message.unwrap().text, "No command to repeat yet");
        assert_eq!(
            runs(&press(&mut e, "3+")),
            vec![(Command::ZoomStep { out: false }, Some(3))]
        );
        assert_eq!(
            runs(&press(&mut e, ".")),
            vec![(Command::ZoomStep { out: false }, Some(3))]
        );
        assert_eq!(
            runs(&press(&mut e, "2.")),
            vec![(Command::ZoomStep { out: false }, Some(2))]
        );
        press(&mut e, ":reload<Return>");
        press(&mut e, ":");
        press(&mut e, "<Escape>");
        assert_eq!(
            runs(&press(&mut e, ".")),
            vec![(Command::Reload { force: false }, None)]
        );
    }

    #[test]
    fn config_list_and_dict_commands() {
        let mut e = engine();
        press(
            &mut e,
            ":config-list-add url.start_pages https://a.org/<Return>",
        );
        assert_eq!(
            e.settings().list("url.start_pages").last().unwrap(),
            "https://a.org/"
        );
        press(
            &mut e,
            ":config-list-remove url.start_pages https://a.org/<Return>",
        );
        assert!(
            !e.settings()
                .list("url.start_pages")
                .contains(&"https://a.org/".to_string())
        );
        press(
            &mut e,
            ":config-dict-add url.searchengines ddg https://duckduckgo.com/?q={}<Return>",
        );
        assert_eq!(
            e.settings().map("url.searchengines").unwrap()["ddg"],
            "https://duckduckgo.com/?q={}"
        );
        press(
            &mut e,
            ":config-dict-add url.searchengines ddg https://x.org/?q={}<Return>",
        );
        assert!(e.status().message.unwrap().text.contains("--replace"));
        press(
            &mut e,
            ":config-dict-add url.searchengines ddg nope<Return>",
        );
        assert_eq!(
            e.settings().map("url.searchengines").unwrap()["ddg"],
            "https://duckduckgo.com/?q={}"
        );
        press(&mut e, ":config-dict-remove url.searchengines ddg<Return>");
        assert!(
            !e.settings()
                .map("url.searchengines")
                .unwrap()
                .contains_key("ddg")
        );
        press(&mut e, ":config-list-add hints.chars x<Return>");
        assert!(
            e.status()
                .message
                .unwrap()
                .text
                .contains("not a list setting")
        );
        press(&mut e, ":set hints.chars qwer<Return>");
        let out = press(&mut e, ":config-clear<Return>");
        assert_eq!(e.settings().str("hints.chars"), "asdfghjkl");
        assert!(config_changes(&out).contains(&ConfigOp::Unset {
            name: "hints.chars".into()
        }));
    }

    #[test]
    fn config_cycle_and_unset() {
        let mut e = engine();
        press(&mut e, ":config-cycle hints.mode number letter<Return>");
        assert_eq!(e.settings().str("hints.mode"), "number");
        press(&mut e, ":config-cycle hints.mode number letter<Return>");
        assert_eq!(e.settings().str("hints.mode"), "letter");
        let before = e.settings().bool("search.incremental");
        press(&mut e, ":config-cycle search.incremental<Return>");
        assert_eq!(e.settings().bool("search.incremental"), !before);
        press(&mut e, ":set hints.chars qwer<Return>");
        let out = press(&mut e, ":config-unset hints.chars<Return>");
        assert_eq!(e.settings().str("hints.chars"), "asdfghjkl");
        assert_eq!(
            config_changes(&out),
            vec![ConfigOp::Unset {
                name: "hints.chars".into()
            }]
        );
    }

    #[test]
    fn message_commands_show_and_clear() {
        let mut e = engine();
        press(&mut e, ":message-warning careful<Return>");
        let message = e.status().message.unwrap();
        assert_eq!(
            (message.level, message.text.as_str()),
            (Level::Warning, "careful")
        );
        press(&mut e, ":clear-messages<Return>");
        assert!(e.status().message.is_none());
        assert_eq!(e.message_log().len(), 1);
    }

    #[test]
    fn messages_are_logged_beyond_the_screen() {
        let mut e = engine();
        for i in 0..8 {
            e.show_message(Level::Info, format!("m{i}"));
        }
        let log = e.message_log();
        assert_eq!(log.len(), 8);
        assert_eq!(log[0].text, "m0");
        assert_eq!(log[7].text, "m7");
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
    fn alt_e_asks_for_a_folder_in_file_prompts_only() {
        let mut e = engine();
        let text = |path| PromptKind::Text {
            default: "~/x.txt".into(),
            masked: false,
            path,
        };
        e.push_prompt(prompt(1, text(true)));
        let out = press(&mut e, "<Alt-e>");
        assert_eq!(runs(&out), vec![(Command::PromptFileselectExternal, None)]);
        e.set_path_prompt_text("/tmp/picked/");
        assert_eq!(e.prompt_view().unwrap().input, "/tmp/picked/");
        press(&mut e, "<Escape>");
        e.push_prompt(prompt(2, text(false)));
        assert!(runs(&press(&mut e, "<Alt-e>")).is_empty());
        e.set_path_prompt_text("/nope");
        assert_eq!(e.prompt_view().unwrap().input, "~/x.txt");
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
                path: false,
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
                path: false,
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
                    remember: Remember::Always,
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
                path: false,
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
