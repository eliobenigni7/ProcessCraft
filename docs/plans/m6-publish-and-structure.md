# ProcessCraft M6 Implementation Plan — Publish and structure

**Goal:** Deliver M6.1–M6.3 of the [roadmap](roadmap.md): a process handbook exported as static HTML
and as PDF, a process map with drill-down across the documents of a project, and a semantic diff for
files and checkpoints.

**Architecture:** The handbook is generated in memory by `processcraft-render` from a
`Session::snapshot()` and the shared scene; `processcraft-io` and the callers write the bytes through
the existing `FileGrant`/`WriterSession` path. The process map is a second, small document kind in
`processcraft-core` with its own mutations. The diff generalises the existing checkpoint comparison in
`queries.rs`. No new crate; no new runtime dependency is assumed.

**Authority:** [roadmap](roadmap.md) §M6; [solution design](../design/solution-design.md) §1.2
(document kinds are specified separately), §10.2–10.3 (revisions and comparison), §13 (shared scene,
export contract), §16.1 (CLI), §17 (limits). Depends on [M5](m5-analyst-foundations.md): attributes
(M5.1), RACI (M5.3) and the overlay (M5.4).

**Status:** Planned; not started.

## Boundaries

In scope: one project in, one handbook out; a structured process map; navigation between documents;
comparison of two project states.

Out of scope, and not to be scaffolded:

- A hosted portal, comments or approvals on the published handbook, incremental publishing.
- Free-form drawing on the process map (see Task M6.2 and Open decision 3).
- Three-way merge, or applying a diff as a patch.
- Diff of content that is not in a qualified profile: for `Capability::PreservedSource` documents the
  diff reports only whether the source bytes differ.
- MCP export of the handbook. The MCP `Export` tool is per document (`document_id` is required);
  handbook export is offered in the GUI and the CLI first.

## Findings from the current code that shape this plan

1. **The PDF pipeline outlines all text.** `crates/processcraft-render/src/pdf.rs` states it in
   `OUTLINE_NOTICE` ("text is not searchable, selectable or tagged"), and the M3 evidence records "no
   extractable text". The roadmap acceptance "its PDF text is extractable" cannot be met by "the
   existing PDF pipeline" as it is. `pdf.rs` also places exactly one `DiagramScene` on tiled A4
   landscape pages; it has no notion of text pages. Task M6.1 therefore contains two pieces of new
   renderer work (an invisible text layer and a minimal block layout), or the PDF is cut; see Open
   decision 2.
2. **SVG already has a searchable layer.** `DiagramScene::to_svg` draws outlines and adds a
   transparent `<text>` layer (`SVG_TEXT_NOTICE`). Inline SVG in the handbook is searchable in the
   browser without further work.
3. **Double-click on a shape starts label editing** (`apps/processcraft/src/app/canvas.rs`, the
   `response.double_clicked()` branch that opens `label_edit`), matching bpmn-js. The roadmap's
   "double-click drills down" conflicts with it; see Open decision 4.
4. **`ProjectData.links` (`NavigationLink { source, target: DocumentId, label }`) is modelled,
   validated, pruned on delete and compared, but no `Mutation` creates one.** Only tests push links
   directly. Drill-down can use it once a mutation exists. Such a link can never be broken today:
   `ProjectData::validate()` rejects a link whose target document is missing, and
   `Mutation::DeleteDocument` silently removes every link that points at the deleted document. The
   roadmap's "broken links show in the problems panel" therefore needs a decision (Open decision 8).
5. **A collapsed sub-process already owns a diagram.** `toggleCollapse` moves the content's DI into a
   diagram whose plane is the sub-process. Drilling into a collapsed sub-process is a diagram switch
   inside the same document, not a cross-document link.
6. **`queries::compare_checkpoint(project, checkpoint_id)` compares only the working project with one
   checkpoint**, returning `CheckpointComparison`. Its body works on a `FrozenProject` and a
   `ProjectData`, so it generalises to any two states with a signature change.
7. **CLI output is always JSON today** (`--json` flags say "always the output format"). A diff usable
   as a git `difftool` needs a human-readable text mode, the first in the CLI.
8. **The CLI binary is `processcraft-cli`.** The roadmap writes `processcraft diff`; `processcraft.exe`
   is the desktop application.

## Delivery order

M6.2 first, because the handbook's entry page is the process map when one exists. Then M6.1. M6.3 is
independent of both and can run in parallel with either; its visual part needs only M5.4.

