# Contributing

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org), which [the changelog](../reference/changelog.md) is generated from (`feat` and `fix` appear in it; `docs`, `test`, `ci` and `chore` don't): `feat(tabs): add pinned tabs`, `fix: …`, `docs: …`, `ci: …`. Run `./task hooks` once to check messages locally before CI does.

## Before you push

- `./task check` runs everything CI does: the [linters](testing.md#linters), unit tests and the smoke test.
- Generated files must be current: `UPDATE_LUA_TYPES=1 cargo test -p rt-config` rewrites `docs/lua/rt.meta.lua`, `docs/settings.md` and the generated reference pages after you add or change a command, setting or default binding.
- User-visible changes need documentation in the same change; see [Writing documentation](docs.md).
