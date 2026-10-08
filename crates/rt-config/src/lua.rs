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
const PRELUDE: &str = r##"
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

-- rt.statusbar.widget(name, fn): fn gives the text of "lua:<name>" in
-- statusbar.widgets each time the bar is drawn; nil removes the widget.
rt.statusbar = {}
local widgets = {}
function rt.statusbar.widget(name, fn)
  if type(name) ~= "string" or not name:match("^[%w_-]+$") then
    error("widget names use letters, digits, - and _", 2)
  end
  if fn ~= nil and type(fn) ~= "function" then error("rt.statusbar.widget takes a function", 2) end
  widgets[name] = fn
end
-- The texts of the widgets in names; a widget that fails is removed.
function rt._widget_texts(names)
  local texts, errors = {}, {}
  for _, name in ipairs(names) do
    local fn = widgets[name]
    if fn then
      local ok, text = pcall(fn)
      if ok and (text == nil or type(text) == "string" or type(text) == "number") then
        texts[name] = text == nil and "" or tostring(text):sub(1, 200)
      else
        widgets[name] = nil
        errors[#errors + 1] = "widget " .. name .. ": " .. (ok and "return a string" or tostring(text))
      end
    end
  end
  return texts, errors
end

-- rt.pack.add(spec or list of specs): plugins, loaded once config.lua has
-- run (after installing or approving them). A spec is a git URL, or a table:
-- { "url" or src = "url" or dir = "~/folder", name, version, trusted, opts, config,
--   event, cmd, keys }; event, cmd and keys make it wait for one of them to load.
rt.pack = {}
local pack_specs = {}
local function add_spec(spec)
  if type(spec) == "string" then spec = { spec } end
  local src = spec.src or spec[1]
  if spec.builtin ~= nil then
    if src ~= nil or spec.dir ~= nil then error("a builtin plugin has no git URL or dir", 3) end
    if spec.name ~= nil and spec.name ~= spec.builtin then error("a builtin plugin keeps its own name", 3) end
  elseif src == nil and spec.dir == nil then
    error("a plugin needs a git URL, dir or builtin", 3)
  end
  local name = spec.builtin or spec.name or rt._plugin_name(src or spec.dir)
  local function list(value)
    if value == nil then return {} end
    if type(value) ~= "table" then return { value } end
    return value
  end
  local keys = {}
  for _, k in ipairs(list(spec.keys)) do
    if type(k) == "table" then
      keys[#keys + 1] = { k[1], k.mode or "normal" }
    else
      keys[#keys + 1] = { k, "normal" }
    end
  end
  rt._pack_spec(name, src or "", spec.dir, spec.version, spec.trusted == true,
    list(spec.event), list(spec.cmd), keys, spec.builtin ~= nil)
  pack_specs[name] = spec
end
function rt.pack.add(specs)
  if type(specs) == "string" or specs.src or specs.dir or specs.builtin or type(specs[1]) == "string" then
    add_spec(specs)
  else
    for _, spec in ipairs(specs) do add_spec(spec) end
  end
end

-- After a plugin loads: its spec's config function, or require(name).setup(opts).
function rt._pack_ready(name)
  local spec = pack_specs[name]
  if not spec then return end
  if spec.config then
    spec.config()
  elseif spec.opts then
    local module = require(name)
    if type(module) == "table" and type(module.setup) == "function" then module.setup(spec.opts) end
  end
end

-- A plugin's globals: the safe parts of Lua, its own copy of rt with only the
-- functions its permissions allow, and require limited to plugin modules.
local SAFE = {
  "assert", "error", "ipairs", "next", "pairs", "pcall", "rawequal", "rawget", "rawlen",
  "rawset", "select", "setmetatable", "getmetatable", "tonumber", "tostring", "type", "xpcall",
}
local GATED = { spawn = "spawn", run = "commands", set = "settings", get = "settings" }
-- Defined further down, with rt.ui and rt.page.
local ui_for, page_for
local function copy(t)
  local out = {}
  for k, v in pairs(t) do out[k] = v end
  return out
end
function rt._sandbox(name, permissions, require_fn)
  local env = {}
  for _, key in ipairs(SAFE) do env[key] = _G[key] end
  for _, lib in ipairs({ "string", "table", "math", "utf8", "coroutine" }) do env[lib] = copy(_G[lib]) end
  if permissions.files then
    env.io, env.os = io, os
  else
    env.os = { time = os.time, date = os.date, clock = os.clock, difftime = os.difftime }
  end
  -- Text only, and in the plugin's globals unless it gives others.
  env.load = function(chunk, chunkname, _, globals) return load(chunk, chunkname, "t", globals or env) end
  env.print = function(...)
    local parts = {}
    for i = 1, select("#", ...) do parts[#parts + 1] = tostring((select(i, ...))) end
    rt.notify(name .. ": " .. table.concat(parts, " "))
  end
  env.require = require_fn
  local api = {}
  for key, value in pairs(rt) do
    if type(key) == "string" and key:sub(1, 1) ~= "_" and key ~= "pack" then
      local needs = GATED[key]
      if needs == nil or permissions[needs] then
        api[key] = type(value) == "table" and copy(value) or value
      end
    end
  end
  -- A key bound to a command line could run anything, :spawn included.
  if not permissions.commands then
    local function only_functions(rhs)
      if type(rhs) ~= "function" then
        error("binding keys to a command line needs the commands permission; bind a function", 3)
      end
    end
    local bind, set = api.bind, api.keymap.set
    api.bind = function(keys, rhs, mode) only_functions(rhs) return bind(keys, rhs, mode) end
    api.keymap.set = function(mode, keys, rhs, opts) only_functions(rhs) return set(mode, keys, rhs, opts) end
  end
  -- Questions name the plugin asking. rt.page acts as this plugin, only with
  -- the pages permission, and the browser checks the site again when it runs.
  local own = ui_for(name)
  api.ui = copy(rt.ui)
  api.ui.select, api.ui.input = own.select, own.input
  api.ui.float = rt._float_for(name)
  api.ui.panel = rt._panel_for(name)
  if permissions.pages and #permissions.pages > 0 then
    api.page = page_for(name)
  else
    api.page = nil
  end
  -- Each plugin's stores are its own.
  api.store = function(store) return rt.store(name .. "--" .. (store or "data")) end
  if permissions.settings then env.c = c end
  env.rt = api
  env._G = env
  return env
end

-- Nobody changes the string methods for everyone else.
getmetatable("").__metatable = false

-- rt.ui: pickers and questions in the prompt area, as vim.ui has them.
function ui_for(source)
  local ui = {}
  function ui.select(items, opts, on_choice)
    if type(opts) == "function" then opts, on_choice = {}, opts end
    opts = opts or {}
    if #items == 0 then return on_choice(nil, nil) end
    local labels = {}
    for i, item in ipairs(items) do
      labels[i] = tostring(opts.format and opts.format(item) or item)
    end
    rt._ask(source, "select", opts.prompt or "Pick one", labels, function(answer)
      local i = tonumber(answer)
      if i then on_choice(items[i + 1], i + 1) else on_choice(nil, nil) end
    end)
  end
  function ui.input(opts, on_confirm)
    if type(opts) == "function" then opts, on_confirm = {}, opts end
    opts = opts or {}
    rt._ask(source, "input", opts.prompt or "Answer", { default = opts.default, secret = opts.secret }, on_confirm)
  end
  return ui
end

-- rt.page: the current tab's page, as `source` (nil for config.lua).
function page_for(source)
  return {
    type = function(text) rt._page(source, "type", text) end,
    key = function(keys) rt._page(source, "key", keys) end,
    fill_login = function(login) rt._page(source, "fill_login", login) end,
  }
end
rt.ui = ui_for(nil)
rt.page = page_for(nil)

-- rt.store(name): data kept between runs, saved on every change.
local stores = {}
function rt.store(name)
  if stores[name] then return stores[name] end
  local data = rt._store_load(name)
  local store = {}
  function store.get(key) return data[key] end
  function store.set(key, value)
    data[key] = value
    rt._store_save(name, data)
  end
  function store.all() return data end
  function store.clear()
    data = {}
    rt._store_save(name, data)
  end
  stores[name] = store
  return store
end

-- rt.keymap.set(mode, keys, fn_or_command, { desc = "…" }), as vim.keymap.set:
-- one mode or a list; desc shows in the key hints.
rt.keymap = {}
local function modes_of(mode)
  if mode == nil then return { "normal" } end
  if type(mode) == "string" then return { mode } end
  return mode
end
function rt.keymap.set(mode, keys, rhs, opts)
  for _, m in ipairs(modes_of(mode)) do
    local command = rt.bind(keys, rhs, m)
    if opts and opts.desc then rt._describe(command, opts.desc) end
  end
end
function rt.keymap.del(mode, keys)
  for _, m in ipairs(modes_of(mode)) do rt.unbind(keys, m) end
end

-- rt.ui.float(opts): a box of text over the page; see rt.meta.lua. A
-- plugin's floats carry its name, so they can't pass for riptide's own.
local floats, last_float = {}, 0
local function float_spec(opts)
  local spec = {}
  for _, key in ipairs({ "title", "lines", "width", "position", "timeout" }) do spec[key] = opts[key] end
  spec.keys = {}
  for key, fn in pairs(opts.keys or {}) do
    if type(key) ~= "string" or type(fn) ~= "function" then error("keys maps key names to functions", 3) end
    spec.keys[#spec.keys + 1] = key
  end
  return spec
end
function rt._float_for(source)
  return function(opts)
    if type(opts) ~= "table" then error("rt.ui.float takes a table", 2) end
    last_float = last_float + 1
    local id = last_float
    local handle = { id = id }
    local current = opts
    floats[id] = { opts = opts, handle = handle }
    function handle:update(changes)
      if not floats[id] then return end
      local merged = {}
      for k, v in pairs(current) do merged[k] = v end
      for k, v in pairs(changes or {}) do merged[k] = v end
      current = merged
      floats[id].opts = merged
      rt._float(id, source or "", float_spec(merged))
    end
    function handle:close()
      if floats[id] then
        floats[id] = nil
        rt._float_close(id)
      end
    end
    function handle:is_open() return floats[id] ~= nil end
    rt._float(id, source or "", float_spec(opts))
    return handle
  end
end
function rt._float_key(id, key)
  local float = floats[id]
  local fn = float and float.opts.keys and float.opts.keys[key]
  if fn then fn(float.handle) end
end
function rt._float_closed(id)
  local float = floats[id]
  floats[id] = nil
  if float and float.opts.on_close then float.opts.on_close() end
end
rt.ui.float = rt._float_for(nil)

-- rt.ui.panel(opts): lines beside or below the page; see rt.meta.lua.
local panels, last_panel = {}, 0
local function panel_spec(opts)
  local spec = {}
  for _, key in ipairs({ "title", "lines", "side", "size" }) do spec[key] = opts[key] end
  spec.keys = {}
  for key, fn in pairs(opts.keys or {}) do
    if type(key) ~= "string" or type(fn) ~= "function" then error("keys maps key names to functions", 3) end
    spec.keys[#spec.keys + 1] = key
  end
  return spec
end
function rt._panel_for(source)
  return function(opts)
    if type(opts) ~= "table" then error("rt.ui.panel takes a table", 2) end
    last_panel = last_panel + 1
    local id = last_panel
    local handle = { id = id }
    panels[id] = { opts = opts, handle = handle }
    function handle:update(changes)
      local panel = panels[id]
      if not panel then return end
      local merged = {}
      for k, v in pairs(panel.opts) do merged[k] = v end
      for k, v in pairs(changes or {}) do merged[k] = v end
      panel.opts = merged
      rt._panel(id, source or "", panel_spec(merged))
    end
    function handle:close()
      if panels[id] then
        panels[id] = nil
        rt._panel_close(id)
      end
    end
    function handle:focus() if panels[id] then rt._panel_focus(id) end end
    function handle:is_open() return panels[id] ~= nil end
    rt._panel(id, source or "", panel_spec(opts))
    return handle
  end
end
function rt._panel_key(id, key, line)
  local panel = panels[id]
  local fn = panel and panel.opts.keys and panel.opts.keys[key]
  if fn then fn(panel.handle, line) end
end
function rt._panel_closed(id)
  local panel = panels[id]
  panels[id] = nil
  if panel and panel.opts.on_close then panel.opts.on_close() end
end
rt.ui.panel = rt._panel_for(nil)
"##;

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
    /// `rt.keymap.set`'s `desc`, by the command a key runs (`lua-call 3`).
    descriptions: std::collections::BTreeMap<String, String>,
    /// Plugins from `rt.pack.add`, in order.
    specs: Vec<PluginSpec>,
    /// Plugins loaded so far, whose modules `require` finds.
    plugins: Vec<LoadedPlugin>,
}

/// A plugin `rt.pack.add` asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginSpec {
    pub name: String,
    /// A git URL; empty for a local folder.
    pub src: String,
    /// A local folder, already expanded.
    pub dir: Option<std::path::PathBuf>,
    /// A tag, branch or commit to use.
    pub version: String,
    /// Run without the sandbox, with all permissions.
    pub trusted: bool,
    /// Load only once one of these events fires, commands runs or keys are
    /// pressed; with none of them, load at startup.
    pub events: Vec<String>,
    pub commands: Vec<String>,
    pub keys: Vec<(Mode, String)>,
    /// One of the plugins that ship with riptide, found by name.
    pub builtin: bool,
}

impl PluginSpec {
    /// Whether it waits for an event, command or key to load.
    pub fn is_lazy(&self) -> bool {
        !(self.events.is_empty() && self.commands.is_empty() && self.keys.is_empty())
    }
}

#[derive(Clone)]
struct LoadedPlugin {
    name: String,
    dir: std::path::PathBuf,
    /// Its globals: a sandbox, or the shared ones when trusted.
    env: mlua::Table,
    /// The sites `rt.page` may act on; `None` when trusted.
    pages: Option<Vec<String>>,
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
    /// `rt.open`: open a URL, never through a command line (a URL could
    /// otherwise carry `;;` and a second command).
    Open { url: String, target: OpenTarget },
    /// `rt.defer`/`rt.every`: call timer `id` back after `ms` milliseconds.
    Timer { id: u32, ms: u32 },
    /// `rt.spawn`: run a program; [`spawned`] hands the result to `callback`.
    Spawn(SpawnRequest),
    /// `rt.bind` after loading: bind for this run only, never saved.
    Bind {
        mode: Mode,
        keys: String,
        command: String,
    },
    /// `rt.unbind` after loading, never saved.
    Unbind { mode: Mode, keys: String },
    /// `rt.ui.select`/`rt.ui.input`: ask in the prompt area; [`answered`]
    /// hands the answer to callback `id`. `source` names who asks.
    Ask {
        id: u32,
        source: String,
        prompt: String,
        ask: Ask,
    },
    /// `rt.page.*` on the current tab, never through a command line, so the
    /// text stays out of history, `.` and messages. `pages` is where the
    /// plugin may act (`None`: config.lua or a trusted plugin), checked
    /// against the tab when it runs.
    Page {
        plugin: Option<String>,
        pages: Option<Vec<String>>,
        request: PageRequest,
    },
    /// `rt.ui.float` or a float's `update`: show float `id`. `source` is the
    /// plugin that drew it, empty for `config.lua`.
    Float {
        id: u32,
        source: String,
        spec: FloatSpec,
    },
    /// A float's `close`.
    FloatClose { id: u32 },
    /// `rt.ui.panel` or a panel's `update`.
    Panel {
        id: u32,
        source: String,
        spec: PanelSpec,
    },
    /// A panel's `close`.
    PanelClose { id: u32 },
    /// A panel's `focus`: its keys work until Escape.
    PanelFocus { id: u32 },
}

/// What `rt.ui` asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Pick one of these labels.
    Select(Vec<String>),
    /// Type an answer; `secret` masks it.
    Input { default: String, secret: bool },
}

/// What `rt.page` does to the current page.
#[derive(Clone, PartialEq, Eq)]
pub enum PageRequest {
    /// Type text into the focused field.
    Type(String),
    /// Press keys, as `:fake-key` does.
    Key(String),
    /// Fill the page's login form, only while the tab is still on `host`.
    FillLogin {
        host: String,
        username: Option<String>,
        password: Option<String>,
        submit: bool,
    },
}

// Debug output never shows what's typed or filled.
impl std::fmt::Debug for PageRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Type(_) => f.write_str("Type(..)"),
            Self::Key(keys) => write!(f, "Key({keys:?})"),
            Self::FillLogin { host, submit, .. } => {
                write!(f, "FillLogin {{ host: {host:?}, submit: {submit}, .. }}")
            }
        }
    }
}

