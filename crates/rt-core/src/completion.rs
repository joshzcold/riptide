//! Command line completion: what to offer for the text typed so far, and
//! what the line becomes when an item is chosen with Tab.

use serde::Serialize;

use crate::command::COMMANDS;
use crate::settings::SETTINGS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub category: &'static str,
    /// Inserted into the command line when chosen.
    pub name: String,
    pub description: String,
    /// When a history entry was last visited, in seconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<i64>,
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

/// Items for the current command line text.
pub fn compute(text: &str, source: Option<&Source>) -> Vec<Completion> {
    let Some(typed) = text.strip_prefix(':') else {
        return Vec::new();
    };
    if !typed.contains(char::is_whitespace) {
        return COMMANDS
            .iter()
            .filter(|c| !c.hidden && c.name.starts_with(typed))
            .map(|c| Completion {
                time: None,
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
        "set" if !parsed.pattern.contains(char::is_whitespace) => {
            return SETTINGS
                .iter()
                .filter(|d| d.name.starts_with(parsed.pattern))
                .map(|d| Completion {
                    time: None,
                    category: "Settings",
                    name: d.name.to_string(),
                    description: d.description.to_string(),
                })
                .collect();
        }
        "open" => CompletionKind::Url,
        "quickmark-load" | "quickmark-del" => CompletionKind::Quickmark,
        "bookmark-load" | "bookmark-del" => CompletionKind::Bookmark,
        "session-load" | "session-delete" | "session-save" => CompletionKind::Session,
        "spell-replace" => CompletionKind::Spelling,
        "tab-select" | "tab-take" => CompletionKind::Tab,
        _ => return Vec::new(),
    };
    source.map(|s| s(kind, parsed.pattern)).unwrap_or_default()
}

/// The command line after choosing `item` from completions of `base`.
pub fn insert(base: &str, item: &Completion) -> String {
    match parse(base) {
        Some(parsed) if item.category != "Commands" => {
            let space = if item.category == "Settings" { " " } else { "" };
            format!(":{} {}{space}", parsed.prefix, item.name)
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
            compute(":open -t rust docs", Some(&s))[0].name,
            "Url:rust docs"
        );
        assert_eq!(
            compute(":quickmark-load -b gh", Some(&s))[0].name,
            "Quickmark:gh"
        );
        assert_eq!(compute(":session-load ", Some(&s))[0].name, "Session:");
        assert_eq!(
            compute(":spell-replace th", Some(&s))[0].name,
            "Spelling:th"
        );
        assert!(compute(":reload x", Some(&s)).is_empty());
        assert!(compute(":open x", None).is_empty());
        assert_eq!(compute(":tab-c", None)[0].name, "tab-close");
        assert_eq!(compute(":set hints.c", None)[0].name, "hints.chars");
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
