//! Browser-process state shared by every CEF callback. All of it lives on the
//! CEF UI thread, so it is kept in a thread-local rather than behind a lock.

use std::cell::RefCell;

use cef::*;
use hb_config::{AutoConfig, Paths};
use hb_core::command::{Direction, OpenTarget, YankWhat};
use hb_core::completion::CompletionView;
use hb_core::engine::Level;
use hb_core::tabs::{Position, TabList};
use hb_core::url::fuzzy_url;
use hb_core::{Command, Effect, Engine, Mode};
use serde_json::json;

use crate::{clipboard, hints, storage, tabs};

const SCROLL_JS: &str = include_str!("../js/scroll.js");
const SCROLL_STEP_PX: u32 = 40;
const COMPLETION_ROW_HEIGHT: i32 = 18;
const COMPLETION_MAX_ROWS: usize = 12;
const OVERLAY_MAX_ROWS: usize = 14;

pub struct Tab {
    pub view: BrowserView,
    pub url: String,
    pub title: String,
    /// PNG data URL of the site icon.
    pub favicon: Option<String>,
    pub progress: Option<f64>,
    pub load_error: bool,
    /// Error to draw into Chromium's error document once it commits.
    pub pending_error: Option<(String, String)>,
}

impl Tab {
    pub fn new(view: BrowserView) -> Self {
        Self {
            view,
            url: String::new(),
            title: String::new(),
            favicon: None,
            progress: None,
            load_error: false,
            pending_error: None,
        }
    }

    pub fn browser(&self) -> Option<Browser> {
        self.view.browser()
    }
}

pub struct Shell {
    pub engine: Engine,
    pub paths: Paths,
    /// Where `:set`/`:bind`/`:unbind` are persisted; set once config loads.
    pub autoconfig: Option<AutoConfig>,
    /// Settings the user's config files set, which beat `:set` at startup.
    pub overridden: std::collections::BTreeSet<String>,
    /// Which file set each setting, and the files read, for the help page.
    pub setting_sources: std::collections::BTreeMap<String, String>,
    pub config_files: Vec<std::path::PathBuf>,
    pub window: Option<Window>,
    pub content: Option<Panel>,
    pub tabs: TabList<Tab>,
    /// Closed tabs as (index, url), newest last, for `undo`.
    pub closed: Vec<(usize, String)>,
    pub tabbar: Option<BrowserView>,
    pub statusbar: Option<BrowserView>,
    pub completion: Option<BrowserView>,
    pub overlay: Option<OverlayController>,
    pub tabbar_ready: bool,
    pub statusbar_ready: bool,
    pub completion_ready: bool,
    /// Set by `on_before_popup` for the popup view that CEF creates next.
    pub popup_in_background: bool,
    /// Set once the window starts closing, so tab closes go through CEF.
    pub window_closing: bool,
    /// Set by `:quit --save`; `auto_save.session` has the same effect.
    pub save_session_on_quit: bool,
    pub open_browsers: usize,
    pub suppress_char: bool,
    /// Browser id of the tab currently showing hint labels.
    pub hint_browser: Option<i32>,
    last_status: String,
    last_tabbar: String,
    last_title: String,
    /// What the overlay shows (a prompt or completions), to skip redraws.
    last_overlay: String,
    last_overlay_rows: usize,
    timed_message: u64,
}

impl Shell {
    pub fn new(engine: Engine, paths: Paths) -> Self {
        Self {
            engine,
            paths,
            autoconfig: None,
            overridden: Default::default(),
            setting_sources: Default::default(),
            config_files: Vec::new(),
            window: None,
            content: None,
            tabs: TabList::default(),
            closed: Vec::new(),
            tabbar: None,
            statusbar: None,
            completion: None,
            overlay: None,
            tabbar_ready: false,
            statusbar_ready: false,
            completion_ready: false,
            popup_in_background: false,
            window_closing: false,
            save_session_on_quit: false,
            open_browsers: 0,
            suppress_char: false,
            hint_browser: None,
            last_status: String::new(),
            last_tabbar: String::new(),
            last_title: String::new(),
            last_overlay: String::new(),
            last_overlay_rows: 0,
            timed_message: 0,
        }
    }

