//! `config.lua`. A full Lua 5.4 with the standard library: the file is the
//! user's own code, trusted like a shell rc file.
//!
//! ```lua
//! c.hints.chars = "asdfjkl"                -- same as rt.set("hints.chars", ...)
//! rt.bind("<Ctrl-x>", "quit")             -- normal mode by default
//! rt.bind("<Ctrl-e>", "mode-leave", "insert")
//! rt.unbind("d")
//! if rt.platform == "macos" then c.hints.uppercase = true end
//! require("work")                          -- loads lua/work.lua from the config dir
//!
//! -- Scripting: the VM stays alive after the file runs.
//! rt.bind("<Ctrl-g>", function() rt.message("on " .. rt.url()) end)
//! rt.command("wiki", function(args) rt.open("https://en.wikipedia.org/wiki/" .. args, "tab") end)
//! rt.on("load_finished", function(e) if e.url:find("example") then rt.run("zoom-in") end end)
//! ```
//!
//! Callbacks don't touch the browser directly: they read a [`Context`]
//! (URL, title, mode, count) and return [`Action`]s for the browser to carry out.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Value};
use rt_core::Mode;
use rt_core::config::ConfigOp;
use rt_core::key::Key;
use rt_core::settings::{self, Settings};

use crate::paths::{Paths, Platform};

/// `c.a.b = v` → `rt.set("a.b", v)`; reading a full name returns its value.
const PRELUDE: &str = r#"
local function proxy(prefix)
  return setmetatable({}, {
    __index = function(_, key)
      local name = prefix .. key
      if rt._is_setting(name) then return rt.get(name) end
      return proxy(name .. ".")
    end,
    __newindex = function(_, key, value) rt.set(prefix .. key, value) end,
  })
end
c = proxy("")

-- rt.on: hooks per event, with a URL pattern, a group to clear them by, and
-- once. rt._dispatch runs them; the browser calls it for each event.
local events = {}
for _, name in ipairs(rt.EVENTS) do events[name] = true end
local hooks, next_id = {}, 0

function rt.on(event, opts, fn)
  if type(opts) == "function" then opts, fn = {}, opts end
  if not events[event] then
    error(("unknown event %q; events: %s"):format(event, table.concat(rt.EVENTS, ", ")), 2)
  end
  if type(fn) ~= "function" then error("rt.on needs a function", 2) end
  opts = opts or {}
  next_id = next_id + 1
  hooks[event] = hooks[event] or {}
  table.insert(hooks[event], { id = next_id, fn = fn, pattern = opts.pattern, group = opts.group, once = opts.once })
  return next_id
end

-- Remove one hook (its id from rt.on), or every hook of a group.
function rt.off(id)
  for _, list in pairs(hooks) do
    for i = #list, 1, -1 do
      if list[i].id == id or (type(id) == "string" and list[i].group == id) then table.remove(list, i) end
    end
  end
end

-- A name for hooks that belong together; { clear = true } first removes the
-- group's hooks, so a script can run again without doubling them.
function rt.group(name, opts)
  if opts and opts.clear then rt.off(name) end
  return name
end

rt._entry = {}
function rt._dispatch(event, payload)
  local list = hooks[event]
  if not list then return end
  for _, hook in ipairs({ table.unpack(list) }) do
    local wanted = hook.pattern == nil
      or (payload.url ~= nil and rt._matches(hook.pattern, payload.url))
    if wanted then
      if hook.once then rt.off(hook.id) end
      hook.fn(payload)
    end
  end
end
rt._entry[0] = rt._dispatch

-- rt.defer and rt.every: the browser calls rt._fire(id) after the delay.
local timers, next_timer = {}, 0
local function start(ms, fn, every)
  if type(ms) ~= "number" or ms < 0 then error("the delay is in milliseconds, 0 or more", 3) end
  if type(fn) ~= "function" then error("needs a function", 3) end
  next_timer = next_timer + 1
  local id = next_timer
  timers[id] = { fn = fn, ms = math.floor(ms), every = every }
  rt._timer(id, math.floor(ms))
  return { id = id, stop = function() timers[id] = nil end }
end
function rt.defer(ms, fn) return start(ms, fn, false) end
function rt.every(ms, fn)
  if type(ms) == "number" and ms < 10 then error("rt.every needs 10 ms or more", 2) end
  return start(ms, fn, true)
end
function rt._fire(id)
  local timer = timers[id]
  if not timer then return end
  if timer.every then rt._timer(id, timer.ms) else timers[id] = nil end
  timer.fn()
end
rt._entry[1] = rt._fire

-- Neovim's name for it.
rt.notify = rt.message
"#;

/// The events `rt.on` takes, with what each one's table carries.
pub const EVENTS: &[(&str, &str)] = &[
    ("startup", "riptide has started and loaded config.lua"),
    ("quit", "riptide is about to quit"),
    ("load_started", "a tab started loading a page (url)"),
    ("load_finished", "a tab finished loading a page (url)"),
    ("url_changed", "a tab's address changed (url)"),
    ("title_changed", "a tab's title changed (url, title)"),
    ("tab_opened", "a tab was opened (url)"),
    ("tab_closed", "a tab was closed (url)"),
    (
        "tab_selected",
        "another tab became the current one (url, index from 1)",
    ),
    (
        "window_opened",
        "a window was opened (private: \"true\" or \"false\")",
    ),
    ("window_closed", "a window was closed"),
    ("mode_changed", "the mode changed (from, to)"),
    ("setting_changed", "a setting changed (name, value as text)"),
    ("download_started", "a download started (url, path)"),
    (
        "download_finished",
        "a download finished (url, path, state: done, failed or cancelled)",
    ),
];

