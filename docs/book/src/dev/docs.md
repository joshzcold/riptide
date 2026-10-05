# Writing documentation

This book is the documentation. It's built with [mdBook](https://rust-lang.github.io/mdBook/) from `docs/book/` and published to <https://joshzcold.github.io/riptide/> on every push to `main`. A change that users or contributors would notice updates the book in the same commit.

## Where things go

| Change | Update |
|---|---|
| A new or changed command, setting or default binding | Nothing by hand: regenerate the reference (below). Add or update a guide page only if the feature needs explaining beyond its one-line description. |
| New user-visible behaviour (a mode, a prompt, a page, a file riptide reads) | The matching page under `docs/book/src/guide/` or `configuration/`. Add a page to `SUMMARY.md` if no existing one fits. |
| Something a qutebrowser user would trip over | A note on [Moving from qutebrowser](../guide/qutebrowser.md). |
| How riptide is built, structured, tested or released | The matching page under `docs/book/src/dev/`. |
| A pitfall or lesson learned (a CEF quirk, a threading rule) | `docs/book/src/dev/`, and the milestone notes in `docs/PLAN.md`. |
| Plans, status and gaps | `docs/PLAN.md` only; it isn't published. |

Refactors, internal-only fixes and test-only changes don't need documentation.

## Generated pages

These are written by code and checked by unit tests, so CI fails when they're stale. Never edit them by hand:

| File | Generated from |
|---|---|
| `docs/book/src/reference/commands.md` | `COMMANDS` and the default keymap, through `rt_core::help::build` (the same data as `:help`) |
| `docs/book/src/reference/bindings.md` | `Keymap::defaults()` |
| `docs/settings.md` (included by the settings page) | the settings registry |
| `docs/lua/rt.meta.lua` (included by the Lua API page) | the settings registry and the `rt.*` API |

Regenerate all of them with:

```sh
UPDATE_LUA_TYPES=1 cargo test -p rt-config
```

To change their text, change the description in the registry (`crates/rt-core/src/command.rs`, `settings.rs`, `keymap.rs`) or the generator (`crates/rt-config/src/reference.rs`, `lua_types.rs`). The changelog page includes `CHANGELOG.md`, which git-cliff generates from commit messages.

## Building and previewing

```sh
./task docs         # build into docs/book/book/
./task docs-serve   # serve at http://localhost:3000, rebuilding on save
```

## Style

**User guide and configuration pages:**
- Write for someone using the browser, in the second person ("`:set` changes a setting"), starting with the task and then how to do it.
- Show one short example per feature: the keys, or a `config.lua`/`config.toml` snippet.
- Use plain language, and name settings and commands exactly, in backticks.
- Say what's missing or different from qutebrowser where users would expect it.

**Developer guide pages:**
- Explain why as well as what, and link to the code (`crates/rt-cef/src/shell.rs`).
- Keep instructions runnable: commands should work when pasted.

**Both:**
- Keep sentences short, and use a table or list when there are three or more parallel items.
- Link to other pages rather than repeating them, and to the reference for exhaustive lists.
- Write links to repository files outside the book as full GitHub URLs, since the site can't serve them.

## For AI agents

`.claude/skills/docs-user/` and `.claude/skills/docs-dev/` turn these rules into checklists for agents, and `AGENTS.md` points other tools at them. When the rules here change, update the skills too.
