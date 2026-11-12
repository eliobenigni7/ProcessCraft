# M3 process-collaboration fixtures

| File | Origin | License | SHA-256 | Expected import mode |
|---|---|---|---|---|
| `process-collaboration.bpmn` | synthetic, authored for ProcessCraft | CC0 1.0 Universal | `0b3dd2be097811a046ce8ad5202f2470fdd66d06867bb0e9f9f8624bd39cb922` | typed editable (`report.editable == true`) |

- Content: collaboration (name, isClosed, choreographyRef), participants (name, interfaceRef, endPointRef, processRef), a named message flow with messageRef, a root message with itemRef, a process with processType/isClosed/isExecutable/definitionalCollaborationRef/supports, a laneSet with two named lanes and flowNodeRef, and a sequenceFlow with isImmediate.
- XSD check: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/process-collaboration/process-collaboration.bpmn` -> `XSD valid` (2026-11-12).
- Export check: `cargo test -p processcraft-io --test bpmn_process_collaboration` writes the exported XML to `target/tmp/m3-process-collaboration-export.bpmn`; `python scripts/check_bpmn20_xsd.py target/tmp/m3-process-collaboration-export.bpmn` -> `XSD valid` (2026-11-12).
- Used by: `crates/processcraft-io/tests/bpmn_process_collaboration.rs`.
