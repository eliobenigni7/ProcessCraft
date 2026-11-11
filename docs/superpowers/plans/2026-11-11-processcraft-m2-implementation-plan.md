# ProcessCraft M2 Implementation Plan

> **For agentic workers:** Use subagent-driven development with Terra implementers and independent spec then quality reviews. Steps use checkbox tracking. Never launch Windows Narrator; the user explicitly prohibited it.

**Goal:** Complete M2.1â€“M2.6 from the accepted roadmap: library, dictionary, table, qualified BPMN interchange, SVG/PNG export, and explicitly authorized local automation.

**Architecture:** Extend the existing core/session, bounded native format and shared renderer. The desktop owns the mutable visible session. I/O handles explicit grants and OS effects; headless and attached automation share typed commands. Unsupported XML remains preserved source. No SQLite or additional rendering runtime.

**Tech Stack:** Existing Rust workspace and pinned egui/quick-xml/ZIP/text stack; official rmcp SDK for stdio only, with transport runtime confined to CLI. Use standard Windows named pipes with a narrow reviewed adapter for attachment.

**Authority:** [Accepted architecture](../specs/2026-11-11-processcraft-solution-design.md), sections 5â€“17, and [M2 roadmap](2026-11-11-processcraft-implementation-plan.md#m2m4-follow-on-work). The user explicitly authorized M2 implementation. This refines execution, not the accepted product direction. The checkout has no Git metadata; reuse it without initializing Git or creating a worktree.

**Status:** Tasks 17–23 implemented and independently reviewed. See [M2 evidence](../../implementation/m2-evidence.md) for 217 passing tests, actual native attachment checks, the qualified BPMN subset and rename latency disposition. No Narrator session was opened.

## Chunk 1: Authoritative model and interchange

### Task 17: Library, dictionary and table core (M2.1â€“M2.3)

**Own:** `crates/processcraft-core/src/{ids,model,identity,commands,history,lib}.rs`, new `queries.rs` and focused core tests; `crates/processcraft-io/src/native.rs` and its format tests for schema evolution. Keep Node/Lane layouts stable where possible by storing dictionary relationships at project level.

- [x] Add UUID `DictionaryEntryId`, entries with Role/System/Document kinds, name/description, and typed relationships from `ElementKey`. Linked targets resolve names from entries; explicit unlink retains a literal name. Keep literals distinct from live references.
- [x] Add folder create/rename/move/delete mutations; reject containment cycles and deletion of nonempty folders. Existing document-property commands move/rename by stable ID. Pure library/search projections return deterministic IDs and scan memory.
- [x] Add entry CRUD, usage queries, link/unlink and explicit deletion resolution (reject, same-kind replacement, or literal unlink). Validate relation/source kinds and duplicate links; delete node/document cleans its links. Every persistent change participates in validation, identity, history estimates, undo and dirty state.
- [x] Query role-to-lane choices without guessing by label. Zero mappings offers lane creation, one mapping permits assignment, multiple mappings require explicit lane ID. Lane membership and optional DI bounds use the existing transactional assignment.
- [x] Project activity rows by stable ElementKey, with lane/role, systems/documents and outgoing flow IDs, targets and conditions. Sort only the projection; never infer or rewrite topology from row order. Preserve unsupported document capabilities.
- [x] Support adding an activity at an explicit selected graph position. A qualified simple-chain splice updates nodes/flows/DI transactionally in one undo step; refuse ambiguous branches or require a separately qualified explicit canvas operation. Add branch-preservation and undo regressions. Never treat row reordering as insertion or implicit branch conversion.
- [x] Evolve native schema to 2 and required reader capability. Verify schema-1 canonical identity against its original field set before adding empty M2 fields; migrate in memory and require Save As. Preserve schema-0 exercise and refusal of unknown fields/capabilities. New writer output must reopen through the same bounded reader.
- [x] Generate JSON schemas from actual shared request/model types for command discovery and automation; retain descriptors and test tag/schema parity. No independent transport mutation DTOs.
- [x] Run meaningful rollback, usage rename/delete resolution, ambiguous-role, graph-preserving table, identity/history, schema migration/tamper and package round-trip checks. Run affected core/I/O tests and Clippy; independently review spec then correctness.

### Task 18: Qualified BPMN profile and compatibility reports (M2.4)

**Own:** new I/O `interchange.rs`, `tests/interchange.rs`, synthetic `tests/fixtures/interchange/` and capability manifest. Keep the M0 parser/source-preservation contract. Coordinate module exports with the root; do not modify native schema while Task 17 owns it.

- [x] Import into a temporary prepared document/resources bundle. First enforce existing byte/depth/reference limits, then expanded-name, attribute and parent-context allowlists. Promote to Editable only when every semantic/DI value is represented and validated; unsupported standard attributes, foreign elements/attributes, ambiguous QNames or unqualified structures downgrade the entire source to preservation.
- [x] Qualify the flat initial profile actually implemented: tasks/user/manual, none start/end, XOR/parallel, sequence flows and stored conditions, lanes/pools and other initial-profile kinds only as backed by fixtures. Preserve IDs, labels/docs, bounds, waypoints and multiple diagrams. Never autolayout or silently drop import DI. Retained-extension structural editing stays disabled until separately qualified.
- [x] Retain original bytes as immutable provenance; preserved documents export those exact bytes. Editable export emits canonical namespace-correct XML and explicit retained/transformed/omitted/blocked reports. Preserve meaningful namespace context via original resource or represented data. Native-only links/metadata require explicit omission acknowledgement; unrepresentable semantics must block, not become a success warning.
- [x] Test import/edit/export/reimport semantic and every-diagram DI equality; unknown standard/foreign content classification and unchanged source bytes; malformed/oversized input; conditions and XML escaping. No external URL/schema/entity loads. Update fixture hashes/license/provenance and a precise support matrix.

### Task 19: User SVG/PNG exports (M2.5)

**Own:** renderer `export.rs`, scene exports/tests and manifest only as needed. This can be implemented with Task 18 in the same independent rendering/I/O boundary owner.

- [x] Generate full-document SVG/PNG bytes from the existing shared scene and embedded glyph paths, with explicit diagram, appearance and size/scale options. Report actual origin, logical bounds, pixel dimensions and diagnostics. Reject source-only placeholder export and bounded-size failures before writing.
- [x] Use full scene, not viewport omissions; explicit colors and glyph layout match the canvas. No selection overlay, external resource, script or new SVG engine. Fail on unacknowledged omissions; report font substitutions.
- [x] Keep I/O and render independent. Caller obtains explicit output grant, generates bytes, then commits through WriterSession with explicit NotCommitted/Committed/Indeterminate outcome. Never truncate an existing output on generation failure; export never changes native save binding or dirty identity.
- [x] Verify exact PNG dimensions, SVG bounds/escaped labels, shared scene parity, negative coordinates, multiple diagrams, oversize refusal and original output preservation. Independently inspect an exported image.

## Chunk 2: Desktop and automation

### Task 20: Four synchronized desktop views (M2.1â€“M2.5)

**Own:** desktop app/controller and focused `panels.rs` or equivalent modules, plus composition-only manifest changes. Use existing approved FigCraft boards/tokens, English Light/Dark, 100%/200% layout and named accessible controls.

- [x] Add real Editor/Library/Dictionary/Table navigation. Library filters and folders retain DocumentId selection; rename/move and folder operations dispatch core mutations. Add diagram/process selection rather than assuming index zero.
- [x] Dictionary entry editors, typed links and reverse usages share core projections. Deletion requires explicit replacement/unlink action. Table edits names/links/roles through the same dispatcher; resolve ambiguous lanes explicitly and offer creation when unmapped. Table/outline/canvas/inspector selection use the same ElementKey, and problems target the correct document/generation.
- [x] Table Add activity selects an explicit existing graph position and invokes the qualified transactional splice; explain refusal of ambiguous branches without changing them. Presentation sorting remains view state.
- [x] Wire explicit native BPMN Import/Export and SVG/PNG Export dialogs, prepared import generation checks, provenance/capability messaging and compatibility/omission reports. Do not enable destructive controls for preserved source. Output jobs carry originating context and cannot overwrite the project save binding.
- [x] Preserve dirty drafts across view changes and canceled prompts. Native effects remain bounded; generation changes do not discard committed acknowledgements. UI appearance, filters and presentation order never become model mutations.
- [x] Verify native controls and deterministic egui regressions, both themes/scales and constrained widths without opening Narrator or interacting with unrelated user windows. Use native PrintWindow captures if the shared desktop is busy.

### Task 21: CLI queries, import/export and opt-in stdio MCP (M2.6)

**Own:** CLI args/routing plus focused automation modules/tests; I/O root-grant helper if required. Coordinate renderer dev-to-normal dependency change for CLI exports; CLI still has no desktop dependency.

- [x] Expose library/dictionary/usages/table queries and explicit import/BPMN/SVG/PNG exports in CLI, using the APIs above and shared strict request schemas. Preserve exit categories 0/1/2/3/4 and structured effect outcomes.
- [x] Pin a compatible official rmcp SDK and isolate its async transport runtime in CLI. `mcp` starts only explicitly, speaks standard newline-delimited JSON-RPC on stdin/stdout and emits diagnostics on stderr. Bound each incoming line before SDK parsing; reject messages above 4 MiB.
- [x] Expose inspect/query/validate/apply/save/export tools. Mutations and file effects require originating session and expected generation, plus request ID. Root grants come only from explicit launch arguments or GUI authorization, never model paths or arbitrary client escalation. Check component containment/reparse safety through the existing OS boundary.
- [x] Deduplicate request IDs by payload/context within a bounded session cache. Return prior completed result; reject mismatched reuse; do not replay pending, evicted or previous-session mutations automatically. Save/export results preserve committed/indeterminate identity and reconciliation requirements.
- [x] Test actual subprocess initialize/tools/list/read-only/mutate/save/export, schema parity, framing bounds, root escapes, generation conflict, dropped-response retry and stdout protocol purity. No preferences/recovery/listener created by normal headless commands.

### Task 22: Explicit visible-session named-pipe attachment (M2.6)

**Own:** narrow I/O local IPC adapter/tests, shared automation wire DTOs where needed and new desktop `attachment.rs`; Task 20 owner wires app/controller polling. Core remains OS/transport independent.

- [x] Default off. GUI Enable/Disable explicitly scopes the current session and root grants. Use unpredictable pipe name, protected current-user SID DACL, first-instance and remote-client rejection. Bound framing, concurrent requests and queues before allocation/dispatch; no TCP listener or remote provider.
- [x] Pipe worker marshals requests to GUI-owned session queue. Inspect/query and mutation use current visible state and the same schemas/generation checks. Async saves reply with known file outcomes after controller reconciliation; never mutate a worker-owned copy and call it attached editing.
- [x] Authentication/session capabilities are separate from locality. Revoke on disable, session replacement and close; reconnect never blindly replays. Read/write roots are explicitly authorized, not inferred from recovery storage or client JSON. Retain same-user filesystem race limitations honestly.
- [x] Real Windows pipe tests cover authentication, owner DACL/remote flag, framing/queue limits, revocation, retries, request-ID conflict, generations and file grants. A live native smoke must show an authorized remote rename in the visible session and undo through the GUI; unauthorized request leaves identity unchanged.

## Chunk 3: Integration and evidence

### Task 23: Qualify M2 and close the plan

**Own:** `docs/implementation/m2-evidence.md`, README, roadmap/spec status and qualification artifacts/example changes needed by observed failures.

- [x] Record component spec/quality reviews and resolve blockers. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`, `cargo test --workspace --all-targets --all-features --locked --offline`, and release desktop/CLI builds.
- [x] Repeat procurement across library/dictionary/table/editor, graph-preserving rename and role changes, schema-1 Save As migration, BPMN supported/unsupported round trips, exact image export and output failure, real stdio MCP and visible-session attachment. Recheck no-loss, ownership, retry and recovery tests on the final source.
- [x] Measure medium-fixture library/search/table and current rename/save behavior before considering caches or storage changes. Investigate M1 hashing/compression misses; preserve integrity, and report any remaining performance deviation with an explicit disposition rather than guessing a pass.
- [x] Record artifacts, exact tests, qualified editable subset, assistive-technology limitation (Narrator prohibited), and actual filesystem/platform scope. Update README to distinguish implemented M2 from M3/M4. Do not claim broader Signavio/BPMN compatibility or PDF/checkpoint features.

Independent work can run concurrently only with exclusive file ownership: core/native schema; BPMN/render; automation/IPC. Desktop integration follows their published APIs. Root owns cross-component integration, module/manifest conflicts and final evidence. Do not run redundant broad builds while another worker owns the build directory.
