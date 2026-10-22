//! Spell checking with Chromium's built-in checker, driven from the keyboard.
//!
//! Chromium only hands out suggestions with a context menu, so
//! `:spell-suggest` right-clicks the word at the text cursor, takes the
//! suggestions from `OnBeforeContextMenu`, hides the menu, and offers them as
//! completions for `:spell-replace`. `:spell-next` and `:spell-prev` find a
//! misspelled word the same way, right-clicking one word after another.

use std::cell::RefCell;

use cef::*;
use rt_core::Command;
use rt_core::completion::Completion;
use rt_core::engine::Level;

use crate::{eval, shell};

const SPELL_JS: &str = include_str!("../js/spell.js");
const STEP_JS: &str = include_str!("../js/spell_step.js");
/// The most words `:spell-next` looks at before giving up.
const MAX_STEPS: u32 = 500;

#[derive(Default)]
struct State {
    /// The browser a `:spell-suggest` right-click went to.
    pending: Option<i32>,
    /// A `:spell-next` or `:spell-prev` under way.
    search: Option<Search>,
    word: String,
    suggestions: Vec<String>,
    /// The languages last written to Chromium's preferences.
    applied: Option<Vec<String>>,
}

/// `:spell-next`'s progress: the word its right-click went to, and where the
/// cursor was, to put it back if no word is misspelled.
struct Search {
    browser: i32,
    back: bool,
    at: i64,
    origin: i64,
    steps: u32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Turn the checker on for `spellcheck.languages`, or off when it's empty.
pub fn apply(languages: Vec<String>) {
    if STATE.with(|s| s.borrow().applied.as_ref() == Some(&languages)) {
        return;
    }
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    let (Some(mut enabled), Some(mut dictionaries), Some(mut list)) =
        (value_create(), value_create(), list_value_create())
    else {
        return;
    };
    enabled.set_bool((!languages.is_empty()).into());
    for (i, language) in languages.iter().enumerate() {
        list.set_string(i, Some(&CefString::from(language.as_str())));
    }
    dictionaries.set_list(Some(&mut list));
    for (name, value) in [
        ("spellcheck.dictionaries", &mut dictionaries),
        ("browser.enable_spellchecking", &mut enabled),
    ] {
        // CEF rejects a null error string, which is what an empty CefString becomes.
        let mut error = CefString::from(" ");
        if context.set_preference(Some(&CefString::from(name)), Some(value), Some(&mut error)) == 0
        {
            tracing::warn!(pref = name, %error, "could not set spell-check preference");
        }
    }
    STATE.with(|s| s.borrow_mut().applied = Some(languages));
}

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::SpellSuggest => suggest(),
        Command::SpellNext { back } => {
            if spelling_on()
                && let Some(browser) = current_browser()
            {
                STATE.with(|s| s.borrow_mut().search = None);
                step(&browser, *back, None, 0);
            }
        }
        Command::SpellReplace { word } => {
            if let Some(host) = current_browser().and_then(|b| b.host()) {
                host.replace_misspelling(Some(&CefString::from(word.as_str())));
                // The text field still has focus; carry on typing.
                let effects = shell::with(|s| s.engine.execute_str("mode-enter insert", None));
                shell::apply(effects.unwrap_or_default());
            }
        }
        Command::SpellAdd => {
            let word = STATE.with(|s| s.borrow().word.clone());
            if word.is_empty() {
                shell::show_message(Level::Error, "Run :spell-suggest on a word first");
            } else if let Some(host) = current_browser().and_then(|b| b.host()) {
                host.add_word_to_dictionary(Some(&CefString::from(word.as_str())));
                shell::show_message(Level::Info, format!("Added “{word}” to your dictionary"));
            }
        }
        Command::SpellInstall { languages } if languages.is_empty() => {
            // The command line, with the languages to pick from listed.
            let effects =
                shell::with(|s| s.engine.execute_str("cmd-set-text -s :spell-install", None));
            shell::apply(effects.unwrap_or_default());
        }
        Command::SpellInstall { languages } => {
            for language in languages {
                install(language);
            }
        }
        _ => return false,
    }
    true
}

