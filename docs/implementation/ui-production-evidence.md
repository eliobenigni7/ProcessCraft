# Native table/tree integration and production UI polish

November 11, 2026. This implements the requested UI crates and refines the existing FigCraft workbench direction. The domain, portable format, command dispatcher, renderer and opt-in automation boundary remain shared.

## Compatibility verified

Desktop dependencies are exact `egui_extras = 0.37.0` and `egui_ltreeview = 0.10.0`, both with default features disabled. Cargo adds only these packages and `enum-map` / `enum-map-derive`; no loader, HTTP or persistence feature is enabled for the UI crates.

Both compile unmodified against the existing StoryTold egui revision `6d518fc799681571963842f95b55a6b68431b15a`. `cargo tree -p processcraft -i egui --locked --offline` confirms one egui instance shared by eframe, both UI crates and ProcessCraft. The resolved graph is recorded in `target/ui-production/egui-compatibility.txt`. Locked offline checks/builds use the downloaded graph; an uncached first build still needs dependency downloads.

## Visible behavior

- Library, Dictionary and Activity table use actual `TableBuilder` instances, virtualized equal-height rows, fixed table headers, clipped/resizable columns, continuous header surfaces and selected rows. Name cells have stable model-key IDs and visible keyboard focus; filtering does not transfer activation to another entry.
- The project tree uses actual `TreeView` hierarchy and expansion with document/folder IDs. Search retains matching ancestors, deep display traversal is bounded without hiding models, long names truncate with full tooltips, and current document ancestry opens when needed. Row controls are named and focusable. Left/right expands/collapses; up/down selects through the same guarded navigation used by clicks.
- Tabs close without deleting documents and refuse to hide unresolved dictionary drafts. Resizable panes retain stable egui identities while their geometry changes. Save is labelled, Undo/Redo availability reflects history/drafts, Model/Table has a grouped switch, capability explanations are available, and form/empty/loading states use real session state.
- The existing English Light/Dark palette, bundled fonts and Lucide assets cover the shell, icon buttons and native application icon. The property inspector has a separate type/title/field hierarchy, optional-field hints and disabled Apply when unchanged. Narrow layouts stack details in the page scroll; wide detail panes scroll independently. Floating panels are bounded to the viewport.

## Automated checks

The complete locked offline workspace suite passes **226 tests**. Coverage includes table activation/focus after filtering, tree hierarchy/search/deep display, full-workbench arrow navigation across idle frames, draft-preserving navigation/tab closure, and stacked data views at 400 × 250. Existing drag/escape, save/recovery, command, format, CLI and attachment tests remain green.

Formatting, workspace all-target/all-feature Clippy with `-D warnings`, and release desktop/CLI builds pass. Final logs are in `target/ui-production/workspace-tests.log`.

## Native Windows checks

The native release was exercised with Windows UI Automation, real pointer/keyboard input and PrintWindow captures. `operations-ui.processcraft` is a synthetic four-model project made through the existing CLI/command engine; it is qualification data, not a default or fabricated product status.

- Tree root collapse/expand, then consecutive down-arrow navigation root → folder → model, with idle frames between steps. Search-field caret keys retain edit focus and do not navigate the tree.
- A column boundary moved 70 physical pixels; the project pane moved 48 pixels. The visible table and tree updated, and the model remained intact.
- Closing the only open tab left all four models in Library. Reopening returned to the same document.
- Editing a dictionary draft then navigating to Table was refused; explicit Discard restored it. Activity rename changed the real table row; Undo restored it.
- Native Save As wrote `native-final-smoke.processcraft`. The CLI confirms the saved ProjectId and content identity exactly match the source after Undo, and the bounded native reader validates the file.
- Light/Dark captures cover the four views. At 200%, the property window stays bounded and scroll reaches Apply/Advanced/Connections. Narrow Library/Dictionary details use the page scroll so their actions remain reachable.

Native QA found and fixed two adapter/layout issues that isolated component tests missed: stale focus restoration between tree rows, and nested scrolling in stacked details. Regression checks cover the complete workbench and narrow page content. Independent Terra reviews found no further concrete blocker in the examined paths.

`egui_ltreeview` 0.10 does not supply native AccessKit TreeItem semantics. The app exposes named egui row controls and keyboard navigation; this is not full screen-reader qualification. Narrator was never launched, as requested.

## M3 follow-on

[M3.0 auto-layout](../plans/m3-auto-layout.md) is now ordered before comments/checkpoints in the main plan. It specifies a bounded deterministic offline algorithm, a supported single-pool/flat-lane profile, explicit preview/Apply/Cancel, stale-session/generation rejection, exact undo/redo and semantic/style preservation through the shared engine. It is planned work; no inactive auto-layout button or command is exposed.
