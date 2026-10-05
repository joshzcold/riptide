# Notes for AI agents

Riptide is a keyboard-driven browser on CEF, written in Rust. Start with the [developer guide](docs/book/src/dev/building.md) and the [architecture](docs/book/src/dev/architecture.md); plans and status are in [docs/PLAN.md](docs/PLAN.md).

## Rules

- **Testing:** never kill or reuse the user's own riptide. Follow [.claude/skills/local-testing/SKILL.md](.claude/skills/local-testing/SKILL.md): always `--basedir <scratch dir>`, your own Xvfb display, and stop processes by PID.
- **Commits:** Conventional Commits (`feat(tabs): …`, `fix: …`, `docs: …`); CI checks them. Run `./task check` before pushing.
- **Documentation ships with the change:**
  - User-visible change: follow [.claude/skills/docs-user/SKILL.md](.claude/skills/docs-user/SKILL.md).
  - Build, structure, testing or release change: follow [.claude/skills/docs-dev/SKILL.md](.claude/skills/docs-dev/SKILL.md).
  - Regenerate the reference with `UPDATE_LUA_TYPES=1 cargo test -p rt-config`, and never hand-edit generated files.
- **Other agents may be working in this checkout at the same time.** Don't revert, stash or reformat changes you didn't make, and stage files by name rather than with `git add -A`.