/// What `rt.ui.float` draws: text in lines of highlighted chunks, never HTML.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct FloatSpec {
    pub title: String,
    /// Each line's chunks: text and an optional highlight group from [`FLOAT_GROUPS`].
    pub lines: Lines,
    /// The widest it gets, in characters.
    pub width: u32,
    /// One of [`FLOAT_POSITIONS`].
    pub position: String,
    /// Close by itself after this many milliseconds.
    pub timeout: Option<u32>,
    /// The keys it takes while it's the newest float with keys.
    pub keys: Vec<String>,
}

/// Highlight groups a float's text can use; the theme gives their colours.
pub const FLOAT_GROUPS: &[&str] = &[
    "title", "muted", "accent", "match", "url", "key", "info", "warning", "error",
];

pub const FLOAT_POSITIONS: &[&str] = &["center", "top", "bottom", "top-right", "bottom-right"];

const FLOAT_MAX_LINES: usize = 500;

/// Text lines for a float or panel: each a list of `(text, highlight)` chunks.
pub type Lines = Vec<Vec<(String, Option<String>)>>;

/// `opts.lines` for `what` (`rt.ui.float`), at most `max` lines.
fn lines_from_lua(what: &str, table: &mlua::Table, max: usize) -> mlua::Result<Lines> {
    let bad = |text: String| mlua::Error::runtime(format!("{what}: {text}"));
    let shape = "a line is a string or a list of { text, highlight }";
    let chunk = |value: mlua::Value| -> mlua::Result<(String, Option<String>)> {
        match value {
            mlua::Value::String(s) => Ok((s.to_str()?.to_string(), None)),
            mlua::Value::Table(t) => {
                let text: String = t.get(1)?;
                let group: Option<String> = t.get(2)?;
                if let Some(g) = &group
                    && !FLOAT_GROUPS.contains(&g.as_str())
                {
                    return Err(bad(format!(
                        "no highlight {g:?}; use {}",
                        FLOAT_GROUPS.join(", ")
                    )));
                }
                Ok((text, group))
            }
            _ => Err(bad(shape.into())),
        }
    };
    let mut lines = Vec::new();
    let raw: Option<mlua::Table> = table.get("lines")?;
    for line in raw.iter().flat_map(|t| t.sequence_values::<mlua::Value>()) {
        match line? {
            mlua::Value::String(s) => {
                for text in s.to_str()?.split('\n') {
                    lines.push(vec![(text.to_string(), None)]);
                }
            }
            mlua::Value::Table(chunks) => {
                let line = chunks
                    .sequence_values::<mlua::Value>()
                    .map(|c| chunk(c?))
                    .collect::<mlua::Result<_>>()?;
                lines.push(line);
            }
            _ => return Err(bad(shape.into())),
        }
        if lines.len() > max {
            return Err(bad(format!("at most {max} lines")));
        }
    }
    Ok(lines)
}

