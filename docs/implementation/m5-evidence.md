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
  structural invariant: it is enforced by `SetAttributeValues`, the restricted
  update resolutions and `MoveTo`, so `replaceElement` cannot make a project
  unloadable. A test loads a project whose value sits outside its definition's
  `applies_to`, validates it, renames the definition and clears the value;
  clearing never needs a target, and `queries::element_attributes` lists rows
  whose element lost its target with `target: null`.
- Same limits as planned, as constants: 200 definitions per project, 64 enum
  options, 16 dictionary references per value (no duplicates), all text
  (names, descriptions, units, options, values, literals) within the 16 KiB
  `short()` bound, at most one value per (source, definition). Definition
  names carry no leading or trailing whitespace, `applies_to` is not empty, and
  a `Currency` definition has an ISO 4217-shaped unit (three uppercase letters).
- Durations are normalized by elapsed time: `PT90M` is stored as `PT1H30M`
  and `PT24H` as `P1D`, so equal durations have one canonical string.
- `SetAttributeValues` replaces a value in place: re-setting an unchanged value
  keeps the generation and content identity (no undo step, no dirty state),
  and `compare_checkpoint` compares the two vectors as sets.
- `MoveTo` refuses a target that does not apply to a valued element, has a
  different unit (Number/Currency), or already holds a value for one of the
  moved elements; it never drops a value silently.
- Native format: `Manifest.schema` stays 3; the capability
  `analyst-attributes-v1` is written only when the project or any checkpoint has
  a definition or a value, is in the reader allow-list, and a payload carrying
  the data without the capability is refused ("analyst attributes require
  capability"). `FrozenProject` carries both vectors, `same_checkpoint`
  compares them, `history.rs` counts them in the retained-memory estimates.
- Queries: `queries::attribute_definitions` (definitions with usage counts),
  `queries::element_attributes` (per element: target, name, values),
  `ActivityRow.attributes` (additive cells, sorted by definition name like
  every other surface), `compare_checkpoint` gained
  `attributes_changed` and one `PropertyDifference` per changed value with
  `property = "attribute:<definition name>"` merged into the element's change.
- CLI `query attributes <document_id>` (definitions + per-element rows); MCP
  `Query::Attributes { document_id, offset, limit }` in `mcp.rs` and in the
  GUI's `attachment.rs`, both enforcing `limit` 1..100 and counting the
  definitions in the 3 MiB response budget; `query table` rows carry the values
  as additive cells.
- BPMN export treats attribute values as native-only until M5.1b writes
  `pc:attributes`: a document with values needs `acknowledge_native_fields`
  and the report lists the omission.

### What is verified

- Quality gates on the final source: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features --locked --offline
  -- -D warnings`, `cargo test --workspace --all-targets --all-features
  --locked --offline` (117 test binaries, 0 failures), and
  `python scripts/check_text_hygiene.py` — all pass.
- New `crates/processcraft-core/tests/attributes.rs` (24 tests): every
  validator (accept/reject/canonical form, ISO 8601 duration subset without
  years or months, real calendar dates), target mapping, set/clear through
  mutations with canonical stored forms, `applies_to` enforcement at
  `SetAttributeValues` and non-invariance in `validate()`, every update and
  delete resolution (`Reject`/`Clear`/`MapOptions`, `Reject`/`Discard`/
  `MoveTo` with same-type, kind and enum-option checks), failed batch leaves
  state/history/generation unchanged (`Undo` then reports `NothingToUndo`),
  undo/redo, pruning on element and document delete plus remap on
  `setElementXmlId`, a delete of every element class (flow nodes, flows, lanes,
  data references through `Delete`; data objects through `DeleteDataObject`
  and `SetScopeDataObjects`), sequence flows inside expanded subprocesses as
  targets, in-place re-set with unchanged identity, unique dictionary
  references after a retarget, `MoveTo` refusals, definition text bounds, dictionary entry delete with
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
  a checkpoint-only usage also writes the capability (the archive loads);
  stripping the capability is refused both for checkpoint-only data and for the
  current project's data; `export_bpmn` refuses a document with values without
  the acknowledgement and reports the omission with it.
- New `apps/processcraft-cli/tests/cli.rs` tests: `run --batch
  tests/fixtures/commands/attributes.json` (a new fixture, registered in
  `tests/fixtures/manifest.json` with provenance, licence and SHA-256; no
  existing fixture was touched) then `query attributes` and `query table`
  return the eight definitions and the values; an MCP stdio session applies the
  same batch, answers the same values through `query`, saves, and a CLI query
  of the saved file serializes every row and every definition (with usage
  counts) to the same bytes as the MCP page.

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
- The content identity still depends on the order in which values are first
  set (two batches setting the same values in a different order differ);
  re-setting an existing value no longer moves it. The attached-mode
  `Query::Attributes` path is checked by code review only, not by a test.
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
