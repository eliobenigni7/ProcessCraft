# M3 partial-delivery evidence

M3 work resumed on 2026-11-11 at the user's explicit request. The earlier Windows partial-close evidence below remains historical; it is not a Cloud result. M3 and full BPMN 2.0.2 modeling/interchange remain unfinished.

## Supporting-catalog and activity-binding Cloud checkpoint

Source commit `013f382` adds typed item definitions, interfaces and operations, resources and resource parameters; a shared atomic `setSupportingCatalog` command; native catalog and binding editors; and paginated catalog query parity. Optional IDs and explicit/omitted default values survive native and XML roundtrips. Interface operation cardinality, global IDs and known local message/error/item references are validated; external type/service targets remain inert QName data. Catalog edits that create dangling activity operation references roll back. Inherited documentation, extensions and foreign metadata still cause source preservation and are not completed by accepting the root element names.

Service tasks now expose operation bindings; Send/Receive expose message and operation bindings. The codec preserves local, external and empty-namespace QNames, including owner scopes with loops, documentation, I/O and associations. Namespace changes do not silently retarget references. New local wrong-kind or missing references are refused; historical unresolved or non-exportable reference metadata remains readable, with diagnostics and unchanged identity. Editing unrelated fields does not erase or newly reject that historical data. No unsupported equality between task messageRef and operation in/out messages was inferred from the specification; complete I/O compatibility remains unverified.

The native schema stays 3. Nonempty supporting catalogs in live documents or checkpoints require `bpmn-supporting-catalog`. Activity-binding fields already existed in the legacy format, so nonempty historical QName values do not gain a new capability. The newly accepted empty-namespace activity references require `bpmn-empty-namespace-activity-bindings`, including nested/frozen content; missing declarations are rejected. Tests cover both compatibility paths.

`processcraft-cli query <input> catalog <document-id> --offset 0 --limit 50` and MCP `query` kind `catalog` expose the shared core projection. Each item contains a JSON Pointer `path` and one scalar or empty-container `value`; large nested operation lists are not truncated into oversized individual response items. Pages carry content identity, plus generation in session-based MCP responses. The real stdio MCP test applies the catalog command and verifies multiple pages, exact values, advancing offsets and stable identity. Attached Windows dispatch uses the same core projection, but its native runtime remains unqualified here.

Final integrated results for this checkpoint:

| Gate | Actual result |
|---|---|
| Formatting and workspace all-target/all-feature Clippy with `-D warnings`, locked/offline | Passed; `target/m3-catalog-clippy.log`. |
| Workspace all-target/all-feature tests, locked/offline, `--no-fail-fast` | **319 passed, 15 failed, 0 ignored across 37 targets**; exit 101, `target/m3-catalog-workspace-tests.log`. The same 15 existing CLI filesystem/grant-dependent failures listed below remain. This is not a passing full workspace suite. |
| Normal release desktop and CLI | Passed; `target/m3-catalog-release.log`. |
| `catalog_qualify` | Passed native in-memory identity/reopen and complete typed catalog BPMN roundtrip; both actual source and output XSD-valid. `target/m3-catalog-qualification.log`. |
| `activity_bindings_qualify` | Passed native identity/reopen and typed process/catalog BPMN roundtrip; both actual source and output XSD-valid. `target/m3-activity-bindings-qualification.log`. |
| Synthetic fixtures | CC0 provenance/version/hash manifests beside `supporting-catalog.bpmn` and `activity-bindings.bpmn`; both hashes independently verified. No type/schema/service location was fetched or executed. |
| Inventory and preservation | Generator `--check` passed; pinned normative sources, Cargo.lock, dependency revisions and toolchain unchanged. |

