//! `config.lua`. A full Lua 5.4 with the standard library: the file is the
//! user's own code, trusted like a shell rc file.
//!
//! ```lua
//! c.hints.chars = "asdfjkl"                -- same as hb.set("hints.chars", ...)
//! hb.bind("<Ctrl-x>", "quit")             -- normal mode by default
//! hb.bind("<Ctrl-e>", "mode-leave", "insert")
//! hb.unbind("d")
//! if hb.platform == "macos" then c.hints.uppercase = true end
//! require("work")                          -- loads lua/work.lua from the config dir
//! ```

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use hb_core::Mode;
use hb_core::config::ConfigOp;
use hb_core::key::Key;
use hb_core::settings::{self, Settings};
use mlua::{Lua, LuaSerdeExt, Value};

use crate::paths::{Paths, Platform};

/// `c.a.b = v` → `hb.set("a.b", v)`; reading a full name returns its value.
const PRELUDE: &str = r#"
local function proxy(prefix)
  return setmetatable({}, {
    __index = function(_, key)
      local name = prefix .. key
      if hb._is_setting(name) then return hb.get(name) end
      return proxy(name .. ".")
    end,
    __newindex = function(_, key, value) hb.set(prefix .. key, value) end,
  })
end
c = proxy("")
"#;

struct State {
    ops: Vec<ConfigOp>,
    settings: Settings,
}

/// Run a Lua config file. `settings` is the state so far, which `hb.get` reads.
/// Changes made before an error are kept, like qutebrowser's `config.py`.
pub fn run(path: &Path, paths: &Paths, settings: Settings) -> (Vec<ConfigOp>, Option<String>) {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => return (Vec::new(), Some(format!("{}: {e}", path.display()))),
    };
    let state = Rc::new(RefCell::new(State {
        ops: Vec::new(),
        settings,
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
    let ops = std::mem::take(&mut state.borrow_mut().ops);
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
        .filter(|l| !l.starts_with("[C]") && !l.contains("hb-prelude"))
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
    let hb = lua.create_table()?;
    hb.set("platform", Platform::current().name())?;
    hb.set("version", env!("CARGO_PKG_VERSION"))?;
    hb.set("config_dir", paths.config_dir.display().to_string())?;
    hb.set("data_dir", paths.data_dir.display().to_string())?;

    let s = state.clone();
    hb.set(
        "set",
        lua.create_function(
            move |lua, (name, value, pattern): (String, Value, Option<String>)| {
                let def = settings::find(&name)
                    .ok_or_else(|| mlua::Error::runtime(format!("no option {name:?}")))?;
                let json: serde_json::Value = lua.from_value(value)?;
                let value = def.from_json(&json).map_err(mlua::Error::runtime)?;
                let mut state = s.borrow_mut();
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
    hb.set(
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

    hb.set(
        "_is_setting",
        lua.create_function(|_, name: String| Ok(settings::find(&name).is_some()))?,
    )?;

    let s = state.clone();
    hb.set(
        "bind",
        lua.create_function(
            move |_, (keys, command, mode): (String, String, Option<String>)| {
                Key::parse_sequence(&keys).map_err(|e| mlua::Error::runtime(e.to_string()))?;
                let mode = mode_arg(mode)?;
                s.borrow_mut().ops.push(ConfigOp::Bind {
                    mode,
                    keys,
                    command,
                });
                Ok(())
            },
        )?,
    )?;

    let s = state;
    hb.set(
        "unbind",
        lua.create_function(move |_, (keys, mode): (String, Option<String>)| {
            Key::parse_sequence(&keys).map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let mode = mode_arg(mode)?;
            s.borrow_mut().ops.push(ConfigOp::Unbind { mode, keys });
            Ok(())
        })?,
    )?;

    lua.globals().set("hb", hb)?;

    // Our searcher runs right after the preload table, before package.path.
    let package: mlua::Table = lua.globals().get("package")?;
    let searchers: mlua::Table = package.get("searchers")?;
    searchers.raw_insert(2, searcher(lua, &paths.config_dir)?)?;

    lua.load(PRELUDE).set_name("=hb-prelude").exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hb_set_takes_a_url_pattern() {
        let dir = std::env::temp_dir().join(format!("hb-lua-pattern-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            "hb.set('content.geolocation', 'true', '*.example.com')\nhb.set('hints.chars', 'ab', 'x.org')\n",
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (ops, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert!(ops.contains(&ConfigOp::SetFor {
            pattern: "*.example.com".into(),
            name: "content.geolocation".into(),
            value: hb_core::settings::Value::Str("true".into()),
        }));
        assert!(error.is_some_and(|e| e.contains("can't be set per site")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    use hb_core::settings::Value as SettingValue;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("hb-lua-{name}-{}", std::process::id()));
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
            hb.bind("<Ctrl-x>", "quit")
            hb.unbind("d", "normal")
            hb.bind("<Ctrl-e>", "mode-leave", "insert")
            assert(hb.platform == "linux" or hb.platform == "macos" or hb.platform == "windows")
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
        let err = run_lua("mode", "hb.bind('x', 'quit', 'sideways')", &[]).unwrap_err();
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
