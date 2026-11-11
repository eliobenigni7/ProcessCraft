# BPMN 2.0.2 modeling and interchange requirements matrix

This is a construct-level working crosswalk, continuing the [foundation matrix](bpmn-2-foundation-matrix.md). Sources are the pinned PDF and XSD/CMOF files; page references are printed PDF pages. It is an audit and continuation record, not a complete mapping of every inherited property or normative sentence. Rows group closely related properties; expand them as each is implemented and verified. No overall conformance claim follows from a row marked partial.

Columns are model, edit/create/delete, render/hit-test, import, export and validate. **P** is partial evidence for a subset, **U** is unverified across the row, **S** is bounded protected-source retention (not editing), and **—** denotes no separate prescribed glyph for the property. P in one dimension does not imply P in another. Evidence names refer to tests in `crates/processcraft-io/tests/interchange.rs` unless stated otherwise; they establish only the fixture/profile in the test. Native interaction, independent consumers, and comprehensive semantics remain unverified. Unsupported XML can be refused instead of retained when bounded importer limits are exceeded.

## Events and control flow

| Requirement | Normative source / constraints | Model | Edit | Render | Import | Export | Validate | Bounded evidence and gap |
|---|---|---|---|---|---|---|---|---|
| Event base, catch and throw | §10.5.1, Tables 10.81–10.83, 10.110–10.111; inherited FlowNode/BaseElement | P | P | P | P; S | P; S | P | Selected event kinds and inline definitions; event data inputs/outputs, associations and complete referenced definitions open. |
| Top-level Start Event | §10.5.2, Tables 10.84, 10.87, 10.119 | P | P | P | P; S | P; S | P | `start_timer_round_trips`; incoming/conditional outgoing diagnostics exist. Complete valid trigger and data context remains open. |
| Embedded and event-subprocess starts | §10.5.2, Tables 10.85–10.87 | P | U | U | P; S | P; S | U | Event-subprocess trigger context and `isInterrupting` are distinct from boundary `cancelActivity`; all valid combinations and markers need tests. |
| End Event | §10.5.3, Tables 10.88, 10.121 | P | P | P | P; S | P; S | P | `end_terminate_definition_round_trips`; no outgoing flow diagnostic. Context-specific result rules remain open. |
| Intermediate catch and throw | §10.5.4, Table 10.89; Tables 10.113–10.114 | P | P | P | P; S | P; S | P | `catch_timer_details_round_trip`, `conditional_catch_expression_round_trips`; definition/context matrix is incomplete. |
| Boundary attachment and interruption | §10.5.4, Tables 10.90–10.91, 10.102 | P | P | P | P; S | P; S | P | `boundary_timer_attachment_and_cancel_round_trip`; attachment, valid triggers, scope and interrupting/non-interrupting combinations need full qualification. |
| None, multiple and parallel-multiple | §10.5.5, Table 10.93 | P | U | U | P; S | P; S | U | No definition means None; multiple definitions and parallelMultiple behavior, allowed contexts and correct markers remain open. |
| Inline versus root/referenced definitions | §10.5.1, Tables 10.82–10.83; §10.5.5, Table 10.111 | P | U | U | P; S | P; S | P | Selected inline definitions and root message/signal/error/escalation references exist; referenced EventDefinition roots and complete QName ownership remain open. `event_reference_to_wrong_root_kind_cannot_become_editable`. |
| Message | §10.5.5, Table 10.99 and 10.116 | P | P | P | P; S | P; S | P | `message_and_signal_references_round_trip_or_preserve`; message/operation associations and catch/throw contexts remain incomplete. |
| Timer | §10.5.5, Tables 10.101, 10.122 | P | P | P | P; S | P; S | P | `catch_timer_details_round_trip`, `start_timer_round_trips`; date/duration/cycle, formal-expression context and all owners require evidence. |
| Error and escalation | §10.5.5, Tables 10.96–10.97, 10.108–10.109 | P | P | P | P; S | P; S | P | `error_and_escalation_end_events_keep_typed_root_references`; catch/throw contexts, payload and code semantics remain open. |
| Conditional | §10.5.5, Tables 10.95, 10.107 | P | P | P | P; S | P; S | P | `conditional_catch_expression_round_trips`; full expression representation and allowed contexts remain open. |
| Signal | §10.5.5, Tables 10.100, 10.118 | P | P | P | P; S | P; S | P | `message_and_signal_references_round_trip_or_preserve`; external references and all contexts remain open. |
| Link | §10.5.5, Tables 10.98, 10.115 | P | P | P | P; S | P; S | P | `inline_link_cancel_and_compensation_roundtrip`; local inline link IDs, references and catch/throw glyphs have focused tests. Multiple-source pairing, all reference forms and complete context qualification remain open. |
| Cancel | §10.5.5, Tables 10.93, 10.103 | P | P | P | P; S | P; S | P | Inline transaction boundary/end profile has focused tests. Catch must attach to transaction boundary and end must be within transaction; all nested and referenced-definition contexts remain open. |
| Compensation | §10.5.5, Tables 10.94, 10.106; §10.7.2 | P | P | P | P; S | P; S | P | Inline activity reference and waitForCompletion presence round-trip; throw/boundary markers and guarded commands have focused tests. Boundary compensation is solid and the interrupting distinction does not apply. Handler/association and complete event-subprocess contexts remain open. |
| Terminate | §10.5.5, Tables 10.93, 10.120 | P | P | P | P; S | P; S | P | `end_terminate_definition_round_trips`; expanded context and semantics qualification remains open. |
| Sequence flow and condition | §8.4.13 pp. 95–97, Table 8.51; §10.6.1 | P | P | P | P; S | P; S | P | Activity-sourced conditions are allowed with another outgoing flow; Table 2.2 footnote is Analytic subclass-specific. Parallel/event gateway condition constraints and full formal expression remain open. Core `diagnostics.rs:activity_conditions_are_allowed_but_event_gateway_conditions_are_diagnosed`. |
| Gateway direction and defaults | §10.6.1–10.6.4, Tables 8.46, 10.123–10.124 | P | P | P | P; S | P; S | P | `gateway_metadata_and_default_flow_round_trip`; directed split/merge constraints and default handling require full subtype tests. |
| Complex gateway | §10.6.5, Tables 10.125, 10.128 | P | P | P | P; S | P; S | P | Simple activation condition profile only; full condition semantics and bindings are open. |
| Event-based gateway | §10.6.6, Tables 10.127, 10.129 | P | P | P | P; S | P; S | P | Selected metadata and advisory diagnostics; no outgoing conditions, allowed targets, instantiation and complete restrictions require positive/negative context tests. |

