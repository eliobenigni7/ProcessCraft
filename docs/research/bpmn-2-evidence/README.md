# BPMN 2 evidence fragments

Each `*.json` file here is one fragment (one auditor owns one file). `python scripts/generate_bpmn20_inventory.py` merges all fragments (sorted by file name, cells by schema/path/dimension) into `docs/research/bpmn-2-coverage.json`; a verified cell overrides the profile-derived state.

```json
{"family": "<name>", "cells": [
  {"path": "<declaration path exactly as in coverage json>", "schema": "<xsd file name>",
   "dimension": "model|edit|render|import|export|validate", "state": "verified",
   "evidence": ["<repo-relative file>", "<file>:<test_fn_name>"], "note": "<optional>"}]}
```

Rules (enforced on every run, including `--check`): unknown path or dimension is an error; a duplicate (schema, path, dimension) across fragments is an error; the evidence list must be non-empty; every file must exist; for `file:name` the file must contain `name` (the `fn`/`def` name or literal). Only `state: "verified"` is accepted.

Commands: `--check` (output equals committed JSON), `--gate` (prints required-scope counts; exit 1 while any required cell is not verified), `--gate --list` (also lists pending rows grouped by top-level owner).

## "Verified" per dimension

Preserved-source never counts.

| Dimension | Verified means (evidence pointer) |
|---|---|
| model | Typed field in `processcraft-core`, with a core test constructing and reading it; opaque-source-only storage does not count. |
| edit | A `Mutation` (present in `COMMAND_DESCRIPTORS`) sets/clears it, with a command test including undo, plus a GUI or CLI path test. |
| render | A processcraft-render scene test or golden/PDF check for the notation, plus hit-testing where applicable. |
| import | An `interchange.rs` test importing a fixture in `tests/fixtures/**` (with manifest entry) into the typed model, not preserved-source. |
| export | An export test whose output validates against the pinned XSD (`scripts/check_bpmn20_xsd.py`) and re-imports equal. |
| validate | A negative fixture or core validation test with a diagnostic referencing the clause. |
