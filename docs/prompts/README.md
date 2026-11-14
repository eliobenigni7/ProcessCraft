# ProcessCraft MCP prompt pack

Four instruction documents for an agent that drives ProcessCraft through its local MCP server. Each
is plain text and also an agent skill (`SKILL.md` with `name` and `description` front matter):

| Document | Use it to |
|---|---|
| [text-to-model](text-to-model/SKILL.md) | turn a written process description into a new model |
| [review-conventions](review-conventions/SKILL.md) | find and fix modeling-convention findings in an existing model |
| [explain-process](explain-process/SKILL.md) | explain an existing model in plain language (read only) |
| [fill-attributes](fill-attributes/SKILL.md) | collect missing attribute values from source material and write them |

The server has no `prompts` capability: these files are the prompts. Load one as a skill, or paste
its text into the agent's instructions. Nothing is installed with the application.

<!-- ANCHOR: protocol (included by docs/book/src/agents/mcp.md) -->
## Start the server

```text
processcraft-cli mcp --new "Invoice approval" --write-root C:\work\out
processcraft-cli mcp --in C:\work\in\procurement.processcraft --read-root C:\work\in --write-root C:\work\out
processcraft-cli mcp --attach <pipe> --token <token> --session <session id>
```

The first two start a standalone session on stdio (new project, or an existing file). The third, on
Windows, attaches to the visible session of the desktop app, using the pipe, token and session ID
the app shows when the user authorizes it; the agent then edits the same project as the person
looking at the screen. No listener starts by default.

A complete start on an existing file that will be edited and saved:

```text
processcraft-cli mcp --in C:\work\in\procurement.processcraft --read-root C:\work\in --write-root C:\work\out
```

Roots are absolute paths of existing folders; each flag may be repeated. What each needs:

| Task | Needs |
|---|---|
| read only (`explain-process`), file given with `--in` | the file's folder as `--read-root` (or `--write-root`) |
| edit and save a new project (`--new`) | `--write-root` only |
| edit an `--in` file and save a result | the `--in` file inside a `--read-root` or `--write-root`, and a `--write-root` for `save` and `export` |
| edit the `--in` file in place | the file's own folder as `--write-root`; save to the same path with `overwrite: true`, only if the person asked |

## Talking to the server

The server speaks JSON-RPC 2.0, one JSON message per line, on stdin and stdout.

1. Send `initialize` with a `protocolVersion` (`2025-03-26`, `2025-06-18` and `2025-11-25` were all
   accepted; the reply echoes the one you sent), `capabilities` `{}` and a `clientInfo`.
2. Send the notification `notifications/initialized` (no `id`; it gets no reply).
3. Call a tool with `{"method": "tools/call", "params": {"name": "<tool>", "arguments": {...}}}`.
   The reply is `result.structuredContent`, the JSON object the tables below describe, and the same
   object as text in `result.content[0].text`; `result.isError` is `false`. A bad argument (an
   unknown field, a missing `document_id`, a limit outside 1..100) is a JSON-RPC `error` with code
   `-32602` and a message such as ``missing field `process_id` ``, not a result.
4. To end a standalone stdio session, close the server's stdin; the process then exits with status
   0. Save first: a session that is not saved is gone.

`contentId` and `diskDigest` are arrays of 32 byte values (a SHA-256), not hex strings. Compare them
whole; the pack never needs them except to see that a value changed.

## The tools

| Tool | Arguments | Effect |
|---|---|---|
| `inspect` | none | `sessionId`, `generation`, `contentId`, `projectId`, project `name`, `documents` (a count). For the IDs use `query documents`. |
| `validate` | none | `{"valid": true, "errorCount": n, "warningCount": n}`; if the project is structurally invalid, `{"valid": false, "error": "..."}` |
| `query` | `kind` plus that kind's fields, `offset`, `limit` | read-only, paged; see below |
| `apply` | `sessionId`, `requestId`, `expectedGeneration`, `request` | the only way to change the model |
| `save` | `sessionId`, `requestId`, `expectedGeneration`, `path`, `overwrite` | native `.processcraft` file in a write root; see "Save and reconcile" |
| `export` | like `save`, plus `documentId`, `format`, `acknowledgeNativeFields` | BPMN, SVG, PNG or PDF in a write root |
| `reconcile` | `sessionId`, `requestId`, `expectedGeneration`, `operationId` | settle a file operation that reported `reconciliationRequired` |

## The query kinds

