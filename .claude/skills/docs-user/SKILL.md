---
name: docs-user
description: Use after a change users can see (a new or changed command, setting, default binding, mode, prompt, page, file riptide reads, or behaviour) and before committing it — updates the user guide and regenerates the reference in the mdBook site under docs/book/. Also use when asked to write or fix user documentation, or to document a feature for qutebrowser users.
---

# Updating user documentation

The documentation site is an mdBook in `docs/book/`, published to GitHub Pages from `main`. The full rules are in `docs/book/src/dev/docs.md`; this is the checklist for a user-visible change. Do it in the same commit as the change.

## 1. Regenerate the reference

Commands, default bindings, settings and the Lua API pages are generated from the registries. Never edit these files by hand:

- `docs/book/src/reference/commands.md`
- `docs/book/src/reference/bindings.md`
- `docs/settings.md`
- `docs/lua/rt.meta.lua`

```sh
UPDATE_LUA_TYPES=1 cargo test -p rt-config
```

To change their wording, edit the one-line description in `crates/rt-core/src/command.rs`, `settings.rs` or `keymap.rs` and regenerate. Keep descriptions to one plain sentence; `:help` shows the same text.

## 2. Update the guide

Find the page that covers the feature in `docs/book/src/SUMMARY.md`:

| Feature area | Page |
|---|---|
| Modes, keys, the command line | `guide/keys.md` |
| Tabs, the tab bar, windows, private windows | `guide/tabs.md` |
| Sessions, crash recovery, history, quickmarks, bookmarks | `guide/sessions.md` |
| JS dialogs, logins, downloads, permissions, per-site settings | `guide/prompts.md` |
| Content blocking, network traffic, privacy | `guide/privacy.md` |
| Dark mode, spell checking, Widevine | `guide/pages.md` |
| `:spawn`, userscripts, Greasemonkey | `guide/scripts.md` |
| The `riptide` command line, IPC, `riptide://` pages | `guide/terminal.md` |
| Config files, `:set`/`:bind`, `config.toml` | `configuration/files.md` |
| `config.lua`, `rt.*` | `configuration/lua.md` |
| Installing, the sandbox, paths | `guide/installing.md` |

- A command or setting whose one-line description says it all needs no guide text, only the regenerated reference.
- Otherwise, add a short section: what it does for the user, then how (keys, command or setting), with one example.
- If no page fits, add one to `docs/book/src/guide/` and list it in `SUMMARY.md`.
- Update the "What works today" list in `docs/book/src/introduction.md` and the README summary only for a major feature.
- Remove or correct text the change made wrong. Search for the old command or setting name: `grep -rn 'old-name' docs/book/src README.md`.

## 3. Style

- Second person, task first: "`:tab-pin` keeps a tab at the left", not "This command pins…".
- Exact names in backticks: `:open -t`, `tabs.position`, `config.lua`.
- One short example per feature; a table for three or more parallel items.
- Plain language and short sentences. Link to the reference instead of repeating full lists.
- Links to repository files outside the book must be full `https://github.com/joshzcold/riptide/blob/main/...` URLs.

## 4. Check

```sh
cargo test -p rt-config     # generated pages are current
./task docs                 # the book builds with no warnings
```

`./task docs-serve` previews at http://localhost:3000. CI also checks every internal link and anchor.

## Doesn't need docs

Refactors, internal-only fixes, test-only changes, and fixes that make the browser do what the docs already say.