## Task M6.2: Process map and drill-down

**Own:** new `crates/processcraft-core/src/process_map.rs`; `crates/processcraft-core/src/{model,
commands,identity,history,queries,validation}.rs`; `crates/processcraft-io/src/native.rs`; new
`crates/processcraft-render/src/map.rs`; `apps/processcraft/src/app/{canvas,shell,analyst_views}.rs`,
new `apps/processcraft/src/app/process_map.rs`; `apps/processcraft-cli/src/{args,run,mcp}.rs`;
`apps/processcraft/src/attachment.rs`.

### Process map

- [ ] Model in `process_map.rs`: `ProcessMap { id: DocumentId, name, folder: Option<XmlId>, bands:
  Vec<MapBand> }`, `MapBand { kind: Management | Core | Support, items: Vec<MapItem> }`, `MapItem {
  id: XmlId, name, description, target: Option<MapTarget>, children: Vec<MapItem> }`, `MapTarget`:
  `Document(DocumentId)` or `Map(DocumentId)`. The map is structured data with no stored geometry;
  the renderer lays it out (management band on top, core items as a chevron chain, support band
  below). Limits: 200 items per map, depth 4, 100 maps per project.
- [ ] `ProjectData.maps: Vec<Arc<ProcessMap>>` (serde default, skipped when empty), also in
  `FrozenProject`. `ProjectData::validate()` checks unique IDs across documents and maps, folder
  existence and acyclic map-to-map targets. A target that no longer exists is allowed and reported
  as a diagnostic. Native capability `"process-map-v1"`
  written only when a map exists (`native.rs`, same pattern as M5.1).
- [ ] Commands: `CreateProcessMap { map }`, `SetProcessMap { map }` (whole-value replace, like
  `SetSupportingCatalog`), `DeleteProcessMap { map_id }`, with descriptors. `DeleteDocument` and
  `DeleteProcessMap` leave items that targeted the removed document in place with a dangling target,
  which is reported as a diagnostic rather than silently cleared.
- [ ] Render: `map.rs` builds a `DiagramScene` from a `ProcessMap` with the shared `TextEngine`
  (deterministic layout, same fonts), plus hit regions per item. It is used by the canvas, by
  SVG/PNG/PDF export and by the handbook.
- [ ] Library: maps appear in the library list and search (`queries::library_search`) with a kind
  label; rename and move reuse `DocumentProperties` semantics through `SetProcessMap`.
- [ ] Desktop: opening a map shows the rendered scene read-only on the canvas area and an outline
  editor in the side panel (`process_map.rs`): add, rename, reorder and remove items, choose the
  target from a document picker. Every edit submits `SetProcessMap`. Keyboard access as for the
  existing outline.

### Drill-down

- [ ] `Mutation::SetNavigationLink { source: ElementKey, target: Option<DocumentId>, label: String }`
  with a descriptor; allowed sources are call activities, collapsed sub-processes and (for M8)
  business rule tasks. At most one link per source. The existing invariant stays: a link always
  points at an existing document. `DeleteDocument` keeps removing inbound links but now names each
  removed link in `ChangeSet.warnings` (the existing `warn` facility), and the GUI delete
  confirmation lists them from a new `queries::inbound_links(project, document_id)`.
- [ ] `queries::drill_target(project, &ElementKey) -> DrillTarget` in `queries.rs`:
  1. a `NavigationLink` from the element → `Document(id)`;
  2. `NodeDetails::CallActivity { called_element }` whose `QNameRef` matches the
     `DefinitionsMetadata.target_namespace` and a process ID of another project document →
     `Process(document_id, process_id)`;
  3. a collapsed sub-process with its own diagram → `Diagram(diagram_id)`;
  4. a call activity whose project-local `called_element` resolves to nothing →
     `Broken(reason)`; otherwise `None`.
- [ ] Problems: `ProjectData::diagnostics()` reports `unresolvedCalledElement` when a call
  activity's namespace belongs to a project document but the process is missing (an external
  namespace stays opaque, as `activity_reference_issues` treats it today), and `brokenMapTarget`.
  The existing `incompleteCallActivity` is unchanged.
