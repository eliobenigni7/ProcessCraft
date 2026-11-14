# The .processcraft file

A project is a ZIP package of versioned JSON and resources:

```text
Acme.processcraft
  mimetype                    application/vnd.processcraft+zip
  manifest.json               format, schema version, reader requirements, content identity
  project.json                the current project: documents, diagrams, library, dictionary, ...
  checkpoints/<uuid>.json     named revisions, when there are any
  resources/<sha256>          retained original files, content-addressed
```

`manifest.json` of a project written by 0.2.0 looks like this:

```json
{"format": "processcraft", "schema": 3, "min_reader": 3,
 "project_id": "933d728b-c04b-490d-be4a-a8c777eca97a",
 "content_identity": [125, 114, 202, ...],
 "required_capabilities": ["comments", "checkpoints"],
 "writer_version": "0.2.0"}
```

## Compatibility rules

- **Schema 3** is the current schema. A reader refuses a file whose `min_reader` it does not meet.
- **Required capabilities** name features a reader must understand to open the file safely. They are
  written only when the project uses them: `analyst-attributes-v1` for attribute definitions and
  values, `modeling-conventions-v1` for a stored conventions profile. Older readers refuse such files
  instead of opening them and losing data.
- A reader never parses what it recognises and saves over the rest.

## Saving

- Saves are atomic: the new file is written and then replaces the old one, so a failure leaves the
  previous version intact.
- A sidecar `<file>.lock` stops two ProcessCraft sessions from writing the same file; the second one
  is read-only.
- Recovery snapshots are written outside the project.
- Writable storage is qualified on fixed local Windows NTFS drives only.

## Treat it as an application format

The JSON inside is readable, but it is not a public interchange format and may change while
ProcessCraft is pre-alpha. To read a project from a program, use `processcraft-cli query` or MCP; to
change one, use a batch. For exchange with other tools, export BPMN.

The design and its rationale are in sections 8 and 9 of
[the solution design](https://github.com/eliobenigni7/ProcessCraft/blob/main/docs/design/solution-design.md).
