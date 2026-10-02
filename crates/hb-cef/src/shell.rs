//! Browser-process state shared by every CEF callback. All of it lives on the
//! CEF UI thread, so it is kept in a thread-local rather than behind a lock.

use std::cell::RefCell;

use cef::*;
use hb_core::command::{Direction, OpenTarget};
use hb_core::engine::{Completion, Level};
use hb_core::url::{DEFAULT_SEARCH_ENGINE, DEFAULT_START_PAGE, fuzzy_url};
use hb_core::{Command, Effect, Engine};
use serde_json::json;

const SCROLL_JS: &str = include_str!("../js/scroll.js");
const SCROLL_STEP_PX: u32 = 40;
const COMPLETION_ROW_HEIGHT: i32 = 18;
const COMPLETION_MAX_ROWS: usize = 12;
const MESSAGE_TIMEOUT_MS: i64 = 3000;

pub struct Shell {
    pub engine: Engine,
    pub window: Option<Window>,
    pub tab: Option<BrowserView>,
    pub statusbar: Option<BrowserView>,
    pub completion: Option<BrowserView>,
    pub overlay: Option<OverlayController>,
    pub statusbar_ready: bool,
    pub completion_ready: bool,
    pub url: String,
    pub progress: Option<f64>,
    pub load_error: bool,
    /// Error to draw into Chromium's error document once it commits.
    pub pending_error: Option<(String, String)>,
    pub open_browsers: usize,
    pub suppress_char: bool,
    last_status: String,
    last_completion: Vec<Completion>,
    timed_message: u64,
}

impl Shell {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            window: None,
            tab: None,
            statusbar: None,
            completion: None,
            overlay: None,
            statusbar_ready: false,
            completion_ready: false,
            url: String::new(),
            progress: None,
            load_error: false,
            pending_error: None,
            open_browsers: 0,
            suppress_char: false,
            last_status: String::new(),
            last_completion: Vec::new(),
            timed_message: 0,
        }
    }

    pub fn tab_browser(&self) -> Option<Browser> {
        self.tab.as_ref()?.browser()
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

pub fn apply(effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Run { command, count } => run_command(command, count),
            Effect::ModeChanged { from, to } => tracing::debug!(%from, %to, "mode changed"),
        }
    }
    refresh_ui();
}

fn run_command(command: Command, count: Option<u32>) {
    let Some(browser) = with(|s| s.tab_browser()).flatten() else {
        return;
    };
    let n = count.unwrap_or(1).max(1);
    match command {
        Command::Open { target, url } => {
            if target != OpenTarget::Current {
                with(|s| {
                    s.engine.show_message(
                        Level::Warning,
                        "Tabs and windows are not implemented yet; opening here",
                    )
                });
            }
            let url = url.map_or_else(
                || DEFAULT_START_PAGE.to_string(),
                |u| fuzzy_url(&u, DEFAULT_SEARCH_ENGINE),
            );
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
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
        Command::Quit => {
            if let Some(window) = with(|s| s.window.clone()).flatten() {
                window.close();
            }
        }
        other => tracing::warn!(
            ?other,
            "command reached the browser layer but is handled by the engine"
        ),
    }
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
    statusbar: Option<Frame>,
    status_json: Option<String>,
    completion: Option<Frame>,
    completion_json: Option<String>,
    overlay: Option<(OverlayController, Option<Rect>)>,
    expire_message: Option<u64>,
}

/// Push engine and page state to the status bar and completion overlay.
/// Unchanged state is skipped, so this is cheap to call after every event.
pub fn refresh_ui() {
    let Some(update) = with(collect_ui_update) else {
        return;
    };
    if let (Some(frame), Some(json)) = (update.statusbar, update.status_json) {
        exec_js(&frame, &format!("hbRender({json})"));
    }
    if let (Some(frame), Some(json)) = (update.completion, update.completion_json) {
        exec_js(&frame, &format!("hbRender({json})"));
    }
    if let Some(generation) = update.expire_message {
        let mut task = ExpireMessage::new(generation);
        post_delayed_task(ThreadId::UI, Some(&mut task), MESSAGE_TIMEOUT_MS);
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
        Some((overlay, s.last_completion.len()))
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
    let status = s.engine.status();
    let generation = s.engine.message_generation();
    let expire_message = (status.message.is_some() && generation != s.timed_message).then(|| {
        s.timed_message = generation;
        generation
    });
    let status_json = json!({
        "mode": status.mode,
        "command_line": status.command_line,
        "keystring": status.keystring,
        "message": status.message,
        "url": s.url,
        "progress": s.progress,
        "load_error": s.load_error,
    })
    .to_string();
    let status_changed = s.statusbar_ready && status_json != s.last_status;
    if status_changed {
        s.last_status = status_json.clone();
    }

    let completions = s.engine.completions();
    let completion_changed = completions != s.last_completion;
    let completion_json = (s.completion_ready && completion_changed)
        .then(|| serde_json::to_string(&completions).unwrap_or_default());
    let overlay = completion_changed
        .then(|| {
            let overlay = s.overlay.clone()?;
            let bounds = (!completions.is_empty())
                .then(|| completion_bounds(s, completions.len()))
                .flatten();
            Some((overlay, bounds))
        })
        .flatten();
    if s.completion_ready || completions.is_empty() {
        s.last_completion = completions;
    }

    UiUpdate {
        statusbar: status_changed.then(|| frame_of(&s.statusbar)).flatten(),
        status_json: status_changed.then_some(status_json),
        completion: completion_json
            .is_some()
            .then(|| frame_of(&s.completion))
            .flatten(),
        completion_json,
        overlay,
        expire_message,
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

fn completion_bounds(s: &Shell, rows: usize) -> Option<Rect> {
    let bar = View::from(s.statusbar.as_ref()?).bounds();
    let height = (rows.min(COMPLETION_MAX_ROWS) as i32 + 1) * COMPLETION_ROW_HEIGHT;
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
