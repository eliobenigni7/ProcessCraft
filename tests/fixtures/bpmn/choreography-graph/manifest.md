# Synthetic choreography fixture

`nested.bpmn` was authored for ProcessCraft qualification and is made available under CC0-1.0. It contains a nested scope, call mapping, global choreography and external QName sharing a local name with a local message flow.

SHA-256: `c693cace1398f49fa3e0be82604b03f78be26d8572c4f757972e0d63bafa6f1a`

The pinned XSD permits one or two task `messageFlowRef` elements; BPMN 2.0.2 Table 11.2 describes one or more. These are distinct schema and specification statements, not an inferred extra constraint.

`boundary-event-io.bpmn` is synthetic CC0-1.0, with two boundary catch events, scoped data output associations and local QName attachment to a SubChoreography. Its SHA-256 is `8b2236cda849334a7208aa55f1c6b616cfa6653dfb5f2380c4360e4cc9b9c102`.

`subplane.bpmn` is synthetic CC0-1.0. It contains a SubChoreography plane with an expanded nested SubChoreography, its participant bands, events and sequence flows, plus a sibling scope for negative membership tests. Its SHA-256 is `9b88a93da5c587ec70c0fe5be16a3c78b0a6375c88e92cfa724d4e48aee07c63`.
