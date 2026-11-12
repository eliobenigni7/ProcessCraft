# M3 artifacts fixtures

## artifacts-all.bpmn

- Origin: synthetic fixture authored for ProcessCraft; no external model content.
- Version: 1
- License: CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/).
- SHA-256: `02e5a604aa4c05d03280c7c12a60f7bf06d7a8e7bb85fe566188acc763c4ce41`
- XSD check: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/artifacts/artifacts-all.bpmn` -> `XSD valid`.
- Expected import mode: editable typed model (not preserved source).
- Purpose: category "Stage" with categoryValue "Review", a text annotation (textFormat text/plain), three associations (directions One, Both, None) and a group referencing the category value.