/// `opts.keys`' names (the prelude passes the names, keeping the functions): each one key.
fn keys_from_lua(what: &str, table: &mlua::Table) -> mlua::Result<Vec<String>> {
    let mut keys = Vec::new();
    for key in table.get::<Vec<String>>("keys")? {
        match Key::parse_sequence(&key) {
            Ok(seq) if seq.len() == 1 => keys.push(key),
            _ => {
                return Err(mlua::Error::runtime(format!(
                    "{what}: {key:?} isn't one key"
                )));
            }
        }
    }
    Ok(keys)
}

impl FloatSpec {
    fn from_lua(table: &mlua::Table) -> mlua::Result<Self> {
        let what = "rt.ui.float";
        let title: Option<String> = table.get("title")?;
        let width: Option<u32> = table.get("width")?;
        let position: Option<String> = table.get("position")?;
        let position = position.unwrap_or_else(|| "center".into());
        if !FLOAT_POSITIONS.contains(&position.as_str()) {
            return Err(mlua::Error::runtime(format!(
                "{what}: position is one of {}",
                FLOAT_POSITIONS.join(", ")
            )));
        }
        Ok(Self {
            title: title.unwrap_or_default(),
            lines: lines_from_lua(what, table, FLOAT_MAX_LINES)?,
            width: width.unwrap_or(60).clamp(10, 200),
            position,
            timeout: table.get("timeout")?,
            keys: keys_from_lua(what, table)?,
        })
    }
}

/// What `rt.ui.panel` draws: a list of lines beside or below the page.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PanelSpec {
    pub title: String,
    pub lines: Lines,
    /// `left`, `right` or `bottom`.
    pub side: String,
    /// Its width beside the page, or height below it, in pixels.
    pub size: u32,
    /// The keys it takes while focused; `j`/`k` move its cursor.
    pub keys: Vec<String>,
}

pub const PANEL_SIDES: &[&str] = &["left", "right", "bottom"];

const PANEL_MAX_LINES: usize = 5000;

impl PanelSpec {
    fn from_lua(table: &mlua::Table) -> mlua::Result<Self> {
        let what = "rt.ui.panel";
        let side: Option<String> = table.get("side")?;
        let side = side.unwrap_or_else(|| "left".into());
        if !PANEL_SIDES.contains(&side.as_str()) {
            return Err(mlua::Error::runtime(format!(
                "{what}: side is one of {}",
                PANEL_SIDES.join(", ")
            )));
        }
        let size: Option<u32> = table.get("size")?;
        let default = if side == "bottom" { 200 } else { 300 };
        Ok(Self {
            title: table.get::<Option<String>>("title")?.unwrap_or_default(),
            lines: lines_from_lua(what, table, PANEL_MAX_LINES)?,
            side,
            size: size.unwrap_or(default).clamp(80, 2000),
            keys: keys_from_lua(what, table)?,
        })
    }
}

/// A key pressed while panel `id` is focused, its cursor on `line` (from 1).
pub fn panel_key(id: u32, key: &str, line: u32, context: &Context) -> Result<Vec<Action>, String> {
    let key = key.to_string();
    run_guarded(context, move |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
        api.get::<mlua::Function>("_panel_key")?
            .call::<()>((id, key, line))
    })
}

/// Panel `id` was closed by riptide (its window closed): its `on_close`.
pub fn panel_closed(id: u32, context: &Context) -> Result<Vec<Action>, String> {
    run_guarded(context, move |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
        api.get::<mlua::Function>("_panel_closed")?.call::<()>(id)
    })
}

/// A key pressed while float `id` takes keys: call its function.
pub fn float_key(id: u32, key: &str, context: &Context) -> Result<Vec<Action>, String> {
    let key = key.to_string();
    run_guarded(context, move |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
        api.get::<mlua::Function>("_float_key")?
            .call::<()>((id, key))
    })
}

/// Float `id` was closed by riptide (Escape, its timeout, its window): its `on_close`.
pub fn float_closed(id: u32, context: &Context) -> Result<Vec<Action>, String> {
    run_guarded(context, move |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
        api.get::<mlua::Function>("_float_closed")?.call::<()>(id)
    })
}

/// Where `rt.open` opens a URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Current,
    Tab,
    Background,
    Window,
    Private,
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
    let table = table.to_string();
    run_guarded(context, move |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
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
    })
}

/// Run `f` in the runtime with `context`, stopping it after
/// [`CALLBACK_LIMIT`], and return the actions it asked for.
fn run_guarded(
    context: &Context,
    f: impl FnOnce(&Lua, &Rc<RefCell<State>>) -> mlua::Result<()>,
) -> Result<Vec<Action>, String> {
    run_limited(CALLBACK_LIMIT, context, f)
}