- [ ] Desktop: a drill-down action on elements with a `DrillTarget` — the marker on the shape, the
  context pad, Enter on the selected element, and Ctrl+double-click (see Open decision 4). It goes
  through `prepare_navigation()` so drafts are committed or kept, then switches `self.doc` /
  `self.diagram_id`. A breadcrumb in the document header (`shell.rs`) records the path of
  (document, diagram) pairs and returns on click or Alt+Left; the existing choreography scope
  breadcrumb (`ScopeInfo.label` in `canvas.rs`) is the visual precedent. The trail is view state.
  The inspector of a call activity offers "Link to process…", which sets the `NavigationLink` and,
  when the target has a target namespace, `called_element` in the same batch.
- [ ] CLI `query drill-targets <document_id>` and MCP `Query::DrillTargets` in `mcp.rs` and
  `attachment.rs`, so an agent can follow the hierarchy.

**Tests:** new `crates/processcraft-core/tests/process_map.rs` (limits, cycles, target validation,
undo, checkpoint capture and restore, dangling targets after delete); new
`crates/processcraft-core/tests/drill_down.rs` (each resolution order case, external namespace stays
opaque, broken cases produce exactly one diagnostic, link pruned when its source is deleted — the
existing prune sites already cover `links`). `crates/processcraft-io/tests/native.rs` (capability and
round trip). `crates/processcraft-render/tests/render_integration.rs` (map scene deterministic, text
within bounds, long names wrap). GUI unit tests in `process_map.rs` and `canvas.rs` (drill and return
keep drafts; plain double-click still edits the label).

**Acceptance:** a three-level fixture project (landscape → process → called process and a collapsed
sub-process) can be traversed down and back in the GUI by keyboard only; deleting the called
document lists its inbound links before the delete, and afterwards the call activity's unresolved
`calledElement` and the map item's missing target each appear as a problem that navigates to its
element; `query drill-targets` returns the same targets.

## Task M6.1: Process handbook

**Own:** new `crates/processcraft-render/src/handbook.rs`; `crates/processcraft-render/src/{pdf,scene,
lib}.rs`; `apps/processcraft/src/{effects.rs,app/shell.rs}`; `apps/processcraft-cli/src/{args,
run}.rs`; new `scripts/check_handbook.py`.

### HTML

- [ ] `handbook::export_html(project: &ProjectData, options: &HandbookOptions, fonts: &mut
  TextEngine) -> Result<Handbook, RenderError>` returning bytes plus diagnostics. Input is a snapshot;
  nothing is written here. Only `Capability::Editable` documents get diagrams and element pages;
  other documents are listed with their capability and a note, never rendered from guessed content.
- [ ] Output form: **one self-contained `.html` file** (see Open decision 1). Sections with stable
  `id`s are the "pages"; a short inline script shows one section at a time from the URL fragment, and
  without scripting all sections are visible in document order. Inline CSS, inline SVG, inline search
  index; no `src`, `href` or `url()` pointing outside the file except user-authored links.
- [ ] Sections: entry page (the process maps of M6.2 when present, otherwise the library folder
  tree); one section per document with each diagram as inline SVG from `BpmnScene` (Light
  appearance), with a transparent link layer built from `BpmnScene.hits` so shapes are clickable; one
  section per activity with name, documentation (`DocumentationSet::plain_text`, or the text content
  of structured documentation), lane and role, systems and documents (`activity_table`), attributes
  (M5.1), RACI row (M5.3), outgoing flows and comments; dictionary and glossary with reverse usages
  (`dictionary_usages`); RACI matrix per process.
- [ ] Search: an inline JSON index of (section id, title, kind, text) and a substring search over
  it. No network, no web worker, no storage.
- [ ] Trust boundary. Names, documentation and comments may come from imported files: every text is
  HTML-escaped, the JSON index is escaped for a script context, `Url` attribute values become links
  only for `http`, `https` and `mailto` (with `rel="noopener noreferrer"`), and the file carries a
  `Content-Security-Policy` meta tag that allows inline style and script only.
- [ ] Deterministic output: the same snapshot and options give the same bytes. A generation date is
  an explicit option, not read from the clock inside `render`.
- [ ] Bounds: refuse before building when the estimated output exceeds 64 MiB or the project exceeds
  the existing document and element limits; diagnostics from each scene (clipped labels and similar)
  are collected and need acknowledgement, as `export_diagram` requires today.

### PDF

