//! Plugins' secret options (passwords, tokens) in the OS keyring: the macOS
//! Keychain, Windows' Credential Manager or the Secret Service on Linux. On
//! Linux without a Secret Service they go to the kernel keyring, which keeps
//! them until the computer restarts, or else to memory until riptide quits.
//! They never go into a file, a log or a message. A plugin reads only its
//! own, and only ones its riptide-plugin.toml declares as secrets.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cef::*;

use rt_core::engine::Level;

use crate::shell;

/// The keyring service secrets are kept under. Tests name their own
/// (`RIPTIDE_SECRET_SERVICE`), since the kernel keyring is the user's.
fn service() -> String {
    #[cfg(any(debug_assertions, feature = "test-control"))]
    if let Ok(service) = std::env::var("RIPTIDE_SECRET_SERVICE") {
        return service;
    }
    "riptide".to_string()
}

/// Where a secret is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Store {
    Keyring,
    /// The kernel keyring (keyutils), on Linux without a Secret Service.
    Kernel,
    Memory,
}

impl Store {
    fn name(self) -> &'static str {
        match self {
            Store::Keyring => "keyring",
            Store::Kernel => "kernel",
            Store::Memory => "memory",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        [Store::Keyring, Store::Kernel, Store::Memory]
            .into_iter()
            .find(|s| s.name() == name)
    }

    /// Where a saved secret went, and for how long, for the message.
    fn kept(self) -> &'static str {
        match self {
            Store::Keyring => "in the keyring",
            Store::Kernel => {
                "in the kernel keyring until the computer restarts, as there's no keyring service"
            }
            Store::Memory => "until riptide quits, as there's no keyring service",
        }
    }
}

/// `RIPTIDE_SECRET_STORE=memory` keeps secrets in memory, for tests on
/// machines without a keyring; `kernel` uses the kernel keyring, as Linux
/// does without a Secret Service.
fn forced() -> Option<Store> {
    std::env::var("RIPTIDE_SECRET_STORE")
        .ok()
        .and_then(|v| Store::parse(&v))
}

static MEMORY: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn account(plugin: &str, option: &str) -> String {
    format!("plugin/{plugin}/{option}")
}

/// The entry for `account` in `store`, which isn't [`Store::Memory`].
fn entry(store: Store, account: &str) -> keyring_core::Result<keyring_core::Entry> {
    match store {
        #[cfg(target_os = "linux")]
        Store::Kernel => {
            use keyring_core::api::CredentialStoreApi;
            linux_keyutils_keyring_store::Store::new()?.build(&service(), account, None)
        }
        _ => keyring::Entry::new(&service(), account).map(|e| e.inner),
    }
}

