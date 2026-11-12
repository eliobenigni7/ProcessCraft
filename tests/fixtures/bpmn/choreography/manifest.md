# M3 choreography field-value fixtures

Both files are synthetic, authored for ProcessCraft qualification, license CC0 1.0 Universal. Both are valid against the pinned OMG `BPMN20.xsd` (`python scripts/check_bpmn20_xsd.py <file>` printed "XSD valid" for each).

| File | SHA-256 | Expected import mode |
|---|---|---|
| `values.bpmn` | `fa0ac422558144d15dedda4606c61606e8df44e4b083402d7e7727d405a29a39` | Editable (typed model; choreography graph). Task initiator/participantRef/messageFlowRef/loopType, participantMultiplicity min/max, callChoreography calledChoreographyRef and globalChoreographyTask initiatingParticipantRef are imported as typed values. |
| `bad-references.bpmn` | `e69d7dcf2320c4fc0df8bece023e18328a452fa1a87c97bfdf4233f8347af43a` | Not editable: retained byte-for-byte as preserved source (callChoreography targets a participant and a participant of the wrong type; global initiator belongs to a foreign choreography). |

Tests: `crates/processcraft-io/tests/bpmn_choreography.rs`.
