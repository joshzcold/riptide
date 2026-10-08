//! Plugins from `rt.pack.add`: once config.lua has run, each is loaded in
//! its sandbox, after you approve any permissions it asks for that you
//! haven't approved before.

use std::path::{Path, PathBuf};

use cef::*;
use rt_config::lua::PluginSpec;
use rt_config::plugins::{Lockfile, Manifest, Permissions, git};
use rt_core::engine::Level;
use rt_core::prompt::{PromptAnswer, PromptKind, Remember, Topic};

use crate::prompts::{self, Scope};
use crate::shell;

/// Where each plugin is at, for the plugins page.
#[derive(Clone, Debug, Default, serde::Serialize)]
struct Status {
    /// installing, waiting (for approval), refused, loaded or failed.
    state: &'static str,
    error: Option<String>,
    /// From "check for updates": the newest commit and the commits after the pinned one.
    update: Option<(String, Vec<String>)>,
}

thread_local! {
    static STATUS: std::cell::RefCell<std::collections::BTreeMap<String, Status>> =
        std::cell::RefCell::new(Default::default());
}

/// A plugin approved and waiting for one of its events, commands or keys.
struct Waiting {
    spec: PluginSpec,
    dir: PathBuf,
    permissions: Permissions,
}

thread_local! {
    static WAITING: std::cell::RefCell<Vec<Waiting>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn set_state(name: &str, state: &'static str, error: Option<String>) {
    STATUS.with(|s| {
        let mut all = s.borrow_mut();
        let status = all.entry(name.to_string()).or_default();
        status.state = state;
        status.error = error;
    });
    crate::settings_page::refresh();
}

fn paths() -> Option<(PathBuf, PathBuf)> {
    shell::with(|s| (s.paths.config_dir.clone(), s.paths.data_dir.clone()))
}

/// Where a plugin lives: its own folder, or `<data>/pack/<name>` for one from git.
fn folder(spec: &PluginSpec, data_dir: &Path) -> PathBuf {
    spec.dir
        .clone()
        .unwrap_or_else(|| data_dir.join("pack").join(&spec.name))
}

/// Load every plugin `rt.pack.add` asked for: install or move to its locked
/// commit if it comes from git, then ask about permissions, then load.
pub fn start() {
    let Some((config_dir, data_dir)) = paths() else {
        return;
    };
    let lock = Lockfile::load(&config_dir).unwrap_or_default();
    // The config was just applied: the old placeholders went with it.
    WAITING.with(|w| w.borrow_mut().clear());
    for spec in rt_config::lua::plugin_specs() {
        let dir = folder(&spec, &data_dir);
        if spec.dir.is_some() || spec.src.is_empty() {
            prepare(&spec.name);
            continue;
        }
        let locked = lock
            .plugins
            .get(&spec.name)
            .map(|l| l.commit.clone())
            .filter(|c| !c.is_empty());
        if !dir.is_dir() {
            shell::show_message(Level::Info, format!("Installing plugin {}…", spec.name));
            set_state(&spec.name, "installing", None);
            let rev = locked.or_else(|| Some(spec.version.clone()).filter(|v| !v.is_empty()));
            let (name, src) = (spec.name.clone(), spec.src.clone());
            std::thread::spawn(move || {
                let result = git::install(&src, &dir, rev.as_deref());
                finished(name, true, result);
            });
            continue;
        }
        match (locked, git::head(&dir)) {
            (Some(locked), Ok(head)) if locked != head => {
                // The lockfile moved, e.g. synced from another computer.
                let name = spec.name.clone();
                std::thread::spawn(move || {
                    let result = git::checkout(&dir, &locked);
                    finished(name, false, result);
                });
            }
            (None, Ok(head)) => {
                record(&spec.name, &spec.src, &head);
                prepare(&spec.name);
            }
            (_, Ok(_)) => prepare(&spec.name),
            (_, Err(e)) => {
                set_state(&spec.name, "failed", Some(e.clone()));
                shell::show_message(Level::Error, format!("Plugin {}: {e}", spec.name));
            }
        }
    }
}

/// From the git thread: hand the result to the UI thread.
fn finished(name: String, installed: bool, result: Result<String, String>) {
    let (ok, text) = match result {
        Ok(commit) => (true, commit),
        Err(e) => (false, e),
    };
    let mut task = GitDone::new(name, installed, ok, text);
    post_task(ThreadId::UI, Some(&mut task));
}

wrap_task! {
    struct GitDone {
        name: String,
        installed: bool,
        ok: bool,
        // The commit, or why git failed.
        text: String,
    }

    impl Task {
        fn execute(&self) {
            if !self.ok {
                set_state(&self.name, "failed", Some(self.text.clone()));
                return shell::show_message(Level::Error, format!("Plugin {}: {}", self.name, self.text));
            }
            let src = rt_config::lua::plugin_specs()
                .into_iter()
                .find(|s| s.name == self.name)
                .map(|s| s.src)
                .unwrap_or_default();
            record(&self.name, &src, &self.text);
            if self.installed {
                let short: String = self.text.chars().take(8).collect();
                shell::show_message(Level::Info, format!("Installed plugin {} ({short})", self.name));
            }
            prepare(&self.name);
        }
    }
}

/// Pin `name` to `commit` in the lockfile.
fn record(name: &str, src: &str, commit: &str) {
    let Some((config_dir, _)) = paths() else {
        return;
    };
    let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
        let entry = lock.plugins.entry(name.to_string()).or_default();
        entry.src = src.to_string();
        entry.commit = commit.to_string();
        lock.save(&config_dir)
    });
    if let Err(e) = saved {
        shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
    }
}

