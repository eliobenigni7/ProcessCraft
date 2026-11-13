---
name: processcraft-text-to-model
description: Turn a written description of a business process into a new ProcessCraft model through the local MCP server, checking it against the modeling conventions. Use when the user gives a process in prose and wants a diagram-backed model, not when an existing model must be changed.
---

# Text to model

Read [../README.md](../README.md) first: tools, change protocol, ID rules and the global must-nots.
Fenced `json` blocks below carry an info string (`tool=apply`, `tool=query`); the pack's test
validates each one against the live schemas.

## Inputs

- The process description (prose). If it leaves a branch condition, an actor or an end state open,
  ask. If nobody can answer (an autonomous run), build only what the text decides and list each
  open point in the reply, see the README; do not invent it.
- A running `processcraft-cli mcp` session, usually `--new "<name>"` with a `--write-root` when the
  result must be saved. A project that already contains documents is not the target of this prompt:
  use [review-conventions](../review-conventions/SKILL.md) or add the new document beside them only
  if the user said so.

## Tool sequence

1. `inspect`: note `sessionId`, `generation` and `documents` (0 for a new project).
2. `query` `dictionary`: see which roles and systems exist. Reuse them; create a new dictionary
   entry only for an actor the description names (see "Actors and lanes").
3. `apply` one batch that builds the whole model, with `expectedGeneration` = the generation from
   step 1. Order inside the batch: `createDocument`, `createProcess`, `createDiagram` (empty
   `shapes` and `edges`), `createAt` for the start event, then `append` for each element after its
   predecessor, `connect` for a flow that joins an existing element, and `rename` on a flow to name
   it. After an `append` the new element is the source of the next one.
4. `apply` a second batch with `setConventionsProfile` (the recommended profile below) when the
   user asked for convention checking, using the generation returned by step 3.
5. `query` `diagnostics`, then `validate`. For each finding, fix it with a further `apply` only when
   the description decides the answer (a missing label, a gateway flow without a name). Otherwise
   report it.
6. `save` when the user named a path inside the write root.

Build elements in reading order, one `append` per element. Kinds you will use: `StartEvent`,
`EndEvent`, `UserTask`, `ServiceTask`, `ManualTask`, `Task`, `ExclusiveGateway` (a decision),
`ParallelGateway` (steps that happen at the same time, and the point where they must all be done).

- An exclusive (or inclusive) gateway that splits needs a label on every outgoing flow, except a
  declared default flow. A parallel gateway needs no flow labels: the convention checks only
  exclusive and inclusive splits. Branches that came from a parallel split meet again at a parallel
  gateway ("both ready"); branches of a decision meet at an exclusive gateway or at the end. A task
  with two incoming flows is an implicit join and is reported by the conventions.
- Write each activity label as a verb followed by an object ("Create employee record"). The
  conventions check the first word against a built-in English verb list: a label the description
  forces into another shape, or a verb the list lacks, is reported by the check; keep the
  description's meaning and report the finding rather than bend the label.
- One end event. When a branch ends in the description, route it to the end event; several
  outcomes may share one end event (`connect` to it). If the description does not say what
  happens after a step on a branch, connect that step to the existing end event and list the open
  point in the reply. Do not add a second end event or a loop the text does not state.

## Actors and lanes

When the description names who does a step ("HR", "the manager", "IT"), put each actor in a lane,
linked to a dictionary `Role` entry, so that the table and RACI queries can show it. Create the
entries and lanes in a batch after the elements exist, because a lane lists its members by node
ID (one `createDictionaryEntry` with a fresh UUID per actor, one `createLane` per actor, one
`setDictionaryLinks` of kind `Role` per lane). A node belongs to one lane. An actor the
description does not name is not invented: leave the step out of every lane and report it. Reuse
an entry from `query dictionary` instead of creating a second one with the same name.

```json tool=request
{
  "type": "batch",
  "mutations": [
    {"type": "createDictionaryEntry", "entry": {"id": "aaaaaaaa-0000-4000-8000-0000000000a1", "kind": "Role", "name": "Accounts payable", "description": ""}},
    {"type": "createLane", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "process_id": "invoice", "lane": {"id": "lane_ap", "name": "Accounts payable", "parent": null, "members": ["check", "big", "approve", "join", "pay"]}},
    {"type": "setDictionaryLinks", "source": {"document_id": "aaaaaaaa-0000-4000-8000-000000000001", "xml_id": "lane_ap"}, "kind": "Role", "targets": [{"Entry": "aaaaaaaa-0000-4000-8000-0000000000a1"}]}
  ]
}
```

This batch is the second one after the worked example below (generation 1 to 2, or put its three
mutations at the end of the first batch). Start and end events may stay outside lanes.

## Worked example

Description: "An invoice arrives and is checked. If the amount is above 1000 it needs approval;
otherwise it goes straight on. Then it is paid and the process ends."

