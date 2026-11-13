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

## The seven tools

| Tool | Arguments | Effect |
|---|---|---|
| `inspect` | none | `sessionId`, `generation`, `contentId`, project name, number of documents. It does not list document IDs. |
| `validate` | none | structural validity plus `errorCount` and `warningCount` of the diagnostics |
| `query` | `kind` plus that kind's fields, `offset`, `limit` | read-only, paged; see below |
| `apply` | `sessionId`, `requestId`, `expectedGeneration`, `request` | the only way to change the model |
| `save` | `sessionId`, `requestId`, `expectedGeneration`, `path`, `overwrite` | native `.processcraft` file in a write root |
| `export` | like `save`, plus `documentId`, `format`, `acknowledgeNativeFields` | BPMN, SVG, PNG or PDF in a write root |
| `reconcile` | `sessionId`, `requestId`, `expectedGeneration`, `operationId` | settle a file operation that reported `reconciliationRequired` |

`query` kinds used by the pack: `diagnostics`, `dictionary`, `attributes`, `table`, `raci` and `model`. The kind-specific field names are `snake_case` (`document_id`, `process_id`, `entry_id`),
unlike the camelCase envelope of `apply`. Results page with `offset` and `limit`; follow
`nextOffset` until it is `null`. `query attributes` lists only elements that already have a value;
use `query table` (every activity) or `query diagnostics` to find the ones that do not.

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
4. Call `query diagnostics` (and `validate`) after the change and report what remains.

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
- Existing IDs come only from query results (`elementId` in diagnostics, `key.xml_id` in table and
  attribute rows, `id` in dictionary rows) or from the task input. The document ID of an existing
  project is in the task input, in `processcraft-cli info <file>`, or in the `documentId` of a
  diagnostics row.
- Do not place shapes by hand. Create the first element with `createAt`, add every following one
  with `append` from its predecessor, join branches with `connect`. The core chooses positions and
  routes. `createShape`, `createEdge` and `moveShape` are for repairing a diagram, not for building one.

## File roots

`save` and `export` write only below a `--write-root` (standalone) or a folder the person
authorized in the desktop app (attached). A path outside every root is answered with
`pathNotAuthorized`, which also hides whether the file exists. `--read-root` governs reading other
files and is not needed for `--new`. Use `overwrite: false` unless the person asked to replace a
file.

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