/// The plugin is on disk: load it, asking about new permissions first.
fn prepare(name: &str) {
    let Some((config_dir, data_dir)) = paths() else {
        return;
    };
    let Some(spec) = rt_config::lua::plugin_specs()
        .into_iter()
        .find(|s| s.name == name)
    else {
        return;
    };
    let dir = folder(&spec, &data_dir);
    if !dir.is_dir() {
        return shell::show_message(
            Level::Error,
            format!("Plugin {}: {} doesn't exist", spec.name, dir.display()),
        );
    }
    let manifest = match Manifest::read(&dir) {
        Ok(manifest) => manifest,
        Err(e) => return shell::show_message(Level::Error, format!("Plugin {}: {e}", spec.name)),
    };
    let wanted = manifest.permissions;
    let approved = Lockfile::load(&config_dir)
        .ok()
        .and_then(|lock| lock.plugins.get(&spec.name).map(|l| l.approved.clone()))
        .unwrap_or_default();
    let needed = wanted.beyond(&approved);
    if spec.trusted || needed.is_empty() {
        ready(spec, dir, wanted);
    } else {
        set_state(&spec.name, "waiting", None);
        ask(spec.name, dir, wanted, needed, config_dir);
    }
}

/// Ask whether `name` may do what it asks; yes saves that and loads it.
fn ask(name: String, dir: PathBuf, wanted: Permissions, needed: Permissions, config_dir: PathBuf) {
    let list: String = needed
        .describe()
        .iter()
        .map(|line| format!("\n  • {line}"))
        .collect();
    let message = format!("The plugin {name} asks to:{list}\nAllow it?");
    let kind = PromptKind::YesNo {
        default: false,
        remember: Remember::Never,
    };
    prompts::ask(
        None,
        Scope::Other,
        Topic::Confirm,
        "Plugin permissions",
        message,
        kind,
        move |answer| {
            if !matches!(answer, PromptAnswer::Yes { .. }) {
                shell::show_message(
                    Level::Warning,
                    format!("Didn't load {name}: its permissions weren't approved"),
                );
                set_state(&name, "refused", None);
                return;
            }
            let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
                lock.plugins.entry(name.clone()).or_default().approved = wanted.clone();
                lock.save(&config_dir)
            });
            if let Err(e) = saved {
                shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
            }
            if let Some(spec) = rt_config::lua::plugin_specs()
                .into_iter()
                .find(|s| s.name == name)
            {
                ready(spec, dir, wanted);
            }
        },
    );
}

