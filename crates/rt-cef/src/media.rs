//! `:media-devices`: pick the camera, microphone and speaker a site gets.
//! The choice is a per-site `content.media.*` setting, which pages apply
//! themselves (`media_devices.js`), so a call in progress switches at once.

use std::cell::RefCell;

use cef::*;
use rt_core::Command;
use rt_core::engine::Level;
use rt_core::prompt::{PromptAnswer, PromptKind, Topic};

use crate::prompts::{self, Scope};
use crate::renderer::{MEDIA_KINDS, MEDIA_LIST_MESSAGE};
use crate::shell;

/// What `enumerateDevices` calls each of [`MEDIA_KINDS`], and what riptide does.
const DEVICE_KINDS: [&str; 3] = ["videoinput", "audioinput", "audiooutput"];
const LABELS: [&str; 3] = ["Camera", "Microphone", "Speaker"];

thread_local! {
    /// The tab `:media-devices` asked for its devices, and its URL.
    static ASKED: RefCell<Option<(i32, String)>> = const { RefCell::new(None) };
}

pub fn run_command(command: &Command) -> bool {
    if !matches!(command, Command::MediaDevices) {
        return false;
    }
    let current =
        shell::with(|s| Some((s.current_browser()?, s.tabs.current()?.url.clone()))).flatten();
    let Some((browser, url)) = current.filter(|(_, url)| url.starts_with("http")) else {
        shell::show_message(Level::Error, "Pick media devices on a web page");
        return true;
    };
    let (Some(frame), Some(mut message)) = (
        browser.main_frame(),
        process_message_create(Some(&CefString::from(MEDIA_LIST_MESSAGE))),
    ) else {
        return true;
    };
    ASKED.with(|a| *a.borrow_mut() = Some((browser.identifier(), url)));
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    true
}

#[derive(serde::Deserialize)]
struct Device {
    kind: String,
    label: String,
}

/// Each kind's device names, in the page's order, from its `enumerateDevices` JSON.
fn names_by_kind(json: &str) -> [Vec<String>; 3] {
    let devices: Vec<Device> = serde_json::from_str(json).unwrap_or_default();
    DEVICE_KINDS.map(|kind| {
        let mut names: Vec<String> = Vec::new();
        for device in devices
            .iter()
            .filter(|d| d.kind == kind && !d.label.is_empty())
        {
            if !names.contains(&device.label) {
                names.push(device.label.clone());
            }
        }
        names
    })
}

/// The page reported its devices: ask which kind to change, then which device.
pub fn listed(browser: i32, json: &str) {
    let Some((_, url)) = ASKED.with(|a| a.borrow_mut().take_if(|(id, _)| *id == browser)) else {
        return;
    };
    let site = rt_core::url::origin(&url)
        .map(|o| o.trim_end_matches('/').to_string())
        .unwrap_or_else(|| url.clone());
    let names = names_by_kind(json);
    if names.iter().all(Vec::is_empty) {
        return shell::show_message(
            Level::Error,
            format!("{site} can't see device names until it may use your camera or microphone"),
        );
    }
    let chosen: Vec<String> = MEDIA_KINDS
        .iter()
        .map(|kind| {
            shell::with(|s| {
                s.engine
                    .settings()
                    .str_for(&format!("content.media.{kind}"), &url)
                    .to_string()
            })
            .unwrap_or_default()
        })
        .collect();
    let items = (0..3)
        .map(|i| match chosen[i].as_str() {
            "" => format!("{}: the site's own choice", LABELS[i]),
            name => format!("{}: {name}", LABELS[i]),
        })
        .collect();
    prompts::ask(
        Some(browser),
        Scope::Other,
        Topic::Confirm,
        "Media devices",
        format!("Devices for {site}"),
        PromptKind::Select { items },
        move |answer| {
            let Some(kind) = picked(&answer).filter(|&k| k < 3) else {
                return;
            };
            pick_device(browser, site, kind, names[kind].clone());
        },
    );
}

fn picked(answer: &PromptAnswer) -> Option<usize> {
    match answer {
        PromptAnswer::Text(index) => index.parse().ok(),
        _ => None,
    }
}

fn pick_device(browser: i32, site: String, kind: usize, names: Vec<String>) {
    let label = LABELS[kind];
    if names.is_empty() {
        return shell::show_message(
            Level::Error,
            format!("{site} lists no {} names", label.to_lowercase()),
        );
    }
    let items = std::iter::once("The site's own choice".to_string())
        .chain(names.iter().cloned())
        .take(rt_core::prompt::SELECT_MAX)
        .collect();
    prompts::ask(
        Some(browser),
        Scope::Other,
        Topic::Confirm,
        label,
        format!("{label} for {site}"),
        PromptKind::Select { items },
        move |answer| {
            let Some(index) = picked(&answer) else { return };
            let device = match index {
                0 => String::new(),
                i => match names.get(i - 1) {
                    Some(name) => name.clone(),
                    None => return,
                },
            };
            save(&site, kind, &device);
        },
    );
}

/// Keep the choice as a per-site setting in `autoconfig.toml`; pages pick it up at once.
fn save(site: &str, kind: usize, device: &str) {
    let op = rt_core::config::ConfigOp::SetFor {
        pattern: site.to_string(),
        name: format!("content.media.{}", MEDIA_KINDS[kind]),
        value: rt_core::settings::Value::Str(device.to_string()),
    };
    match shell::with(|s| s.engine.apply_config(&op)) {
        Some(Ok(_)) => shell::apply(vec![rt_core::Effect::ConfigChanged(op)]),
        Some(Err(e)) => return shell::show_message(Level::Error, e),
        None => return,
    }
    let device = if device.is_empty() {
        "the site's own choice"
    } else {
        device
    };
    shell::show_message(
        Level::Info,
        format!("{} for {site}: {device}", LABELS[kind]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_come_by_kind_once_each_without_hidden_ones() {
        let json = r#"[{"kind":"audioinput","label":"Mic A"},{"kind":"audioinput","label":"Mic A"},
            {"kind":"videoinput","label":"Cam"},{"kind":"audiooutput","label":""},{"kind":"audioinput","label":"Mic B"}]"#;
        let names = names_by_kind(json);
        assert_eq!(names[0], ["Cam"]);
        assert_eq!(names[1], ["Mic A", "Mic B"]);
        assert!(names[2].is_empty());
        assert!(names_by_kind("not json").iter().all(Vec::is_empty));
    }
}
