//! Lua callbacks from `config.lua`: key bindings to functions, commands from
//! `hb.command`, and `hb.on` event hooks. Lua returns actions; this carries
//! them out, outside any shell borrow.

use std::cell::Cell;

use hb_config::lua::{self, Action, Context};
use hb_core::Command;
use hb_core::engine::Level;

use crate::shell;

thread_local! {
    /// Hooks can trigger events themselves; stop runaway chains.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

const MAX_DEPTH: u32 = 8;

fn context(count: Option<u32>) -> Context {
    shell::with(|s| {
        let tab = s.tabs.current();
        Context {
            url: tab.map(|t| t.url.clone()).unwrap_or_default(),
            title: tab.map(|t| t.title.clone()).unwrap_or_default(),
            mode: s.engine.mode().name().to_string(),
            count,
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
            Action::Message { error, text } => {
                shell::show_message(if error { Level::Error } else { Level::Info }, text)
            }
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

/// Run the `hb.on(event, fn)` hooks.
pub fn emit(event: &str, fields: &[(&str, &str)]) {
    carry_out(lua::emit(event, fields, &context(None)));
}
