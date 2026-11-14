# The command-line tool

`processcraft-cli` ships next to the desktop app (`processcraft-cli.exe` in the installer and the
portable ZIP). It runs exactly the commands the canvas runs, without a window, and never changes a
file it was not told to write. Every subcommand and option is listed in
[Reference: CLI subcommands](../reference/cli.md).

The examples use the fixtures in the repository, so you can run them from a clone.

## Build a project from a batch

```sh
processcraft-cli commands --json      # every command, with the JSON Schema of the request protocol
processcraft-cli run --new Procurement --batch tests/fixtures/commands/procurement.json --out procurement.processcraft
processcraft-cli info procurement.processcraft
```

`run` prints a JSON receipt with `"ok": true` and the content identity of the saved project. `info`
lists the documents and their diagram IDs, here the document
`11111111-1111-4111-8111-111111111111` with the diagram `procurement_diagram`.

To change an existing project, start from it with `--in` instead of `--new`. The output is always a
separate `--out` file; `--overwrite` is needed to replace an existing file other than the input.

```sh
processcraft-cli run --in procurement.processcraft --batch tests/fixtures/commands/library-dictionary.json --out procurement-dictionary.processcraft
```

## Read without writing

`query` returns projections as JSON and never changes the file:

```sh
processcraft-cli query procurement-dictionary.processcraft dictionary
processcraft-cli query procurement-dictionary.processcraft documents
```

Other kinds include `table`, `raci`, `attributes`, `conventions`, `diagnostics`, `replace-options` and
`checkpoints`; `processcraft-cli query --help` lists them all.

## Validate in CI

`validate` checks structure and applies the conventions profile stored in the project, or the one you
pass with `--profile`. It exits with status 3 when a finding reaches the threshold: errors by default,
warnings with `--fail-on warning`. The summary goes to stderr and the diagnostics to stdout as JSON.

```sh
processcraft-cli run --new Procurement --batch tests/fixtures/conventions/pass.json --out conventions.processcraft
processcraft-cli run --in conventions.processcraft --batch tests/fixtures/commands/raci.json --out project.processcraft
processcraft-cli validate project.processcraft --fail-on warning
# stderr: 0 error and 4 warning diagnostics; exit status 3
```

A profile file to try rules without storing them:

```json
{"rules": [
  {"type": "labelRequired", "severity": "warning", "kinds": ["Activity", "Event", "Lane", "Participant"]},
  {"type": "gatewayFlowsLabelled", "severity": "warning"},
  {"type": "noImplicitSplitJoin", "severity": "error"}
]}
```

```sh
processcraft-cli validate project.processcraft --profile profile.json
# stderr: 1 error and 0 warning diagnostics; exit status 3
```

The schema of a profile is the `setConventionsProfile` command in `commands --json`.

## RACI, images and BPMN

```sh
processcraft-cli query project.processcraft raci 11111111-1111-4111-8111-111111111111 procurement
processcraft-cli export-raci project.processcraft 11111111-1111-4111-8111-111111111111 procurement --out raci.csv

processcraft-cli export-image project.processcraft --document-id <document-id> --diagram-id <diagram-id> --format png --out diagram.png
processcraft-cli export-image project.processcraft --document-id <document-id> --diagram-id <diagram-id> --format pdf --out cost.pdf --overlay-attribute <attribute-id> --overlay-mode heatmap

processcraft-cli export-bpmn project.processcraft --document-id <document-id> --out diagram.bpmn --acknowledge-native-fields
processcraft-cli import-bpmn diagram.bpmn --out diagram.processcraft
processcraft-cli info diagram.bpmn
```

- The RACI CSV is UTF-8 with a byte order mark and CRLF line ends.
- `export-image` writes SVG, PNG or PDF; `--dark` renders the dark appearance and `--scale` the size.
  Attribute IDs come from `query <file> attributes <document-id>`.
- Without `--acknowledge-native-fields`, `export-bpmn` refuses when the project holds data BPMN cannot
  carry, and says what it would leave out.
- `info` on a BPMN file reports how it would import before anything is created.
- `export-drawio`, `layout-preview` and `layout-apply` complete the set.

A repeatable end-to-end run of these commands is `scripts/m5_cli_workflow.py`.
