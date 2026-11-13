# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

ProcessCraft is an offline, native Rust desktop workbench for BPMN process modeling (egui GUI + headless CLI + local MCP). Product copy, identifiers, and documentation are in English.

## Commands

Toolchain is pinned to Rust 1.98.1 (`rust-toolchain.toml`, edition 2024). Dependencies include pinned git forks (`storytold/egui`, `storytold/winit`, `craft-libs`); after the first fetch, everything runs with `--locked --offline`.

```powershell
# Quality gates (all three are expected to pass before a change is called done; .github/workflows/ci.yml runs them plus scripts/check_text_hygiene.py)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-targets --all-features --locked --offline

# One integration-test file / one test by name filter
cargo test -p processcraft-core --test commands
cargo test -p processcraft-core --test commands <test_name>
cargo test -p processcraft lifecycle          # GUI crate unit tests live inside src/ (filter by name)

# Run
cargo run --release -p processcraft           # desktop app
cargo run -p processcraft-cli -- commands --json   # command descriptors + JSON Schema of the request protocol
cargo run -p processcraft-cli -- run --new Procurement --batch tests/fixtures/commands/procurement.json --out target/procurement.processcraft
cargo run -p processcraft-cli -- query target/procurement.processcraft dictionary
cargo run -p processcraft-cli -- validate target/procurement.processcraft --json
```

Other CLI subcommands: `info`, `import-bpmn`, `export-bpmn`, `export-image`, `export-drawio`, `layout-preview`, `layout-apply`, `mcp` (see `apps/processcraft-cli/src/args.rs`).

Qualification harnesses are Cargo examples (`cargo run -p processcraft-io --example <name>_qualify --locked --offline`, `cargo run --release -p processcraft-cli --example workbench_qualify -- --out target/...`). The Python scripts in `scripts/` are independent cross-checks of their output and need `lxml` (XSD/XML checks), `pypdf` + `Pillow` and Poppler (PDF/render checks). `python scripts/generate_bpmn20_inventory.py --check` verifies `docs/research/bpmn-2-coverage.json` against the pinned OMG schemas.

Windows installer: `powershell -File packaging/windows/package.ps1` (needs WiX 5.0.2 as a dotnet tool) builds `dist/release/processcraft-<version>-windows-x64.msi` and a portable ZIP; see `docs/implementation/m4-evidence.md`.

The `test-fault` feature on `processcraft-cli` is a debug-only fault-injection seam for tests (`--all-features` enables it); it is compiled out of release builds.

## Architecture

Five workspace members with a strict dependency direction:

```
apps/processcraft (egui GUI) ─┐
apps/processcraft-cli (CLI/MCP) ─┼─> processcraft-io ──┐
                               └─> processcraft-render ─┴─> processcraft-core
```

- **`processcraft-core`** — domain model, typed commands, history, validation, query projections, geometry, auto-layout. No filesystem, egui, dialogs, or networking.
- **`processcraft-io`** — native `.processcraft` package (ZIP + versioned JSON, schema 3), BPMN XML import/export, atomic save, file grants, recovery, local IPC for MCP attachment. No business mutation rules.
- **`processcraft-render`** — toolkit-free scene, text layout (pinned fonts in `assets/fonts`), CPU raster, SVG/PNG/PDF export. Receives snapshots; never writes projects.
- `io` and `render` both depend on `core` and **never on each other**. The GUI and CLI must not carry their own copy of BPMN semantics or mutation behavior.

### Command pipeline (the central contract)

Every persistent change, from GUI, CLI batch, or MCP, goes through `processcraft_core::commands::Session`:

- `Request::Batch { mutations: Vec<Mutation> }` / `Undo` / `Redo`. A batch is all-or-nothing and is one undo step; on failure state, history, and generation are unchanged.
- State is `Arc<ProjectData>` with copy-on-write (`Arc::make_mut`); `Session::snapshot()` hands immutable snapshots to queries, rendering, and background workers.
- Three distinct notions: **generation** (monotonic, used for `expectedGeneration` conflict checks and discarding stale worker results), **content identity** (SHA-256 of canonical persistent content; `dirty` = differs from last saved identity, so undo back to saved content is clean), and **checkpoints** (persistent revisions, unrelated to undo).
- A successful command returns a `ChangeSet` (not an event bus) used to invalidate derived state.

Adding a mutation means touching, in `crates/processcraft-core/src/commands.rs`: the `Mutation` enum variant (serde `type` tag, camelCase, `deny_unknown_fields`), its application in `Session`, and the `COMMAND_DESCRIPTORS` table. Tests in `crates/processcraft-core/tests/` assert that descriptors and the generated JSON Schema agree; CLI `commands --json` and MCP both expose this same schema, so no transport-specific command definitions exist.

### GUI structure

`apps/processcraft/src/app.rs` owns the `ProcessCraft` eframe app; its submodules live in `apps/processcraft/src/app/` (one per area: `canvas` for gestures, context pad and popups, `overlays` for the floating palette/zoom/arrange capsules, `shell` for menus, rail, document header, bottom panel and layout, `inspector` and the per-area property editors such as `event`, `data`, `collaboration`, `catalog`, `di`, plus `analyst_views` for library/dictionary/table and `review` for comments, checkpoints and layout preview). Modeling behavior (rules, append/connect, replace, move/resize, lanes, clipboard, align) lives in `processcraft-core` (`rules.rs`, `modeling.rs`, `replace.rs`, `manipulate.rs`, `containers.rs`, `clipboard.rs`, `arrange.rs`); the canvas only turns gestures into those mutations. Panels never mutate the model directly; they build `Mutation`s and submit them through `controller.rs` (`Controller` wraps the `Session`, save lifecycle, and recovery). Blocking work runs off the UI thread: `effects.rs` (import/export/file writes) and `jobs.rs` (render and recovery writers, latest-wins queues); results are polled and dropped if their generation/key is stale. `attachment.rs` (Windows only) exposes the visible session to `processcraft-cli mcp --attach` over a named pipe.

