# The MCP server

`processcraft-cli mcp` serves the Model Context Protocol over stdio. It offers seven tools:
`inspect`, `validate`, `query`, `apply`, `save`, `export` and `reconcile`. Nothing listens by default:
a standalone server runs only while its client keeps it open, and attachment to the desktop app is
switched on by the person using it.

## Configuring a client

Most MCP clients take a command and its arguments. For example, a standalone server that may read
from one folder and write to another:

```json
{
  "mcpServers": {
    "processcraft": {
      "command": "C:\\Users\\you\\AppData\\Local\\Programs\\ProcessCraft\\processcraft-cli.exe",
      "args": ["mcp", "--new", "Invoice approval", "--write-root", "C:\\work\\out"]
    }
  }
}
```

The installer puts `processcraft-cli.exe` in `%LOCALAPPDATA%\Programs\ProcessCraft` by default; with
the portable ZIP it is in the folder you unzipped.

The rest of this page is the protocol section of the prompt pack (`docs/prompts/README.md` in the
repository), included here unchanged. Its examples are checked against the live schemas by the
project's tests.

---

{{#include ../../../prompts/README.md:protocol}}
