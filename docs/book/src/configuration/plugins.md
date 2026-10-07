# Plugins

Plugins add commands, key bindings, hooks and more, written in Lua like `config.lua`. They work like Neovim's: a plugin is a folder with `lua/<name>/init.lua`, and you set it up from `config.lua`.

A plugins page and loading on demand are coming.

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
| `dir` | A folder on your computer instead (`~/` works), for writing your own. |
| `name` | Its name, which `require` uses; by default the URL's or folder's last part, without a `riptide-` prefix or a `.nvim`-style suffix. |
| `opts` | Passed to `require(name).setup(opts)` once it loads. |
| `config` | A function run once it loads, instead of `opts`. |
| `trusted = true` | Skip the sandbox and give it everything, as Neovim does. Only for code you vouch for. |

Plugins load after `config.lua` has run, so set them up with `opts` or `config` rather than calling `require` straight away.

## Versions and the lockfile

`rt-pack-lock.json` in the config folder pins each plugin from git to the commit installed, so a plugin never changes on its own and the same config gives the same plugins on every computer. Keep it with your dotfiles: on another computer, riptide installs exactly the pinned commits, and if the lockfile moves a plugin to another commit, riptide checks that one out at the next start. Git runs without asking for passwords, so a private repository fails with a message instead of waiting.

## Permissions

Plugins run in a sandbox. Without asking, a plugin can react to events, bind keys to its functions, add commands, use timers, show messages, open URLs and keep its own data (`rt.store`, which other plugins can't read). Anything more is a permission it lists in its `riptide-plugin.toml`, and the first time it loads, riptide shows them and asks you:

| Permission | Lets it |
|---|---|
| `spawn` | run programs on your computer (`rt.spawn`) |
| `files` | read and write your files (Lua's `io` and `os`) |
| `commands` | run any riptide command (`rt.run`) and bind keys to command lines; this includes running programs |
| `settings` | read and change your settings (`rt.get`, `rt.set`, `c`) |
| `clipboard` | read and write the clipboard |
| `keys` | see every key you press |
| `network = ["host", …]` | connect to these hosts |
| `pages = ["*.example.com", …]` | read and change pages on these sites |
| `frames = ["host", …]` | show these sites inside its own pages |

Your answer is kept in `rt-pack-lock.json` in the config folder, so it's asked once; a new version that asks for more asks again, for the new permissions only. Saying no leaves the plugin unloaded. Delete its entry in `rt-pack-lock.json` to be asked again.

In the sandbox, `load` only runs text in the plugin's own globals, `require` only finds plugins' modules (not your config's), and a plugin's changes to its `rt` table don't affect anyone else. Every callback, a plugin's included, is stopped after 2 seconds.

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

The [Lua page](lua.md) describes the API; errors name the plugin's file and line, e.g. `reading-list/lua/reading-list/init.lua:7: …`.
