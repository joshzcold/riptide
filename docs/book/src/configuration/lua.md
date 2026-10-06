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

-- Hooks: load_finished, url_changed, tab_opened (e.url), mode_changed (e.from, e.to).
rt.on("load_finished", function(e)
  if e.url:find("^https://news%.example%.com/") then rt.run("scroll-to-perc 0") end
end)
```

In callbacks, `rt.url()`, `rt.title()`, `rt.mode()`, `rt.count()` and `rt.tabs()` (the window's tabs, with `title`, `url`, `current` and `pinned`) describe the current state. `rt.run(line)`, `rt.open(url, target)`, `rt.message(text, level)` and `rt.set(...)` act on it. Errors show as `config.lua:line: message`. `:config-source` reloads everything.

`rt.spawn(argv, [opts], [callback])` runs a program in the background, without a shell, and calls `callback` with `{code, stdout, stderr, error}` when it exits. `argv` is a list, or a command line split as `:spawn` splits it. `opts` can set `stdin`, `cwd` and `env`:

```lua
rt.command("translate", function(text)
  rt.spawn({ "trans", "-brief", ":en" }, { stdin = text }, function(r)
    if r.code == 0 then rt.message(r.stdout) else rt.message(r.stderr, "error") end
  end)
end)
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
