//! Spell checking with Chromium's built-in checker, driven from the keyboard.
//!
//! Chromium only hands out suggestions with a context menu, so
//! `:spell-suggest` right-clicks the word at the text cursor, takes the
//! suggestions from `OnBeforeContextMenu`, hides the menu, and offers them as
//! completions for `:spell-replace`.

use std::cell::RefCell;

use cef::*;
use rt_core::Command;
use rt_core::completion::Completion;
use rt_core::engine::Level;

use crate::{eval, shell};

const SPELL_JS: &str = include_str!("../js/spell.js");

#[derive(Default)]
struct State {
    /// The browser a `:spell-suggest` right-click went to.
    pending: Option<i32>,
    word: String,
    suggestions: Vec<String>,
    /// The languages last written to Chromium's preferences.
    applied: Option<Vec<String>>,
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

fn suggest() {
    let enabled = shell::with(|s| !s.engine.settings().list("spellcheck.languages").is_empty());
    if enabled != Some(true) {
        shell::show_message(
            Level::Error,
            "Spell checking is off; set spellcheck.languages, e.g. [\"en-US\"]",
        );
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
        let (Some(point), Some(host)) = (point, target.host()) else {
            shell::show_message(Level::Error, "No word at the text cursor");
            return shell::refresh_ui();
        };
        tracing::debug!(
            x = point.x,
            y = point.y,
            "right-clicking the word at the cursor"
        );
        STATE.with(|s| s.borrow_mut().pending = Some(target.identifier()));
        let zoom = 1.2_f64.powf(host.zoom_level());
        let event = MouseEvent {
            x: (point.x * zoom).round() as i32,
            y: (point.y * zoom).round() as i32,
            modifiers: 0,
        };
        host.send_mouse_click_event(Some(&event), MouseButtonType::RIGHT, 0, 1);
        host.send_mouse_click_event(Some(&event), MouseButtonType::RIGHT, 1, 1);
    });
}

#[derive(serde::Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

/// Called for every context menu in a tab. Returns true when it was ours.
pub fn context_menu(browser: &Browser, params: &ContextMenuParams, model: &MenuModel) -> bool {
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
            if let (Some(browser), Some(params), Some(model)) = (browser, params, model) {
                context_menu(browser, params, model);
            }
        }
    }
}
