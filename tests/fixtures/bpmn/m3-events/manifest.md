# M3 events fixtures

Both files are synthetic, authored for ProcessCraft, license CC0-1.0 (https://creativecommons.org/publicdomain/zero/1.0/). Both validate against the pinned OMG XSD: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/m3-events/*.bpmn` reported `XSD valid` for each. Exports of both (through `export_bpmn` after import, via `M3_EVENTS_EXPORT_DIR` in `crates/processcraft-io/tests/m3_events.rs`) also passed the same script.

| File | SHA-256 | Expected import mode | Purpose |
|---|---|---|---|
| `timers-boundary-terminate.bpmn` | `2cf93749c67fb7c5fd35f66c99f2d68f87773ecb01ce8f644be4dac123c06c15` | editable (typed model) | timeDate/timeDuration/timeCycle timers, non-interrupting boundary (`cancelActivity="false"`, `attachedToRef`), terminate end event |
| `references-links-conditions.bpmn` | `96e93f21c803cbf540c580baa22b32efaf281aad8093a1a2a6288f8949135d4d` | editable (typed model) | error and escalation refs, throw `eventDefinitionRef`, message `messageRef` + `operationRef`, link throw/catch pair, conditional catch |
