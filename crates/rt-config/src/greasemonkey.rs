//! Greasemonkey userscripts: `*.js` files in `<data>/greasemonkey/` (as in
//! qutebrowser) or `<config>/greasemonkey/`, with a `// ==UserScript==`
//! metadata block saying where and when they run.

use std::path::{Path, PathBuf};

use rt_core::url::{glob, pattern_matches as match_pattern};
use serde::{Deserialize, Serialize};

use crate::Paths;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunAt {
    /// Before the page's own scripts.
    Start,
    /// At `DOMContentLoaded`, the default.
    #[default]
    End,
    /// After the page has loaded.
    Idle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub name: String,
    pub version: String,
    pub description: String,
    pub matches: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub run_at: RunAt,
    /// `@noframes`: only in the top-level page.
    pub no_frames: bool,
    pub code: String,
}

impl Script {
    /// `file_name` names the script when it has no `@name`.
    pub fn parse(file_name: &str, code: &str) -> Script {
        let mut script = Script {
            name: file_name
                .trim_end_matches(".user.js")
                .trim_end_matches(".js")
                .to_string(),
            code: code.to_string(),
            ..Script::default()
        };
        let mut in_block = false;
        for line in code.lines() {
            let line = line.trim();
            if line.starts_with("// ==UserScript==") {
                in_block = true;
                continue;
            }
            if line.starts_with("// ==/UserScript==") {
                break;
            }
            let Some(meta) = line.strip_prefix("//").map(str::trim).filter(|_| in_block) else {
                continue;
            };
            let Some(meta) = meta.strip_prefix('@') else {
                continue;
            };
            let (key, value) = meta.split_once(char::is_whitespace).unwrap_or((meta, ""));
            let value = value.trim().to_string();
            match key {
                "name" => script.name = value,
                "version" => script.version = value,
                "description" => script.description = value,
                "match" => script.matches.push(value),
                "include" => script.includes.push(value),
                "exclude" | "exclude-match" => script.excludes.push(value),
                "noframes" => script.no_frames = true,
                "run-at" => {
                    script.run_at = match value.as_str() {
                        "document-start" => RunAt::Start,
                        "document-idle" => RunAt::Idle,
                        _ => RunAt::End,
                    }
                }
                _ => {}
            }
        }
        script
    }

    /// Whether the script runs on `url`. Without `@match` or `@include` it
    /// runs everywhere; `@exclude` always wins.
    pub fn applies_to(&self, url: &str) -> bool {
        if self
            .excludes
            .iter()
            .any(|p| glob(p, url) || match_pattern(p, url))
        {
            return false;
        }
        if self.matches.is_empty() && self.includes.is_empty() {
            return url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("file://");
        }
        self.matches.iter().any(|p| match_pattern(p, url))
            || self.includes.iter().any(|p| glob(p, url))
    }
}

pub fn dirs(paths: &Paths) -> [PathBuf; 2] {
    [
        paths.data_dir.join("greasemonkey"),
        paths.config_dir.join("greasemonkey"),
    ]
}

/// Every script in the greasemonkey directories, sorted by file name.
pub fn load(paths: &Paths) -> (Vec<Script>, Vec<String>) {
    let mut files: Vec<PathBuf> = dirs(paths)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "js"))
        .collect();
    files.sort_by_key(|p| p.file_name().map(ToOwned::to_owned));
    let mut errors = Vec::new();
    let scripts = files
        .iter()
        .filter_map(|path| match std::fs::read_to_string(path) {
            Ok(code) => Some(Script::parse(&file_name(path), &code)),
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                None
            }
        })
        .collect();
    (scripts, errors)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "\
// ==UserScript==
// @name        Dark news
// @version     1.2
// @match       *://*.example.com/news/*
// @include     https://other.org/*
// @exclude     https://other.org/private*
// @run-at      document-start
// @noframes
// ==/UserScript==
document.body.style.background = 'black';
";

    #[test]
    fn parses_the_metadata_block() {
        let s = Script::parse("dark.user.js", SCRIPT);
        assert_eq!(s.name, "Dark news");
        assert_eq!(s.version, "1.2");
        assert_eq!(s.matches, ["*://*.example.com/news/*"]);
        assert_eq!(s.run_at, RunAt::Start);
        assert!(s.no_frames);
        assert_eq!(Script::parse("plain.user.js", "alert(1)").name, "plain");
        assert_eq!(Script::parse("plain.js", "alert(1)").run_at, RunAt::End);
    }

    #[test]
    fn matches_urls() {
        let s = Script::parse("x.js", SCRIPT);
        assert!(s.applies_to("https://example.com/news/today"));
        assert!(s.applies_to("http://www.example.com/news/"));
        assert!(!s.applies_to("https://example.com/sports/"));
        assert!(!s.applies_to("https://badexample.com/news/x"));
        assert!(!s.applies_to("ftp://example.com/news/x"));
        assert!(s.applies_to("https://other.org/a"));
        assert!(!s.applies_to("https://other.org/private/a"));
        let everywhere = Script::parse("e.js", "1");
        assert!(everywhere.applies_to("https://a.org/"));
        assert!(!everywhere.applies_to("riptide://help/"));
    }

    #[test]
    fn loads_scripts_from_both_directories() {
        let base = std::env::temp_dir().join(format!("rt-gm-{}", std::process::id()));
        let paths = Paths::resolve(Some(&base)).unwrap();
        let [data, config] = dirs(&paths);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(data.join("b.user.js"), "1").unwrap();
        std::fs::write(config.join("a.js"), "2").unwrap();
        std::fs::write(config.join("notes.txt"), "x").unwrap();
        let (scripts, errors) = load(&paths);
        assert!(errors.is_empty());
        assert_eq!(
            scripts.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