Every `query` takes `kind`, `offset` and `limit` (1..100) plus the fields in the table, except
`conventions`, which takes only `kind` (adding `offset` or `limit` is refused with `-32602`:
``unknown field `limit` ``). The field names are `snake_case` (`document_id`,
`process_id`, `entry_id`), unlike the camelCase envelope of `apply`. A page is `{"total", "items",
"nextOffset"}`. Follow `nextOffset` while you still need rows, and stop as soon as you have them:
a complete list (`diagnostics`, `table`, `raci`, `dictionary`) is paged to the end, but a `model`
dump of hundreds of rows is read only as far as the nodes, lanes and flows you need (the diagram
shapes come last).

| kind | extra fields | `items` hold |
|---|---|---|
| `documents` | none | one row per document: `id`, `name`, `capability` (`editable`, `retainedExtensions` or `preservedSource`), `editable`, and `processes`: each `{id, name, kind: "process" or "subProcess", lanes: [{id, name}]}` |
| `conventions` | none | not paged: `{"conventions": {"rules": [...]}}`, or `{"conventions": null}` when the project stores no profile |
| `diagnostics` | none | findings: `code`, `severity`, `message`, `documentId`, `elementId` |
| `dictionary` | none | `{id, kind, name}` per entry (kind `Role`, `System`, ...) |
| `model` | `document_id` | `{"path": "/processes/0/nodes/1/name", "value": ...}` rows: a JSON-Pointer dump of the whole document, diagram shapes included (hundreds of rows for a small process) |
| `table` | `document_id`, `process_id` | one row per activity: `key`, `name`, `lane`, `role`, `systems`, `documents`, `attributes`, `outgoing` |
| `raci` | `document_id`, `process_id` | one row per activity, plus a top-level `columns`; see "RACI" |
| `attributes` | `document_id` | top-level `definitions`; `items` only for elements that already have a value |
| `usages` | `entry_id` | where one dictionary entry is used |

Others exist (`events`, `data`, `catalog`, `identities`, `diagrams`, `comments`, `checkpoints`,
`checkpointFields`, `compareCheckpoint`, `layoutPreview`, `library`, `replaceOptions`); the
`tools/list` schema is the authority. `table` and `raci` need a `process_id` as well as a
`document_id`; every kind that names a document needs its `document_id`, and a call without it is
refused with `-32602`.

`query attributes` lists only elements that already have a value; use `query table` (every
activity) or `query diagnostics` to find the ones that do not.

### Document and process IDs

`inspect` gives only a count. To get the IDs, call `query documents`:

```json tool=query
{"kind": "documents", "offset": 0, "limit": 100}
```

```json output
{"total": 1, "nextOffset": null, "items": [{"id": "44444444-4444-4444-8444-444444444444", "name": "Conventions", "capability": "editable", "editable": true, "processes": [{"id": "conv_process", "name": "Request handling", "kind": "process", "lanes": [{"id": "lane_requester", "name": "Requester"}, {"id": "lane_buyer", "name": "Buyer"}]}]}]}
```

`items[].id` is the `document_id`; `processes[].id` is the `process_id`. A document whose
`editable` is `false` cannot be changed through `apply`; do not try. If it holds several
processes and the task does not say which, ask.

### Stored conventions profile

`query conventions` returns the profile stored in the project, so you can see the rules and the
severities in force before you decide what to change. `null` means the project has none, and only
the built-in advisory findings are reported.

```json tool=query
{"kind": "conventions"}
```

```json output
{"conventions": {"rules": [{"type": "activityVerbObject", "severity": "warning", "verbs": []}]}}
```

### RACI

`query raci` answers `{"columns": [...], "items": [...], "total", "nextOffset"}`. `columns` names
every role that appears, as `{"name", "target"}` where `target` is `{"Entry": "<dictionary id>"}`
or `{"Literal": "<text>"}`. Each item is an activity (`key`, `name`, `lane`) with `cells`: one cell
per role that holds at least one letter, `{"name", "target", "responsible", "accountable",
"consulted", "informed"}` with booleans. Roles with no letter on that activity are absent. R comes
from the Role link of the activity's lane (an activity in a lane with no role link has no R); A, C
and I come from the activity's `Accountable`, `Consulted` and `Informed` attribute values. In
`table` and `attributes` those values are `{"DictionaryRefs": [...]}` lists whose members are
`{"Entry": "<dictionary id>"}` (a role in the dictionary) or `{"Literal": "<text>"}` (free text that
is not in the dictionary: shown, but it cannot be reused or checked). The matrix is what the model
records, not policy.

The authoritative schemas are the live ones: `processcraft-cli commands --json` for the request
protocol and the MCP `tools/list` reply for the tool inputs. The pack's examples are checked
against them by `cargo test -p processcraft-cli --test prompt_pack`.

## The change protocol

1. Call `inspect`. Keep `sessionId` and `generation`.
2. Read with `query` before you write: names, IDs and current values come from results, never from
   memory or guesses.