/// Approved: load it now, or wait for its event, command or keys.
fn ready(spec: PluginSpec, dir: PathBuf, permissions: Permissions) {
    if !spec.is_lazy() {
        return load(&spec.name, &dir, &permissions, spec.trusted);
    }
    let name = spec.name.clone();
    for (mode, keys) in &spec.keys {
        let command = format!("pack-load {name} {keys}");
        if let Some(Err(e)) = shell::with(|s| s.engine.bind_from_lua(*mode, keys, &command)) {
            shell::show_message(Level::Error, format!("Plugin {name}: {keys}: {e}"));
        }
    }
    WAITING.with(|w| {
        w.borrow_mut().push(Waiting {
            spec,
            dir,
            permissions,
        })
    });
    set_state(&name, "lazy", None);
    refresh_commands();
}

/// Load `name` if it's waiting. True if it was.
fn wake(name: &str) -> bool {
    let Some(waiting) = WAITING.with(|w| {
        let mut all = w.borrow_mut();
        let at = all.iter().position(|p| p.spec.name == name)?;
        Some(all.remove(at))
    }) else {
        return false;
    };
    // The plugin binds its own keys as it loads.
    for (mode, keys) in &waiting.spec.keys {
        let _ = shell::with(|s| s.engine.unbind_from_lua(*mode, keys));
    }
    load(
        name,
        &waiting.dir,
        &waiting.permissions,
        waiting.spec.trusted,
    );
    true
}

/// The Plugins tab's "Load now".
pub fn load_now(name: &str) {
    if !wake(name) {
        shell::show_message(Level::Error, format!("Plugin {name} isn't waiting to load"));
    }
}

/// Before an event's hooks run: load the plugins waiting for it.
pub fn on_event(event: &str) {
    let names: Vec<String> = WAITING.with(|w| {
        w.borrow()
            .iter()
            .filter(|p| p.spec.events.iter().any(|e| e == event))
            .map(|p| p.spec.name.clone())
            .collect()
    });
    for name in names {
        wake(&name);
    }
}

/// Before a command from Lua runs: load the plugin waiting for it.
pub fn on_command(command: &str) {
    let name = WAITING.with(|w| {
        w.borrow()
            .iter()
            .find(|p| p.spec.commands.iter().any(|c| c == command))
            .map(|p| p.spec.name.clone())
    });
    if let Some(name) = name {
        wake(&name);
    }
}

/// The commands from Lua, and the ones waiting plugins will define.
fn refresh_commands() {
    let mut commands = rt_config::lua::user_commands();
    WAITING.with(|w| {
        for waiting in w.borrow().iter() {
            for command in &waiting.spec.commands {
                if !commands.iter().any(|(n, _)| n == command) {
                    let desc = format!("Loads the plugin {}", waiting.spec.name);
                    commands.push((command.clone(), desc));
                }
            }
        }
    });
    shell::with(|s| s.engine.set_user_commands(commands));
}

pub fn run_command(command: &rt_core::Command) -> bool {
    match command {
        rt_core::Command::PackUpdate { name } => check_updates(name.as_deref()),
        rt_core::Command::PackLoad { name, keys } => {
            if !wake(name) {
                let loaded =
                    STATUS.with(|s| s.borrow().get(name).is_some_and(|st| st.state == "loaded"));
                if !loaded || keys.is_none() {
                    shell::show_message(
                        Level::Error,
                        format!("Plugin {name} isn't waiting to load"),
                    );
                    return true;
                }
            }
            // A key loaded it: press it again, now that the plugin has bound it.
            let replay = keys
                .as_deref()
                .and_then(|k| rt_core::key::Key::parse_sequence(k).ok());
            if let Some(effects) =
                replay.and_then(|keys| shell::with(|s| s.engine.replay_keys(&keys)))
            {
                shell::apply(effects);
            }
        }
        _ => return false,
    }
    true
}

