# M3 DI fixtures

## di-values.bpmn

- File: `di-values.bpmn`
- Origin: synthetic fixture authored for ProcessCraft; no external model content.
- License: CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/).
- SHA-256: `78ea186fff78244f13894b74eefee71c63c278b9b511c25037f30888a11108ad`
- Expected import mode: editable typed import (`report.editable == true`), not preserved-source.
- XSD: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/di/di-values.bpmn` -> `XSD valid`; the CLI re-export of the same fixture is also XSD valid.
- Purpose: distinct fractional/negative Bounds and Point values, expanded sub-process shape, diagram name/documentation/resolution, label bounds and `labelStyle` reference.
