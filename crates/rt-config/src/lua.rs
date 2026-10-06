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
"#;

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
        error: bool,
        text: String,
    },
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
        Some(Ok((key, args))) => invoke("_hooks", key, args, context),
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
        "_is_setting",
        lua.create_function(|_, name: String| Ok(settings::find(&name).is_some()))?,
    )?;

    api.set("_callbacks", lua.create_table()?)?;
    api.set("_commands", lua.create_table()?)?;
    api.set("_hooks", lua.create_table()?)?;
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

    api.set(
        "on",
        lua.create_function(move |lua, (event, f): (String, mlua::Function)| {
            const EVENTS: &[&str] = &["load_finished", "url_changed", "tab_opened", "mode_changed"];
            if !EVENTS.contains(&event.as_str()) {
                return Err(mlua::Error::runtime(format!(
                    "unknown event {event:?}; events: {}",
                    EVENTS.join(", ")
                )));
            }
            let api: mlua::Table = lua.globals().get("rt")?;
            let hooks: mlua::Table = api.get("_hooks")?;
            let list = match hooks.get::<Option<mlua::Table>>(event.clone())? {
                Some(list) => list,
                None => {
                    let list = lua.create_table()?;
                    hooks.set(event, list.clone())?;
                    list
                }
            };
            list.push(f)
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
            let error = level.as_deref() == Some("error");
            s.borrow_mut().actions.push(Action::Message { error, text });
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
                error: false,
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
                error: false,
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
                error: false,
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
