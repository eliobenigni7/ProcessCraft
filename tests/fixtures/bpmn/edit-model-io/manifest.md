# M3 round 2 edit-model-io fixtures

| File | Origin | License | SHA-256 | Expected import mode |
|---|---|---|---|---|
| `lane-partitions.bpmn` | synthetic, authored for ProcessCraft | CC0 1.0 Universal | `15c22bc4b4184b660e5559ffc158aa3d158324ad5d595b7dc26d4cb3e89f0429` | typed editable (`report.editable == true`) |

- Content: collaboration with two participants, a named message flow with messageRef, a root message, a process with a named `laneSet`, a lane with a `partitionElement` (`xsi:type="tCategoryValue"`, because tBaseElement is abstract in the pinned XSD) and a named `childLaneSet`, a lane with `partitionElementRef`, and two sequence flows.
- XSD check: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/edit-model-io/lane-partitions.bpmn` -> `XSD valid` (2026-11-12).
- Export check: `cargo test -p processcraft-io --test edit_model_io` writes the exported XML to `target/tmp/m3r2-edit-model-io-export.bpmn`; `python scripts/check_bpmn20_xsd.py target/tmp/m3r2-edit-model-io-export.bpmn` -> `XSD valid` (2026-11-12).
- Used by: `crates/processcraft-io/tests/edit_model_io.rs`.
