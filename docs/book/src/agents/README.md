# Reading these docs as an agent

This section is for AI agents, and for the people who configure them, that drive ProcessCraft through
its local MCP server or its command-line tool.

## Machine-readable entry points

| URL | What it is |
|---|---|
| [`llms.txt`](https://eliobenigni7.github.io/ProcessCraft/docs/llms.txt) | An index of every page in this book, with a one-line summary of the product. |
| [`llms-full.txt`](https://eliobenigni7.github.io/ProcessCraft/docs/llms-full.txt) | Every page in one plain-text file, in reading order. |
| `<page>.md` | Every page is also published as Markdown next to its HTML: replace `.html` with `.md`. |
| [`reference/commands.json`](../reference/commands.json) | The JSON Schema of the request protocol and the descriptor of every batch command, as `processcraft-cli commands --json` prints it. |

The authoritative schemas are always the live ones: `processcraft-cli commands --json` for batches and
the MCP `tools/list` reply for tool inputs. The published copies are regenerated from the same
program, but your installed version wins if they differ.

## What to read

1. [The MCP server](mcp.md): starting it, the seven tools, the query kinds, the change protocol,
   IDs and file roots. Read it before the first call.
2. [The prompt pack](prompt-pack.md): four ready-made task instructions.
3. [Batches and the command pipeline](../developers/batches.md) and
   [Reference: batch commands](../reference/commands.md) when you build a batch.
4. The analyst pages ([conventions](../analysts/conventions.md),
   [attributes](../analysts/attributes.md)) explain what the findings and values mean to the person you
   work for.

## Rules that hold in every task

- Read before you write. IDs, names and values come from query results or the task, never from
  memory or guesses.
- Change the model only through `apply`, with the generation you last saw and a new request ID.
- Build diagrams with `createAt`, `append` and `connect`; do not place coordinates by hand.
- Diagnostics are advisory checks of this application. Do not describe a model as "valid BPMN",
  compliant or certified.
- Save or export only inside the folders you were given, and do not overwrite a file nobody asked to
  overwrite.
- When the input leaves something open and nobody can be asked, leave it out and report it as an open
  point instead of inventing it.