The new catalog/binding GUI controls have automated guarded-draft/history tests. The earlier Xvfb pointer smoke below covered expression metadata, not every later catalog/binding workflow. Windows NTFS safety, named-pipe dispatch, full native accessibility/DPI coverage, independent tool reopen, complete BaseElement inheritance, data assignments/resource roles, root/referenced/multiple events, global/callable catalogs, collaboration/choreography and full DI remain open. M3 is still unfinished; the scoped matrices and original work packages remain the continuation record.

Reproduce the new codec evidence with `cargo run -p processcraft-io --example catalog_qualify --locked --offline` and `cargo run -p processcraft-io --example activity_bindings_qualify --locked --offline`, then validate their generated input/output files under `target/m3-qualification/` with `scripts/check_bpmn20_xsd.py`. As with earlier examples, these are synthetic codec artifacts, not qualified file-session saves.

## Expanded Cloud modeling checkpoint

Source commit `962ea6e` extends the preceding definitions/import work. It does not complete M3. The product-quality objective now explicitly calls for reproducible, named-version comparisons with tools such as Signavio and Camunda across the agreed native/offline modeling workflows; no superiority claim has been established by these tests.

### Implemented behavior

- Every existing expression owner uses one `Expression` value: a legacy JSON string or structured text with optional XML ID, Base/Formal type and explicit-type presence, optional language/result QName, and effective namespace bindings. Legacy string serialization remains unchanged. Flow conditions, timer/conditional events, gateway activation, standard/multi-instance loops and ad-hoc completion share this representation. Core validates XML characters, identity collisions, namespace bindings and representability; empty expressions remain editable with an advisory. All current owners declare `tExpression`, so Formal metadata requires an explicit type. In particular, a conditional-event condition is not implicitly Formal in the pinned XSD.
- Import captures namespace scope. Export declares namespaces at narrower scopes and verifies structured expression metadata and namespace meaning after reparse; incompatible owner contexts cause an explicit refusal. Binding order is not treated as semantic identity. The desktop provides persistent typed metadata drafts, native fields for ID/type/language/result QName/bindings, explicit Apply/Cancel and protected navigation/history. Text edits retain structured metadata. CLI/MCP discovery and activity-table queries retain the whole expression instead of flattening it to text.
- A bounded inline Link/Cancel/Compensate profile has core commands, context/reference checks, desktop fields, codec paths and native capability handling. Link references resolve to opposite-direction definitions in the same scope with the same name; missing pairing remains advisory. Complete reciprocity is not yet enforced. Cancel is limited to interrupting transaction boundaries and transaction-scope end events. Compensation supports the qualified start/event-subprocess, boundary and throw/end contexts and local activity references. Optional `waitForCompletion` presence survives roundtrip. Multiple/root/referenced event definitions and full event-subprocess metadata remain open.
- The Cancel glyph is an outlined X for catching boundary events and a filled X for throwing end events. Link arrows and paired compensation triangles are tested on validated context fixtures. Semantic hit tests now use circles for all event kinds and diamonds for all gateway kinds.
- Moving a qualified single-source data association repairs its visible endpoints, including verified activity input/output ports. Conversation links use the shared repair path. Tests apply moves of both endpoints through valid sessions, checking route/label preservation and exact Undo/Redo identities. Multi-source data associations, unresolved visible endpoints and the existing routing/collision budgets remain explicit limits.
- The bounded XML preservation validator distinguishes external QName targets from local ID references. External targets can remain unchanged source without being loaded or falsely resolved against an identically named local ID. Local ID checks remain in place. Link source/target text and compensation activity references are checked as expanded QNames.

Native schema remains 3. Structured expressions require `bpmn-structured-expressions`, and the newly qualified inline event variants require `bpmn-link-cancel-compensation`, in current documents and nested checkpoint content. Missing declarations are rejected and old readers must refuse unknown required capabilities. Native tests cover legacy strings, nested traversal, live/checkpoint round trips and missing-capability rejection.

### Integrated checks