- [ ] Text layer: `pdf.rs` adds an invisible text run per visible line of each `Label` (text
  rendering mode 3) using one small embedded glyphless font with a `ToUnicode` map, mirroring the
  SVG transparent layer. This makes existing diagram PDFs extractable too; `OUTLINE_NOTICE` is
  reworded to what then holds (searchable and extractable, not tagged for accessibility).
- [ ] Pages: generalise `pdf::export` from one tiled scene to a list of page scenes. A minimal block
  layout in `handbook.rs` — heading, paragraph, key/value table, grid table, diagram scaled to the
  page — fills A4 portrait `DiagramScene` pages using `TextEngine` for wrapping. No floats, no
  columns, no hyphenation. Page limit 2,000 with a refusal above it.
- [ ] `handbook::export_pdf` produces the same sections in the same order as the HTML, with a
  contents page and page numbers.

### Callers

- [ ] CLI: `export-handbook <INPUT> --out <FILE.html> [--pdf <FILE.pdf>] [--document-id <ID>]…
  [--overwrite] [--acknowledge-diagnostics]`, written through `write_export`.
- [ ] Desktop: "Process handbook (HTML)…" and "Process handbook (PDF)…" in `interchange_ui`
  (`shell.rs`); generation runs in `Effects::export` off the UI thread with a new GUI `ExportKind`
  variant; indeterminate write outcomes use the existing reconcile path.

### Checks

- [ ] `scripts/check_handbook.py` (independent of the Rust code): strict HTML5 parse, every `#id`
  link resolves, no remote resource reference, no `fetch`/`XMLHttpRequest`/`import` in the script,
  and — for the PDF — text extraction with pypdf contains every activity name of the fixture.
- [ ] `crates/processcraft-render/tests/` new `handbook.rs`: structure and anchors for a fixture
  project, escaping of hostile names (`<script>`, quotes, `</script>` in documentation), determinism,
  size refusal, non-editable documents listed without diagrams. `tests/export.rs`: text layer present
  and existing PDF geometry unchanged. `apps/processcraft-cli/tests/cli.rs`: `export-handbook`
  writes both files and refuses to overwrite.

**Acceptance:** for the procurement fixture extended with attributes, RACI and a process map:
`check_handbook.py` passes; the file opened from `file://` in two browsers navigates, searches and
prints (recorded manually in the evidence, with versions); the PDF's extracted text contains every
activity name. Opening from a file share is checked the same way. SharePoint behaviour is recorded as
observed, not promised (see Risks).

## Task M6.3: Semantic diff

**Own:** `crates/processcraft-core/src/{queries,overlay}.rs`; `apps/processcraft-cli/src/{args,
run}.rs`; `apps/processcraft/src/app/review.rs`; `crates/processcraft-render/src/bpmn.rs` (overlay
palette only).

- [ ] Core: `queries::compare_projects(before: &ProjectData, after: &ProjectData) ->
  CheckpointComparison`; `compare_checkpoint` becomes a thin wrapper
  (`checkpoint.content.project()` versus the working project). Extend the result with what M5 added:
  attribute definitions and values (already per element from M5.1), conventions profile, process
  maps. The paginated field views (`checkpoint_fields_page`) keep working.
- [ ] CLI `diff <A> <B> [--json] [--exit-code]` in `args.rs`/`run.rs`. Each side is a native project
  or a BPMN file (detected as `inspect` does with `is_xml`). Native versus native pairs documents by
  `DocumentId`. When either side is BPMN, both sides must contain exactly one document and those two
  are paired; anything else is refused with exit 2 and a message. Elements pair by
  `ElementSelector`. A `PreservedSource` document reports only `sourceBytesDiffer`. Default output is
  a plain-text summary grouped as §10.3 requires (added, removed, renamed, changed properties,
  changed attributes, changed flows, moved shapes); `--json` emits the `CheckpointComparison`. Exit 0
  on success whether or not differences exist; `--exit-code` returns 3 when they do.
- [ ] Document the git setup in `README.md`: `git difftool --extcmd="processcraft-cli diff"` and the
  equivalent `difftool.processcraft.cmd` configuration. No git integration code.
- [ ] Visual comparison. `overlay::comparison_overlays(&CheckpointComparison, document_id,
  diagram_id) -> (Overlay, Overlay)`: one for the newer state (added, changed, moved) and one for the
  older state (removed, changed). The M5.4 overlay gains a fixed three-colour "diff" palette and
  applies to any shape and to edge strokes, not only activity fills. In `review.rs`, the checkpoint
  comparison view gets a "Show on diagram" toggle that feeds the first overlay to the canvas and the
  second to the existing reference window (`reference_window`, `ReferenceImage`); a second selector
  allows comparing two checkpoints instead of checkpoint versus working state.