## Process, activities and artifacts

| Requirement | Normative source / constraints | Model | Edit | Render | Import | Export | Validate | Bounded evidence and gap |
|---|---|---|---|---|---|---|---|---|
| Process attributes and callable identity | §10.2, Table 10.1 | P | P | P | P; S | P; S | P | Basic process scope and identity exist; executable metadata, resources, correlations and callable associations remain incomplete. |
| Activity base properties | §10.3, Table 10.3; `tActivity` | P | P | P | P; S | P; S | P | Compensation, quantity and default-flow subsets; `activity_attributes_and_plain_script_round_trip`. Arbitrary XML integer range, bindings and complete inheritance remain open. |
| Resource roles, performers and assignments | §10.3.1–10.3.2, Tables 10.5–10.7, 10.17–10.18 | U | U | — | S | S | U | Resource-role identity, parameter bindings, potential/human owners and assignment expressions differ from analyst dictionary entries. |
| Abstract/manual task | §10.3.3, Table 10.15 | P | P | P | P; S | P; S | P | Selected task kinds; all inherited activity fields and prescribed glyphs require complete qualification. |
| Service task | §10.3.3, Tables 10.8, 10.46 | P | P | P | P; S | P; S | P | Selected operationRef QName retains local, external and empty namespaces; local wrong-kind references are rejected. Complete interface and I/O bindings remain open. |
| Send and receive tasks | §10.3.3, Tables 10.9–10.10, 10.42, 10.45 | P | P | P | P; S | P; S | P | Selected messageRef/operationRef QNames retain local, external and empty namespaces; local wrong-kind references are rejected and unresolved legacy references have an advisory. Complete task I/O and context rules remain open. |
| Business rule task | §10.3.3, Tables 10.11, 10.34 | P | P | P | P; S | P; S | P | Selected subtype metadata only; implementation bindings and inherited obligations remain open. |
| Script task | §10.3.3, Tables 10.12, 10.44 | P | P | P | P; S | P; S | P | Plain script and script format subset; scripts are data and are never executed. |
| User task and human interaction | §10.3.3–10.3.4, Tables 10.13, 10.16–10.18 | P | P | P | P; S | P; S | P | Selected implementation property; performer/resource assignment and rendering/interaction qualification remain open. |
| Subprocess and event subprocess | §10.3.5, Table 10.20, 10.48 | P | P | P | P; S | P; S | P | `nested_subprocess_call_and_expanded_geometry_round_trip`; triggeredByEvent, event-start restrictions and all inherited metadata need qualification. |
| Transaction and ad-hoc subprocess | §10.3.5, Tables 10.21–10.22, 10.33, 10.50 | P | P | P | P; S | P; S | P | `ad_hoc_and_transaction_properties_round_trip`; protocol, ordering, cancellation and completion expression semantics incomplete. |
| Call activity and callable/global tasks | §10.3.6–10.3.7, Tables 10.23–10.26, 10.35–10.39 | P | P | P | P; S | P; S | P | Local call target subset; external callable identity, I/O bindings and global subtype properties remain open. |
| Standard loop | §10.3.8, Tables 10.28, 10.47 | P | P | P | P; S | P; S | P | `activity_loop_characteristics_round_trip`; complete condition metadata, maximum and defaults remain open. |
| Multi-instance loop and complex behavior | §10.3.8, Tables 10.29, 10.31, 10.41 | P | P | P | P; S | P; S | P | Simple sequential/parallel metadata; input/output collections, assignment, complex behavior and conditions need coverage. |
| Text annotation | §8.4.1, Tables 8.24, 8.30 | P | P | P | P; S | P; S | P | Plain text profile; textFormat, inherited documentation, markup and complete DI open. |
| Association | §8.4.1, Tables 8.20, 8.26 | P | P | P | P; S | P; S | P | Selected direction/endpoints; full endpoint legality and geometry/labels remain open. |
| Group, category and category value | §8.4.1, Tables 8.21–8.23, 8.27–8.29 | P | P | P | P; S | P; S | P | `descriptive_artifacts_and_nested_lanes_round_trip`; associations, multiple values and complete UI/DI require qualification. |

