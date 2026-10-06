//! Command line completion: what to offer for the text typed so far, and
//! what the line becomes when an item is chosen with Tab.

use serde::Serialize;

use crate::command::COMMANDS;
use crate::settings::{Kind, SETTINGS, Settings, Value};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub category: &'static str,
    /// Inserted into the command line when chosen.
    pub name: String,
    pub description: String,
    /// When a history entry was last visited, in seconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<i64>,
    /// Shown at the right, e.g. a setting's current value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Dynamic sources the browser layer provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    /// Quickmarks, bookmarks and history, for `:open`.
    Url,
    Quickmark,
    Bookmark,
    Session,
    /// Suggestions from the last `:spell-suggest`.
    Spelling,
    /// Open tabs in every window, for `:tab-select`.
    Tab,
    /// Tabs in the other windows, for `:tab-take`.
    OtherTab,
}

pub type Source = Box<dyn Fn(CompletionKind, &str) -> Vec<Completion>>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CompletionView {
    pub items: Vec<Completion>,
    pub selected: Option<usize>,
}

/// The parts of a command line that completion cares about.
struct Parsed<'a> {
    command: &'a str,
    /// The command and its flags, e.g. `open -t`.
    prefix: String,
    pattern: &'a str,
}

fn parse(text: &str) -> Option<Parsed<'_>> {
    let typed = text.strip_prefix(':')?;
    let (command, mut rest) = typed.split_once(char::is_whitespace)?;
    let mut prefix = command.to_string();
    loop {
        let trimmed = rest.trim_start();
        match trimmed.split_once(char::is_whitespace) {
            Some((flag, after)) if flag.starts_with('-') => {
                prefix.push(' ');
                prefix.push_str(flag);
                rest = after;
            }
            _ => {
                rest = trimmed;
                break;
            }
        }
    }
    Some(Parsed {
        command,
        prefix,
        pattern: rest,
    })
}

/// A value as `:set` takes it: plain text for strings, JSON otherwise.
fn set_text(value: &Value) -> String {
    match value {
        Value::Str(text) => text.clone(),
        other => other.to_json().to_string(),
    }
}

/// `:set` completions: setting names with their current values, then the
/// values a setting can take.
fn complete_set(parsed: &Parsed<'_>, settings: &Settings) -> Vec<Completion> {
    // `:set -u pattern …`: the pattern isn't part of what completes.
    let mut words = parsed.pattern.split_whitespace();
    if parsed.prefix.contains("-u") || parsed.prefix.contains("--pattern") {
        words.next();
    }
    let words: Vec<&str> = words.collect();
    let typing_new_word =
        parsed.pattern.ends_with(char::is_whitespace) || parsed.pattern.is_empty();
    let (name, partial) = match (words.as_slice(), typing_new_word) {
        ([], _) => (None, ""),
        ([name], false) => (None, *name),
        ([name], true) => (Some(*name), ""),
        ([name, value], false) => (Some(*name), *value),
        _ => return Vec::new(),
    };
    let current = |def: &crate::settings::SettingDef| {
        settings.get(def.name).map(set_text).unwrap_or_default()
    };
    let Some(name) = name else {
        return SETTINGS
            .iter()
            .filter(|d| d.name.starts_with(partial))
            .map(|d| Completion {
                category: "Settings",
                name: d.name.to_string(),
                description: d.description.to_string(),
                time: None,
                detail: Some(current(d)),
            })
            .collect();
    };
    let Some(def) = crate::settings::find(name) else {
        return Vec::new();
    };
    let now = current(def);
    let default = set_text(&def.default_value());
    let candidates: Vec<String> = match def.kind {
        Kind::Bool => vec!["true".into(), "false".into()],
        Kind::Enum(options) => options.iter().map(|o| o.to_string()).collect(),
        _ if now == default => vec![now.clone()],
        _ => vec![now.clone(), default.clone()],
    };
    candidates
        .into_iter()
        .filter(|v| v.starts_with(partial))
        .map(|v| {
            let note = match (v == now, v == default) {
                (true, true) => "current, default",
                (true, false) => "current",
                (false, true) => "default",
                _ => "",
            };
            Completion {
                category: "Values",
                description: note.to_string(),
                name: v,
                time: None,
                detail: None,
            }
        })
        .collect()
}

