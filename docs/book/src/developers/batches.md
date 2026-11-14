# Batches and the command pipeline

Every persistent change to a project, from the canvas, the CLI or MCP, is a typed command applied by
one `Session` in `processcraft-core`. The GUI and the CLI hold no modeling rules of their own, so a
batch that works in one works in the others.

## A batch

A request is a JSON object. The one you will use most is a batch of mutations:

```json
{
  "type": "batch",
  "mutations": [
    {"type": "createDocument", "document": {"id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "name": "Invoice approval", "folder": null, "definitions_id": "defs", "processes": [], "collaboration": null, "diagrams": [], "provenance": null, "capability": "Editable"}},
    {"type": "createProcess", "document_id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "process": {"id": "invoice", "name": "Invoice approval", "documentation": "", "nodes": [], "lanes": [], "flows": []}},
    {"type": "createDiagram", "document_id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "diagram": {"id": "invoice_diagram", "plane_id": "invoice_plane", "plane": "invoice", "shapes": [], "edges": []}},
    {"type": "createAt", "document_id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "diagram_id": "invoice_diagram", "kind": "StartEvent", "name": "Invoice received", "center": {"x": 100, "y": 120}, "node_id": "start", "shape_id": "start_shape"},
    {"type": "append", "document_id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "diagram_id": "invoice_diagram", "source_id": "start", "kind": "UserTask", "name": "Check invoice", "node_id": "check", "shape_id": "check_shape", "flow_id": "f1", "edge_id": "f1_edge"},
    {"type": "append", "document_id": "5b1f0c3e-8d2a-4c47-9a10-3f6e2b7d9c01", "diagram_id": "invoice_diagram", "source_id": "check", "kind": "EndEvent", "name": "Invoice paid", "node_id": "end", "shape_id": "end_shape", "flow_id": "f2", "edge_id": "f2_edge"}
  ]
}
```

Save it as `invoice.json` and run
`processcraft-cli run --new Invoices --batch invoice.json --out invoices.processcraft`.

- `type` names the command; the other fields are its parameters in `snake_case`. Unknown fields are
  refused.
- The caller chooses new IDs. Documents, dictionary entries and attribute definitions use UUIDs;
  BPMN elements, flows, shapes and edges use XML IDs (letters, digits, `_` and `-`, starting with a
  letter or `_`).
- Build diagrams with the modeling commands, not coordinates: `createAt` for the first element,
  `append` for each next one, `connect` to join branches. The core places shapes and routes flows.
  `createShape`, `createEdge` and `moveShape` exist for repairing a diagram.

[Reference: batch commands](../reference/commands.md) lists all of them, and `commands --json` prints
the full JSON Schema.

## What the session guarantees

- **All or nothing.** If any mutation fails, nothing in the batch is applied, and the project,
  history and generation are unchanged.
- **One undo step** per batch.
- **At most 1,000 mutations** per batch. Undo, redo, saving and exporting are not batch members.
- **Valid by construction.** Structural invariants are checked before a batch commits, and modeling
  commands follow the BPMN connection rules the canvas uses.

## Generation, content identity and checkpoints

Three separate notions:

- **Generation** is a counter that grows with every change, including undo and redo. Automated
  clients send the generation they last saw as `expectedGeneration`; if the project moved in the
  meantime, the request is refused with a conflict instead of applying to a state the client has not
  seen.
- **Content identity** is a SHA-256 of the canonical persistent content. A project is dirty when it
  differs from the identity of the last save, so undoing back to the saved content makes it clean
  again.
- **Checkpoints** are named, persistent revisions stored in the file. They are unrelated to undo.

## Request IDs over MCP

The MCP `apply` tool wraps a request in an envelope with `sessionId`, `requestId` and
`expectedGeneration`. The same `requestId` with the same payload returns the stored result without
applying it again, so retries are safe; the same `requestId` with a different payload is refused.
See [The MCP server](../agents/mcp.md).
