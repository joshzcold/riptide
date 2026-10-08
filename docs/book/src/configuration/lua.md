# Lua

`config.lua` gets `c` (qutebrowser-style `c.hints.chars = "asdf"`), `rt.set/get/bind/unbind`, `rt.platform` (`linux`, `macos`, `windows`), `rt.config_dir`, and `require()` from the config directory (`name.lua` or `lua/name.lua`). It is a normal Lua with the standard library, trusted like a shell rc file.

The Lua VM stays alive after the file runs, so config can also script the browser:

```lua
-- A key bound to a function, with access to the page and the count.
rt.bind("<Ctrl-g>", function() rt.message(rt.title() .. " — " .. rt.url()) end)

-- A command, :wiki rust, with completion next to the built-in ones.
rt.command("wiki", function(args)
  rt.open("https://en.wikipedia.org/wiki/" .. args, "tab")
end, "Search Wikipedia")

-- A hook: runs whenever a page on news.example.com finishes loading.
rt.on("load_finished", { pattern = "news.example.com" }, function(e)
  rt.run("scroll-to-perc 0")
end)
```

## Keys and commands

`rt.keymap.set(mode, keys, rhs, opts)` binds keys like `vim.keymap.set`: `mode` is one mode or a list, `rhs` a command line or a function, and `opts.desc` describes it in the key hints popup. `rt.keymap.del(mode, keys)` removes a binding. `rt.bind` and `rt.unbind` still work, and both also work inside callbacks.

`rt.command(name, fn, opts)` takes a description, or a table with `desc` and `complete`. `complete` gets what's typed after the command and returns its completions, as strings or `{ name = "…", desc = "…" }`:

```lua
rt.keymap.set({ "normal", "insert" }, "<Ctrl-y>", "reload", { desc = "Reload the page" })

rt.command("project", function(name) rt.open("https://github.com/me/" .. name, "tab") end, {
  desc = "Open one of my projects",
  complete = function(arglead)
    local out = {}
    for _, p in ipairs({ "riptide", "dotfiles", "notes" }) do
      if p:find(arglead, 1, true) then table.insert(out, p) end
    end
    return out
  end,
})
```

## Keeping data

`rt.store(name)` keeps data between runs, saved as `<data>/plugin-data/<name>.json` on every change. It has `get(key)`, `set(key, value)`, `all()` and `clear()`; values are strings, numbers, booleans and tables of them:

```lua
local list = rt.store("reading-list")
rt.keymap.set("normal", "<Space>r", function()
  local urls = list.get("urls") or {}
  table.insert(urls, rt.url())
  list.set("urls", urls)
  rt.notify("Saved for later (" .. #urls .. ")")
end, { desc = "Read later" })
```

## Status bar widgets

`rt.statusbar.widget(name, fn)` adds a widget that `statusbar.widgets` shows as `lua:<name>`. The function returns its text each time the bar is drawn, so keep it quick: it gets 50 ms, and one that fails or runs longer is removed with an error.

```lua
c.statusbar.widgets = { "keypress", "url", "lua:list", "tabs" }
local list = rt.store("reading-list")
rt.statusbar.widget("list", function()
  local urls = list.get("urls") or {}
  return #urls > 0 and ("📚" .. #urls) or ""
end)
```

An empty string hides the widget. What a widget function asks for besides its text, such as `rt.notify`, is ignored; to change a widget on a timer, use `rt.every`, which redraws the bar when it runs.

## Events

`rt.on(event, [opts], fn)` runs `fn` with a table describing the event:

| Event | When | The table has |
|---|---|---|
| `startup` | riptide has started and loaded `config.lua` | |
| `quit` | riptide is about to quit | |
| `load_started`, `load_finished` | a tab starts or finishes loading a page | `url` |
| `url_changed` | a tab's address changes | `url` |
| `title_changed` | a tab's title changes | `url`, `title` |
| `tab_opened`, `tab_closed` | a tab opens or closes | `url` |
| `tab_selected` | another tab becomes the current one | `url`, `index` (from 1) |
| `window_opened`, `window_closed` | a window opens or closes | `private` (`"true"`, `"false"`) on opening |
| `mode_changed` | the mode changes | `from`, `to` |
| `setting_changed` | a setting changes (`:set`, the settings page) | `name`, `value` (as text) |
| `download_started`, `download_finished` | a download starts or ends | `url`, `path`; `state` (`done`, `failed`, `cancelled`) when it ends |

`opts` can have:

- `pattern`: only pages matching it, written as for `:set -u` (`example.com`, `*.example.com`, `https://example.com`, `*://*.example.com/docs/*`).
- `once = true`: run the first time only.
- `group`: a name, so `rt.off(group)` removes them all.

`rt.on` returns an id for `rt.off(id)`. `rt.group(name, { clear = true })` removes the group's hooks and returns its name, so a script that runs again (`:config-source`) doesn't add its hooks twice:

```lua
local g = rt.group("reading", { clear = true })
rt.on("tab_selected", { group = g, pattern = "*.wikipedia.org" }, function(e)
  rt.message("Reading tab " .. e.index)
end)
rt.on("download_finished", { group = g }, function(e)
  if e.state == "done" then rt.spawn({ "notify-send", "Downloaded", e.path }) end
end)
```

`rt.defer(ms, fn)` calls `fn` once after `ms` milliseconds, and `rt.every(ms, fn)` keeps calling it; both return a handle whose `:stop()` cancels it. `rt.notify(text, level)` (or `rt.message`) shows a message, with `level` `info` (the default), `warning` or `error`:

```lua
-- Remind me to stretch every 45 minutes.
rt.every(45 * 60 * 1000, function() rt.notify("Time to stretch", "warning") end)
```

A callback that runs for more than 2 seconds is stopped with an error, so a mistake like an endless loop can't freeze the browser; long work belongs in `rt.spawn` or a timer.

