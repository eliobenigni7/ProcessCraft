# M3 activities fixtures

## activities.bpmn

- Origin: synthetic, authored for ProcessCraft; no external model content.
- License: CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/).
- SHA-256: `91356c21a1f9428276df23f02a4a60947eebec4fbc58f39a0022d4cde3f3221c`
- Expected import mode: typed editable (`report.editable == true`); not preserved-source.
- Content: task (isForCompensation, startQuantity, completionQuantity, default), userTask, serviceTask, sendTask, receiveTask (instantiate, implementation, messageRef, operationRef), callActivity (calledElement), event subProcess (triggeredByEvent).
- XSD: `python scripts/check_bpmn20_xsd.py tests/fixtures/bpmn/activities/activities.bpmn` -> "XSD valid" (pinned OMG BPMN20.xsd). The exported output (`target/tmp/m3-activities-export.bpmn`, written by the io test) is also XSD valid.
- Used by: `crates/processcraft-io/tests/bpmn_activities.rs` (negative variants are derived in-test by string replacement, not stored).
