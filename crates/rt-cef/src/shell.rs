//! Browser-process state shared by every CEF callback. All of it lives on the
//! CEF UI thread, so it is kept in a thread-local rather than behind a lock.

use std::cell::RefCell;

use cef::*;
use rt_config::{AutoConfig, Paths};
use rt_core::command::{Direction, OpenTarget, YankWhat};
use rt_core::completion::CompletionView;
use rt_core::engine::Level;
use rt_core::tabs::{Position, TabList};
use rt_core::url::fuzzy_url;
use rt_core::{Command, Effect, Engine, Mode};
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
    /// The mode this tab was in when the user last left it.
    pub mode: rt_core::Mode,
    /// Page zoom in percent.
    pub zoom: u32,
    pub muted: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// The last search's match number and count.
    pub search_match: Option<(i32, i32)>,
    /// How far down the page is scrolled, in percent; -1 when it all fits.
    pub scroll: Option<i32>,
    /// `session.lazy_restore`: the URL to load when the tab is first shown.
    /// Until then the tab shows `about:blank` but keeps `url` and `title`.
    pub pending: Option<String>,
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
            mode: rt_core::Mode::Normal,
            zoom: 100,
            muted: false,
            can_go_back: false,
            can_go_forward: false,
            search_match: None,
            scroll: None,
            pending: None,
        }
    }

    pub fn browser(&self) -> Option<Browser> {
        self.view.browser()
    }
}

/// One browser window: its views, tabs and what was last drawn in it.
pub struct WindowState {
    /// Stable across other windows opening and closing.
    pub id: u32,
    /// Private windows keep nothing: an in-memory profile, no history, no session.
    pub private: bool,
    pub window: Option<Window>,
    pub content: Option<Panel>,
    pub tabs: TabList<Tab>,
    /// Closed tabs as (index, url), newest last, for `undo`.
    pub closed: Vec<(usize, String)>,
    pub tabbar: Option<BrowserView>,
    /// The page area's row, which also holds a left or right tab bar.
    pub row: Option<Panel>,
    /// Where the bars were last put.
    pub bar_placement: crate::window::BarPlacement,
    pub tabbar_shown: bool,
    pub statusbar_shown: bool,
    /// For `tabs.show = switching`: the tab last shown, and when the bar hides again.
    last_tab_index: Option<usize>,
    switching_until: Option<std::time::Instant>,
    pub statusbar: Option<BrowserView>,
    pub completion: Option<BrowserView>,
    pub overlay: Option<OverlayController>,
    pub tabbar_ready: bool,
    pub statusbar_ready: bool,
    pub completion_ready: bool,
    /// Set once the window starts closing, so tab closes go through CEF.
    pub window_closing: bool,
    last_status: String,
    last_tabbar: String,
    last_title: String,
    /// What the overlay shows (a prompt or completions), to skip redraws.
    last_overlay: String,
    last_overlay_rows: usize,
}

impl WindowState {
    pub fn new(id: u32, private: bool) -> Self {
        Self {
            id,
            private,
            window: None,
            content: None,
            tabs: TabList::default(),
            closed: Vec::new(),
            tabbar: None,
            row: None,
            bar_placement: Default::default(),
            tabbar_shown: true,
            statusbar_shown: true,
            last_tab_index: None,
            switching_until: None,
            statusbar: None,
            completion: None,
            overlay: None,
            tabbar_ready: false,
            statusbar_ready: false,
            completion_ready: false,
            window_closing: false,
            last_status: String::new(),
            last_tabbar: String::new(),
            last_title: String::new(),
            last_overlay: String::new(),
            last_overlay_rows: 0,
        }
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
    /// Never empty. `Shell` derefs to the active one, which gets the keys.
    pub windows: Vec<WindowState>,
    pub active: usize,
    next_window_id: u32,
    /// The in-memory profile private windows share, created on first use.
    pub private_context: Option<RequestContext>,
    /// Set by `on_before_popup` for the popup view that CEF creates next.
    pub popup_in_background: bool,
    /// Set by `:quit --save`; `auto_save.session` has the same effect.
    pub save_session_on_quit: bool,
    /// `:quit` is closing every window; it saved the session itself.
    pub quitting: bool,
    pub open_browsers: usize,
    pub suppress_char: bool,
    /// Browser id of the tab currently showing hint labels.
    pub hint_browser: Option<i32>,
    /// The user said yes to `confirm_quit`; quitting goes ahead without asking again.
    pub quit_confirmed: bool,
    /// When a hint was last followed, for `hints.auto_follow_timeout`.
    pub hint_followed_at: Option<std::time::Instant>,
    timed_message: u64,
    /// The pending key chain and when it last changed, for `keyhint.delay`.
    keyhint_chain: String,
    keyhint_since: std::time::Instant,
}

impl std::ops::Deref for Shell {
    type Target = WindowState;