    /// Turn `:open` text into a URL using the configured search engines.
    pub fn fuzzy_url(&self, input: &str) -> String {
        let engines = self
            .engine
            .settings()
            .map("url.searchengines")
            .cloned()
            .unwrap_or_default();
        fuzzy_url(input, &engines)
    }

    pub fn default_page(&self) -> String {
        self.engine.settings().str("url.default_page").to_string()
    }

    /// Position for a new tab, from `tabs.new_position.related` or `.unrelated`.
    pub fn new_tab_position(&self, related: bool) -> Position {
        let name = if related {
            "tabs.new_position.related"
        } else {
            "tabs.new_position.unrelated"
        };
        Position::from_setting(self.engine.settings().str(name))
    }

    pub fn current_browser(&self) -> Option<Browser> {
        self.tabs.current()?.browser()
    }

    pub fn tab_index(&self, browser: &Browser) -> Option<usize> {
        let id = browser.identifier();
        self.tabs
            .position(|t| t.browser().is_some_and(|b| b.identifier() == id))
    }
}

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
}

pub fn install(shell: Shell) {
    SHELL.with(|s| *s.borrow_mut() = Some(shell));
}

/// Run `f` against the shell. Only call side-effect-free CEF getters inside `f`;
/// anything that can re-enter our callbacks must run after it returns.
pub fn with<R>(f: impl FnOnce(&mut Shell) -> R) -> Option<R> {
    debug_assert_ne!(
        currently_on(ThreadId::UI),
        0,
        "shell used off the UI thread"
    );
    SHELL.with(|s| match s.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(f),
        Err(_) => {
            tracing::warn!("re-entrant shell access ignored");
            None
        }
    })
}

/// Run `f` against the tab that owns `browser`; the bool says whether it is current.
pub fn with_tab<R>(
    browser: Option<&mut Browser>,
    f: impl FnOnce(&mut Shell, usize, bool) -> R,
) -> Option<R> {
    let browser = browser?;
    with(|s| {
        let index = s.tab_index(browser)?;
        let current = index == s.tabs.current_index();
        Some(f(s, index, current))
    })
    .flatten()
}

pub fn show_message(level: Level, text: impl Into<String>) {
    with(|s| s.engine.show_message(level, text));
}

/// (Re)read the config files into the engine. Returns the errors found.
pub fn load_config() -> Vec<String> {
    let Some(paths) = with(|s| s.paths.clone()) else {
        return Vec::new();
    };
    // Runs Lua, but never calls into CEF, so it is safe outside the borrow.
    let loaded = hb_config::load(&paths);
    let errors = with(|s| {
        s.engine.reset_config();
        let mut errors = loaded.errors;
        for op in &loaded.ops {
            if let Err(e) = s.engine.apply_config(op) {
                errors.push(e);
            }
        }
        s.autoconfig = Some(loaded.autoconfig);
        s.overridden = loaded.overridden;
        s.setting_sources = loaded.sources;
        s.config_files = loaded.files;
        storage::set_history_limit(s.engine.settings().int("completion.web_history.max_items"));
        crate::adblock::sync_settings(s.engine.settings());
        errors
    })
    .unwrap_or_default();
    crate::help::refresh();
    apply_spellcheck();
    errors
}

/// Outside the shell borrow: setting Chromium preferences can call back into us.
fn apply_spellcheck() {
    if let Some(languages) = with(|s| s.engine.settings().list("spellcheck.languages").to_vec()) {
        crate::spell::apply(languages);
    }
}

