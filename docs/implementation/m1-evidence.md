# M1 evidence — Tasks 05–16

November 11, 2026. The first native editor slice is implemented. Functional, no-loss and keyboard gates pass within the Windows/local-NTFS scope below. Two measured latency targets are missed and explicitly dispositioned; this is not a claim of the later agent-ready alpha, complete BPMN interchange or all-platform qualification. Terra subagents implemented and reviewed the components; integration checks exposed and corrected connected-edge movement, drag release, recovery notification and 200% layout defects.

## Task 05 evidence

The initial domain integration tests were run before implementation with `cargo test -p processcraft-core --test domain`; compilation failed as expected because `XmlId`, `ElementKey`, and `model` did not yet exist.

After implementation, `cargo fmt --all`, `cargo test -p processcraft-core` (3 unit tests and 9 integration tests), `cargo clippy -p processcraft-core --all-targets -- -D warnings`, and `cargo check --workspace` passed. A Clippy collapsible-if warning was fixed before the passing run.

The tests cover document-scoped identities, lane references, shared semantics across diagrams, unresolved references, incomplete and cyclic process graphs, persisted-content changes, geometry and negative zero, identifier deserialization, and preserved-source resource requirements. They do not establish full BPMN conformance, import/export behavior, or comprehensive field-by-field identity coverage. `identity.rs` holds validation and canonical hashing separately to keep the domain types focused. Resource bytes are hashed in ID order, independent of archive ordering; float negative zero is normalized and non-finite geometry rejected.

Follow-up: resource bytes use shared `Arc<[u8]>` and a verified SHA-256 descriptor; diagram planes and flow endpoint restrictions are validated; nested unknown serialized fields are rejected. The identity test now exercises representative changes across metadata, folders, resources, import capability, links, and DI. It is still not exhaustive field-by-field. Final rerun: 3 unit and 12 integration tests passed, core Clippy and workspace check passed.

## Task 06 evidence

A preliminary `cargo test -p processcraft-core --test commands` failed before implementation because the commands module did not exist. After implementation, `cargo test -p processcraft-core --test commands` passed 11 tests; `cargo test -p processcraft-core` passed the 3 unit and 14 domain tests as well as the command tests, and `cargo clippy -p processcraft-core --all-targets -- -D warnings` passed. `cargo fmt -p processcraft-core` was run.

Tests cover rollback on a late invalid connection including content, selection, generation and history; optimistic conflicts; single batch undo; clean undo to saved content; stale save acknowledgement; no-op behavior; the 200 gesture limit; document Arc sharing; capability restrictions; dangling reference rejection; and JSON unknown fields and batch size. The 128 MiB history accounting is conservative estimated allocation accounting, not a heap profiler measurement; resources are charged once by shared allocation and document snapshots once per distinct Arc. The procurement fixture is a typed batch and exercises cross-command IDs and a branching graph. Routing, file I/O and desktop integration remain outside this task.

Task 06 review follow-up: deletion now respects both protected capabilities; retained-extension document properties allow label changes while rejecting folder moves. History accounting now walks owned string and vector capacities, includes allocation overhead, and charges shared document and resource allocations once. New unit tests exercise eviction with a reduced byte budget and 16 KiB folder label, plus shared Arc accounting; integration tests cover protected document deletion and folder changes. Rerun: 5 core unit, 12 command and 14 domain tests passed, along with core Clippy and formatting. The eviction threshold test uses a reduced budget rather than allocating 128 MiB.

History budget follow-up: both undo and redo snapshots now count toward the shared 200-gesture and estimated 128 MiB limits after every successful mutation, undo or redo. Eviction removes the oldest undo first, then the furthest redo; a single oversized redo can be discarded. A reduced-budget test covers a large current state moved to redo, and the shared-Arc accounting test now spans both stacks. Rerun: 6 core unit, 12 command and 14 domain tests passed; core formatting and Clippy passed.

## Tasks 07–16: implementation and qualification

M0 remains separately qualified in [m0-evidence.md](m0-evidence.md). The final checks below supersede the historical focused test counts above.

