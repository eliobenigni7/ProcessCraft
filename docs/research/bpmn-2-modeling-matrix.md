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
| Service task | §10.3.3, Tables 10.8, 10.46 | P | P | P | P; S | P; S | P | Subtype implementation profile only; interface/operation and I/O bindings are open. |
| Send and receive tasks | §10.3.3, Tables 10.9–10.10, 10.42, 10.45 | P | P | P | P; S | P; S | P | Selected implementation/instantiate metadata; operation/message references and context rules remain open. |
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
| Edges, waypoints, endpoints and labels | §12.2.3, Tables 12.4–12.5; §12.3.1 | P | P | P | P; S | P; S | P | Selected geometry and labels; core `valid_data_association_endpoints_move_undo_and_redo` and `valid_conversation_link_endpoints_move_undo_and_redo` cover qualified endpoint moves. Complete representation endpoints, clipping, hit testing and other geometry updates remain open. |
| Fonts, labels and styles | §12.2.3, Table 12.6; §12.3.1 | P | P | P | P; S | P; S | U | Full source style, label bounds, shared-scene rendering and theme behavior require explicit no-loss tests. |

The crosswalk records requirements and current limits; it does not certify normative correctness or establish that all test pointers passed in the current environment. Continue with individual attributes, inheritance and positive/negative examples, and link actual native, XML schema and independent-consumer evidence before marking any required cell complete.

The inline event profile is bounded by `inline_link_cancel_and_compensation_roundtrip`, `inline_events_preserve_unrepresented_or_invalid_content`, `compensation_wait_attribute_presence_roundtrips`, core `link_and_compensation_events_validate_atomically`, native `compensation_event_capability_covers_live_and_checkpoint_documents`, and renderer tests `link_markers_distinguish_catch_and_throw_in_shared_scene`, `cancel_boundary_and_end_use_hollow_and_filled_crosses`, and `compensation_boundary_and_throw_render_distinct_markers`. These are test pointers, not complete semantic, GUI or independent-consumer qualification.