In callbacks, `rt.url()`, `rt.title()`, `rt.mode()`, `rt.count()` and `rt.tabs()` (the window's tabs, with `title`, `url`, `current` and `pinned`) describe the current state. `rt.run(line)`, `rt.open(url, target)`, `rt.message(text, level)` and `rt.set(...)` act on it. Errors show as `config.lua:line: message`. `:config-source` reloads everything.

`rt.spawn(argv, [opts], [callback])` runs a program in the background, without a shell, and calls `callback` with `{code, stdout, stderr, error}` when it exits. `argv` is a list, or a command line split as `:spawn` splits it. `opts` can set `stdin`, `cwd` and `env`:

```lua
rt.command("translate", function(text)
  rt.spawn({ "trans", "-brief", ":en" }, { stdin = text }, function(r)
    if r.code == 0 then rt.message(r.stdout) else rt.message(r.stderr, "error") end
  end)
end)
```

`rt.json.decode(text)` and `rt.json.encode(value)` read and write JSON, such as a program's output; `null` becomes `nil`.

## Questions and the page

`rt.ui.select(items, opts, on_choice)` and `rt.ui.input(opts, on_confirm)` ask in the prompt area, like Neovim's `vim.ui`. A picker gives each item a key (`1`–`9`, then `a`–`z`); `secret = true` masks what's typed:

```lua
rt.ui.select({ "work", "home" }, { prompt = "Profile" }, function(choice)
  if choice then rt.notify("Using " .. choice) end
end)
rt.ui.input({ prompt = "Passphrase", secret = true }, function(text) --[[ nil if cancelled ]] end)
```

`rt.page` acts on the current tab's page:

| | |
|---|---|
| `rt.page.type(text)` | types into the focused field, like `:insert-text` |
| `rt.page.key(keys)` | presses keys, like `:fake-key` |
| `rt.page.fill_login({ host, username, password, submit })` | fills the page's login form, but only while the tab is still on `host` |
| `rt.page.eval(code, fn)` | evaluates a JavaScript expression and calls `fn(value)` with its value, or `fn(nil, why)` |
| `rt.page.css(css)` | adds a stylesheet to the page until it next loads |
| `rt.page.selection(fn)` | calls `fn(text)` with the selected text |
| `rt.page.hint({ selector, action })` | hints the elements a CSS selector matches and calls `action({ url, text })` with the one you pick, instead of clicking it |

`eval` runs in the page's own world: the page can see the code and change what it returns, so treat the value as the page's word.

They never go through a command line, so what's typed isn't kept in the command history or `:messages`. They don't act on riptide's own pages, and a plugin needs the `pages` permission for the site.

## Floats

`rt.ui.float(opts)` draws a box of text over the page, beside whatever else is on screen, and returns a handle with `update(changes)`, `close()` and `is_open()`. Lines are text, or lists of `{ text, highlight }` chunks coloured by the theme (`title`, `muted`, `accent`, `match`, `url`, `key`, `info`, `warning`, `error`); they're never HTML.

```lua
local list = rt.store("reading-list")
rt.keymap.set("normal", "<Space>l", function()
  local lines = {}
  for i, url in ipairs(list.get("urls") or {}) do
    lines[#lines + 1] = { { tostring(i), "key" }, { " " .. url, "url" } }
  end
  rt.ui.float({
    title = "Reading list",
    lines = lines,
    keys = { c = function(f) list.clear(); f:close() end },
  })
end, { desc = "Show the reading list" })
```

| Option | |
|---|---|
| `position` | `center` (the default), `top`, `bottom`, `top-right` or `bottom-right` of the page |
| `width` | the widest it gets, in characters (default 60) |
| `timeout` | close by itself after this many milliseconds, for a notice |
| `keys` | functions for keys pressed in normal mode while it's the newest float with keys; `Escape` closes it |
| `on_close` | runs when riptide closes it: `Escape`, its timeout or its window closing |

A plugin's floats show its name in the corner, so a float can't pass for riptide's own question.

## Panels

`rt.ui.panel(opts)` keeps lines beside the page (`side = "left"` or `"right"`) or below it (`"bottom"`): a tab tree, a reading list, notes. It takes the same `title` and `lines` as a float, plus `size` in pixels, and returns a handle with `update`, `close`, `focus` and `is_open`. Each side of a window holds one panel; a new one replaces it.

A panel takes keys only while it has focus: `:panel-focus` (or `focus()`) gives it focus, `j`/`k` move its cursor, and its `keys` functions get the cursor's line. `Escape` returns to the page, and clicking a line focuses the panel there.

```lua
local function tab_lines()
  local lines = {}
  for i, tab in ipairs(rt.tabs()) do
    lines[i] = { { tostring(i) .. " ", "key" }, { tab.title, tab.current and "accent" or nil } }
  end
  return lines
end
local tree = rt.ui.panel({
  title = "Tabs",
  lines = tab_lines(),
  keys = { ["<Return>"] = function(_, line) rt.run("tab-select " .. line) end },
})
for _, event in ipairs({ "tab_opened", "tab_closed", "tab_selected", "title_changed" }) do
  rt.on(event, function() tree:update({ lines = tab_lines() }) end)
end
rt.keymap.set("normal", "<Space>t", function() tree:focus() end, { desc = "Focus the tab tree" })
```

For completion and type checking in Neovim, VS Code and other editors using lua-language-server:

```sh
dir="$(riptide --paths | sed -n 's/^config: //p')"
mkdir -p "$dir" && riptide --lua-types > "$dir/rt.meta.lua"
```

The [Lua API reference](../reference/lua-api.md) lists every function and setting with its type.

## Example `config.lua`

```lua
{{#include ../../../config.example.lua}}
```
