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
fn global_prefs() -> Vec<(&'static str, Value)> {
    vec![
        // Stops component updates except the ones Chromium exempts as security
        // data (CRLSets and the like), as the ComponentUpdatesEnabled policy does.
        ("component_updates.component_updates_enabled", json!(false)),
    ]
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
    let name = CefString::from("disable-features");
    let given = CefStringUtf16::from(&command_line.switch_value(Some(&name))).to_string();
    let mut features: Vec<&str> = given.split(',').filter(|f| !f.is_empty()).collect();
    features.extend(DISABLED_FEATURES);
    command_line.append_switch_with_value(
        Some(&name),
        Some(&CefString::from(features.join(",").as_str())),
    );
}

/// Write the preferences into the profile before CEF reads it.
pub fn seed_prefs(data_dir: &Path, profile_dir: &Path) {
    seed(&data_dir.join("Local State"), &global_prefs());
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
