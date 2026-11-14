## What and why

<!-- What this changes and the problem it solves. Link the issue: "Fixes #123". -->

## How it was verified

<!-- Tests added or run, commands, files opened. Say what you did NOT check. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy` (no warnings) and `cargo test` pass locally
- [ ] Modeling changes go through `processcraft-core` commands, not the GUI or CLI directly
- [ ] No existing fixture in `tests/fixtures/` or font in `assets/fonts/` was re-saved; new fixtures have a manifest entry
- [ ] If a command or CLI subcommand changed: the reference in `docs/book/src/reference/` is regenerated
- [ ] User-visible change: a line in `CHANGELOG.md` under Unreleased, and the docs updated
- [ ] Verified limits recorded in the matching `docs/implementation/*-evidence.md`, if this is a feature