fn persist(op: hb_core::config::ConfigOp) {
    if let hb_core::config::ConfigOp::Set { name, .. } = &op {
        with(|s| {
            s.setting_sources
                .insert(name.clone(), ":set (autoconfig.toml)".to_string())
        });
    }
    crate::help::refresh();
    with(|s| {
        storage::set_history_limit(s.engine.settings().int("completion.web_history.max_items"));
        crate::adblock::sync_settings(s.engine.settings());
    });
    apply_spellcheck();
    let result = with(|s| {
        let auto = s.autoconfig.as_mut()?;
        auto.record(&op);
        let overridden = match &op {
            hb_core::config::ConfigOp::Set { name, .. } => {
                s.overridden.contains(name).then(|| name.clone())
            }
            _ => None,
        };
        Some((auto.save(), overridden))
    })
    .flatten();
    match result {
        Some((Err(e), _)) => show_message(Level::Error, format!("Could not save autoconfig: {e}")),
        Some((Ok(()), Some(name))) => show_message(
            Level::Warning,
            format!("Saved, but your config file also sets {name} and wins at startup"),
        ),
        _ => {}
    }
}

pub fn apply(effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Run { command, count } => run_command(command, count),
            Effect::ModeChanged { from, to } => {
                tracing::debug!(%from, %to, "mode changed");
                if from == Mode::Hint {
                    hints::clear();
                }
                let prompting = |m: Mode| matches!(m, Mode::Prompt | Mode::YesNo);
                if prompting(to) != prompting(from) {
                    focus_for_prompt(prompting(to));
                }
            }
            Effect::ShowHints { labels } => hints::show(&labels),
            Effect::FilterHints { typed } => hints::filter(&typed),
            Effect::FollowHint { index, url, target } => hints::follow(index, url, target),
            Effect::ConfigChanged(op) => persist(op),
            Effect::PromptAnswered { id, answer } => crate::prompts::answered(id, answer),
        }
    }
    refresh_ui();
}

fn run_command(command: Command, count: Option<u32>) {
    if tabs::run_command(&command, count)
        || crate::help::run_command(&command)
        || storage::run_command(&command)
        || crate::downloads::run_command(&command, count)
        || crate::adblock::run_command(&command)
        || crate::spell::run_command(&command)
    {
        return;
    }
    match command {
        Command::Hint(request) => return hints::request(request),
        Command::Yank(what) => return yank(what),
        Command::ConfigSource => {
            let errors = load_config();
            if errors.is_empty() {
                let dir = with(|s| s.paths.config_dir.display().to_string()).unwrap_or_default();
                show_message(Level::Info, format!("Config reloaded from {dir}"));
            } else {
                crate::report_config_errors(&errors);
            }
            return;
        }
        _ => {}
    }
    let Some(browser) = with(|s| s.current_browser()).flatten() else {
        return;
    };
    let n = count.unwrap_or(1).max(1);
    match command {
        Command::Open {
            target,
            related,
            url,
        } => open(target, related, url),
        Command::Back if n == 1 => browser.go_back(),
        Command::Back => run_js(&browser, &format!("history.go(-{n})")),
        Command::Forward if n == 1 => browser.go_forward(),
        Command::Forward => run_js(&browser, &format!("history.go({n})")),
        Command::Reload { force: true } => browser.reload_ignore_cache(),
        Command::Reload { force: false } => browser.reload(),
        Command::Stop => browser.stop_load(),
        Command::Scroll(dir) => {
            let step = f64::from(SCROLL_STEP_PX * n);
            let (x, y) = match dir {
                Direction::Up => (0.0, -step),
                Direction::Down => (0.0, step),
                Direction::Left => (-step, 0.0),
                Direction::Right => (step, 0.0),
                Direction::Top => return scroll(&browser, "perc", "null", "0"),
                Direction::Bottom => return scroll(&browser, "perc", "null", "100"),
            };
            scroll(&browser, "by", &x.to_string(), &y.to_string());
        }
        Command::ScrollPage { x, y } => {
            let n = f64::from(n);
            scroll(&browser, "page", &(x * n).to_string(), &(y * n).to_string());
        }
        Command::ScrollToPerc { perc, horizontal } => {
            let perc = count
                .map(f64::from)
                .or(perc)
                .unwrap_or(100.0)
                .clamp(0.0, 100.0);
            let (x, y) = if horizontal {
                (perc.to_string(), "null".to_string())
            } else {
                ("null".to_string(), perc.to_string())
            };
            scroll(&browser, "perc", &x, &y);
        }
        Command::Quit { save } => {
            if let Some(window) = with(|s| {
                s.save_session_on_quit |= save;
                s.window.clone()
            })
            .flatten()
            {
                window.close();
            }
        }
        other => tracing::warn!(
            ?other,
            "command reached the browser layer but is handled by the engine"
        ),
    }
}