struct State {
    ops: Vec<ConfigOp>,
    settings: Settings,
    /// False while `config.lua` runs; afterwards `rt.set` becomes a `:set`.
    loaded: bool,
    context: Context,
    actions: Vec<Action>,
    next_callback: u32,
    commands: Vec<(String, String)>,
}

/// What callbacks see of the browser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub url: String,
    pub title: String,
    pub mode: String,
    pub count: Option<u32>,
    /// The tabs of the current window, in order.
    pub tabs: Vec<TabInfo>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabInfo {
    pub title: String,
    pub url: String,
    pub current: bool,
    pub pinned: bool,
}

/// What callbacks ask the browser to do, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A command line, e.g. `open -t x`.
    Run(String),
    Message {
        level: rt_core::engine::Level,
        text: String,
    },
    /// `rt.defer`/`rt.every`: call timer `id` back after `ms` milliseconds.
    Timer { id: u32, ms: u32 },
    /// `rt.spawn`: run a program; [`spawned`] hands the result to `callback`.
    Spawn(SpawnRequest),
}

/// A program for the browser to run for `rt.spawn`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnRequest {
    pub argv: Vec<String>,
    pub stdin: Option<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    /// Kept in `rt._spawned`; `None` when there's no callback.
    pub callback: Option<u32>,
}

/// How an `rt.spawn` program ended, for its callback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnResult {
    /// The exit code; `None` if it was killed by a signal or didn't start.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Why it couldn't run.
    pub error: Option<String>,
}

