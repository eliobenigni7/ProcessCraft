# Architecture

```text
apps/processcraft      desktop app (egui)  ─┐
apps/processcraft-cli  CLI + MCP server     ─┼─▶ processcraft-io ─────┐
                                             └─▶ processcraft-render ─┴─▶ processcraft-core
```

| Crate | Job |
|---|---|
| `processcraft-core` | Domain model, typed commands with undo and redo, BPMN modeling rules, validation, conventions, attributes, queries, geometry, routing and auto-layout. No file system, UI or network. |
| `processcraft-io` | The `.processcraft` format, BPMN XML import and export, atomic save, recovery, file grants, local IPC for MCP attachment. No modeling rules. |
| `processcraft-render` | A toolkit-free scene, text layout with bundled fonts, a CPU rasterizer, SVG, PNG and PDF export. Receives snapshots and never writes projects. |
| `processcraft` | The desktop app: canvas gestures, context pad, panels, menus, background jobs. |
| `processcraft-cli` | Argument parsing, JSON batches, the MCP server. |

`processcraft-io` and `processcraft-render` both depend on the core and never on each other.

## Principles

- **One command pipeline.** Every persistent change goes through `processcraft_core::commands::Session`.
  Panels and canvas gestures build mutations and submit them; they do not change the model directly.
  See [Batches and the command pipeline](batches.md).
- **Snapshots, not locks.** State is shared copy-on-write. Rendering, queries and background workers
  get immutable snapshots, and results for an older generation are dropped.
- **Nothing lost silently.** BPMN content outside the supported profile is preserved byte for byte or
  refused, never dropped or reinterpreted.
- **Bounded input.** Parsers and archive readers enforce size and count limits; XML parsing rejects
  DTDs and external entities and never loads remote resources.
- **No unsafe code**, except in a few reviewed OS adapters in `processcraft-io`.
- **Nothing listens by default.** MCP over stdio is started by the user; attachment to the desktop app
  is switched on in the app and limited to folders the user authorizes.

## Further reading

- [The solution design](https://github.com/eliobenigni7/ProcessCraft/blob/main/docs/design/solution-design.md):
  the architectural source of truth.
- [Evidence files](https://github.com/eliobenigni7/ProcessCraft/tree/main/docs/implementation):
  what was verified for each milestone, with measured limits.
- [BPMN coverage](https://github.com/eliobenigni7/ProcessCraft/blob/main/docs/research/bpmn-2-coverage.md):
  element-by-element support.
- [Roadmap](https://github.com/eliobenigni7/ProcessCraft/blob/main/docs/plans/roadmap.md).
