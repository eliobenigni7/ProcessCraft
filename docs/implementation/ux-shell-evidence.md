# UX/UI shell redesign — evidence

Plan: [`docs/plans/ux-shell.md`](../plans/ux-shell.md). Every step below passed `cargo fmt --all -- --check`,
workspace all-target/all-feature Clippy with `-D warnings`, and the workspace all-target/all-feature
tests. Native screenshots of the release build (1560×960, sample project from
`tests/fixtures/commands/procurement.json`) are under `target/ux-shell/` (not committed).

## 1. Foundation

- **Tokens:** `apps/processcraft/src/theme.rs` holds the light-first tokens. The amber app colour is
  the accent, with `#9a6400` as the ink colour for amber text; blue is kept for canvas selection.
- **Shape and density:** radii 4/6/8/12, 28 px controls, and a 13 px body text with an 11–16 px type
  scale.
- **Widget states:** set through `craft_ui::theme::WidgetPalette`. Controls show their state by fill;
  there are no resting outlines.
- **Interface fonts:**
  - Inter Regular, Medium and SemiBold, plus JetBrains Mono, all installed through
    `craft_ui::fonts::UiFonts`.
  - The bundled Noto Arabic and CJK faces act as fallbacks, and system fonts are off.
  - The four new faces are byte-identical to WordCraft's pinned set. They are recorded in
    `assets/fonts/provenance.json` (OFL-1.1, SHA-256 checked by `text_parity`).
  - They are used only by the interface; diagram rendering and export still use Noto.
- **Test:** `theme::tests::interface_fonts_cover_latin_arabic_and_chinese` checks Latin, monospace,
  Arabic and Chinese coverage.
- **Screenshot:** `step1-foundation-light.png`.

## 2. Shell

- **Rows above the canvas:** the toolbar row is gone, leaving three rows: menu bar, document tabs and
  document header.
- **Document header:** a breadcrumb, the title, a Model/Table segmented control and the Diagram menu.
  On the right are Undo, Redo, Save, Export and a primary Validate button. The compact header
  (height < 600 px) shows the same actions as icons.
- **Rail:** panel toggles, focus mode and Settings now sit at the foot of the rail.
- **Panels:** separated by hairline seams instead of boxed outlines. The Project sidebar keeps search,
  the model tree and a collapsible Outline; the BPMN palette has left it.
- **Canvas:** full-bleed, with no frame or zoom row. `app/overlays.rs` draws floating capsules
  (radius 12, soft e1 shadow; a hairline border in dark mode):
  - **Palette**, on the left: Space tool; start, intermediate throw and end events; exclusive and
    parallel gateways; task, user task and subprocess; pool and lane; and "More elements". Elements
    can be clicked or dragged onto the canvas. Choreography diagrams get an "Add choreography
    element" menu instead.
  - **Zoom**, at the bottom right: zoom out, the percentage (click resets to 100%), zoom in and Fit.
  - **Arrange**, at the top centre: Align and Distribute, shown when two or more shapes are selected.
- **Fit:** leaves out the strip the palette covers.
- **Status bar:** 28 px. It shows the file name, the save state (amber when unsaved), lane and
  activity counts for the shown process, and "BPMN 2.0 · Local, offline".
- **Focus mode:** `Ctrl+.`, the View menu or the rail. It hides the rail, sidebar, document tabs and
  header, inspector and problems.
- **Icon buttons:** use `craft_ui::buttons::IconButton`. They are transparent at rest and amber when
  selected.
- **New Lucide icons:** maximize, triangle-alert, message-square, history, circle-check and scan, all
  lucide-static 0.469.0. Their source SVGs are in `design/assets/lucide`, and the 48 px white masks
  in `assets/ui-icons` were rendered with VectorCraft's CLI.
- **Symbols:** egui's Hack and emoji faces back up Inter for symbols such as ▾ ✓ ⋯.
- **Screenshots:** `step2-shell-light.png` and `step2-focus.png`.

## 3. Canvas appearance

- **New appearances:** `processcraft_render::Appearance` gains `CanvasLight` and `CanvasDark`. The
  interactive editor is the only user; previews, the frozen checkpoint viewer and every export still
  use `Light` and `Dark`. The renderer's export tests passed unchanged.
- **Canvas look:**
  - Paper is `#FAFBFC` in light mode and `#121417` in dark mode.
  - A dot grid every 16 model units stays 1.5 px on screen and is hidden when dots would be closer
    than 10 px. All the dots form one path under the scene.
  - Activities are white (`#20242A` in dark mode) with `#2A2E36` strokes.
  - Pool and lane header bands are tinted (`#F2F4F7` / `#23272D`).
