# ProcessCraft native UI alignment

**Goal:** Bring the working M2 application into the already approved FigCraft workbench before M3. This executes the existing visual direction; it does not request a new design approval or create a web prototype.

**Visual targets:** `design/previews/01-editor.png`, `02-library.png`, `03-dictionary.png`, `04-table.png` and their English Dark variants. Sources are 1600 × 1000 authoring boards exported at 1.5×. Use the existing 23 semantic tokens, Lucide assets and licensed Noto fonts. Actual model data and capability states replace the mockup's illustrative approvals/revisions.

## Visible corrections

- [x] Replace the wrapping command dump with File/Edit/View/Model/Tools menus and a compact primary toolbar. Display file status and local operation state in a footer; move scale/theme and attachment credentials into appropriate menus/windows.
- [x] Add a permanent navigation rail, coherent project/folder sidebar and active-document tabs. Keep the existing core dispatcher, IDs, draft guards and effect lifecycle.
- [x] Give the editor a real document title/breadcrumb, Model/Table switch, spacious bounded canvas, automatic first fit, palette, structured Properties panel and bottom Problems area. Reuse the current render/gesture pipeline and actual diagrams.
- [x] Rebuild Library as a searchable full-width document list with counts, selection and model details; Dictionary as categories/list/editor with live usages; Table as readable columns, selected-row actions and actual model preview where practical.
- [x] Apply readable typography, consistent button/field/row sizing, restrained borders and selected states. Reuse real source icons; do not use Unicode stand-ins or introduce another SVG runtime.
- [x] Keep narrow/high-DPI layouts usable through bounded scrolling and collapsible/contextual panels. Preserve English and Light/Dark; never launch Narrator.

## Ownership and verification

Root owns shell/layout and app integration. A Terra worker owns theme/widgets and source icon assets. A second Terra worker owns `app_m2.rs` data views. Exclusive file ownership avoids collisions; no core/format/command/automation changes are needed. Reuse the existing checkout (no Git metadata).

Capture the actual native baseline and the final four views using PrintWindow, compare against the visual targets, and fix visible spacing/clipping. Verify selection, dictionary drafts/rename/history, native save and protected attachment remain functional. Run formatting, all-target Clippy and the existing desktop/full-workspace checks appropriate to presentation changes. Do not add tests that merely mirror styling. Keep M3 controls absent until implemented.

Completed: see `docs/implementation/ui-alignment-evidence.md` for checks, native captures and crate research.
