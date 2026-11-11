# M2 evidence — Tasks 17–23

November 11, 2026. Implements the six M2 roadmap slices using Terra implementation and independent spec/quality reviews. The accepted Rust/offline architecture, portable ZIP/JSON ownership and single mutation engine remain intact. No SQLite, second diagram renderer, account or default network listener was added. Narrator was never launched during M2, as explicitly instructed by the user.

## Implemented scope

| Slice | Actual behavior |
| --- | --- |
| M2.1 Library | Stable DocumentId search/filtering and folder create/rename/move/delete. Nonempty deletion and cycles are refused transactionally. GUI and CLI share projections. |
| M2.2 Dictionary | UUID entries for Role/System/Document; typed ElementKey relationships resolve live labels. Reverse usages, explicit same-kind replacement or literal unlink, duplicate-merge handling, undo and complete persistent identity. Role assignment chooses a lane explicitly when ambiguous, creates a lane when unmapped, and moves connected DI in the same batch. |
| M2.3 Table | Actual column grid over stable activity IDs; names/links/roles dispatch core commands. Outgoing names/conditions are read-only. Presentation never rewrites branches. Insertion is explicitly limited to qualified simple chains, one diagram and horizontal two-point DI; ambiguous branches refuse without mutation. |
| M2.4 BPMN | Qualified flat tasks/user/manual, none start/end, exclusive/parallel gateways, sequence flows, flat lanes, plain documentation, supported formal conditions, external DI labels and multiple diagrams. Every unknown/unrepresented element or attribute downgrades to exact preserved source. Pools and other unqualified constructs remain source-only. |
| M2.5 Images | Full shared scene produces SVG/PNG, explicit bounds/scale/dimensions and preflight diagnostics. Fractional-scale SVG uses the same uniform scale/padding as PNG. Preserved-source placeholders cannot be exported as diagrams. Generation precedes atomic output replacement. |
| M2.6 Automation | Official rmcp 3.9.0 stdio, actual shared request schema, inspect/query/validate/apply/save/export/reconcile. GUI attachment uses protected current-user Windows pipes, explicit enable and root grants, a bounded owner queue and the actual visible mutable session. |

The desktop offers Editor/Library/Dictionary/Table and process/diagram selectors in English Light/Dark. Filters and theme changes are not persistent model mutations. Accessible labels cover new edit fields; no Narrator validation is claimed. Native UI controls and deterministic egui tests cover entry drafts, selection, roles, history, cancellation and effect lifecycle.

## Native format and no-loss boundaries

Schema 2 includes dictionary entries and relationships. Schema 0/1 identity is verified against the original field set before migration; opening does not overwrite source and replacing the old representation requires Save As. Resources remain content addressed. Native reader limits, strict duplicate/unknown fields and required capability refusal remain in force.

Integration found that comparing an old recovery record to the migrated identity hid valid M1 snapshots. The loader now exposes its verified source identity; recovery compares that identity while retaining payload digest, project ID and metadata checks. A regression rebuilds a schema-1 recovery snapshot, verifies discovery/restoration and unchanged source bytes, and rejects a mismatched record identity.

Native and raw export writes retain originating context and distinguish NotCommitted, Committed and Indeterminate. Uncertain native saves retain FileSession; uncertain exports retain writer and intended bytes. A disconnected writer is treated as unknown and blocks transitions. Acknowledging a message is not reconciliation. GUI and IPC export reject the active native target, including aliases/hardlinks, before writing. Exports do not change the native save binding or saved-content identity.

Independent reviews corrected pending-export races through Recovery, after-save transitions and Discard, and cancellation of Open preserving drafts. Dictionary Undo/Redo and remote updates refresh committed editors without manufacturing or silently reapplying drafts; reselecting an entry preserves its unsaved text.

## Automation boundary and ceilings

- Default off. Explicit enable creates an unpredictable local pipe and token. ACL runtime tests verify current-user ownership and a protected sole owner ACE; first-instance and remote-client rejection flags are set. Endpoint validation prevents remote/ordinary-file connections. Same-user filesystem race limits from the qualified I/O boundary still apply.
- Private pipe frames and MCP input lines are capped at 4 MiB before parsing. Duplicate nested JSON keys are rejected before map deserialization. Stdio remains standard newline JSON-RPC owned by the official SDK, with no progress text on protocol stdout.
- Root grants come from explicit standalone launch arguments or the GUI folder picker. Attached clients cannot enlarge grants through JSON or CLI roots. Existing ancestor handles and reparse/hardlink restrictions remain effective. Recovery access is not an automation grant.
- Mutations and effects carry session, generation and request ID. Repeated completed requests return cached outcomes; changing payload/context under an ID conflicts. One pipe client and a 16-entry owner queue bound transport work. The ordinary ledger retains at most 128 IDs and 16 MiB results and refuses new work when full rather than forgetting and replaying it.
- Standalone reconciliation has a separate eight-ID session budget and cross-checks request IDs with ordinary effects. Unknown operations do not consume that budget. Exhaustion prevents new save/export before path access and requests manual investigation; in-memory edits remain possible within the ordinary budget. Reconciliation never repeats physical replacement. A completed original request stays cached as originally returned.
- Attached uncertain effects are reconciled in the GUI. MCP returns an explicit `reconciliationViaGui`/unknown outcome rather than claiming settlement. Abrupt worker loss before a writer can be retained blocks transitions with captured operation/session/target context and requires manual investigation. There is no durable exactly-once or automatic crash replay claim.

## Actual native attachment smoke

An actual native window, not a copied Session harness, was connected through a real rmcp subprocess and Windows pipe:

1. `inspect` identified the visible project; `apply` renamed it to `Procurement attached denied`. The native title/status reflected that mutation. Repeating the same request returned the same result with one generation increment.
2. Save outside an authorized root returned `pathNotAuthorized`; no output was created. GUI Undo restored `Procurement` and Saved status.
3. The native folder picker explicitly authorized only this checkout's `target/m2-qualification` directory. A new remote rename, Save As and SVG export returned committed receipts after GUI-owner polling. The native UI showed Saved and the new binding; CLI info/validate reopened the written package, and Python independently parsed the SVG.
4. Attachment was disabled after the test. Connection tokens were not printed or written into evidence artifacts.

Records: `target/m2-qualification/gui-attachment-{denied,effects}.json`, `gui-attached.processcraft`, `gui-attached.svg`. The ignored `gui-attachment-smoke.py` documents the subprocess procedure and obtains credentials only from environment values read from the enabled GUI. GUI Undo/Redo, dictionary rename propagation and 100%/200% Light/Dark were also exercised. Native PrintWindow captures avoid interacting with unrelated windows.

## Interchange and independent inspection

`tests/fixtures/bpmn/m2-flat-lanes.bpmn` is synthetic CC0 data with a SHA-256 manifest. Tests cover supported semantic/DI round trips, multiple diagrams, unsupported standard/foreign attributes, namespace-sensitive conditions, clipping diagnostics and fractional image scale. Prefix-dependent conditions and unavailable namespace context preserve the entire original or block export; no expression is executed.

The canonical procurement model with two lanes and dictionary links was exported with an explicit native-field omission acknowledgment, reimported as Editable, and checked independently. Definitions ID, processes (including conditions), collaboration and every diagram were equal to the source native model. Python ElementTree parsed the XML; the report and SHA-256 are in `target/m2-qualification/independent-interchange.json`. PNG `procurement.png` was visually inspected; it contains both lanes, branch labels and complete external labels.

This is not OMG XSD validation, MIWG coverage or Signavio/vendor compatibility certification. Native-only dictionary/navigation/owner data requires an explicit omission acknowledgment for BPMN. XML whitespace/prefix spelling may change for Editable; preserved source remains byte-for-byte. The existing 64 MiB RGBA / 16,777,216-pixel raster ceiling remains, with a 16,384-pixel axis limit.

## Performance

Command: `cargo run --release -p processcraft-cli --example m2_qualify -- --out target/m2-performance-optimized`. Same fixture and five warmups/30 measured samples before and after the observed-cost fixes: 20 documents, 5,000 nodes, 100 lanes, 30 entries, 10,100 typed relationships, 4,980 flows and a 20 MiB resource. Table projection has 500 rows. Hardware is the qualified Windows x64 Ryzen 5 5600X / 16 GiB / Samsung 980 PRO NVMe configuration; background activity and warm OS caches limit generalization.

| Operation | Initial p95 | Final p95 |
| --- | ---: | ---: |
| Library filter | 0.01 ms | 0.01 ms |
| Dictionary usages | 0.81 ms | 1.02 ms |
| Activity table, 500 rows | 21.62 ms | 22.22 ms |
| Rename | 143.31 ms | 134.01 ms |
| Native serialization | 1,062.32 ms | 321.82 ms |
| Native FileSession save | 1,861.03 ms | 949.95 ms |
| Native open | 320.67 ms | 358.03 ms |

Validation now builds ephemeral source-kind sets once rather than searching document/node lists for every relationship. ZIP Deflate level 1 reduces measured save cost; the archive grows from 21,133,428 to 21,218,947 bytes (about 0.4%) and preserves canonical identity and all reader limits. A resource probe measured rename without the resource at 139.43 ms and SHA-256 of 20 MiB at 10.41 ms; attributing rename primarily to resource hashing would be unsupported. Repeated model serialization and history accounting remain profiling targets.

Final table digest matches the original run: `5f8dc6fb3a3dad0a606f07f9d75c6d07f87ce4633c211fd357e641ff5c3d83af`. Both reports have no correctness errors. Peak working set observed in the final release qualification was 319,848,448 bytes (about 305 MiB). After 200 renames, 50 undo entries remained due to the shared 128 MiB history ceiling; 200 is a maximum count, not a promise to retain all gestures at any size.

**Disposition:** library, table, memory and medium save targets pass on this machine. Rename remains above the 20 ms target and is accepted as an explicit M2 latency limitation. Before expanding large-model editing, profile incremental canonical metadata/history work against this fixture. Do not skip trust-boundary validation, introduce SQLite or guess a pass. Baseline, optimized and resource-probe JSON remain separate under ignored `target/m2-performance*`.

## Final checks

Dependency inspection confirms toolkit/OS-free core (including shared schemars schemas), separate I/O/render libraries and a GUI-free CLI. Tokio/rmcp are confined to CLI; normal GUI operation does not start MCP. Commands use the pinned toolchain and lockfile with cached offline dependencies.

| Gate | Result |
| --- | --- |
| Independent component spec and quality reviews | Pass; reported blockers corrected and rechecked |
| Format / all-target, all-feature Clippy | Pass: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` |
| Workspace all-target, all-feature tests | Pass: **217 tests**, `cargo test --workspace --all-targets --all-features --locked --offline`; `target/m2-qualification/workspace-tests.log` |
| Release desktop and CLI | Pass: `cargo build --release -p processcraft -p processcraft-cli --locked --offline`; responding native release window and normal close exit 0; release CLI reopened/validated the attached GUI package |
| Native four views, draft/history and actual attachment | Pass within the stated manual/automated scope |
| Qualified interchange and image output | Pass within the stated profile; no XSD/vendor claim |

No Git metadata exists in this checkout, so no commits/worktrees were created. Comments, checkpoints/reference comparison, product PDF, broader interchange, packaging and broad platform/accessibility qualification remain M3–M4. No Narrator session or full screen-reader qualification was performed during M2.
