//! The `hb://help/` page: a template filled with data from the live
//! registries. It is rebuilt on the UI thread whenever config changes and
//! read by the scheme handler on CEF's IO thread.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use hb_core::Command;
use hb_core::command::OpenTarget;
use hb_core::engine::Level;

use crate::shell;

const TEMPLATE: &str = include_str!("../ui/help.html");

static PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn page() -> Arc<[u8]> {
    let cached = PAGE.read().ok().and_then(|p| p.clone());
    cached.unwrap_or_else(|| Arc::from(TEMPLATE.as_bytes()))
}

fn cef_version() -> String {
    let raw = cef::sys::CEF_VERSION;
    String::from_utf8_lossy(&raw[..raw.len().saturating_sub(1)]).into_owned()
}

/// `hackers-browser 0.1.0 (abc1234, CEF …)` for `--version`.
pub fn version_line() -> String {
    format!(
        "hackers-browser {} ({}, CEF {})",
        env!("CARGO_PKG_VERSION"),
        env!("HB_GIT_COMMIT"),
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
            ("Git commit".to_string(), env!("HB_GIT_COMMIT").to_string()),
            ("CEF".to_string(), cef_version()),
            (
                "Platform".to_string(),
                hb_config::Platform::current().name().to_string(),
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
        ];
        let data = hb_core::help::build(s.engine.keymap(), s.engine.settings(), &sources, info);
        // `</` would end the inline <script> early.
        let json = serde_json::to_string(&data)
            .unwrap_or_else(|_| "null".into())
            .replace("</", "<\\/");
        TEMPLATE.replace("/*HB_DATA*/null", &json)
    });
    if let (Some(html), Ok(mut page)) = (html, PAGE.write()) {
        *page = Some(Arc::from(html.into_bytes()));
    }
}

pub fn run_command(command: &Command) -> bool {
    let (tab, topic) = match command {
        Command::Help { tab, topic } => (*tab, topic.as_deref()),
        Command::Version => (false, Some("version")),
        _ => return false,
    };
    let anchor = match hb_core::help::anchor(topic) {
        Ok(anchor) => anchor,
        Err(e) => {
            shell::show_message(Level::Error, e);
            return true;
        }
    };
    refresh();
    let url = match anchor {
        Some(anchor) => format!("hb://help/#{anchor}"),
        None => "hb://help/".to_string(),
    };
    let target = if tab {
        OpenTarget::Tab
    } else {
        OpenTarget::Current
    };
    shell::open(target, true, Some(url));
    true
}