/// `:open` and everything that behaves like it (quickmarks, bookmarks).
pub fn open(target: OpenTarget, related: bool, url: Option<String>) {
    let Some((url, position, browser)) = with(|s| {
        let url = url.map_or_else(|| s.default_page(), |u| s.fuzzy_url(&u));
        (url, s.new_tab_position(related), s.current_browser())
    }) else {
        return;
    };
    // `tabs.pinned.frozen`: a pinned tab keeps its page; :open goes to a new tab.
    let frozen = with(|s| {
        s.tabs.is_pinned(s.tabs.current_index()) && s.engine.settings().bool("tabs.pinned.frozen")
    })
    .unwrap_or(false);
    let target = if target == OpenTarget::Current && frozen {
        OpenTarget::Tab
    } else {
        target
    };
    match target {
        OpenTarget::Current => {
            if let Some(frame) = browser.and_then(|b| b.main_frame()) {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        OpenTarget::Tab => tabs::open(&url, position, true),
        OpenTarget::Background => tabs::open(&url, position, false),
        OpenTarget::Window | OpenTarget::Private => {
            show_message(
                Level::Warning,
                "Separate and private windows are not implemented yet; opened a tab",
            );
            tabs::open(&url, position, true);
        }
    }
}

/// The open tabs, for `:session-save` and saving on quit.
pub fn current_session() -> hb_storage::Session {
    with(|s| hb_storage::Session {
        windows: vec![hb_storage::WindowState {
            active: s.tabs.current_index(),
            tabs: s
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.url.is_empty())
                .map(|(i, t)| hb_storage::TabState {
                    url: t.url.clone(),
                    title: t.title.clone(),
                    pinned: s.tabs.is_pinned(i),
                })
                .collect(),
        }],
    })
    .unwrap_or_default()
}

/// Chromium ignores input to a page while it shows a JavaScript dialog, so
/// keys for prompts are taken from the status bar's browser instead.
fn focus_for_prompt(prompting: bool) {
    let Some((statusbar, tab)) = with(|s| {
        (
            s.statusbar.clone(),
            s.tabs.current().map(|t| t.view.clone()),
        )
    }) else {
        return;
    };
    if let Some(bar) = &statusbar {
        let view = View::from(bar);
        view.set_focusable(prompting.into());
        if prompting {
            view.request_focus();
        }
    }
    if !prompting && let Some(tab) = &tab {
        View::from(tab).request_focus();
    }
}

fn yank(what: YankWhat) {
    let Some((url, title)) =
        with(|s| s.tabs.current().map(|t| (t.url.clone(), t.title.clone()))).flatten()
    else {
        return;
    };
    match what {
        YankWhat::Url => clipboard::yank(&url, "URL"),
        YankWhat::Title => clipboard::yank(&title, "title"),
        YankWhat::Domain => match domain_of(&url) {
            Some(domain) => clipboard::yank(&domain, "domain"),
            None => show_message(Level::Error, "This page has no domain"),
        },
    }
}

/// `scheme://host[:port]`, like qutebrowser's `:yank domain`.
fn domain_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

fn scroll(browser: &Browser, op: &str, x: &str, y: &str) {
    run_js(browser, &format!("({SCROLL_JS})({op:?}, {x}, {y});"));
}

fn run_js(browser: &Browser, code: &str) {
    if let Some(frame) = browser.main_frame() {
        exec_js(&frame, code);
    }
}

pub fn exec_js(frame: &Frame, code: &str) {
    let url = frame.url();
    frame.execute_java_script(
        Some(&CefString::from(code)),
        Some(&CefString::from(&url)),
        0,
    );
}

struct UiUpdate {
    scripts: Vec<(Frame, String)>,
    overlay: Option<(OverlayController, Option<Rect>)>,
    /// Message generation and timeout in milliseconds.
    expire_message: Option<(u64, i64)>,
    title: Option<(Window, String)>,
}

/// Push engine and tab state to the tab bar, status bar and completion overlay.
/// Unchanged state is skipped, so this is cheap to call after every event.
pub fn refresh_ui() {
    let Some(update) = with(collect_ui_update) else {
        return;
    };
    for (frame, json) in update.scripts {
        exec_js(&frame, &format!("hbRender({json})"));
    }
    if let Some((window, title)) = update.title {
        window.set_title(Some(&CefString::from(title.as_str())));
    }
    if let Some((generation, timeout)) = update.expire_message {
        let mut task = ExpireMessage::new(generation);
        post_delayed_task(ThreadId::UI, Some(&mut task), timeout);
    }
    if let Some((overlay, bounds)) = update.overlay {
        match bounds {
            Some(bounds) => {
                overlay.set_bounds(Some(&bounds));
                overlay.set_visible(1);
            }
            None => overlay.set_visible(0),
        }
    }
}

/// Re-anchor the completion overlay after the window is resized.
pub fn position_overlay() {
    let Some(Some((overlay, rows))) = with(|s| {
        let overlay = s.overlay.clone()?;
        Some((overlay, s.last_overlay_rows))
    }) else {
        return;
    };
    if rows > 0
        && let Some(bounds) = with(|s| completion_bounds(s, rows)).flatten()
    {
        overlay.set_bounds(Some(&bounds));
    }
}

fn collect_ui_update(s: &mut Shell) -> UiUpdate {
    let mut scripts = Vec::new();
    let status = s.engine.status();
    let generation = s.engine.message_generation();
    let timeout = s.engine.settings().int("messages.timeout");
    // Start the timer only once the message is on screen, so errors raised
    // while the window is still loading (e.g. from config) are seen.
    let shown = status.message.is_some() && s.statusbar_ready;
    let expire_message = (shown && generation != s.timed_message).then(|| {
        s.timed_message = generation;
        (generation, timeout)
    });
    // A timeout of 0 keeps messages until the next one replaces them.
    let expire_message = expire_message.filter(|_| timeout > 0);

    let current = s.tabs.current();
    let status_json = json!({
        "mode": status.mode,
        "command_line": status.command_line,
        "keystring": status.keystring,
        "message": status.message,
        "url": current.map_or("", |t| t.url.as_str()),
        "progress": current.and_then(|t| t.progress),
        "load_error": current.is_some_and(|t| t.load_error),
        "downloads": crate::downloads::summary(),
        "tab_index": s.tabs.current_index() + 1,
        "tab_count": s.tabs.len(),
    })
    .to_string();
    if s.statusbar_ready
        && status_json != s.last_status
        && let Some(frame) = frame_of(&s.statusbar)
    {
        s.last_status = status_json.clone();
        scripts.push((frame, status_json));
    }

    let settings = s.engine.settings();
    let favicons = settings.str("tabs.favicons.show");
    let tabs: Vec<_> = s
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let pinned = s.tabs.is_pinned(i);
            let show_icon = favicons == "always" || (favicons == "pinned" && pinned);
            json!({
                "title": t.title,
                "url": t.url,
                "loading": t.progress.is_some(),
                "error": t.load_error,
                "pinned": pinned,
                "favicon": if show_icon { t.favicon.as_deref() } else { None },
            })
        })
        .collect();
    let tabbar_json = json!({
        "tabs": tabs,
        "current": s.tabs.current_index(),
        "shrink": settings.bool("tabs.pinned.shrink"),
    })
    .to_string();
    if s.tabbar_ready
        && tabbar_json != s.last_tabbar
        && let Some(frame) = frame_of(&s.tabbar)
    {
        s.last_tabbar = tabbar_json.clone();
        scripts.push((frame, tabbar_json));
    }
    // Recomputed every time: `{mode}` changes without the tab bar changing.
    let mut title = None;
    if let (Some(window), Some(tab)) = (s.window.clone(), s.tabs.current()) {
        let text = hb_core::title::format(
            s.engine.settings().str("window.title_format"),
            &tab.title,
            &tab.url,
            s.engine.mode().name(),
        );
        if text != s.last_title {
            s.last_title = text.clone();
            title = Some((window, text));
        }
    }

    // A prompt takes the overlay; otherwise it shows command completions.
    let (payload, rows) = match s.engine.prompt_view() {
        Some(prompt) => {
            let rows = prompt_rows(s, &prompt);
            (json!({ "kind": "prompt", "prompt": prompt }), rows)
        }
        None => {
            let rows = completion_rows(&s.engine.completions());
            let count = rows.len();
            (json!({ "kind": "rows", "rows": rows }), count)
        }
    };
    let payload = payload.to_string();
    let overlay_changed = payload != s.last_overlay;
    if s.completion_ready
        && overlay_changed
        && let Some(frame) = frame_of(&s.completion)
    {
        scripts.push((frame, payload.clone()));
    }
    let overlay = overlay_changed
        .then(|| {
            let overlay = s.overlay.clone()?;
            let bounds = (rows > 0).then(|| completion_bounds(s, rows)).flatten();
            Some((overlay, bounds))
        })
        .flatten();
    if s.completion_ready || rows == 0 {
        s.last_overlay = payload;
        s.last_overlay_rows = rows;
    }

    UiUpdate {
        scripts,
        overlay,
        expire_message,
        title,
    }
}