| Gate | Actual result |
|---|---|
| Final formatting and all-target/all-feature Clippy with `-D warnings`, locked/offline | Passed; `target/m3-expanded-clippy.log`. |
| Workspace all-target/all-feature tests, locked/offline with `--no-fail-fast` | **300 passed, 15 failed, 0 ignored across 34 targets**; exit 101, `target/m3-expanded-tests.log`. The same 15 CLI persistence/grant-dependent cases listed below remain unavailable on Linux. No test was skipped. |
| Normal release desktop and CLI | Passed; `target/m3-expanded-release.log`. |
| Normative inventory generator `--check` | Passed: 580 declarations, not a conformance denominator. Pinned normative files, Cargo.lock, toolchain and dependency revisions are unchanged. |
| `expressions_qualify` example | Eight expression positions passed in-memory native identity/reopen and BPMN metadata roundtrip; actual source and output passed pinned XSD. `target/m3-expression-codec-qualification.log`. |
| `inline_events_qualify` example | Typed process/event data passed native identity/reopen and BPMN roundtrip; actual source and output passed pinned XSD. `target/m3-inline-events-qualification.log`. |
| Fixture provenance | Synthetic CC0 fixtures and version/hash manifests committed beside `structured-expressions.bpmn` and `inline-events.bpmn`; hashes independently checked. No external consumer qualification follows. |

Reproduce codec evidence with `cargo run -p processcraft-io --example expressions_qualify --locked --offline` and `cargo run -p processcraft-io --example inline_events_qualify --locked --offline`, then pass each generated `input.bpmn` and `output.bpmn` under `target/m3-qualification/` to `scripts/check_bpmn20_xsd.py`. These examples write synthetic qualification artifacts; they do not qualify the Windows file-session adapter.

### Native Linux inspection

The shared-desktop control tool remained unavailable as recorded below, but a separate native release smoke test succeeded on isolated Xvfb `:99`, 1440 × 900, with XTEST pointer/keyboard input and captured X11 screenshots. A private temporary XDG profile was used. The window was verified by X11 title/class, not merely by PID. Artifacts and input helper are under ignored `target/m3-qualification/native-smoke/`.

The smoke test created an intermediate catch timer, opened the new expression editor, retained invalid ID `1bad` with a visible error, refused table navigation without dropping that draft, corrected the ID to `TimerExpression`, applied metadata and properties, and inspected the stored metadata. Undo removed the ID; Redo restored the committed history state. Light 100%, Dark 100% and Dark 200% were inspected. At 200%, the bounded Properties window scrolls to the new metadata fields and Apply/Cancel actions. An ordinary window-close request showed the unsaved-change guard; the synthetic unsaved model was explicitly discarded and the native process exited normally. The expected Windows-NTFS recovery refusal remained visible. No production/user documents were opened or discarded.

This is a narrow native smoke test, not full Light/Dark/DPI/keyboard/accessibility qualification for every family. No screen reader or audio assistant was launched. Windows NTFS save/locking/ACL/recovery and named pipes, independent external consumers, full mixed/XML expression content with inherited documentation/extensions, root catalogs, full events, choreography, DI and the remaining matrix rows are still unfinished.

## Cloud continuation: definitions and import declarations

Starting checkout: clean branch `work`, commit `5a90a7626bfeec0b779e7faa0b537eb1efbc6a15`. The Linux Cloud executor uses the pinned Rust 1.98.1 and existing Craft revisions and lockfile. Load `/workspace/.processcraft-setup/env.sh` before Rust commands. `cargo fetch --locked` succeeded before offline checks. No repository visibility, dependency revision or file-adapter safety policy was changed. Terra workers handled independent core, interchange, native, desktop and crosswalk lanes; the coordinator integrated them.