/// `:spell-install`: fetch one dictionary, check it, save it where Chromium
/// looks, and add its language to `spellcheck.languages`. Installing first
/// means Chromium never downloads it from Google itself.
fn install(language: &str) {
    let Some(dictionary) = rt_core::dictionaries::find(language) else {
        shell::show_message(
            Level::Error,
            format!("There's no dictionary for {language}; :spell-install <Tab> lists them"),
        );
        return;
    };
    let Some(dir) = shell::with(|s| s.paths.data_dir.join("Dictionaries")) else {
        return;
    };
    shell::show_message(
        Level::Info,
        format!("Downloading the {} dictionary…", dictionary.language),
    );
    crate::fetch::get(&dictionary.url(), move |result| {
        let saved = result.and_then(|body| {
            // gitiles serves files as base64 text.
            let text: String = String::from_utf8_lossy(&body).split_whitespace().collect();
            let decoded = base64_decode(Some(&CefString::from(text.as_str())))
                .ok_or_else(|| "the download isn't base64".to_string())?;
            let mut bytes = vec![0u8; decoded.size()];
            decoded.data(Some(&mut bytes), 0);
            dictionary.verify(&bytes)?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(dictionary.file);
            let partial = path.with_extension("bdic.part");
            std::fs::write(&partial, &bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&partial, &path).map_err(|e| e.to_string())
        });
        match saved {
            Ok(()) => {
                let on = shell::with(|s| {
                    s.engine
                        .settings()
                        .list("spellcheck.languages")
                        .iter()
                        .any(|l| l == dictionary.language)
                })
                .unwrap_or(false);
                let effects = if on {
                    Vec::new()
                } else {
                    let line = format!(
                        "config-list-add spellcheck.languages {}",
                        dictionary.language
                    );
                    shell::with(|s| s.engine.execute_str(&line, None)).unwrap_or_default()
                };
                shell::apply(effects);
                shell::show_message(
                    Level::Info,
                    format!(
                        "Installed the {} dictionary; spell checking uses it now",
                        dictionary.language
                    ),
                );
            }
            Err(e) => shell::show_message(
                Level::Error,
                format!(
                    "Could not install the {} dictionary: {e}",
                    dictionary.language
                ),
            ),
        }
        shell::refresh_ui();
    });
}

fn current_browser() -> Option<Browser> {
    shell::with(|s| s.current_browser()).flatten()
}

/// Whether `spellcheck.languages` has any; says so when it hasn't.
fn spelling_on() -> bool {
    let enabled = shell::with(|s| !s.engine.settings().list("spellcheck.languages").is_empty());
    if enabled != Some(true) {
        shell::show_message(
            Level::Error,
            "Spell checking is off; set spellcheck.languages, e.g. [\"en-US\"]",
        );
    }
    enabled == Some(true)
}

/// Move the text cursor to the next word (`back`: the previous one) from
/// index `from`, or from the cursor, and right-click it; `context_menu` says
/// whether it's misspelled and takes the next step if not.
fn step(browser: &Browser, back: bool, from: Option<i64>, steps: u32) {
    let call = format!(
        "{}({}, {}, null)",
        STEP_JS.trim_end(),
        if back { -1 } else { 1 },
        from.map_or("null".to_string(), |f| f.to_string())
    );
    let target = browser.clone();
    eval::eval(browser, &call, move |result| {
        let found = result
            .ok()
            .and_then(|json| serde_json::from_str::<Option<Step>>(&json).ok())
            .flatten();
        let Some(found) = found else {
            let origin = STATE.with(|s| s.borrow_mut().search.take().map(|s| s.origin));
            give_up(&target, origin, back, steps > 0);
            return;
        };
        let origin = STATE.with(|s| s.borrow().search.as_ref().map(|s| s.origin));
        STATE.with(|s| {
            s.borrow_mut().search = Some(Search {
                browser: target.identifier(),
                back,
                at: found.at,
                origin: origin.unwrap_or(found.origin),
                steps: steps + 1,
            })
        });
        right_click(&target, found.x, found.y);
    });
}

/// No misspelled word that way: put the cursor back where it was.
fn give_up(browser: &Browser, origin: Option<i64>, back: bool, looked: bool) {
    if let Some(origin) = origin {
        let call = format!("{}(1, null, {origin})", STEP_JS.trim_end());
        eval::eval(browser, &call, |_| {});
    }
    let text = match (looked, back) {
        (false, _) => "No text field with words to check",
        (true, false) => "No misspelled word after the cursor",
        (true, true) => "No misspelled word before the cursor",
    };
    shell::show_message(Level::Info, text);
    shell::refresh_ui();
}

#[derive(serde::Deserialize)]
struct Step {
    x: f64,
    y: f64,
    at: i64,
    origin: i64,
}

/// A right-click at `x`, `y` (CSS pixels), whose context menu stays hidden.
fn right_click(browser: &Browser, x: f64, y: f64) {
    let Some(host) = browser.host() else {
        return;
    };
    let zoom = 1.2_f64.powf(host.zoom_level());
    let event = MouseEvent {
        x: (x * zoom).round() as i32,
        y: (y * zoom).round() as i32,
        modifiers: 0,
    };
    host.send_mouse_click_event(Some(&event), MouseButtonType::RIGHT, 0, 1);
    host.send_mouse_click_event(Some(&event), MouseButtonType::RIGHT, 1, 1);
}