- **GUI overlays:**
  - Canvas selection uses the blue selection token: a 2 px rounded outline with an 8% wash.
  - Resize handles are 8 px white squares with selection strokes; bendpoints use the same colour.
  - Snap guides are selection blue at 60%.
  - Context-pad buttons have radius 6, a hairline border and a selection-soft hover.
- **Test:** `bpmn_scene::canvas_appearance_adds_dots_and_keeps_export_appearances_plain` checks the
  dots, their hiding when zoomed out, and that the Light and Dark paper is unchanged.
- **Screenshots:** `step3-canvas-light.png` and `step3-canvas-dark-selected.png`. From this step on,
  screenshots capture only the app window (PrintWindow).
- **Limit:** the synthetic click in the screenshot harness does not reach winit, so selection is
  covered by code and the existing GUI tests, not by a screenshot.

## 4. Bottom panel

- **One docked panel:** a single resizable bottom panel (`ShellState::bottom`, a `BottomTab`)
  replaces the Problems panel and the floating "Local comments" and "Checkpoints and local
  reference" windows.
- **Tabs:**
  - Tabs are `craft_ui::tabs::Tab`, with Lucide icons, an amber underline on the active tab and
    count pills.
  - The Problems icon and pill turn red when errors exist.
  - Problems rows show a severity icon, and selecting a row selects the element.
  - The empty state shows "No model problems found." with a success check.
- **Opening the panel:**
  - Validate, the Model menu entries, the inspector's "Element comments…", View › Bottom panel and
    the new `Ctrl+J` all open the matching tab. Focus mode hides it.
  - Validation still runs off the UI thread, only while the panel is open.
- **Modal dialogs kept:** the restore confirmation and the frozen checkpoint viewer stay modal
  windows.
- **Test:** `shell::tests::bottom_panel_tabs_replace_the_review_windows` renders each tab and checks
  its content in the accessibility tree, that no comments window appears, and that focus mode hides
  the panel.
- **Limit:** inputs sent to a background window do not produce a new frame, so this step has no
  screenshot. The test above is the evidence.

## 5. Inspector and settings

- **Header card:** the inspector opens with a card showing the element's palette icon, its type in
  small caps and its name, with Change type (R) and Comments actions. The Comments action shows the
  element's comment count and opens the Comments tab. With nothing selected, the card reads
  "Project and document".
- **Sections:** the rest of the inspector is one column of `widgets::section` groups. Each is
  collapsible with an animated chevron, a SemiBold title and an optional count, and keeps its open
  state for the session.
  - Element: General (names, documentation, condition, association direction, choreography flow,
    BPMN properties); Responsibility & systems (owner, contact and dictionary links); Advanced BPMN
    (closed by default); Catalogs; Connections and lanes.
  - Document: General; Data & collaboration (closed by default); Advanced BPMN; Connections and lanes.
- **Data & collaboration:** the former "Collaboration and data" floating window now renders inside
  this inspector section. Model › Data & collaboration… deselects, shows the inspector and opens the
  section. The draft and session rules of `collaboration.rs` are unchanged.
- **Settings:** a Settings dialog (Tools › Settings… or the rail) replaces the "Local automation"
  window. It has an Appearance section (Light/Dark) and the Local automation section.
- **Width:** the inspector is now 280–420 px wide.
- **Test:** `shell::tests::inspector_sections_host_collaboration_and_settings` checks that the
  section is closed by default, that the menu command opens it in place with no floating window, and
  the Settings dialog sections.
- **Screenshot:** `step5-inspector.png`.

## 6. Rail and Library

- **Rail:** it now has three places: Library, Model and Dictionary. Table is a way of looking at the
  open model, so it is only the Model/Table segment of the document header, and the Model place
  stays lit while Table is shown.
- **Table view header:** it uses the same breadcrumb, title and segmented control as the model
  header, so you can return to Model with one click.
- **Library:**
  - A primary "New model" button sits in the header.
  - The Model details pane opens with a framed preview of the model's diagram (the existing
    non-interactive `diagram_preview`), then the name in SemiBold, its counts, its source,
    "Open in modeler" and "Move model to…".
  - The columns stay Name, Folder, Notation and Capability. The model stores no owner or
    modification date, so those prototype columns were left out rather than filled with invented
    data.
- **Test:** `shell::tests::rail_has_three_places_and_library_shows_model_details`.
- **Screenshot:** `step6-start.png` (empty project, three-place rail and empty canvas state). Library
  needs a click to reach and is covered by the test.
