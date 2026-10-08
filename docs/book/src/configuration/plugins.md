# Plugins

Plugins add commands, key bindings, hooks and more, written in Lua like `config.lua`. They work like Neovim's: a plugin is a folder with `lua/<name>/init.lua`, and you set it up from `config.lua`.

## Adding a plugin

```lua
-- config.lua
rt.pack.add({
  "https://github.com/someone/riptide-tab-tools",
  { "https://github.com/someone/reading-list", version = "v1.2", opts = { folder = "~/Reading" } },
  { dir = "~/code/my-plugin", config = function() require("my-plugin").setup() end },
})
```

A spec is a git URL, or a table:

| Key | |
|---|---|
| `"url"` or `src` | Where to clone it from. riptide installs it into `<data>/pack/<name>` the first time, in the background. |
| `version` | A tag, branch or commit to install; by default the newest commit. |
| `subdir` | The plugin's folder in a repository of several, e.g. `subdir = "passwords"`; its name is the folder's by default. |
| `dir` | A folder on your computer instead (`~/` works), for writing your own. |
| `name` | Its name, which `require` uses; by default the URL's or folder's last part, without a `riptide-` prefix or a `.nvim`-style suffix. |
| `opts` | Passed to `require(name).setup(opts)` once it loads. |
| `config` | A function run once it loads, instead of `opts`. |
| `trusted = true` | Skip the sandbox and give it everything, as Neovim does. Only for code you vouch for. |
| `event`, `cmd`, `keys` | Load it only when needed; see below. |

Plugins load after `config.lua` has run, so set them up with `opts` or `config` rather than calling `require` straight away.

## Loading when needed

A plugin with `event`, `cmd` or `keys` loads only once you need it, which keeps startup quick:

```lua
rt.pack.add({
  { "https://github.com/someone/reading-list", cmd = { "read-later", "reading-list" }, opts = {} },
  { dir = "~/code/tab-tools", keys = { "<Space>t", { "<C-t>", mode = "insert" } }, opts = {} },
  { dir = "~/code/history-sync", event = "window_closed", opts = {} },
})
```

| Key | Loads it |
|---|---|
| `cmd` | when you run one of these commands; it then runs with your arguments |
| `keys` | when you press one of these keys (normal mode unless a `mode` is given); the keys are pressed again for the plugin's own binding |
| `event` | before the first of these events' hooks run, so the plugin's hooks see it too |

Its permissions are still asked for at startup, so a key or command never stops to ask. The Plugins tab lists what each one waits for, and **Load now** or `:pack-load <name>` loads it straight away.

## Versions and the lockfile

`rt-pack-lock.json` in the config folder pins each plugin from git to the commit installed, along with the permissions you approved, so a plugin never changes on its own and the same config gives the same plugins on every computer. Keep it with your dotfiles: on another computer, riptide installs exactly the pinned commits, and if the lockfile moves a plugin to another commit, riptide checks that one out at the next start, or straight away with `:pack-restore`. Plugins from one repository share a checkout, so they're always on the same commit and update together. Git runs without asking for passwords, so a private repository fails with a message instead of waiting.

| Command | Plugins tab | Does |
|---|---|---|
| `:pack-check [name]` | **Check all**, **Check for updates** | fetches and lists each plugin's new commits; nothing changes yet |
| | **Update**, **Update all** | moves to the commits a check listed, the ones you read |
| `:pack-update [name]` | | fetches and moves straight to the newest commits |
| `:pack-sync` | **Sync** | cleans, updates everything and installs what's missing |
| `:pack-restore` | **Restore** | puts every plugin back on its commit in `rt-pack-lock.json` |
| `:pack-clean` | **Clean** | deletes checkouts and lockfile entries of plugins no longer in `config.lua` |

Every update is recorded in `rt-pack-lock.json` and reloads your config once; if a new version asks for more permissions, you're asked about those first.

Every `plugins.check_interval` days (7 by default; 0 turns it off), riptide checks in the background a few seconds after it starts and says which plugins have updates, such as "Plugin updates for pass, passwords; review them on :plugins". The check changes nothing.

## The plugins page

`:plugins` opens the Plugins tab of `:settings`: each plugin with where it comes from, the commit it's on, whether it loaded (and the error if it didn't) and the permissions you approved, with the buttons above.

To review before updating, press **Check all** (or `:pack-check`), read each plugin's new commits on the page, then press **Update** on one or **Update all**. **Revoke** forgets the permissions you approved, so the plugin asks again. **Remove** deletes a plugin's lockfile entry and its checkout, unless another plugin uses it; take it out of `config.lua` too, or it's installed again at the next start.

## Permissions