### BPMN interchange

`processcraft-io/src/bpmn.rs` does bounded, safe parsing into a source-preserving form; `interchange.rs` (`prepare_import` / `export_bpmn`) projects qualified profiles into the typed model. The governing rule: content outside a qualified profile is **preserved byte-for-byte or refused, never silently dropped or reinterpreted** — imports that cannot round-trip fall back to non-editable preserved source. Do not describe the result as complete BPMN 2.0.2 coverage or OMG conformance; `docs/research/bpmn-2-*.md` track what is and is not covered.

### Analyst layer (M5)

- **Attributes** (`processcraft-core/src/attributes.rs`): project-level `attribute_definitions` and `attribute_values` (keyed by `ElementKey`), typed (`Number`, `Currency`, `Duration`, `Enum`, `Date`, `DictionaryRef`, `Url`, `Text`). Entry points are the mutations `CreateAttributeDefinition`, `UpdateAttributeDefinition`, `DeleteAttributeDefinition` and `SetAttributeValues`; their logic lives in `attributes.rs` (`apply_*`) and is only called from `commands.rs`, which must not grow it inline. Element deletes prune values through one helper.
- **Conventions** (`conventions.rs`): `ConventionsProfile` is stored project content set by `Mutation::SetConventionsProfile`. Evaluation is the pure `conventions::diagnostics`, reached through `ProjectData::diagnostics()` (stored profile) or `diagnostics_with(profile)` (CLI `validate --profile FILE`); superseded built-ins are listed in `SUPERSEDED_BUILTINS`. All rules default to warnings.
- **RACI** is three Role-reference attributes (`RaciAttribute`: Accountable, Consulted, Informed; Responsible is the activity lane role), not a separate store. `queries::raci_matrix` / `raci_csv` project it; CLI `query raci` and `export-raci`.
- **Overlay** (`overlay.rs`): `attribute_overlay` computes buckets with exact decimal arithmetic and never mutates the project. The renderer draws it only through `BpmnScene::build_with_overlay`; it is screen-only unless an export asks for it (`export-image --overlay-attribute/--overlay-mode`, MCP `export` `overlayAttribute`).
- **Native format** stays schema 3. Files that use these features declare the required capabilities `analyst-attributes-v1` and `modeling-conventions-v1` (written only when used; older readers refuse them). **BPMN interchange** carries attribute values in a trailing `pc:attributes` extension element, namespace `urn:processcraft:attributes:1`; dictionary references are native-only and do not round-trip.
- GUI: `apps/processcraft/src/app/{attributes,conventions,analyst_views}.rs`; panels build mutations and submit them through the `Controller` like every other panel. MCP prompt pack: `docs/prompts/` (checked by `cargo test -p processcraft-cli --test prompt_pack`). Evidence, limits and the M5.6 qualification: `docs/implementation/m5-evidence.md`; repeatable CLI workflow: `scripts/m5_cli_workflow.py`; timings: `M5_PROBE=1 cargo run --release -p processcraft-cli --example workbench_qualify -- --out DIR`.

### Safety and platform constraints

- `unsafe_code = "forbid"` workspace-wide. `processcraft-io` relaxes this to `deny`, and only `save` (Windows), `recovery::private`, and `windows_pipe` may override it for reviewed OS adapters.
- Writable storage is qualified only for fixed local Windows NTFS. File access goes through explicit `FileGrant`s; MCP file effects require read/write roots (standalone) or GUI-authorized folders (attached). No listener starts by default.
- Parsers and archive readers enforce explicit size/count limits (e.g. `MAX_ELEMENTS` in `bpmn.rs`); keep new input paths bounded.

## Fixtures

`tests/fixtures/**` and `assets/fonts/**` are pinned byte-exact (`.gitattributes` disables text normalization) and their SHA-256 hashes are recorded in `tests/fixtures/manifest.json` or a sibling `*.manifest.md`/`.json`. Never reformat or re-save an existing fixture; a new fixture needs a manifest entry with provenance, license, hash, and expected import mode. `tests/fixtures/bpmn20-schema/` holds the pinned OMG XSD/CMOF used by the Python checks.

## Documentation map

- `docs/design/solution-design.md` — the architectural source of truth (crate ownership §4, commands/undo §7, native format §8, save/recovery §9, trust boundaries and limits §17, verification §20).
- `docs/plans/` — milestone plans. M0–M2 are implemented; M3 closed under its adopted required scope; `modeling-parity.md` (bpmn-js-level canvas modeling) and `ux-shell.md` (shell redesign: tokens in `theme.rs`, Inter UI fonts, craft-ui components, screen-only `Appearance::Canvas*`) are implemented; M4 (installer, accessibility/performance qualification, platform expansion) is in progress; `roadmap.md` sequences M5–M9 (attributes and conventions, handbook and process map, simulation, DMN, optional mining).
- `docs/implementation/*-evidence.md` — what was actually verified per milestone, with measured limits. Feature commits update the matching evidence file (and regenerate `docs/research/bpmn-2-coverage.json` when BPMN coverage changes), sometimes in a follow-up "Record …" commit; evidence states measured limits rather than broad claims.
- `design/` — FigCraft visual prototype and English previews; a design reference, separate from the Rust app. Its authoring batch and working files are local-only (git-ignored).
- The OMG `BPMN-2.0.2.pdf` is not redistributed (git-ignored); `scripts/generate_bpmn20_inventory.py` pins its hash so the inventory check passes with or without a local copy.