/// Blocking keyring calls, for a background thread.
fn read(store: Store, plugin: &str, option: &str) -> Result<Option<String>, String> {
    let account = account(plugin, option);
    if store == Store::Memory {
        let memory = MEMORY.lock().map_err(|_| "the secret store is broken")?;
        return Ok(memory.as_ref().and_then(|m| m.get(&account).cloned()));
    }
    match entry(store, &account).and_then(|e| e.get_password()) {
        Ok(value) => Ok(Some(value)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn write_to(store: Store, account: &str, value: Option<&str>) -> Result<(), keyring_core::Error> {
    if store == Store::Memory {
        let mut memory = MEMORY
            .lock()
            .map_err(|_| keyring_core::Error::Invalid("store".into(), "broken".into()))?;
        let memory = memory.get_or_insert_with(HashMap::new);
        match value {
            Some(value) => memory.insert(account.to_string(), value.to_string()),
            None => memory.remove(account),
        };
        return Ok(());
    }
    let entry = entry(store, account)?;
    match value {
        Some(value) => entry.set_password(value),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e),
        },
    }
}

/// Keep (or, with `None`, clear) a secret, and say where it went. Clearing
/// uses the store it was kept in; saving tries the keyring first.
fn write(
    plugin: &str,
    option: &str,
    value: Option<&str>,
    kept: Option<Store>,
) -> Result<Store, String> {
    let account = account(plugin, option);
    if value.is_none() {
        let store = kept.or_else(forced).unwrap_or(Store::Keyring);
        return write_to(store, &account, None)
            .map(|()| store)
            .map_err(|e| e.to_string());
    }
    if let Some(store) = forced() {
        return write_to(store, &account, value)
            .map(|()| store)
            .map_err(|e| e.to_string());
    }
    match write_to(Store::Keyring, &account, value) {
        Ok(()) => Ok(Store::Keyring),
        // No Secret Service at all, rather than a locked one or a refused prompt.
        Err(keyring_core::Error::PlatformFailure(_) | keyring_core::Error::NoDefaultStore)
            if cfg!(target_os = "linux") =>
        {
            let fallback = if write_to(Store::Kernel, &account, value).is_ok() {
                Store::Kernel
            } else {
                write_to(Store::Memory, &account, value).map_err(|e| e.to_string())?;
                Store::Memory
            };
            Ok(fallback)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Which secrets are set, and where, as `plugin/option store` lines, so the
/// Plugins tab can say so without opening the keyring. Names only, never values.
fn marks_file(data_dir: &Path) -> PathBuf {
    data_dir.join("plugin-secrets")
}

/// Secrets kept in memory are gone once riptide quits, so their marks are too.
fn marks(data_dir: &Path) -> BTreeMap<String, Store> {
    let memory: Vec<String> = MEMORY
        .lock()
        .ok()
        .and_then(|m| m.as_ref().map(|m| m.keys().cloned().collect()))
        .unwrap_or_default();
    parse_marks(&std::fs::read_to_string(marks_file(data_dir)).unwrap_or_default())
        .into_iter()
        .filter(|(key, store)| *store != Store::Memory || memory.contains(&format!("plugin/{key}")))
        .collect()
}

/// A line without a store is from before the fallback, in the keyring.
fn parse_marks(text: &str) -> BTreeMap<String, Store> {
    text.lines()
        .filter_map(|line| {
            let (key, store) = line.split_once(' ').unwrap_or((line, "keyring"));
            Some((key.to_string(), Store::parse(store)?)).filter(|(k, _)| !k.is_empty())
        })
        .collect()
}

/// Whether plugin `plugin`'s secret `option` has been set.
pub fn is_set(data_dir: &Path, plugin: &str, option: &str) -> bool {
    marks(data_dir).contains_key(&format!("{plugin}/{option}"))
}

fn kept_in(data_dir: &Path, plugin: &str, option: &str) -> Option<Store> {
    marks(data_dir).get(&format!("{plugin}/{option}")).copied()
}

fn mark(data_dir: &Path, plugin: &str, option: &str, store: Option<Store>) {
    let mut all = parse_marks(&std::fs::read_to_string(marks_file(data_dir)).unwrap_or_default());
    let key = format!("{plugin}/{option}");
    match store {
        Some(store) => all.insert(key, store),
        None => all.remove(&key),
    };
    let text: String = all
        .into_iter()
        .map(|(key, store)| format!("{key} {}\n", store.name()))
        .collect();
    let _ = std::fs::write(marks_file(data_dir), text);
}

/// The Plugins tab saved (or cleared, with `None`) a secret: into the
/// keyring on a background thread, then a message saying where it went.
pub fn store(data_dir: PathBuf, plugin: String, option: String, value: Option<String>) {
    let kept = kept_in(&data_dir, &plugin, &option);
    std::thread::spawn(move || {
        let result = write(&plugin, &option, value.as_deref(), kept);
        let set = value.is_some();
        // The value is dropped here; only the outcome goes back.
        drop(value);
        let (store, error) = match result {
            Ok(store) => (store.name().to_string(), String::new()),
            Err(e) => (String::new(), e),
        };
        let mut task = Stored::new(
            data_dir.display().to_string(),
            plugin,
            option,
            set,
            store,
            error,
        );
        post_task(ThreadId::UI, Some(&mut task));
    });
}

wrap_task! {
    struct Stored {
        data_dir: String,
        plugin: String,
        option: String,
        set: bool,
        // Where it went, as a [`Store`] name.
        store: String,
        // Why the keyring refused; empty when it didn't.
        error: String,
    }

    impl Task {
        fn execute(&self) {
            let (Some(store), true) = (Store::parse(&self.store), self.error.is_empty()) else {
                return shell::show_message(
                    Level::Error,
                    format!("Couldn't keep {}'s {} in the keyring: {}", self.plugin, self.option, self.error),
                );
            };
            let data_dir = Path::new(&self.data_dir);
            mark(data_dir, &self.plugin, &self.option, self.set.then_some(store));
            let message = if self.set {
                format!("Saved {}'s {} {}", self.plugin, self.option, store.kept())
            } else {
                format!("Cleared {}'s {}", self.plugin, self.option)
            };
            shell::show_message(Level::Info, message);
            crate::settings_page::refresh();
        }
    }
}

/// `rt.secret.get` from plugin `plugin`: the value of its secret `option`,
/// or why not, to Lua callback `callback`. `declared` says whether its
/// manifest has that option as a secret.
pub fn get(data_dir: PathBuf, plugin: String, option: String, declared: bool, callback: u32) {
    if !declared {
        let why = format!("{option} isn't one of {plugin}'s secret options");
        return answer(&plugin, callback, &serde_json::json!({ "error": why }));
    }
    let kept = kept_in(&data_dir, &plugin, &option);
    let store = kept.or_else(forced).unwrap_or(Store::Keyring);
    std::thread::spawn(move || {
        let value = read(store, &plugin, &option);
        // The kernel keyring forgets everything when the computer restarts.
        let forgotten = kept.is_some() && matches!(value, Ok(None));
        let envelope = match value {
            Ok(value) => serde_json::json!({ "ok": value }),
            Err(e) => serde_json::json!({ "error": format!("the keyring: {e}") }),
        };
        let data_dir = data_dir.display().to_string();
        let mut task = Read::new(
            plugin,
            option,
            data_dir,
            forgotten,
            callback,
            envelope.to_string(),
        );
        post_task(ThreadId::UI, Some(&mut task));
    });
}

wrap_task! {
    struct Read {
        plugin: String,
        option: String,
        data_dir: String,
        // It was saved, but its store no longer has it.
        forgotten: bool,
        callback: u32,
        // {"ok": value} or {"error": why}.
        envelope: String,
    }

    impl Task {
        fn execute(&self) {
            if self.forgotten {
                mark(Path::new(&self.data_dir), &self.plugin, &self.option, None);
                shell::show_message(
                    Level::Info,
                    format!("{}'s {} is no longer kept (the computer restarted); save it again on the Plugins tab", self.plugin, self.option),
                );
                crate::settings_page::refresh();
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_say_where_each_secret_is_and_old_ones_are_in_the_keyring() {
        let marks = parse_marks("a/token\nb/password kernel\nc/key memory\nd/x nowhere\n\n");
        assert_eq!(marks.get("a/token"), Some(&Store::Keyring));
        assert_eq!(marks.get("b/password"), Some(&Store::Kernel));
        assert_eq!(marks.get("c/key"), Some(&Store::Memory));
        assert_eq!(marks.len(), 3, "an unknown store is dropped: {marks:?}");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_kernel_keyring_keeps_and_clears_a_secret() {
        // Safety: the only test that reads RIPTIDE_SECRET_SERVICE.
        unsafe {
            std::env::set_var(
                "RIPTIDE_SECRET_SERVICE",
                format!("riptide-unit-{}", std::process::id()),
            )
        };
        let account = account("unit", "token");
        write_to(Store::Kernel, &account, Some("kept")).unwrap();
        assert_eq!(
            read(Store::Kernel, "unit", "token").unwrap().as_deref(),
            Some("kept")
        );
        write_to(Store::Kernel, &account, None).unwrap();
        assert_eq!(read(Store::Kernel, "unit", "token").unwrap(), None);
    }
}