fn suggest() {
    if !spelling_on() {
        return;
    }
    let Some(browser) = current_browser() else {
        return;
    };
    let target = browser.clone();
    eval::eval(&browser, SPELL_JS, move |result| {
        let point = result
            .ok()
            .and_then(|json| serde_json::from_str::<Option<Point>>(&json).ok())
            .flatten();
        let Some(point) = point.filter(|_| target.host().is_some()) else {
            shell::show_message(Level::Error, "No word at the text cursor");
            return shell::refresh_ui();
        };
        tracing::debug!(
            x = point.x,
            y = point.y,
            "right-clicking the word at the cursor"
        );
        STATE.with(|s| s.borrow_mut().pending = Some(target.identifier()));
        right_click(&target, point.x, point.y);
    });
}

/// The word `search` right-clicked: stop there if it's misspelled, or look
/// at the next one, after the menu callback has returned.
fn searched(browser: &Browser, params: &ContextMenuParams, search: Search) {
    let word = CefString::from(&params.misspelled_word()).to_string();
    if !word.is_empty() {
        shell::show_message(Level::Info, format!("“{word}”; Alt-s suggests fixes"));
        return shell::refresh_ui();
    }
    let browser = browser.clone();
    if search.steps >= MAX_STEPS {
        give_up(&browser, Some(search.origin), search.back, true);
        return;
    }
    let (back, at, steps, origin) = (search.back, search.at, search.steps, search.origin);
    crate::client::later(0, move || {
        STATE.with(|s| {
            s.borrow_mut().search = Some(Search {
                browser: browser.identifier(),
                back,
                at,
                origin,
                steps,
            })
        });
        step(&browser, back, Some(at), steps);
    });
}

#[derive(serde::Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

/// Called for every context menu in a tab. Returns true when it was ours.
pub fn context_menu(browser: &Browser, params: &ContextMenuParams, model: &MenuModel) -> bool {
    let search = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let ours = s
            .search
            .as_ref()
            .is_some_and(|f| f.browser == browser.identifier());
        if ours { s.search.take() } else { None }
    });
    if let Some(search) = search {
        model.clear();
        searched(browser, params, search);
        return true;
    }
    let ours = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let ours = s.pending == Some(browser.identifier());
        if ours {
            s.pending = None;
        }
        ours
    });
    if !ours {
        return false;
    }
    model.clear();
    let word = CefString::from(&params.misspelled_word()).to_string();
    let mut list = CefStringList::new();
    params.dictionary_suggestions(Some(&mut list));
    let suggestions = crate::favicons::read_list(&mut list);
    tracing::debug!(word, count = suggestions.len(), "spelling suggestions");
    if word.is_empty() {
        shell::show_message(Level::Info, "No spelling mistake at the text cursor");
        shell::refresh_ui();
        return true;
    }
    let none = suggestions.is_empty();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.word = word.clone();
        s.suggestions = suggestions;
    });
    if none {
        shell::show_message(
            Level::Info,
            format!("No suggestions for “{word}”; :spell-add adds it"),
        );
        shell::refresh_ui();
    } else {
        let effects = shell::with(|s| s.engine.execute_str("cmd-set-text -s :spell-replace", None));
        shell::apply(effects.unwrap_or_default());
    }
    true
}

/// Completions for `:spell-replace`.
pub fn completions(pattern: &str) -> Vec<Completion> {
    STATE.with(|s| {
        let s = s.borrow();
        s.suggestions
            .iter()
            .filter(|w| w.starts_with(pattern))
            .map(|w| Completion {
                icon: None,
                time: None,
                detail: None,
                category: "Spelling",
                name: w.clone(),
                description: format!("instead of “{}”", s.word),
            })
            .collect()
    })
}

/// The context menu's "Copy link address", among CEF's ids for embedders.
fn copy_link() -> i32 {
    MenuId::USER_FIRST.get_raw() as i32
}

wrap_context_menu_handler! {
    pub struct RtContextMenuHandler {}

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let (Some(browser), Some(params), Some(model)) = (browser, params, model) else {
                return;
            };
            if context_menu(browser, params, model) {
                return;
            }
            // A link: copy its address, as other browsers offer.
            if !CefString::from(&params.link_url()).to_string().is_empty() {
                if model.count() > 0 {
                    model.add_separator();
                }
                model.add_item(copy_link(), Some(&CefString::from("Copy link address")));
            }
        }

        fn on_context_menu_command(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: ::std::os::raw::c_int,
            _event_flags: EventFlags,
        ) -> ::std::os::raw::c_int {
            if command_id != copy_link() {
                return 0;
            }
            if let Some(params) = params {
                let url = CefString::from(&params.link_url()).to_string();
                crate::clipboard::yank(&url, "link address");
                shell::refresh_ui();
            }
            1
        }
    }
}