## Collaboration, choreography, data and visual interchange

| Requirement | Normative source / constraints | Model | Edit | Render | Import | Export | Validate | Bounded evidence and gap |
|---|---|---|---|---|---|---|---|---|
| Collaboration and participants | §9.2, §9.3.1; Tables 9.1–9.2 | P | P | P | P; S | P; S | P | Basic pools/participants; complete references, process ownership and attributes open. |
| Partner entity/role, multiplicity, participant association | §9.3.1, Tables 9.3–9.7 | U | U | U | S | S | U | Distinct identities and relationship/multiplicity constraints require end-to-end implementation. |
| Lanes and nested lane sets | §9.3.2; §12.4.2 | P | P | P | P; S | P; S | P | `descriptive_artifacts_and_nested_lanes_round_trip`, `generated_lane_set_ids_avoid_collisions_and_preserve_nested_lanes`; full metadata/orientation open. |
| Message flow and associations | §9.4, Tables 9.8–9.9 | P | P | P | P; S | P; S | P | Selected flows; messageRef, endpoint legality, associations and scopes need complete checks. |
| Conversation and sub-conversation | §9.5.1–9.5.3, Tables 9.10–9.11 | P | P | P | P; S | P; S | P | `collaboration_conversation_and_links_round_trip`; nested ownership, participant/message references and navigation are partial. |
| Call/global conversation and associations | §9.5.4–9.5.5, §9.5.8, Tables 9.14–9.15, 9.22 | U | U | U | S | S | U | Called references, participants and correlation bindings need qualified implementation. |
| Conversation link | §9.5.6, Table 9.13 | P | P | P | P; S | P; S | P | Selected links; complete endpoint legality and connected geometry remain open. |
| Correlation keys/properties/subscriptions | §8.4.2, Tables 8.31–8.40; §9.5.8 | U | U | — | S | S | U | Retrieval and binding expressions, ownership and message references need complete coverage. |
| Choreography task and owned participants | §11.5, Tables 11.1–11.4 | P | P | P | P; S | P; S | P | `choreography_task_and_data_references_round_trip`; local ownership and complete participant/message constraints remain partial. |
| Sub/call/global choreography | §11.5.2–11.5.4; §12.3.2 | U | U | U | S | S | U | Nested scope, collapsed/expanded DI, callable bindings and participant ownership require implementation. |
| Choreography events/gateways/flows | §§11.6–11.7; §11.4 | U | U | U | S | S | U | Context-specific allowed constructs, participant visibility, conditions and flow constraints require separate process-independent checks. |
| Choreography participant bands/message visibility | §12.2.3, §12.3.2; Table 12.33 | P | U | P | P; S | P; S | U | Existing task profile is limited; all bands, ordering, multiplicity and visibility metadata remain open. |
| Data object/store and references | §10.4.1; Tables 10.51–10.57 | P | P | P | P; S | P; S | P | `choreography_task_and_data_references_round_trip`; item-aware types, data state and scope remain partial. |
| I/O specification, ports and sets | §10.4.1, Tables 10.58–10.64 | P | P | P | P; S | P; S | P | `data_io_sets_ports_and_associations_round_trip`; all owners, relationships, attributes and scoped reference semantics remain open. |
| Data associations, transformations, assignments | §10.4.1, Tables 10.63–10.64, 10.69–10.70 | P | P | P | P; S | P; S | P | Simple associations only; `invalid_data_ports_and_unmodeled_assignments_cannot_become_editable`; transformation/assignment expressions remain protected or refused. |
| Diagram, plane and ownership | §12.2.3, Tables 12.1–12.2 | P | P | P | P; S | P; S | P | Multiple diagrams and selected planes supported; semantic versus representation identity and all ownership rules remain open. |
| Shapes, bounds, orientation and expansion | §12.2.3, Table 12.3; §12.3.2 | P | P | P | P; S | P; S | P | Selected bounds and optional expansion; `nested_subprocess_call_and_expanded_geometry_round_trip`; orientation and marker presence need qualification. |
| Edges, waypoints, endpoints and labels | §12.2.3, Tables 12.4–12.5; §12.3.1 | P | P | P | P; S | P; S | P | Selected geometry and labels; core endpoint-move tests cover geometry mechanics, not permission to depict activity-local port shapes. §12.3.2 permits port rendering only for Processes. Table 12.35 alternate DataAssociation depiction requires BPMNEdge sourceElement/targetElement support, currently unmodeled; clipping, hit testing and other geometry updates remain open. |
| Fonts, labels and styles | §12.2.3, Table 12.6; §12.3.1 | P | P | P | P; S | P; S | U | Full source style, label bounds, shared-scene rendering and theme behavior require explicit no-loss tests. |

