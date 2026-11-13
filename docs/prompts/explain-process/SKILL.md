---
name: processcraft-explain-process
description: Explain an existing ProcessCraft process in plain language (steps, decisions, who does what, attributes, open findings) from the local MCP server, without changing the model. Use when the user asks what a process does, who is responsible for a step, or for a summary for a reader who has not seen the diagram.
---

# Explain a process

Read [../README.md](../README.md) first: tools, change protocol, ID rules and the global must-nots.
This prompt is read only: it never calls `apply`, `save` or `export`. Fenced `json` blocks below
carry an info string; the pack's test validates each one against the live schemas.

## Inputs

- A session on the project (`--in <file>`, or attached to the desktop app).
- The document ID and the process ID. Take them from the user or from `query documents` (below).
  If the project has several processes and the user did not say which, ask; in an autonomous run,
  explain every listed process in turn, or the one the task names, and say which.
- Optionally the audience (a manager, an auditor, a new colleague) and a length.

## Tool sequence

1. `inspect`: confirm the session and that `documents` is not 0. Note `generation`; if it changes
   while you work (someone is editing), say that the explanation describes an earlier state.
   `query` `documents` lists the documents with their IDs, process IDs, lane IDs and whether each
   is editable; it is the source of `document_id` and `process_id` for the queries below.
2. `query` `model` for the document (needs `document_id`): JSON-Pointer field pages (`limit` at most 100). Read the
   structure from `/processes/N/nodes/M/{name,kind}`, `/processes/N/lanes/M/name` and
   `/processes/N/flows/M/{source,target,name,condition}`; the gateways and their branch
   conditions are only visible here. The rows are `{"path", "value"}` pairs of the whole document
   including the diagram shapes (about 200 rows for a five-element process), so the structure comes
   first and the shapes last. Follow `nextOffset` when the process you explain is not yet in the
   page. `table` is the shorter way to the activities; use `model` for gateways and flows.
3. `query` `table` for the process (needs `document_id` and `process_id`): one row per activity
   with its lane, the role of the lane, the systems and documents linked to it, its attribute
   values, and its outgoing flows with conditions. This is the backbone of the explanation.
4. `query` `raci` for the process (same two IDs): who is Responsible, Accountable, Consulted and
   Informed per activity, as one `cell` per role holding at least one letter (booleans
   `responsible`, `accountable`, `consulted`, `informed`). R comes from the lane's Role link,
   A, C and I from the activity's attribute values (README, "RACI"). A `Literal` target is a
   free-text name that is not a dictionary role: say so.
5. `query` `dictionary`: the names behind role and system entries.
6. `query` `attributes` for the document: definitions with `usage_count`, and the values per element.
7. `query` `diagnostics` and `validate`: findings the reader should know about. No findings
   means none under the stored profile (or only the built-in checks when no profile is stored);
   say that, not "the process is correct". `query` `conventions` shows which profile that is.
8. Write the explanation from those results only.

Page each query until `nextOffset` is `null`. If a query is refused for size, lower `limit`.

## Worked example

```json tool=query
{"kind": "model", "document_id": "11111111-1111-4111-8111-111111111111", "offset": 0, "limit": 100}
```

```json tool=query
{"kind": "table", "document_id": "11111111-1111-4111-8111-111111111111", "process_id": "procurement", "offset": 0, "limit": 50}
```

```json tool=query
{"kind": "raci", "document_id": "11111111-1111-4111-8111-111111111111", "process_id": "procurement", "offset": 0, "limit": 50}
```

```json tool=query
{"kind": "attributes", "document_id": "11111111-1111-4111-8111-111111111111", "offset": 0, "limit": 50}
```

```json tool=query
{"kind": "dictionary", "offset": 0, "limit": 50}
```

A good explanation names the steps in order, then the decisions with their conditions, then the
responsibilities, then what is not known. For the process above it reads like: "Procurement starts
when a request is received. The requester's lane reviews it (Review request, a user task) and a
gateway asks whether it was approved; approved requests go to Procurement to place the order, and
rejected ones end. Review request: Requester is Responsible, Procurement Accountable, Finance
manager Consulted, Legal Informed (Legal is not in the dictionary, so it is an unlinked label).
There are no diagnostics under the stored profile."

## On GenerationConflict

This prompt does not write, so there is no conflict to resolve. If `generation` moved between two
queries, repeat the query that came first and use the newer result, or state which generation each
fact comes from.

## Must not

- Call `apply`, `save`, `export` or `reconcile`. If the user also wants changes, finish the
  explanation, then switch to [review-conventions](../review-conventions/SKILL.md) or
  [fill-attributes](../fill-attributes/SKILL.md) with the user's agreement.
- State a fact that is not in a query result: durations, costs, owners, system names, intent. Say
  "not recorded" instead.
- Guess a document or process ID, or describe a different process because it has a similar name.
- Present a RACI letter as official policy; it is what the model records.
- Claim the model is correct, complete, or conformant to BPMN. Report the diagnostics as findings
  under the stored profile.