/// Items for the current command line text.
pub fn compute(text: &str, source: Option<&Source>, settings: &Settings) -> Vec<Completion> {
    let Some(typed) = text.strip_prefix(':') else {
        return Vec::new();
    };
    if !typed.contains(char::is_whitespace) {
        return COMMANDS
            .iter()
            .filter(|c| !c.hidden && c.name.starts_with(typed))
            .map(|c| Completion {
                time: None,
                detail: None,
                category: "Commands",
                name: c.name.to_string(),
                description: c.description.to_string(),
            })
            .collect();
    }
    let Some(parsed) = parse(text) else {
        return Vec::new();
    };
    let kind = match parsed.command {
        "set" => return complete_set(&parsed, settings),
        "open" => CompletionKind::Url,
        "quickmark-load" | "quickmark-del" => CompletionKind::Quickmark,
        "bookmark-load" | "bookmark-del" => CompletionKind::Bookmark,
        "session-load" | "session-delete" | "session-save" => CompletionKind::Session,
        "spell-replace" => CompletionKind::Spelling,
        "tab-select" => CompletionKind::Tab,
        "tab-take" => CompletionKind::OtherTab,
        _ => return Vec::new(),
    };
    source.map(|s| s(kind, parsed.pattern)).unwrap_or_default()
}

/// The command line after choosing `item` from completions of `base`.
pub fn insert(base: &str, item: &Completion) -> String {
    match parse(base) {
        Some(parsed) if item.category != "Commands" => {
            let space = if item.category == "Settings" { " " } else { "" };
            // Keep earlier words: `:set -u site name` or `:set name value`.
            let head = if matches!(item.category, "Settings" | "Values") {
                parsed
                    .pattern
                    .rsplit_once(char::is_whitespace)
                    .map_or("", |(head, _)| head.trim_end())
            } else {
                ""
            };
            let head = if head.is_empty() {
                String::new()
            } else {
                format!("{head} ")
            };
            format!(":{} {head}{}{space}", parsed.prefix, item.name)
        }
        _ => format!(":{} ", item.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(category: &'static str, name: &str) -> Completion {
        Completion {
            time: None,
            detail: None,
            category,
            name: name.into(),
            description: String::new(),
        }
    }

    fn source() -> Source {
        Box::new(|kind, pattern| vec![item("History", &format!("{kind:?}:{pattern}"))])
    }

    #[test]
    fn picks_the_source_from_the_command() {
        let s = source();
        assert_eq!(
            compute(":open -t rust docs", Some(&s), &Settings::default())[0].name,
            "Url:rust docs"
        );
        assert_eq!(
            compute(":quickmark-load -b gh", Some(&s), &Settings::default())[0].name,
            "Quickmark:gh"
        );
        assert_eq!(
            compute(":session-load ", Some(&s), &Settings::default())[0].name,
            "Session:"
        );
        assert_eq!(
            compute(":spell-replace th", Some(&s), &Settings::default())[0].name,
            "Spelling:th"
        );
        assert!(compute(":reload x", Some(&s), &Settings::default()).is_empty());
        assert!(compute(":open x", None, &Settings::default()).is_empty());
        assert_eq!(
            compute(":tab-c", None, &Settings::default())[0].name,
            "tab-close"
        );
        assert_eq!(
            compute(":set hints.c", None, &Settings::default())[0].name,
            "hints.chars"
        );
    }

    #[test]
    fn insert_keeps_command_and_flags() {
        assert_eq!(
            insert(":open -t rus", &item("History", "https://rust-lang.org/")),
            ":open -t https://rust-lang.org/"
        );
        assert_eq!(
            insert(":set hints", &item("Settings", "hints.chars")),
            ":set hints.chars "
        );
        assert_eq!(
            insert(":tab", &item("Commands", "tab-close")),
            ":tab-close "
        );
        assert_eq!(
            insert(":quickmark-load  gi", &item("Quickmarks", "github")),
            ":quickmark-load github"
        );
    }
}
