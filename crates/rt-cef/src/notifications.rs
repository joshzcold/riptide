//! `content.notifications.presenter = libnotify`: page notifications sent to
//! the desktop by riptide itself, through `notify-send`, so their app name,
//! urgency, timeout and icon follow riptide's settings. Clicking one shows
//! its tab. The page's text goes in as separate arguments; no shell is run.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use cef::*;
use rt_core::engine::Level;

use crate::shell;

/// One notification, read from the settings on the UI thread.
struct Desktop {
    app_name: String,
    urgency: String,
    timeout: i64,
    icon: Option<PathBuf>,
    summary: String,
    body: String,
    /// The tab to show when it's clicked.
    browser: Option<i32>,
}

/// Show a page's notification on the desktop. `origin` is the site, when
/// `content.notifications.show_origin` asks for it.
pub fn show(browser: Option<i32>, origin: Option<String>, title: String, body: String) {
    let Some(mut desktop) = shell::with(|s| {
        let settings = s.engine.settings();
        Desktop {
            app_name: settings.str("content.notifications.app_name").to_string(),
            urgency: settings.str("content.notifications.urgency").to_string(),
            timeout: settings.int("content.notifications.timeout"),
            icon: None,
            summary: title.clone(),
            body: match &origin {
                Some(origin) if body.is_empty() => origin.clone(),
                Some(origin) => format!("{origin}\n{body}"),
                None => body.clone(),
            },
            browser,
        }
    }) else {
        return;
    };
    if shell::with(|s| s.engine.settings().bool("content.notifications.site_icon")).unwrap_or(false)
    {
        desktop.icon = browser.and_then(site_icon);
    }
    std::thread::spawn(move || send(desktop));
}

/// The tab's favicon as a PNG file, for `notify-send --icon`.
fn site_icon(id: i32) -> Option<PathBuf> {
    let (favicon, dir) = shell::with(|s| {
        let favicon = s
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .find(|t| t.browser().is_some_and(|b| b.identifier() == id))
            .and_then(|t| t.favicon.clone());
        (favicon, s.paths.data_dir.join("notification-icons"))
    })?;
    let encoded = favicon?.strip_prefix("data:image/png;base64,")?.to_string();
    let bytes = base64_decode(Some(&CefString::from(encoded.as_str())))?;
    let size = bytes.size();
    let mut png = vec![0u8; size];
    if bytes.data(Some(&mut png), 0) != size {
        return None;
    }
    // Named by content, so each site's icon is written once.
    let name = format!("{:016x}.png", fnv(&png));
    let path = dir.join(name);
    if !path.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, &png).ok()?;
    }
    Some(path)
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Whether this `notify-send` can wait for a click (`--action`, libnotify 0.7.9+).
fn supports_actions() -> bool {
    static ACTIONS: OnceLock<bool> = OnceLock::new();
    *ACTIONS.get_or_init(|| {
        Command::new("notify-send")
            .arg("--help")
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("--action"))
    })
}

/// The `notify-send` arguments for `desktop`.
fn args(desktop: &Desktop, actions: bool) -> Vec<String> {
    let mut args = vec![
        format!("--app-name={}", desktop.app_name),
        format!("--urgency={}", desktop.urgency),
    ];
    // -1 leaves it to the desktop.
    if desktop.timeout >= 0 {
        args.push(format!("--expire-time={}", desktop.timeout));
    }
    if let Some(icon) = &desktop.icon {
        args.push(format!("--icon={}", icon.display()));
    }
    if actions && desktop.browser.is_some() {
        args.push("--action=default=Show".into());
        args.push("--wait".into());
    }
    // Page text after `--`, so it can't be read as an option.
    args.push("--".into());
    args.push(desktop.summary.clone());
    args.push(desktop.body.clone());
    args
}

/// Off the UI thread: run `notify-send` and wait for a click.
fn send(desktop: Desktop) {
    let args = args(&desktop, supports_actions());
    match Command::new("notify-send").args(&args).output() {
        Ok(output) => {
            let clicked = String::from_utf8_lossy(&output.stdout).trim() == "default";
            if let (true, Some(id)) = (clicked, desktop.browser) {
                let mut task = ShowTab::new(id);
                post_task(ThreadId::UI, Some(&mut task));
            }
        }
        Err(e) => {
            let text = if e.kind() == std::io::ErrorKind::NotFound {
                "notify-send isn't installed (libnotify); showing the notification here instead"
                    .to_string()
            } else {
                format!("notify-send failed: {e}")
            };
            shell::post_message(Level::Warning, text);
            shell::post_message(
                Level::Info,
                format!("{}: {}", desktop.summary, desktop.body),
            );
        }
    }
}

wrap_task! {
    struct ShowTab {
        id: i32,
    }

    impl Task {
        fn execute(&self) {
            show_tab(self.id);
        }
    }
}

/// A notification was clicked: bring its window forward on its tab.
fn show_tab(id: i32) {
    let found = shell::with(|s| {
        s.windows.iter().enumerate().find_map(|(w, state)| {
            state
                .tabs
                .position(|t| t.browser().is_some_and(|b| b.identifier() == id))
                .map(|t| (w, t))
        })
    })
    .flatten();
    let Some((window, tab)) = found else { return };
    shell::with(|s| s.active = window);
    crate::tabs::select(tab);
    if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
        window.activate();
    }
    shell::refresh_ui();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop() -> Desktop {
        Desktop {
            app_name: "riptide".into(),
            urgency: "normal".into(),
            timeout: -1,
            icon: None,
            summary: "--help".into(),
            body: "https://example.com\nHi".into(),
            browser: Some(3),
        }
    }

    #[test]
    fn page_text_comes_after_the_options() {
        let args = args(&desktop(), false);
        assert_eq!(
            args,
            [
                "--app-name=riptide",
                "--urgency=normal",
                "--",
                "--help",
                "https://example.com\nHi"
            ]
        );
    }

    #[test]
    fn timeout_icon_and_click_when_set() {
        let mut d = desktop();
        d.timeout = 0;
        d.icon = Some(PathBuf::from("/tmp/i.png"));
        let args = args(&d, true);
        assert!(args.contains(&"--expire-time=0".to_string()));
        assert!(args.contains(&"--icon=/tmp/i.png".to_string()));
        assert!(args.contains(&"--wait".to_string()));
        let end = args.iter().position(|a| a == "--").unwrap();
        assert!(args.iter().position(|a| a == "--wait").unwrap() < end);
    }
}
