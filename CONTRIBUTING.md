# Contributing to ProcessCraft

Thank you for helping. ProcessCraft is a small project with one maintainer, so a clear issue or a
focused pull request goes a long way.

## Before you start

- **Bugs and small fixes:** open a pull request directly, or an issue if you are not sure it is a bug.
- **Anything larger:** open an issue first and describe what you want to change and why. This is
  especially important for the `.processcraft` format, BPMN import and export, the command protocol
  (CLI, MCP) and anything that changes how files are saved.
- **Security problems:** do not open a public issue. See [SECURITY.md](SECURITY.md).
- **Questions:** see [SUPPORT.md](SUPPORT.md).

Product copy, identifiers, code comments and documentation are in English.

## Set up

You need [rustup](https://rustup.rs) and Git. The repository pins Rust 1.98.1 in
`rust-toolchain.toml`; the first build fetches pinned forks of egui, winit and craft-libs, and after
that everything builds with `--locked --offline`.

```sh
git clone https://github.com/eliobenigni7/ProcessCraft.git
cd ProcessCraft
cargo run --release -p processcraft            # desktop app
cargo run -p processcraft-cli -- --help        # command-line tool
```

The desktop app and saving are qualified on Windows 10 and 11. The Python scripts in `scripts/`
are independent cross-checks and need `lxml`, `pypdf`, `Pillow` and Poppler.

## The quality gates

A change is ready when these pass. CI runs them on every pull request.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-targets --all-features --locked --offline
python -I scripts/check_text_hygiene.py
```

Run a single test file or test with, for example,
`cargo test -p processcraft-core --test commands <name>`.

## How the code is organised

Read the [architecture page](https://eliobenigni7.github.io/ProcessCraft/docs/developers/architecture.html)
and, for depth, [docs/design/solution-design.md](docs/design/solution-design.md). The rules that
reviews check most often:

- **Every persistent change is a command.** BPMN behaviour and mutations live in
  `processcraft-core` and go through `commands::Session`. The GUI, the CLI and MCP build `Mutation`s
  and submit them; they never change the model directly and never carry their own copy of BPMN rules.
- **Crate boundaries.** `processcraft-core` has no file system, UI or network code.
  `processcraft-io` and `processcraft-render` depend on the core and never on each other.
- **Adding a mutation** touches the `Mutation` enum, its application in `Session` and the
  `COMMAND_DESCRIPTORS` table in `crates/processcraft-core/src/commands.rs`. Tests check that the
  descriptors and the JSON Schema agree.
- **Nothing is lost silently.** BPMN content outside the supported profile is preserved byte for
  byte or refused with a reason. Never drop or reinterpret it.
- **Bounded input.** New parsers or readers need explicit size and count limits.
- **No `unsafe`.** It is forbidden in the workspace, apart from a few reviewed OS adapters in
  `processcraft-io`.

## Tests and fixtures

- Add focused tests next to the code you change: integration tests in `crates/*/tests/`, GUI unit
  tests inside `apps/processcraft/src/`.
- Files in `tests/fixtures/` and `assets/fonts/` are pinned byte for byte, with SHA-256 hashes in a
  manifest. Never reformat or re-save one. A new fixture needs a manifest entry with its provenance,
  license, hash and expected import mode.
- Only add fixtures you have the right to share. Never commit a real organisation's processes.

## Documentation

- User documentation is the mdBook in `docs/book/`. If you change a CLI subcommand or a command,
  regenerate the reference pages and commit them; CI fails while they are stale:

  ```sh
  cargo build -p processcraft-cli --locked --offline
  python -I scripts/docs_book.py reference --cli target/debug/processcraft-cli.exe
  ```

- What a feature verified, with measured limits, goes into the matching
  `docs/implementation/*-evidence.md`. State what was measured, not broad claims.
- Add a line to the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md) for anything a user would
  notice.

## Commits and pull requests

- Write commit subjects in the imperative, as the history does: "Add …", "Fix …", "Record …".
  Explain the why in the body when it is not obvious.
- Keep a pull request to one topic. Fill in the template: what changed, how you verified it, and
  what you did not check.
- If an AI assistant wrote part of the change, you are still the author: review it, run the gates and
  say what you verified.

## License

ProcessCraft is released under the [MIT License](LICENSE). By contributing you agree that your
contribution is released under the same license.

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