Implemented a bounded definitions profile: explicit target namespace, optional name/language/exporter attributes, and ordered import declarations containing namespace, location and importType. Optional attributes retain absence versus explicit empty strings. Imports are inert model data: locations are never fetched or executed. The shared `setDefinitionsMetadata` command replaces the metadata and import list atomically; desktop add/edit/remove controls, CLI discovery and MCP all use that command. Undo/Redo, no-op behavior, rollback, checkpoint field comparison and frozen native content are covered by focused tests. The generated target namespace is shared between core and exporter; initializing metadata without changing that namespace remains possible when local call references exist. Namespace retargeting with existing qualified references is refused.

The native schema remains 3. Metadata in either live documents or frozen checkpoints requires `bpmn-definitions-metadata`; unknown-capability readers must refuse it, and archives that omit the capability despite containing metadata are rejected. Absent metadata and empty import lists do not introduce serialized fields into legacy identities. Existing migration checks remain in force.

Limits are explicit: at most 256 imports, at most 16 KiB per metadata string, XML 1.0 characters, and a nonempty target namespace for the qualified editable/export profile. These are implementation bounds, not normative restrictions on BPMN xs:anyURI. Unsupported attributes/content, out-of-profile metadata and import declarations fall back to unchanged source where the existing bounded XML validator accepts the input. External QName references can still be refused by that validator; import declarations do not provide external reference resolution. Old imported native documents without explicit definitions metadata keep the provenance-based exporter fallback; their metadata editor refuses changes until the original namespace can be verified. Definitions without IDs, rich expressions, relationships, extensions and supporting root catalogs are not completed by this slice.

Reproduce the codec evidence with:

```sh
source /workspace/.processcraft-setup/env.sh
CARGO_INCREMENTAL=0 cargo run -p processcraft-io --example definitions_qualify --locked --offline
python scripts/check_bpmn20_xsd.py target/m3-qualification/definitions/input.bpmn target/m3-qualification/definitions/output.bpmn
```

The example passed native in-memory write/reopen identity and BPMN export/reimport metadata equality, then wrote synthetic artifacts under `target/m3-qualification/definitions`. Both actual input and output passed independent pinned OMG XSD validation. It does not exercise the Windows file-session adapter. Its committed synthetic fixture, license, version and hash are in `tests/fixtures/bpmn/definitions-imports.{bpmn,manifest.json}`. No named external consumer has reopened this fixture; there is no broad compatibility claim.

The first workspace run overlapped worker edits and is retained only as a transitional diagnostic log (`target/cloud-baseline-tests.log`), not as an immutable clean-baseline qualification. The final integrated gates are recorded separately below. Narrow target-gated imports/annotations address pre-existing Linux unused-code warnings without changing the Windows adapter or ignoring tests.

The Cloud desktop control attempt failed before inspection with `the gaas-browser node_repl sandbox cannot be disabled`. New native pointer/keyboard, Light/Dark, DPI and accessibility qualification is therefore not established by this run. Automated egui draft/history tests remain separate evidence. Narrator and other audio assistants were not launched. Windows NTFS save/lock/ACL and named-pipe qualification, independent licensed-fixture reopen in two named tool/version combinations, and full normative coverage remain open.

The continuation order and concrete acceptance tests are in the M3 implementation plan: formal expressions and namespace context, root catalogs and references, then dependent activity/event/data/collaboration/choreography/DI slices. The family crosswalk and 580-declaration inventory remain partial engineering evidence, not a conformance percentage or completion gate.

### Integrated Cloud gates

Final source implementation commit: `7493782`; isolated Linux lint adaptation: `f3764fd`. All commands below used the pinned lockfile and dependencies; `CARGO_INCREMENTAL=0` was set for build, lint and test work.

