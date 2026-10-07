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
            (_, Err(e)) => shell::show_message(Level::Error, format!("Plugin {}: {e}", spec.name)),
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
        load(&spec.name, &dir, &wanted, spec.trusted);
    } else {
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
                return;
            }
            let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
                lock.plugins.entry(name.clone()).or_default().approved = wanted.clone();
                lock.save(&config_dir)
            });
            if let Err(e) = saved {
                shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
            }
            load(&name, &dir, &wanted, false);
        },
    );
}

fn load(name: &str, dir: &Path, permissions: &Permissions, trusted: bool) {
    let context = crate::lua::current_context();
    let result = rt_config::lua::load_plugin(name, dir, permissions, trusted, &context);
    crate::lua::carry_out_for(name, result);
    // Commands it defined complete and run like the others.
    let commands = rt_config::lua::user_commands();
    shell::with(|s| s.engine.set_user_commands(commands));
    shell::refresh_ui();
}
