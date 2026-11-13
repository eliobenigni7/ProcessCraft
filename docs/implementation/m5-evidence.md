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
- BPMN export treated attribute values as native-only in this phase; M5.1b
  (below) replaced that with `pc:attributes`.

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

## M5.1b — custom attributes in BPMN interchange

### What exists

- `PC_ATTRIBUTES_NS = "urn:processcraft:attributes:1"` in `interchange.rs`; the
  reader, writer and commit planner live in the submodule
  `crates/processcraft-io/src/interchange/attributes.rs`.
- Export: `export_bpmn` passes the document's values to `export_document`, which
  appends one `pc:attributes` as the last child of each owner's
  `extensionElements` (through `visit_base_metadata_mut` /
  `visit_legacy_metadata_mut`, creating the extension when absent). Every
  `pc:attribute` carries the definition ID, name, type, unit and, for
  references, `dictionaryKind`; a `pc:ref` carries the entry ID and name, or
  the name alone for a literal. Values are no longer part of the native-only
  gate (navigation and dictionary links still are). The report lists "Custom
  attribute values written to the ProcessCraft extension namespace" under
  `transformed` and, when the project has definitions, one `omitted` line for
  definition metadata the values do not carry (description, `required`,
  `applies_to`, unused enum options) and for definitions without values; that
  line is informational and does not require the acknowledgement. The export
  re-parses its own output, lifts the `pc:attributes` back out and blocks the
  export unless the values read equal the values written and the rest of the
  typed model is unchanged. A value whose owner has no metadata writer blocks
  the export ("have no qualified metadata writer"); the same
  guard exists for a preserved-source document with values, but it is defensive
  and untested: `ProjectData::validate()` rejects values on a document without
  the element first, so `export_bpmn` cannot reach it.
- Import: `prepare_import` lifts well-formed `pc:attributes` of an editable
  document into `PreparedImport.attributes` (definitions and values) and removes
  them from the stored extension content; an extension left with only
  whitespace is dropped. An element that fails validation (unknown type or
  attribute, bad UUID, any `check_value` failure such as an invalid decimal or
  date, unexpected child, a definition set twice on one element, a definition
  redeclared with another name, type, unit or kind, more than 200 attributes, an
  owner without XML ID or attribute target) stays in the extension content and
  is named in `ImportReport.limitations`. Numbers, durations and currency
  amounts are normalized by the core validators (`12.50` becomes `12.5`,
  `PT90M` becomes `PT1H30M`) rather than rejected. The file carries no
  `applies_to`, `required`, description or option list, so an imported
  definition gets `applies_to` = the targets of the elements that carry its
  values and enum options = the options seen in values. Preserved-source
  documents are not read for `pc:attributes`.
- Commit: `plan_attribute_import(project, document, attributes)` returns the
  mutations (definitions first, then one `SetAttributeValues` per element) and
  the notes shown with the import status. The desktop import handler appends
  them to the `CreateDocument` batch; `import_bpmn_cli` applies them through the
  session. Rules: a definition ID the project lacks is created as read; an ID
  that exists with another type, unit or dictionary kind is created again under
  a fresh ID and "<name> (imported)"; a new name that collides with an existing
  name gets the same suffix ("(imported N)" if that collides too); values for an
  existing definition that does not apply to the element target, or do not fit
  it (for example an enum option it lacks), or whose definition could not be
  created (limit or invalid name), are not imported: `plan_attribute_import`
  takes the document mutably and puts the original `pc:attribute` back into the
  owner extension content (in a new trailing `pc:attributes`), so re-export
  writes it unchanged, and each one is named in the notes (definition, owner,
  reason). An existing definition is never widened; a `pc:ref` whose entry is
  not in the project (or has another kind) becomes
  `DictionaryTarget::Literal(name)`, never matched by spelling.
- One core change: `SetAttributeValues` now checks its document with
  `doc(.., false)` instead of `true`. Values live in project-level vectors and
  leave the document untouched, and with `true` a document carrying foreign
  extension XML (the fixture) refused the values with "opaque XML prevents safe
  structural edits". `AttributeDefinition::validate_shape` became `pub` so the
  importer applies the same shape rules.
- Fixtures `tests/fixtures/bpmn/attributes/foreign-and-processcraft.bpmn` and
  `foreign-and-processcraft-malformed.bpmn` (synthetic, CC0) with
  `foreign-and-processcraft.manifest.md` (provenance, licence, SHA-256, import
  mode).

### What is verified

