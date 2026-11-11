# ProcessCraft UI production polish

**Goal:** Integrate the requested native table/tree crates on the pinned Craft fork and refine the approved FigCraft workbench in the actual Rust desktop app.

**Architecture:** Keep the existing domain commands, stable IDs, save/recovery/effect boundaries and shared BPMN renderer. UI crates own presentation state only. English Light/Dark, offline runtime and explicit automation grants remain requirements. The existing checkout has no Git metadata; use it without initializing Git or creating a worktree.

## Work

- [x] Pin `egui_extras = 0.37.0` and `egui_ltreeview = 0.10.0` without default features. Resolve the lockfile; verify that both compile against the single Craft egui revision. Do not enable loaders, network or persistence features.
- [x] Replace the three custom data grids with TableBuilder, fixed headers, clipped/resizable columns, virtualized rows, accessible selection and stable model IDs. Keep all draft and command semantics.
- [x] Replace the flat project list with a real folder/document tree, expansion, keyboard selection and ancestor-preserving search. Bound hierarchy traversal and synchronize rejected/stale selections with the actual session.
- [x] Polish workbench density, pane boundaries, active-document tabs, context menus, focus/disabled states, empty states, form labels and canvas loading/empty states against the approved visual target. Do not add mock product features.
- [x] Verify actual native views in both themes, resize/high-DPI behavior, tree selection/search, table resizing/selection, dictionary draft navigation, undo and save. Run the workspace suite, Clippy, formatting and release build with the locked offline graph.
- [x] Add an explicit future M3 auto-layout slice with scope, geometry/command boundaries, stale-result refusal, preview/apply/undo and deterministic fixtures; do not implement or expose an inactive button now.

Root owns Cargo, shell, theme/widgets/inspector/canvas integration and native QA. Terra workers separately own table views, the new tree module and the M3 plan. Record concrete compatibility and verification results in `docs/implementation/ui-production-evidence.md`.

Completed: see `docs/implementation/ui-production-evidence.md` for the final compatibility graph, 226 passing tests and native checks. M3.0 remains planned work.
