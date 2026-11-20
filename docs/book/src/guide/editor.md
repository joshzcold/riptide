# Editing local files

Riptide has a built-in editor for local text files, with vim keys and syntax highlighting. It opens in a tab like any other page, so you can split it next to the page you're reading or keep notes open beside a call.

## Opening a file

- `:edit ~/notes/todo.md` opens a file in a new tab. The path must be absolute or start with `~/`. If the file doesn't exist yet, the first save creates it.
- `:edit` with no path reopens the current `file://` page in the editor.
- Opening a local text file (`:open ~/src/main.rs`, or a link in a `file://` folder listing) goes straight to the editor. HTML, SVG and PDF files still render as pages. `editor.file_urls = false` turns this off, and local files then show as Chromium shows them.

The editor's address is the file's path under `riptide://edit/`, for example `riptide://edit/home/you/notes/todo.md`.

## Keys

The editor tab starts in passthrough mode, so the editor gets every key. `<Shift-Escape>` hands keys back to riptide, for example to switch tabs or run a riptide command, and `<Ctrl-v>` returns them to the editor.

With `editor.vim` on (the default), the editor uses vim keys. These commands work as in vim:

| Command | What it does |
|---|---|
| `:w` | Write the file |
| `:w!` | Write it even if it changed on disk since you opened it |
| `:wq`, `:x` | Write the file, then close the tab |
| `:q` | Close the tab, unless there are unwritten changes |
| `:q!` | Close the tab and drop unwritten changes |

Yanks go to the system clipboard (`yy`, `yw`, `"+y`), and `"*y` yanks to the primary selection on Linux. Deletes stay in the editor's registers, so `x` and `dd` don't replace what you copied. To paste from the clipboard, use `Ctrl+V` in insert mode.

`Ctrl+S` also writes the file. With `editor.vim = false` the editor uses ordinary editing keys.

The header shows the path, a dot when there are unwritten changes, and the result of the last write. Closing the tab some other way (`d`, `:tab-close`) asks first if there are unwritten changes.

## Limits

- Files must be UTF-8 text and at most 8 MB.
- Windows line endings (`\r\n`) are kept as they are.
- Syntax highlighting covers Rust, JavaScript and TypeScript, Python, Go, C and C++, Java, HTML, CSS, JSON, YAML, TOML, Markdown, SQL, XML, Lua, shell scripts, Dockerfiles and INI-style config files. Anything else is plain text.
- `:open-editor` and `:config-edit` are separate: they still open `editor.command`, your own editor program, in its own window.
