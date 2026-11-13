# M5 evidence — Analyst foundations

Recorded 2026-11-13 on Windows 11 Pro (x64), Rust 1.98.1, `--locked --offline`.

## M5.1a — custom attributes: model, commands, native format, CLI and MCP

### What exists

- `crates/processcraft-core/src/attributes.rs` owns the whole attribute domain:
  `AttributeType` (`Text`, `Number`, `Duration`, `Currency`, `Enum`, `Date`,
  `DictionaryRef`, `Url`), `AttributeTarget` (`Activity`, `Event`, `Gateway`,
  `SequenceFlow`, `Lane`, `Participant`, `Process`, `DataObject`) with one shared
  `target_of(document, selector)` mapper, `AttributeDefinition`,
  `AttributeValue` (numbers, amounts and durations as canonical strings; never
  `f64`), `AttributeAssignment`, the update/delete resolution enums and the
  application functions called from `commands::mutate`. Starter definitions
  (Duration, Cost, SLA, Risk, Control, Process owner — decision 2) are pure data
  with fresh IDs; nothing creates them implicitly.
- Four new mutations with descriptors and generated JSON Schema:
  `createAttributeDefinition`, `updateAttributeDefinition`,
  `deleteAttributeDefinition`, `setAttributeValues`. `DeleteDictionaryEntry`
  also counts attribute usages for `Reject`, retargets them for `Replace` and
  turns them into `DictionaryTarget::Literal` for `Unlink`;
  `queries::dictionary_usages` includes those elements.
- The six project-level pruning sites in `commands.rs` (navigation reconciliation,
  choreography flow-element/artifact/data-association deletes, element delete,
  document delete) now call one `prune_removed_element_refs` helper that prunes
  navigation links, dictionary links and attribute values together;
  `element_identity_edit::set_xml_id` remaps attribute value sources like the
  other project-level keys.
- `ProjectData::validate()` (in `identity.rs`) calls `attributes::validate_project`:
  unique definition IDs and names, type-specific fields (unit only for
  Number/Currency, options only for Enum with at least one entry,
  `dictionary_kind` only for DictionaryRef), every value's definition exists,
  variant matches the type, enum options respected, canonical decimal/duration/
  date forms, dictionary entries exist with the right kind, the source element
  exists, no duplicate (source, definition). `applies_to` is deliberately not a
  structural invariant: it is enforced by `SetAttributeValues` (and the
  restricted update resolutions) only, so `replaceElement` cannot make a project
  unloadable. A test asserts a value survives a narrowing update followed by
  `validate()`.
- Same limits as planned, as constants: 200 definitions per project, 64 enum
  options, 16 dictionary references per value, all text within the 16 KiB
  `short()` bound, at most one value per (source, definition).
- Native format: `Manifest.schema` stays 3; the capability
  `analyst-attributes-v1` is written only when the project or any checkpoint has
  a definition or a value, is in the reader allow-list, and a payload carrying
  the data without the capability is refused ("analyst attributes require
  capability"). `FrozenProject` carries both vectors, `same_checkpoint`
  compares them, `history.rs` counts them in the retained-memory estimates.
- Queries: `queries::attribute_definitions` (definitions with usage counts),
  `queries::element_attributes` (per element: target, name, values),
  `ActivityRow.attributes` (additive cells), `compare_checkpoint` gained
  `attributes_changed` and one `PropertyDifference` per changed value with
  `property = "attribute:<definition name>"` merged into the element's change.
- CLI `query attributes <document_id>` (definitions + per-element rows); MCP
  `Query::Attributes { document_id, offset, limit }` in `mcp.rs` and in the
  GUI's `attachment.rs`; `query table` rows carry the values as additive cells.

### What is verified

- Quality gates on the final source: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features --locked --offline
  -- -D warnings`, `cargo test --workspace --all-targets --all-features
  --locked --offline` (117 test binaries, 0 failures), and
  `python scripts/check_text_hygiene.py` — all pass.
- New `crates/processcraft-core/tests/attributes.rs` (18 tests): every
  validator (accept/reject/canonical form, ISO 8601 duration subset without
  years or months, real calendar dates), target mapping, set/clear through
  mutations with canonical stored forms, `applies_to` enforcement at
  `SetAttributeValues` and non-invariance in `validate()`, every update and
  delete resolution (`Reject`/`Clear`/`MapOptions`, `Reject`/`Discard`/
  `MoveTo` with same-type, kind and enum-option checks), failed batch leaves
  state/history/generation unchanged, undo/redo, pruning on element and
  document delete plus remap on `setElementXmlId`, dictionary entry delete with
  each resolution, checkpoint capture/restore/compare, limits, definition shape
  violations, starter definitions, serde wire shape.
- `descriptors_match_protocol_tags` covers the four new tags;
  `analyst_workbench::request_schema_covers_all_discovered_tags` covers the
  generated schema automatically.
- New `crates/processcraft-io/tests/native.rs`
  `analyst_attributes_round_trip_and_capability_gate`: the pinned
  `identity-baseline-2f0f770.processcraft` loads with its unchanged content
  identity and re-saves without the new capability; a project with definitions
  and values round-trips through the ZIP format with the same content identity;
  a checkpoint-only usage also writes the capability; stripping the capability
  from an archive that carries the data is refused.
- New `apps/processcraft-cli/tests/cli.rs` tests: `run --batch
  tests/fixtures/commands/attributes.json` (a new fixture, registered in
  `tests/fixtures/manifest.json` with provenance, licence and SHA-256; no
  existing fixture was touched) then `query attributes` and `query table`
  return the eight definitions and the values; an MCP stdio session applies the
  same batch, answers the same values through `query`, saves, and a CLI query
  of the saved file returns byte-equal values and definitions.

### Exit criterion

A project with all eight attribute types (`tests/fixtures/commands/
attributes.json`) applies as one batch, saves, reopens with the same content
identity, and answers the same values through CLI `query attributes` and the
MCP stdio `query` (asserted equal in the test). Deleting a used definition
without a resolution fails with `InvalidParams` ("attribute definition is in
use"), as asserted in the core tests.

### Limits and what is not established

- Clipboard copy/paste and `duplicate` do not copy attribute values, matching
  dictionary links (decision 6); recorded here as a known limit.
- Re-setting a value moves it to the end of `attribute_values`, so the content
  identity depends on set order; batches are expected to be deterministic.
- Values live at project level keyed by `ElementKey`; no per-node storage and
  no per-attribute permissions. Attributes on dictionary entries or documents
  stay out of scope (plan boundaries).
- Nothing in this phase touches BPMN interchange (M5.1b) or the desktop
  editing surfaces (M5.1c): the GUI only compiles and passes its tests with the
  new `ProjectData` fields; the `pc:attributes` namespace is not written or
  read yet, and no BPMN conformance claim follows from this phase.
- No performance measurements were taken in this phase; the M5.6 qualification
  step measures the 10,000-value save and the table view with ten columns.
- MCP file effects, grants and reconciliation behave as in M4; no new
  capability was added to the attachment protocol beyond the query.