Choose a fresh UUID for the document and keep readable XML IDs for everything else.

```json tool=apply
{
  "sessionId": "00000000-0000-4000-8000-000000000000",
  "requestId": "invoice-model-1",
  "expectedGeneration": 0,
  "request": {
    "type": "batch",
    "mutations": [
      {"type": "createDocument", "document": {"id": "aaaaaaaa-0000-4000-8000-000000000001", "name": "Invoice approval", "folder": null, "definitions_id": "defs", "processes": [], "collaboration": null, "diagrams": [], "provenance": null, "capability": "Editable"}},
      {"type": "createProcess", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "process": {"id": "invoice", "name": "Invoice approval", "documentation": "", "nodes": [], "lanes": [], "flows": []}},
      {"type": "createDiagram", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram": {"id": "invoice_diagram", "plane_id": "invoice_plane", "plane": "invoice", "shapes": [], "edges": []}},
      {"type": "createAt", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "kind": "StartEvent", "name": "Invoice received", "center": {"x": 100, "y": 120}, "node_id": "start", "shape_id": "start_shape"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "start", "kind": "UserTask", "name": "Check invoice", "node_id": "check", "shape_id": "check_shape", "flow_id": "f_start_check", "edge_id": "f_start_check_edge"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "check", "kind": "ExclusiveGateway", "name": "Amount above 1000?", "node_id": "big", "shape_id": "big_shape", "flow_id": "f_check_big", "edge_id": "f_check_big_edge"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "big", "kind": "UserTask", "name": "Approve invoice", "node_id": "approve", "shape_id": "approve_shape", "flow_id": "f_big_approve", "edge_id": "f_big_approve_edge"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "approve", "kind": "ExclusiveGateway", "name": "Routes joined", "node_id": "join", "shape_id": "join_shape", "flow_id": "f_approve_join", "edge_id": "f_approve_join_edge"},
      {"type": "connect", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "big", "target_id": "join", "flow_id": "f_big_join", "edge_id": "f_big_join_edge"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "join", "kind": "ServiceTask", "name": "Pay invoice", "node_id": "pay", "shape_id": "pay_shape", "flow_id": "f_join_pay", "edge_id": "f_join_pay_edge"},
      {"type": "append", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "diagram_id": "invoice_diagram", "source_id": "pay", "kind": "EndEvent", "name": "Invoice paid", "node_id": "end", "shape_id": "end_shape", "flow_id": "f_pay_end", "edge_id": "f_pay_end_edge"},
      {"type": "rename", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "element_id": "f_big_approve", "name": "Yes"},
      {"type": "rename", "document_id": "aaaaaaaa-0000-4000-8000-000000000001", "element_id": "f_big_join", "name": "No"}
    ]
  }
}
```

In a real run replace `sessionId` with the value from `inspect`. The reply looks like this (a `contentId` field is omitted here):

```json output
{"ok": true, "requestId": "invoice-model-1", "generation": 1, "changed": true, "dirty": true}
```

The recommended profile (all eight rules as warnings) is the `request` of the second batch:

```json tool=request
{
  "type": "batch",
  "mutations": [
    {"type": "setConventionsProfile", "profile": {"rules": [
      {"type": "labelRequired", "severity": "warning", "kinds": ["Activity", "Event", "Lane", "Participant"]},
      {"type": "activityVerbObject", "severity": "warning", "verbs": []},
      {"type": "gatewayFlowsLabelled", "severity": "warning"},
      {"type": "noImplicitSplitJoin", "severity": "warning"},
      {"type": "singleStartEnd", "severity": "warning"},
      {"type": "maxElements", "severity": "warning", "limit": 50},
      {"type": "requiredAttributes", "severity": "warning"},
      {"type": "dictionaryTerms", "severity": "warning"}
    ]}}
  ]
}
```

Then read the findings:

```json tool=query
{"kind": "diagnostics", "offset": 0, "limit": 50}
```

An empty `items` list with `total` 0 means no finding under the stored profile. It does not mean the
model is complete or conformant. If no profile is stored, only the built-in advisory findings can
appear, so an empty list there is weaker evidence still.

## On GenerationConflict

Nothing was applied. `inspect`, then `query diagnostics` and `query dictionary` again, check whether
the document you are building already exists (someone may have created it), rebuild the batch
from what is there, and resend under a new `requestId`. If the document already exists with the
same content, do not create a second one.

## Must not

- Invent actors, conditions, steps or end events the description does not contain; ask, or in an
  autonomous run list the open point in the reply.
- Place shapes with coordinates beyond the first `createAt`; use `append` and `connect`.
- Reuse an element ID inside the document, or guess that an ID is free in a project that already has
  content: `query` first.
- Leave a gateway branch unlabelled or a merge implicit to make the model "look" shorter.
- Say the result is valid BPMN, conformant or certified. Say what `validate` and the diagnostics
  returned, and that they are advisory.
