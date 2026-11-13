# UX/UI shell redesign

Status: planned (decisions agreed 2026-11-13). Scope: the desktop GUI shell in `apps/processcraft`
and a screen-only canvas appearance in `processcraft-render`. Modeling behavior is unchanged.

## Goals

- **Primary user:** a process analyst who draws and documents. **Fast lane:** an expert BPMN
  modeler who uses the keyboard.
- **The problems we are fixing:** scattered information architecture (floating windows, too many
  stacked rows), modeling friction (the palette is buried in the sidebar), and a stock egui look.
- **Keep:** egui on the pinned forks, the canvas interaction model, and the FigCraft prototype in
  `design/` as the base layout. Where this plan disagrees with the prototype, this plan wins.
- **Visual identity:** the family conventions of the ArtCraft office apps (WordCraft, GridCraft,
  DeckCraft), combined with patterns from modern BPM and pro tools (SAP Signavio, Camunda/bpmn-js,
  Figma UI3, Linear).
- **Copy:** all UI copy, identifiers and documentation are in English.

## Principles

1. **Floating windows are only for one-off modal actions.** Persistent content always docks.
2. **The rail holds places; the document header holds ways of looking at a document.**
3. **The canvas fills the space.** Palette and zoom are floating overlays, not rows or panels.
4. **Advanced content is collapsed, not hidden behind tabs.**
5. **Every palette and context-pad action has a shortcut, and the tooltip shows it.**

## Target layout

```
┌ File Edit View Model Tools ───────────────────────────── ProcessCraft ┐
│ [Procurement ×] [Onboarding]                                          │
├──┬──────────┬ Project › Procurement  [Model|Table]  ↶ ↷ 💾 Export ✓ ┬───────────────┐
│▤ │ Search   │╭─╮                                                 │ ▣ User task    │
│✎ │ Tree     ││○│            full-bleed dotted canvas             │   Approve PO   │
│▥ │ Resources││◇│                                                 │ General      ▾ │
│  │ Outline  ││▭│                                     ╭───────╮   │ Responsibility▾│
│⚙ │          ││…│                                     │ − ⤢ + │   │ Data & collab ▸│
│  │          │╰─╯                                     ╰───────╯   │ Advanced BPMN ▸│
│  │          ├ Problems (3) · Comments · Revisions ──────────────┤                │
├──┴──────────┴───────────────────────────────────────────────────┴────────────────┤
│ procurement.processcraft · Saved · 3 lanes · 6 activities · Local, offline       │
└──────────────────────────────────────────────────────────────────────────────────┘
```

## Decisions

| Area | Decision |
|---|---|
| Rows above the canvas | Drop the toolbar row. Undo, Redo, Save, Export and Validate move to the right side of the document header. Panel toggles move to the View menu and the bottom of the rail. The result is three rows: menu bar, document tabs, document header. |
| Palette | A floating vertical icon capsule on the canvas's left edge, with "More elements…" at the bottom. It is removed from the Project sidebar. |
| Canvas | Full-bleed, with no frame. Zoom and Fit sit in a capsule at the bottom right. Statistics move to the status bar. |
| Bottom panel | One dockable, resizable panel with **Problems · Comments · Revisions** tabs and an error-count badge. Restore and the frozen checkpoint viewer stay modal. |
| Rail | **Library · Model · Dictionary.** Table is only a toggle in the document header. |
| Other floating windows | "Collaboration and data" becomes an inspector section. Local automation (MCP) moves to a Settings dialog. Paste BPMN stays modal. |
| Inspector | A header card (element icon, type, name). Below it, one column of collapsible sections: General, Responsibility & systems (dictionary Role/System/Document links), Data & collaboration, and Advanced BPMN (collapsed by default). Each section remembers whether it is open, and a count badge appears where useful. |
| Library | A details pane in the right slot (thumbnail, metadata, "Open in modeler"). Columns show only data that exists: name, notation, owner, modified. |
| Expert lane | Shortcuts in tooltips. No command palette yet. |
| Narrow windows (<980px) | Unchanged (floating Project/Properties); revisit in M4. |
| Canvas appearance | A new **screen-only** `Appearance` in `processcraft-render`. SVG/PNG/PDF exports and their pinned outputs do not change. |
| Theme | Light first. Dark mode derives from the same tokens. |

## Visual system

### Typography

- **Faces:** Inter Regular, Medium and SemiBold for the UI, and JetBrains Mono for IDs and
  expressions. These are the same faces the office siblings use.
- **Fallbacks:** Noto faces for other scripts, through `craft_ui::fonts::UiFonts`.
- **Pinning:** the faces are pinned in `assets/fonts` with manifest entries.
- **Scale:** 11 status bar and captions · 12 labels · 13 body, inputs and tree · 14 SemiBold section
  headers · 16 SemiBold panel titles. Geometry fields use tabular numerals.

### Density

- **Interact height:** 28px.
- **Spacing scale:** 2, 4, 8, 12, 16, 24, 32.

### Radius

| Radius | Used for |
|---:|---|
| 4 | inputs, chips |
| 6 | buttons, list rows, menus |
| 8 | cards, popups, windows |
| 12 | floating canvas capsules |
| full | pills |

### Colour tokens

- **Accent:** the ProcessCraft app colour (amber). It marks identity and the active state: the
  primary button, the active rail item or tab, pool headers and the focus ring. Text on light
  backgrounds uses the amber ink.