- New `crates/processcraft-io/tests/attributes_interchange.rs` (11 tests):
  `native_to_bpmn_to_native_keeps_every_value_and_all_eight_types` (all eight
  types on one task plus a text value on a process, participant, lane, event,
  gateway, sequence flow, data object, data object reference, subprocess and a
  task nested in a subprocess; re-import into the same project binds
  definitions and the dictionary entry by ID with no duplicate definition, into
  an empty project creates the definitions under their file IDs and turns the
  entry into a literal; a second export writes all 18 values again);
  `pc_attributes_are_the_last_child_after_foreign_extensions_and_survive_in_order`
  (the fixture; foreign elements equal in the model before and after, written
  before `pc:attributes`);
  `malformed_pc_attributes_stay_extension_content_and_are_named`;
  `invalid_attribute_elements_are_never_lifted` (nine malformed variants, each
  kept and named); `import_bounds_definitions_and_whitespace` (201 attributes);
  `colliding_ids_and_names_and_inapplicable_targets_never_fail_the_import`;
  `dictionary_entry_of_another_kind_becomes_a_literal`;
  `a_value_on_a_missing_owner_blocks_the_export_instead_of_being_dropped`;
  `export_report_says_what_is_written_and_what_is_not`;
  `values_the_project_cannot_take_stay_in_the_document_and_are_written_back`
  (same-ID definition whose `applies_to` excludes the element: value stays as
  extension content, note names it, re-export contains it);
  `set_attribute_values_is_refused_on_preserved_source_documents`.
- `native.rs::analyst_attributes_round_trip_and_capability_gate` now asserts the
  export carries the namespace and the definition-metadata omission line; its
  baseline project still has links, so it passes the acknowledgement for those.
- `apps/processcraft-cli/tests/cli.rs::import_bpmn_lifts_attributes_and_export_writes_them_back`:
  `import-bpmn` of the fixture, `query attributes` returns four definitions and
  four canonical values with the unknown entry as literals, the saved file keeps
  its content identity, `export-bpmn` writes four `pc:attribute` after the
  foreign elements.
- `python scripts/check_bpmn20_xsd.py` (lxml 6.0.3) passed on two exports made
  with the CLI: the fixture round trip and the eight-type
  `tests/fixtures/commands/attributes.json` project. This checks XML schema
  validity only.