The crosswalk records requirements and current limits; it does not certify normative correctness or establish that all test pointers passed in the current environment. Continue with individual attributes, inheritance and positive/negative examples, and link actual native, XML schema and independent-consumer evidence before marking any required cell complete.

The inline event profile is bounded by `inline_link_cancel_and_compensation_roundtrip`, `inline_events_preserve_unrepresented_or_invalid_content`, `compensation_wait_attribute_presence_roundtrips`, core `link_and_compensation_events_validate_atomically`, native `compensation_event_capability_covers_live_and_checkpoint_documents`, and renderer tests `link_markers_distinguish_catch_and_throw_in_shared_scene`, `cancel_boundary_and_end_use_hollow_and_filled_crosses`, and `compensation_boundary_and_throw_render_distinct_markers`. These are test pointers, not complete semantic, GUI or independent-consumer qualification.

## Data inheritance and property continuation

Each row inherits the six partial/unverified dimensions in the data family above; this table expands obligations, not capability claims. References to XSD are to pinned `Semantic.xsd` and its declared base types. PDF references are §10.4.1 and the specified tables.

| Construct/property | Inheritance and requirement | Acceptance still needed |
|---|---|---|
| ItemAwareElement | Table 10.51; itemSubjectRef identifies ItemDefinition and dataState records state | Namespace-aware type references, optional state and inherited BaseElement identity/documentation for every applicable owner. |
| DataObject | Table 10.52, `tDataObject` → `tFlowElement` → `tBaseElement`; dataState, itemSubjectRef QName, isCollection default false | Preserve explicit/omitted default, scope and state; no identity merge by name. |
| DataObjectReference | Table 10.53, `tDataObjectReference` → `tFlowElement`; dataObjectRef plus item-aware context | Correct local target kind and scope, label/DI and referenced collection state; native and interchange negative cases. |
| DataState | Table 10.54; `tDataState` → `tBaseElement` | Preserve state name and inherited metadata across every owner. |
| DataStore | Tables 10.55–10.56, `tDataStore` → `tRootElement`; capacity is `xsd:integer`, isUnlimited defaults true, itemSubjectRef QName, optional state | Preserve arbitrary integer lexical range and explicit default presence; do not narrow to unsigned machine integers. |
| Property | Table 10.57, `tProperty` → `tBaseElement` | Complete item-aware metadata, owner scope and identity. |
| InputOutputSpecification | Table 10.58, `tInputOutputSpecification` → `tBaseElement` | All owner scopes, input/output ports and set membership; consistent deletion and references. |
| DataInput and DataOutput | Tables 10.59–10.60; each extends `tBaseElement`, includes optional state/name, itemSubjectRef QName and isCollection default false | Preserve QName context and omission, identity, owner, state and port relationships. |
| InputSet and OutputSet | Tables 10.61–10.62; each extends `tBaseElement`; lists of port IDs, optional/while-executing subsets and opposite-set IDs | Validate correct scope, target kind and membership, including empty/optional combinations; distinguish IDREF from QName on export. |
| DataAssociation | Table 10.63, `tDataAssociation` → `tBaseElement`; zero or more sourceRef IDREFs, exactly one targetRef, optional transformation and repeated assignment | Preserve source ordering and all endpoints, transforms/assignments and scoped references; geometry must follow each qualified endpoint move. |
| Assignment | Table 10.64, `tAssignment` → `tBaseElement`; required from/to expressions | Preserve complete expression context and identity; no evaluation during editing/import/export. |