wrap_task! {
    struct ExpireMessage {
        generation: u64,
    }

    impl Task {
        fn execute(&self) {
            with(|s| s.engine.expire_message(self.generation));
            refresh_ui();
        }
    }
}

/// The rows to draw: category headers and items, scrolled so the selected
/// item is visible, at most `COMPLETION_MAX_ROWS` in total.
fn completion_rows(view: &CompletionView) -> Vec<serde_json::Value> {
    let items = &view.items;
    let mut start = view
        .selected
        .map_or(0, |i| i.saturating_sub(COMPLETION_MAX_ROWS / 2));
    loop {
        let mut rows = Vec::new();
        let mut category = "";
        for (i, item) in items.iter().enumerate().skip(start) {
            let header = item.category != category || rows.is_empty();
            if rows.len() + usize::from(header) + 1 > COMPLETION_MAX_ROWS {
                break;
            }
            if header {
                category = item.category;
                rows.push(json!({ "header": item.category }));
            }
            rows.push(json!({
                "name": item.name,
                "description": item.description,
                "selected": view.selected == Some(i),
            }));
        }
        // Many headers can push the selection out of view; start at it then.
        let shown = rows.iter().any(|r| r["selected"] == true);
        match view.selected {
            Some(i) if !shown && start != i => start = i,
            _ => return rows,
        }
    }
}

