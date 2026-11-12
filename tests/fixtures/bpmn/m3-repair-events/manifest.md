# M3 repair events fixtures

Synthetic, authored for ProcessCraft, license CC0-1.0 (https://creativecommons.org/publicdomain/zero/1.0/). Validates against the pinned OMG XSD: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/m3-repair-events/events-inline.bpmn` reported `XSD valid`. The exports written by `crates/processcraft-io/tests/m3_repair_events.rs` to `target/tmp/m3-repair-events-inline-export.bpmn` and `target/tmp/m3-repair-events-refs-export.bpmn` (the latter from `tests/fixtures/bpmn/m3-events/references-links-conditions.bpmn`) were also checked with the same script and reported `XSD valid`.

| File | SHA-256 | Expected import mode | Purpose |
|---|---|---|---|
| `events-inline.bpmn` | `0de448a9f85ae3b78a7042384237fb75b1fbf605ba267f9f5d6a5cec084ea613` | editable (typed model) | inline signal start/throw, non-interrupting message boundary, end event with `eventDefinitionRef` to a root signal definition |