- Gates on the final source: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets --all-features --locked --offline -- -D warnings`,
  `cargo test --workspace --all-targets --all-features --locked --offline`
  (118 test binaries, 0 failures, rerun after the review fixes) and `python scripts/check_text_hygiene.py`.

### Limits and what is not established

- A kept (unplaced) `pc:attribute` that conflicts with a definition the project
  writes for the same ID (for example another name) makes the export self-check
  fail, so that export is blocked with "Attribute values do not survive the BPMN
  round trip"; the export-side check accepts kept values only when they are
  consistent with the written ones.
- `SetAttributeValues` is allowed on documents with foreign XML or retained
  extensions, while `SetDictionaryLinks` is not: deliberate, values are
  project-level and leave the document untouched. It is still refused on a
  preserved-source document (tested).
- Exported XML repeats in-scope namespace declarations on foreign elements (the
  existing canonical serializer); foreign nodes are equal in the typed model,
  not byte-identical text.
- A definition description, `required`, `applies_to` and unused enum options do
  not travel; enum options are rebuilt from the values.
- The commit rules are tested through the shared planner and the CLI; the
  desktop import handler only calls the planner and was not exercised by a GUI
  test.
- Owner kinds with no metadata writer cannot be reached from a valid project in
  the tests: every attribute target tested (listed above) has a writer. The
  block is exercised only for a value whose owner is missing.
- Import reads only the extension content of owners the importer already
  qualifies; `pc:attributes` elsewhere (preserved-source documents, unsupported
  owners) stays untouched source or extension content.
- No performance measurement; the bounds tested are the 200-attribute limit and
  the existing XML limits. No BPMN conformance claim follows from this phase.

## M5.1c — desktop editing of custom attributes

### What exists

- New `apps/processcraft/src/app/attributes.rs`; the shared table widget
  `data_table` in `analyst_views.rs` is now a wrapper over
  `data_table_editable`, which takes a per-cell callback (`true` = the callback
  drew the cell, `false` = default text label).
- Dictionary view: an "Entries / Attributes" tab. The Attributes tab lists
  definitions (name, type, applies to, value count), creates and edits them,
  and has "Add starter attributes" (`CreateAttributeDefinition` for the starter
  names the project lacks, one batch). The form shows `validate_shape` messages
  from core; the type is read-only on an existing definition. A save that core
  refuses as is but accepts with `Clear` (removed or renamed option in use,
  narrowed `applies_to` in use) is not submitted: it opens an inline prompt
  with "Clear the affected values", a per-option mapping (`MapOptions`) and
  Cancel. "Delete attribute…" always opens a prompt (Discard, "Move the
  values to…" for same-type definitions, or Delete when nothing uses it, plus
  Cancel).
- Inspector: an "Attributes" section for the selected element (and for the
  process when nothing is selected) with one typed editor per applicable
  definition (text, number, currency, date, URL and duration as text fields;
  option list as a combo; dictionary entries as checkboxes). Duration accepts
  `2h 30m` (units d, h, m, s) or ISO 8601 and shows the stored value in the
  friendly form. Text-like editors keep a draft and commit when the field
  loses focus; choices commit when changed; each commit is one
  `SetAttributeValues`. An invalid draft shows the core validator message,
  stays, and is not submitted; selecting another element or navigating is
  refused while it exists ("Discard edit" removes it). Values whose definition
  no longer applies are listed under "No longer applicable to this element"
  with a Clear button.
- Table: "Attribute columns…" menu (activity-applicable definitions; view state
  only), editable cells through the same typed editor, Ctrl/Shift+click toggles
  rows into a multi-row selection, and editing an attribute cell of one marked
  row submits one batch with one `SetAttributeValues` per marked row.
- Controls carry accessible names (`labelled_by` for labelled fields,
  `accessible_name` for table cells and combos); they are standard egui
  widgets, so they take keyboard focus like the dictionary controls.

### Tests

`cargo test -p processcraft` (167 passed). Added:

- `app::attributes::tests::invalid_draft_does_not_submit_and_is_kept`
- `app::attributes::tests::multi_row_edit_is_one_undo_step`
- `app::attributes::tests::drafts_survive_view_changes` (cell draft and
  definition draft across five view/tab switches rendered through
  `analyst_views_panel`; generation unchanged)
- `app::attributes::tests::durations_use_friendly_text_and_core_messages`
- `app::attributes::tests::starter_attributes_are_added_once`
- `app::attributes::tests::stale_value_is_shown_and_clearable` (renders the
  inspector section for a stale value without panicking, then clears it)
- `app::attributes::tests::editable_cell_callback_replaces_the_default_label`
- `app::attributes::tests::marked_rows_are_limited_to_visible_rows_each_frame`
- `app::attributes::tests::new_or_opened_project_discards_attribute_state`
- `app::attributes::tests::unusable_drafts_are_pruned_and_errors_name_the_element`
- `app::attributes::tests::move_targets_mirror_core_checks`
- `app::analyst_views::tests::restricted_definition_update_opens_the_prompt_instead_of_submitting`
- `app::analyst_views::tests::deleting_a_used_definition_opens_the_prompt_instead_of_submitting`
- `app::analyst_views::tests::attribute_tab_and_table_columns_render`

### Review fixes

- Attribute state is reset by `discard_property_drafts` (new/open/recovery) and
  counted by `has_drafts()`.
- Cell drafts whose element, definition or applicability is gone are dropped on
  commit; hiding a column drops its drafts; an invalid draft's message names
  the element, and the error area has "Discard attribute edits".
- Marked rows are `(document, element)` keys intersected with the visible rows
  every frame; Ctrl+click seeds only from a visible activity row. Setting the
  value a marked row already has still applies to all marked rows. Drafts
  committed by navigation/select/undo write their own element only, so marks
  are cleared first (no fan-out).
- The update prompt closes when the form is edited and keeps its option
  mappings after a refused resolution; the move-to list mirrors core's checks
  (type, unit, dictionary kind, option coverage, applies_to, free target);
  Save with no changes does not submit; the stale-value Clear button is named
  "Clear <definition>"; the Attributes section is hidden for elements without
  an attribute target and values.
- Not covered by tests: the Discard attribute edits button and the
  prompt-closes-on-edit behaviour (code only), and the force path of the
  table editors.

### Limits and what is not established

- No screenshot and no manual GUI session: pointer clicks, Ctrl+click row
  selection, Enter/blur commits and the visual layout were not exercised; the
  tests call the same methods the controls call and render frames without
  asserting pixels. Keyboard reachability and accessible names rest on the
  widget choice, not on a screen-reader or Tab-order test.
- The restricted-update check is a dry run of core's `apply_update` on a clone
  of the project (`Reject` refused and `Clear` accepted); core was not changed.
- Marked table rows are view state and are not cleared when the model changes
  underneath them; an edit that core refuses for any marked row fails the whole
  batch with core's message.
- The Attributes tab shows definitions in one stacked list/editor layout (no
  side-by-side panel as in Entries).
- Clipboard and duplicate still do not copy attribute values (decision 6).

## M5.3 RACI matrix

Scope: fixed RACI attribute definitions, `queries::raci_matrix` / `raci_csv`,
CLI `query raci` and `export-raci`, MCP and attached `query` kind `raci`, and a
RACI segment in the activity table. Statements below are what the named tests
assert; nothing else is claimed.

### What the tests establish

- `crates/processcraft-core/tests/raci.rs`:
  - `responsible_follows_lanes_and_attributes_fill_a_c_i`: on the pinned
    two-lane procurement fixture (`tests/fixtures/commands/raci.json`) the
    columns are the three dictionary roles by name then the literal "Legal",
    rows are the two activities only, and the cells are exactly R/A/C/I as
    authored (one role holds `R,A`).
  - `responsible_reads_the_link_target_not_the_label`: two role entries with
    the same name stay two columns; R lands on the lane's linked entry, A on the
    same-named twin.
  - `role_rename_is_reflected_in_the_column_name_only` and
    `rename_keeps_one_column_and_same_name_entries_stay_two`: a rename changes
    the column name, not the column key or its cells.
  - `literals_follow_entries_and_are_keyed_by_their_text`: literals sort after
    entries, by text; the same literal text is one column.
  - `activity_outside_any_lane_has_no_responsible_and_ghost_roles_are_skipped`:
    an activity in no lane has no R; rows keep the activity-table order.
  - `csv_matches_the_pinned_file_and_is_deterministic`: the CSV equals the
    byte-pinned `raci.expected.csv` (UTF-8 BOM, CRLF) and two calls agree.
  - `csv_quotes_fields_and_neutralises_formulas`: quotes, commas and newlines
    follow RFC 4180; fields starting with `=`, `+`, `-` or `@` (activity names,
    role names, literals) are prefixed with `'`.
  - `raci_definitions_have_fixed_ids_and_are_valid_mutations`: the three IDs are
    distinct, match the fixture, round-trip through `from_id`, and all three are
    accepted by `CreateAttributeDefinition` (a second creation is refused).
  - `unknown_process_has_no_matrix`.
