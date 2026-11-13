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
