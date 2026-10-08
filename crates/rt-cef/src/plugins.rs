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
    dependencies: Vec<String>,
}

/// An approved plugin held back until the plugins it depends on have loaded.
struct Blocked {
    name: String,
    dir: PathBuf,
    permissions: Permissions,
    trusted: bool,
    dependencies: Vec<String>,
}

thread_local! {
    static BLOCKED: std::cell::RefCell<Vec<Blocked>> = const { std::cell::RefCell::new(Vec::new()) };
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
    // Plugins waiting for this one won't load now.
    if state == "failed" {
        retry_blocked();
    }
}

fn paths() -> Option<(PathBuf, PathBuf)> {
    shell::with(|s| (s.paths.config_dir.clone(), s.paths.data_dir.clone()))
}

/// Where a plugin lives: its own folder, or its `subdir` of `<data>/pack/<name>` for one from git.
fn folder(spec: &PluginSpec, data_dir: &Path) -> PathBuf {
    match &spec.dir {
        Some(dir) => dir.clone(),
        None => repo_folder(spec, data_dir).join(&spec.subdir),
    }
}

/// The git checkout a plugin from git is in: one per repository, shared by
/// every plugin from it, at `<data>/pack/<repo>-<hash of its URL>`.
fn repo_folder(spec: &PluginSpec, data_dir: &Path) -> PathBuf {
    match &spec.dir {
        Some(dir) => dir.clone(),
        None => data_dir.join("pack").join(repo_key(&spec.src)),
    }
}

/// A checkout's folder name: the repository's name, and a hash of its URL so
/// two repositories with the same name don't share one.
fn repo_key(src: &str) -> String {
    let hash = src.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{}-{:08x}", rt_config::plugins::name_from(src), hash as u32)
}

/// The plugins from git, by repository URL, in config order.
fn repositories() -> Vec<(String, Vec<PluginSpec>)> {
    let mut repos: Vec<(String, Vec<PluginSpec>)> = Vec::new();
    for spec in rt_config::lua::plugin_specs() {
        if spec.dir.is_some() || spec.src.is_empty() {
            continue;
        }
        match repos.iter_mut().find(|(src, _)| *src == spec.src) {
            Some((_, members)) => members.push(spec),
            None => repos.push((spec.src.clone(), vec![spec])),
        }
    }
    repos
}

/// The repository URL plugin `name` comes from, if it's from git.
fn repo_of(name: &str) -> Option<String> {
    rt_config::lua::plugin_specs()
        .into_iter()
        .find(|s| s.name == name && s.dir.is_none() && !s.src.is_empty())
        .map(|s| s.src)
}

/// The names of the plugins from repository `src`.
fn members_of(src: &str) -> Vec<String> {
    rt_config::lua::plugin_specs()
        .into_iter()
        .filter(|s| s.dir.is_none() && s.src == src)
        .map(|s| s.name)
        .collect()
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
    crate::pages::unregister_all();
    BLOCKED.with(|b| b.borrow_mut().clear());
    for spec in rt_config::lua::plugin_specs() {
        if spec.dir.is_some() || spec.src.is_empty() {
            prepare(&spec.name);
        }
    }
    for (src, members) in repositories() {
        begin(&src, &members, &lock, &data_dir);
    }
    // Once installs started now have had time to finish.
    if !CHECKED.replace(true) {
        let mut task = BackgroundCheck::new();
        post_delayed_task(ThreadId::UI, Some(&mut task), 5_000);
    }
}

wrap_task! {
    struct BackgroundCheck {}

    impl Task {
        fn execute(&self) {
            maybe_check();
        }
    }
}