/// [`run_guarded`] with its own time limit.
fn run_limited(
    limit: std::time::Duration,
    context: &Context,
    f: impl FnOnce(&Lua, &Rc<RefCell<State>>) -> mlua::Result<()>,
) -> Result<Vec<Action>, String> {
    RUNTIME.with(|r| {
        let runtime = r.borrow();
        let Some(rt) = runtime.as_ref() else {
            return Err("config.lua isn't loaded".to_string());
        };
        rt.state.borrow_mut().context = context.clone();
        // A callback runs on the browser's UI thread: one that doesn't
        // finish (a loop) would freeze the browser, so stop it instead.
        let deadline = std::time::Instant::now() + limit;
        let _ = rt.lua.set_hook(
            mlua::HookTriggers::new().every_nth_instruction(10_000),
            move |_, _| {
                if std::time::Instant::now() > deadline {
                    Err(mlua::Error::runtime(format!(
                        "stopped after {} ms; long work belongs in rt.spawn or a timer",
                        limit.as_millis()
                    )))
                } else {
                    Ok(mlua::VmState::Continue)
                }
            },
        );
        let result = f(&rt.lua, &rt.state);
        rt.lua.remove_hook();
        let actions = std::mem::take(&mut rt.state.borrow_mut().actions);
        result
            .map(|()| actions)
            .map_err(|e| tidy_error(&e.to_string(), &rt.config_dir))
    })
}

/// Status bar widgets get this long each time the bar is drawn.
const WIDGET_LIMIT: std::time::Duration = std::time::Duration::from_millis(50);

/// The texts of the `rt.statusbar.widget`s named, and the errors of any
/// that failed (which are removed). What else they ask for is ignored.
pub fn widget_texts(
    names: &[String],
    context: &Context,
) -> (std::collections::BTreeMap<String, String>, Vec<String>) {
    let out = RefCell::new(Default::default());
    let result = run_limited(WIDGET_LIMIT, context, |lua, _| {
        let api: mlua::Table = lua.globals().get("rt")?;
        let texts: mlua::Function = api.get("_widget_texts")?;
        *out.borrow_mut() = texts.call(names.to_vec())?;
        Ok(())
    });
    let (texts, mut errors): (std::collections::BTreeMap<String, String>, Vec<String>) =
        out.into_inner();
    if let Err(e) = result {
        errors.push(e);
    }
    (texts, errors)
}

/// The plugins `rt.pack.add` asked for, in order.
pub fn plugin_specs() -> Vec<PluginSpec> {
    RUNTIME.with(|r| {
        r.borrow()
            .as_ref()
            .map(|rt| rt.state.borrow().specs.clone())
            .unwrap_or_default()
    })
}

/// Load plugin `name` from `dir`: give it its globals (a sandbox allowing
/// `permissions`, or the shared ones when `trusted`), run its `plugin/*.lua`,
/// then its spec's `opts` or `config`.
pub fn load_plugin(
    name: &str,
    dir: &Path,
    permissions: &crate::plugins::Permissions,
    trusted: bool,
    context: &Context,
) -> Result<Vec<Action>, String> {
    let (name, dir, permissions) = (name.to_string(), dir.to_path_buf(), permissions.clone());
    run_guarded(context, move |lua, state| {
        if state.borrow().plugins.iter().any(|p| p.name == name) {
            return Ok(());
        }
        let env = if trusted {
            lua.globals()
        } else {
            let s = state.clone();
            let require = lua.create_function(move |lua, module: String| {
                plugin_module(lua, &s, &module)?.ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "module {module:?} not found; plugins can require their own modules and other plugins'"
                    ))
                })
            })?;
            let api: mlua::Table = lua.globals().get("rt")?;
            let sandbox: mlua::Function = api.get("_sandbox")?;
            sandbox.call::<mlua::Table>((name.as_str(), lua.to_value(&permissions)?, require))?
        };
        state.borrow_mut().plugins.push(LoadedPlugin {
            name: name.clone(),
            dir: dir.clone(),
            env: env.clone(),
            pages: (!trusted).then(|| permissions.pages.clone()),
        });
        let mut scripts: Vec<std::path::PathBuf> = std::fs::read_dir(dir.join("plugin"))
            .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        scripts.retain(|p| p.extension().is_some_and(|e| e == "lua"));
        scripts.sort();
        for script in scripts {
            let source = std::fs::read_to_string(&script).map_err(mlua::Error::external)?;
            let file = script
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            lua.load(source)
                .set_name(format!("@{name}/plugin/{file}"))
                .set_environment(env.clone())
                .exec()?;
        }
        let api: mlua::Table = lua.globals().get("rt")?;
        let ready: mlua::Function = api.get("_pack_ready")?;
        ready.call::<()>(name.as_str())
    })
}

/// A loaded plugin's module (`lua/<path>.lua` or `lua/<path>/init.lua`),
/// run once in its plugin's globals and kept.
fn plugin_module(
    lua: &Lua,
    state: &Rc<RefCell<State>>,
    module: &str,
) -> mlua::Result<Option<Value>> {
    let cache: mlua::Table = lua.named_registry_value("rt_plugin_modules")?;
    let cached: Value = cache.get(module)?;
    if !cached.is_nil() {
        return Ok(Some(cached));
    }
    let path = module.replace('.', "/");
    if path.split('/').any(|part| part.is_empty() || part == "..") {
        return Ok(None);
    }
    let plugins = state.borrow().plugins.clone();
    for plugin in plugins {
        for relative in [format!("lua/{path}.lua"), format!("lua/{path}/init.lua")] {
            let Ok(source) = std::fs::read_to_string(plugin.dir.join(&relative)) else {
                continue;
            };
            let value: Value = lua
                .load(source)
                .set_name(format!("@{}/{relative}", plugin.name))
                .set_environment(plugin.env.clone())
                .call(module)?;
            let value = if value.is_nil() {
                Value::Boolean(true)
            } else {
                value
            };
            cache.set(module, value.clone())?;
            return Ok(Some(value));
        }
    }
    Ok(None)
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

/// Call an `rt.ui` callback with the answer (`None` when cancelled), once.
pub fn answered(id: u32, answer: Option<String>, context: &Context) -> Result<Vec<Action>, String> {
    let prepared = RUNTIME.with(|r| -> Option<mlua::Result<mlua::MultiValue>> {
        let runtime = r.borrow();
        let rt = runtime.as_ref()?;
        Some((|| {
            let value = match &answer {
                Some(text) => mlua::Value::String(rt.lua.create_string(text)?),
                None => mlua::Value::Nil,
            };
            Ok(mlua::MultiValue::from_vec(vec![value]))
        })())
    });
    let args = match prepared {
        None => return Ok(Vec::new()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(args)) => args,
    };
    let key = mlua::Value::Integer(id.into());
    let actions = invoke("_answers", key.clone(), args, context);
    RUNTIME.with(|r| {
        if let Some(rt) = r.borrow().as_ref() {
            let api: mlua::Result<mlua::Table> = rt.lua.globals().get("rt");
            if let Ok(table) = api.and_then(|api| api.get::<mlua::Table>("_answers")) {
                let _ = table.set(key, mlua::Value::Nil);
            }
        }
    });
    actions
}

/// What `rt.keymap.set`'s `desc` says a key's command does, for key hints.
pub fn describe(command: &str) -> Option<String> {
    RUNTIME.with(|r| {
        r.borrow()
            .as_ref()
            .and_then(|rt| rt.state.borrow().descriptions.get(command).cloned())
    })
}

/// Completions for the arguments of command `name`, from its `complete`
/// function, as `(text, description)`. Called while the browser is busy
/// completing, so the function gets no context and can't act.
pub fn complete_command(name: &str, arglead: &str) -> Vec<(String, String)> {
    let found = RUNTIME.with(|r| -> mlua::Result<Vec<(String, String)>> {
        let runtime = r.borrow();
        let Some(rt) = runtime.as_ref() else {
            return Ok(Vec::new());
        };
        let api: mlua::Table = rt.lua.globals().get("rt")?;
        let Some(complete) = api
            .get::<mlua::Table>("_completers")?
            .get::<Option<mlua::Function>>(name)?
        else {
            return Ok(Vec::new());
        };
        let items: mlua::Table = complete.call(arglead)?;
        items
            .sequence_values::<Value>()
            .map(|item| match item? {
                Value::String(s) => Ok((s.to_str()?.to_string(), String::new())),
                Value::Table(t) => Ok((
                    t.get("name")?,
                    t.get::<Option<String>>("desc")?.unwrap_or_default(),
                )),
                _ => Err(mlua::Error::runtime(
                    "completion items are strings or { name = …, desc = … }",
                )),
            })
            .collect()
    });
    found.unwrap_or_else(|e| {
        tracing::warn!("completion for :{name}: {e}");
        Vec::new()
    })
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
        descriptions: Default::default(),
        specs: Vec::new(),
        plugins: Vec::new(),
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
fn searcher(
    lua: &Lua,
    config_dir: &Path,
    state: Rc<RefCell<State>>,
) -> mlua::Result<mlua::Function> {
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
        // Then the loaded plugins' modules, run in their own globals.
        if let Some(value) = plugin_module(lua, &state, &module)? {
            let loader = lua.create_function(move |_, ()| Ok(value.clone()))?;
            return Ok(mlua::Value::Function(loader));
        }
        let first = module.split('.').next().unwrap_or_default();
        let pending = state.borrow().specs.iter().any(|s| s.name == first);
        let tried = if pending {
            format!(
                "\n\tplugin {first:?} loads after config.lua; set it up in rt.pack.add with opts or config"
            )
        } else {
            format!("\n\tno file '{file}' or 'lua/{file}' in the config dir or a loaded plugin")
        };
        Ok(mlua::Value::String(lua.create_string(tried)?))
    })
}

