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
    shell::with(|s| {
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
    })
    .unwrap_or_default()
}

fn carry_out(result: Result<Vec<Action>, String>) {
    let actions = match result {
        Ok(actions) => actions,
        Err(e) => return shell::show_message(Level::Error, format!("config.lua: {e}")),
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
        }
    }
    DEPTH.with(|d| d.set(d.get() - 1));
    shell::refresh_ui();
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::LuaCall { id } => carry_out(lua::call(*id, &context(count))),
        Command::User { name, args } => carry_out(lua::run_command(name, args, &context(count))),
        _ => return false,
    }
    true
}

/// Run the `rt.on(event, fn)` hooks.
pub fn emit(event: &str, fields: &[(&str, &str)]) {
    carry_out(lua::emit(event, fields, &context(None)));
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
