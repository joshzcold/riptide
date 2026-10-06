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
    /// `@require` URLs, run before the script.
    #[serde(default)]
    pub requires: Vec<String>,
    /// The downloaded `@require` code, filled in by [`load`].
    #[serde(default)]
    pub required_code: String,
    /// What `GM_setValue` stored, filled in by [`load`].
    #[serde(default)]
    pub values: serde_json::Map<String, serde_json::Value>,
    /// `@grant`: the APIs the script asks for beyond the basic ones.
    #[serde(default)]
    pub grants: Vec<String>,
    /// `@connect`: the hosts `GM_xmlhttpRequest` may reach (`*` for any).
    #[serde(default)]
    pub connects: Vec<String>,
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
                "require" => script.requires.push(value),
                "grant" => script.grants.push(value),
                "connect" => script.connects.push(value.to_lowercase()),
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

    /// Whether `@grant` asks for `api` (either its `GM_` or `GM.` name).
    pub fn grants(&self, api: &str) -> bool {
        let dotted = api.replacen("GM_", "GM.", 1);
        self.grants
            .iter()
            .any(|g| g == api || g.eq_ignore_ascii_case(&dotted))
    }

    /// Whether `GM_xmlhttpRequest` from a page at `page` may fetch `target`:
    /// only http(s), and only the page's own host or an `@connect` host
    /// (with its subdomains), `self`, or `*`.
    pub fn may_connect(&self, page: &str, target: &str) -> bool {
        if !(target.starts_with("http://") || target.starts_with("https://")) {
            return false;
        }
        let host = rt_core::url::host(target).to_lowercase();
        let page_host = rt_core::url::host(page).to_lowercase();
        if host.is_empty() {
            return false;
        }
        host == page_host
            || self.connects.iter().any(|c| {
                c == "*"
                    || (c == "self" && host == page_host)
                    || host == *c
                    || host.ends_with(&format!(".{c}"))
            })
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

/// Downloaded `@require` files and `GM_setValue` values. Kept outside the
/// script directories, which load every `.js` file as a script.
fn data_dir(paths: &Paths) -> PathBuf {
    paths.data_dir.join("greasemonkey-data")
}

/// Where the code for an `@require` URL is kept.
pub fn require_path(paths: &Paths, url: &str) -> PathBuf {
    // FNV-1a: a stable file name for the URL.
    let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    data_dir(paths)
        .join("requires")
        .join(format!("{hash:016x}.js"))
}

/// Where a script's `GM_setValue` values are kept, by script name.
pub fn values_path(paths: &Paths, script: &str) -> PathBuf {
    let safe: String = script
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir(paths).join("values").join(format!("{safe}.json"))
}

/// `@require` URLs no script has downloaded yet.
pub fn missing_requires(paths: &Paths, scripts: &[Script]) -> Vec<String> {
    let mut missing: Vec<String> = scripts
        .iter()
        .flat_map(|s| &s.requires)
        .filter(|url| !require_path(paths, url).exists())
        .cloned()
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

/// Every script in the greasemonkey directories, sorted by file name, with
/// its downloaded `@require` code and stored values.
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
            Ok(code) => {
                let mut script = Script::parse(&file_name(path), &code);
                for url in &script.requires {
                    if let Ok(code) = std::fs::read_to_string(require_path(paths, url)) {
                        script.required_code.push_str(&code);
                        script.required_code.push_str(";\n");
                    }
                }
                script.values = std::fs::read_to_string(values_path(paths, &script.name))
                    .ok()
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or_default();
                Some(script)
            }
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

    #[test]
    fn grants_and_connects_limit_cross_origin_requests() {
        let script = Script::parse(
            "x.user.js",
            "// ==UserScript==\n// @grant GM.xmlHttpRequest\n// @connect API.example.org\n// ==/UserScript==\n",
        );
        assert!(script.grants("GM_xmlhttpRequest"));
        assert!(!script.grants("GM_openInTab"));
        let page = "https://news.site/a";
        assert!(script.may_connect(page, "https://news.site/b"));
        assert!(script.may_connect(page, "https://api.example.org/v1"));
        assert!(script.may_connect(page, "https://eu.api.example.org/v1"));
        assert!(!script.may_connect(page, "https://evilapi.example.org/"));
        assert!(!script.may_connect(page, "https://example.org/"));
        assert!(!script.may_connect(page, "file:///etc/passwd"));
        let any = Script {
            connects: vec!["*".into()],
            ..Script::default()
        };
        assert!(any.may_connect(page, "http://anything.test/"));
    }

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
        let lib = "https://cdn.example/lib.js";
        std::fs::write(
            config.join("c.js"),
            format!("// ==UserScript==\n// @require {lib}\n// @require https://cdn.example/missing.js\n// ==/UserScript==\n3"),
        )
        .unwrap();
        std::fs::create_dir_all(require_path(&paths, lib).parent().unwrap()).unwrap();
        std::fs::write(require_path(&paths, lib), "var lib = 1").unwrap();
        std::fs::create_dir_all(values_path(&paths, "c").parent().unwrap()).unwrap();
        std::fs::write(values_path(&paths, "c"), r#"{"seen": 2}"#).unwrap();
        let (scripts, errors) = load(&paths);
        assert!(errors.is_empty());
        assert_eq!(
            scripts.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(scripts[2].required_code, "var lib = 1;\n");
        assert_eq!(scripts[2].values["seen"], 2);
        assert_eq!(
            missing_requires(&paths, &scripts),
            ["https://cdn.example/missing.js"]
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