| Gate | Actual result |
|---|---|
| `cargo fmt --all -- --check` | Passed on the final source. |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Passed; `target/cloud-clippy-final.log`. |
| `cargo test --workspace --all-targets --all-features --locked --offline --no-fail-fast` | **270 passed, 15 failed, 0 ignored across 32 targets**; exit 101, `target/cloud-tests-final.log`. This is not a passing workspace suite. All failures are in the existing CLI integration target and exercise unavailable Linux file/grant or Windows persistence paths. |
| `cargo build -p processcraft -p processcraft-cli --release --locked --offline` | Passed; `target/cloud-release-build-final.log`. Both normal release binaries were built; no backend features or dependency revisions changed. |
| `python scripts/generate_bpmn20_inventory.py --check` | Passed: 580 declarations; not a coverage percentage. |
| Definitions qualification example and pinned XSD checks | Passed, including actual exported XML; `target/cloud-definitions-qualification.log`. Synthetic fixture hash matches its committed manifest. |
| New desktop definitions/import draft tests | Both passed; these are automated egui state/history tests, not native pointer/accessibility qualification. |
| New MCP definitions test without file grants | Passed: real stdio initialization, discovered schema, shared mutation, retry deduplication and exact Undo/Redo content identity. |
| Windows NTFS, named pipes, new native interaction and independent consumers | Not qualified in this environment. |

The 15 failed CLI tests are `batch_failure_writes_nothing`, `bpmn_limitations_are_reported_without_validation_failure`, `cli_import_and_export_bpmn_svg_png`, `cli_matches_engine_content`, `cli_queries_persisted_comments_and_checkpoint_reference`, `exit_codes_and_nested_schema_are_explicit`, `gui_owned_file_refuses_headless_writer`, `headless_run_creates_no_recovery_or_preferences`, `indeterminate_save_subprocess_emits_reconciliation_and_exit_one`, `layout_file_preview_apply_and_stale_guard`, `mcp_stdio_uses_real_handshake_and_shared_schema`, `oversized_batch_is_rejected_before_output`, `paths_are_explicit_grants`, `query_library_and_dictionary_return_json_without_side_effects`, and `schema_and_validation_exit_codes`. The original NTFS refusal remains unchanged; no test was skipped or rewritten to disguise it.

The Analytic-profile condition concern was also audited: the validator already allows activity conditional flows in the general profile. A new positive/negative regression checks that activity flow conditions remain allowed while event-based gateway conditions receive their existing context-specific diagnostic (§8.4.13 and §10.6.6). No broader semantic-conformance claim follows.

## PDF export

`cargo run -p processcraft-cli --example pdf_qualify --locked --offline` generated Light, Dark and four-page tiled fixtures in `target/m3-qualification/pdf`. `python scripts/check_pdf.py` independently parsed PDFs with pypdf and Poppler, rasterized them with `pdftoppm` at 96 dpi, and wrote `report.json`. It checked A4 landscape page dimensions and count, rejection without acknowledgement and above 100 pages, no image XObjects or embedded fonts in the generated fixture resources, and no extractable text. The explicit outline notice was present. Comparison with the same scene PNG gave mean absolute RGB differences of 1.893/255 (Light) and 1.714/255 (Dark); four tiled pages were 0.071–0.780/255, excluding two pixels at tile seams. Latin accents, Arabic and CJK were visually inspected in rasterized samples. These comparisons do not establish accessibility, text searchability or correctness for all files. PDF text is outlines and is not selectable, searchable or tagged.

The tiling clips at page boundaries. Visual inspection showed a shape and its text split between pages; the feature does not keep BPMN elements together or repeat page context. The two-pixel seam exclusion means parity at the exact seam was not measured.

## Auto-layout

`cargo test -p processcraft-core --test layout --locked --offline` covers deterministic repeated preview, a branch/join, cycle and self-loop, one pool with lane and unlaned nodes, labels and explicit styles, event/gateway border endpoints, malformed/missing/duplicate DI and cancellation, proposal tampering, exact session apply/no-op/undo/redo, 400 nodes, and 500-edge refusal/acceptance without mutating the source. Tests are fixture-scoped; they do not establish all BPMN structures or imports.