- `apps/processcraft-cli/tests/cli.rs`
  `export_raci_writes_the_pinned_csv_and_refuses_to_overwrite`: `export-raci`
  writes the pinned bytes, leaves an existing file untouched without
  `--overwrite`, replaces it with the flag, and `query raci` reports the same
  columns and the same cells for the checked row.
- `apps/processcraft` unit test
  `app::analyst_views::tests::toggling_a_raci_cell_is_one_undo_step_and_creates_the_definition_once`:
  the first toggle creates the fixed definition and the value in one batch
  (generation +1); a second toggle clears the value; each toggle is one undo
  step.

### Design decisions where the plan was silent

- Matrix cells are sparse per row (a role without a letter has no cell) and each
  cell carries its target and name, so a row is readable without the column
  list; the MCP and attached `raci` query page the rows (items), not the
  columns. The CLI `query raci` returns the whole matrix.
- A role is a column only when it holds at least one letter in the process. The
  desktop grid additionally offers "Add role column…" for dictionary roles not
  yet in use; those extra columns are view state and are not part of the query
  or the CSV.
- If a lane has several `Role` links, the first one in link order is the
  Responsible role (as `ActivityRow.role`). Dictionary targets that no longer
  resolve to a name are skipped.
- The fixed RACI definitions are created with their fixed ID by the first
  toggle, in the same batch as the value. If a project already has a different
  definition named "Accountable" (and so a different ID), creation is refused by
  core and the GUI shows core's message.
- The RACI segment selection is view state in egui memory, not project data.

### Limits and what is not established

- The RACI grid, its sticky name column, keyboard toggling and the "Export CSV"
  save dialog were not exercised manually and not rendered in a test; only the
  mutation path behind the toggle is tested. Keyboard reachability rests on the
  use of standard selectable labels, not on a Tab-order test.
- MCP and attached `query raci` compile against the same code as the other query
  kinds but have no dedicated transport test; the CLI test covers the shared
  core projection only.
- The matrix is recomputed when the model generation changes; no timing was
  measured (M5.6).
- Attribute values on activities are not copied by clipboard or duplicate
  (decision 6), so RACI assignments are not either.
