# Native UI alignment evidence

November 11, 2026. Implements the existing FigCraft visual direction before M3, in the native Rust desktop application. Terra workers handled theme/assets, data views, canvas extraction, property controls, independent review and crate research.

## Implemented

- File/Edit/View/Model/Tools menus, compact primary toolbar, document tabs and navigation rail. Save state and the local filename live in the footer; full paths remain available in tooltips. Automation controls and credentials are under Tools / Local automation, with the original explicit enable and folder grants.
- Project navigation and search, folder filtering with All models, a compact BPMN palette and collapsible outline. The editor has model breadcrumbs/title, actual capability, Model/Table navigation, centered initial fit, an artboard and a structured Properties pane. Advanced editing and connection commands remain available in expandable sections.
- Library, Dictionary and Activity table use full-row selection, readable columns and contextual details. Dictionary retains explicit Save/Discard and linked usages. Table includes a preview rendered from the actual current document with a separate camera.
- English Light/Dark themes, bundled Noto fonts and source Lucide icons. Default BPMN tasks/events use semantic theme colors; explicit document styles retain precedence. Source artwork and ISC/OFL licenses are retained. The PNG codec was already in the dependency graph and is now a direct desktop dependency for embedded icons; no SVG runtime was added.
- Below 980 logical pixels, project/properties move to accessible windows. Data views stack and scroll. The toolbar reduces at small widths; menus retain the commands.

## Checks

`cargo test --workspace --all-targets --all-features --locked --offline` passed **219 tests**. The new workbench regression renders all four views in both themes at 320, 720 and 1440 logical pixels and checks that navigation cannot discard a dictionary draft or mutate document content. Existing command, drag/escape, recovery, cancellation, save and attachment tests remain green. The renderer regression verifies semantic defaults and explicit style precedence.

Workspace all-target/all-feature Clippy with `-D warnings`, formatting, and release desktop/CLI builds passed. Logs and native screenshots are under `target/ui-alignment/`.

The native release was exercised with the M2 procurement fixture using Windows UI Automation and PrintWindow capture. Editing a dictionary entry then clicking Table kept the dictionary draft visible and blocked navigation; Discard restored it. Renaming an activity through Table changed the visible row; Undo restored its original name. Native Save As wrote `native-ui-smoke.processcraft`, which the CLI validated. Its ProjectId and content identity exactly match the original after Undo.

Native captures cover all four views in both themes. The 200% Properties window is bounded to the viewport; wheel scrolling brings Apply properties and the advanced controls into view (`properties-200-scrolled.png`).

Independent static review found and corrected inconsistent narrow-panel thresholds and navigation guards. No further concrete regression was found in the reviewed paths. Narrator was never launched; this records UIA labels and keyboard-capable widgets, not a screen-reader certification.

## Crate research

See [Rust UI crate research](../research/rust-ui-crates.md). Existing egui row virtualization is the first option. `egui_extras` and `egui_ltreeview` are candidates for advanced tables/tree behavior; neither was added. Version requirements alone do not prove compatibility with the pinned Craft egui fork.
