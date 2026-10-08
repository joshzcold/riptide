//! Plugins' secret options (passwords, tokens) in the OS keyring: the macOS
//! Keychain, Windows' Credential Manager or the Secret Service on Linux. They
//! never go into a file, a log or a message. A plugin reads only its own,
//! and only ones its riptide-plugin.toml declares as secrets.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cef::*;

use rt_core::engine::Level;

use crate::shell;

const SERVICE: &str = "riptide";

/// `RIPTIDE_SECRET_STORE=memory` keeps secrets in memory, for tests on
/// machines without a keyring.
fn in_memory() -> bool {
    std::env::var("RIPTIDE_SECRET_STORE").is_ok_and(|v| v == "memory")
}

static MEMORY: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn account(plugin: &str, option: &str) -> String {
    format!("plugin/{plugin}/{option}")
}

/// Blocking keyring calls, for a background thread.
fn read(plugin: &str, option: &str) -> Result<Option<String>, String> {
    let account = account(plugin, option);
    if in_memory() {
        let memory = MEMORY.lock().map_err(|_| "the secret store is broken")?;
        return Ok(memory.as_ref().and_then(|m| m.get(&account).cloned()));
    }
    let entry = keyring::Entry::new(SERVICE, &account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn write(plugin: &str, option: &str, value: Option<&str>) -> Result<(), String> {
    let account = account(plugin, option);
    if in_memory() {
        let mut memory = MEMORY.lock().map_err(|_| "the secret store is broken")?;
        let memory = memory.get_or_insert_with(HashMap::new);
        match value {
            Some(value) => memory.insert(account, value.to_string()),
            None => memory.remove(&account),
        };
        return Ok(());
    }
    let entry = keyring::Entry::new(SERVICE, &account).map_err(|e| e.to_string())?;
    match value {
        Some(value) => entry.set_password(value).map_err(|e| e.to_string()),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

/// Which secrets are set, as `plugin/option` lines, so the Plugins tab can
/// say so without opening the keyring. Names only, never values.
fn marks_file(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-secrets")
}

fn marks(data_dir: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(marks_file(data_dir))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether plugin `plugin`'s secret `option` has been set.
pub fn is_set(data_dir: &Path, plugin: &str, option: &str) -> bool {
    marks(data_dir).contains(&format!("{plugin}/{option}"))
}

fn mark(data_dir: &Path, plugin: &str, option: &str, set: bool) {
    let mut all = marks(data_dir);
    let key = format!("{plugin}/{option}");
    if set {
        all.insert(key);
    } else {
        all.remove(&key);
    }
    let text: String = all.into_iter().map(|line| line + "\n").collect();
    let _ = std::fs::write(marks_file(data_dir), text);
}

/// The Plugins tab saved (or cleared, with `None`) a secret: into the
/// keyring on a background thread, then a message saying so.
pub fn store(data_dir: PathBuf, plugin: String, option: String, value: Option<String>) {
    std::thread::spawn(move || {
        let result = write(&plugin, &option, value.as_deref());
        let set = value.is_some();
        // The value is dropped here; only the outcome goes back.
        drop(value);
        let error = result.err().unwrap_or_default();
        let mut task = Stored::new(data_dir.display().to_string(), plugin, option, set, error);
        post_task(ThreadId::UI, Some(&mut task));
    });
}

wrap_task! {
    struct Stored {
        data_dir: String,
        plugin: String,
        option: String,
        set: bool,
        // Why the keyring refused; empty when it didn't.
        error: String,
    }

    impl Task {
        fn execute(&self) {
            if !self.error.is_empty() {
                return shell::show_message(
                    Level::Error,
                    format!("Couldn't keep {}'s {} in the keyring: {}", self.plugin, self.option, self.error),
                );
            }
            mark(Path::new(&self.data_dir), &self.plugin, &self.option, self.set);
            let what = if self.set { "Saved" } else { "Cleared" };
            shell::show_message(
                Level::Info,
                format!("{what} {}'s {} in the keyring", self.plugin, self.option),
            );
            crate::settings_page::refresh();
        }
    }
}

/// `rt.secret.get` from plugin `plugin`: the value of its secret `option`,
/// or why not, to Lua callback `callback`. `declared` says whether its
/// manifest has that option as a secret.
pub fn get(plugin: String, option: String, declared: bool, callback: u32) {
    if !declared {
        let why = format!("{option} isn't one of {plugin}'s secret options");
        return answer(&plugin, callback, &serde_json::json!({ "error": why }));
    }
    std::thread::spawn(move || {
        let envelope = match read(&plugin, &option) {
            Ok(value) => serde_json::json!({ "ok": value }),
            Err(e) => serde_json::json!({ "error": format!("the keyring: {e}") }),
        };
        let mut task = Read::new(plugin, callback, envelope.to_string());
        post_task(ThreadId::UI, Some(&mut task));
    });
}

wrap_task! {
    struct Read {
        plugin: String,
        callback: u32,
        // {"ok": value} or {"error": why}.
        envelope: String,
    }

    impl Task {
        fn execute(&self) {
            let context = crate::lua::current_context();
            let result = rt_config::lua::answered(self.callback, Some(self.envelope.clone()), &context);
            crate::lua::carry_out_for(&self.plugin, result);
        }
    }
}

fn answer(plugin: &str, callback: u32, envelope: &serde_json::Value) {
    let context = crate::lua::current_context();
    let result = rt_config::lua::answered(callback, Some(envelope.to_string()), &context);
    crate::lua::carry_out_for(plugin, result);
}
