//! Lua callbacks from `config.lua`: key bindings to functions, commands from
//! `rt.command`, and `rt.on` event hooks. Lua returns actions; this carries
//! them out, outside any shell borrow.

use std::cell::Cell;

use cef::*;

use rt_config::lua::{self, Action, Context};
use rt_core::Command;
use rt_core::engine::Level;

use crate::shell;

thread_local! {
    /// Hooks can trigger events themselves; stop runaway chains.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

const MAX_DEPTH: u32 = 8;

fn context(count: Option<u32>) -> Context {
    shell::with(|s| context_of(s, count)).unwrap_or_default()
}

fn context_of(s: &shell::Shell, count: Option<u32>) -> Context {
    {
        let tab = s.tabs.current();
        let current = s.tabs.current_index();
        Context {
            url: tab.map(|t| t.url.clone()).unwrap_or_default(),
            title: tab.map(|t| t.title.clone()).unwrap_or_default(),
            mode: s.engine.mode().name().to_string(),
            count,
            tabs: s
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| lua::TabInfo {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    current: i == current,
                    pinned: s.tabs.is_pinned(i),
                })
                .collect(),
        }
    }
}

/// The texts of the `lua:` widgets in `statusbar.widgets`, for each window.
pub fn statusbar_widgets() -> Vec<std::collections::BTreeMap<String, String>> {
    let (names, contexts) = shell::with(|s| {
        let names: Vec<String> = s
            .engine
            .settings()
            .list("statusbar.widgets")
            .iter()
            .filter_map(|w| w.strip_prefix("lua:"))
            .map(String::from)
            .collect();
        if names.is_empty() {
            return (names, Vec::new());
        }
        let focused = s.active;
        let contexts = (0..s.windows.len())
            .map(|i| {
                s.active = i;
                context_of(s, None)
            })
            .collect();
        s.active = focused;
        (names, contexts)
    })
    .unwrap_or_default();
    let mut failed = Vec::new();
    let texts = contexts
        .iter()
        .map(|context| {
            let (texts, errors) = lua::widget_texts(&names, context);
            failed.extend(errors);
            texts
        })
        .collect();
    for error in failed {
        shell::show_message(Level::Error, error);
    }
    texts
}

fn carry_out(result: Result<Vec<Action>, String>) {
    carry_out_for("config.lua", result);
}

/// Carry out what Lua asked for; `source` names it in errors (a plugin's
/// errors already say which file).
pub fn carry_out_for(source: &str, result: Result<Vec<Action>, String>) {
    // A callback may have added a command (rt.command); the engine has to
    // know it before running anything, the new command included.
    crate::plugins::refresh_commands();
    let actions = match result {
        Ok(actions) => actions,
        Err(e) if source == "config.lua" => {
            return shell::show_message(Level::Error, format!("config.lua: {e}"));
        }
        Err(e) => return shell::show_message(Level::Error, format!("Plugin {source}: {e}")),
    };
    if DEPTH.with(Cell::get) >= MAX_DEPTH {
        return shell::show_message(Level::Error, "config.lua: hooks call each other too deeply");
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    for action in actions {
        match action {
            Action::Run(line) => {
                if let Some(effects) = shell::with(|s| s.engine.execute_str(&line, None)) {
                    shell::apply(effects);
                }
            }
            Action::Message { level, text } => shell::show_message(level, text),
            Action::Timer { id, ms } => {
                let mut task = LuaTimer::new(id);
                post_delayed_task(ThreadId::UI, Some(&mut task), i64::from(ms));
            }
            Action::Spawn(request) => crate::spawn::run_for_lua(request),
            Action::Bind {
                mode,
                keys,
                command,
            } => match shell::with(|s| s.engine.bind_from_lua(mode, &keys, &command)) {
                Some(Err(e)) => shell::show_message(Level::Error, format!("{source}: {keys}: {e}")),
                Some(Ok(_)) if source != "config.lua" => {
                    PLUGIN_KEYS.with(|k| {
                        let mut k = k.borrow_mut();
                        let list = k.entry(source.to_string()).or_default();
                        list.retain(|(m, key, _)| (*m, key.as_str()) != (mode, keys.as_str()));
                        list.push((mode, keys, command));
                    });
                }
                _ => {}
            },
            Action::Float { id, source, spec } => crate::float::show(id, source, spec),
            Action::FloatClose { id } => crate::float::close(id, true),
            Action::Panel { id, source, spec } => crate::panel::show(id, source, spec),
            Action::PanelClose { id } => crate::panel::close(id, true),
            Action::PanelFocus { id } => crate::panel::focus(id),
            Action::PluginPage {
                source,
                id,
                path,
                title,
                panel,
            } => crate::pages::open(&source, id, &path, title, panel),
            Action::PluginPageClose { source, id } => crate::pages::close(&source, id),
            Action::Secret {
                plugin,
                option,
                callback,
            } => crate::plugins::read_secret(plugin, option, callback),
            Action::Keys(keys) => {
                let keys = rt_core::key::Key::parse_sequence(&keys).unwrap_or_default();
                if let Some(effects) = shell::with(|s| s.engine.replay_keys(&keys)) {
                    shell::apply(effects);
                }
            }
            Action::Exit(code) => {
                crate::EXIT_CODE.store(code, std::sync::atomic::Ordering::SeqCst);
                if let Some(effects) = shell::with(|s| s.engine.execute_str("quit", None)) {
                    shell::apply(effects);
                }
            }
            Action::PluginPageSend {
                source,
                id,
                name,
                json,
            } => crate::pages::send(&source, id, &name, &json),
            Action::Unbind { mode, keys } => {
                if let Some(Err(e)) = shell::with(|s| s.engine.unbind_from_lua(mode, &keys)) {
                    shell::show_message(Level::Error, format!("{source}: {keys}: {e}"));
                }
                PLUGIN_KEYS.with(|k| {
                    for list in k.borrow_mut().values_mut() {
                        list.retain(|(m, key, _)| (*m, key.as_str()) != (mode, keys.as_str()));
                    }
                });
            }
            Action::Ask {
                id,
                source,
                prompt,
                ask,
            } => {
                let kind = match ask {
                    lua::Ask::Select(items) => rt_core::prompt::PromptKind::Select { items },
                    lua::Ask::Input { default, secret } => rt_core::prompt::PromptKind::Text {
                        default,
                        masked: secret,
                        path: false,
                    },
                };
                crate::prompts::ask(
                    None,
                    crate::prompts::Scope::Other,
                    rt_core::prompt::Topic::Confirm,
                    source,
                    prompt,
                    kind,
                    move |answer| {
                        let text = match answer {
                            rt_core::prompt::PromptAnswer::Text(text) => Some(text),
                            _ => None,
                        };
                        carry_out(lua::answered(id, text, &context(None)));
                    },
                );
            }
            Action::Page {
                plugin,
                pages,
                request,
            } => crate::page::carry_out(plugin.as_deref(), pages.as_deref(), request),
            Action::Open { url, target } => {
                let target = match target {
                    lua::OpenTarget::Current => rt_core::command::OpenTarget::Current,
                    lua::OpenTarget::Tab => rt_core::command::OpenTarget::Tab,
                    lua::OpenTarget::Background => rt_core::command::OpenTarget::Background,
                    lua::OpenTarget::Window => rt_core::command::OpenTarget::Window,
                    lua::OpenTarget::Private => rt_core::command::OpenTarget::Private,
                };
                shell::open(target, true, Some(url));
            }
        }
    }
    DEPTH.with(|d| d.set(d.get() - 1));
    shell::refresh_ui();
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::LuaCall { id } => carry_out(lua::call(*id, &context(count))),
        Command::User { name, args } => {
            crate::plugins::on_command(name);
            carry_out(lua::run_command(name, args, &context(count)));
        }
        _ => return false,
    }
    true
}

