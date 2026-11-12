# M3 required-scope proposal

**Status: adopted by the user on 2026-11-12 with Option B (external gates deferred to a post-M3 qualification milestone). Not a conformance claim.** It makes the M3 gate "zero pending required rows" (M3 plan, M3.4 last item) computable. Source data: `docs/research/bpmn-2-coverage.json` (580 XSD declarations x 6 dimensions = 3480 cells; today 683 `partial`, 2797 `unverified`, 0 verified). Element lists follow the Descriptive/Analytic tables as summarised in `bpmn-2-conformance-gap.md` (Tables 2.1 and 2.2, pp. 3-6); they were not re-checked against the PDF here and must be confirmed in review (WP2). Common Executable and Full Process Modeling are explicitly deferred.

## 1. Selection rule

A declaration is **required** when it is one of:

1. A top-level element named in one of the sets below, together with its complexType and every attribute/child declaration owned by that type or by its `extension` base chain (so `tBaseElement` id/documentation/extensionElements are required through every selected element).
2. A simpleType used as the type of a selected attribute (7 enums).
3. The BPMNDI/DI/DC declarations for the diagram objects in set I.

Sets (XSD names), **frozen by WP2** (the `REQUIRED_SETS` dict in `scripts/generate_bpmn20_inventory.py` is authoritative):

- **D, Descriptive:** definitions, process, collaboration, participant, laneSet, lane, childLaneSet, sequenceFlow, messageFlow, task, userTask, serviceTask, subProcess, callActivity, exclusiveGateway, parallelGateway, startEvent, endEvent, messageEventDefinition, timerEventDefinition, terminateEventDefinition, dataObject, dataStore, dataStoreReference, dataInput, dataOutput, ioSpecification, inputSet, outputSet, dataInputAssociation, dataOutputAssociation, textAnnotation, association, group, category, categoryValue, documentation.
- **A, Analytic (adds):** conditionExpression, message, sendTask, receiveTask, standardLoopCharacteristics, multiInstanceLoopCharacteristics, inclusiveGateway, eventBasedGateway, intermediateCatchEvent, intermediateThrowEvent, boundaryEvent, linkEventDefinition, errorEventDefinition, escalationEventDefinition, signalEventDefinition, conditionalEventDefinition.
- **C, Choreography:** choreography, choreographyTask, subChoreography, callChoreography, globalChoreographyTask, participantMultiplicity.
- **I, Interchange:** BPMNDiagram, BPMNPlane, BPMNShape, BPMNEdge, BPMNLabel, Bounds, Point, Waypoint and their attributes.

WP2 change log (against Tables 2.1/2.2): removed manualTask, scriptTask, script, businessRuleTask, dataObjectReference (not in the tables; reinstate dataObjectReference if tool practice is preferred), loopCardinality, completionCondition (still required as children of tMultiInstanceLoopCharacteristics via the type rule). Moved D to A: conditionExpression, message, intermediateCatchEvent, intermediateThrowEvent. Kept: boundaryEvent and link/error/escalation/signal/conditional definitions in A; dataStore, data I/O substructure, category/categoryValue, structural containers and documentation in D; callChoreography (contested, sec 2.5.2 does not name it) and participantMultiplicity in C. cancelEventDefinition stays out of scope (flag if choreography event coverage widens).

Not required (deferred): conversation family, adHocSubProcess/transaction, complexGateway, compensation/cancel/multiple event definitions, resources/roles, interfaces/operations/itemDefinitions/assignments, correlations, imports, WSDL/XPath bindings. Optional bucket the user may add to the gate: conversation/subConversation/conversationLink (already has a round-trip profile).

### Required dimensions per declaration

| Declaration | Required dimensions |
|---|---|
| complexType, simpleType | model, import, export, validate |
| Element/attribute with no notation | model, edit, import, export, validate |
| Top-level element with notation (flow nodes, flows, pool/lane, data objects/stores, artifacts, choreography activities) and attributes that change notation (name, isExpanded, isMarkerVisible, isMessageVisible, messageVisibleKind, participantBandKind, isHorizontal, cancelActivity, isCollection, loopType, sequential, default, instantiate, eventGatewayType, isForCompensation, triggeredByEvent, initiatingParticipantRef) | the above + render |
| BPMNDI/DI/DC | model, render, import, export (edit is covered by the semantic element) |

## 2. Resulting count

| Measure | Value |
|---|---|
| Required declarations | **299** of 580 |
| Required cells | **1450** of 3480 |
| Currently `partial` | **288** |
| Currently `unverified` | **1162** |
| Verified | 0 |

Set membership (overlapping through inheritance): D 173, A 89, C 34, I 33, S 7 declarations (cells: D 861, A 438, C 166, I 132, S 28). Counts are produced by `python scripts/generate_bpmn20_inventory.py --gate` and stored in `required_scope` of the coverage JSON (pre-WP2 baseline with the unfrozen lists: 313 / 1519 / 306 / 1213).