fn load(name: &str, dir: &Path, permissions: &Permissions, trusted: bool) {
    let context = crate::lua::current_context();
    let result = rt_config::lua::load_plugin(name, dir, permissions, trusted, &context);
    match &result {
        Ok(_) => set_state(name, "loaded", None),
        Err(e) => set_state(name, "failed", Some(e.clone())),
    }
    crate::lua::carry_out_for(name, result);
    // Commands it defined complete and run like the others.
    refresh_commands();
    shell::refresh_ui();
}

/// What makes a lazy plugin load, in words: `:cmd`, `<Space>p`, `tab_opened`.
fn triggers(spec: &PluginSpec) -> Vec<String> {
    let commands = spec.commands.iter().map(|c| format!(":{c}"));
    let keys = spec.keys.iter().map(|(mode, keys)| match mode {
        rt_core::Mode::Normal => keys.clone(),
        mode => format!("{keys} ({mode})"),
    });
    commands
        .chain(keys)
        .chain(spec.events.iter().cloned())
        .collect()
}

/// Every plugin for the plugins page: its spec, lockfile entry and status.
pub fn page_data() -> serde_json::Value {
    let Some((config_dir, data_dir)) = paths() else {
        return serde_json::json!([]);
    };
    let lock = Lockfile::load(&config_dir).unwrap_or_default();
    let status = STATUS.with(|s| s.borrow().clone());
    rt_config::lua::plugin_specs()
        .into_iter()
        .map(|spec| {
            let locked = lock.plugins.get(&spec.name).cloned().unwrap_or_default();
            let state = status.get(&spec.name).cloned().unwrap_or_default();
            let description = Manifest::read(&folder(&spec, &data_dir))
                .ok()
                .and_then(|m| m.description)
                .unwrap_or_default();
            serde_json::json!({
                "name": spec.name,
                "description": description,
                "src": if spec.src.is_empty() {
                    spec.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default()
                } else {
                    spec.src.clone()
                },
                "git": spec.dir.is_none() && !spec.src.is_empty(),
                "commit": locked.commit,
                "trusted": spec.trusted,
                "approved": locked.approved.describe(),
                "triggers": triggers(&spec),
                "state": if state.state.is_empty() { "pending" } else { state.state },
                "error": state.error,
                "update": state.update.map(|(commit, log)| serde_json::json!({ "commit": commit, "log": log })),
            })
        })
        .collect()
}

/// Fetch `name` (or every plugin from git) and note what's new; changes nothing.
pub fn check_updates(name: Option<&str>) {
    let Some((_, data_dir)) = paths() else { return };
    let specs: Vec<PluginSpec> = rt_config::lua::plugin_specs()
        .into_iter()
        .filter(|s| s.dir.is_none() && !s.src.is_empty())
        .filter(|s| name.is_none_or(|n| n == s.name))
        .collect();
    if specs.is_empty() {
        return shell::show_message(Level::Info, "No plugins from git to update");
    }
    shell::show_message(
        Level::Info,
        format!("Checking {} plugin(s) for updates…", specs.len()),
    );
    for spec in specs {
        let dir = folder(&spec, &data_dir);
        std::thread::spawn(move || {
            let result = git::head(&dir).and_then(|head| {
                let latest = git::fetch_latest(&dir)?;
                let log = git::log(&dir, &head, &latest)?;
                Ok((latest, log))
            });
            let (latest, log) = match result {
                Ok((latest, log)) => (latest, log.join("\n")),
                Err(e) => (String::new(), format!("error: {e}")),
            };
            let mut task = UpdateChecked::new(spec.name, latest, log);
            post_task(ThreadId::UI, Some(&mut task));
        });
    }
}