/// `~/x` as a path in the home folder.
fn expand_home(path: &str, _data_dir: &Path) -> std::path::PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
        _ => std::path::PathBuf::from(path),
    }
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

/// `rt._pack_spec`'s arguments: name, src, dir, version, trusted, events,
/// commands, `{ keys, mode }` pairs and whether it's builtin.
type PackSpecArgs = (
    String,
    String,
    Option<String>,
    Option<String>,
    bool,
    Vec<String>,
    Vec<String>,
    Vec<Vec<String>>,
    bool,
);

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
    api.set("_completers", lua.create_table()?)?;
    api.set("_commands", lua.create_table()?)?;
    let events = lua.create_table()?;
    for (name, _) in EVENTS {
        events.push(*name)?;
    }
    api.set("EVENTS", events)?;
    api.set("_spawned", lua.create_table()?)?;
    api.set("_answers", lua.create_table()?)?;

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
                let mut state = s.borrow_mut();
                if state.loaded {
                    // From a callback: the same as typing :bind.
                    state.actions.push(Action::Bind {
                        mode,
                        keys,
                        command: command.clone(),
                    });
                } else {
                    state.ops.push(ConfigOp::Bind {
                        mode,
                        keys,
                        command: command.clone(),
                    });
                }
                Ok(command)
            },
        )?,
    )?;

    let s = state.clone();
    api.set(
        "_describe",
        lua.create_function(move |_, (command, description): (String, String)| {
            s.borrow_mut().descriptions.insert(command, description);
            Ok(())
        })?,
    )?;

    let s = state.clone();
    api.set(
        "command",
        lua.create_function(
            move |lua, (name, f, options): (String, mlua::Function, Value)| {
                // A description, or { desc = "…", complete = function(arglead) … end }.
                let (description, complete) = match options {
                    Value::Nil => (None, None),
                    Value::String(text) => (Some(text.to_str()?.to_string()), None),
                    Value::Table(t) => (
                        t.get::<Option<String>>("desc")?,
                        t.get::<Option<mlua::Function>>("complete")?,
                    ),
                    _ => {
                        return Err(mlua::Error::runtime(
                            "rt.command's third argument is a description or an options table",
                        ));
                    }
                };
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
                api.get::<mlua::Table>("_completers")?
                    .set(name.clone(), complete)?;
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

    // rt.store: JSON files under <data>/plugin-data, one per name.
    let store_dir = paths.data_dir.join("plugin-data");
    let store_path = move |name: &str| -> mlua::Result<std::path::PathBuf> {
        let valid = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(mlua::Error::runtime(format!(
                "store name {name:?}: use letters, digits, - and _"
            )));
        }
        Ok(store_dir.join(format!("{name}.json")))
    };
    let load_path = store_path.clone();
    api.set(
        "_store_load",
        lua.create_function(move |lua, name: String| {
            let path = load_path(&name)?;
            let json: serde_json::Value = match std::fs::read_to_string(&path) {
                Ok(text) => serde_json::from_str(&text)
                    .map_err(|e| mlua::Error::runtime(format!("{}: {e}", path.display())))?,
                Err(_) => serde_json::json!({}),
            };
            lua.to_value(&json)
        })?,
    )?;
    api.set(
        "_store_save",
        lua.create_function(move |lua, (name, data): (String, Value)| {
            let path = store_path(&name)?;
            let json: serde_json::Value = lua.from_value(data)?;
            let text = serde_json::to_string_pretty(&json).map_err(mlua::Error::external)?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(mlua::Error::external)?;
            }
            // Write a temporary file and rename it, so a crash never leaves half a file.
            let partial = path.with_extension("json.part");
            std::fs::write(&partial, text).map_err(mlua::Error::external)?;
            std::fs::rename(&partial, &path).map_err(mlua::Error::external)
        })?,
    )?;

    api.set(
        "_plugin_name",
        lua.create_function(|_, src: String| Ok(crate::plugins::name_from(&src)))?,
    )?;

    let s = state.clone();
    let data_dir = paths.data_dir.clone();
    api.set(
        "_pack_spec",
        lua.create_function(
            move |_, (name, src, dir, version, trusted, events, commands, keys, builtin): PackSpecArgs| {
                let bad =
                    |what: String| Err(mlua::Error::runtime(format!("plugin {name:?}: {what}")));
                if !crate::plugins::valid_name(&name) {
                    return Err(mlua::Error::runtime(format!(
                        "plugin name {name:?}: use letters, digits, - and _ (set name = \"…\")"
                    )));
                }
                if let Some(event) = events.iter().find(|e| !EVENTS.iter().any(|(n, _)| n == e)) {
                    return bad(format!("no event {event:?}"));
                }
                if let Some(command) = commands.iter().find(|c| !crate::plugins::valid_name(c)) {
                    return bad(format!("{command:?} isn't a command name"));
                }
                let mut parsed = Vec::new();
                for pair in keys {
                    let [keys, mode]: [String; 2] = pair
                        .try_into()
                        .map_err(|_| mlua::Error::runtime("keys: { \"<keys>\", mode = \"…\" }"))?;
                    if let Err(e) = Key::parse_sequence(&keys) {
                        return bad(format!("keys {keys:?}: {e}"));
                    }
                    parsed.push((mode_arg(Some(mode))?, keys));
                }
                let mut state = s.borrow_mut();
                if state.specs.iter().any(|spec| spec.name == name) {
                    return Err(mlua::Error::runtime(format!(
                        "plugin {name:?} is added twice"
                    )));
                }
                let dir = dir.map(|d| expand_home(&d, &data_dir));
                state.specs.push(PluginSpec {
                    name,
                    src,
                    dir,
                    version: version.unwrap_or_default(),
                    trusted,
                    events,
                    commands,
                    keys: parsed,
                    builtin,
                });
                Ok(())
            },
        )?,
    )?;

    let s = state.clone();
    api.set(
        "_float",
        lua.create_function(move |_, (id, source, spec): (u32, String, mlua::Table)| {
            let spec = FloatSpec::from_lua(&spec)?;
            s.borrow_mut()
                .actions
                .push(Action::Float { id, source, spec });
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "_panel",
        lua.create_function(move |_, (id, source, spec): (u32, String, mlua::Table)| {
            let spec = PanelSpec::from_lua(&spec)?;
            s.borrow_mut()
                .actions
                .push(Action::Panel { id, source, spec });
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "_panel_close",
        lua.create_function(move |_, id: u32| {
            s.borrow_mut().actions.push(Action::PanelClose { id });
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "_panel_focus",
        lua.create_function(move |_, id: u32| {
            s.borrow_mut().actions.push(Action::PanelFocus { id });
            Ok(())
        })?,
    )?;
    let s = state.clone();
    api.set(
        "_float_close",
        lua.create_function(move |_, id: u32| {
            s.borrow_mut().actions.push(Action::FloatClose { id });
            Ok(())
        })?,
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
            let target = match target.as_deref() {
                None | Some("current") => OpenTarget::Current,
                Some("tab") => OpenTarget::Tab,
                Some("tab-bg") => OpenTarget::Background,
                Some("window") => OpenTarget::Window,
                Some("private") => OpenTarget::Private,
                Some(other) => {
                    return Err(mlua::Error::runtime(format!("unknown target {other:?}")));
                }
            };
            // A javascript: URL would run script in the page.
            if url
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("javascript:")
            {
                return Err(mlua::Error::runtime(
                    "rt.open doesn't open javascript: URLs",
                ));
            }
            s.borrow_mut().actions.push(Action::Open { url, target });
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

    let s = state.clone();
    api.set(
        "unbind",
        lua.create_function(move |_, (keys, mode): (String, Option<String>)| {
            Key::parse_sequence(&keys).map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let mode = mode_arg(mode)?;
            let mut state = s.borrow_mut();
            if state.loaded {
                state.actions.push(Action::Unbind { mode, keys });
            } else {
                state.ops.push(ConfigOp::Unbind { mode, keys });
            }
            Ok(())
        })?,
    )?;
    // rt.ui.select/input (the prelude) ask through here; who asks is shown.
    let s = state.clone();
    api.set(
        "_ask",
        lua.create_function(
            move |lua,
                  (source, kind, prompt, data, callback): (
                Option<String>,
                String,
                String,
                Value,
                mlua::Function,
            )| {
                let ask = match (kind.as_str(), data) {
                    ("select", Value::Table(items)) => {
                        let items: Vec<String> = items
                            .sequence_values::<String>()
                            .collect::<mlua::Result<_>>()?;
                        if items.len() > rt_core::prompt::SELECT_MAX {
                            return Err(mlua::Error::runtime(format!(
                                "rt.ui.select offers at most {} items",
                                rt_core::prompt::SELECT_MAX
                            )));
                        }
                        Ask::Select(items)
                    }
                    ("input", Value::Table(opts)) => Ask::Input {
                        default: opts.get::<Option<String>>("default")?.unwrap_or_default(),
                        secret: opts.get::<Option<bool>>("secret")?.unwrap_or(false),
                    },
                    _ => return Err(mlua::Error::runtime("rt.ui: bad arguments")),
                };
                let id = {
                    let mut state = s.borrow_mut();
                    state.next_callback += 1;
                    state.next_callback - 1
                };
                let api: mlua::Table = lua.globals().get("rt")?;
                api.get::<mlua::Table>("_answers")?.set(id, callback)?;
                let source =
                    source.map_or_else(|| "config.lua".to_string(), |p| format!("Plugin {p}"));
                s.borrow_mut().actions.push(Action::Ask {
                    id,
                    source,
                    prompt,
                    ask,
                });
                Ok(())
            },
        )?,
    )?;
    // rt.page (the prelude) acts through here, as config.lua (`nil`) or a
    // plugin, whose `pages` permission goes with the request.
    let s = state.clone();
    api.set(
        "_page",
        lua.create_function(
            move |_, (plugin, kind, arg): (Option<String>, String, Value)| {
                // Errors never echo the argument: it may be a password.
                let text = |what: &str| match &arg {
                    Value::String(t) => Ok(t.to_str()?.to_string()),
                    _ => Err(mlua::Error::runtime(format!("rt.page.{kind} takes {what}"))),
                };
                let request = match kind.as_str() {
                    "type" => PageRequest::Type(text("text")?),
                    "key" => {
                        let keys = text("keys")?;
                        Key::parse_sequence(&keys)
                            .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                        PageRequest::Key(keys)
                    }
                    "fill_login" => {
                        let Value::Table(login) = &arg else {
                            return Err(mlua::Error::runtime(
                                "rt.page.fill_login takes { host, username, password, submit }",
                            ));
                        };
                        let host: Option<String> = login.get("host")?;
                        let Some(host) = host.filter(|h| !h.is_empty()) else {
                            return Err(mlua::Error::runtime(
                                "rt.page.fill_login needs the host the login is for",
                            ));
                        };
                        PageRequest::FillLogin {
                            host,
                            username: login.get("username")?,
                            password: login.get("password")?,
                            submit: login.get::<Option<bool>>("submit")?.unwrap_or(false),
                        }
                    }
                    _ => return Err(mlua::Error::runtime("rt.page: unknown action")),
                };
                let mut state = s.borrow_mut();
                let pages = match &plugin {
                    None => None,
                    Some(name) => Some(
                        state
                            .plugins
                            .iter()
                            .find(|p| &p.name == name)
                            .map_or_else(Vec::new, |p| {
                                p.pages.clone().unwrap_or_else(|| vec!["*".into()])
                            }),
                    ),
                };
                state.actions.push(Action::Page {
                    plugin,
                    pages,
                    request,
                });
                Ok(())
            },
        )?,
    )?;
    let json = lua.create_table()?;
    json.set(
        "decode",
        lua.create_function(|lua, text: String| {
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| mlua::Error::runtime(format!("rt.json.decode: {e}")))?;
            let options = mlua::serde::SerializeOptions::new()
                .serialize_none_to_null(false)
                .serialize_unit_to_null(false);
            lua.to_value_with(&value, options)
        })?,
    )?;
    json.set(
        "encode",
        lua.create_function(|lua, value: Value| {
            let value: serde_json::Value = lua.from_value(value)?;
            serde_json::to_string(&value).map_err(mlua::Error::external)
        })?,
    )?;
    api.set("json", json)?;

    // `hb` is the old name, kept so existing configs still load.
    lua.globals().set("rt", api.clone())?;
    lua.globals().set("hb", api)?;

    // Our searcher runs right after the preload table, before package.path.
    let package: mlua::Table = lua.globals().get("package")?;
    let searchers: mlua::Table = package.get("searchers")?;
    searchers.raw_insert(2, searcher(lua, &paths.config_dir, state.clone())?)?;
    lua.set_named_registry_value("rt_plugin_modules", lua.create_table()?)?;

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
    fn floats_carry_checked_text_and_their_plugins_name() {
        let dir = std::env::temp_dir().join(format!("rt-lua-floats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugins/painter/plugin")).unwrap();
        std::fs::write(
            dir.join("plugins/painter/plugin/painter.lua"),
            "rt.ui.float({ lines = { 'from a plugin' } })",
        )
        .unwrap();
        let dir_text = dir.display().to_string();
        std::fs::write(
            dir.join("config.lua"),
            format!(
                r#"
rt.pack.add({{ dir = "{dir_text}/plugins/painter" }})
rt.command("draw", function()
  rt.ui.float({{ title = "T", lines = {{ "a\nb", {{ {{ "c", "error" }}, "d" }} }}, position = "top", keys = {{ q = function() end }} }})
end)
rt.command("bad-position", function() rt.ui.float({{ position = "left" }}) end)
rt.command("bad-highlight", function() rt.ui.float({{ lines = {{ {{ {{ "x", "<b>" }} }} }} }}) end)
rt.command("bad-key", function() rt.ui.float({{ keys = {{ ["ab"] = function() end }} }}) end)
"#
            ),
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        assert_eq!(
            run(&dir.join("config.lua"), &paths, Settings::default()).1,
            None
        );
        let ctx = Context::default();
        let actions = run_command("draw", "", &ctx).unwrap();
        let [Action::Float { source, spec, .. }] = actions.as_slice() else {
            panic!("{actions:?}");
        };
        assert_eq!(source, "");
        assert_eq!(spec.title, "T");
        assert_eq!(spec.position, "top");
        assert_eq!(spec.keys, ["q"]);
        assert_eq!(
            spec.lines,
            [
                vec![("a".to_string(), None)],
                vec![("b".to_string(), None)],
                vec![
                    ("c".to_string(), Some("error".to_string())),
                    ("d".to_string(), None)
                ],
            ]
        );
        for bad in ["bad-position", "bad-highlight", "bad-key"] {
            assert!(run_command(bad, "", &ctx).is_err(), "{bad}");
        }
        // A plugin's float says whose it is, whatever it passes.
        let none = crate::plugins::Permissions::default();
        let actions =
            load_plugin("painter", &dir.join("plugins/painter"), &none, false, &ctx).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Float { source, .. } if source == "painter")),
            "{actions:?}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panels_take_a_side_and_size_and_carry_their_plugins_name() {
        let dir = std::env::temp_dir().join(format!("rt-lua-panels-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugins/tree/plugin")).unwrap();
        std::fs::write(
            dir.join("plugins/tree/plugin/tree.lua"),
            "rt.ui.panel({ side = 'right', lines = { 'a' } })",
        )
        .unwrap();
        let dir_text = dir.display().to_string();
        std::fs::write(
            dir.join("config.lua"),
            format!(
                r#"
rt.pack.add({{ dir = "{dir_text}/plugins/tree" }})
rt.command("open-panel", function()
  local p = rt.ui.panel({{ side = "bottom", size = 150, lines = {{ "x" }}, keys = {{ ["<Return>"] = function() end }} }})
  p:focus()
end)
rt.command("bad-side", function() rt.ui.panel({{ side = "top" }}) end)
"#
            ),
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        assert_eq!(
            run(&dir.join("config.lua"), &paths, Settings::default()).1,
            None
        );
        let ctx = Context::default();
        let actions = run_command("open-panel", "", &ctx).unwrap();
        let [
            Action::Panel { id, source, spec },
            Action::PanelFocus { id: focused },
        ] = actions.as_slice()
        else {
            panic!("{actions:?}");
        };
        assert_eq!((source.as_str(), id), ("", focused));
        assert_eq!((spec.side.as_str(), spec.size), ("bottom", 150));
        assert_eq!(spec.keys, ["<Return>"]);
        assert!(run_command("bad-side", "", &ctx).is_err());
        let none = crate::plugins::Permissions::default();
        let actions = load_plugin("tree", &dir.join("plugins/tree"), &none, false, &ctx).unwrap();
        assert!(
            actions.iter().any(
                |a| matches!(a, Action::Panel { source, spec, .. } if source == "tree" && spec.side == "right" && spec.size == 300)
            ),
            "{actions:?}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn statusbar_widgets_draw_text_and_drop_when_they_fail() {
        let dir = std::env::temp_dir().join(format!("rt-lua-widgets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
rt.statusbar.widget("where", function() return "at " .. rt.url() end)
rt.statusbar.widget("count", function() return 3 end)
rt.statusbar.widget("bad", function() return {} end)
rt.statusbar.widget("slow", function() while true do end end)
assert(not pcall(rt.statusbar.widget, "a b", function() end))
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        assert_eq!(
            run(&dir.join("config.lua"), &paths, Settings::default()).1,
            None
        );
        let ctx = Context {
            url: "https://example.com/".into(),
            ..Context::default()
        };
        let names: Vec<String> = ["where", "count", "bad", "slow", "none"]
            .map(String::from)
            .into();
        let (texts, errors) = widget_texts(&names, &ctx);
        assert_eq!(
            texts.get("where").map(String::as_str),
            Some("at https://example.com/")
        );
        assert_eq!(texts.get("count").map(String::as_str), Some("3"));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[1].contains("stopped after"), "{errors:?}");
        // Failed widgets are gone.
        let (texts, errors) = widget_texts(&names, &ctx);
        assert_eq!(texts.len(), 2);
        assert!(errors.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lazy_plugins_name_their_events_commands_and_keys() {
        let dir = std::env::temp_dir().join(format!("rt-lua-lazy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = |text: &str| {
            std::fs::write(dir.join("config.lua"), text).unwrap();
            let paths = Paths::resolve(Some(&dir)).unwrap();
            run(&dir.join("config.lua"), &paths, Settings::default()).1
        };
        let error = config(
            r#"rt.pack.add({
  { dir = "/p/eager" },
  { dir = "/p/lazy", event = "tab_opened", cmd = { "lazy-go" }, keys = { "<Space>l", { "<C-l>", mode = "insert" } } },
})"#,
        );
        assert_eq!(error, None);
        let specs = plugin_specs();
        assert!(!specs[0].is_lazy());
        assert!(specs[1].is_lazy());
        assert_eq!(specs[1].events, ["tab_opened"]);
        assert_eq!(specs[1].commands, ["lazy-go"]);
        assert_eq!(
            specs[1].keys,
            [
                (Mode::Normal, "<Space>l".to_string()),
                (Mode::Insert, "<C-l>".to_string())
            ]
        );
        for bad in [
            r#"rt.pack.add({ dir = "/p/x", event = "nope" })"#,
            r#"rt.pack.add({ dir = "/p/x", cmd = "a b" })"#,
            r#"rt.pack.add({ dir = "/p/x", keys = { { "<Nope>" } } })"#,
        ] {
            assert!(config(bad).is_some(), "{bad}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn plugins_run_sandboxed_with_their_permissions() {
        let dir = std::env::temp_dir().join(format!("rt-lua-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |path: &str, text: &str| {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("lua/secret.lua", "return { token = 'x' }");
        write(
            "plugins/demo/lua/demo/init.lua",
            r#"
local M = {}
function M.setup(opts)
  rt.command("demo-hello", function() rt.notify((opts.greeting or "?") .. " from demo") end)
  rt.command("demo-probe", function()
    local found = {}
    for _, k in ipairs({ "spawn", "run", "set", "get" }) do if rt[k] then table.insert(found, k) end end
    if io then table.insert(found, "io") end
    if os.execute then table.insert(found, "os.execute") end
    if c then table.insert(found, "c") end
    if load("return rt and rt.run")() then table.insert(found, "load-escape") end
    if getmetatable("") then table.insert(found, "string-metatable") end
    if pcall(require, "secret") then table.insert(found, "user-module") end
    if pcall(rt.keymap.set, "normal", "zz", "spawn evil") then table.insert(found, "bind-command") end
    if pcall(rt.bind, "zz", "spawn evil") then table.insert(found, "bind-command") end
    if pcall(rt.open, "javascript:alert(1)") then table.insert(found, "javascript") end
    if rt.page then table.insert(found, "page") end
    if rt._page or rt._ask or rt._answers then table.insert(found, "private") end
    rt.open("x;;spawn evil", "tab")
    rt.keymap.set = nil
    rt.store().set("k", "v")
    rt.notify("allowed: " .. table.concat(found, ","))
  end)
end
return M
"#,
        );
        write("plugins/demo/plugin/demo.lua", "rt.notify('demo loaded')");
        write("plugins/boom/plugin/boom.lua", "error('bad plugin')");
        write(
            "plugins/free/plugin/free.lua",
            "rt.notify(io and rt.run and 'trusted' or 'sandboxed')",
        );
        let dir_text = dir.display().to_string();
        write(
            "config.lua",
            &format!(
                r#"
rt.pack.add({{
  {{ dir = "{dir_text}/plugins/demo", opts = {{ greeting = "hi" }} }},
  {{ dir = "{dir_text}/plugins/boom" }},
  {{ dir = "{dir_text}/plugins/free", trusted = true }},
}})
assert(not pcall(require, "demo"))
rt.command("still", function() rt.keymap.set("normal", "zq", "reload") end)
"#
            ),
        );
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        let specs = plugin_specs();
        assert_eq!(
            specs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["demo", "boom", "free"]
        );
        let ctx = Context::default();
        let none = crate::plugins::Permissions::default();
        let texts = |actions: Vec<Action>| -> Vec<String> {
            actions
                .into_iter()
                .filter_map(|a| match a {
                    Action::Message { text, .. } => Some(text),
                    _ => None,
                })
                .collect()
        };
        let demo = specs[0].dir.clone().unwrap();
        assert_eq!(
            texts(load_plugin("demo", &demo, &none, false, &ctx).unwrap()),
            ["demo loaded"]
        );
        assert_eq!(
            texts(run_command("demo-hello", "", &ctx).unwrap()),
            ["hi from demo"]
        );
        let probe = run_command("demo-probe", "", &ctx).unwrap();
        assert_eq!(texts(probe.clone()), ["allowed: "]);
        // The URL stays a URL.
        assert!(probe.contains(&Action::Open {
            url: "x;;spawn evil".into(),
            target: OpenTarget::Tab
        }));
        assert!(paths.data_dir.join("plugin-data/demo--data.json").exists());
        // The plugin's own rt was changed, not everyone's.
        assert_eq!(
            run_command("still", "", &ctx).unwrap(),
            [Action::Bind {
                mode: Mode::Normal,
                keys: "zq".into(),
                command: "reload".into()
            }]
        );
        // The user's require finds a loaded plugin's module, the same one.
        let error =
            load_plugin("boom", specs[1].dir.as_ref().unwrap(), &none, false, &ctx).unwrap_err();
        assert!(error.starts_with("boom/plugin/boom.lua:1:"), "{error}");
        let free = specs[2].dir.clone().unwrap();
        assert_eq!(
            texts(load_plugin("free", &free, &none, true, &ctx).unwrap()),
            ["trusted"]
        );
        // Permissions open what they name.
        let files = crate::plugins::Permissions {
            files: true,
            spawn: true,
            ..Default::default()
        };
        write(
            "plugins/granted/plugin/g.lua",
            "rt.notify(tostring(io ~= nil) .. ' ' .. tostring(rt.spawn ~= nil) .. ' ' .. tostring(rt.run ~= nil))",
        );
        assert_eq!(
            texts(
                load_plugin("granted", &dir.join("plugins/granted"), &files, false, &ctx).unwrap()
            ),
            ["true true false"]
        );
        // rt.page acts as the plugin, with its pages; rt.ui names who asks.
        let pages = crate::plugins::Permissions {
            pages: vec!["*.example.com".into()],
            ..Default::default()
        };
        write(
            "plugins/filler/plugin/f.lua",
            r#"
rt.command("filler-fill", function()
  rt.page.fill_login({ host = "example.com", username = "ann", password = "hunter2" })
  if pcall(rt.page.fill_login, { password = "hunter2" }) then rt.notify("no host") end
  local ok, err = pcall(rt.page.type, { "hunter2" })
  rt.notify(tostring(ok) .. " " .. tostring(err):gsub("^.-: ", ""))
end)
rt.command("filler-ask", function()
  rt.ui.input({ prompt = "Master password", secret = true }, function(answer) rt.notify("got " .. tostring(answer)) end)
  rt.ui.select({ { n = "a" }, { n = "b" } }, { format = function(x) return x.n end }, function(item, i)
    rt.notify("picked " .. tostring(item and item.n) .. " " .. tostring(i))
  end)
end)
"#,
        );
        load_plugin("filler", &dir.join("plugins/filler"), &pages, false, &ctx).unwrap();
        let fill = run_command("filler-fill", "", &ctx).unwrap();
        let refused = texts(fill.clone()).join("");
        assert!(
            refused.starts_with("false rt.page.type takes text"),
            "{refused}"
        );
        assert!(
            !refused.contains("hunter2"),
            "errors don't echo what was passed"
        );
        let [
            Action::Page {
                plugin,
                pages,
                request,
            },
            ..,
        ] = fill.as_slice()
        else {
            panic!("{fill:?}");
        };
        assert_eq!(plugin.as_deref(), Some("filler"));
        assert_eq!(pages.as_deref(), Some(&["*.example.com".to_string()][..]));
        assert!(matches!(request, PageRequest::FillLogin { host, .. } if host == "example.com"));
        assert!(
            !format!("{fill:?}").contains("hunter2"),
            "secrets stay out of debug output"
        );
        let asks = run_command("filler-ask", "", &ctx).unwrap();
        let ids: Vec<u32> = asks
            .iter()
            .filter_map(|a| match a {
                Action::Ask { id, source, .. } => {
                    assert_eq!(source, "Plugin filler");
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        assert!(matches!(
            &asks[0],
            Action::Ask {
                ask: Ask::Input { secret: true, .. },
                ..
            }
        ));
        assert!(
            matches!(&asks[1], Action::Ask { ask: Ask::Select(items), .. } if items == &["a", "b"])
        );
        assert_eq!(
            texts(answered(ids[0], Some("pw".into()), &ctx).unwrap()),
            ["got pw"]
        );
        assert_eq!(
            texts(answered(ids[1], Some("1".into()), &ctx).unwrap()),
            ["picked b 2"]
        );
        assert!(
            texts(answered(ids[1], Some("0".into()), &ctx).unwrap()).is_empty(),
            "answered once"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn json_round_trips_and_config_pages_act_anywhere() {
        let dir = std::env::temp_dir().join(format!("rt-lua-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
local v = rt.json.decode('{"a": [1, "x", null, true], "b": {"c": null}}')
assert(v.a[1] == 1 and v.a[2] == "x" and v.a[3] == nil and v.a[4] == true and v.b.c == nil)
assert(rt.json.encode({ k = "v" }) == '{"k":"v"}')
assert(not pcall(rt.json.decode, "{oops"))
rt.command("cfg-type", function() rt.page.type("hello") end)
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        assert_eq!(
            run_command("cfg-type", "", &Context::default()).unwrap(),
            [Action::Page {
                plugin: None,
                pages: None,
                request: PageRequest::Type("hello".into())
            }]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stores_keep_data_between_runs() {
        let dir = std::env::temp_dir().join(format!("rt-lua-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
local list = rt.store("reading-list")
rt.command("list-add", function(url)
  local urls = list.get("urls") or {}
  table.insert(urls, url)
  list.set("urls", urls)
  list.set("count", #urls)
end)
rt.command("list-show", function()
  rt.notify(table.concat(list.get("urls") or {}, " ") .. " (" .. tostring(list.get("count")) .. ")")
end)
rt.command("list-forget", function() list.clear() end)
assert(not pcall(rt.store, "../escape"))
"#,
        )
        .unwrap();
        let ctx = Context::default();
        let shown = |dir: &Path| {
            let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
            assert_eq!(error, None);
            match run_command("list-show", "", &ctx).unwrap().as_slice() {
                [Action::Message { text, .. }] => text.clone(),
                other => panic!("{other:?}"),
            }
        };
        let (_, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        run_command("list-add", "https://a.example", &ctx).unwrap();
        run_command("list-add", "https://b.example", &ctx).unwrap();
        // A new VM, as after a restart, reads what was saved.
        assert_eq!(shown(&dir), "https://a.example https://b.example (2)");
        run_command("list-forget", "", &ctx).unwrap();
        assert_eq!(shown(&dir), " (nil)");
        assert!(
            paths
                .data_dir
                .join("plugin-data/reading-list.json")
                .exists()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keymaps_with_descriptions_and_commands_with_completion() {
        let dir = std::env::temp_dir().join(format!("rt-lua-keymap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            r#"
rt.keymap.set("normal", "<Space>h", function() rt.notify("hi") end, { desc = "Say hi" })
rt.keymap.set({ "normal", "insert" }, "<Ctrl-y>", "reload", { desc = "Reload the page" })
rt.keymap.set("normal", "gx", "tab-close")
rt.keymap.del("normal", "gx")
rt.command("greet", function(args) rt.notify("hello " .. args) end, {
  desc = "Greet someone",
  complete = function(arglead)
    local out = {}
    for _, n in ipairs({ "alice", "bob", { name = "carol", desc = "a friend" } }) do
      local name = type(n) == "table" and n.name or n
      if name:find("^" .. arglead) then table.insert(out, n) end
    end
    return out
  end,
})
rt.command("late-bind", function() rt.keymap.set("normal", "zz", "reload") end)
"#,
        )
        .unwrap();
        let paths = Paths::resolve(Some(&dir)).unwrap();
        let (ops, error) = run(&dir.join("config.lua"), &paths, Settings::default());
        assert_eq!(error, None);
        let binds: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                ConfigOp::Bind {
                    mode,
                    keys,
                    command,
                } => Some(format!("{mode} {keys} {command}")),
                ConfigOp::Unbind { mode, keys } => Some(format!("{mode} -{keys}")),
                _ => None,
            })
            .collect();
        assert_eq!(
            binds,
            [
                "normal <Space>h lua-call 1",
                "normal <Ctrl-y> reload",
                "insert <Ctrl-y> reload",
                "normal gx tab-close",
                "normal -gx",
            ]
        );
        assert_eq!(describe("lua-call 1").as_deref(), Some("Say hi"));
        assert_eq!(describe("reload").as_deref(), Some("Reload the page"));
        assert_eq!(describe("tab-close"), None);
        assert_eq!(
            complete_command("greet", ""),
            [
                ("alice".to_string(), String::new()),
                ("bob".to_string(), String::new()),
                ("carol".to_string(), "a friend".to_string()),
            ]
        );
        assert_eq!(
            complete_command("greet", "b"),
            [("bob".to_string(), String::new())]
        );
        assert!(complete_command("nope", "").is_empty());
        assert!(user_commands().contains(&("greet".to_string(), "Greet someone".to_string())));
        // A binding made from a callback becomes a :bind.
        assert_eq!(
            run_command("late-bind", "", &Context::default()).unwrap(),
            [Action::Bind {
                mode: Mode::Normal,
                keys: "zz".into(),
                command: "reload".into()
            }]
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
            [Action::Open {
                url: "https://en.wikipedia.org/wiki/Rust".into(),
                target: OpenTarget::Tab
            }]
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