| Task | Implemented scope and evidence to run |
| --- | --- |
| 07 Geometry | Bounded hit testing, deterministic orthogonal routing and endpoint repair are in `crates/processcraft-core/src/geometry.rs`; command and geometry tests cover transactions, geometry and routing. Run `cargo test -p processcraft-core --test geometry` and `cargo test -p processcraft-core --test commands`. |
| 08 Native format | `processcraft-io::native` implements bounded ZIP/JSON packages, content identity, resource digests, format/capability checks and schema migration. Run `cargo test -p processcraft-io --test native`. Migration requires Save As or backup; unsupported content is rejected rather than silently discarded. |
| 09 File sessions | Grants, locking, staged writes, verified native content, conflict detection and explicit commit outcomes are implemented in I/O. Run `cargo test -p processcraft-io --test project` and `cargo test -p processcraft-io --test storage`. Non-cooperating writers remain subject to the documented filesystem race boundary. |
| 10 Recovery | Private bounded snapshots, listing/opening/discarding and quota handling are implemented in I/O; scheduling is wired into the desktop. Run `cargo test -p processcraft-io` and complete the forced-termination GUI recovery gate below. |
| 11 Scene | Domain-driven BPMN scenes, labels and CPU viewport rasterization are in `processcraft-render`; tests cover semantic keys, themes, explicit colors, viewport clipping and font diagnostics. Run `cargo test -p processcraft-render`. Production export controls are deferred. |
| 12 CLI | `info`, `commands`, `validate` and transactional `run` use the shared engine and explicit paths; subprocess tests exercise output, conflicts, validation and ownership. Run `cargo test -p processcraft-cli --test cli`. BPMN import is preservation-only. |
| 13–14 Desktop | The native single-project workbench includes file actions, navigation, canvas, inspector, outline, palette, keyboard actions and Light/Dark appearance. Move gestures repair connected endpoints in one batch, preserve manual interior bends and support Escape. The FigCraft prototype is a separate design artifact. |
| 15 Lifecycle | Bounded jobs and recovery scheduling distinguish discardable stale render results from file effects that must be reconciled. Run `cargo test -p processcraft lifecycle` and the desktop recovery gate below. |
| 16 Qualification | The deterministic release example, native integration checks and results below establish the M1 scope. Known ceilings and performance dispositions remain explicit. |

## Measured qualification

Command: `cargo run --release -p processcraft-cli --example qualify -- --out target/m1-qualification`. The generated [measurement report](../../target/m1-qualification/measurements.json) and native fixtures are ignored build artifacts; rerun the command to regenerate them. The medium fixture has 20 documents, 5,000 semantic nodes, 500 active shapes and labels, a deterministic 20 MiB resource, and a 20,990,582-byte native archive. The scene is 2,022 × 1,022 logical pixels. The small fixture has 10 nodes; the bounded near-limit fixture has 2,000 nodes and shapes in one document. It is not evidence for 100,000-element interactive operation. A deliberately oversized document was rejected. Projects were validated and native archives loaded after writing.

Each warm measurement used five warmups and 30 samples, with nearest-rank p95 and wall-clock `Instant`. On Windows 10.0.26300.9818, x86-64, rustc 1.98.1, AMD Ryzen 5 5600X (6 cores/12 threads), 17,099,001,856 bytes RAM, and Samsung SSD 980 PRO NVMe, observed p95 values were:

| Operation | p95 |
| --- | ---: |
| Rename one node | 38.771 ms |
| Hit test 500 shapes | 0.005 ms |
| Search 5,000 nodes in memory | 0.264 ms |
| Raster full scene, 100% / 200% | 39.538 / 67.572 ms |
| Open native archive in memory | 86.166 ms |
| Serialize native archive in memory | 844.489 ms |
| FileSession save, including serialization and disk commit | 1,036.066 ms |

After 200 renames the undo count was 200. A separate external-process memory run sampled every 100 ms and observed peak working set 200,171,520 bytes (190.9 MiB), with maximum sampled private memory 213,053,440 bytes, including the whole release qualification and 200 retained gestures. The debug GUI with the medium project used 275,283,968 working-set bytes and 279,711,744 private bytes. These measurements are below the 512 MiB target; they do not isolate history allocations or prove every workload fits. See [memory.json](../../target/m1-qualification-memory/memory.json). The original timing report was preserved separately.

Five separately launched release CLI `info` processes took 231.897, 165.910, 164.814, 151.789 and 164.034 ms, including startup, read, validation and output capture; see [cold-process.json](../../target/m1-qualification/cold-process.json). The OS file cache was not flushed: these are cold-process, not cold-disk, results. Timing results are machine- and load-specific.

**Performance disposition:** the observed rename p95 exceeds the 20 ms target and FileSession save exceeds the 1,000 ms target. M1 accepts these known latency ceilings; neither target is reported as met. Before alpha/M2, profile incremental verified-resource hashing and compression, validate any optimization against integrity, package limits and no-loss tests, and repeat qualification. The current implementation validates the resource hash and includes resource bytes in content identity; native save also compresses the resource. The measurements do not isolate the cost of each. Validation and hashes were retained.

## Final gates

