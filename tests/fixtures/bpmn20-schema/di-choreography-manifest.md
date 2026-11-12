# Synthetic choreography DI fixture

`di-choreography-bands.bpmn` is newly authored for ProcessCraft and dedicated to the public domain under CC0-1.0. It does not contain a vendor model. The local normative sources consulted were the pinned BPMN 2.0.2 PDF (§12.3, Tables 12.27–12.35) and the schemas listed below. This fixture is test data, not a statement that ProcessCraft implements every feature it contains.

The document includes two independently identified diagram representations of one choreography; independent band and label IDs; every `participantBandKind` value; three participants and a participant multiplicity; message flows and visible-message flags; choreography tasks, nested subchoreographies, a call and a global choreography task; events and a gateway; a DI extension and mixed shape/edge ordering. Label styles have independent IDs in each diagram. The nested subchoreographies are collapsed in the supplied DI.

The schema checks XML structure and datatypes, not all BPMN semantic, visual, or interoperability constraints. In particular this fixture does not claim to test a data-association-to-sequence-flow visual shortcut: §12.3 describes that shortcut as normalizing two semantic data associations, so a separate fixture must include and verify those associations. It also does not exercise the `messageVisibleKind` edge attribute.

Validation, from repository root, with Python `lxml` 6.1.0:

```sh
python -c 'from lxml import etree as E; p="tests/fixtures/bpmn20-schema/"; s=E.XMLSchema(E.parse(p+"BPMN20.xsd")); s.assertValid(E.parse(p+"di-choreography-bands.bpmn")); print("valid")'
```

SHA-256 of the exact test data and local sources:

```text
bed034d03a6f7a0636320d0a04c53ef8d8a77d6252758b9e6c6cacfc5ec52036  di-choreography-bands.bpmn
a07c159cb0594573dd7c97b1370dd116112378f377e43c89a8bf512ac5030705  BPMN20.xsd
c4318842f7d2bbc262d7954c9452c501db16f0868eac0b8732ec5d7fb384d9a7  Semantic.xsd
f0dff1cd559d1514d8ebfc8c646f58402bcaced27ec22e2aa6456c2dcc80b038  BPMNDI.xsd
8220b179c175572df74e08a51bffabe957867962035cee7b5fee0b6acb4c4498  DI.xsd
a2f90e5ad9bb48c6915e4e034b4e27ac838264a1d4f27bfc70dbdfc69351312d  DC.xsd
d3d9258a70bf808c11ac52becea4ce645540a8b881117535f60a56fbf13e22f2  BPMN-2.0.2.pdf
```