    fn deref(&self) -> &WindowState {
        &self.windows[self.active]
    }
}

impl std::ops::DerefMut for Shell {
    fn deref_mut(&mut self) -> &mut WindowState {
        &mut self.windows[self.active]
    }
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
            windows: vec![WindowState::new(0, false)],
            active: 0,
            next_window_id: 1,
            private_context: None,
            popup_in_background: false,
            save_session_on_quit: false,
            quitting: false,
            open_browsers: 0,
            suppress_char: false,
            hint_browser: None,
            timed_message: 0,
            hint_followed_at: None,
            quit_confirmed: false,
            keyhint_chain: String::new(),
            keyhint_since: std::time::Instant::now(),
        }
    }

    /// A slot for a window about to be created. The first window reuses the
    /// slot that exists from the start.
    pub fn new_window(&mut self, private: bool) -> u32 {
        if self.windows.len() == 1
            && self.windows[0].window.is_none()
            && !self.windows[0].window_closing
        {
            self.windows[0].private = private;
            return self.windows[0].id;
        }
        let id = self.next_window_id;
        self.next_window_id += 1;
        self.windows.push(WindowState::new(id, private));
        id
    }

    pub fn window_index(&self, id: u32) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    /// The window and tab index showing `browser`.
    pub fn find_browser(&self, browser: &Browser) -> Option<(usize, usize)> {
        let id = browser.identifier();
        self.windows.iter().enumerate().find_map(|(w, state)| {
            state
                .tabs
                .position(|t| t.browser().is_some_and(|b| b.identifier() == id))
                .map(|t| (w, t))
        })
    }

    /// The window a UI or tab browser belongs to, by browser id.
    pub fn window_of_browser(&self, id: i32) -> Option<usize> {
        let is = |view: &Option<BrowserView>| {
            view.as_ref()
                .and_then(|v| v.browser())
                .is_some_and(|b| b.identifier() == id)
        };
        self.windows.iter().position(|w| {
            is(&w.tabbar)
                || is(&w.statusbar)
                || is(&w.completion)
                || w.tabs
                    .iter()
                    .any(|t| t.browser().is_some_and(|b| b.identifier() == id))
        })
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

/// Run `f` against the tab that owns `browser`, in whichever window it is;
/// inside `f` that window is the shell's. The bool says whether the tab is
/// the current one of the active window.
pub fn with_tab<R>(
    browser: Option<&mut Browser>,
    f: impl FnOnce(&mut Shell, usize, bool) -> R,
) -> Option<R> {
    let browser = browser?;
    with(|s| {
        let (window, index) = s.find_browser(browser)?;
        let active = s.active;
        let current = window == active && index == s.windows[window].tabs.current_index();
        s.active = window;
        let result = f(s, index, current);
        s.active = active.min(s.windows.len() - 1);
        Some(result)
    })
    .flatten()
}

/// Make the window that shows browser `id` (a tab or one of its bars) the
/// active one, e.g. because a key arrived there.
pub fn activate_browser(id: i32) {
    let changed = with(|s| {
        let window = s.window_of_browser(id)?;
        (window != s.active).then(|| s.active = window)
    })
    .flatten();
    if changed.is_some() {
        refresh_ui();
    }
}

/// Make window `id` the active one.
pub fn activate_window(id: u32) {
    let changed = with(|s| {
        let window = s.window_index(id)?;
        (window != s.active).then(|| s.active = window)
    })
    .flatten();
    if changed.is_some() {
        refresh_ui();
    }
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
    apply_config(rt_config::load(&paths))
}

/// Put config that was already read into the engine.
pub fn apply_config(loaded: rt_config::Loaded) -> Vec<String> {
    let user_commands = rt_config::lua::user_commands();
    let errors = with(|s| {
        s.engine.reset_config();
        s.engine.set_user_commands(user_commands);
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
    apply_chromium_settings();
    errors
}

/// Outside the shell borrow: setting Chromium preferences can call back into us.
fn apply_chromium_settings() {
    let Some((languages, scheme, sites)) = with(|s| {
        let settings = s.engine.settings();
        (
            settings.list("spellcheck.languages").to_vec(),
            settings
                .str("colors.webpage.preferred_color_scheme")
                .to_string(),
            crate::permissions::sync_site_settings(settings),
        )
    }) else {
        return;
    };
    crate::spell::apply(languages);
    // Cloned so no CEF call happens while the shell is borrowed.
    if let Some(settings) = with(|s| s.engine.settings().clone()) {
        crate::content::apply_globals(&settings);
    }
    crate::permissions::apply_site_settings(sites);
    let variant = match scheme.as_str() {
        "light" => ColorVariant::LIGHT,
        "dark" => ColorVariant::DARK,
        _ => ColorVariant::SYSTEM,
    };
    if let Some(context) = request_context_get_global_context()
        && context.chrome_color_scheme_mode() != variant
    {
        context.set_chrome_color_scheme(variant, 0);
    }
}

fn persist(op: rt_core::config::ConfigOp) {
    match &op {
        rt_core::config::ConfigOp::Set { name, .. } => {
            with(|s| {
                s.setting_sources
                    .insert(name.clone(), ":set (autoconfig.toml)".to_string())
            });
        }
        rt_core::config::ConfigOp::Unset { name } => {
            with(|s| s.setting_sources.remove(name));
        }
        _ => {}
    }
    crate::help::refresh();
    with(|s| {
        storage::set_history_limit(s.engine.settings().int("completion.web_history.max_items"));
        crate::adblock::sync_settings(s.engine.settings());
    });
    apply_chromium_settings();
    let result = with(|s| {
        let auto = s.autoconfig.as_mut()?;
        auto.record(&op);
        let overridden = match &op {
            rt_core::config::ConfigOp::Set { name, .. } => {
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
                crate::lua::emit("mode_changed", &[("from", from.name()), ("to", to.name())]);
                if from == Mode::Hint {
                    hints::clear();
                }
                if (from == Mode::Caret) != (to == Mode::Caret) {
                    crate::caret::mode_changed(to == Mode::Caret);
                }
                let prompting = |m: Mode| matches!(m, Mode::Prompt | Mode::YesNo);
                if prompting(to) != prompting(from) {
                    focus_for_prompt(prompting(to));
                }
            }
            Effect::ShowHints { labels } => hints::show(&labels),
            Effect::FilterHints { typed } => hints::filter(&typed),
            Effect::FollowHint { index, url, target } => {
                with(|s| s.hint_followed_at = Some(std::time::Instant::now()));
                hints::follow(index, url, target)
            }
            Effect::ConfigChanged(op) => persist(op),
            Effect::PromptAnswered { id, answer } => crate::prompts::answered(id, answer),
            Effect::PassKey(key) => crate::client::send_to_page(key),
            Effect::DeleteCompletion(item) => storage::delete_completion(&item),
            Effect::YankText { text, primary } => clipboard::yank_to(&text, "text", primary),
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
        || crate::spawn::run_command(&command, count)
        || crate::marks::run_command(&command)
        || crate::caret::run_command(&command, count)
        || crate::greasemonkey::run_command(&command)
        || crate::search::run_command(&command, count)
        || crate::navigate::run_command(&command, count)
        || crate::lua::run_command(&command, count)
        || crate::view::run_command(&command, count)
        || crate::actions::run_command(&command)
        || crate::screenshot::run_command(&command)
        || crate::fileselect::run_command(&command)
        || crate::configcmd::run_command(&command)
    {
        return;
    }
    match command {
        Command::Hint(request) => return hints::request(request),
        Command::Yank(what) => return yank(what, false),
        Command::YankPrimary(what) => return yank(what, true),
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
        Command::ScrollPx { x, y } => {
            let n = i64::from(n);
            scroll(&browser, "by", &(x * n).to_string(), &(y * n).to_string());
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
        Command::Close => {
            if let Some(window) = with(|s| s.window.clone()).flatten() {
                window.close();
            }
        }
        Command::Quit { save } => {
            if !confirm_quit(move || run_command(Command::Quit { save }, None)) {
                return;
            }
            let save = with(|s| {
                s.quitting = true;
                save || s.save_session_on_quit || s.engine.settings().bool("auto_save.session")
            })
            .unwrap_or(false);
            if save && let Err(e) = storage::save_session(crate::storage::DEFAULT_SESSION) {
                tracing::warn!("could not save session: {e}");
            }
            let windows: Vec<Window> =
                with(|s| s.windows.iter().filter_map(|w| w.window.clone()).collect())
                    .unwrap_or_default();
            for window in windows {
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
        OpenTarget::Window => crate::window::create(vec![url], Vec::new(), false),
        OpenTarget::Private => crate::window::create(vec![url], Vec::new(), true),
    }
}

/// The open tabs of every window except private ones, for `:session-save`
/// and saving on quit. The active window comes first.
pub fn current_session() -> rt_storage::Session {
    with(|s| {
        let mut order: Vec<usize> = (0..s.windows.len()).collect();
        order.sort_by_key(|&i| i != s.active);
        rt_storage::Session {
            windows: order
                .into_iter()
                .map(|i| &s.windows[i])
                .filter(|w| !w.private && w.window.is_some())
                .map(|w| rt_storage::WindowState {
                    active: w.tabs.current_index(),
                    tabs: w
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| !t.url.is_empty())
                        .map(|(i, t)| rt_storage::TabState {
                            url: t.url.clone(),
                            title: t.title.clone(),
                            pinned: w.tabs.is_pinned(i),
                        })
                        .collect(),
                })
                .filter(|w| !w.tabs.is_empty())
                .collect(),
        }
    })
    .unwrap_or_default()
}

/// Chromium ignores input to a page while it shows a JavaScript dialog, so
/// keys for prompts are taken from the status bar's browser instead.
fn focus_for_prompt(prompting: bool) {
    // statusbar.show may hide the bar outside prompts; it has to be shown
    // before it can take focus.
    if prompting {
        refresh_ui();
    }
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

/// `confirm_quit`: true if quitting may go ahead now. Otherwise asks, and
/// calls `quit` if the answer is yes.
pub fn confirm_quit(quit: impl FnOnce() + 'static) -> bool {
    let reason = with(|s| {
        if s.quit_confirmed {
            return None;
        }
        let tabs = s.windows.iter().map(|w| w.tabs.len()).sum();
        let values = s.engine.settings().list("confirm_quit").to_vec();
        rt_core::settings::confirm_quit_reason(&values, tabs, crate::downloads::running_count())
    })
    .flatten();
    let Some(reason) = reason else {
        return true;
    };
    crate::prompts::ask(
        None,
        crate::prompts::Scope::Other,
        "Quit riptide?",
        reason,
        rt_core::prompt::PromptKind::YesNo {
            default: false,
            remember: rt_core::prompt::Remember::Never,
        },
        move |answer| {
            if matches!(answer, rt_core::prompt::PromptAnswer::Yes { .. }) {
                with(|s| s.quit_confirmed = true);
                quit();
            }
        },
    );
    false
}

fn yank(what: YankWhat, primary: bool) {
    let Some((url, title)) =
        with(|s| s.tabs.current().map(|t| (t.url.clone(), t.title.clone()))).flatten()
    else {
        return;
    };
    match what {
        YankWhat::Url => {
            let ignored = with(|s| {
                s.engine
                    .settings()
                    .list("url.yank_ignored_parameters")
                    .to_vec()
            })
            .unwrap_or_default();
            clipboard::yank_to(&rt_core::url::strip_params(&url, &ignored), "URL", primary)
        }
        YankWhat::Title => clipboard::yank_to(&title, "title", primary),
        YankWhat::Domain => match domain_of(&url) {
            Some(domain) => clipboard::yank_to(&domain, "domain", primary),
            None => show_message(Level::Error, "This page has no domain"),
        },
        YankWhat::Selection => crate::caret::yank(primary),
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
    let smooth = with(|s| s.engine.settings().bool("scrolling.smooth")).unwrap_or(false);
    run_js(
        browser,
        &format!("({SCROLL_JS})({op:?}, {x}, {y}, {smooth});"),
    );
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
    bars: Option<BarsChange>,
    /// Refresh again after this many milliseconds (`tabs.show = switching`).
    refresh_after: Option<i64>,
}

struct BarsChange {
    window: Window,
    row: Panel,
    tabbar: BrowserView,
    statusbar: BrowserView,
    /// A new placement, if it changed.
    placement: Option<crate::window::BarPlacement>,
    tabbar_visible: Option<bool>,
    statusbar_visible: Option<bool>,
}

/// Push engine and tab state to the tab bar, status bar and completion overlay.
/// Unchanged state is skipped, so this is cheap to call after every event.
pub fn refresh_ui() {
    let Some(updates) = with(|s| {
        crate::tabs::remember_open_tabs(s);
        let focused = s.active;
        let mut updates = Vec::new();
        for i in 0..s.windows.len() {
            s.active = i;
            updates.push(collect_ui_update(s, i == focused));
        }
        s.active = focused;
        updates
    }) else {
        return;
    };
    for update in updates {
        apply_ui_update(update);
    }
}

fn apply_ui_update(update: UiUpdate) {
    for (frame, json) in update.scripts {
        exec_js(&frame, &format!("rtRender({json})"));
    }
    if let Some((window, title)) = update.title {
        window.set_title(Some(&CefString::from(title.as_str())));
    }
    if let Some((generation, timeout)) = update.expire_message {
        let mut task = ExpireMessage::new(generation);
        post_delayed_task(ThreadId::UI, Some(&mut task), timeout);
    }
    if let Some(change) = update.bars {
        if let Some(placement) = &change.placement {
            crate::window::arrange_bars(
                &change.window,
                &change.row,
                &change.tabbar,
                &change.statusbar,
                placement,
            );
        }
        if let Some(visible) = change.tabbar_visible {
            View::from(&change.tabbar).set_visible(visible.into());
        }
        if let Some(visible) = change.statusbar_visible {
            View::from(&change.statusbar).set_visible(visible.into());
        }
        // The overlay is anchored to the status bar.
        position_overlay();
    }
    if let Some(delay) = update.refresh_after {
        let mut task = RefreshUi::new();
        post_delayed_task(ThreadId::UI, Some(&mut task), delay);
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

/// What to redraw in the active window. Only the `focused` one (the window
/// keys go to) shows the mode, command line, messages and overlay.
fn collect_ui_update(s: &mut Shell, focused: bool) -> UiUpdate {
    let mut scripts = Vec::new();
    let mut status = s.engine.status();
    if !focused {
        status.mode = Mode::Normal;
        status.command_line = None;
        status.message = None;
        status.keystring.clear();
    }
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
        "private": s.private,
        "zoom": current.map_or(100, |t| t.zoom),
        "muted": current.is_some_and(|t| t.muted),
        "widgets": s.engine.settings().list("statusbar.widgets"),
        "back": current.is_some_and(|t| t.can_go_back),
        "forward": current.is_some_and(|t| t.can_go_forward),
        "search_match": current.and_then(|t| t.search_match),
        "scroll": current.and_then(|t| t.scroll),
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
    let shrink = settings.bool("tabs.pinned.shrink")
        && !matches!(settings.str("tabs.position"), "left" | "right");
    let (format, format_pinned) = (
        settings.str("tabs.title.format"),
        settings.str("tabs.title.format_pinned"),
    );
    let tabs: Vec<_> = s
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let pinned = s.tabs.is_pinned(i);
            let show_icon = favicons == "always" || (favicons == "pinned" && pinned);
            let template = if pinned && shrink {
                format_pinned
            } else {
                format
            };
            let label = rt_core::title::tab_label(
                template,
                &rt_core::title::TabFields {
                    index: i + 1,
                    count: s.tabs.len(),
                    title: &t.title,
                    url: &t.url,
                    progress: t.progress,
                    muted: t.muted,
                    private: s.private,
                },
            );
            json!({
                "label": label,
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
        "tooltips": settings.bool("tabs.tooltips"),
        "vertical": matches!(settings.str("tabs.position"), "left" | "right"),
    })
    .to_string();
    if s.tabbar_ready
        && tabbar_json != s.last_tabbar
        && let Some(frame) = frame_of(&s.tabbar)
    {
        s.last_tabbar = tabbar_json.clone();
        scripts.push((frame, tabbar_json));
    }
    let (bars, refresh_after) = bars_change(s, status.mode, status.message.is_some());
    // Recomputed every time: `{mode}` changes without the tab bar changing.
    let mut title = None;
    if let (Some(window), Some(tab)) = (s.window.clone(), s.tabs.current()) {
        let text = rt_core::title::format(
            s.engine.settings().str("window.title_format"),
            &tab.title,
            &tab.url,
            status.mode.name(),
        );
        if text != s.last_title {
            s.last_title = text.clone();
            title = Some((window, text));
        }
    }

    let mut keyhint_wait = None;
    // A prompt takes the overlay; otherwise it shows command completions.
    let prompt = if focused {
        s.engine.prompt_view()
    } else {
        None
    };
    let (payload, rows) = match prompt {
        Some(prompt) => {
            let rows = prompt_rows(s, &prompt);
            (json!({ "kind": "prompt", "prompt": prompt }), rows)
        }
        None if focused => 'rows: {
            let mut rows = completion_rows(&s.engine.completions());
            if rows.is_empty() {
                let (hints, wait) = keyhints(s);
                keyhint_wait = wait;
                if let Some(hints) = hints {
                    break 'rows hints;
                }
            }
            // Without completions, show the messages the status bar replaced.
            if rows.is_empty() && status.command_line.is_none() {
                rows = s
                    .engine
                    .earlier_messages()
                    .into_iter()
                    .map(|m| json!({ "message": m.text, "level": m.level }))
                    .collect();
            }
            let count = rows.len();
            (json!({ "kind": "rows", "rows": rows }), count)
        }
        None => (json!({ "kind": "rows", "rows": [] }), 0),
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
    // Come back when the key hint delay runs out, or the tab bar's switching delay.
    let refresh_after = match (refresh_after, keyhint_wait) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };

    UiUpdate {
        scripts,
        overlay,
        expire_message,
        title,
        bars,
        refresh_after,
    }
}

/// Where the bars should be and whether they should show, compared with
/// what was last applied. Also says when to check again after a tab switch.
fn bars_change(s: &mut Shell, mode: Mode, message: bool) -> (Option<BarsChange>, Option<i64>) {
    let settings = s.engine.settings();
    let placement = crate::window::BarPlacement::from_settings(settings);
    let show = settings.str("tabs.show").to_string();
    let delay = settings.int("tabs.show_switching_delay");
    let statusbar_visible =
        rt_core::mode::statusbar_visible(settings.str("statusbar.show"), mode, message);
    let now = std::time::Instant::now();
    let mut refresh_after = None;
    let index = s.tabs.current_index();
    if s.last_tab_index != Some(index) {
        if s.last_tab_index.is_some() && show == "switching" {
            s.switching_until = Some(now + std::time::Duration::from_millis(delay as u64));
            refresh_after = Some(delay + 10);
        }
        s.last_tab_index = Some(index);
    }
    let switching = s.switching_until.is_some_and(|until| now < until);
    let tabbar_visible = rt_core::tabs::bar_visible(&show, s.tabs.len(), switching);
    let placement = (placement != s.bar_placement).then(|| {
        s.bar_placement = placement.clone();
        placement
    });
    let tabbar_visible = (tabbar_visible != s.tabbar_shown).then(|| {
        s.tabbar_shown = tabbar_visible;
        tabbar_visible
    });
    let statusbar_visible = (statusbar_visible != s.statusbar_shown).then(|| {
        s.statusbar_shown = statusbar_visible;
        statusbar_visible
    });
    let changed = placement.is_some() || tabbar_visible.is_some() || statusbar_visible.is_some();
    let change = match (
        s.window.clone(),
        s.row.clone(),
        s.tabbar.clone(),
        s.statusbar.clone(),
    ) {
        (Some(window), Some(row), Some(tabbar), Some(statusbar)) if changed => Some(BarsChange {
            window,
            row,
            tabbar,
            statusbar,
            placement,
            tabbar_visible,
            statusbar_visible,
        }),
        _ => None,
    };
    (change, refresh_after)
}

wrap_task! {
    struct RefreshUi {}

    impl Task {
        fn execute(&self) {
            refresh_ui();
        }
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

/// Width of one key hint column, in characters.
const KEYHINT_COLUMN_CHARS: i32 = 40;
/// Approximate width of a character in the overlay's monospace font.
const OVERLAY_CHAR_WIDTH: i32 = 8;

/// The key hint popup once the pending chain has been unchanged for
/// `keyhint.delay`, with the rows it needs; otherwise how long until it's due.
fn keyhints(s: &mut Shell) -> (Option<(serde_json::Value, usize)>, Option<i64>) {
    let hints = s.engine.keyhints();
    let chain = hints
        .as_ref()
        .map(|(prefix, _)| prefix.clone())
        .unwrap_or_default();
    if chain != s.keyhint_chain {
        s.keyhint_chain = chain;
        s.keyhint_since = std::time::Instant::now();
    }
    let Some((prefix, items)) = hints else {
        return (None, None);
    };
    let delay = s.engine.settings().int("keyhint.delay");
    let waited = s.keyhint_since.elapsed().as_millis() as i64;
    if waited < delay {
        return (None, Some(delay - waited + 10));
    }
    // As many columns as fit, filled top to bottom; a header row above them.
    let width = s
        .statusbar
        .as_ref()
        .map_or(800, |v| View::from(v).bounds().width);
    let columns = (width / (KEYHINT_COLUMN_CHARS * OVERLAY_CHAR_WIDTH)).max(1) as usize;
    let rows = items.len().div_ceil(columns).min(OVERLAY_MAX_ROWS - 1);
    let items: Vec<_> = items
        .into_iter()
        .take(rows * columns)
        .map(|(keys, command)| json!({ "keys": keys, "command": command }))
        .collect();
    let payload = json!({ "kind": "keyhints", "prefix": prefix, "items": items, "rows": rows });
    (Some((payload, rows + 1)), None)
}

/// Overlay rows for a prompt: title, wrapped message, input or hint line.
fn prompt_rows(s: &Shell, prompt: &rt_core::prompt::PromptView) -> usize {
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

/// Next to the status bar, on the page's side of it; at the page area's
/// edge when the status bar is hidden.
fn completion_bounds(s: &Shell, rows: usize) -> Option<Rect> {
    let height = rows.min(OVERLAY_MAX_ROWS) as i32 * COMPLETION_ROW_HEIGHT;
    let top = s.bar_placement.statusbar == "top";
    let (x, width, edge) = if s.statusbar_shown {
        let bar = View::from(s.statusbar.as_ref()?).bounds();
        (
            bar.x,
            bar.width,
            if top { bar.y + bar.height } else { bar.y },
        )
    } else {
        let row = View::from(s.row.as_ref()?).bounds();
        (
            row.x,
            row.width,
            if top { row.y } else { row.y + row.height },
        )
    };
    let y = if top { edge } else { edge - height };
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

fn frame_of(view: &Option<BrowserView>) -> Option<Frame> {
    view.as_ref()?.browser()?.main_frame()
}

/// Show a message from another thread.
pub fn post_message(level: Level, text: String) {
    let mut task = ShowMessage::new(level, text);
    post_task(ThreadId::UI, Some(&mut task));
}

wrap_task! {
    struct ShowMessage {
        level: Level,
        text: String,
    }

    impl Task {
        fn execute(&self) {
            show_message(self.level, self.text.clone());
            refresh_ui();
        }
    }
}
