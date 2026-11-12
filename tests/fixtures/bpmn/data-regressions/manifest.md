# data-regressions fixtures

| File | Provenance | License | SHA-256 | Expected import mode |
|---|---|---|---|---|
| data-repair.bpmn | Synthetic, written for this repository (M3 data repair evidence) | CC0-1.0 | 1e522b0ee0a60d2cb2238e7865b20ab1df48de5b212ae09b866072a8389b10a3 | Editable typed model (not preserved source) |

XSD validation (`python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/data-regressions/data-repair.bpmn`): XSD valid.
The bytes exported by `crates/processcraft-io/tests/bpmn_data_regressions.rs` (written to `target/tmp/m3-repair-data-export.bpmn`) were checked with the same script: XSD valid.
