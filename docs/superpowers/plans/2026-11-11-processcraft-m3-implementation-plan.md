# ProcessCraft M3 implementation plan

**Goal:** Deliver local analysis/review tools and extend the workbench to complete BPMN 2.0.2 modeling/interchange coverage, with explicit evidence per normative requirement.

**Architecture:** Rust, native Craft egui, one core mutation/history engine, bounded ZIP/JSON packages and the shared glyph scene. GUI, CLI and attached MCP use the same commands and file grants. The user explicitly expanded M3 beyond the earlier flat BPMN profile. A workflow execution server is outside the analyst workbench scope; executable-model metadata remains part of modeling/interchange coverage.

**Sources:** OMG BPMN 2.0.2 normative PDF, XSD and CMOF listed at https://www.omg.org/spec/BPMN/2.0.2. Schema declarations alone do not establish behavioral or semantic conformance.

## Partial delivery checkpoint

**Status (November 11, 2026): temporary partial close requested by the user.** The original review/export tools and the first BPMN expansion waves are implemented. The complete BPMN modeling/interchange target remains approved and unfinished. Do not mark M3 complete or reopen feature work without the user's continuation.

- Delivered: explicit bounded auto-layout preview/cancel/apply, comments with orphan handling, schema-3 named checkpoints/reference/frozen views, paginated detailed comparison and guarded restore, shared-scene outlined PDF export, shared CLI/MCP commands and broader typed BPMN profiles.
- Qualified: workspace regression, formatting and Clippy, focused official-schema XML round trips, native review/export checks and resource/checkpoint limit experiments; see [evidence](../../implementation/m3-evidence.md) for exact build scope and limitations.
- Remaining: the unchecked full-coverage work packages below, complete UI/property/DI parity, normative semantic validation and independent external-tool interoperability. The [normative gap crosswalk](../../research/bpmn-2-conformance-gap.md) identifies why the current implementation does not satisfy full process/choreography modeling coverage.

## Delivery order and parallel ownership

1. **M3.0 Auto-layout:** execute the existing [detailed plan](2026-11-11-processcraft-m3-auto-layout.md). A pure bounded deterministic proposal, explicit preview, cancel, stale-safe apply, exact one-step undo/redo and no-op detection. Initial auto-layout coverage remains explicitly narrower than the final BPMN editor; unsupported structures refuse without change.
2. **M3.1 Comments:** plaintext comments with stable IDs and descriptive authors, explicit orphan status, shared create/update/delete commands, history/dirty/native round trips and bounded UI. No authenticated-author claim.
3. **M3.2 Checkpoints/reference:** immutable non-recursive frozen project snapshots including dictionary/resource context, named catalog, explicit local Reference marker, comparison and atomic restore. At most 20 checkpoints, bounded aggregate payloads, immutable resource sharing, protected reference deletion and schema migration with old-source identity verification.
4. **M3.3 PDF:** user-facing shared-scene vector PDF, bounded A4 pagination and explicit outlined-text fallback. Diagnostics must be acknowledged before writing; existing atomic output and uncertain-outcome handling remain in use. Independently render and inspect representative output; no searchable-text/accessibility claim for outlined text.
5. **M3.4 Complete BPMN modeling/interchange:** the work packages below are required before any 100% claim. They are not collapsed into the presence of element names or preserved XML.
6. **M3.5 Qualification:** full workspace regression, native GUI/CLI/MCP parity, resource/checkpoint performance, external interchange checks and per-capability acceptance artifacts.

Terra workers run independent auto-layout, core/native review features, and BPMN scope/interchange lanes. Root owns GUI/CLI/MCP integration, shared renderer/PDF, cross-lane compatibility and qualification. Shared model/command/format files have one writer; API contracts are exchanged before integration. Reuse the current checkout without initializing Git. Never launch Narrator.

## M3.4 coverage work packages

- [ ] Pin authoritative source files and hashes; inventory XSD declarations, reconcile CMOF classes and normative PDF constraints. Track model, edit, render, import, export and validation independently, with actual fixture/test pointers. Keep preserved-source behavior distinct from implemented semantics.
- [ ] Extend typed activity/event/gateway metadata without weakening validation or old native identity verification. Cover every task type; loops/multi-instance; subprocess, event subprocess, ad-hoc subprocess, transaction and call activity; all gateway and conditional/default-flow properties.
- [ ] Implement all start/intermediate/end/boundary event definitions, interrupting/non-interrupting behavior, catch/throw rules, event-definition references and root definitions. Preserve expressions, namespaces and QName resolution; reject or retain unsupported cases until qualified.
- [ ] Add nested flow scopes, lane hierarchies, participants, collaborations, message flows, conversations/sub-conversations, choreography tasks/subprocesses/call choreography and their associations/multiplicity.
- [ ] Cover data inputs/outputs/associations, stores, item definitions, expressions, resources/roles, interfaces/operations/messages, errors/signals/escalations, correlations, imports and standard definition metadata. Keep foreign extensions preserved and governed by capability checks.
- [ ] Extend the scene, palette, property editors, hierarchy navigation and transactional commands for each supported family. New enum variants with generic boxes do not count as rendering support. Every mutation keeps IDs/references, undo, dictionary/comment relationships and checkpoint context consistent.
- [ ] Qualify full namespace-aware XML import/export and DI semantics, complete per-family properties, nesting, references and failure paths. Unsupported or unqualified data remains safely retained; no selective field dropping to claim wider editing support.
- [ ] Add normative semantic diagnostics with PDF clause references and negative fixtures, plus external reopen checks in named tool/version combinations. Record limits and failures explicitly. Zero pending required rows, not a rounded headline percentage, is the full-coverage gate.

## Checks and acceptance

- [ ] Core feature tests: preview/cancel/stale/apply/undo/no-op; comment orphans and author/text limits; frozen checkpoint dictionary/resources, compare/restore/reference guards; typed BPMN references and invalid combinations.
- [ ] Native format tests: V0–V2 digest verification before migration, separate checkpoint payloads, resource deduplication, tamper and expanded-size rejection, Save As requirements, old recovery preservation and unchanged source on failure.
- [ ] CLI/MCP: same proposal/commands, expected session/generation, explicit file grants, bounded inputs, retry deduplication, no mutation or file write after refusal, and structured diagnostics for output fallback.
- [ ] Render/export: exact shared geometry/glyphs, all implemented markers, page clipping and bounds, independent PDF raster/outline-policy checks and no external resources/scripts.
- [ ] Native desktop: genuine controls and previews in English Light/Dark, keyboard navigation and 200% scrolling, pending-draft guards, cancellation and late-result rejection. No fake approvals or placeholder success controls.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` and `cargo test --workspace --all-targets --all-features --locked --offline`; build both release binaries and retain artifacts under `target/m3-qualification/`.

M3 remains in progress until every required delivery/coverage gate above is met. A parser accepting more tags, a schema catalog, outlined PDF support or a subset auto-layout result alone does not complete this milestone.