/// What Lua callbacks see of the browser now.
pub fn current_context() -> Context {
    context(None)
}

/// Run the `rt.on(event, fn)` hooks.
pub fn emit(event: &str, fields: &[(&str, &str)]) {
    crate::plugins::on_event(event);
    carry_out(lua::emit(event, fields, &context(None)));
}

/// A key a plugin bound: mode, keys and command.
type PluginKey = (rt_core::Mode, String, String);

thread_local! {
    /// The keys each plugin bound since the config last loaded.
    static PLUGIN_KEYS: std::cell::RefCell<std::collections::BTreeMap<String, Vec<PluginKey>>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The config is loading again, and with it every plugin's keys.
pub fn forget_plugin_keys() {
    PLUGIN_KEYS.with(|k| k.borrow_mut().clear());
}

/// The keys `plugin` bound, for the Plugins tab: mode, keys and what they do.
pub fn plugin_keys(plugin: &str) -> Vec<serde_json::Value> {
    PLUGIN_KEYS.with(|k| {
        k.borrow()
            .get(plugin)
            .into_iter()
            .flatten()
            .map(|(mode, keys, command)| {
                // A function bound without a `desc` has nothing worth showing.
                let what = lua::describe(command)
                    .or_else(|| (!command.starts_with("lua-call ")).then(|| command.clone()))
                    .unwrap_or_default();
                serde_json::json!({ "mode": mode.name(), "keys": keys, "desc": what })
            })
            .collect()
    })
}

thread_local! {
    /// The theme in use when last checked; `None` until the config first loads.
    static THEME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Emit `theme_changed` if the theme in use isn't the one last seen.
pub fn check_theme() {
    let Some(now) = shell::with(|s| rt_core::theme::current_name(s.engine.settings())) else {
        return;
    };
    let before = THEME.with(|t| t.borrow_mut().replace(now.clone()));
    if before.is_some_and(|before| before != now) {
        emit("theme_changed", &[("theme", &now)]);
    }
}

/// Hand an `rt.spawn` program's result to its callback.
pub fn spawned(callback: u32, result: &lua::SpawnResult) {
    carry_out(lua::spawned(callback, result, &context(None)));
}

wrap_task! {
    struct LuaTimer {
        id: u32,
    }

    impl Task {
        fn execute(&self) {
            carry_out(lua::timer(self.id, &context(None)));
        }
    }
}
