# Build from source

You need [rustup](https://rustup.rs) and Git. The repository pins Rust 1.98.1 in
`rust-toolchain.toml`, and rustup installs it on first use. The first build fetches pinned forks of
[egui](https://github.com/storytold/egui), [winit](https://github.com/storytold/winit) and
[craft-libs](https://github.com/storytold/craft-libs); after that, everything builds with
`--locked --offline`.

```sh
git clone https://github.com/eliobenigni7/ProcessCraft.git
cd ProcessCraft
cargo run --release -p processcraft                  # desktop app
cargo run --release -p processcraft -- model.bpmn    # open a project or import a BPMN file
cargo run --release -p processcraft-cli -- --help    # command-line tool
```

The desktop app and the writable storage are qualified on Windows 10 and 11 with local NTFS drives.
The core, I/O and rendering crates have no Windows-only code apart from reviewed OS adapters, but
macOS and Linux are not qualified.

## Quality gates

A change is done when all three pass. CI runs them on every push and pull request, together with
`scripts/check_text_hygiene.py`, the BPMN inventory check and the docs reference check.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-targets --all-features --locked --offline
```

Run one integration-test file or one test with, for example,
`cargo test -p processcraft-core --test commands <name>`.

## The Windows installer

`powershell -File packaging/windows/package.ps1` builds the MSI and the portable ZIP into
`dist/release/`. It needs WiX 5.0.2 as a dotnet tool.

## This documentation

The book lives in `docs/book`. Build it with [mdBook](https://rust-lang.github.io/mdBook/) 0.5.4:

```sh
cargo build -p processcraft-cli --locked --offline
python -I scripts/docs_book.py reference --cli target/debug/processcraft-cli.exe   # refresh generated pages
mdbook build docs/book                                                            # writes target/book
python -I scripts/docs_book.py agents target/book                                 # llms.txt and .md copies
```

The two reference pages are generated from the CLI. If you change a command or a subcommand, run the
first two lines and commit the result; CI fails while the committed pages are out of date.
