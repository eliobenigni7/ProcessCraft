# M3 gateways fixtures

## gateways-all.bpmn

- Origin: synthetic fixture authored for ProcessCraft M3 gateway evidence; no external model content.
- License: CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/).
- SHA-256: `13a094db58a0762eb0ad73e3e406c74bc41e45e073e1462283155385bd8c53d2`
- XSD: valid against the pinned OMG BPMN20.xsd (`python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/gateways/gateways-all.bpmn` -> `XSD valid`).
- Expected import mode: editable typed model (not preserved-source); exports and re-imports with equal gateway details.
- Purpose: one exclusive, one inclusive, one parallel and two event-based gateways covering `default`, all four `gatewayDirection` values, `instantiate` and both `eventGatewayType` values.
- Export check: the re-exported bytes (written by `gateway_fixture_export_reimports_equal_and_keeps_attributes` to `target/m3-gateways-export.bpmn`) also pass `check_bpmn20_xsd.py` -> `XSD valid`.
