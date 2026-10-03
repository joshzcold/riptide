//! Turn off Chromium's background calls to Google that only serve Google.
//! Security updates (CRLSets, PKI metadata) keep working; the README's
//! "Network traffic" section lists what is left.
//!
//! Most of these services start within the first 100 ms, before any CEF
//! callback could change a preference, so the values are written into
//! Chromium's preference files before CEF starts.

use std::path::Path;

use cef::*;
use serde_json::{Value, json};

/// Features disabled on the command line.
const DISABLED_FEATURES: &[&str] = &[
    // AI Mode, which hackers-browser has no UI for. On startup it fetches
    // google.com/async/folae to check whether the user may use it.
    "AimEnabled",
    // Keeps connections open to the default search engine (google.com).
    "PreconnectToSearch",
    "SearchEnginePreconnect2",
];

/// Browser-wide preferences, in `Local State`.
fn global_prefs(component_updates: bool) -> Vec<(&'static str, Value)> {
    vec![
        // Off, this stops component updates except the ones Chromium exempts
        // as security data (CRLSets and the like), as the
        // ComponentUpdatesEnabled policy does.
        (
            "component_updates.component_updates_enabled",
            json!(component_updates),
        ),
    ]
}

/// Say so once Chromium has fetched the CDM, which loads at the next start.
pub fn watch_widevine_download(data_dir: std::path::PathBuf) {
    crate::shell::show_message(
        hb_core::engine::Level::Info,
        "Downloading Widevine from Google; this takes a minute",
    );
    std::thread::spawn(move || {
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if widevine_installed(&data_dir) {
                crate::shell::post_message(
                    hb_core::engine::Level::Info,
                    "Widevine downloaded; restart to enable it".into(),
                );
                return;
            }
        }
    });
}

/// Whether Chromium has downloaded the Widevine CDM into this profile.
pub fn widevine_installed(data_dir: &Path) -> bool {
    std::fs::read_dir(data_dir.join("WidevineCdm"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|version| version.path().join("manifest.json").is_file())
}

/// Per-profile preferences, in `Default/Preferences`.
fn profile_prefs() -> Vec<(&'static str, Value)> {
    vec![
        // No Google sign-in, so no accounts.google.com/ListAccounts checks.
        // Chromium reads the second one at startup.
        ("signin.allowed", json!(false)),
        ("signin.allowed_on_next_startup", json!(false)),
        // hackers-browser has its own search engines (`url.searchengines`).
        // Without Chrome's, nothing preconnects to or prewarms google.com.
        ("default_search_provider.enabled", json!(false)),
        // Dictionaries are downloaded from Google; M17 makes spell checking opt-in.
        ("browser.enable_spellchecking", json!(false)),
        ("spellcheck.dictionaries", json!([])),
    ]
}

/// Adds our features to any `--disable-features` the user passed.
pub fn append_switches(command_line: &mut CommandLine) {
    crate::append_to_list_switch(
        command_line,
        "disable-features",
        &DISABLED_FEATURES.join(","),
    );
}

/// Write the preferences into the profile before CEF reads it.
/// `component_updates` is on only to fetch Widevine (`content.widevine`):
/// Chromium has no switch for a single component.
pub fn seed_prefs(data_dir: &Path, profile_dir: &Path, component_updates: bool) {
    seed(
        &data_dir.join("Local State"),
        &global_prefs(component_updates),
    );
    seed(&profile_dir.join("Preferences"), &profile_prefs());
}

fn seed(path: &Path, prefs: &[(&str, Value)]) {
    let mut root = match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => value,
            // Leave a file we can't read for Chromium to deal with.
            _ => {
                tracing::warn!(path = %path.display(), "can't parse; privacy preferences not applied");
                return;
            }
        },
        Err(_) => json!({}),
    };
    if !set_all(&mut root, prefs) {
        return;
    }
    let tmp = path.with_extension("hb-tmp");
    let written = std::fs::write(&tmp, root.to_string()).and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = written {
        tracing::warn!(path = %path.display(), "can't write privacy preferences: {e}");
    }
}

/// Set dotted paths in a JSON object. Returns whether anything changed.
fn set_all(root: &mut Value, prefs: &[(&str, Value)]) -> bool {
    let mut changed = false;
    for (name, value) in prefs {
        let mut node = &mut *root;
        let mut parts = name.split('.').peekable();
        while let Some(part) = parts.next() {
            if !node.is_object() {
                *node = json!({});
            }
            let map = node.as_object_mut().expect("just made an object");
            if parts.peek().is_none() {
                if map.get(part) != Some(value) {
                    map.insert(part.to_string(), value.clone());
                    changed = true;
                }
                break;
            }
            node = map.entry(part).or_insert_with(|| json!({}));
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_nested_values_and_keeps_the_rest() {
        let mut root = json!({ "browser": { "theme": 1 }, "signin": true });
        let prefs = [
            ("browser.enable_spellchecking", json!(false)),
            ("signin.allowed", json!(false)),
        ];
        assert!(set_all(&mut root, &prefs));
        assert_eq!(
            root,
            json!({
                "browser": { "theme": 1, "enable_spellchecking": false },
                "signin": { "allowed": false },
            })
        );
        assert!(!set_all(&mut root, &prefs));
    }
}
