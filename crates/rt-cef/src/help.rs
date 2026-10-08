//! The `riptide://help/` page: a template filled with data from the live
//! registries. It is rebuilt on the UI thread whenever config changes and
//! read by the scheme handler on CEF's IO thread.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rt_core::Command;
use rt_core::command::OpenTarget;
use rt_core::engine::Level;

use crate::shell;

const TEMPLATE: &str = include_str!("../ui/help.html");
const CHANGELOG_TEMPLATE: &str = include_str!("../ui/changelog.html");
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");
const HISTORY_TEMPLATE: &str = include_str!("../ui/history.html");

/// `riptide://history/`, rebuilt each time `:history` runs.
static HISTORY_PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn history_page() -> Arc<[u8]> {
    let cached = HISTORY_PAGE.read().ok().and_then(|p| p.clone());
    cached.unwrap_or_else(|| Arc::from(HISTORY_TEMPLATE.as_bytes()))
}

/// How many entries the history page lists.
const HISTORY_PAGE_ENTRIES: usize = 2000;

static PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn page() -> Arc<[u8]> {
    let cached = PAGE.read().ok().and_then(|p| p.clone());
    cached.unwrap_or_else(|| Arc::from(TEMPLATE.as_bytes()))
}

/// `riptide://changelog/`: the CHANGELOG.md this binary was built with.
pub fn changelog_page() -> Arc<[u8]> {
    static PAGE: std::sync::OnceLock<Arc<[u8]>> = std::sync::OnceLock::new();
    PAGE.get_or_init(|| {
        let body = rt_core::changelog::to_html(CHANGELOG);
        Arc::from(
            CHANGELOG_TEMPLATE
                .replace("<!--RT_BODY-->", &body)
                .into_bytes(),
        )
    })
    .clone()
}

/// Tell the user once when the browser was updated since the last start.
/// Returns whether `changelog_after_upgrade` wants the changelog opened.
pub fn note_upgrade(data_dir: &std::path::Path) -> bool {
    let path = data_dir.join("last-version");
    let current = env!("CARGO_PKG_VERSION");
    let last = std::fs::read_to_string(&path).ok();
    if last.as_deref().map(str::trim) == Some(current) {
        return false;
    }
    if let Err(e) = std::fs::write(&path, current) {
        tracing::warn!("can't write {}: {e}", path.display());
    }
    let Some(last) = last else {
        return false;
    };
    shell::show_message(
        Level::Info,
        format!("Updated to {current}; :changelog for details"),
    );
    let level = shell::with(|s| {
        s.engine
            .settings()
            .str("changelog_after_upgrade")
            .to_string()
    })
    .unwrap_or_default();
    rt_core::changelog::show_after_upgrade(&level, &last, current)
}

fn cef_version() -> String {
    let raw = cef::sys::CEF_VERSION;
    String::from_utf8_lossy(&raw[..raw.len().saturating_sub(1)]).into_owned()
}

/// `riptide 0.1.0 (abc1234, CEF …)` for `--version`.
/// The sandbox state, decided once at startup.
pub static SANDBOX: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn version_line() -> String {
    format!(
        "riptide {} ({}, CEF {})",
        env!("CARGO_PKG_VERSION"),
        env!("RT_GIT_COMMIT"),
        cef_version()
    )
}