/// Install repository `src` or move it to its locked commit if need be, then
/// prepare `members`, the plugins from it.
fn begin(src: &str, members: &[PluginSpec], lock: &Lockfile, data_dir: &Path) {
    let Some(first) = members.first() else { return };
    let dir = repo_folder(first, data_dir);
    let names: Vec<String> = members.iter().map(|m| m.name.clone()).collect();
    // One checkout, so one commit; plugins from it agree in the lockfile.
    let locked = names.iter().find_map(|n| {
        lock.plugins
            .get(n)
            .map(|l| l.commit.clone())
            .filter(|c| !c.is_empty())
    });
    let version = members
        .iter()
        .map(|m| m.version.clone())
        .find(|v| !v.is_empty());
    if !dir.is_dir() {
        shell::show_message(
            Level::Info,
            format!("Installing plugin {}…", names.join(", ")),
        );
        for name in &names {
            set_state(name, "installing", None);
        }
        let rev = locked.or(version);
        let src = src.to_string();
        std::thread::spawn(move || {
            let result = git::install(&src, &dir, rev.as_deref());
            finished(src, names, true, result);
        });
        return;
    }
    match (locked, git::head(&dir)) {
        (Some(locked), Ok(head)) if locked != head => {
            // The lockfile moved, e.g. synced from another computer.
            let src = src.to_string();
            std::thread::spawn(move || {
                let result = git::checkout(&dir, &locked);
                finished(src, names, false, result);
            });
        }
        (_, Ok(head)) => {
            record(src, &names, &head);
            for name in &names {
                prepare(name);
            }
        }
        (_, Err(e)) => {
            for name in &names {
                set_state(name, "failed", Some(e.clone()));
            }
            shell::show_message(Level::Error, format!("Plugin {}: {e}", names.join(", ")));
        }
    }
}

/// From the git thread: hand the result to the UI thread.
fn finished(src: String, names: Vec<String>, installed: bool, result: Result<String, String>) {
    let (ok, text) = match result {
        Ok(commit) => (true, commit),
        Err(e) => (false, e),
    };
    let mut task = GitDone::new(src, names.join("\n"), installed, ok, text);
    post_task(ThreadId::UI, Some(&mut task));
}

wrap_task! {
    struct GitDone {
        src: String,
        // The plugins from it, one per line.
        names: String,
        installed: bool,
        ok: bool,
        // The commit, or why git failed.
        text: String,
    }

    impl Task {
        fn execute(&self) {
            let names: Vec<String> = self.names.lines().map(str::to_string).collect();
            if !self.ok {
                for name in &names {
                    set_state(name, "failed", Some(self.text.clone()));
                }
                return shell::show_message(Level::Error, format!("Plugin {}: {}", names.join(", "), self.text));
            }
            record(&self.src, &names, &self.text);
            if self.installed {
                let short: String = self.text.chars().take(8).collect();
                shell::show_message(Level::Info, format!("Installed plugin {} ({short})", names.join(", ")));
            }
            for name in &names {
                prepare(name);
            }
        }
    }
}

/// Pin `names`, the plugins from repository `src`, to `commit` in the lockfile.
fn record(src: &str, names: &[String], commit: &str) {
    let Some((config_dir, _)) = paths() else {
        return;
    };
    let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
        for name in names {
            let entry = lock.plugins.entry(name.clone()).or_default();
            entry.src = src.to_string();
            entry.commit = commit.to_string();
        }
        lock.save(&config_dir)
    });
    if let Err(e) = saved {
        shell::show_message(Level::Error, format!("Plugin {}: {e}", names.join(", ")));
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
    let failed = |why: String| {
        set_state(&spec.name, "failed", Some(why.clone()));
        shell::show_message(Level::Error, format!("Plugin {}: {why}", spec.name));
    };
    if !dir.is_dir() {
        return failed(format!("{} doesn't exist", dir.display()));
    }
    let manifest = match Manifest::read(&dir) {
        Ok(manifest) => manifest,
        Err(e) => return failed(e.to_string()),
    };
    let dependencies = manifest.dependencies;
    for dependency in &dependencies {
        if let Err(why) = add_dependency(&spec, dependency, &config_dir, &data_dir) {
            return failed(why);
        }
    }
    let wanted = manifest.permissions;
    let approved = Lockfile::load(&config_dir)
        .ok()
        .and_then(|lock| lock.plugins.get(&spec.name).map(|l| l.approved.clone()))
        .unwrap_or_default();
    let needed = wanted.beyond(&approved);
    if spec.trusted || needed.is_empty() {
        ready(spec, dir, wanted, dependencies);
    } else {
        set_state(&spec.name, "waiting", None);
        ask(spec.name, dir, wanted, needed, config_dir, dependencies);
    }
}

/// Ask whether `name` may do what it asks; yes saves that and loads it.
fn ask(
    name: String,
    dir: PathBuf,
    wanted: Permissions,
    needed: Permissions,
    config_dir: PathBuf,
    dependencies: Vec<String>,
) {
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
                // Plugins that needed it won't load either.
                retry_blocked();
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
                ready(spec, dir, wanted, dependencies);
            }
        },
    );
}