A bounded straight-route fast path handles the connected 400-node / 399-edge linear fixture without changing the shared manual router. A second fixture adds 101 non-adjacent edges (400 nodes / 500 edges); it was **refused with `routing`**, without changing the source. Complex diagrams can be refused on collision, crossing, shared channel, obstacle limit or the 500,000-unit routing-work budget. The shared corridor-search helper still limits non-straight routing to 256 obstacles; 400 connected nodes therefore does not imply arbitrary 400-node layouts are supported. Loop/nested and newly modelled BPMN kinds require separate qualification; only the explicitly checked subset is eligible.

On this machine (Windows 11 Pro 10.0.26300, AMD Ryzen 5 5600X, 6 cores / 12 logical processors), one debug-build test-process wall-clock measurement for the 400-node chain plus the attempted 500-edge fixture was 293 ms. This includes test process startup and both cases, is a single sample, is not a release benchmark or a guarantee on other hardware, and does not establish the plan's worst-case two-second target.

## Renderer

At the nested-renderer checkpoint, `cargo test -p processcraft-render --locked --offline` passed the renderer suite and `cargo clippy -p processcraft-render --all-targets --locked --offline -- -D warnings` passed. Tests cover nested semantic IDs, a subprocess child plane, expanded/collapsed visibility in full scene and viewport, child flow labels, standard/multi-instance markers, event hit shapes and explicit color preservation. Parsing, schema preservation and end-to-end UI navigation remain separate qualifications. A missing `isExpanded` value remains missing; the renderer reports its ambiguity rather than silently asserting a normative default.

## Nested geometry move

`cargo test -p processcraft-core --test geometry --locked --offline` passed 16 tests after nested move changes. The targeted fixtures verify a subprocess drag moves child shapes, translates its internal route and explicit edge-label bounds in one undoable batch, preserves label style, repairs a directly moved nested child route, and refuses conflicting parent/child deltas without mutation. The existing direct multi-selection policy keeps manual interior bends. At that historical checkpoint, connected data associations and conversation links were refused when their endpoints moved. Later bounded endpoint-move tests cover selected data associations and conversation links; full family and native interaction qualification remains open. These tests do not cover every nested BPMN family or native pointer interaction.

## Checkpoint and native limits: synthetic release run

Reproduce with `CARGO_INCREMENTAL=0 cargo run -p processcraft-cli --example m3_qualify --release --locked --offline -- --large` (allocates roughly 256 MiB of resource bytes and can use substantially more working memory). Generated synthetic archives and measurements are in `target/m3-qualification/checkpoint`; no user files were used. Results below are one run on Windows 11 Pro 10.0.26300, AMD Ryzen 5 5600X, 6 cores / 12 logical processors; they are not a statistical benchmark.

- A 32 MiB resource passed model validation; 32 MiB + 1 byte was rejected. Twenty checkpoints were accepted and survived native reopen; the 21st was rejected without changing content identity. The 20-checkpoint small fixture had 20 undo entries.
- Eight distinct, highly compressible synthetic resource blobs totaling 268,369,920 bytes (256 MiB minus 64 KiB), plus project/checkpoint metadata, were saved and reopened with the frozen reference and content identity intact. The archive was 1,226,572 bytes. Write took 639.7 ms and reopen 646.8 ms; the entire large-fixture phase took 2,805.9 ms. The test did not locate the exact largest expanded-byte payload; eight full 32 MiB blobs plus metadata were rejected by the expanded-size validator. The 64 KiB gap is a deliberately conservative near-limit case.
- Independent Python `zipfile` inspection found eight resource entries for those eight unique hashes, shared between current and frozen content rather than duplicated for the checkpoint. The one-resource 32 MiB fixture similarly had one blob entry and retained its reference after reopening. Compressed sizes reflect repeated-byte test content and do not predict real-file sizes or qualification of the separate 128 MiB archive-size limit.
- The near-expanded session reported **zero undo entries** after creating the checkpoint and reference because history has a 128 MiB budget. The operation itself succeeded; an unconditional promise that every checkpoint operation is undoable would therefore be inaccurate near the resource ceiling. The small 32 MiB fixture retained two undo entries.