/// Rebuild the page from the engine's current settings and bindings.
pub fn refresh() {
    let html = shell::with(|s| {
        let sources: HashMap<String, String> = s.setting_sources.clone().into_iter().collect();
        let files = if s.config_files.is_empty() {
            "none".to_string()
        } else {
            s.config_files
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let info = vec![
            ("Version".to_string(), env!("CARGO_PKG_VERSION").to_string()),
            ("Git commit".to_string(), env!("RT_GIT_COMMIT").to_string()),
            ("CEF".to_string(), cef_version()),
            (
                "Platform".to_string(),
                rt_config::Platform::current().name().to_string(),
            ),
            (
                "Config directory".to_string(),
                s.paths.config_dir.display().to_string(),
            ),
            (
                "Data directory".to_string(),
                s.paths.data_dir.display().to_string(),
            ),
            ("Config files loaded".to_string(), files),
            (
                "Sandbox".to_string(),
                SANDBOX.get().cloned().unwrap_or_default(),
            ),
        ];
        let mut data = rt_core::help::build(
            s.engine.keymap(),
            s.engine.settings(),
            &sources,
            info,
            s.engine.user_commands(),
        );
        data.lua = rt_config::lua_types::api()
            .into_iter()
            .filter_map(|entry| serde_json::to_value(entry).ok())
            .collect();
        // `</` would end the inline <script> early.
        let json = serde_json::to_string(&data)
            .unwrap_or_else(|_| "null".into())
            .replace("</", "<\\/");
        TEMPLATE.replace("/*RT_DATA*/null", &json)
    });
    // Plugins' READMEs are read outside the shell borrow, then added.
    let plugins = serde_json::to_string(&crate::plugins::help_entries())
        .unwrap_or_else(|_| "[]".into())
        .replace("</", "<\\/");
    let html = html.map(|h| h.replace("/*RT_PLUGINS*/[]", &plugins));
    if let (Some(html), Ok(mut page)) = (html, PAGE.write()) {
        *page = Some(Arc::from(html.into_bytes()));
    }
}

/// `rt.ui.float` or a plugin's name: their place on the help page.
fn extra_anchor(topic: &str) -> Option<String> {
    if rt_config::lua_types::api().iter().any(|e| e.name == topic) {
        return Some(format!("lua-{topic}"));
    }
    rt_config::lua::plugin_specs()
        .iter()
        .any(|s| s.name == topic)
        .then(|| format!("plugin-{topic}"))
}

/// `:help` topics matching `pattern`.
pub fn completions(pattern: &str) -> Vec<rt_core::completion::Completion> {
    let pattern = pattern
        .trim_start_matches("-t ")
        .trim_start_matches("--tab ");
    let item = |category: &'static str, name: String, description: String| {
        rt_core::completion::Completion {
            icon: None,
            time: None,
            detail: None,
            category,
            name,
            description,
        }
    };
    let mut topics: Vec<(&'static str, String, String)> = Vec::new();
    topics.extend(
        rt_core::command::COMMANDS
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| {
                (
                    "Commands",
                    format!(":{}", c.name),
                    c.description.to_string(),
                )
            }),
    );
    topics.extend(
        rt_core::settings::SETTINGS
            .iter()
            .map(|d| ("Settings", d.name.to_string(), d.description.to_string())),
    );
    topics.extend(
        rt_config::lua_types::api()
            .into_iter()
            .map(|e| ("Lua API", e.name, e.doc)),
    );
    topics.extend(
        rt_config::lua::plugin_specs()
            .into_iter()
            .map(|s| ("Plugins", s.name, String::new())),
    );
    rt_core::completion::ranked(topics, pattern, |(_, name, _)| name.as_str())
        .into_iter()
        .map(|(category, name, description)| item(category, name, description))
        .collect()
}

pub fn run_command(command: &Command) -> bool {
    let (tab, topic) = match command {
        Command::History { tab } => {
            let entries = crate::storage::recent_history(HISTORY_PAGE_ENTRIES);
            // `</` would end the inline <script> early.
            let json = serde_json::to_string(&entries)
                .unwrap_or_else(|_| "[]".into())
                .replace("</", "<\\/");
            if let Ok(mut page) = HISTORY_PAGE.write() {
                *page = Some(Arc::from(
                    HISTORY_TEMPLATE
                        .replace("/*RT_DATA*/null", &json)
                        .into_bytes(),
                ));
            }
            let target = if *tab {
                OpenTarget::Tab
            } else {
                OpenTarget::Current
            };
            shell::open(target, true, Some("riptide://history/".to_string()));
            return true;
        }
        Command::Recover => {
            shell::open(OpenTarget::Tab, true, Some(crate::recover::URL.to_string()));
            return true;
        }
        Command::CrashReport => {
            let email = shell::with(|s| s.engine.settings().str("crash_report.email").to_string())
                .unwrap_or_default();
            crate::crash::set_email(email);
            shell::open(OpenTarget::Tab, true, Some("riptide://crash/".to_string()));
            return true;
        }
        Command::Changelog { tab } => {
            let target = if *tab {
                OpenTarget::Tab
            } else {
                OpenTarget::Current
            };
            shell::open(target, true, Some("riptide://changelog/".to_string()));
            return true;
        }
        Command::Help { tab, topic } => (*tab, topic.as_deref()),
        Command::Version => (false, Some("version")),
        Command::Report => {
            let body = format!(
                "**What happened:**\n\n**What you expected:**\n\n**Steps to reproduce:**\n\n---\n{}\nOS: {} {}\nSandbox: {}",
                version_line(),
                std::env::consts::OS,
                std::env::consts::ARCH,
                SANDBOX.get().map_or("unknown", String::as_str),
            );
            let url = format!(
                "https://github.com/joshzcold/riptide/issues/new?body={}",
                rt_core::url::encode_query(&body)
            );
            shell::open(OpenTarget::Tab, true, Some(url));
            return true;
        }
        Command::DebugLogFilter { filter } => {
            match crate::set_log_filter(filter) {
                Ok(()) => shell::show_message(Level::Info, format!("Log filter: {filter}")),
                Err(e) => {
                    shell::show_message(Level::Error, format!("Bad log filter {filter:?}: {e}"))
                }
            }
            return true;
        }
        _ => return false,
    };
    let anchor = match topic.map(str::trim).and_then(extra_anchor) {
        Some(anchor) => Ok(Some(anchor)),
        None => rt_core::help::anchor(topic),
    };
    let anchor = match anchor {
        Ok(anchor) => anchor,
        Err(e) => {
            shell::show_message(Level::Error, e);
            return true;
        }
    };
    refresh();
    let url = match anchor {
        Some(anchor) => format!("riptide://help/#{anchor}"),
        None => "riptide://help/".to_string(),
    };
    let target = if tab {
        OpenTarget::Tab
    } else {
        OpenTarget::Current
    };
    shell::open(target, true, Some(url));
    true
}
