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
    // AI Mode, which riptide has no UI for. On startup it fetches
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
        rt_core::engine::Level::Info,
        "Downloading Widevine from Google; this takes a minute",
    );
    std::thread::spawn(move || {
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if widevine_installed(&data_dir) {
                crate::shell::post_message(
                    rt_core::engine::Level::Info,
                    "Widevine downloaded; restart to enable it".into(),
                );
                return;
            }
        }
    });
}

/// When Chromium last had component updates on to update Widevine.
const WIDEVINE_LAST_UPDATE: &str = "widevine-last-update";

/// What startup does about the Widevine CDM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widevine {
    Off,
    /// `content.widevine` was turned off, so the CDM was deleted.
    Removed,
    /// On but not downloaded yet.
    Fetch,
    /// Installed, and `content.widevine_update_interval` days have passed.
    Update,
    Installed,
}

impl Widevine {
    /// Chromium has no switch for a single component, so updates are on for
    /// the whole run that fetches or updates the CDM.
    pub fn component_updates(self) -> bool {
        matches!(self, Self::Fetch | Self::Update)
    }
}

/// Before CEF starts, which is when Chromium loads the CDM: delete it when
/// `on` is false, and fetch it, or check for a newer one every `days` days (0: never).
pub fn prepare_widevine(data_dir: &Path, on: bool, days: i64) -> Widevine {
    let stamp = data_dir.join(WIDEVINE_LAST_UPDATE);
    match (on, widevine_installed(data_dir)) {
        (false, true) => match std::fs::remove_dir_all(data_dir.join("WidevineCdm")) {
            Ok(()) => {
                let _ = std::fs::remove_file(&stamp);
                Widevine::Removed
            }
            Err(e) => {
                tracing::warn!("can't remove the Widevine CDM: {e}");
                Widevine::Off
            }
        },
        (false, false) => Widevine::Off,
        (true, false) => {
            crate::background::record(&stamp);
            Widevine::Fetch
        }
        (true, true) if crate::background::due(&stamp, days) => Widevine::Update,
        (true, true) => Widevine::Installed,
    }
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
        // riptide has its own search engines (`url.searchengines`).
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
/// `component_updates` is on only to fetch or update Widevine (see [`Widevine`]).
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
    let tmp = path.with_extension("rt-tmp");
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

    #[test]
    fn widevine_is_fetched_updated_every_few_days_and_deleted_when_off() {
        let dir =
            std::env::temp_dir().join(format!("riptide-widevine-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stamp = dir.join(WIDEVINE_LAST_UPDATE);
        assert_eq!(prepare_widevine(&dir, false, 7), Widevine::Off);
        assert_eq!(prepare_widevine(&dir, true, 7), Widevine::Fetch);
        assert!(stamp.exists(), "the fetch starts the clock");

        let cdm = dir.join("WidevineCdm/4.10.2830.0");
        std::fs::create_dir_all(&cdm).unwrap();
        std::fs::write(cdm.join("manifest.json"), "{}").unwrap();
        assert_eq!(prepare_widevine(&dir, true, 7), Widevine::Installed);
        std::fs::write(&stamp, "0").unwrap();
        assert_eq!(
            prepare_widevine(&dir, true, 0),
            Widevine::Installed,
            "0: never"
        );
        assert_eq!(prepare_widevine(&dir, true, 7), Widevine::Update);
        assert_eq!(
            prepare_widevine(&dir, true, 7),
            Widevine::Installed,
            "not again so soon"
        );

        assert_eq!(prepare_widevine(&dir, false, 7), Widevine::Removed);
        assert!(!dir.join("WidevineCdm").exists());
        assert!(!stamp.exists());
        assert!(Widevine::Fetch.component_updates() && Widevine::Update.component_updates());
        assert!(!Widevine::Installed.component_updates() && !Widevine::Removed.component_updates());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