/// Approved: load it now, or wait for its event, command or keys.
fn ready(spec: PluginSpec, dir: PathBuf, permissions: Permissions, dependencies: Vec<String>) {
    if !spec.is_lazy() {
        return load(&spec.name, &dir, &permissions, spec.trusted, dependencies);
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
            dependencies,
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
        waiting.dependencies,
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
pub fn refresh_commands() {
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
    shell::with(|s| {
        if s.engine.user_commands() != commands.as_slice() {
            s.engine.set_user_commands(commands);
        }
    });
}

pub fn run_command(command: &rt_core::Command) -> bool {
    match command {
        rt_core::Command::PackCheck { name } => check_updates(name.as_deref()),
        rt_core::Command::PackUpdate { name } => update(name.as_deref()),
        rt_core::Command::PackSync => sync(),
        rt_core::Command::PackClean => clean(),
        rt_core::Command::PackRestore => restore(),
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

/// A plugin `spec` depends on that config.lua didn't add: the folder of that
/// name beside it, in its git repository or on disk, installed and loaded
/// like the others.
fn add_dependency(
    spec: &PluginSpec,
    name: &str,
    config_dir: &Path,
    data_dir: &Path,
) -> Result<(), String> {
    if rt_config::lua::plugin_specs()
        .iter()
        .any(|s| s.name == name)
    {
        return Ok(());
    }
    let needs = format!("needs the plugin {name}; add it with rt.pack.add");
    let dependency = if let Some(dir) = &spec.dir {
        let sibling = dir
            .parent()
            .map(|p| p.join(name))
            .filter(|d| d.is_dir())
            .ok_or(needs)?;
        PluginSpec {
            name: name.to_string(),
            dir: Some(sibling),
            ..PluginSpec::default()
        }
    } else if !spec.src.is_empty() && !spec.subdir.is_empty() {
        let subdir = Path::new(&spec.subdir).with_file_name(name);
        PluginSpec {
            name: name.to_string(),
            src: spec.src.clone(),
            subdir: subdir.to_string_lossy().into_owned(),
            version: spec.version.clone(),
            ..PluginSpec::default()
        }
    } else {
        return Err(needs);
    };
    if rt_config::lua::add_plugin_spec(dependency.clone()) {
        let lock = Lockfile::load(config_dir).unwrap_or_default();
        if dependency.dir.is_some() {
            prepare(&dependency.name);
        } else {
            begin(&dependency.src.clone(), &[dependency], &lock, data_dir);
        }
    }
    Ok(())
}

/// Whether `name`'s dependencies have loaded: `Ok(true)` yes, `Ok(false)`
/// not yet, `Err` one never will.
fn dependencies_loaded(name: &str, dependencies: &[String]) -> Result<bool, String> {
    let mut all = true;
    for dependency in dependencies {
        // A dependency waiting for its event or keys loads now.
        wake(dependency);
        let state = STATUS.with(|s| s.borrow().get(dependency).map(|st| st.state));
        match state {
            Some("loaded") => {}
            Some("failed" | "refused") => {
                return Err(format!("needs the plugin {dependency}, which didn't load"));
            }
            _ => {
                let circular = BLOCKED.with(|b| {
                    b.borrow()
                        .iter()
                        .any(|p| &p.name == dependency && p.dependencies.iter().any(|d| d == name))
                });
                if circular {
                    return Err(format!(
                        "it and the plugin {dependency} depend on each other"
                    ));
                }
                all = false;
            }
        }
    }
    Ok(all)
}

/// Load the plugins that were waiting for others which have now loaded, and
/// fail the ones whose dependencies never will.
fn retry_blocked() {
    let blocked = BLOCKED.with(|b| std::mem::take(&mut *b.borrow_mut()));
    for p in blocked {
        load(&p.name, &p.dir, &p.permissions, p.trusted, p.dependencies);
    }
}

fn load(
    name: &str,
    dir: &Path,
    permissions: &Permissions,
    trusted: bool,
    dependencies: Vec<String>,
) {
    match dependencies_loaded(name, &dependencies) {
        Ok(true) => {}
        Ok(false) => {
            let waits = format!("waits for {}", dependencies.join(", "));
            BLOCKED.with(|b| {
                b.borrow_mut().push(Blocked {
                    name: name.to_string(),
                    dir: dir.to_path_buf(),
                    permissions: permissions.clone(),
                    trusted,
                    dependencies,
                })
            });
            return set_state(name, "blocked", Some(waits));
        }
        Err(why) => {
            shell::show_message(Level::Error, format!("Plugin {name}: {why}"));
            return set_state(name, "failed", Some(why));
        }
    }
    let context = crate::lua::current_context();
    let result =
        rt_config::lua::load_plugin(name, dir, permissions, trusted, &dependencies, &context);
    match &result {
        Ok(_) => {
            crate::pages::register(name, dir, permissions, trusted);
            set_state(name, "loaded", None);
        }
        Err(e) => set_state(name, "failed", Some(e.clone())),
    }
    crate::lua::carry_out_for(name, result);
    // Commands it defined complete and run like the others.
    refresh_commands();
    shell::refresh_ui();
    retry_blocked();
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
/// The most of a plugin's README the help page shows.
const README_LIMIT: usize = 200_000;

/// Each plugin for `:help <plugin>`: what it is, what it may do, and its
/// `doc/<name>.md` or README.
pub fn help_entries() -> Vec<serde_json::Value> {
    let Some((_, data_dir)) = paths() else {
        return Vec::new();
    };
    rt_config::lua::plugin_specs()
        .into_iter()
        .map(|spec| {
            let dir = folder(&spec, &data_dir);
            let manifest = Manifest::read(&dir).ok();
            let readme = [
                dir.join("doc").join(format!("{}.md", spec.name)),
                dir.join("README.md"),
                dir.join("README"),
            ]
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .map(|mut text| {
                if text.len() > README_LIMIT {
                    let mut end = README_LIMIT;
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    text.truncate(end);
                }
                text
            })
            .unwrap_or_default();
            serde_json::json!({
                "name": spec.name,
                "description": manifest.as_ref().and_then(|m| m.description.clone()).unwrap_or_default(),
                "permissions": manifest.map(|m| m.permissions.describe()).unwrap_or_default(),
                "trusted": spec.trusted,
                "readme": readme,
            })
        })
        .collect()
}

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
                } else if spec.subdir.is_empty() {
                    spec.src.clone()
                } else {
                    format!("{} ({})", spec.src, spec.subdir)
                },
                "git": spec.dir.is_none() && !spec.src.is_empty(),
                // The plugins it shares a checkout with, which update with it.
                "shares": if spec.dir.is_none() && !spec.src.is_empty() {
                    members_of(&spec.src).into_iter().filter(|n| *n != spec.name).collect()
                } else {
                    Vec::new()
                },
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

/// What to do with a repository after fetching it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    /// List its new commits for review.
    Check,
    /// Move to its newest commit.
    Update,
}

thread_local! {
    /// Repositories still being fetched, and whether any moved, for one
    /// reload once the last is done.
    static FETCHING: std::cell::Cell<(usize, bool)> = const { std::cell::Cell::new((0, false)) };
    /// A background check (`plugins.check_interval`) is running: one message
    /// at the end naming the plugins with updates, instead of one each.
    static QUIET: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    /// The background check is due once per run of riptide, not per reload.
    static CHECKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// When it last checked in the background, as seconds since the epoch.
fn last_check_file(data_dir: &Path) -> PathBuf {
    data_dir.join("pack-last-check")
}

/// At startup: check for updates in the background if `plugins.check_interval`
/// days have passed since the last time.
fn maybe_check() {
    let Some((_, data_dir)) = paths() else { return };
    let days = shell::with(|s| s.engine.settings().int("plugins.check_interval")).unwrap_or(0);
    if days <= 0 || repositories().is_empty() {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // The first run starts the clock: what was just installed is current.
    let Some(last) = std::fs::read_to_string(last_check_file(&data_dir))
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok())
    else {
        let _ = std::fs::write(last_check_file(&data_dir), now.to_string());
        return;
    };
    if now.saturating_sub(last) < days as u64 * 86_400 {
        return;
    }
    let _ = std::fs::write(last_check_file(&data_dir), now.to_string());
    QUIET.with(|q| *q.borrow_mut() = Some(Vec::new()));
    fetch(None, After::Check);
}

/// The repositories `name` (or every plugin from git) comes from.
fn repos_for(name: Option<&str>) -> Vec<(String, Vec<PluginSpec>)> {
    let wanted = name.map(repo_of);
    repositories()
        .into_iter()
        .filter(|(src, _)| {
            wanted
                .as_ref()
                .is_none_or(|w| w.as_deref() == Some(src.as_str()))
        })
        .collect()
}

/// `:pack-check`: fetch and list what's new; changes nothing.
pub fn check_updates(name: Option<&str>) {
    fetch(name, After::Check);
}

/// `:pack-update`: fetch and move to the newest commit, then reload once,
/// which asks about any new permissions.
pub fn update(name: Option<&str>) {
    fetch(name, After::Update);
}

fn fetch(name: Option<&str>, after: After) {
    let Some((_, data_dir)) = paths() else { return };
    // What isn't installed yet installs when the config loads, not here.
    let repos: Vec<_> = repos_for(name)
        .into_iter()
        .filter(|(_, members)| repo_folder(&members[0], &data_dir).is_dir())
        .collect();
    if repos.is_empty() {
        // A background check with nothing installed says nothing.
        if QUIET.with(|q| q.borrow_mut().take()).is_some() {
            return;
        }
        if FETCHING.with(|f| f.get()) == (0, true) {
            // :pack-sync with nothing installed yet.
            FETCHING.with(|f| f.set((0, false)));
            return reload();
        }
        let text = match name {
            Some(name) => format!("Plugin {name} isn't from git"),
            None => "No plugins from git".to_string(),
        };
        return shell::show_message(Level::Info, text);
    }
    let verb = if after == After::Check {
        "Checking"
    } else {
        "Updating"
    };
    let names: Vec<String> = repos
        .iter()
        .flat_map(|(_, m)| m.iter().map(|s| s.name.clone()))
        .collect();
    if QUIET.with(|q| q.borrow().is_none()) {
        shell::show_message(Level::Info, format!("{verb} {}…", names.join(", ")));
    }
    FETCHING.with(|f| {
        let (pending, moved) = f.get();
        f.set((pending + repos.len(), moved));
    });
    for (src, members) in repos {
        let dir = repo_folder(&members[0], &data_dir);
        let names: Vec<String> = members.iter().map(|m| m.name.clone()).collect();
        std::thread::spawn(move || {
            let result = git::head(&dir).and_then(|head| {
                let latest = git::fetch_latest(&dir)?;
                let log = git::log(&dir, &head, &latest)?;
                if after == After::Update && !log.is_empty() {
                    git::checkout(&dir, &latest)?;
                }
                Ok((latest, log))
            });
            let (latest, log) = match result {
                Ok((latest, log)) => (latest, log.join("\n")),
                Err(e) => (String::new(), format!("error: {e}")),
            };
            let mut task = Fetched::new(src, names.join("\n"), after == After::Update, latest, log);
            post_task(ThreadId::UI, Some(&mut task));
        });
    }
}

wrap_task! {
    struct Fetched {
        src: String,
        // The plugins from it, one per line.
        names: String,
        // Moved to `latest` already.
        updated: bool,
        // The newest commit; empty if fetching failed.
        latest: String,
        // The new commits, one per line, or "error: …".
        log: String,
    }

    impl Task {
        fn execute(&self) {
            let names: Vec<String> = self.names.lines().map(str::to_string).collect();
            let label = names.join(", ");
            let log: Vec<String> = self.log.lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
            let moved = !self.latest.is_empty() && self.updated && !log.is_empty();
            let quiet = QUIET.with(|q| {
                let mut q = q.borrow_mut();
                let found = q.as_mut()?;
                if !log.is_empty() {
                    found.extend(names.iter().cloned());
                }
                Some(())
            });
            if quiet.is_some() {
                if self.latest.is_empty() {
                    tracing::warn!("background check of {label}: {}", self.log);
                }
            } else if self.latest.is_empty() {
                shell::show_message(Level::Error, format!("Plugin {label}: {}", self.log.trim_start_matches("error: ")));
            } else if moved {
                record(&self.src, &names, &self.latest);
                shell::show_message(Level::Info, format!("Updated {label}: {} new commit(s)", log.len()));
            } else if log.is_empty() {
                shell::show_message(Level::Info, format!("{label}: up to date"));
            } else {
                shell::show_message(Level::Info, format!("{label}: {} new commit(s); review them on :plugins", log.len()));
            }
            // Every plugin from the repository shows the same update.
            STATUS.with(|s| {
                let mut all = s.borrow_mut();
                for name in &names {
                    let status = all.entry(name.clone()).or_default();
                    status.update = (!self.updated && !log.is_empty()).then(|| (self.latest.clone(), log.clone()));
                }
            });
            crate::settings_page::refresh();
            let (pending, any) = FETCHING.with(|f| {
                let (pending, any) = f.get();
                let next = (pending.saturating_sub(1), any || moved);
                f.set(next);
                next
            });
            if pending == 0 {
                FETCHING.with(|f| f.set((0, false)));
                if let Some(found) = QUIET.with(|q| q.borrow_mut().take())
                    && !found.is_empty()
                {
                    shell::show_message(
                        Level::Info,
                        format!("Plugin updates for {}; review them on :plugins", found.join(", ")),
                    );
                }
                if any {
                    reload();
                }
            }
        }
    }
}

/// The Plugins tab's Update: move `name`'s repository to the commit its check
/// listed (the one reviewed, not whatever is newest now), then reload.
pub fn apply_update(name: &str) {
    if apply_reviewed(name) {
        reload();
    }
}

/// The Plugins tab's Update all: every reviewed update, then one reload.
pub fn apply_all_updates() {
    let names: Vec<String> = STATUS.with(|s| {
        s.borrow()
            .iter()
            .filter(|(_, st)| st.update.is_some())
            .map(|(n, _)| n.clone())
            .collect()
    });
    let mut moved = false;
    for name in names {
        // A repository's other plugins were moved with the first.
        if STATUS.with(|s| s.borrow().get(&name).is_some_and(|st| st.update.is_some())) {
            moved |= apply_reviewed(&name);
        }
    }
    if moved {
        reload();
    } else {
        shell::show_message(Level::Info, "No updates to apply; check for updates first");
    }
}

fn apply_reviewed(name: &str) -> bool {
    let Some((_, data_dir)) = paths() else {
        return false;
    };
    let Some(update) = STATUS.with(|s| s.borrow().get(name).and_then(|st| st.update.clone()))
    else {
        shell::show_message(Level::Error, format!("Check {name} for updates first"));
        return false;
    };
    let Some(spec) = rt_config::lua::plugin_specs()
        .into_iter()
        .find(|s| s.name == name)
    else {
        return false;
    };
    let names = members_of(&spec.src);
    match git::checkout(&repo_folder(&spec, &data_dir), &update.0) {
        Ok(commit) => {
            record(&spec.src, &names, &commit);
            STATUS.with(|s| {
                let mut all = s.borrow_mut();
                for member in &names {
                    if let Some(status) = all.get_mut(member) {
                        status.update = None;
                    }
                }
            });
            shell::show_message(Level::Info, format!("Updated {}", names.join(", ")));
            true
        }
        Err(e) => {
            shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
            false
        }
    }
}

/// `:pack-restore`: every plugin from git back on its commit in
/// rt-pack-lock.json, e.g. after pulling your dotfiles. Reloading does it.
pub fn restore() {
    shell::show_message(Level::Info, "Restoring the plugins in rt-pack-lock.json");
    reload();
}

/// `:pack-clean`: delete checkouts no plugin in config.lua uses any more,
/// and their lockfile entries.
pub fn clean() {
    let Some((config_dir, data_dir)) = paths() else {
        return;
    };
    let specs = rt_config::lua::plugin_specs();
    let used: Vec<PathBuf> = specs.iter().map(|s| repo_folder(s, &data_dir)).collect();
    let pack = data_dir.join("pack");
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&pack).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && !used.contains(&path) && path.starts_with(&pack) {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => removed.push(entry.file_name().to_string_lossy().into_owned()),
                Err(e) => shell::show_message(Level::Error, format!("{}: {e}", path.display())),
            }
        }
    }
    let _ = Lockfile::load(&config_dir).and_then(|mut lock| {
        lock.plugins
            .retain(|name, _| specs.iter().any(|s| &s.name == name));
        lock.save(&config_dir)
    });
    let text = if removed.is_empty() {
        "Nothing to clean".to_string()
    } else {
        format!("Removed {}", removed.join(", "))
    };
    shell::show_message(Level::Info, text);
    crate::settings_page::refresh();
}

/// `:pack-sync`: clean, then update everything; reloading installs what's missing.
pub fn sync() {
    clean();
    if repositories().is_empty() {
        reload();
    } else {
        // Reload even when nothing moved, so missing plugins install.
        FETCHING.with(|f| {
            let (pending, _) = f.get();
            f.set((pending, true));
        });
        update(None);
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

/// Delete `name`'s lockfile entry, and its checkout unless another plugin
/// in config.lua comes from the same repository. Only a checkout riptide
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
    let shared = spec.dir.is_none() && members_of(&spec.src).len() > 1;
    if spec.dir.is_none() && !shared {
        let dir = repo_folder(&spec, &data_dir);
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