The current bounded basic I/O tests are `data_io_sets_ports_and_associations_round_trip` and `invalid_data_ports_and_unmodeled_assignments_cannot_become_editable`; they do not establish completion of these individual properties.

## Shared flow and callable inheritance continuation

These shared obligations apply in addition to each concrete activity, gateway, event or flow row above. Their presence in the schema does not establish full support; all six dimensions remain partial or unverified as recorded for each descendant.

| Base construct | Pinned schema and PDF | Audit obligations |
|---|---|---|
| FlowElement | `tFlowElement` → `tBaseElement`; §8.4.7, Table 8.44 | Preserve optional name, auditing and monitoring, categoryValueRef QNames and inherited identity/documentation/extensions. Table 8.44 confines audit/monitoring definitions to a Process; check owner context rather than accepting arbitrary placement. |
| FlowNode | `tFlowNode` → `tFlowElement`; §8.4.14, Tables 8.52, 8.57 | Reconcile incoming/outgoing QName declarations with actual sequence-flow endpoints and scope; preserve source namespace identity or safely refuse unsupported declarations. |
| CallableElement | `tCallableElement` → `tRootElement`; §10.3.6, Tables 10.24–10.25, 10.35 | Preserve name, supportedInterfaceRef QNames, optional I/O specification and repeated I/O bindings; validate references and scope without loading external services. |
| CallActivity | `tCallActivity` → `tActivity`; §10.3.6, Tables 10.23, 10.36 | Preserve calledElement QName and inherited activity metadata; local and external callable targets require distinct resolution and safe handling. |
| SequenceFlow | `tSequenceFlow` → `tFlowElement`; §8.4.13, Tables 8.51, 8.64 | sourceRef/targetRef are IDREFs (unqualified IDs), unlike QName references elsewhere. Preserve optional isImmediate and condition expression presence; validate endpoint/scope and context-specific rules. |