/// Overlay rows for a prompt: title, wrapped message, input or hint line.
fn prompt_rows(s: &Shell, prompt: &hb_core::prompt::PromptView) -> usize {
    const CHAR_WIDTH: i32 = 8;
    const MAX_MESSAGE_ROWS: usize = 8;
    let width = s
        .statusbar
        .as_ref()
        .map_or(800, |v| View::from(v).bounds().width);
    let per_line = (width / CHAR_WIDTH).max(20) as usize;
    let message_rows: usize = prompt
        .message
        .lines()
        .map(|line| line.chars().count().div_ceil(per_line).max(1))
        .sum();
    let input_row = usize::from(prompt.kind == "text");
    1 + message_rows.clamp(1, MAX_MESSAGE_ROWS) + input_row + 1
}

fn completion_bounds(s: &Shell, rows: usize) -> Option<Rect> {
    let bar = View::from(s.statusbar.as_ref()?).bounds();
    let height = rows.min(OVERLAY_MAX_ROWS) as i32 * COMPLETION_ROW_HEIGHT;
    Some(Rect {
        x: bar.x,
        y: bar.y - height,
        width: bar.width,
        height,
    })
}

fn frame_of(view: &Option<BrowserView>) -> Option<Frame> {
    view.as_ref()?.browser()?.main_frame()
}
