# data fixtures

| File | Provenance | License | SHA-256 | Expected import mode |
|---|---|---|---|---|
| data-attributes.bpmn | Synthetic, written for this repository (M3 data evidence) | CC0-1.0 | 9700872f7b6f8276bdaea3cbb175f97abbbebee991239adc6fcc7f60f2d2e1d0 | Editable typed model (not preserved source) |

XSD validation (`python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/data/data-attributes.bpmn`): XSD valid.
The exported bytes re-imported by `crates/processcraft-io/tests/bpmn_data.rs` (written to `target/tmp/m3-data-export.bpmn`) were checked with the same script: XSD valid.

Notes: `itemDefinition` carries `isCollection="true"` because the importer keeps a document editable only when ports/objects with `isCollection="true"` reference a collection item definition.