Send and Receive task message references denote a sent and received message respectively (Tables 10.9–10.10). Those tables do not state a general equality rule between `messageRef` and an operation's input or output message. The specific item-definition and input/output-set constraints on p. 158–160 require separate checks; do not manufacture an operation-direction constraint.

Task-binding evidence: `activity_message_and_operation_qnames_roundtrip_with_loop_context`, `activity_unqualified_qname_and_wrong_subtype_are_safe`, and `activity_unqualified_operation_with_loop_and_documentation_roundtrips` in interchange tests; `activity_bindings_validate_new_local_refs_and_preserve_legacy` in core commands. These cover bounded reference behavior, not all task subtype metadata or semantics.

## Data property status and rule checklist

The following cells identify the current bounded profile independently for each property. P means a tested subset, U means the entire property is unverified, S means protected-source fallback for unsupported content; — means no independent glyph. Do not infer full support from P. References are to §10.4.1 and pinned `Semantic.xsd`; changes in the active data implementation require new evidence before these statuses are advanced.

| Property or relation | Source / normative distinction | Model | Edit | Render | Import | Export | Validate | Outstanding acceptance |
|---|---|---|---|---|---|---|---|---|
| Item subject reference | Table 10.51; QName to ItemDefinition | P | P | — | P; S | P; S | P | Selected owners preserve QName context; scope/kind and all owner paths remain open. |
| Data state identity and name | Tables 10.51, 10.54 | P | P | U | P; S | P; S | P | Inherited identity/documentation and reference-specific state label. PDF p. 204 says DataObjects cannot specify state, while p. 205 says they can and pinned XSD permits it; document serialization and avoid dropping valid source. |
| DataObject collection | Table 10.52; default false; must match referenced ItemDefinition collection | P | P | P | P; S | P; S | P | Explicit presence, item-type match and all owners remain open; renderer has a collection-marker test. |
| Data object/reference identity and scope | Tables 10.52–10.53; pp. 204–207 | P | P | P | P; S | P; S | P | Multiple representations and ancestor/sibling accessibility need full tests; don't merge identities by name. |
| DataStore capacity / unlimited | Tables 10.55–10.56; `xsd:integer`, default `isUnlimited=true` | U | U | U | S | S | U | Arbitrary integer range, default presence, state and item type. |
| Port name, ID and ownership | Tables 10.58–10.60 | P | P | P | P; S | P; S | P | Process-port depiction is optional; §12.3.2 p. 392 limits Data Input/Output rendering to Processes. Activity-local port shapes are skipped/diagnosed, source preserved or interchange refused, and native shapes retained; semantic ownership and optional identities require broader tests. |
| Port type, state and collection | Tables 10.51, 10.59–10.60 | P | P | U | P; S | P; S | P | Match collection to referenced type when determinable, preserve omission and state. |
| Input/output sets and base membership | Tables 10.61–10.62 | P | P | — | P; S | P; S | P | Every input/output belongs to at least one corresponding set; preserve scoped IDs. |
| Optional and while-executing set subsets | Tables 10.61–10.62 | P | P | — | P; S | P; S | P | Each such reference must also be present in its base input/output list. |
| Paired input/output set references | Tables 10.61–10.62 | P | P | — | P; S | P; S | P | Pair reciprocal relationships and verify scoped target kinds. |
| DataAssociation source/target | Table 10.63; source IDREF 0..*, target IDREF exactly one | P | P | P | P; S | P; S | P | With no transformation exactly one source; with transformation zero or more. Test owner/lifecycle scope and all routes. |
| DataAssociation transformation | Table 10.63, p. 221–222; formal expression | P | P | — | P; S | P; S | P | Preserve expression context and payload; match source/target item types or provide transformation. Never evaluate. |
| Assignment from/to | Table 10.64; required expressions | P | P | — | P; S | P; S | P | Ordered assignments, owner identity, complete expression content and failure rollback. Never execute. |
| Service task input/output mapping | §10.4.1 p. 216 | U | U | — | S | S | U | For associated operation, required message input type must match its input Message; output condition/type follows operation output. Avoid guessing task messageRef equality. |
| Send/receive task data mapping | §10.4.1 pp. 216–217, Tables 10.9–10.10 | U | U | — | S | S | U | At most one respective set/port when associated with Message; if port exists its item definition matches the Message. |

