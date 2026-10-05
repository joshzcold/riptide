//! The website's generated reference pages, built from the same data as
//! `riptide://help/` so the two never disagree.

use std::collections::HashMap;
use std::fmt::Write;

use rt_core::help::{self, HelpData};
use rt_core::keymap::Keymap;
use rt_core::settings::Settings;

const GENERATED: &str = "<!-- Generated from the command and binding registries; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->";

/// The pages, as `(path under docs/book/src, contents)`.
pub fn pages() -> Vec<(&'static str, String)> {
    let data = defaults();
    vec![
        ("reference/commands.md", commands_markdown(&data)),
        ("reference/bindings.md", bindings_markdown(&data)),
    ]
}

fn defaults() -> HelpData {
    help::build(
        &Keymap::defaults(),
        &Settings::default(),
        &HashMap::new(),
        Vec::new(),
        &[],
    )
}

/// A Markdown code span that survives backticks and table pipes in `text`.
fn code(text: &str) -> String {
    let text = text.replace('|', "\\|");
    if text.contains('`') {
        format!("`` {text} ``")
    } else {
        format!("`{text}`")
    }
}

/// Plain table text, where `<text>` placeholders must not read as HTML.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn commands_markdown(data: &HelpData) -> String {
    let mut out = format!(
        "# Commands\n\n{GENERATED}\n\n\
         Type these after `:`, or bind them to keys. `:help` shows the same list in the browser, \
         with your own bindings.\n\n\
         | Command | Default keys | Description |\n|---|---|---|\n"
    );
    for c in &data.commands {
        let keys: Vec<String> = c.keys.iter().map(|k| code(k)).collect();
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            code(&format!(":{}", c.name)),
            keys.join(" "),
            cell(&c.description)
        );
    }
    out
}

fn bindings_markdown(data: &HelpData) -> String {
    let mut out = format!(
        "# Default key bindings\n\n{GENERATED}\n\n\
         Change these with `:bind`, `[bindings.<mode>]` in `config.toml` or `rt.bind()` in `config.lua`. \
         `:help bindings` shows your current bindings, with your changes marked.\n"
    );
    for mode in data.modes.iter().filter(|m| !m.bindings.is_empty()) {
        let _ = write!(
            out,
            "\n## {} mode\n\n| Keys | Command |\n|---|---|\n",
            mode.name
        );
        for b in &mode.bindings {
            let _ = writeln!(out, "| {} | {} |", code(&b.keys), code(&b.command));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerate with `UPDATE_LUA_TYPES=1 cargo test -p rt-config`.
    #[test]
    fn checked_in_reference_is_current() {
        let book = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src");
        let update = std::env::var_os("UPDATE_LUA_TYPES").is_some();
        for (name, generated) in pages() {
            let path = book.join(name);
            if update {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, &generated).unwrap();
            }
            // Git may check files out with CRLF line endings on Windows.
            let current = std::fs::read_to_string(&path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            assert!(
                current == generated,
                "docs/book/src/{name} is stale; run UPDATE_LUA_TYPES=1 cargo test -p rt-config"
            );
        }
    }

    #[test]
    fn every_command_is_listed() {
        let commands = commands_markdown(&defaults());
        for c in rt_core::command::COMMANDS.iter().filter(|c| !c.hidden) {
            assert!(commands.contains(&format!("`:{}`", c.name)), "{}", c.name);
        }
    }

    #[test]
    fn code_spans_survive_backticks_and_pipes() {
        assert_eq!(code("`a"), "`` `a ``");
        assert_eq!(code("a|b"), "`a\\|b`");
        assert_eq!(cell(":open <url>"), ":open &lt;url&gt;");
    }
}
