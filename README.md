# ProcessCraft

**An offline, native BPMN 2.0 process modeling workbench written in Rust.**

ProcessCraft is a desktop application, a headless command-line tool and a local [Model Context Protocol](https://modelcontextprotocol.io) server that share one process engine. It runs entirely on your machine: no account, no cloud service, no telemetry. Projects are portable files you own.

> **Status: pre-alpha (0.1).** ProcessCraft is usable for modeling and BPMN interchange, but it has not had a public release yet, file formats may still change between versions, and writable storage has so far only been qualified on Windows (local NTFS). See [Project status](#project-status) for what is and is not verified.

---

## Contents

- [Features](#features)
- [Project status](#project-status)
- [Getting started](#getting-started)
- [Using the desktop app](#using-the-desktop-app)
- [Command-line interface](#command-line-interface)
- [MCP server for AI agents](#mcp-server-for-ai-agents)
- [BPMN interchange](#bpmn-interchange)
- [The `.processcraft` project format](#the-processcraft-project-format)
- [Architecture](#architecture)
- [Development](#development)
- [Documentation map](#documentation-map)
- [Contributing](#contributing)
- [License](#license)

---

## Features

### Modeling canvas

The canvas follows the interaction model of [bpmn-js](https://bpmn.io), re-implemented natively in Rust:

- **Context pad** next to the selected element: append task, gateway, intermediate or end event, text annotation; connect; change type; delete. Lanes and pools offer *add lane above/below* and *divide into 2/3 lanes*.
- **Connect** by dragging from the pad or with two clicks. BPMN rules pick the connection type automatically (sequence flow, message flow between pools, data input/output association, association, compensation association) and the target is highlighted green or red with the reason when a connection is not allowed. Dropping a connection on empty canvas offers to create a connected element there.
- **Auto-place** of appended elements using the spacing of existing neighbours, with collision avoidance; **Manhattan routing** with BPMN preferred layouts (gateways, loops, boundary events, message flows).
- **Change type** (`R` or the wrench): tasks ↔ user/service/script/… tasks, gateway kinds, event types and positions (start, intermediate, end, boundary, interrupting or not), sub-process variants, collapse/expand, data object ↔ data store, sequence flow plain/default/conditional. IDs, names, documentation and valid connections are kept; changes that would lose data are refused with an explanation.
- **Direct manipulation:** move with snapping guides (or a 10 px grid), resize where BPMN allows it, boundary events that follow their activity, attach/detach events by dropping them on or off an activity border, drop an element onto a flow to insert it, edit bendpoints and segments, reconnect flow ends, drag elements from the palette with a live preview.
- **Pools, lanes and containers:** add, split, resize and delete lanes; pools and sub-processes grow when content reaches their border; a new pool wraps existing content; a **space tool** (`S`) inserts or removes space.
- **Editing productivity:** inline label and annotation editing (`E` or double-click), copy/cut/paste/duplicate with fresh IDs, select all, align and distribute, canvas search (`Ctrl+F`), keyboard nudging, undo/redo for every change.
- **Automatic layout** of a whole diagram with a preview before applying.

### Analyst workbench

- A **local library** of models organised in folders, with search.
- A **project dictionary** (roles, systems, documents) linked to model elements, with reverse usages and safe rename.
- An **activity table** synchronised with the diagram, editable in place.
- A **problems** panel with BPMN validation linked to the exact element.
- **Comments** on elements, **named checkpoints**, comparison against a reference version and restore.
- **Export** to SVG, PNG and PDF, light and dark appearance; SVG text is searchable.
- Light and dark themes, keyboard-first operation, remembered window layout and recent files.

### Automation

- A **CLI** that applies the same typed commands as the GUI from JSON batches, queries projects, validates, imports/exports BPMN and renders images.
- An **MCP server** (stdio) so AI agents can read and edit a project, or the project currently open in the desktop app, through the same commands and with explicit file permissions.

### Reliability

- Every change is an atomic, undoable command; a failed batch leaves the project untouched.
- Atomic saves, crash recovery snapshots and detection of external file changes.
- Bounded parsers and archive readers: hostile or oversized files are refused rather than processed.
- No `unsafe` code outside three reviewed Windows OS adapters.

## Project status

| Area | State |
|---|---|
| Desktop modeling, analyst views, CLI, MCP | Implemented and covered by automated tests (about 1,100 tests) |
| BPMN 2.0 import/export | A broad, tested profile. Content outside it is preserved byte-for-byte or refused, never silently dropped. **Not** a claim of complete BPMN 2.0.2 coverage or OMG conformance; see [`docs/research/bpmn-2-coverage.md`](docs/research/bpmn-2-coverage.md). Typical Camunda Modeler and bpmn.io files open as editable models. |
| Platforms | Developed and qualified on **Windows 11 x64**. The code is portable Rust, but macOS and Linux have not been tested yet (dialogs, file locking, fonts, paths). |
| Accessibility | Keyboard operation for core workflows. Screen-reader support for the custom canvas (Windows Narrator) is **not** yet qualified. |
| Performance | Measured on a 2,000–4,000 element model: rename ≈ 36–41 ms, undo ≈ 5–11 ms, validation ≈ 20–50 ms. Formal targets are tracked in the [solution design](docs/design/solution-design.md#19-performance-targets). |
| Packaging | No installer or signed binaries yet; build from source. |

Planned next (milestone M4, "desktop alpha"): Windows installer and file association, accessibility and performance qualification, and a documented capability matrix. See [`docs/plans/`](docs/plans/).

## Getting started

### Requirements

- **Rust 1.98.1** — installed automatically by [rustup](https://rustup.rs) from [`rust-toolchain.toml`](rust-toolchain.toml).
- **Git**, to fetch pinned forks of [egui](https://github.com/storytold/egui), [winit](https://github.com/storytold/winit) and [craft-libs](https://github.com/storytold/craft-libs) on the first build.
- Windows 10/11 x64 is the supported platform today.

### Build and run

```sh
git clone <this repository> ProcessCraft
cd ProcessCraft

# Desktop application
cargo run --release -p processcraft

# Command-line tool
cargo run --release -p processcraft-cli -- --help
```

The first build downloads dependencies. Afterwards everything builds with `--locked --offline`.

## Using the desktop app

| Action | How |
|---|---|
| Add an element | Click a palette entry (auto-placed, connected to the selection) or drag it onto the canvas |
| Append / connect / change type / delete | Context pad next to the selection |
| Connect | Drag from the pad's connect entry, or click it and then click the target |
| Rename | Double-click or `E`; annotations accept multi-line text (`Ctrl+Enter` to finish) |
| Change type | `R` |
| Pan / zoom | Drag empty canvas, middle mouse or `Space`+drag / `Ctrl`+wheel, `Ctrl` `+` `−` `0` |
| Select | Click, `Shift`+click to toggle, `Shift`+drag for a box, `Ctrl+A` for all |
| Move precisely | Arrow keys (1 px), `Shift`+arrows (10 px); hold `Alt` to drag without snapping |
| Clipboard | `Ctrl+C` `Ctrl+X` `Ctrl+V` `Ctrl+D`; pasting BPMN XML copied from another tool imports it as a new model |
| Find | `Ctrl+F` |
| Space tool | `S`, drag to insert or remove space, `Esc` to leave |
| Undo / redo | `Ctrl+Z` / `Ctrl+Y` or `Ctrl+Shift+Z` |
| Save / open / new | `Ctrl+S` / `Ctrl+O` / `Ctrl+N` |

Properties of the selected element are edited in the **Properties** panel; fields apply when you press Enter or leave them. Rarely used BPMN and XML details live under **Advanced**.

## Command-line interface

`processcraft-cli` runs the same commands as the desktop app, without a window.

```sh
# Describe every command and print the JSON Schema of the request protocol
processcraft-cli commands --json

# Create a project from a batch of commands
processcraft-cli run --new Procurement --batch tests/fixtures/commands/procurement.json --out procurement.processcraft

# Inspect, validate, query
processcraft-cli info procurement.processcraft
processcraft-cli validate procurement.processcraft --json
processcraft-cli query procurement.processcraft dictionary

# BPMN interchange
processcraft-cli import-bpmn diagram.bpmn --out diagram.processcraft
processcraft-cli export-bpmn diagram.processcraft --document-id <id> --out diagram.bpmn

# Images
processcraft-cli export-image diagram.processcraft --document-id <id> --diagram-id <id> --format svg --out diagram.svg

# Automatic layout
processcraft-cli layout-preview diagram.processcraft --document-id <id> --diagram-id <id> --out proposal.json
processcraft-cli layout-apply diagram.processcraft --proposal proposal.json --out laid-out.processcraft
```

`info` lists document and diagram IDs. Every subcommand documents its options with `--help`. A batch is a JSON list of typed mutations; it is applied atomically and becomes one undo step. The modeling operations of the canvas (`append`, `connect`, `replaceElement`, `moveElements`, `resizeShape`, `addLane`, `align`, `duplicate`, …) are available as batch commands too.

## MCP server for AI agents

```sh
# Standalone: an agent works on a project file, with explicit folder permissions
processcraft-cli mcp --in project.processcraft --read-root C:\models --write-root C:\models\out

# Attached: an agent edits the project open in the desktop app
processcraft-cli mcp --attach <endpoint> --session <session> --token <token>
```

The MCP tools expose the same command descriptors, queries and validation as the CLI. Nothing listens by default: attachment is enabled from the desktop app (**Enable attachment**), which shows the endpoint, session and token, and file access is limited to folders you authorize. Attachment currently uses a same-user Windows named pipe. Keep the token private.

## BPMN interchange

ProcessCraft reads and writes BPMN 2.0 XML with diagram interchange (DI). An import ends in one of three modes:

| Mode | Meaning |
|---|---|
| **Editable** | The model fits the qualified profile and is fully editable. |
| **Editable with retained extensions** | Vendor extensions are kept verbatim. Vendor *attributes* (e.g. `camunda:assignee`) do not restrict modeling, but an element they reference by ID cannot be deleted; unmodeled vendor *elements* block structural edits that could invalidate them. |
| **Preserved source** | The original XML is kept byte-for-byte and can be exported unchanged; the import report says which element or attribute prevented editing. |

The governing rule is: **content outside the qualified profile is preserved or refused, never silently dropped or reinterpreted.** Parsing is bounded, rejects DTDs and external entities, and never loads remote resources. Coverage details are tracked element by element in [`docs/research/bpmn-2-coverage.md`](docs/research/bpmn-2-coverage.md) and checked against the pinned OMG schemas in [`tests/fixtures/bpmn20-schema/`](tests/fixtures/bpmn20-schema/).

## The `.processcraft` project format

A project is a single ZIP package containing versioned JSON (schema 3) and resources: one or more BPMN documents with their diagrams, the dictionary, folders, comments and immutable checkpoints. Saves are atomic (write, verify, replace), a sidecar lock prevents two writers, and recovery snapshots are kept outside the project. Older schema versions are verified and migrated on open and must be saved under a new name. The format is described in [section 8 of the solution design](docs/design/solution-design.md#8-native-project-format).

## Architecture

```text
apps/processcraft        desktop app (egui)  ─┐
apps/processcraft-cli    CLI + MCP server     ─┼─▶ processcraft-io ─────┐
                                               └─▶ processcraft-render ─┴─▶ processcraft-core
```

| Crate | Responsibility |
|---|---|
| `processcraft-core` | Domain model, typed commands with undo/redo, BPMN modeling rules, validation, queries, geometry, routing and auto-layout. No file system, UI or network. |
| `processcraft-io` | Native package format, BPMN XML import/export, atomic save, recovery, file grants, user settings, local IPC. |
| `processcraft-render` | Toolkit-independent scene, text layout with bundled fonts, CPU rasterizer, SVG/PNG/PDF export. |
| `processcraft` | Desktop composition: canvas gestures, panels, menus, background jobs. |
| `processcraft-cli` | CLI parsing, JSON batches, MCP server. |

Every persistent change, whether from the GUI, a CLI batch or an MCP client, goes through one command pipeline (`processcraft_core::commands::Session`). The GUI never mutates the model directly and holds no BPMN semantics of its own, so the three entry points behave identically. `io` and `render` depend only on `core`, never on each other. The full design and its rationale are in [`docs/design/solution-design.md`](docs/design/solution-design.md).

## Development

```sh
# The three quality gates every change must pass
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-targets --all-features --locked --offline

# One crate, one test file, one test
cargo test -p processcraft-core --test commands
cargo test -p processcraft-core --test commands <test_name>
```

- **Fixtures** under `tests/fixtures/` and fonts under `assets/fonts/` are pinned byte-for-byte; their SHA-256 hashes are recorded in manifests. Never reformat or re-save them; add new fixtures with a manifest entry (provenance, license, hash, expected import mode).
- **Qualification harnesses** are Cargo examples (`cargo run -p processcraft-io --example <name>_qualify`). The Python scripts in `scripts/` independently cross-check their output and need `lxml`, `pypdf`, `Pillow` and Poppler.
- The `test-fault` feature of `processcraft-cli` is a debug-only fault-injection seam; it is compiled out of release builds.
- `unsafe_code` is forbidden workspace-wide, except for reviewed Windows adapters in `processcraft-io`.

## Documentation map

| Document | Content |
|---|---|
| [`docs/design/solution-design.md`](docs/design/solution-design.md) | Architecture source of truth: crate ownership, commands and undo, native format, saving and recovery, trust boundaries, verification |
| [`docs/plans/`](docs/plans/) | Milestone plans (M0–M3 implemented, modeling parity with bpmn-js, M4 planned) |
| [`docs/implementation/`](docs/implementation/) | Evidence of what was actually verified per milestone, with measured limits |
| [`docs/research/`](docs/research/) | BPMN 2.0 coverage inventory and conformance gap analysis |
| [`design/`](design/) | Visual design prototype (FigCraft) used as a reference for the UI |

## Contributing

Contributions are welcome. Before opening a pull request:

1. Open an issue to discuss larger changes, especially anything touching the native format or BPMN interchange.
2. Keep BPMN behavior in `processcraft-core` and expose it through the command pipeline; the GUI, CLI and MCP must stay consistent.
3. Add focused tests and make sure the three quality gates pass.
4. Update the matching evidence file in `docs/implementation/` with what you verified, stating measured limits rather than broad claims.

Please report security issues privately rather than in a public issue.

## License

ProcessCraft is released under the [MIT License](LICENSE).

Third-party components keep their own licenses:

- **Noto Sans** fonts in [`assets/fonts/`](assets/fonts/) — SIL Open Font License 1.1 ([attribution](assets/fonts/ATTRIBUTION.md)).
- **Lucide** icons used in the design prototype — ISC License ([`design/assets/lucide/LICENSE`](design/assets/lucide/LICENSE)).
- **OMG BPMN 2.0.2 schemas and specification** in [`tests/fixtures/bpmn20-schema/`](tests/fixtures/bpmn20-schema/) — © Object Management Group, included unmodified only to verify the importer and exporter against the standard; they are not part of the MIT-licensed code.
- Rust dependencies are fetched by Cargo under their respective licenses.

The modeling interactions are modeled on the behavior of [bpmn-js](https://github.com/bpmn-io/bpmn-js) by bpmn.io; ProcessCraft contains no bpmn-js code. BPMN is a standard of the [Object Management Group](https://www.omg.org/spec/BPMN/2.0.2/).