### Data scope and owner rules

- A DataObject belongs to a Process or Sub-Process. Its immediate parent and sibling FlowElements, including their descendants, can access it; a child activity can therefore access an ancestor-scope object. A local object in one subprocess is not thereby available to an unrelated sibling subprocess. The examples on pp. 206–207 make this hierarchy explicit. Property visibility is similarly tied to its owning Process/Sub-Process or FlowElement (pp. 209–210).
- Tasks and CallableElements, including Processes and Global Tasks, may define an I/O specification. Embedded Sub-Processes must not directly define Data Inputs or Data Outputs, although multi-instance characteristics can define them indirectly (p. 210). A top-level Process Data Input must not be the target of an underlying DataAssociation; a top-level Process Data Output must not be its source (pp. 212, 215). A called-process port may be drawn across a CallActivity boundary, but the underlying association addresses the CallActivity port.
- DataInputAssociation sources and DataOutputAssociation targets may be accessible ItemAwareElements in their execution scope (p. 223); selection must respect direction, lifetime and ownership. Selected ancestor-scope data objects/references and process ports are supported; the current association owner must be an activity with I/O ports, leaving event-owned associations and Property endpoints outside this profile.
- The PDF explicitly permits a DataStoreReference as association source or target (p. 207). The CMOF also declares global DataStore an ItemAwareElement, and Table 10.63 uses ItemAwareElement for endpoints. A categorical ban on direct global DataStore endpoints is not established by these passages; retain or refuse unsupported forms safely until their complete mapping is established.

The active optional-data-identity migration represents absent IDs on data structures and optional names where supported. `data_elements_allow_optional_xml_ids_and_process_io` tests a bounded process I/O and absent-ID import/export case. Existing node names are not optional in the model, so a DataObjectReference/DataStoreReference whose name is omitted must remain safely preserved until that distinction is represented. These pointers do not establish that every optional ID/name, owner or migration path has been qualified.

Expanded-data bounded evidence: `item_aware_metadata_transformations_and_assignments_roundtrip`, `data_elements_allow_optional_xml_ids_and_process_io`, `data_store_qualified_and_no_namespace_references_keep_identity`, `unsupported_data_state_metadata_and_invalid_assignments_are_preserved` in interchange tests; `expanded_data_capability_covers_live_and_checkpoint_documents` in native tests. Coordinator reports 65 interchange tests and the focused native capability test passed. Complete renderer/UI qualification and event-owned associations and Property endpoints remain open; see M3 evidence for execution details.

Naming distinction: §10.4.1 p. 204 derives a DataObjectReference name from the referenced DataObject name and reference state; its inherited optional XML `name` still needs lossless preservation. The DataStoreReference discussion on pp. 207–208 does not state the same derivation; it inherits optional FlowElement `name`, and Table 2.1 lists that attribute. Do not override an explicitly present DataStoreReference name merely by applying the DataObjectReference display rule. Omitted versus empty names remains a separate identity/presence gap.

The post-checkpoint DataStoreReference correction is documented in [M3 evidence](../implementation/m3-evidence.md): source-value assertions now check `dataStoreRef` QName, rather than comparing only post-import values.

Data port depiction correction: §12.3.2 p. 392 makes Data Input/Output rendering optional and permits it only for Processes. Existing activity-port geometry fixtures are not evidence of normative visual support. Table 12.35 pp. 410–411 requires a representation-level BPMNEdge targetElement for the alternate association-to-sequence-flow depiction; sourceElement/targetElement are not currently modeled.