A subsequent release run after the undo-budget guard changed the near-limit project after its checkpoint and attempted restore: restore was rejected with `operation cannot fit in the undo/redo history budget`; content identity and generation stayed unchanged. That run recorded 635.5 ms write, 666.2 ms reopen, and 3,509.7 ms for the large-fixture phase. The previous measurements remain a separate single run, not an aggregate benchmark.

The labelled two-lane operations fixture now produces a layout proposal: existing external shape labels are routing obstacles, and multiple event/gateway edges use different cardinal attachment points. This does not remove the documented bounded-routing refusals for more complex diagrams.

Gateway renderer fixture tests verify default-flow marking by the source gateway's semantic flow ID (not the DI edge ID), and compare marker-present/absent SVG, PNG, PDF and viewport output. Event-based exclusive/parallel and instantiating/non-instantiating fixture vectors differ as expected. These are rendering checks, not an assertion that all gateway XML attributes or behaviors are supported.

## Native desktop review and schema 3 persistence

An actual debug Windows desktop session opened the synthetic schema-2 `target/ui-production/operations-ui.processcraft`, reported migration/Save As, and saved a separate schema-3 `target/m3-qualification/native-ui-review.processcraft`. UI Automation exercised posting an English comment on `review`, refusal to save an unposted comment draft, checkpoint creation, local-reference selection and frozen-content viewing. Renaming the activity changed the comparison to one renamed element and zero property/diagram changes. Explicit checkpoint restore, Undo and Redo were then invoked. Independent ZIP/JSON inspection confirmed the restored activity name, posted comment, reference ID and separate checkpoint payload. This evidence is specific to that synthetic project and build; it does not qualify all new BPMN property controls.

The native PDF flow displayed outline and pagination notices, required explicit acceptance, then wrote `target/m3-qualification/native-ui-review.pdf`. Independent pypdf parsing found one page and no extractable text, consistent with the disclosed outline policy. Shared-scene raster checks are recorded separately above.

Light, Dark and Dark at UI 200% were visually inspected. At 200%, vertical wheel scrolling brought Restore and the protected disabled Delete-reference action into the window. Inspection captures were saved initially in the temporary directory, with retained copies in `target/m3-qualification/ui/`. They show actual native controls, not browser mockups. Later collaboration/data and activity-wave controls require a separate native pass.

## Comparison and history accounting

`checkpoint_compare` verifies that semantic renames, property edits, DI changes and project/dictionary/resource/comment/link context are reported separately without changing the frozen snapshot. Semantic descendants are compared by ID instead of classifying a child rename as an ancestor property change. The desktop caches the structured comparison by generation/checkpoint and paginates each displayed group with 100 entries per section; structured CLI/MCP results retain their existing response-size ceiling.

History now charges the entire typed BPMN payload through a streaming JSON-size meter in addition to fixed allocated capacities, once per shared document. The four-times serialized-size allowance is intentionally conservative and can shorten available history. Targeted history tests verify large typed metadata is charged and shared document/resource bytes are not charged twice. Restore/layout preflight both previous and candidate states so a successful operation retains the promised Undo/Redo path; refusal changes neither content, generation nor existing history.

## Final partial-delivery checks and remaining work

The final workspace regression passed **326 tests across 30 test targets** with `cargo test --workspace --all-targets --all-features --locked --offline`; its complete output is retained in `target/m3-qualification/workspace-tests.log`. Formatting and workspace Clippy with `-D warnings` passed. The inventory checker reproduced 580 XSD declarations; that number is an inventory, not a coverage percentage. The I/O interchange suite contains 35 tests after the last activity and generated-lane-set changes.