wrap_task! {
    struct UpdateChecked {
        name: String,
        // The newest commit; empty if checking failed.
        latest: String,
        // The new commits, one per line, or "error: …".
        log: String,
    }

    impl Task {
        fn execute(&self) {
            if self.latest.is_empty() {
                shell::show_message(Level::Error, format!("Plugin {}: {}", self.name, self.log.trim_start_matches("error: ")));
                return;
            }
            let log: Vec<String> = self.log.lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
            let text = if log.is_empty() {
                format!("Plugin {} is up to date", self.name)
            } else {
                format!("Plugin {} has {} new commit(s); review them on :plugins", self.name, log.len())
            };
            STATUS.with(|s| {
                let mut all = s.borrow_mut();
                let status = all.entry(self.name.clone()).or_default();
                status.update = (!log.is_empty()).then(|| (self.latest.clone(), log));
            });
            shell::show_message(Level::Info, text);
            crate::settings_page::refresh();
        }
    }
}

/// Move `name` to the update found by [`check_updates`], then reload, which
/// asks about any new permissions.
pub fn apply_update(name: &str) {
    let Some((_, data_dir)) = paths() else { return };
    let Some(update) = STATUS.with(|s| s.borrow().get(name).and_then(|st| st.update.clone()))
    else {
        return shell::show_message(Level::Error, format!("Check {name} for updates first"));
    };
    let Some(spec) = rt_config::lua::plugin_specs()
        .into_iter()
        .find(|s| s.name == name)
    else {
        return;
    };
    match git::checkout(&folder(&spec, &data_dir), &update.0) {
        Ok(commit) => {
            record(&spec.name, &spec.src, &commit);
            STATUS.with(|s| {
                if let Some(status) = s.borrow_mut().get_mut(name) {
                    status.update = None;
                }
            });
            shell::show_message(Level::Info, format!("Updated {name}; reloading the config"));
            reload();
        }
        Err(e) => shell::show_message(Level::Error, format!("Plugin {name}: {e}")),
    }
}

/// Forget `name`'s approvals, so it asks again when the config reloads.
pub fn revoke(name: &str) {
    let Some((config_dir, _)) = paths() else {
        return;
    };
    let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
        if let Some(entry) = lock.plugins.get_mut(name) {
            entry.approved = Permissions::default();
        }
        lock.save(&config_dir)
    });
    match saved {
        Ok(()) => {
            shell::show_message(
                Level::Info,
                format!("Revoked {name}'s permissions; reloading the config"),
            );
            reload();
        }
        Err(e) => shell::show_message(Level::Error, format!("Plugin {name}: {e}")),
    }
}

/// Delete `name`'s installed copy and its lockfile entry. Only a copy riptide
/// installed (under `<data>/pack`) is deleted; it comes back at the next
/// start unless it's also taken out of config.lua.
pub fn remove(name: &str) {
    let Some((config_dir, data_dir)) = paths() else {
        return;
    };
    let Some(spec) = rt_config::lua::plugin_specs()
        .into_iter()
        .find(|s| s.name == name)
    else {
        return;
    };
    if spec.dir.is_none() {
        let dir = folder(&spec, &data_dir);
        if dir.starts_with(data_dir.join("pack"))
            && dir.is_dir()
            && let Err(e) = std::fs::remove_dir_all(&dir)
        {
            return shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
        }
    }
    let _ = Lockfile::load(&config_dir).and_then(|mut lock| {
        lock.plugins.remove(name);
        lock.save(&config_dir)
    });
    STATUS.with(|s| s.borrow_mut().remove(name));
    shell::show_message(
        Level::Info,
        format!(
            "Removed {name}; take it out of config.lua too, or it's installed again at the next start"
        ),
    );
    crate::settings_page::refresh();
}

fn reload() {
    if let Some(effects) = shell::with(|s| s.engine.execute_str("config-source", None)) {
        shell::apply(effects);
    }
}