/// The Lua VM `config.lua` ran in, kept for its callbacks. It lives on the
/// thread that loads the config (CEF's UI thread).
struct Runtime {
    lua: Lua,
    state: Rc<RefCell<State>>,
    config_dir: std::path::PathBuf,
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Forget the VM, e.g. because `config.lua` no longer exists.
pub fn clear_runtime() {
    RUNTIME.with(|r| *r.borrow_mut() = None);
}

/// Commands defined with `rt.command`, as `(name, description)`.
pub fn user_commands() -> Vec<(String, String)> {
    RUNTIME.with(|r| {
        r.borrow()
            .as_ref()
            .map(|rt| rt.state.borrow().commands.clone())
            .unwrap_or_default()
    })
}

/// How long one callback may run before it's stopped.
const CALLBACK_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Call a Lua function from the runtime's tables with `args`.
fn invoke(
    table: &str,
    key: mlua::Value,
    args: mlua::MultiValue,
    context: &Context,
) -> Result<Vec<Action>, String> {
    RUNTIME.with(|r| {
        let runtime = r.borrow();
        let Some(rt) = runtime.as_ref() else {
            return Err("config.lua isn't loaded".to_string());
        };
        rt.state.borrow_mut().context = context.clone();
        // A callback runs on the browser's UI thread: one that doesn't
        // finish (a loop) would freeze the browser, so stop it instead.
        let deadline = std::time::Instant::now() + CALLBACK_LIMIT;
        let _ = rt.lua.set_hook(
            mlua::HookTriggers::new().every_nth_instruction(10_000),
            move |_, _| {
                if std::time::Instant::now() > deadline {
                    Err(mlua::Error::runtime(format!(
                        "stopped after {} seconds; long work belongs in rt.spawn or a timer",
                        CALLBACK_LIMIT.as_secs()
                    )))
                } else {
                    Ok(mlua::VmState::Continue)
                }
            },
        );
        let result = (|| -> mlua::Result<()> {
            let api: mlua::Table = rt.lua.globals().get("rt")?;
            let table: mlua::Table = api.get(table)?;
            match table.get::<mlua::Value>(key)? {
                mlua::Value::Function(f) => f.call::<()>(args),
                mlua::Value::Table(hooks) => {
                    for hook in hooks.sequence_values::<mlua::Function>() {
                        hook?.call::<()>(args.clone())?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        })();
        rt.lua.remove_hook();
        let actions = std::mem::take(&mut rt.state.borrow_mut().actions);
        result
            .map(|()| actions)
            .map_err(|e| tidy_error(&e.to_string(), &rt.config_dir))
    })
}

/// Call an `rt.spawn` callback with the program's result, once.
pub fn spawned(
    callback: u32,
    result: &SpawnResult,
    context: &Context,
) -> Result<Vec<Action>, String> {
    let prepared = RUNTIME.with(|r| -> Option<mlua::Result<mlua::MultiValue>> {
        let runtime = r.borrow();
        let rt = runtime.as_ref()?;
        Some((|| {
            let table = rt.lua.create_table()?;
            table.set("code", result.code)?;
            table.set("stdout", result.stdout.as_str())?;
            table.set("stderr", result.stderr.as_str())?;
            table.set("error", result.error.as_deref())?;
            Ok(mlua::MultiValue::from_vec(vec![mlua::Value::Table(table)]))
        })())
    });
    let args = match prepared {
        None => return Ok(Vec::new()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(args)) => args,
    };
    let key = mlua::Value::Integer(callback.into());
    let actions = invoke("_spawned", key.clone(), args, context);
    RUNTIME.with(|r| {
        if let Some(rt) = r.borrow().as_ref() {
            let api: mlua::Result<mlua::Table> = rt.lua.globals().get("rt");
            if let Ok(table) = api.and_then(|api| api.get::<mlua::Table>("_spawned")) {
                let _ = table.set(key, mlua::Value::Nil);
            }
        }
    });
    actions
}

/// A timer from `rt.defer`/`rt.every` ran out: call its function.
pub fn timer(id: u32, context: &Context) -> Result<Vec<Action>, String> {
    invoke(
        "_entry",
        mlua::Value::Integer(1),
        mlua::MultiValue::from_vec(vec![mlua::Value::Integer(id.into())]),
        context,
    )
}

/// Run the function bound with `rt.bind(keys, function)`.
pub fn call(callback: u32, context: &Context) -> Result<Vec<Action>, String> {
    invoke(
        "_callbacks",
        mlua::Value::Integer(callback.into()),
        mlua::MultiValue::new(),
        context,
    )
}

/// Run a command defined with `rt.command`; `args` is the rest of the line.
pub fn run_command(name: &str, args: &str, context: &Context) -> Result<Vec<Action>, String> {
    RUNTIME
        .with(|r| -> mlua::Result<(mlua::Value, mlua::MultiValue)> {
            let runtime = r.borrow();
            let rt = runtime
                .as_ref()
                .ok_or_else(|| mlua::Error::runtime("config.lua isn't loaded"))?;
            Ok((
                mlua::Value::String(rt.lua.create_string(name)?),
                mlua::MultiValue::from_vec(vec![mlua::Value::String(rt.lua.create_string(args)?)]),
            ))
        })
        .map_err(|e| e.to_string())
        .and_then(|(key, args)| invoke("_commands", key, args, context))
}

/// Call every `rt.on(event, fn)` hook with a table of `fields`.
pub fn emit(
    event: &str,
    fields: &[(&str, &str)],
    context: &Context,
) -> Result<Vec<Action>, String> {
    let prepared = RUNTIME.with(
        |r| -> Option<mlua::Result<(mlua::Value, mlua::MultiValue)>> {
            let runtime = r.borrow();
            let rt = runtime.as_ref()?;
            Some((|| {
                let payload = rt.lua.create_table()?;
                for (key, value) in fields {
                    payload.set(*key, *value)?;
                }
                Ok((
                    mlua::Value::String(rt.lua.create_string(event)?),
                    mlua::MultiValue::from_vec(vec![mlua::Value::Table(payload)]),
                ))
            })())
        },
    );
    match prepared {
        None => Ok(Vec::new()),
        Some(Err(e)) => Err(e.to_string()),
        Some(Ok((key, args))) => {
            let mut args = args.into_vec();
            args.insert(0, key);
            invoke(
                "_entry",
                mlua::Value::Integer(0),
                mlua::MultiValue::from_vec(args),
                context,
            )
        }
    }
}

/// Run a Lua config file. `settings` is the state so far, which `rt.get` reads.
/// Changes made before an error are kept, like qutebrowser's `config.py`.
pub fn run(path: &Path, paths: &Paths, settings: Settings) -> (Vec<ConfigOp>, Option<String>) {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => return (Vec::new(), Some(format!("{}: {e}", path.display()))),
    };
    let state = Rc::new(RefCell::new(State {
        ops: Vec::new(),
        settings,
        loaded: false,
        context: Context::default(),
        actions: Vec::new(),
        next_callback: 1,
        commands: Vec::new(),
    }));
    let lua = Lua::new();
    let result = setup(&lua, paths, state.clone())
        .map_err(|e| format!("{}: {e}", path.display()))
        .and_then(|()| {
            lua.load(&source)
                .set_name(format!("@{}", chunk_name(path, &paths.config_dir)))
                .exec()
                .map_err(|e| e.to_string())
        });
    let ops = {
        let mut state = state.borrow_mut();
        state.loaded = true;
        state.actions.clear();
        std::mem::take(&mut state.ops)
    };
    RUNTIME.with(|r| {
        *r.borrow_mut() = Some(Runtime {
            lua,
            state,
            config_dir: paths.config_dir.clone(),
        })
    });
    (ops, result.err().map(|e| tidy_error(&e, &paths.config_dir)))
}

/// Chunk names relative to the config dir. Lua truncates long names in error
/// messages, which would hide the file name behind `...`.
fn chunk_name(path: &Path, config_dir: &Path) -> String {
    let relative = path.strip_prefix(config_dir).unwrap_or(path);
    relative.to_string_lossy().replace('\\', "/")
}

/// `require("a.b")` finds `a/b.lua` or `lua/a/b.lua` under the config dir,
/// like Neovim, and names the chunk relative to it.
fn searcher(lua: &Lua, config_dir: &Path) -> mlua::Result<mlua::Function> {
    let dir = config_dir.to_path_buf();
    lua.create_function(move |lua, module: String| {
        let file = format!("{}.lua", module.replace('.', "/"));
        for candidate in [dir.join(&file), dir.join("lua").join(&file)] {
            if let Ok(source) = std::fs::read_to_string(&candidate) {
                let name = format!("@{}", chunk_name(&candidate, &dir));
                let loader = lua.load(source).set_name(name).into_function()?;
                return Ok(mlua::Value::Function(loader));
            }
        }
        let tried = format!("\n\tno file '{file}' or 'lua/{file}' in the config dir");
        Ok(mlua::Value::String(lua.create_string(tried)?))
    })
}

/// One line, `file:line: message`, with paths relative to the config dir.
/// Errors raised from Rust (bad values) only carry the location in the
/// traceback, so take the first frame from the user's files.
fn tidy_error(error: &str, config_dir: &Path) -> String {
    let dir = format!("{}{}", config_dir.display(), std::path::MAIN_SEPARATOR);
    let error = error.replace(&dir, "");
    let mut lines = error.lines();
    let first = lines.next().unwrap_or_default();
    let first = first
        .trim_start_matches("runtime error: ")
        .trim_start_matches("syntax error: ");
    if first.contains(".lua:") {
        return first.to_string();
    }
    let location = lines
        .map(str::trim)
        .filter(|l| !l.starts_with("[C]") && !l.contains("rt-prelude"))
        .find_map(|l| {
            let (location, _) = l.split_once(": ")?;
            location.contains(".lua:").then(|| location.to_string())
        });
    match location {
        Some(location) => format!("{location}: {first}"),
        None => first.to_string(),
    }
}

fn mode_arg(mode: Option<String>) -> mlua::Result<Mode> {
    match mode {
        None => Ok(Mode::Normal),
        Some(name) => name.parse().map_err(mlua::Error::runtime),
    }
}

fn setup(lua: &Lua, paths: &Paths, state: Rc<RefCell<State>>) -> mlua::Result<()> {
    let api = lua.create_table()?;
    api.set("platform", Platform::current().name())?;
    api.set("version", env!("CARGO_PKG_VERSION"))?;
    api.set("config_dir", paths.config_dir.display().to_string())?;
    api.set("data_dir", paths.data_dir.display().to_string())?;

    let s = state.clone();
    api.set(
        "set",
        lua.create_function(
            move |lua, (name, value, pattern): (String, Value, Option<String>)| {
                let def = settings::find(&name)
                    .ok_or_else(|| mlua::Error::runtime(format!("no option {name:?}")))?;
                let json: serde_json::Value = lua.from_value(value)?;
                let value = def.from_json(&json).map_err(mlua::Error::runtime)?;
                let mut state = s.borrow_mut();
                if state.loaded {
                    // From a callback: the same as typing :set.
                    let pattern = pattern.map(|p| format!("-u {p} ")).unwrap_or_default();
                    let text = match &value {
                        settings::Value::Str(text) => text.clone(),
                        other => other.to_json().to_string(),
                    };
                    state
                        .actions
                        .push(Action::Run(format!("set {pattern}{name} {text}")));
                    return Ok(());
                }
                match pattern {
                    Some(pattern) => {
                        state
                            .settings
                            .set_for(&pattern, &name, value.clone())
                            .map_err(mlua::Error::runtime)?;
                        state.ops.push(ConfigOp::SetFor {
                            pattern,
                            name,
                            value,
                        });
                    }
                    None => {
                        let _ = state.settings.set(&name, value.clone());
                        state.ops.push(ConfigOp::Set { name, value });
                    }
                }
                Ok(())
            },
        )?,
    )?;

    let s = state.clone();
    api.set(
        "get",
        lua.create_function(move |lua, name: String| {
            let state = s.borrow();
            let value = state
                .settings
                .get(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("no option {name:?}")))?;
            lua.to_value(&value.to_json())
        })?,
    )?;

    api.set(
        "theme",
        lua.create_function(|lua, (name, spec): (String, mlua::Table)| {
            if !crate::themes::valid_name(&name) {
                return Err(mlua::Error::runtime(format!(
                    "theme {name:?}: a theme name uses only a-z, 0-9, - and _"
                )));
            }
            if rt_core::theme::THEME_CHOICES.contains(&name.as_str()) {
                return Err(mlua::Error::runtime(format!("{name} is a built-in theme")));
            }
            let table = |key: &str| -> mlua::Result<std::collections::BTreeMap<String, String>> {
                match spec.get::<Value>(key)? {
                    Value::Nil => Ok(Default::default()),
                    value => lua.from_value(value),
                }
            };
            let tokens = rt_core::theme::user_theme(&table("palette")?, &table("colors")?)
                .map_err(|e| mlua::Error::runtime(format!("theme {name}: {e}")))?;
            rt_core::theme::add_user_theme(name, tokens);
            Ok(())
        })?,
    )?;

    api.set(
        "_is_setting",
        lua.create_function(|_, name: String| Ok(settings::find(&name).is_some()))?,
    )?;

    api.set("_callbacks", lua.create_table()?)?;
    api.set("_commands", lua.create_table()?)?;
    let events = lua.create_table()?;
    for (name, _) in EVENTS {
        events.push(*name)?;
    }
    api.set("EVENTS", events)?;
    api.set("_spawned", lua.create_table()?)?;

    let s = state.clone();
    api.set(
        "bind",
        lua.create_function(
            move |lua, (keys, command, mode): (String, Value, Option<String>)| {
                Key::parse_sequence(&keys).map_err(|e| mlua::Error::runtime(e.to_string()))?;
                let mode = mode_arg(mode)?;
                let command = match command {
                    Value::String(text) => text.to_str()?.to_string(),
                    Value::Function(f) => {
                        let id = {
                            let mut state = s.borrow_mut();
                            state.next_callback += 1;
                            state.next_callback - 1
                        };
                        let api: mlua::Table = lua.globals().get("rt")?;
                        api.get::<mlua::Table>("_callbacks")?.set(id, f)?;
                        format!("lua-call {id}")
                    }
                    _ => {
                        return Err(mlua::Error::runtime(
                            "rt.bind takes a command string or a function",
                        ));
                    }
                };
                s.borrow_mut().ops.push(ConfigOp::Bind {
                    mode,
                    keys,
                    command,
                });
                Ok(())
            },
        )?,
    )?;

    let s = state.clone();
    api.set(
        "command",
        lua.create_function(
            move |lua, (name, f, description): (String, mlua::Function, Option<String>)| {
                let valid = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if !valid {
                    return Err(mlua::Error::runtime(format!("bad command name {name:?}")));
                }
                if rt_core::command::COMMANDS.iter().any(|c| c.name == name) {
                    return Err(mlua::Error::runtime(format!(
                        ":{name} is a built-in command"
                    )));
                }
                let api: mlua::Table = lua.globals().get("rt")?;
                api.get::<mlua::Table>("_commands")?.set(name.clone(), f)?;
                let mut state = s.borrow_mut();
                state.commands.retain(|(n, _)| *n != name);
                state.commands.push((
                    name,
                    description.unwrap_or_else(|| "Defined in config.lua".into()),
                ));
                Ok(())
            },
        )?,
    )?;

    let s = state.clone();
    api.set(
        "_timer",
        lua.create_function(move |_, (id, ms): (u32, f64)| {
            let ms = ms.clamp(0.0, f64::from(u32::MAX)) as u32;
            s.borrow_mut().actions.push(Action::Timer { id, ms });
            Ok(())
        })?,
    )?;

    api.set(
        "_matches",
        lua.create_function(|_, (pattern, url): (String, String)| {
            Ok(rt_core::url::pattern_matches(&pattern, &url))
        })?,
    )?;

    for (name, field) in [("url", 0), ("title", 1), ("mode", 2)] {
        let s = state.clone();
        api.set(
            name,
            lua.create_function(move |_, ()| {
                let state = s.borrow();
                let c = &state.context;
                Ok(match field {
                    0 => c.url.clone(),
                    1 => c.title.clone(),
                    _ => c.mode.clone(),
                })
            })?,
        )?;
    }
    let s = state.clone();
    api.set(
        "count",
        lua.create_function(move |_, ()| Ok(s.borrow().context.count))?,
    )?;

    let s = state.clone();
    api.set(
        "tabs",
        lua.create_function(move |lua, ()| {
            let list = lua.create_table()?;
            for (i, tab) in s.borrow().context.tabs.iter().enumerate() {
                let t = lua.create_table()?;
                t.set("index", i + 1)?;
                t.set("title", tab.title.clone())?;
                t.set("url", tab.url.clone())?;
                t.set("current", tab.current)?;
                t.set("pinned", tab.pinned)?;
                list.push(t)?;
            }
            Ok(list)
        })?,
    )?;

    let s = state.clone();
    api.set(
        "run",
        lua.create_function(move |_, line: String| {
            s.borrow_mut().actions.push(Action::Run(line));
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "open",
        lua.create_function(move |_, (url, target): (String, Option<String>)| {
            let flag = match target.as_deref() {
                None | Some("current") => "",
                Some("tab") => "-t ",
                Some("tab-bg") => "-b ",
                Some("window") => "-w ",
                Some("private") => "-p ",
                Some(other) => {
                    return Err(mlua::Error::runtime(format!("unknown target {other:?}")));
                }
            };
            s.borrow_mut()
                .actions
                .push(Action::Run(format!("open {flag}{url}")));
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "spawn",
        lua.create_function(move |lua, (argv, second, third): (Value, Value, Value)| {
            let argv: Vec<String> = match argv {
                Value::Table(list) => list
                    .sequence_values::<String>()
                    .collect::<mlua::Result<_>>()?,
                Value::String(line) => {
                    rt_core::shell_words::split(&line.to_str()?).map_err(mlua::Error::runtime)?
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "rt.spawn takes a list of arguments or a command line",
                    ));
                }
            };
            if argv.is_empty() {
                return Err(mlua::Error::runtime("rt.spawn: nothing to run"));
            }
            // rt.spawn(argv, fn) or rt.spawn(argv, opts, fn).
            let (opts, callback) = match (second, third) {
                (Value::Function(f), _) => (None, Some(f)),
                (Value::Table(opts), Value::Function(f)) => (Some(opts), Some(f)),
                (Value::Table(opts), Value::Nil) => (Some(opts), None),
                (Value::Nil, Value::Nil) => (None, None),
                _ => return Err(mlua::Error::runtime("rt.spawn(argv, [opts], [callback])")),
            };
            let mut request = SpawnRequest {
                argv,
                ..SpawnRequest::default()
            };
            if let Some(opts) = opts {
                request.stdin = opts.get("stdin")?;
                request.cwd = opts.get("cwd")?;
                if let Some(env) = opts.get::<Option<mlua::Table>>("env")? {
                    for pair in env.pairs::<String, String>() {
                        request.env.push(pair?);
                    }
                    request.env.sort();
                }
            }
            if let Some(f) = callback {
                let id = {
                    let mut state = s.borrow_mut();
                    state.next_callback += 1;
                    state.next_callback - 1
                };
                let api: mlua::Table = lua.globals().get("rt")?;
                api.get::<mlua::Table>("_spawned")?.set(id, f)?;
                request.callback = Some(id);
            }
            s.borrow_mut().actions.push(Action::Spawn(request));
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "message",
        lua.create_function(move |_, (text, level): (String, Option<String>)| {
            let level = match level.as_deref() {
                None | Some("info") => rt_core::engine::Level::Info,
                Some("warning" | "warn") => rt_core::engine::Level::Warning,
                Some("error") => rt_core::engine::Level::Error,
                Some(other) => {
                    return Err(mlua::Error::runtime(format!(
                        "unknown level {other:?}; use info, warning or error"
                    )));
                }
            };
            s.borrow_mut().actions.push(Action::Message { level, text });
            Ok(())
        })?,
    )?;

    let s = state;
    api.set(
        "unbind",
        lua.create_function(move |_, (keys, mode): (String, Option<String>)| {
            Key::parse_sequence(&keys).map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let mode = mode_arg(mode)?;
            s.borrow_mut().ops.push(ConfigOp::Unbind { mode, keys });
            Ok(())
        })?,
    )?;

    // `hb` is the old name, kept so existing configs still load.
    lua.globals().set("rt", api.clone())?;
    lua.globals().set("hb", api)?;

    // Our searcher runs right after the preload table, before package.path.
    let package: mlua::Table = lua.globals().get("package")?;
    let searchers: mlua::Table = package.get("searchers")?;
    searchers.raw_insert(2, searcher(lua, &paths.config_dir)?)?;

    lua.load(PRELUDE).set_name("=rt-prelude").exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_records_the_program_and_calls_back_once() {
        let dir = std::env::temp_dir().join(format!("rt-lua-spawn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
rt.command("words", function()
  rt.spawn({"wc", "-w"}, { stdin = "a b c", env = { LANG = "C" } }, function(r)
    rt.message(r.stdout:match("%d+") .. " words, code " .. r.code)
  end)
end)
rt.command("fire", function() rt.spawn("notify-send 'hi there'") end)
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        let ctx = Context::default();
        let actions = run_command("words", "", &ctx).unwrap();
        let [Action::Spawn(request)] = actions.as_slice() else {
            panic!("no spawn in {actions:?}");
        };
        assert_eq!(request.argv, ["wc", "-w"]);
        assert_eq!(request.stdin.as_deref(), Some("a b c"));
        assert_eq!(request.env, [("LANG".to_string(), "C".to_string())]);
        let callback = request.callback.unwrap();
        let result = SpawnResult {
            code: Some(0),
            stdout: "3\n".into(),
            ..SpawnResult::default()
        };
        assert_eq!(
            spawned(callback, &result, &ctx).unwrap(),
            [Action::Message {
                level: rt_core::engine::Level::Info,
                text: "3 words, code 0".into()
            }]
        );
        assert!(
            spawned(callback, &result, &ctx).unwrap().is_empty(),
            "called once"
        );
        assert_eq!(
            run_command("fire", "", &ctx).unwrap(),
            [Action::Spawn(SpawnRequest {
                argv: vec!["notify-send".into(), "hi there".into()],
                ..SpawnRequest::default()
            })]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn timers_notify_and_runaway_callbacks() {
        use rt_core::engine::Level;
        let dir = std::env::temp_dir().join(format!("rt-lua-timers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
local ticks = 0
rt.command("later", function() rt.defer(500, function() rt.notify("later", "warning") end) end)
rt.command("tick", function()
  local t
  t = rt.every(100, function()
    ticks = ticks + 1
    rt.notify("tick " .. ticks)
    if ticks == 2 then t:stop() end
  end)
end)
rt.command("spin", function() while true do end end)
rt.command("hello", function() rt.notify("hello") end)
assert(not pcall(rt.notify, "x", "loud"))
assert(not pcall(rt.every, 1, function() end))
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        let ctx = Context::default();
        // rt.defer asks for a timer; firing it runs the function once.
        assert_eq!(
            run_command("later", "", &ctx).unwrap(),
            [Action::Timer { id: 1, ms: 500 }]
        );
        assert_eq!(
            timer(1, &ctx).unwrap(),
            [Action::Message {
                level: Level::Warning,
                text: "later".into()
            }]
        );
        assert!(timer(1, &ctx).unwrap().is_empty());
        // rt.every asks again each time, until stopped.
        assert_eq!(
            run_command("tick", "", &ctx).unwrap(),
            [Action::Timer { id: 2, ms: 100 }]
        );
        assert_eq!(
            timer(2, &ctx).unwrap(),
            [
                Action::Timer { id: 2, ms: 100 },
                Action::Message {
                    level: Level::Info,
                    text: "tick 1".into()
                }
            ]
        );
        assert_eq!(
            timer(2, &ctx).unwrap(),
            [
                Action::Timer { id: 2, ms: 100 },
                Action::Message {
                    level: Level::Info,
                    text: "tick 2".into()
                }
            ]
        );
        assert!(timer(2, &ctx).unwrap().is_empty());
        // A loop is stopped, and the next callback still runs.
        let start = std::time::Instant::now();
        let error = run_command("spin", "", &ctx).unwrap_err();
        assert!(error.contains("stopped after"), "{error}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(
            run_command("hello", "", &ctx).unwrap(),
            [Action::Message {
                level: Level::Info,
                text: "hello".into()
            }]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hooks_take_patterns_groups_and_once() {
        let dir = std::env::temp_dir().join(format!("rt-lua-hooks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
local g = rt.group("mine", { clear = true })
rt.on("load_finished", { pattern = "*.example.com", group = g }, function(e) rt.message("example " .. e.url) end)
rt.on("load_finished", { once = true }, function() rt.message("first load") end)
local id = rt.on("tab_closed", function() rt.message("closed") end)
rt.off(id)
rt.on("setting_changed", function(e) rt.message(e.name .. "=" .. e.value) end)
rt.command("drop-mine", function() rt.group("mine", { clear = true }) end)
assert(not pcall(rt.on, "nope", function() end))
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        let ctx = Context::default();
        let texts = |actions: Vec<Action>| -> Vec<String> {
            actions
                .into_iter()
                .filter_map(|a| match a {
                    Action::Message { text, .. } => Some(text),
                    _ => None,
                })
                .collect()
        };
        let news = [("url", "https://news.example.com/")];
        assert_eq!(
            texts(emit("load_finished", &news, &ctx).unwrap()),
            ["example https://news.example.com/", "first load"]
        );
        // `once` ran once; the pattern keeps other sites out.
        assert_eq!(
            texts(emit("load_finished", &news, &ctx).unwrap()),
            ["example https://news.example.com/"]
        );
        assert!(
            texts(emit("load_finished", &[("url", "https://other.org/")], &ctx).unwrap())
                .is_empty()
        );
        assert!(texts(emit("tab_closed", &[("url", "x")], &ctx).unwrap()).is_empty());
        assert_eq!(
            texts(
                emit(
                    "setting_changed",
                    &[("name", "zoom.default"), ("value", "125")],
                    &ctx
                )
                .unwrap()
            ),
            ["zoom.default=125"]
        );
        // Clearing the group removes its hooks.
        run_command("drop-mine", "", &ctx).unwrap();
        assert!(texts(emit("load_finished", &news, &ctx).unwrap()).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn callbacks_commands_and_hooks() {
        let dir = std::env::temp_dir().join(format!("rt-lua-runtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
rt.bind("<Ctrl-g>", function() rt.message("on " .. rt.url() .. " x" .. tostring(rt.count())) end)
rt.command("wiki", function(args) rt.open("https://en.wikipedia.org/wiki/" .. args, "tab") end, "Look it up")
rt.on("load_finished", function(e) if e.url:find("example") then rt.run("zoom-in") end end)
rt.command("boom", function() error("broken") end)
rt.command("dark", function() rt.set("colors.webpage.preferred_color_scheme", "dark") end)
rt.command("count-tabs", function()
  local pinned = 0
  for _, t in ipairs(rt.tabs()) do if t.pinned then pinned = pinned + 1 end end
  rt.message(#rt.tabs() .. " tabs, " .. pinned .. " pinned, current " .. rt.tabs()[2].title)
end)
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (ops, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        assert_eq!(
            ops,
            [ConfigOp::Bind {
                mode: Mode::Normal,
                keys: "<Ctrl-g>".into(),
                command: "lua-call 1".into()
            }]
        );
        let ctx = Context {
            url: "https://example.com/".into(),
            count: Some(3),
            ..Context::default()
        };
        assert_eq!(
            call(1, &ctx).unwrap(),
            [Action::Message {
                level: rt_core::engine::Level::Info,
                text: "on https://example.com/ x3".into()
            }]
        );
        assert_eq!(
            run_command("wiki", "Rust", &ctx).unwrap(),
            [Action::Run(
                "open -t https://en.wikipedia.org/wiki/Rust".into()
            )]
        );
        assert_eq!(
            emit("load_finished", &[("url", "https://example.com/")], &ctx).unwrap(),
            [Action::Run("zoom-in".into())]
        );
        assert!(
            emit("load_finished", &[("url", "https://other.org/")], &ctx)
                .unwrap()
                .is_empty()
        );
        assert!(emit("tab_opened", &[], &ctx).unwrap().is_empty());
        assert!(
            run_command("boom", "", &ctx)
                .unwrap_err()
                .contains("config.lua:5: broken")
        );
        assert_eq!(
            run_command("dark", "", &ctx).unwrap(),
            [Action::Run(
                "set colors.webpage.preferred_color_scheme dark".into()
            )]
        );
        let tab = |title: &str, current, pinned| TabInfo {
            title: title.into(),
            url: String::new(),
            current,
            pinned,
        };
        let with_tabs = Context {
            tabs: vec![tab("a", false, true), tab("b", true, false)],
            ..Context::default()
        };
        assert_eq!(
            run_command("count-tabs", "", &with_tabs).unwrap(),
            [Action::Message {
                level: rt_core::engine::Level::Info,
                text: "2 tabs, 1 pinned, current b".into()
            }]
        );
        assert_eq!(
            user_commands()
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["wiki", "boom", "dark", "count-tabs"]
        );
        std::fs::write(dir.join("config.lua"), "rt.command('open', function() end)").unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert!(error.is_some_and(|e| e.contains("built-in")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rt_set_takes_a_url_pattern() {
        let dir = std::env::temp_dir().join(format!("rt-lua-pattern-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            "rt.set('content.geolocation', 'true', '*.example.com')\nrt.set('hints.chars', 'ab', 'x.org')\n",
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (ops, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert!(ops.contains(&ConfigOp::SetFor {
            pattern: "*.example.com".into(),
            name: "content.geolocation".into(),
            value: rt_core::settings::Value::Str("true".into()),
        }));
        assert!(error.is_some_and(|e| e.contains("can't be set per site")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn hb_is_kept_as_the_old_name() {
        let dir = std::env::temp_dir().join(format!("rt-lua-alias-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.lua"), "hb.set('hints.chars', 'qwer')\n").unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (ops, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert!(error.is_none(), "{error:?}");
        assert!(ops.contains(&ConfigOp::Set {
            name: "hints.chars".into(),
            value: rt_core::settings::Value::Str("qwer".into()),
        }));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    use rt_core::settings::Value as SettingValue;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rt-lua-{name}-{}", std::process::id()));
            std::fs::create_dir_all(dir.join("lua")).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rt_theme_defines_a_theme_that_ui_theme_accepts() {
        let ops = run_lua(
            "rt-lua-theme",
            r##"
rt.theme("lua-ink", {
  palette = {
    base = "#0a0c0f", surface = "#181616", fg = "#c5c9c5", accent = "#8ba4b0",
    yellow = "#c4b28a", red = "#c4746e", green = "#8a9a7b", blue = "#658594",
  },
  colors = { ["statusbar.insert.bg"] = "#123456" },
})
c.ui.theme = "lua-ink"
"##,
            &[],
        )
        .unwrap();
        assert!(ops.contains(&ConfigOp::Set {
            name: "ui.theme".into(),
            value: settings::Value::Str("lua-ink".into()),
        }));
        let tokens = rt_core::theme::theme("lua-ink").unwrap();
        assert_eq!(tokens["statusbar-insert-bg"], "#123456");
        let error = run_lua(
            "rt-lua-theme-bad",
            r##"rt.theme("half", { palette = { base = "#000000" } })"##,
            &[],
        )
        .unwrap_err();
        assert!(error.contains("palette.surface is missing"), "{error}");
        let error = run_lua(
            "rt-lua-theme-nord",
            r#"rt.theme("nord", { palette = {} })"#,
            &[],
        )
        .unwrap_err();
        assert!(error.contains("built-in"), "{error}");
    }

    fn run_lua(name: &str, code: &str, extra: &[(&str, &str)]) -> Result<Vec<ConfigOp>, String> {
        let (ops, error) = run_lua_partial(name, code, extra);
        match error {
            Some(e) => Err(e),
            None => Ok(ops),
        }
    }

    fn run_lua_partial(
        name: &str,
        code: &str,
        extra: &[(&str, &str)],
    ) -> (Vec<ConfigOp>, Option<String>) {
        let dir = TempDir::new(name);
        for (file, text) in extra {
            std::fs::write(dir.0.join(file), text).unwrap();
        }
        let path = dir.0.join("config.lua");
        std::fs::write(&path, code).unwrap();
        let paths = Paths {
            config_dir: dir.0.clone(),
            data_dir: dir.0.join("data"),
        };
        run(&path, &paths, Settings::default())
    }

    #[test]
    fn proxy_set_get_bind() {
        let ops = run_lua(
            "basic",
            r#"
            c.hints.chars = "qwer"
            assert(c.hints.chars == "qwer")
            assert(c.messages.timeout == 3000)
            c.url.start_pages = { "about:blank" }
            rt.bind("<Ctrl-x>", "quit")
            rt.unbind("d", "normal")
            rt.bind("<Ctrl-e>", "mode-leave", "insert")
            assert(rt.platform == "linux" or rt.platform == "macos" or rt.platform == "windows")
            "#,
            &[],
        )
        .unwrap();
        assert_eq!(
            ops[0],
            ConfigOp::Set {
                name: "hints.chars".into(),
                value: SettingValue::Str("qwer".into())
            }
        );
        assert_eq!(
            ops[1],
            ConfigOp::Set {
                name: "url.start_pages".into(),
                value: SettingValue::List(vec!["about:blank".into()])
            }
        );
        assert_eq!(ops.len(), 5);
        assert!(ops.contains(&ConfigOp::Bind {
            mode: Mode::Insert,
            keys: "<Ctrl-e>".into(),
            command: "mode-leave".into()
        }));
    }

    #[test]
    fn errors_name_the_file_and_line() {
        let err = run_lua(
            "err",
            "c.hints.uppercase = true\nc.hints.chars = 'x'\n",
            &[],
        )
        .unwrap_err();
        assert_eq!(
            err,
            "config.lua:2: hints.chars: needs at least two distinct characters"
        );
        let err = run_lua("raise", "\nerror('boom')", &[]).unwrap_err();
        assert_eq!(err, "config.lua:2: boom");
        let err = run_lua("syntax", "c.hints.chars = ", &[]).unwrap_err();
        assert!(err.starts_with("config.lua:1:"), "{err}");
        let err = run_lua(
            "nested",
            "require('bad')",
            &[("lua/bad.lua", "c.hints.chars = 42")],
        )
        .unwrap_err();
        assert_eq!(err, "lua/bad.lua:1: hints.chars: expected a string, got 42");
        let err = run_lua("unknown", "c.nope.thing = 1", &[]).unwrap_err();
        assert!(err.contains("no option \"nope.thing\""), "{err}");
        let err = run_lua("mode", "rt.bind('x', 'quit', 'sideways')", &[]).unwrap_err();
        assert!(err.contains("unknown mode"), "{err}");
    }

    #[test]
    fn changes_before_an_error_are_kept() {
        let (ops, error) = run_lua_partial(
            "partial",
            "c.hints.uppercase = true\nerror('boom')\nc.hints.chars = 'qw'",
            &[],
        );
        assert_eq!(ops.len(), 1);
        assert!(error.unwrap().contains("boom"));
    }

    #[test]
    fn require_loads_from_config_dir() {
        let ops = run_lua(
            "require",
            "require('work')",
            &[("lua/work.lua", "c.tabs.last_close = 'blank'")],
        )
        .unwrap();
        assert_eq!(
            ops,
            vec![ConfigOp::Set {
                name: "tabs.last_close".into(),
                value: SettingValue::Str("blank".into())
            }]
        );
    }
}
