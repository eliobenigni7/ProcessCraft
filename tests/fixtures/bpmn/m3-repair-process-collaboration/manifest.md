# M3 repair process-collaboration fixtures

| File | Origin | License | SHA-256 | Expected import mode |
|---|---|---|---|---|
| `repair-process-collaboration.bpmn` | synthetic, authored for ProcessCraft | CC0 1.0 Universal | `130efda9c39f2e0a22e0c7cea435a7037ed1197f517908d3b14e622de0bfe4d1` | typed editable (`report.editable == true`) |

- Content: collaboration with name, participant processRef, a named message flow with sourceRef/targetRef/messageRef, a root message with name and itemRef, a process with a laneSet (id, name) holding a lane with a nested childLaneSet (id, name) and an inner lane with flowNodeRefs, and a sequenceFlow with a formal conditionExpression.
- XSD check: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/m3-repair-process-collaboration/repair-process-collaboration.bpmn` -> `XSD valid` (2026-11-12).
- Export check: `cargo test -p processcraft-io --test m3_repair_process_collaboration` writes the exported XML to `target/tmp/m3-repair-process-collaboration-export.bpmn`; `python scripts/check_bpmn20_xsd.py target/tmp/m3-repair-process-collaboration-export.bpmn` -> `XSD valid` (2026-11-12).
- Used by: `crates/processcraft-io/tests/m3_repair_process_collaboration.rs`.