3. Call `apply` with `expectedGeneration` set to the generation you last saw. A `batch` is
   all-or-nothing and one undo step. A success returns `"ok": true` and the new `generation`; use
   that as the next `expectedGeneration`.
4. Call `query diagnostics` (and `validate`) after the change and report what remains. An empty
   `items` list with `total` 0 means no finding under the profile in use (the stored one, or only
   the built-in advisory checks when none is stored); it does not mean the model is complete,
   correct or conformant.

`requestId` must be new for every write. Sending the same `requestId` with the same payload returns
the stored result without applying it again (safe retry); the same `requestId` with a different
payload is refused with `"status": "Conflict"`. A rejected batch changes nothing and its
`requestId` is spent: fix the batch and send it under a new `requestId`.

### GenerationConflict

`{"ok": false, "error": "GenerationConflict"}` means the project changed since the generation you
used (the person edited in the desktop app, or another write landed). Nothing was applied. Call
`inspect`, re-run the queries your batch depended on, rebuild the batch from the fresh results
(elements may have been renamed or deleted), and send it with the new generation and a new
`requestId`. Do not retry the old payload blindly, and do not use `undo` to get around it.

## IDs and layout

- Documents, dictionary entries and attribute definitions use UUIDs. Mint a fresh random UUID for
  each one you create. Elements, flows, lanes, shapes and edges use XML IDs (letters, digits, `_`,
  `-`, starting with a letter or `_`); choose short unique readable ones such as `check_invoice`.
- Existing IDs come only from query results (`id` in `documents` rows for documents, processes and
  lanes, `elementId` in diagnostics, `key.xml_id` in table and attribute rows, `id` in dictionary
  rows) or from the task input.
- Do not place shapes by hand. Create the first element with `createAt`, add every following one
  with `append` from its predecessor, join branches with `connect`. The core chooses positions and
  routes. `createShape`, `createEdge` and `moveShape` are for repairing a diagram, not for building one.

## File roots

`save` and `export` write only below a `--write-root` (standalone) or a folder the person
authorized in the desktop app (attached). The `path` must be absolute (a relative path is not
resolved against a root and is answered with `pathNotAuthorized`), inside a root and not the root
itself, without `..`, UNC or device prefixes, or links; on Windows `C:/work/out/a.processcraft`
and `C:\work\out\a.processcraft` both work. A path outside every root is answered with
`pathNotAuthorized`, which also hides whether the file exists. `--read-root` governs reading other
files and is not needed for `--new`. Use `overwrite: false` unless the person asked to replace a
file.

A writable file leaves a sidecar `<file>.lock` (an empty file) next to it, for example
`onboarding.processcraft.lock`. It is the lock that keeps two ProcessCraft sessions from writing
one file; a second session on a locked file is read-only. The sidecar stays after the session ends
and is harmless; delete it only when no ProcessCraft session has the file open. Do not list it as
a result.

### Save and reconcile

A successful `save` is `{"ok": true, "status": "committed", "operationId", "sessionId",
"contentId", "diskDigest", "warning": null}`; `ok` true with `status` `committed` is the only
success, and a non-null `warning` is worth reporting. Anything else has `"ok": false` and a
`status`: `generationConflict` (the project moved; `inspect` and retry), `pathNotAuthorized`,
`overwriteRequiresAuthorization` (the file exists and `overwrite` was false), `notCommitted`
(nothing written; read `error`), or `indeterminate` with `reconciliationRequired: true` (the write
may or may not have landed). In the last case call `reconcile` with the same `sessionId`, the
`operationId` from the reply, a new `requestId` and the current `expectedGeneration`, and report
what it says; do not save again blindly.

## Working without a person to ask

The documents say "ask" where the input leaves something open. In an autonomous run, where nobody
can answer, do not fill the gap with invented structure, values or IDs. Do the part the input does
decide, leave the open part out of the model, and list each open point in your reply (what is
missing, what you would need, which element it concerns). A question you cannot ask is an open
point, not permission to guess.

## What no document in this pack permits

- Guessing an ID, a document, a role or an attribute value. Query it, or ask.
- Hand placement of coordinates when `createAt`, `append` or `connect` can do it.
- Claiming that a model is "valid BPMN", "BPMN 2.0 compliant" or certified. `validate` and the
  diagnostics are advisory checks of this application, not conformance statements.
- Writing without `expectedGeneration`, reusing a spent `requestId` for a different payload, or
  continuing after a failed `apply` as if it had succeeded.
- Saving or exporting outside the granted roots, or overwriting a file nobody asked to overwrite.
- Deleting content, or changing names and values the task did not mention, to make a diagnostic
  disappear.
<!-- ANCHOR_END: protocol -->
