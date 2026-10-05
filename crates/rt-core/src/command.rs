use thiserror::Error;

use crate::hints::{HintRequest, HintTarget};
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
    /// The text selected in caret mode.
    Selection,
}

/// Where `:navigate` goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigateTo {
    Up,
    Prev,
    Next,
    Increment,
    Decrement,
}

/// Caret movements, with qutebrowser's command names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretMove {
    NextChar,
    PrevChar,
    NextLine,
    PrevLine,
    NextWord,
    PrevWord,
    EndOfWord,
    StartOfLine,
    EndOfLine,
    StartOfDocument,
    EndOfDocument,
    PrevParagraph,
    NextParagraph,
}

impl CaretMove {
    pub const ALL: [(&'static str, CaretMove); 13] = [
        ("move-to-next-char", CaretMove::NextChar),
        ("move-to-prev-char", CaretMove::PrevChar),
        ("move-to-next-line", CaretMove::NextLine),
        ("move-to-prev-line", CaretMove::PrevLine),
        ("move-to-next-word", CaretMove::NextWord),
        ("move-to-prev-word", CaretMove::PrevWord),
        ("move-to-end-of-word", CaretMove::EndOfWord),
        ("move-to-start-of-line", CaretMove::StartOfLine),
        ("move-to-end-of-line", CaretMove::EndOfLine),
        ("move-to-start-of-document", CaretMove::StartOfDocument),
        ("move-to-end-of-document", CaretMove::EndOfDocument),
        ("move-to-prev-block", CaretMove::PrevParagraph),
        ("move-to-next-block", CaretMove::NextParagraph),
    ];

    /// The `Selection.modify` direction and granularity for this move.
    pub fn js(self) -> (&'static str, &'static str) {
        match self {
            CaretMove::NextChar => ("forward", "character"),
            CaretMove::PrevChar => ("backward", "character"),
            CaretMove::NextLine => ("forward", "line"),
            CaretMove::PrevLine => ("backward", "line"),
            CaretMove::NextWord | CaretMove::EndOfWord => ("forward", "word"),
            CaretMove::PrevWord => ("backward", "word"),
            CaretMove::StartOfLine => ("backward", "lineboundary"),
            CaretMove::EndOfLine => ("forward", "lineboundary"),
            CaretMove::StartOfDocument => ("backward", "documentboundary"),
            CaretMove::EndOfDocument => ("forward", "documentboundary"),
            CaretMove::PrevParagraph => ("backward", "paragraph"),
            CaretMove::NextParagraph => ("forward", "paragraph"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Prev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementFilter {
    Id,
    Css,
    Focused,
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
    /// The same, into the primary selection (`yank -s`).
    YankPrimary(YankWhat),
    /// `:set [name[?|!]] [value]`
    Set {
        name: Option<String>,
        value: Option<String>,
        /// `-u <pattern>`: only for matching pages.
        pattern: Option<String>,
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
    /// Fill a file-name prompt with a folder from `fileselect.folder.command`.
    PromptFileselectExternal,
    /// Follow the hint with this label, or the one waiting for Return.
    HintFollow {
        label: Option<String>,
    },
    /// Delete the selected completion: a history entry, quickmark,
    /// bookmark or session, or close a tab.
    CompletionItemDel,
    /// Yank the selected completion's text (to the primary selection with `sel`).
    CompletionItemYank {
        sel: bool,
    },
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
    /// Import qutebrowser's history.sqlite (default: qutebrowser's data dir).
    HistoryImport {
        path: Option<String>,
    },
    /// Download the filter lists and rebuild the content blocker.
    AdblockUpdate,
    /// Run an external program, or a userscript with `userscript`.
    Spawn {
        userscript: bool,
        /// Report when the program exits successfully, too.
        verbose: bool,
        /// Show the program's output as messages.
        output_messages: bool,
        /// Show the program's output in a new tab.
        output: bool,
        /// Set by hints: the URL a hinted userscript gets as `QUTE_URL`.
        hint_url: Option<String>,
        /// Don't wait for the program or report on it.
        detach: bool,
        argv: Vec<String>,
    },
    /// Edit the focused text field in an external editor.
    OpenEditor,
    /// Read the Greasemonkey scripts again.
    GreasemonkeyReload,
    /// Complete the file path typed in a path prompt.
    PromptComplete,
    /// Show this session's downloads.
    Downloads,
    /// Close the current window.
    Close,
    /// Set the zoom to `percent`, or to `zoom.default` (or the count).
    Zoom {
        percent: Option<u32>,
    },
    /// Zoom in (or out) by the count's number of levels.
    ZoomStep {
        out: bool,
    },
    /// Open Chromium's developer tools for the current tab.
    DevTools,
    /// Print the page, or save it as a PDF.
    Print {
        pdf: Option<String>,
    },
    /// Toggle fullscreen for the window.
    Fullscreen,
    /// Save what the current tab shows as an image.
    Screenshot {
        path: String,
        force: bool,
    },
    /// Show the page's source in a new tab.
    ViewSource,
    /// Evaluate JavaScript in the page and show the result.
    JsEval {
        code: String,
    },
    /// Open the start page in the current tab.
    Home,
    /// Mute or unmute the current tab.
    TabMute,
    /// Show the messages of this session.
    Messages,
    /// Run the last command again (`.`).
    RepeatCommand,
    /// Edit `url` (or the current page's) in `editor.command`, then open it.
    EditUrl {
        target: OpenTarget,
        related: bool,
        url: Option<String>,
    },
    /// Edit the command line in `editor.command`, then put it back, or run it.
    CmdEdit {
        run: bool,
    },
    /// Scroll by pixels.
    ScrollPx {
        x: i64,
        y: i64,
    },
    /// Run `command` after `ms` milliseconds.
    Later {
        ms: u64,
        command: String,
    },
    /// Show a message, from a binding or script.
    Message {
        level: crate::engine::Level,
        text: String,
    },
    ClearMessages,
    /// Set a setting to the value after its current one in `values`, or
    /// toggle a true/false setting.
    ConfigCycle {
        name: String,
        values: Vec<String>,
    },
    /// Put a setting back to its default.
    ConfigUnset {
        name: String,
    },
    /// Type text into the focused field.
    InsertText {
        text: String,
    },
    /// Send keys to the page, or with `global` to the browser itself.
    FakeKey {
        keys: String,
        global: bool,
    },
    /// Click an element chosen by id or CSS selector, or the focused one.
    ClickElement {
        filter: ElementFilter,
        value: String,
    },
    /// Scroll to the element with this id or name.
    ScrollToAnchor {
        name: String,
    },
    /// Close every other window.
    WindowOnly,
    /// Do nothing; for unbinding a key without falling through to the page.
    Nop,
    /// Follow the link around the selection (e.g. a search match) or the focused link.
    SelectionFollow {
        tab: bool,
    },
    /// Duplicate the current tab, in the background or a new window.
    TabClone {
        background: bool,
        window: bool,
    },
    /// Move the current tab to window `window` (1-based), or to a new window.
    TabGive {
        window: Option<usize>,
    },
    /// Move tab `window/tab` from another window into this one.
    TabTake {
        target: String,
    },
    /// Show the browsing history page, in a new tab with `tab`.
    History {
        tab: bool,
    },
    /// Go to a tab in any window: `window/tab` (1-based) or text in its title or URL.
    TabSelect {
        target: String,
    },
    /// Run the Lua function bound with `rt.bind(keys, function)`.
    LuaCall {
        id: u32,
    },
    /// A command defined in `config.lua` with `rt.command`.
    User {
        name: String,
        args: String,
    },
    /// Go up the URL, to the previous/next page, or change the number in it.
    Navigate {
        to: NavigateTo,
        tab: bool,
    },
    /// Find `text` in the page (backwards with `reverse`); empty clears the search.
    /// `incremental` searches update as the user types.
    Search {
        text: String,
        reverse: bool,
        incremental: bool,
    },
    /// Go to the next (or with `prev`, previous) match of the last search.
    SearchNext {
        prev: bool,
    },
    /// Remember (`set`) or go back to the scroll position named `key`.
    /// Lowercase marks belong to the page, uppercase ones also remember the URL.
    Mark {
        set: bool,
        key: char,
    },
    CaretMove(CaretMove),
    /// Start or stop selecting in caret mode; `line` selects whole lines.
    SelectionToggle {
        line: bool,
    },
    /// Swap the selection's anchor and focus.
    SelectionReverse,
    /// Start recording keys into `register`, or stop when already recording.
    MacroRecord {
        register: Option<char>,
    },
    /// Replay the keys in `register` (`@` is the last macro run).
    MacroRun {
        register: Option<char>,
    },
    /// Offer replacements for the misspelled word at the cursor.
    SpellSuggest,
    SpellReplace {
        word: String,
    },
    /// Add the word from the last `:spell-suggest` to the dictionary.
    SpellAdd,
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
    /// Start a failed or cancelled download again.
    DownloadRetry,
    /// Take a download off the list, cancelling it if it's running; `all`
    /// takes every finished one.
    DownloadRemove {
        all: bool,
    },
    /// Delete a finished download's file and take it off the list.
    DownloadDelete,
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
    spec(
        "tab-clone",
        "Duplicate the current tab: :tab-clone [-b] [-w]",
    ),
    spec(
        "tab-give",
        "Move the current tab to window N, or to a new window: :tab-give [N]",
    ),
    spec(
        "tab-take",
        "Move a tab from another window here: :tab-take <window/tab>",
    ),
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
        "history-import",
        "Import qutebrowser's history: :history-import [path to history.sqlite]",
    ),
    spec(
        "adblock-update",
        "Download the filter lists in content.blocking.adblock.lists",
    ),
    spec(
        "spell-suggest",
        "Suggest fixes for the misspelled word at the cursor (insert mode)",
    ),
    spec(
        "spell-replace",
        "Replace the misspelled word: :spell-replace <word>",
    ),
    spec(
        "spell-add",
        "Add the word from the last :spell-suggest to your dictionary",
    ),
    spec(
        "spawn",
        "Run a program: :spawn [-u] [-v] [-m] [-o] [-d] <cmd> [args]; -u runs a userscript",
    ),
    spec(
        "open-editor",
        "Edit the focused text field in editor.command (also :edit-text)",
    ),
    spec(
        "edit-text",
        "Edit the focused text field in editor.command (qutebrowser's name for :open-editor)",
    ),
    spec(
        "edit-url",
        "Edit the page's URL in editor.command, then open it: [-t|-b|-w|-p] [-r] [url]",
    ),
    spec(
        "cmd-edit",
        "Edit the command line in editor.command, then put it back: [--run] runs it instead",
    ),
    spec(
        "greasemonkey-reload",
        "Read the scripts in the greasemonkey directories again",
    ),
    spec(
        "close",
        "Close the current window (:quit closes all of them)",
    ),
    spec(
        "tab-select",
        "Go to a tab in any window: :tab-select <window/tab | text> (T)",
    ),
    spec("history", "Show the browsing history: :history [-t]"),
    hidden("prompt-complete", "Complete the file path in the prompt"),
    spec(
        "selection-follow",
        "Follow the link around the selection, e.g. after a search (Return; -t: new tab)",
    ),
    spec(
        "zoom",
        "Set the zoom: :zoom [percent] (=; no value: zoom.default)",
    ),
    spec("zoom-in", "Zoom in a level (+; a count zooms further)"),
    spec("zoom-out", "Zoom out a level (-)"),
    spec("devtools", "Open the developer tools for this tab (wi)"),
    spec("print", "Print the page, or save it: :print [--pdf file]"),
    spec("fullscreen", "Toggle fullscreen (F11)"),
    spec(
        "screenshot",
        "Save what the tab shows as an image: :screenshot [--force] file (.png, .jpg or .webp)",
    ),
    spec("view-source", "Show the page source in a new tab (gf)"),
    spec(
        "jseval",
        "Evaluate a JavaScript expression in the page: :jseval <code>",
    ),
    spec("home", "Open the start page"),
    spec("tab-mute", "Mute or unmute this tab (Alt-m)"),
    spec("messages", "Show this session's messages"),
    spec("repeat-command", "Run the last command again (.)"),
    spec("scroll-px", "Scroll by pixels: :scroll-px <dx> <dy>"),
    spec(
        "cmd-later",
        "Run a command later: :cmd-later <ms> <command>",
    ),
    spec("message-info", "Show a message: :message-info <text>"),
    spec("message-warning", "Show a warning: :message-warning <text>"),
    spec("message-error", "Show an error: :message-error <text>"),
    spec("clear-messages", "Take the messages off the screen"),
    spec(
        "config-cycle",
        "Cycle a setting: :config-cycle <option> [values…] (no values: toggle)",
    ),
    spec(
        "config-unset",
        "Put a setting back to its default: :config-unset <option>",
    ),
    spec(
        "insert-text",
        "Type text into the focused field: :insert-text <text>",
    ),
    spec(
        "fake-key",
        "Send keys to the page: :fake-key [-g] <keys> (-g: to the browser)",
    ),
    spec(
        "click-element",
        "Click an element: :click-element id|css|focused [value]",
    ),
    spec(
        "scroll-to-anchor",
        "Scroll to the element with this id or name",
    ),
    spec("window-only", "Close every other window"),
    spec("nop", "Do nothing (to make a key do nothing)"),
    hidden("lua-call", "Run a Lua function bound in config.lua"),
    spec(
        "navigate",
        "Go up, prev, next, increment or decrement the URL: :navigate <where> [-t]",
    ),
    spec(
        "search",
        "Find text in the page: :search [-r] [text] (no text clears it); / and ? type one",
    ),
    spec("search-next", "Go to the next match of the last search"),
    spec("search-prev", "Go to the previous match of the last search"),
    spec(
        "set-mark",
        "Remember the scroll position as a mark: a-z for this page, A-Z with its URL",
    ),
    spec(
        "jump-mark",
        "Go back to a mark; ' is where the last jump started",
    ),
    spec(
        "macro-record",
        "Record keys into a register until macro-record again (q + register)",
    ),
    spec(
        "macro-run",
        "Replay a macro (@ + register; @@ repeats the last one; a count repeats it)",
    ),
    spec(
        "selection-toggle",
        "Start or stop selecting in caret mode (--line selects whole lines)",
    ),
    spec(
        "selection-reverse",
        "Swap the ends of the selection (caret mode)",
    ),
    hidden("move-to-next-char", "Move the caret (caret mode)"),
    hidden("move-to-prev-char", "Move the caret (caret mode)"),
    hidden("move-to-next-line", "Move the caret (caret mode)"),
    hidden("move-to-prev-line", "Move the caret (caret mode)"),
    hidden("move-to-next-word", "Move the caret (caret mode)"),
    hidden("move-to-prev-word", "Move the caret (caret mode)"),
    hidden("move-to-end-of-word", "Move the caret (caret mode)"),
    hidden("move-to-start-of-line", "Move the caret (caret mode)"),
    hidden("move-to-end-of-line", "Move the caret (caret mode)"),
    hidden("move-to-start-of-document", "Move the caret (caret mode)"),
    hidden("move-to-end-of-document", "Move the caret (caret mode)"),
    hidden("move-to-prev-block", "Move the caret (caret mode)"),
    hidden("move-to-next-block", "Move the caret (caret mode)"),
    spec("download", "Download a URL (default: the current page)"),
    spec("download-cancel", "Cancel a download (count: its number)"),
    spec(
        "download-open",
        "Open a finished download (count: its number)",
    ),
    spec("download-clear", "Remove finished downloads from the list"),
    spec(
        "download-retry",
        "Start a failed or cancelled download again (count: its number)",
    ),
    spec(
        "download-remove",
        "Take a download off the list, cancelling it if it runs (count: its number; --all: every finished one)",
    ),
    spec(
        "download-delete",
        "Delete a finished download's file and take it off the list (count: its number)",
    ),
    spec(
        "downloads",
        "List this session's downloads and their progress",
    ),
    spec(
        "quit",
        "Quit the browser; --save keeps the tabs as the default session",
    ),
    hidden(
        "completion-item-focus",
        "Select the next or previous completion",
    ),
    spec(
        "prompt-fileselect-external",
        "In a file prompt, pick the folder with fileselect.folder.command (Alt-e)",
    ),
    spec(
        "hint-follow",
        "Follow the hint with this label, or the match waiting for Return (Return in hint mode)",
    ),
    spec(
        "completion-item-del",
        "Delete the selected completion: history entry, quickmark, bookmark or session, or close the tab (Ctrl-d)",
    ),
    spec(
        "completion-item-yank",
        "Yank the selected completion's text: [--sel] for the primary selection (Ctrl-c)",
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
        "cmd-set-text" | "set-cmd-text" => {
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
        "tab-clone" => {
            let (mut background, mut window) = (false, false);
            while let Some(flag) = args.flag(&["-b", "--bg", "-w", "--window"]) {
                match flag {
                    "-b" | "--bg" => background = true,
                    _ => window = true,
                }
            }
            Command::TabClone { background, window }
        }
        "tab-give" => Command::TabGive {
            window: match args.optional() {
                None => None,
                Some(n) => Some(
                    n.parse()
                        .map_err(|_| args.error(format!("not a window number: {n:?}")))?,
                ),
            },
        },
        "tab-take" => Command::TabTake {
            target: args.required("window/tab")?.to_string(),
        },
        "undo" => Command::Undo,
        "hint" => {
            let rapid = args.flag(&["-r", "--rapid"]).is_some();
            let group = args.optional().unwrap_or("all").to_string();
            let target = match args.optional() {
                Some(t) => t.parse::<HintTarget>().map_err(|e| args.error(e))?,
                None => HintTarget::Normal,
            };
            let fill = args.rest();
            if matches!(
                target,
                HintTarget::Fill | HintTarget::Spawn | HintTarget::Userscript
            ) && fill.is_empty()
            {
                return Err(args.error(format!("the {target} target needs more text")));
            }
            Command::Hint(HintRequest {
                group,
                target,
                rapid,
                fill: (!fill.is_empty()).then(|| fill.to_string()),
            })
        }
        "yank" => {
            let primary = args.flag(&["-s", "--sel"]).is_some();
            let what = match args.optional() {
                None | Some("url") => YankWhat::Url,
                Some("title") => YankWhat::Title,
                Some("domain") => YankWhat::Domain,
                Some("selection") => YankWhat::Selection,
                Some(other) => return Err(args.error(format!("cannot yank {other:?}"))),
            };
            if primary {
                Command::YankPrimary(what)
            } else {
                Command::Yank(what)
            }
        }
        "set" => {
            let pattern = match args.flag(&["-u", "--pattern"]) {
                Some(_) => Some(args.required("pattern")?.to_string()),
                None => None,
            };
            let name = args.optional().map(String::from);
            let value = args.rest();
            Command::Set {
                name,
                value: (!value.is_empty()).then(|| value.to_string()),
                pattern,
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
        "download-retry" => Command::DownloadRetry,
        "download-remove" => Command::DownloadRemove {
            all: args.flag(&["-a", "--all"]).is_some(),
        },
        "download-delete" => Command::DownloadDelete,
        "download-clear" => Command::DownloadClear,
        "adblock-update" => Command::AdblockUpdate,
        "spawn" => {
            let (mut userscript, mut verbose, mut output_messages, mut output, mut detach) =
                (false, false, false, false, false);
            while let Some(flag) = args.flag(&[
                "-u",
                "--userscript",
                "-v",
                "--verbose",
                "-m",
                "--output-messages",
                "-o",
                "--output",
                "-d",
                "--detach",
            ]) {
                match flag {
                    "-u" | "--userscript" => userscript = true,
                    "-v" | "--verbose" => verbose = true,
                    "-m" | "--output-messages" => output_messages = true,
                    "-o" | "--output" => output = true,
                    _ => detach = true,
                }
            }
            let argv = crate::shell_words::split(args.rest()).map_err(|e| args.error(e))?;
            if argv.is_empty() {
                return Err(args.error("missing argument: command".to_string()));
            }
            Command::Spawn {
                userscript,
                verbose,
                output_messages,
                output,
                hint_url: None,
                detach,
                argv,
            }
        }
        "open-editor" | "edit-text" => Command::OpenEditor,
        "edit-url" => {
            let (target, related) = args.open_target();
            let url = args.rest();
            Command::EditUrl {
                target,
                related,
                url: (!url.is_empty()).then(|| url.to_string()),
            }
        }
        "cmd-edit" => Command::CmdEdit {
            run: args.flag(&["-r", "--run"]).is_some(),
        },
        "greasemonkey-reload" => Command::GreasemonkeyReload,
        "prompt-complete" => Command::PromptComplete,
        "downloads" => Command::Downloads,
        "close" => Command::Close,
        "zoom" => Command::Zoom {
            percent: match args.optional() {
                None => None,
                Some(p) => Some(
                    p.trim_end_matches('%')
                        .parse()
                        .map_err(|_| args.error(format!("not a zoom level: {p:?}")))?,
                ),
            },
        },
        "zoom-in" => Command::ZoomStep { out: false },
        "zoom-out" => Command::ZoomStep { out: true },
        "devtools" => Command::DevTools,
        "print" => Command::Print {
            pdf: match args.flag(&["-p", "--pdf"]) {
                Some(_) => Some(args.required("file")?.to_string()),
                None => None,
            },
        },
        "fullscreen" => Command::Fullscreen,
        "screenshot" => {
            let force = args.flag(&["-f", "--force"]).is_some();
            Command::Screenshot {
                path: args.required("file")?.to_string(),
                force,
            }
        }
        "view-source" => Command::ViewSource,
        "jseval" => {
            let code = args.rest();
            if code.is_empty() {
                return Err(args.error("missing argument: code".to_string()));
            }
            Command::JsEval {
                code: code.to_string(),
            }
        }
        "home" => Command::Home,
        "tab-mute" => Command::TabMute,
        "messages" => Command::Messages,
        "repeat-command" => Command::RepeatCommand,
        "scroll-px" => {
            let x = args.required("dx")?;
            let y = args.required("dy")?;
            let parse = |v: &str, name: &str| {
                v.parse::<i64>()
                    .map_err(|_| args.error(format!("{name} is not a number: {v:?}")))
            };
            Command::ScrollPx {
                x: parse(x, "dx")?,
                y: parse(y, "dy")?,
            }
        }
        "cmd-later" | "later" => {
            let ms = args.required("ms")?;
            let ms = ms
                .parse()
                .map_err(|_| args.error(format!("not a number of milliseconds: {ms:?}")))?;
            let command = args.rest();
            if command.is_empty() {
                return Err(args.error("missing argument: command"));
            }
            Command::Later {
                ms,
                command: command.to_string(),
            }
        }
        "message-info" | "message-warning" | "message-error" => {
            let text = args.rest();
            if text.is_empty() {
                return Err(args.error("missing argument: text"));
            }
            let level = match name {
                "message-info" => crate::engine::Level::Info,
                "message-warning" => crate::engine::Level::Warning,
                _ => crate::engine::Level::Error,
            };
            Command::Message {
                level,
                text: text.to_string(),
            }
        }
        "clear-messages" => Command::ClearMessages,
        "config-cycle" => {
            let name = args.required("option")?.to_string();
            let mut values = Vec::new();
            while let Some(v) = args.optional() {
                values.push(v.to_string());
            }
            Command::ConfigCycle { name, values }
        }
        "config-unset" => Command::ConfigUnset {
            name: args.required("option")?.to_string(),
        },
        "insert-text" => {
            let text = args.rest();
            if text.is_empty() {
                return Err(args.error("missing argument: text"));
            }
            Command::InsertText {
                text: text.to_string(),
            }
        }
        "fake-key" => {
            let global = args.flag(&["-g", "--global"]).is_some();
            let keys = args.rest();
            if keys.is_empty() {
                return Err(args.error("missing argument: keys"));
            }
            crate::key::Key::parse_sequence(keys).map_err(|e| args.error(e.to_string()))?;
            Command::FakeKey {
                keys: keys.to_string(),
                global,
            }
        }
        "click-element" => {
            let filter = match args.required("filter")? {
                "id" => ElementFilter::Id,
                "css" => ElementFilter::Css,
                "focused" => ElementFilter::Focused,
                other => {
                    return Err(
                        args.error(format!("unknown filter {other:?} (id, css or focused)"))
                    );
                }
            };
            let value = args.rest();
            if value.is_empty() && filter != ElementFilter::Focused {
                return Err(args.error("missing argument: value"));
            }
            Command::ClickElement {
                filter,
                value: value.to_string(),
            }
        }
        "scroll-to-anchor" => Command::ScrollToAnchor {
            name: args.required("name")?.to_string(),
        },
        "window-only" => Command::WindowOnly,
        "nop" => Command::Nop,
        "selection-follow" => Command::SelectionFollow {
            tab: args.flag(&["-t", "--tab"]).is_some(),
        },
        "history" => Command::History {
            tab: args.flag(&["-t", "--tab"]).is_some(),
        },
        "tab-select" => Command::TabSelect {
            target: args.rest().to_string(),
        },
        "lua-call" => Command::LuaCall {
            id: args
                .required("id")?
                .parse()
                .map_err(|_| args.error("lua-call takes a callback number".to_string()))?,
        },
        "navigate" => {
            let to = match args.required("where")? {
                "up" => NavigateTo::Up,
                "prev" => NavigateTo::Prev,
                "next" => NavigateTo::Next,
                "increment" => NavigateTo::Increment,
                "decrement" => NavigateTo::Decrement,
                other => return Err(args.error(format!("can't navigate to {other:?}"))),
            };
            Command::Navigate {
                to,
                tab: args.flag(&["-t", "--tab"]).is_some(),
            }
        }
        "search" => {
            let reverse = args.flag(&["-r", "--reverse"]).is_some();
            Command::Search {
                text: args.rest().to_string(),
                reverse,
                incremental: false,
            }
        }
        "search-next" => Command::SearchNext { prev: false },
        "search-prev" => Command::SearchNext { prev: true },
        "selection-toggle" => Command::SelectionToggle {
            line: args.flag(&["-l", "--line"]).is_some(),
        },
        "selection-reverse" => Command::SelectionReverse,
        name if name.starts_with("move-to-")
            && let Some((_, m)) = CaretMove::ALL.iter().find(|(n, _)| *n == name) =>
        {
            Command::CaretMove(*m)
        }
        "macro-record" | "macro-run" => {
            let register = match args.optional() {
                None => None,
                Some(r) => {
                    let mut chars = r.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => Some(c),
                        _ => {
                            return Err(
                                args.error(format!("a register is one character, not {r:?}"))
                            );
                        }
                    }
                }
            };
            if name == "macro-record" {
                Command::MacroRecord { register }
            } else {
                Command::MacroRun { register }
            }
        }
        "set-mark" | "jump-mark" => {
            let key = args.required("key")?;
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return Err(args.error(format!("a mark is one character, not {key:?}")));
            };
            Command::Mark {
                set: name == "set-mark",
                key: c,
            }
        }
        "spell-suggest" => Command::SpellSuggest,
        "spell-replace" => Command::SpellReplace {
            word: args.required("word")?.to_string(),
        },
        "spell-add" => Command::SpellAdd,
        "history-import" => Command::HistoryImport {
            path: args.optional().map(String::from),
        },
        "history-clear" => Command::HistoryClear {
            force: args.flag(&["-f", "--force"]).is_some(),
        },
        "completion-item-focus" => Command::CompletionFocus(match args.required("direction")? {
            "next" => FocusDirection::Next,
            "prev" => FocusDirection::Prev,
            other => return Err(args.error(format!("expected next or prev, got {other:?}"))),
        }),
        "completion-item-del" => Command::CompletionItemDel,
        "prompt-fileselect-external" => Command::PromptFileselectExternal,
        "hint-follow" => Command::HintFollow {
            label: args.optional().map(String::from),
        },
        "completion-item-yank" => Command::CompletionItemYank {
            sel: args.flag(&["-s", "--sel"]).is_some(),
        },
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
                group: "all".into(),
                target: HintTarget::Normal,
                rapid: false,
                fill: None
            })
        );
        assert_eq!(
            parse("hint --rapid links tab-bg").unwrap(),
            Command::Hint(HintRequest {
                group: "links".into(),
                target: HintTarget::TabBg,
                rapid: true,
                fill: None
            })
        );
        assert_eq!(
            parse("hint links fill :open -t {hint-url}").unwrap(),
            Command::Hint(HintRequest {
                group: "links".into(),
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
        // Groups come from hints.selectors, so any name parses; :hint checks it when run.
        assert!(
            matches!(parse("hint code"), Ok(Command::Hint(HintRequest { group, .. })) if group == "code")
        );
        assert!(matches!(
            parse("hint links explode"),
            Err(CommandError::BadArgs { .. })
        ));
        assert_eq!(parse("yank title").unwrap(), Command::Yank(YankWhat::Title));
        assert_eq!(parse("yank").unwrap(), Command::Yank(YankWhat::Url));
        assert_eq!(
            parse("yank -s title").unwrap(),
            Command::YankPrimary(YankWhat::Title)
        );
    }

    #[test]
    fn parses_config_commands() {
        assert_eq!(
            parse("set url.start_pages [\"a\", \"b\"]").unwrap(),
            Command::Set {
                name: Some("url.start_pages".into()),
                value: Some("[\"a\", \"b\"]".into()),
                pattern: None,
            }
        );
        assert_eq!(
            parse("set").unwrap(),
            Command::Set {
                name: None,
                value: None,
                pattern: None,
            }
        );
        assert_eq!(
            parse("set -u *.example.com content.geolocation true").unwrap(),
            Command::Set {
                name: Some("content.geolocation".into()),
                value: Some("true".into()),
                pattern: Some("*.example.com".into()),
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
    fn parses_page_and_script_commands() {
        assert_eq!(
            parse("zoom 150%").unwrap(),
            Command::Zoom { percent: Some(150) }
        );
        assert_eq!(parse("zoom").unwrap(), Command::Zoom { percent: None });
        assert!(parse("zoom big").is_err());
        assert_eq!(
            parse("print --pdf ~/page.pdf").unwrap(),
            Command::Print {
                pdf: Some("~/page.pdf".into())
            }
        );
        assert_eq!(
            parse("scroll-px 0 -40").unwrap(),
            Command::ScrollPx { x: 0, y: -40 }
        );
        assert_eq!(
            parse("cmd-later 500 open -t x").unwrap(),
            Command::Later {
                ms: 500,
                command: "open -t x".into()
            }
        );
        assert_eq!(
            parse("fake-key -g <Escape>").unwrap(),
            Command::FakeKey {
                keys: "<Escape>".into(),
                global: true
            }
        );
        assert!(parse("fake-key <Nope>").is_err());
        assert_eq!(
            parse("click-element css a.next").unwrap(),
            Command::ClickElement {
                filter: ElementFilter::Css,
                value: "a.next".into()
            }
        );
        assert!(parse("click-element id").is_err());
        assert_eq!(
            parse("set-cmd-text -s :open").unwrap(),
            parse("cmd-set-text -s :open").unwrap()
        );
    }

    #[test]
    fn chains_commands() {
        assert_eq!(
            parse_line("back ;; reload -f").unwrap(),
            vec![Command::Back, Command::Reload { force: true }]
        );
    }

    #[test]
    fn spawn_flags_and_words() {
        assert_eq!(
            parse("spawn -v -u password-fill --user 'a b'").unwrap(),
            Command::Spawn {
                userscript: true,
                verbose: true,
                output_messages: false,
                output: false,
                hint_url: None,
                detach: false,
                argv: vec!["password-fill".into(), "--user".into(), "a b".into()],
            }
        );
        let Command::Spawn { detach, argv, .. } = parse("spawn -d mpv https://x.org").unwrap()
        else {
            panic!()
        };
        assert!(detach);
        assert_eq!(argv, ["mpv", "https://x.org"]);
        assert!(parse("spawn -u").is_err());
        assert!(parse("spawn 'unclosed").is_err());
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
            "spell-replace",
            "spawn",
            "navigate",
            "jseval",
            "screenshot",
            "cmd-later",
            "scroll-px",
            "config-unset",
            "scroll-to-anchor",
            "message-info",
            "message-warning",
            "message-error",
            "config-cycle",
            "insert-text",
            "fake-key",
            "click-element",
            "tab-take",
            "lua-call",
            "set-mark",
            "jump-mark",
        ];
        for spec in COMMANDS.iter().filter(|s| !needs_args.contains(&s.name)) {
            assert!(parse(spec.name).is_ok(), "{} failed to parse", spec.name);
        }
    }
}