- **Selection:** blue is reserved for canvas selection and snap guides, where amber would read as a
  warning.

| Token | Light | Dark |
|---|---|---|
| bg.canvas | `#FAFBFC` | `#121417` |
| bg.panel | `#FFFFFF` | `#1A1D21` |
| bg.subtle (hover, input fill, lane header) | `#F2F4F7` | `#23272D` |
| border | `#E3E6EB` | `#2E333A` |
| text.primary / secondary / muted | `#16181D` / `#5B6270` / `#8A91A0` | `#ECEEF2` / `#A8AFBC` / `#6F7684` |
| accent.fill | `#F2A516` | `#F2A516` |
| accent.ink (text, icons) | `#9A6400` | `#F5B940` |
| accent.soft (active row) | `#FDF3DC` | `#3A2E14` |
| selection / selection.soft | `#1E88E5` / `#E6F2FD` | `#4AA3F0` / `#1A2E44` |
| danger | `#D92D20` | `#F2635A` |
| warning | `#E8590C` | `#F79009` |
| success | `#12A150` | `#3CCB7F` |

### Elevation

- **e0:** a flat surface, separated from its neighbour by a 1px border or a fill change.
- **e1 (canvas capsules):** `0 1px 2px rgba(16,24,40,.06), 0 4px 12px rgba(16,24,40,.08)`.
- **e2 (popups, menus):** `0 8px 24px rgba(16,24,40,.14)`.
- **Dark mode:** use a 1px border of white at 8% instead of shadows.

### Signature details

1. Floating palette and zoom capsules (radius 12, e1).
2. A dotted canvas: 1px dots every 16px that fade out as you zoom out.
3. Selection as a 2px blue outline with an 8% wash. Handles are 8px white squares with selection
   strokes. Snap guides are the selection colour at 40%.
4. Pool and lane headers in `bg.subtle` with SemiBold labels, instead of heavy bands.
5. The inspector header card, and count badges on sections.
6. Problem and comment pins on shapes.
7. Focus mode (`Ctrl+.`): hides the rail, sidebar, inspector and bottom panel.

### Icons

- **Format:** Lucide PNGs exported at 2× and 3×, which keeps them sharp at 200% UI zoom without new
  dependencies.
- **New icons:** BPMN type icons for the palette, rail and bottom-panel icons.

### Shared components

- **From `craft-ui`** (already a pinned dependency, unused until now): `layout::Section` for
  inspector sections, `tabs` for the document and bottom-panel tabs, icon buttons,
  `theme::WidgetPalette` and `fonts::UiFonts`.
- **`widgets.rs`:** the in-house helpers it replaces are deleted.

## Deferred

- **Command palette (Ctrl+K):** add it if the shortcuts prove insufficient.
- **Model Status field and filter** (Draft/Approved): this needs a new core mutation.
- **Dictionary autocomplete while typing labels** (Signavio-style): a modeling feature that needs its
  own design pass.
- **SVG icons:** if the suite standardizes on them.
- **New narrow-window mode:** M4.

## Delivery

One commit per step, in this order. Each step passes `fmt`, `clippy -D warnings` and the workspace
tests, and is checked with a screenshot.

1. **Foundation:**
   - colour tokens and the dark derivation in `theme.rs`;
   - Inter and JetBrains Mono pinned, installed through `UiFonts`;
   - `WidgetPalette`;
   - the new density.
2. **Shell:**
   - three rows above the canvas;
   - the palette capsule and the zoom capsule;
   - a full-bleed canvas;
   - status-bar statistics;
   - focus mode.
3. **Canvas appearance:**
   - a screen-only `Appearance`;
   - the dotted background, selection and handles;
   - pool and lane headers;
   - pins.

   Export outputs must stay byte-identical.
4. **Bottom panel:** Problems, Comments and Revisions tabs. The comment and checkpoint windows are
   removed.
5. **Inspector:**
   - the header card;
   - `Section`-based groups;
   - the Data & collaboration section;
   - the Settings dialog for local automation.
6. **Rail and Library:** the three-place rail and the Library details pane.
7. **Polish:**
   - empty states with an action;
   - palette and type icons;
   - shortcuts in tooltips;
   - spacing consistency.

Evidence goes in `docs/implementation/ux-shell-evidence.md`.

## References

- SAP Signavio editor and dictionary:
  https://learning.sap.com/learning-journeys/design-business-processes-with-sap-signavio-solutions/understanding-the-dictionary_d73aabde-688f-4971-8fa4-7d4426f76a8b
- Camunda's simplified modeling canvas: https://camunda.com/blog/2024/02/model-faster-simplified-modeling-canvas/
- The bpmn.io properties panel: https://bpmn.io/blog/posts/2022-new-properties-panel-foundation
- diagram-js CSS (the reference values): https://cdn.jsdelivr.net/npm/diagram-js@15.2.4/assets/diagram-js.css
- Figma UI3: https://www.figma.com/blog/behind-our-redesign-ui3/
- The Linear redesign: https://linear.app/now/how-we-redesigned-the-linear-ui
- craft-libs `crates/ui` README and `docs/theme-infrastructure.md`
- The WordCraft and VectorCraft `crates/ui-egui/src/theme.rs` files
- `assets/app-icon/README.md` (the ProcessCraft palette)
