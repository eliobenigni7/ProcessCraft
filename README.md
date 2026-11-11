# ProcessCraft

An offline desktop workbench for process analysts. The agreed direction is a native Rust application following the Craft Apps philosophy, with Signavio as the primary modeling reference. Product copy, design tokens, component names, and documentation use English.

## Architecture

[Architectural solution design](docs/superpowers/specs/2026-11-11-processcraft-solution-design.md): source comparison across seven Craft applications, domain and command contracts, native format, interoperability, rendering, automation, verification, and delivery gates.

[Implementation plan](docs/superpowers/plans/2026-11-11-processcraft-implementation-plan.md): M0 and M1 are implemented. The [M2 plan](docs/superpowers/plans/2026-11-11-processcraft-m2-implementation-plan.md) implements library, dictionary, table, qualified interchange, image export and local automation. See the [M2 evidence](docs/implementation/m2-evidence.md) for checks and limits. The [expanded M3 plan](docs/superpowers/plans/2026-11-11-processcraft-m3-implementation-plan.md) now has a partial implementation; the user requested a temporary stop at this delivery. See [M3 evidence and remaining scope](docs/implementation/m3-evidence.md). M4 remains planned.

## Design v0.1 · November 11, 2026

- [Editable FigCraft file](design/ProcessCraft.figcraft)
- [Eight design boards in PDF](design/ProcessCraft-boards.pdf)
- [PNG previews](design/previews/)

**01 · Workbench** contains four 1600 × 1000 views: BPMN editor, local process library, project dictionary, and table view. The top row uses the light theme; the bottom row uses the dark theme. **02 · Components** contains controls, BPMN elements, icons, and semantic colors.

Both themes share the **ProcessCraft** variable collection, with **Light** and **Dark** modes. Icons and text bind to semantic colors. Buttons and BPMN tasks are reusable components with text properties. The title bar stays dark in both modes.

To explore the prototype in FigCraft, select a board and use **Present**. Navigation covers the side rail, opening a model from the library, switching between model and table views, and opening the dictionary from task properties. Navigation preserves the selected theme.

This FigCraft file is a navigable visual prototype and design reference, separate from the Rust application and its verification. Acme project data is fictional.

## Implementation direction

Rust with a native egui interface, a UI-independent engine, and shared UI/CLI/MCP commands. The native `.processcraft` format is ZIP + versioned JSON and resources. SQLite is optional only for a demonstrated internal need. Local files and dictionary; no account required for core operations. Atomic saves, recovery, and undo are requirements.

The native workbench runs with `cargo run --release -p processcraft`. Editor, Library, Dictionary and Table share one project session and command engine. M2 adds folders/filtering, linked role/system/document entries and usages, table editing, process/diagram selection, BPMN import/export, SVG/PNG export and explicit local MCP attachment. Themes, panel sizes, filtering and UI scaling remain view state.

The desktop now uses pinned `egui_extras` tables with resizable columns, fixed headers and virtualized rows, and `egui_ltreeview` for the folder/model hierarchy. Both use the same pinned Craft egui fork. The UI includes guarded closable document tabs, resizable panes, explicit keyboard focus, English Light/Dark styling and bounded narrow-screen property windows. See [UI production evidence](docs/implementation/ui-production-evidence.md) for compatibility and native checks. [M3.0 auto-layout](docs/superpowers/plans/2026-11-11-processcraft-m3-auto-layout.md) now provides bounded deterministic preview, cancellation, stale-safe apply and Undo/Redo for its qualified profile.

Native schema 3 uses bounded ZIP/JSON, local comments and separately stored checkpoints with shared resource blobs. Schema 0–2 projects are verified before migration and require Save As. The partial M3 delivery adds local comments, frozen checkpoints/reference viewing, detailed comparison/restore, PDF export and broader typed BPMN profiles. PDF uses outlined text with explicit notices and acknowledgement. Interchange now covers tested profiles for nested scopes, selected events/task/loop/gateway metadata, conversations, choreography tasks, data/I/O, annotations, groups and nested lanes. Advanced or unqualified content is preserved or refused; this is not complete BPMN 2.0.2 coverage or an OMG conformance claim. Installer and platform expansion remain M4. Writable storage remains qualified fixed local Windows NTFS.

```powershell
cargo run --release -p processcraft
cargo run -p processcraft-cli -- commands --json
cargo run -p processcraft-cli -- run --new Procurement --batch tests/fixtures/commands/procurement.json --out target/procurement.processcraft
cargo run -p processcraft-cli -- run --in target/procurement.processcraft --batch tests/fixtures/commands/m2-library-dictionary.json --out target/procurement-m2.processcraft
cargo run -p processcraft-cli -- query target/procurement-m2.processcraft dictionary
cargo run --release -p processcraft-cli --example m2_qualify -- --out target/m2-performance
cargo test --workspace --all-targets --all-features --locked --offline
```

MCP starts only through `processcraft-cli mcp`. Standalone mode takes `--new NAME` or `--in PATH`, with explicit absolute `--read-root` / `--write-root` folders. The GUI's **Enable attachment** shows endpoint, session and token; `mcp --attach ENDPOINT --session SESSION --token TOKEN` edits that visible session. File effects require **Authorize automation write folder…** in the GUI; CLI arguments cannot extend those grants. Disable or changing project revokes the endpoint. No listener starts by default and no account/provider is needed. Keep tokens private.

The historical M2 medium benchmark measured 5,000 nodes and 10,100 dictionary links: table p95 22 ms and save p95 950 ms; rename remains about 134 ms against a 20 ms target. These are M2 measurements, not performance qualification of the expanded M3 model. See the evidence for the measured disposition and other limits, including bounded request ledgers and GUI reconciliation for uncertain attached exports. Dependency downloads are needed on the first build. The `test-fault` feature is a debug-only test seam, absent from normal builds and compiled out of release builds.

## Reproducing the design

`design/workbench.commands.json` is the authoring source for the installed FigCraft CLI:

```powershell
figcraft-cli run .\design\workbench.commands.json --out .\design\ProcessCraft.figcraft
figcraft-cli info .\design\ProcessCraft.figcraft --json
```

The batch records this checkout's absolute paths. After moving the project, update the `E:/Documenti/GitHub/ProcessCraft` prefix in the JSON before regenerating. Regeneration replaces the exports and does not incorporate later manual edits to the FigCraft file.

## References and attribution

- [ArtCraft principles](https://getartcraft.com/) and [FigCraft architecture](https://github.com/storytold/figcraft/blob/main/docs/architecture.md).
- [SAP Signavio Process Modeler](https://www.signavio.com/products/process-modeler/) and [new canvas](https://www.signavio.com/post/introducing-the-new-sap-signavio-process-modeler-canvas/).
- [QuickModel](https://help.sap.com/docs/signavio-process-manager/user-guide/quickmodel-editor?locale=en-US).
- [Lucide](https://github.com/lucide-icons/lucide) icons, ISC license retained in [design/assets/lucide/LICENSE](design/assets/lucide/LICENSE). BPMN symbols are editable geometry; no reference screenshots are embedded in the design.

Validation: reopened the native file, exported and visually inspected all eight boards, checked prototype destinations across both themes, and confirmed no missing fonts. These checks do not test a Rust application.

