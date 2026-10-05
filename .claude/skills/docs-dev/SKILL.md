---
name: docs-dev
description: Use after a change to how riptide is built, structured, tested, released or contributed to (a new crate or module boundary, a build or task change, a CI job, a new test step, a CEF pitfall or threading rule worth remembering) and before committing it — updates the developer guide in the mdBook site under docs/book/src/dev/. Also use when asked to write or fix developer documentation.
---

# Updating developer documentation

The developer guide is part of the mdBook site in `docs/book/` (`docs/book/src/dev/`), published to GitHub Pages from `main`. The full rules are in `docs/book/src/dev/docs.md`. Do this in the same commit as the change.

## Where it goes

| Change | Page |
|---|---|
| Requirements, `./task` targets, CEF download, logging, build flags | `dev/building.md` |
| Crates and what belongs where, processes, the UI channel, the core loop | `dev/architecture.md` |
| Unit tests, the smoke test, CI jobs | `dev/testing.md` |
| Commit conventions, generated files, pre-push checks | `dev/contributing.md` |
| How the docs are built, generated or styled | `dev/docs.md`, and both docs skills (`docs-user`, `docs-dev`) |
| Versioning, changelog, release workflow, packaging | `dev/releasing.md` |
| Plans, status, gaps | `docs/PLAN.md` only (not published) |

- **Pitfalls and lessons learned** (a CEF quirk, a threading rule, a flaky-test cause) go in the developer guide where the next person will look, as well as in the milestone's notes in `docs/PLAN.md`.
- **Manual testing rules** live in `.claude/skills/local-testing/SKILL.md`; `dev/testing.md` links to it rather than copying it.
- A new `./task` target is listed in `dev/building.md`, or `dev/testing.md` if it's a check.
- If the change is also visible to users, use the `docs-user` skill too.

## Style

- Explain why, not only what: the constraint or bug that led to the rule.
- Point to code with repo-relative paths in backticks (`crates/rt-cef/src/shell.rs`). Link files outside the book with full `https://github.com/joshzcold/riptide/blob/main/...` URLs, since the site can't serve them.
- Commands must work when pasted, from the repository root.
- Short sentences; a table or list for three or more parallel items. Link instead of repeating.

## Check

```sh
./task docs        # builds with no warnings
```

`./task docs-serve` previews at http://localhost:3000. CI checks internal links and anchors.

## Doesn't need docs

Refactors that keep the structure, internal fixes, and test changes that don't change how tests are run.