**Tests:** `crates/processcraft-core/tests/checkpoint_compare.rs` (existing cases still pass through
the wrapper; two-checkpoint comparison; attribute and map changes). `apps/processcraft-cli/tests/
cli.rs`: native/native, BPMN/BPMN and mixed; refusal on multi-document BPMN pairing; text and JSON
output; `--exit-code`; an end-to-end `git difftool` run in a temporary repository. Render test for
the diff palette in light and dark. GUI unit test: toggling the overlay changes neither generation
nor dirty state.

**Acceptance:** for two saved versions of a fixture, `diff --json` lists exactly the edits made
between them (one per change class); the same edits are highlighted on the two diagrams in the GUI;
`git difftool` shows the text summary.

## Task M6.4: Qualification and evidence

- [ ] Quality gates from `CLAUDE.md`; new `docs/implementation/m6-evidence.md` with the handbook
  browser checks, PDF extraction result, measured handbook size and generation time for the medium
  fixture, and the limits that were actually exercised.
- [ ] Update `README.md`, `CLAUDE.md` (document kinds, handbook module) and the roadmap status.

## Risks

| Risk | Mitigation |
|---|---|
| Handbook file too large for big projects (outlined glyph paths in every SVG) | Measure on the medium fixture first; size refusal; Open decision 1 offers the multi-file form |
| SharePoint Online may download `.html` files instead of rendering them, depending on tenant settings | Promise `file://` and file shares only; record SharePoint as observed |
| Block layout for the PDF grows into a typesetting engine | Fixed block set; no floats or columns; Open decision 2 offers the print-stylesheet cut |
| Glyphless text layer misorders right-to-left or mixed-direction text on extraction | Emit logical-order text per line; check Arabic and CJK fixtures with `check_pdf.py`; state the result |
| Hostile text in imported models reaches the HTML | Escaping at one function, CSP meta, tests with hostile names |
| Positional pairing of BPMN files hides a renamed process | Refuse when either side has more than one document; report unmatched element IDs as added/removed |
| Map-to-document links go stale silently | Dangling targets are diagnostics, never auto-cleared |

## Open decisions

1. **Handbook form.** Recommended: one self-contained `.html` file — one atomic write through the
   existing `WriterSession`, trivially copied or mailed, no broken relative links. Alternative: a
   folder with one file per page, as the roadmap's "site … one page per activity" reads literally; it
   needs directory creation, many writes through `RootGrant`, and a defined partial-failure outcome.
2. **Native PDF handbook.** The roadmap asks for it with extractable text; the current pipeline has
   neither text pages nor extractable text. Recommended: build the text layer (useful for every PDF
   export) and the minimal block layout. Simpler cut: ship HTML with a print stylesheet and let the
   browser print to PDF — no layout code, extractable text, but not scriptable from the CLI.
3. **Process map editing.** Recommended: structured bands and items with automatic layout, edited in
   an outline. Alternative: a free-form canvas with stored geometry, which repeats much of the BPMN
   canvas work for little analyst value.
4. **Drill-down gesture.** Double-click is label editing today. Recommended: keep it; drill down by
   marker click, context pad, Enter and Ctrl+double-click. Alternative: double-click drills down on
   call activities and collapsed sub-processes only, and F2 edits their label.
5. **HTML validator.** Recommended: `html5lib` strict parsing in `scripts/check_handbook.py` (pip
   only, consistent with the other scripts). Alternative: the Nu Html Checker, which needs a JVM.
6. **Diff exit status.** Recommended: 0 unless `--exit-code`, then 3 on differences (1 stays
   "runtime failure" as in solution design §16.1). Alternative: follow `diff(1)` and return 1.
7. **Handbook appearance.** Recommended: Light only, matching exports. Alternative: a dark variant
   through `prefers-color-scheme`, which doubles the inline SVG.
8. **Broken navigation links.** Today a `NavigationLink` cannot dangle (finding 4). Recommended:
   keep that invariant, warn and list inbound links when a document is deleted, and report only
   unresolved `calledElement` references and map targets as problems. Alternative: let links dangle
   and report them, which changes `validate()`, `DeleteDocument` and their existing tests.