**Superseded:** the embedded script below was the reference for WP1 and is now implemented by the generator (`REQUIRED_SETS`, `required`, `required_dims`, `required_scope`, `--gate`); it is kept for history only and its set lists are out of date.

```python
import json,re,collections,sys
import xml.etree.ElementTree as ET
X="{http://www.w3.org/2001/XMLSchema}"
d=json.load(open('docs/research/bpmn-2-coverage.json'))
D=d['declarations']
base={}
for n in ET.parse('tests/fixtures/bpmn20-schema/Semantic.xsd').getroot().iter(X+'complexType'):
    for e in n.iter(X+'extension'):
        base[n.get('name')]=e.get('base').split(':')[-1];break
DESC="""definitions process collaboration participant laneSet lane childLaneSet sequenceFlow conditionExpression messageFlow
task manualTask userTask serviceTask scriptTask script subProcess callActivity
exclusiveGateway parallelGateway startEvent intermediateCatchEvent intermediateThrowEvent endEvent
messageEventDefinition timerEventDefinition terminateEventDefinition message
dataObject dataObjectReference dataStore dataStoreReference dataInput dataOutput ioSpecification inputSet outputSet dataInputAssociation dataOutputAssociation
textAnnotation association group category categoryValue documentation""".split()
ANA="""sendTask receiveTask businessRuleTask standardLoopCharacteristics multiInstanceLoopCharacteristics loopCardinality completionCondition
inclusiveGateway eventBasedGateway boundaryEvent linkEventDefinition errorEventDefinition escalationEventDefinition signalEventDefinition conditionalEventDefinition""".split()
CHO="""choreography choreographyTask subChoreography callChoreography globalChoreographyTask participantMultiplicity""".split()
DI="BPMNDiagram BPMNPlane BPMNShape BPMNEdge BPMNLabel Bounds Point Waypoint".split()
sets={'D':DESC,'A':ANA,'C':CHO}
elt={x['name']:x for x in D if x['path']==f"/schema/element[@name={x['name']}]" and x['kind']=='element'}
def tname(n):
    e=elt.get(n); t=e['type'].split(':')[-1] if e and e['type'] else 't'+n[0].upper()+n[1:]
    return t
req={}  # path -> set tags
for tag,names in sets.items():
    for n in names:
        t=tname(n); chain=[]
        while t: chain.append(t); t=base.get(t)
        if n in elt: req.setdefault(elt[n]['path'],set()).add(tag)
        for x in D:
            if any(x['path'].startswith(f"/schema/complexType[@name={c}]") and x['schema']=='Semantic.xsd' for c in chain):
                req.setdefault(x['path'],set()).add(tag)
for x in D:
    if x['schema'] in('BPMNDI.xsd','DI.xsd','DC.xsd') and any(x['path'].startswith(f"/schema/{k}[@name={n}]") or x['path'].startswith(f"/schema/complexType[@name={n}]") or x['path'].startswith(f"/schema/complexType[@name=BPMN{n}]") for k in('element',) for n in DI):
        req.setdefault(x['path'],set()).add('I')
R=[x for x in D if x['path'] in req]
# referenced simple types
st={x['name']:x for x in D if x['kind']=='simpleType' and x['schema']=='Semantic.xsd'}
for x in list(R):
    t=(x['type'] or '').split(':')[-1]
    if t in st and st[t] not in R: R.append(st[t]); req[st[t]['path']]={'S'}
VIS={'name','isExpanded','isMarkerVisible','isMessageVisible','messageVisibleKind','participantBandKind','isHorizontal','cancelActivity','isCollection','loopType','sequential','bpmnElement','x','y','width','height','isInterrupting','default','initiatingParticipantRef','instantiate','eventGatewayType','gatewayDirection','isForCompensation','triggeredByEvent'}
NOTATION=set(DESC+ANA+CHO)-{'definitions','script','conditionExpression','documentation','childLaneSet','dataInputAssociation','dataOutputAssociation','ioSpecification','inputSet','outputSet','loopCardinality','completionCondition','category','categoryValue','message','messageEventDefinition','timerEventDefinition','terminateEventDefinition','linkEventDefinition','errorEventDefinition','escalationEventDefinition','signalEventDefinition','conditionalEventDefinition','participantMultiplicity','collaboration','process','choreography','globalChoreographyTask'}
def dims(x):
    k=x['kind'];n=x['name']
    if k in('complexType','simpleType'): return ['model','import','export','validate']
    ds=['model','edit','import','export','validate']
    if x['schema'] in('BPMNDI.xsd','DI.xsd','DC.xsd'): return ['model','render','import','export']
    if (k=='element' and n in NOTATION and x['path'].count('/')==2) or (k=='attribute' and n in VIS): ds.append('render')
    return ds
cells=collections.Counter();decl=collections.Counter()
for x in R:
    ds=dims(x); decl[x['schema'],x['kind']]+=1
    for dm in ds: cells[x['coverage'][dm]]+=1
print('declarations',len(R),dict(collections.Counter(x['kind'] for x in R)))
print('cells',sum(cells.values()),dict(cells))
print('by set',collections.Counter(t for p in req for t in req[p]))
print('distinct decls all',len(D),'cells all',len(D)*6)
print('declarations with any partial cell among required:',sum(any(x['coverage'][dm]=='partial' for dm in dims(x)) for x in R))
```

