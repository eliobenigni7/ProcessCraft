# The prompt pack

Four instruction documents for an agent that drives ProcessCraft through MCP. Each one is plain text
and also an agent skill: a `SKILL.md` with `name` and `description` front matter. Load one as a skill,
or paste its text into the agent's instructions. The MCP server has no `prompts` capability; these
files are the prompts.

| Document | Use it to | Raw file |
|---|---|---|
| text-to-model | turn a written process description into a new model | [SKILL.md](https://raw.githubusercontent.com/eliobenigni7/ProcessCraft/main/docs/prompts/text-to-model/SKILL.md) |
| review-conventions | find and fix modeling-convention findings in an existing model | [SKILL.md](https://raw.githubusercontent.com/eliobenigni7/ProcessCraft/main/docs/prompts/review-conventions/SKILL.md) |
| explain-process | explain an existing model in plain language, read only | [SKILL.md](https://raw.githubusercontent.com/eliobenigni7/ProcessCraft/main/docs/prompts/explain-process/SKILL.md) |
| fill-attributes | collect missing attribute values from source material and write them | [SKILL.md](https://raw.githubusercontent.com/eliobenigni7/ProcessCraft/main/docs/prompts/fill-attributes/SKILL.md) |

Every document starts by pointing to the shared protocol, which is [The MCP server](mcp.md) page.
Nothing is installed with the application: take the files from the repository.

## How they are checked

- The JSON examples in the pack are validated against the live command and tool schemas by
  `cargo test -p processcraft-cli --test prompt_pack`.
- One recorded run per document (`tests/fixtures/prompts/*.transcript.json`) is replayed against the
  MCP server in CI, so the calls it makes keep working.

How well a particular model follows the instructions has not been measured, and no claim is made about
it. Review what an agent changed before you rely on it.
