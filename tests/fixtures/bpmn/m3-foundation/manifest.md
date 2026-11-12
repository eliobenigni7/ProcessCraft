# m3-foundation fixtures

| File | SHA-256 | Provenance | License | Expected import mode | XSD (scripts/check_bpmn20_xsd.py) |
|---|---|---|---|---|---|
| callable-name-implementation.bpmn | 1f4c81b7178465b56dbbc65c48441cc827b9fb3d4b5e4cc2b0f141cd368cdebf | synthetic, written for the M3 foundation family | CC0 | editable (typed model) | XSD valid |

The foundation tests also import pinned fixtures `bpmn20-schema/gateways.bpmn`, `bpmn20-schema/artifacts.bpmn` and `bpmn/activity-correlation.bpmn` (their own manifests apply). Exports of all four are written by the io test to `target/m3-foundation/*.bpmn` and were checked with the XSD script: all valid.