## 3. Meaning of "verified" and the schema change

A cell is **verified** only when it cites evidence that fails if the behaviour breaks:

| Dimension | Verified means (evidence pointer) |
|---|---|
| model | Typed field in `processcraft-core`, with a core test constructing and reading it; opaque-source-only storage does not count. |
| edit | A `Mutation` (present in `COMMAND_DESCRIPTORS`) sets/clears it, with a command test including undo, plus a GUI or CLI path test. |
| render | A processcraft-render scene test or golden/PDF check for the notation, plus hit-testing where applicable. |
| import | An `interchange.rs` test importing a fixture in `tests/fixtures/**` (with manifest entry) into the typed model, not preserved-source. |
| export | An export test whose output validates against the pinned XSD (`scripts/check_bpmn20_xsd.py`) and re-imports equal. |
| validate | A negative fixture or core validation test with a diagnostic referencing the clause. |

Preserved-source never counts. Evidence is `path` or `path:test_name`, existing files only.

Minimal schema change to `bpmn-2-coverage.json`:

- Per declaration: `"required": ["D","A"]` (sets; absent when not required) and `"required_dims": [...]`.
- Cell value becomes `unverified | partial | verified`; add per-declaration `"evidence": {"model": ["crates/...:test"], ...}`, mandatory for `verified`.
- Generator: move the section 2 script in (set lists in a `REQUIRED_SETS` dict). Hand-recorded states and evidence live in a sidecar `docs/research/bpmn-2-evidence.json` so regeneration preserves them. `--check` fails if a verified cell lacks an existing evidence path; a new `--gate` exits non-zero while any required cell is not verified and prints counts per state. Keep `conformance.status` as `not_assessed`; add a `required_scope` summary with counts.

## 4. External gates: user decides

- **Option A, M3 gates:** reopen the shipped fixtures (Descriptive/Analytic + choreography) in named Camunda Modeler and SAP Signavio versions, and a screen-reader pass (NVDA/Narrator on the egui build), recorded as evidence. Pros: matches the plan's comparative-quality intent and the "independent consumer" gap. Cons: needs third-party/licensed tools, egui accessibility support is uncertain, not reproducible in CI; M3 can stay open on external factors.
- **Option B, deferred (suggested default for closing M3):** the M3 gate is the 1519 cells verified by in-repo evidence plus the XSD and existing Python cross-checks. Camunda/Signavio reopen and screen reader move to a named post-M3 qualification milestone; the evidence file records them as `not attempted`, and no interoperability or accessibility claim is made until done.
- **Hybrid:** Camunda Modeler reopen (free, scriptable) in M3; Signavio and screen reader deferred.

## 5. Work packages (ordered)

1. **WP1 (M)** Schema + generator change, sidecar evidence file, `--gate`, set lists. Baseline report (306 / 1213 / 0).
2. **WP2 (S)** Review and freeze the set lists against PDF Tables 2.1, 2.2 and the choreography section; regenerate counts.
3. **WP3 (L)** Verify the 306 partial cells first: map existing tests (interchange.rs, qualify scripts) to cells; fill gaps found.
4. **WP4 (M)** Foundation: BaseElement, definitions, documentation, extensionElements (model/edit/import/export/validate).
5. **WP5 (L)** Descriptive core: process, participants, lanes, tasks, subprocess, call, none/message/timer/terminate events, exclusive/parallel gateways, sequenceFlow, messageFlow.
6. **WP6 (L)** Data and artifacts: I/O specification, sets, associations, data objects/stores/references, annotation, association, group/category.
7. **WP7 (L)** Analytic: conditional/default flow, loop/multi-instance, inclusive/event-based gateways, link/error/escalation/signal/conditional/boundary events (interrupting and not).
8. **WP8 (L)** Choreography: Task/Sub/Call/Global, participant multiplicity, initiating participant, message visibility, remaining constraints, event association depiction.
9. **WP9 (M)** DI (BPMNDI/DI/DC) render/import/export evidence for all of the above, both themes, SVG/PDF checks.
10. **WP10 (M)** Validation: clause-referenced diagnostics and negative fixtures for every required validate cell.
11. **WP11 (S)** Update `m3-evidence.md`, `bpmn-2-conformance-gap.md` and plan checkboxes; run `--gate` and the three quality gates.
12. **WP12 (S to M)** External gates per section 4 decision (Option A or hybrid only).