Plugins run in a sandbox. Without asking, a plugin can react to events, bind keys to its functions, add commands, use timers, show messages, ask you questions (`rt.ui`, which names the plugin asking), open URLs and keep its own data (`rt.store`, which other plugins can't read). Anything more is a permission it lists in its `riptide-plugin.toml`, and the first time it loads, riptide shows them and asks you:

| Permission | Lets it |
|---|---|
| `spawn` | run programs on your computer (`rt.spawn`) |
| `files` | read and write your files (Lua's `io` and `os`) |
| `commands` | run any riptide command (`rt.run`) and bind keys to command lines; this includes running programs |
| `settings` | read and change your settings (`rt.get`, `rt.set`, `c`) |
| `clipboard` | read and write the clipboard |
| `keys` | see every key you press |
| `network = ["host", …]` | connect to these hosts |
| `pages = ["*.example.com", …]` | type and fill logins into pages on these sites (`rt.page`); `["*"]` is every site |
| `frames = ["host", …]` | show these sites inside its own pages |

Your answer is kept in `rt-pack-lock.json` in the config folder, so it's asked once; a new version that asks for more asks again, for the new permissions only. Saying no leaves the plugin unloaded. To be asked again, press **Revoke** on `:plugins`, or delete its entry in `rt-pack-lock.json`.

In the sandbox, `load` only runs text in the plugin's own globals, `require` only finds the plugin's own modules and those of the plugins it lists in `dependencies` (not your config's, nor other plugins'), and a plugin's changes to its `rt` table don't affect anyone else. Every callback, a plugin's included, is stopped after 2 seconds.

## Writing a plugin

```
reading-list/
├── riptide-plugin.toml     what it is and what it needs
├── lua/reading-list/
│   └── init.lua            require("reading-list")
└── plugin/
    └── reading-list.lua    runs when it loads (optional)
```

```toml
# riptide-plugin.toml
name = "reading-list"
description = "Save pages to read later"
[permissions]
spawn = true
```

```lua
-- lua/reading-list/init.lua
local M = {}
function M.setup(opts)
  local list = rt.store()
  rt.keymap.set("normal", "<Space>r", function()
    local urls = list.get("urls") or {}
    table.insert(urls, rt.url())
    list.set("urls", urls)
    rt.notify("Saved for later")
  end, { desc = "Read later" })
end
return M
```

### Dependencies

A plugin that builds on another lists it in `riptide-plugin.toml`:

```toml
dependencies = ["passwords"]
```

It loads once its dependencies have, and may `require` their modules; their functions run with their own permissions, not the dependent's. You don't have to add a dependency yourself: riptide installs it from the same repository (the folder beside it, for one from `subdir`) or from the folder beside a `dir` plugin, and asks for its permissions as usual. Add it to `rt.pack.add` only to pass it options. A plugin whose dependency is refused or fails doesn't load, and `:plugins` says why.

### Pages

A plugin can ship HTML in a `pages/` folder and open it in a tab with `rt.ui.page({ path, on_message })`. Its pages are served as `riptide://<name>.plugin/…`, each plugin its own origin, and they talk to the plugin, and only to it, with messages:

```lua
-- plugin/reading-list.lua
rt.command("reading-list", function()
  rt.ui.page({
    path = "index.html",
    on_message = function(name, data, page)
      if name == "ready" then page:send("urls", rt.store().get("urls") or {}) end
    end,
  })
end)
```

```js
// pages/app.js, loaded with <script src="app.js"> from pages/index.html
addEventListener("rtmessage", (e) => {
  if (e.detail.name === "urls") render(e.detail.data);
});
rt.send("ready", JSON.stringify({}));
```

`where = "panel"` docks the page beside the page area instead, with `side` (`left`, `right` or `bottom`) and `size` in pixels: a sidebar that shows a web app in an iframe, say. Clicking a field in it types there, as in a tab, and `:panel-focus` moves the keyboard to it and back. `page:close()` closes it.

Pages run only their own files: inline scripts, other sites' scripts and `eval` are blocked. They may show other sites in iframes only over https and only for the hosts the `frames` permission lists, and connect to only the hosts `network` lists. A plugin's pages are served once it has loaded.

### Testing

`riptide --plugin-test DIR` runs the plugin in `DIR` in a throwaway profile, with the permissions its manifest asks for already approved, and runs its `test/*_spec.lua` files. It prints [TAP](https://testanything.org/) and exits with 1 if a test failed, so it works in CI under `xvfb-run`:

```lua
-- test/reading_list_spec.lua
describe("reading-list", function()
  it("saves the page", function()
    keys("<Space>r")
    wait_until(function() return last_message() end)
    assert.matches("^Saved", last_message())
  end)
end)
```

| In a spec | |
|---|---|
| `describe(name, fn)`, `it(name, fn)`, `before_each(fn)` | group and name tests |
| `assert(v)`, `assert.equals(expected, actual)`, `assert.same` (deep), `assert.truthy`, `assert.falsy`, `assert.matches(pattern, s)`, `assert.has_error(fn, pattern)` | checks |
| `keys("<Space>r")`, `run("open x")` | press riptide keys or run a command |
| `wait(ms)`, `wait_until(fn, timeout)`, `wait_for(event, { pattern, timeout })` | let the browser work; `wait_for` returns the event |
| `messages()`, `last_message()`, `clear_messages()` | what `rt.notify` showed, the plugin's included |
| `page("login.html")` | the address of a file in the plugin's `test/` folder, served as an ordinary web page (`http://plugin-test.localhost/…`) |
| `plugin_test.dir` | the plugin's folder, e.g. for `opts.command = plugin_test.dir .. "/test/bin/fake-tool"` |

Each test gets 10 seconds. `test/config.lua`, if there is one, replaces the default `rt.pack.add({ dir = DIR, opts = {} })`, to pass other options. [riptide-plugin-template](https://github.com/joshzcold/riptide-plugin-template) is a plugin to start from, with a spec and a GitHub workflow that runs it, and [riptide-plugins](https://github.com/joshzcold/riptide-plugins) collects plugins, each installed with `subdir`.

`:help <name>` shows a plugin's description, permissions and its `doc/<name>.md` (or `README.md`), as text.

The [Lua page](lua.md) describes the API, and `:help rt.ui.float` (any `rt.` function) shows one function; errors name the plugin's file and line, e.g. `reading-list/lua/reading-list/init.lua:7: …`.
