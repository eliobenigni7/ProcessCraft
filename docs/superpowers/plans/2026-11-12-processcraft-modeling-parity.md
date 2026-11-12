# ProcessCraft modeling parity with bpmn-js

Goal: bring canvas modeling up to the interaction level of bpmn-js/diagram-js (bpmn.io), re-implemented in Rust. Reference: bpmn-js `546a069` and diagram-js at the same date, read as behavior specifications. bpmn-js is under the bpmn.io license, so no code is copied; behaviors, rules, and constants are re-implemented.

## Architectural rule

Modeling rules and compound modeling operations live in `processcraft-core`, so the GUI, CLI, and MCP behave identically (solution design §4, §7). The GUI owns gestures, previews, the context pad, popup menus, and snap guides only. Placement and connection logic currently in `apps/processcraft/src/app.rs` (`add_node`, `connect_elements`) moves to core as request builders that return `Vec<Mutation>`.

## Gap summary (2026-11-12)

| Area | Have | Missing |
|---|---|---|
| Rules | Post-hoc validation only | `can_connect`, `can_drop`, `can_attach`, `can_resize`, `can_insert`, `can_copy` before mutation |
| Create | Palette click, append-right with fixed +80 gap | Context pad, bpmn-js auto-place, drag-create with preview, bpmn-js default sizes |
| Connect | Drag handle, Sequence/Association only, same scope | Automatic Message/Data association selection, reconnect, bendpoints, global connect |
| Replace | None (`NodeKind` immutable) | `ReplaceElement` mutation and option table, default/conditional flow toggle |
| Move/resize | Free move, corner resize on any shape | Snapping, grid, boundary events following their host, attach/detach, drop-on-flow, resize rules and minimum sizes, auto-resize, space tool |
| Routing | Facing-side elbow + bounded obstacle search | Per-type preferred Manhattan layouts (gateway v:h/h:v, loops, boundary, message flows) |
| Containers | Create pool/lane | Lane add above/below, split, sibling-aware resize/delete, wrap content on create pool, collapse toggle |
| Productivity | Box select, undo/redo, Delete, arrows | Select all, copy/cut/paste/duplicate, align/distribute, canvas search, bpmn-js keyboard tools |

## Rule and algorithm reference

- **can_connect** (BpmnRules): message flow between different organisations; sequence flow within one scope, with source/target exclusions (end event, start/boundary target, event subprocess, link events, compensation); event-based gateway targets limited to receive task or message/timer/conditional/signal catch events; data input/output associations between data references and activities/events; compensation association from a compensation boundary event; plain association when one end is a text annotation.
- **can_drop / can_create / can_insert / can_attach / can_resize / can_copy**: as in BpmnRules. Resize is limited to expanded subprocesses (min 140×120), lanes (300×60), participants (300×150), groups (140×120), and text annotations (100×40, east/west only).
- **Replace options**: event families by position (start, event-subprocess start incl. non-interrupting, intermediate, end, boundary), gateways, task types and subprocess variants, data object ↔ data store, sequence flow plain/default/conditional, participant expanded/collapsed. A replace keeps id, name, documentation, flows, and DI, removes flows that `can_connect` no longer allows, and refuses when kind-specific details cannot be kept.
- **Auto-place**: horizontal append at `source.right + d`, with `d` learned from existing neighbour gaps (0–250, default 50); boundary sources offset ±80; collision step along y with margin 30 and minimum distance 80. Annotations go above-right (−50); data objects go below-right (+40).
- **Default sizes**: task 100×80, event 36×36, gateway 50×50, expanded subprocess 350×200, collapsed 100×80, data object 36×50, data store 50×50, annotation 100×30, external label 90×20 centred 10 px below.
- **Preferred Manhattan layouts**: sequence flow h:h, out of a gateway v:h, into a gateway h:v, self-loops by side, message flows straight then v:v, boundary flows leave from the attach side; minimum segment 20.
- **Behaviors**: drop-on-flow insert, connection replacement after move, attach/detach events, boundary events moving with their host, lane add/split/resize/delete (indentation 30, new lane 120), auto-resize containers (grow 60/100), wrap content in a new participant (padding 20 + 30 header), toggle collapse.
- **Keyboard**: A select all (Ctrl+A), C global connect, E edit label, R replace, S space tool, L lasso, H hand, Ctrl+C/X/V/D, Ctrl+F search; arrow move 1 px, Shift 10 px.

## Phases

| Phase | Content | Core | GUI | Size |
|---|---|---|---|---|
| 1 | Rules + correct connect | `rules.rs`; connect builder choosing the flow kind; queries for CLI/MCP | allowed/forbidden feedback while connecting; cross-pool message flows | M |
| 2 | Append + auto-place + context pad | default sizes, `auto_place`, append builder, preferred layouts in `connection_route` | context pad around the selection (append task/gateway/intermediate/end/annotation, connect, replace, delete) | M |
| 3 | Replace / morph | `ReplaceElement` mutation, option table, connection cleanup, default/conditional flow | replace popup (R, context pad) | M |
| 4 | Direct manipulation fidelity | boundary follows host, attach/detach, drop-on-flow, reparent on drop, resize rules | snapping guides, grid, bendpoints, endpoint reconnect, drag-create preview | L |
| 5 | Pools, lanes, containers | lane handlers, auto-resize, wrap participant, collapse toggle, space tool | lane actions in the pad, space-tool gesture | L |
| 6 | Selection productivity | clipboard with ID remapping, align/distribute | select all, shortcuts, canvas search, auto-scroll | M |

Each phase passes the three workspace gates, adds focused tests (core rule tables are unit-tested from the lists above), exposes core behavior through the shared command schema, and records results in the implementation evidence.