Independent reviews identified comparison detail/pagination gaps and a possible lane-set export problem. The comparison now exposes field-level before/after values, typed navigation/dictionary link differences, moved shapes and rerouted edges, and paginates frozen model/dictionary/comment content. A genuine egui pointer test with 101 frozen comments reaches page 2. Generated lane-set IDs now share a deterministic collision-avoiding allocator across root, subprocess and child lane sets. The generated-ID collision fixture was reimported and independently XSD-validated. The reviewer could not establish whether the earlier nested-lane path dropped content or already refused it; do not record that original suspected loss as a reproduced failure.

A UI camera regression test verifies preview entry/exit preserves editor pan and zoom. Unit-test UI recovery now uses a private temporary folder rather than reading or writing the desktop user's recovery store.

Full coverage remains open: complete event/reference/namespace and executable-model metadata, complete choreography/conversation families, all property/DI edit paths, semantic rules and independent external consumer qualification. The [normative gap crosswalk](../research/bpmn-2-conformance-gap.md) and [expanded plan](../superpowers/plans/2026-11-11-processcraft-m3-implementation-plan.md) define the continuation. Source preservation and XSD acceptance do not close those gates.

## Release close-out

The normal release build of `processcraft.exe` and `processcraft-cli.exe` completed successfully. Byte sizes and SHA-256 hashes are retained in `target/m3-qualification/build-manifest.json`.

A final native run of this release opened the schema-3 review fixture, prepared the labelled two-lane auto-layout preview, applied it, invoked Undo then Redo, and saved a separate `native-layout-review.processcraft`. Independent ZIP/JSON inspection verified changed DI with unchanged process semantics, comments, checkpoint catalog and reference. The release CLI reopened and validated the result with no warnings. The Collaboration and data window was opened and captured; complete per-form native authoring qualification remains open. Release captures are in `target/m3-qualification/ui/processcraft-m3-release-*.png`. The application was closed after the synthetic test reached Saved state.

This was a historical user-requested partial close. Implementation has since resumed; full M3 completion has not been claimed.


## Cloud expanded-data checkpoint

At commit `4e1830f` (Linux x86_64, kernel 6.18.57), the expanded data slice includes bounded item-aware metadata, optional data IDs/names, process I/O, transformation/assignment expressions, selected data reference handling and shared commands/UI/rendering. It remains a profile: ancestor-scope references, complete identity/name presence, all owners and full semantic/interaction coverage remain open. A follow-on shared identity migration is in progress. Names omitted on data reference nodes still require protected-source handling until shared node-name presence is supported.

- Workspace all-target/all-feature tests with the locked offline dependencies completed with **343 passed, 15 failed, 0 ignored across 38 test targets**, using `--no-fail-fast`; `target/m3-data-workspace-tests.log` records the result. All 15 failures were in `processcraft-cli --test cli`, whose file-based flows encounter the deliberately unqualified non-Windows replacement adapter; the workspace gate therefore **did not pass**. This does not qualify Windows NTFS saves, ownership, ACLs, named pipes or native desktop behavior.
- Workspace all-target/all-feature Clippy with `-D warnings` passed (`target/m3-data-clippy.log`); release builds of desktop and CLI passed (`target/m3-data-release.log`). These are Linux build/check results, not Windows qualification.
- `target/m3-data-qualification.log` records codec identity/field round trips and independent pinned-XSD validation of both source and exported XML for three synthetic fixtures (six validations). The fixture manifests document their origin. This is bounded interoperability evidence, not external-consumer qualification. The interchange suite had 65 passing tests and the focused expanded-data native-capability test passed. A further golden baseline test passed after the workspace run; it is not included in that run's count.
- Current research and generator record per-property partial evidence and preserve unverified cells; `python scripts/generate_bpmn20_inventory.py --check` reproduces 580 declarations. The count is an inventory, not a conformance percentage.

Native Windows interaction, keyboard/accessibility, theme/DPI, qualified file safety and recovery gates, named-version independent consumer reopen checks, expanded-model performance and remaining BPMN requirements are still outstanding. No external-compatibility or comparative-superiority claim is established by this checkpoint.