| Gate | Command or manual procedure | Status / artifact |
| --- | --- | --- |
| Formatting | `cargo fmt --all -- --check` | Pass |
| Lints | `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Pass |
| Workspace tests | `cargo test --workspace --all-targets --all-features --locked --offline` | Pass: 164 tests on the final source, including the actual-widget drag regression; [log](../../target/m1-qualification/workspace-tests-final.log). |
| Release desktop | `cargo build --release -p processcraft --locked --offline`; launch built executable and close normally | Pass: optimized build; responding native ProcessCraft window; normal close/discard exits 0. [Launch record](../../target/m1-ui/release-launch.json). |
| Dependencies | `cargo tree -p processcraft-core`; `cargo tree -p processcraft-cli -e normal`; `cargo metadata --format-version 1 --no-deps` | Pass: core has serde/JSON/SHA/UUID only; I/O and render depend on core, not each other; normal CLI has no GUI/render dependency. Its benchmark uses render as a dev dependency. No SQLite, browser shell or additional SVG rendering runtime. [Direct boundaries](../../target/m1-qualification/dependency-boundaries.json). |
| GUI and CLI | Native dialogs, palette/outline/inspector, complete procurement batch, Save As/reopen, Unicode, constrained width, Light/Dark and 100%/200% UI scale | Pass for the recorded slice. CLI/core fixture identity parity is asserted by subprocess test. Native GUI-created and GUI-edited packages reopen and validate in CLI. [Screenshots](../../target/m1-ui/) are ignored artifacts. |
| No-loss and recovery | Failed batch, external change, writer ownership, interrupted save, migration and preservation tests; forced termination and recovery | Pass. A real GUI-held project refuses a CLI writer with exit 4. Original SHA-256 is unchanged across forced GUI termination; startup Restore recovers the newer Unicode edit into a separate dirty session, saved to `recovered.processcraft`. |
| Keyboard and accessibility | Named outline and controls, endpoint selection, inspector, move/undo/redo, focus and Narrator smoke | Pass for basic names/actions. Native accessible control actions created/edited a model and connection; moving sets dirty, one Undo restores Saved, Redo restores dirty. Narrator's speech recap announced the selected task and Documentation field. Narrator was stopped; no further launch is permitted by the user's instruction. Full screen-reader operation of a complex process is not established. |
| Jobs | `cargo test -p processcraft lifecycle` | Pass: nine matching lifecycle tests, including all five required scenarios. Deterministic controller barriers verify save G / edit G+1 identity and disk acknowledgement without a GPU or production fault-injection switch. |

## Integration details and limits

- The procurement fixture includes two lanes, five nodes, five connected branch flows with conditions, explicit external labels, and deterministic semantic/DI IDs. The package opens visibly in the GUI; warnings do not prevent incomplete-model saves.
- A native project with two diagrams was opened in the GUI, its shared node documentation changed to `Shared semantics across two diagrams — café 采购`, and saved. Independent ZIP/JSON inspection confirmed both diagrams and the updated semantic node; CLI validation passed. M1 displays the first diagram; diagram switching remains a documented UI limit.
- Independent Python `zipfile`/JSON checks validated required entries, MIME, CRC, resource SHA-256 and absence of inline resource bytes; see `target/m1-native/independent-reopen.json`.
- Recovery interruption tests kill a child after payload staging is written and before flush/publication. The prior completed snapshot remains readable, incomplete staging is unlisted, and the original is unchanged. This is a transaction interruption test, not interruption halfway through `write_all`.
- The CLI indeterminate-outcome subprocess test uses an opt-in `test-fault` feature plus `debug_assertions`. It checks the actual executable's stdout, stderr and exit 1 using a synthetic I/O outcome. Actual replacement uncertainty and reconciliation are separately injected inside I/O tests. Normal and release builds contain no environment-triggered fault seam.
- Native unattended posted mouse messages were not accepted as proof of a physical-pointer workflow. A deterministic multi-frame egui `RawInput` regression drives an actual widget response and the runtime gesture helpers through press, movement, release and Escape, asserting one command history entry and no cancellation entry. Connected endpoint repair and group undo also have core session regressions.
- At 200% UI scale all panes clip to their own bounds, canvas tools wrap, fields fit, and rasterization accounts for physical pixels. Panels can be collapsed at constrained width. Theme, viewport, scale and pane changes leave persisted content clean.
- Orthogonal routing has an explicit bounded corridor search and can warn/refuse; it is not a complete obstacle router. Existing manual route interiors survive node movement. Raster output retains the M0 64 MiB RGBA ceiling; very large individual shapes can be omitted with a diagnostic while local content remains usable.
- Fixed local Windows NTFS is the qualified writable filesystem. Private recovery uses a protected current-user DACL, serialized quota accounting and at most two completed snapshots per project in the normal successful path. Cleanup failure reports an error and can retain extras rather than deleting the only recovery copy.
- Editable BPMN import, analyst tables, dictionary workflows, production export controls, MCP attachment, checkpoints, packaging and other-platform qualification remain M2–M4. No SQLite was added. No Git repository was present, so no commits were created.
