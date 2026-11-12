"""Generate the BPMN normative declaration inventory from pinned OMG schemas."""
import hashlib
import json
import re
import sys
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
FIX = ROOT / "tests/fixtures/bpmn20-schema"
OUT = ROOT / "docs/research/bpmn-2-coverage.json"
XSD = "{http://www.w3.org/2001/XMLSchema}"
files = sorted(FIX.iterdir())
# The OMG specification PDF is not redistributed (it is git-ignored); its pinned hash
# lets a local copy downloaded from the URL be checked, and keeps this inventory stable.
SPEC_PDF = {"file": "BPMN-2.0.2.pdf",
            "sha256": "d3d9258a70bf808c11ac52becea4ce645540a8b881117535f60a56fbf13e22f2",
            "url": "https://www.omg.org/spec/BPMN/2.0.2/PDF"}
pdf = FIX / SPEC_PDF["file"]
if pdf.exists() and hashlib.sha256(pdf.read_bytes()).hexdigest() != SPEC_PDF["sha256"]:
    raise SystemExit(f"{pdf} does not match the pinned OMG specification hash")
sources = [SPEC_PDF] + [{"file": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
                         "url": "https://www.omg.org/spec/BPMN/20100501/" + p.name}
                        for p in files if p.suffix in (".xsd", ".cmof")]
declarations = []
for p in files:
    if p.suffix != ".xsd":
        continue
    root = ET.parse(p).getroot()
    namespace = root.get("targetNamespace", "")
    def walk(node, path):
        name = node.get("name")
        kind = node.tag.removeprefix(XSD) if node.tag.startswith(XSD) else ""
        component = f"{kind}[@name={name}]" if name else kind
        current = path + "/" + component
        if kind in ("element", "attribute", "complexType", "simpleType", "group", "attributeGroup") and name:
            declarations.append({"schema": p.name, "namespace": namespace, "kind": kind, "name": name,
                                 "path": current, "type": node.get("type"), "ref": node.get("ref"),
                                 "min_occurs": node.get("minOccurs"), "max_occurs": node.get("maxOccurs"),
                                 "use": node.get("use"), "abstract": node.get("abstract"),
                                 "substitution_group": node.get("substitutionGroup"),
                                 "coverage": {k: "unverified" for k in ("model", "edit", "render", "import", "export", "validate")}})
        for child in node:
            walk(child, current)
    walk(root, "")
cmof = []
for p in files:
    if p.suffix != ".cmof":
        continue
    root = ET.parse(p).getroot()
    xmi = "{http://schema.omg.org/spec/XMI/2.1}"
    def visit(node, owner):
        name = node.get("name")
        item_id = node.get(xmi + "id")
        if name:
            cmof.append({"schema": p.name, "id": item_id, "owner": owner,
                         "name": name, "kind": node.get(xmi + "type", node.tag),
                         "attributes": {k: v for k, v in node.attrib.items()}})
        for child in node:
            visit(child, item_id or owner)
    visit(root, None)
profiles = {
    "descriptive artifacts": {"elements": ["textAnnotation", "association", "group", "category", "categoryValue", "laneSet", "lane"], "evidence": ["tests/fixtures/bpmn20-schema/artifacts.bpmn", "tests/fixtures/bpmn20-schema/groups.bpmn", "tests/fixtures/bpmn20-schema/nested-lanes.bpmn", "crates/processcraft-io/tests/interchange.rs:descriptive_artifacts_and_nested_lanes_round_trip", "crates/processcraft-io/tests/interchange.rs:descriptive_artifact_identity_survives_native_round_trip"], "limit": "Plain annotation, association, groups/categories and bounded nested lanes; rich text and advanced fields remain preserved-source. Full UI/render and semantics not proven."},
    "activity properties": {"elements": ["scriptTask", "serviceTask", "sendTask", "receiveTask", "userTask", "businessRuleTask"], "evidence": ["tests/fixtures/bpmn20-schema/activity.bpmn", "crates/processcraft-io/tests/interchange.rs:activity_attributes_and_plain_script_round_trip"], "limit": "Basic compensation/quantities/default, subtype implementation/instantiate and plain script only; external QName, advanced attributes and complete UI/semantics are not qualified."},
    "gateway metadata": {"elements": ["exclusiveGateway", "inclusiveGateway", "parallelGateway", "complexGateway", "eventBasedGateway"], "evidence": ["tests/fixtures/bpmn20-schema/gateways.bpmn", "crates/processcraft-io/tests/interchange.rs:gateway_metadata_and_default_flow_round_trip"], "limit": "Default outgoing flow, direction, event-based type/instantiate and simple activation condition; semantic/structural and full GUI qualification remain incomplete."},
    "event definitions": {"elements": ["timerEventDefinition", "terminateEventDefinition", "messageEventDefinition", "signalEventDefinition", "errorEventDefinition", "escalationEventDefinition", "conditionalEventDefinition"], "evidence": ["crates/processcraft-io/tests/interchange.rs:catch_timer_details_round_trip", "crates/processcraft-io/tests/interchange.rs:message_and_signal_references_round_trip_or_preserve", "crates/processcraft-io/tests/interchange.rs:error_and_escalation_end_events_keep_typed_root_references", "crates/processcraft-io/tests/interchange.rs:conditional_catch_expression_round_trips"], "limit": "Only documented inline subsets, local refs and simple expressions; semantic conformance and full GUI not established."},
    "nested activities": {"elements": ["subProcess", "adHocSubProcess", "transaction", "callActivity", "standardLoopCharacteristics", "multiInstanceLoopCharacteristics"], "evidence": ["crates/processcraft-io/tests/interchange.rs:nested_subprocess_call_and_expanded_geometry_round_trip", "crates/processcraft-io/tests/interchange.rs:activity_loop_characteristics_round_trip"], "limit": "Only local call targets, bounded nested children/lane sets, selected ad hoc/transaction metadata and simple loops; other metadata preserved-source."},
    "collaboration": {"elements": ["collaboration", "participant", "conversation", "subConversation", "conversationLink", "messageFlow"], "evidence": ["crates/processcraft-io/tests/interchange.rs:collaboration_conversation_and_links_round_trip", "tests/fixtures/bpmn20-schema/conversation.bpmn"], "limit": "Subset without unmodeled attributes; semantic conformance and full GUI not established."},
    "choreography": {"elements": ["choreography", "choreographyTask", "subChoreography", "callChoreography", "globalChoreographyTask"], "evidence": ["crates/processcraft-core/tests/choreography_graph.rs", "crates/processcraft-core/tests/choreography_graph_commands.rs", "crates/processcraft-io/tests/interchange.rs:nested_choreography_graph_round_trips", "crates/processcraft-io/tests/interchange.rs:choreography_boundary_event_io_round_trips_and_owner_order_is_stable", "crates/processcraft-io/tests/interchange.rs:sub_choreography_plane_round_trips_and_excludes_sibling_scope", "apps/processcraft-cli/tests/definitions_mcp.rs:choreography_graph_commands_use_stdio_generation_pages_and_history", "tests/fixtures/bpmn/choreography-graph/manifest.md", "scripts/qualify_choreography_graph.py"], "limit": "Bounded ordered Task/Sub/Call/Global graphs, selected event/gateway contexts, exact namespace references and event-owned I/O. Full semantic, DI, native interaction and consumer qualification remain incomplete. Table 8.45 excludes process Activities, Data Objects and Data Associations as choreography flowElements; event-owned associations are a separate child context."},
    "data": {"elements": ["dataStore", "dataObject", "dataObjectReference", "dataStoreReference", "ioSpecification", "dataInput", "dataOutput", "inputSet", "outputSet", "dataInputAssociation", "dataOutputAssociation"], "evidence": ["tests/fixtures/bpmn20-schema/data.bpmn", "tests/fixtures/bpmn20-schema/data-io.bpmn", "crates/processcraft-io/tests/interchange.rs:data_io_sets_ports_and_associations_round_trip", "crates/processcraft-io/tests/interchange.rs:invalid_data_ports_and_unmodeled_assignments_cannot_become_editable"], "limit": "Local declarations, ports, I/O sets and simple associations only; transformations, assignments, item types and nested I/O remain preserved-source."},
}
for declaration in declarations:
    if declaration["kind"] != "element" or declaration["namespace"] != "http://www.omg.org/spec/BPMN/20100524/MODEL":
        continue
    for label, profile in profiles.items():
        if declaration["name"] in profile["elements"]:
            declaration["coverage"].update({"model": "partial", "import": "partial", "export": "partial"})
            declaration["profile"] = label
            declaration["evidence"] = profile["evidence"]
            declaration["limitations"] = profile["limit"]
            break
# These are attributes of tDefinitions, not a claim about definitions as a whole.
# The optional language defaults are deliberately retained as presence information.
definitions_attributes = {"name", "targetNamespace", "expressionLanguage", "typeLanguage", "exporter", "exporterVersion"}
for declaration in declarations:
    if (declaration["schema"] == "BPMN20.xsd"
            and declaration["kind"] == "attribute"
            and "/complexType[@name=tDefinitions]/" in declaration["path"]
            and declaration["name"] in definitions_attributes):
        declaration["coverage"].update({"model": "partial", "edit": "partial", "import": "partial", "export": "partial", "validate": "partial"})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:definitions_metadata_roundtrips_with_presence_and_namespace_identity",
            "crates/processcraft-io/tests/native.rs:definitions_metadata_requires_capability_in_live_and_checkpoint_documents",
            "crates/processcraft-core/tests/commands.rs:definitions_metadata_is_undoable_and_validated_atomically",
            "apps/processcraft/src/app.rs:definitions_draft_is_guarded_and_uses_history",
            "apps/processcraft-cli/tests/definitions_mcp.rs:definitions_metadata_uses_discovered_mcp_command_and_exact_history",
        ]
        declaration["limitations"] = (
            "Bounded typed definitions-attribute profile; optional language omission is retained. "
            "Shared commands, desktop drafts, history and MCP have focused evidence. "
            "Full native interaction, semantic validation, external references and definitions children remain unverified."
        )
# Bounded declaration support only: import locations are model data and are never loaded.
for declaration in declarations:
    is_import = (declaration["schema"] == "BPMN20.xsd" and (
        (declaration["kind"] == "element" and declaration["name"] == "import") or
        (declaration["kind"] == "complexType" and declaration["name"] == "tImport") or
        (declaration["kind"] == "attribute" and "/complexType[@name=tImport]/" in declaration["path"]
         and declaration["name"] in {"namespace", "location", "importType"})))
    if is_import:
        declaration["coverage"].update({"model": "partial", "edit": "partial", "import": "partial", "export": "partial", "validate": "partial"})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:definitions_imports_roundtrip_without_loading_locations",
            "crates/processcraft-io/tests/interchange.rs:unrepresented_import_content_is_preserved",
            "crates/processcraft-core/tests/commands.rs:definitions_import_changes_are_undoable",
            "crates/processcraft-io/tests/native.rs:definitions_metadata_requires_capability_in_live_and_checkpoint_documents",
        ]
        declaration["limitations"] = (
            "Ordered bounded declarations and their three required attributes only; no external resolution or loading. "
            "Native GUI and CLI/MCP end-to-end qualification and import-dependent reference semantics remain unverified."
        )
# Structured text expressions cover a bounded set of owners, not arbitrary mixed XML.
for declaration in declarations:
    is_expression = (declaration["schema"] == "Semantic.xsd" and (
        (declaration["kind"] == "element" and declaration["name"] in {"expression", "formalExpression"}) or
        (declaration["kind"] == "complexType" and declaration["name"] in {"tExpression", "tFormalExpression"}) or
        (declaration["kind"] == "attribute" and "/complexType[@name=tFormalExpression]/" in declaration["path"]
         and declaration["name"] in {"language", "evaluatesToTypeRef"})))
    if is_expression:
        declaration["coverage"].update({"model": "partial", "import": "partial", "export": "partial"})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:expression_metadata_preserves_scoped_prefixes_and_default_namespace",
            "crates/processcraft-io/tests/interchange.rs:structured_expression_metadata_roundtrips_across_loop_and_adhoc_owners",
            "crates/processcraft-io/tests/interchange.rs:expression_inherited_documentation_mixed_order_roundtrips",
            "tests/fixtures/bpmn/structured-expressions.manifest.json",
        ]
        declaration["limitations"] = (
            "Selected expression owners preserve structured text and mixed metadata, optional type/language and namespace context; "
            "all-owner editing/validation and arbitrary extension semantics remain unverified or preserved."
        )
# Inline event definitions are a limited local-reference profile, not all contexts.
for declaration in declarations:
    if (declaration["schema"] == "Semantic.xsd" and declaration["kind"] == "element"
            and declaration["name"] in {"linkEventDefinition", "cancelEventDefinition", "compensateEventDefinition"}):
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "render", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:inline_link_cancel_and_compensation_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:inline_events_preserve_unrepresented_or_invalid_content",
            "crates/processcraft-core/tests/commands.rs:link_and_compensation_events_validate_atomically",
            "crates/processcraft-render/tests/bpmn_scene.rs:link_markers_distinguish_catch_and_throw_in_shared_scene",
            "crates/processcraft-render/tests/bpmn_scene.rs:cancel_boundary_and_end_use_hollow_and_filled_crosses",
            "crates/processcraft-render/tests/bpmn_scene.rs:compensation_boundary_and_throw_render_distinct_markers",
        ]
        declaration["limitations"] = (
            "Bounded inline local-reference profile and selected context checks only; "
            "root/referenced definitions, complete context rules, GUI and independent consumers remain unverified."
        )
# Selected supporting catalogs; inherited metadata and external resolution are not complete.
catalog_types = {"tItemDefinition", "tInterface", "tOperation", "tResource", "tResourceParameter"}
catalog_elements = {"itemDefinition", "interface", "operation", "resource", "resourceParameter"}
for declaration in declarations:
    if (declaration["schema"] == "Semantic.xsd" and (
        (declaration["kind"] == "element" and declaration["name"] in catalog_elements) or
        (declaration["kind"] == "complexType" and declaration["name"] in catalog_types) or
        (declaration["kind"] == "attribute" and any(
            f"/complexType[@name={name}]/" in declaration["path"] for name in catalog_types)))):
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:supporting_catalog_qnames_and_presence_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:supporting_catalog_unrepresented_metadata_is_preserved",
            "crates/processcraft-core/tests/commands.rs:supporting_catalog_validates_references_and_undoes_atomically",
            "crates/processcraft-io/tests/native.rs:supporting_catalog_capability_covers_live_and_checkpoint_documents",
        ]
        declaration["limitations"] = (
            "Bounded typed catalog profile only; inherited documentation/extensions, all user paths, "
            "external resolution and full normative semantics remain unverified or preserved."
        )
# Task binding attributes only; general task and operation semantics remain partial.
for declaration in declarations:
    if (declaration["schema"] == "Semantic.xsd" and declaration["kind"] == "attribute"
            and any(f"/complexType[@name={name}]/" in declaration["path"]
                    for name in {"tSendTask", "tReceiveTask", "tServiceTask"})
            and declaration["name"] in {"messageRef", "operationRef"}):
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:activity_message_and_operation_qnames_roundtrip_with_loop_context",
            "crates/processcraft-io/tests/interchange.rs:activity_unqualified_qname_and_wrong_subtype_are_safe",
            "crates/processcraft-core/tests/commands.rs:activity_bindings_validate_new_local_refs_and_preserve_legacy",
        ]
        declaration["limitations"] = (
            "Bounded QName binding profile; full task I/O, external resolution, GUI and normative semantics remain unverified. "
            "No general equality between task messageRef and operation messages is inferred."
        )
# Expanded data declarations: evidence is bounded to typed metadata and selected owners.
data_types = {"tDataObject", "tDataStore", "tDataInput", "tDataOutput", "tDataState", "tDataAssociation", "tAssignment"}
data_elements = {"dataObject", "dataStore", "dataInput", "dataOutput", "dataState", "dataAssociation", "assignment"}
for declaration in declarations:
    if (declaration["schema"] == "Semantic.xsd" and (
        (declaration["kind"] == "element" and declaration["name"] in data_elements) or
        (declaration["kind"] == "complexType" and declaration["name"] in data_types) or
        (declaration["kind"] == "attribute" and any(
            f"/complexType[@name={name}]/" in declaration["path"] for name in data_types)))):
        declaration["coverage"].update({k: "partial" for k in ("model", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:item_aware_metadata_transformations_and_assignments_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:data_elements_allow_optional_xml_ids_and_process_io",
            "crates/processcraft-io/tests/interchange.rs:unsupported_data_state_metadata_and_invalid_assignments_are_preserved",
            "crates/processcraft-io/tests/native.rs:expanded_data_capability_covers_live_and_checkpoint_documents",
        ]
        declaration["limitations"] = (
            "Bounded data metadata, expressions and optional identities only; all owners, inherited content, "
            "event-owned associations, Property endpoints, complete renderer/UI and semantic qualification remain unverified or preserved."
        )
# Selected event and DI properties: bounded fixtures, not family-wide conformance.
for declaration in declarations:
    event_field = (declaration["schema"] == "Semantic.xsd" and (
        (declaration["kind"] == "attribute" and declaration["name"] in {"parallelMultiple", "isInterrupting"}
         and any(f"/complexType[@name={name}]/" in declaration["path"] for name in {"tCatchEvent", "tStartEvent"})) or
        (declaration["kind"] == "element" and declaration["name"] == "eventDefinitionRef"
         and any(f"/complexType[@name={name}]/" in declaration["path"] for name in {"tCatchEvent", "tThrowEvent"}))))
    di_field = (declaration["schema"] == "BPMNDI.xsd" and declaration["kind"] == "attribute" and (
        ("/complexType[@name=BPMNShape]/" in declaration["path"] and declaration["name"] in {"isHorizontal", "isMarkerVisible"}) or
        ("/complexType[@name=BPMNEdge]/" in declaration["path"] and declaration["name"] in {"sourceElement", "targetElement"})))
    if event_field or di_field:
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "render", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:event_root_refs_flags_properties_and_owned_io_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:empty_timer_choice_and_event_flags_preserve_omission",
            "crates/processcraft-io/tests/interchange.rs:explicit_di_representation_endpoints_and_shape_flags_roundtrip",
        ]
        declaration["limitations"] = (
            "Bounded event/DI fixture profile; complete contexts, user paths, style, "
            "native interaction and independent consumers remain unverified or preserved."
        )
# Bounded common metadata and label style cases; no full inherited-coverage claim.
for declaration in declarations:
    common = (declaration["schema"] == "Semantic.xsd" and (
        (declaration["kind"] == "element" and declaration["name"] in {"documentation", "script", "text", "extension", "relationship"}) or
        (declaration["kind"] == "complexType" and declaration["name"] in {"tDocumentation", "tScript", "tText", "tExtension", "tRelationship"})))
    label = (declaration["schema"] == "BPMNDI.xsd" and declaration["kind"] in {"element", "complexType"}
             and declaration["name"] in {"BPMNLabel", "BPMNLabelStyle"})
    if common or label:
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "import", "export", "validate")})
        if label:
            declaration["coverage"]["render"] = "partial"
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:process_documentation_mixed_order_and_namespaces_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:anonymous_metadata_stays_on_its_typed_owner",
            "crates/processcraft-io/tests/interchange.rs:anonymous_owner_foreign_context_and_sibling_order_roundtrip",
            "crates/processcraft-io/tests/interchange.rs:script_and_annotation_mixed_xml_roundtrip_inertly",
            "crates/processcraft-io/tests/interchange.rs:definitions_declarations_and_required_unknown_extension_policy",
            "crates/processcraft-io/tests/interchange.rs:diagram_label_style_and_empty_label_roundtrip",
            "scripts/qualify_common_metadata.py",
        ]
        declaration["limitations"] = (
            "Bounded common metadata and DI label profiles; all inherited owners, external QName/style resolution, "
            "arbitrary vendor semantics, native interaction and independent consumer qualification remain open."
        )
# Bounded activity/callable/correlation fixtures; not family-wide completion.
activity_elements = {"globalTask", "globalManualTask", "globalUserTask", "globalScriptTask", "globalBusinessRuleTask", "callableElement", "ioBinding", "resourceRole", "resourceParameterBinding", "resourceAssignmentExpression", "correlationProperty", "correlationPropertyRetrievalExpression", "correlationSubscription", "correlationPropertyBinding", "correlationKey", "complexBehaviorDefinition", "implicitThrowEvent"}
for declaration in declarations:
    if (declaration["schema"] == "Semantic.xsd" and declaration["kind"] == "element"
            and declaration["name"] in activity_elements):
        declaration["coverage"].update({k: "partial" for k in ("model", "edit", "import", "export", "validate")})
        declaration["evidence"] = [
            "crates/processcraft-io/tests/interchange.rs:activity_correlation_and_callable_fixtures_roundtrip_with_values",
            "crates/processcraft-io/tests/interchange.rs:multi_instance_behavior_qnames_preserve_empty_namespace_and_shadowed_prefixes",
            "scripts/qualify_activity_metadata.py",
        ]
        declaration["limitations"] = (
            "Selected callable, role, correlation and loop metadata only; all subtype/context paths, "
            "native interaction, DI, external consumers and complete semantics remain unverified."
        )
# ---- Required scope (M3 proposal sections 1-2; sets frozen by WP2) ----
REQUIRED_SETS = {
    "descriptive": ["definitions", "process", "collaboration", "participant", "laneSet", "lane", "childLaneSet", "sequenceFlow", "messageFlow", "task", "userTask", "serviceTask", "subProcess", "callActivity", "exclusiveGateway", "parallelGateway", "startEvent", "endEvent", "messageEventDefinition", "timerEventDefinition", "terminateEventDefinition", "dataObject", "dataStore", "dataStoreReference", "dataInput", "dataOutput", "ioSpecification", "inputSet", "outputSet", "dataInputAssociation", "dataOutputAssociation", "textAnnotation", "association", "group", "category", "categoryValue", "documentation"],
    "analytic": ["conditionExpression", "message", "sendTask", "receiveTask", "standardLoopCharacteristics", "multiInstanceLoopCharacteristics", "inclusiveGateway", "eventBasedGateway", "intermediateCatchEvent", "intermediateThrowEvent", "boundaryEvent", "linkEventDefinition", "errorEventDefinition", "escalationEventDefinition", "signalEventDefinition", "conditionalEventDefinition"],
    "choreography": ["choreography", "choreographyTask", "subChoreography", "callChoreography", "globalChoreographyTask", "participantMultiplicity"],
    "di": ["BPMNDiagram", "BPMNPlane", "BPMNShape", "BPMNEdge", "BPMNLabel", "Bounds", "Point", "Waypoint"],
}
SET_TAGS = {"descriptive": "D", "analytic": "A", "choreography": "C", "di": "I"}
DI_SCHEMAS = ("BPMNDI.xsd", "DI.xsd", "DC.xsd")
STATES = ("verified", "not_applicable", "partial", "unverified")
DIMENSIONS = ("model", "edit", "render", "import", "export", "validate")
semantic_base = {}
for n in ET.parse(FIX / "Semantic.xsd").getroot().iter(XSD + "complexType"):
    for e in n.iter(XSD + "extension"):
        semantic_base[n.get("name")] = e.get("base").split(":")[-1]
        break
top_elements = {x["name"]: x for x in declarations if x["kind"] == "element" and x["path"] == f"/schema/element[@name={x['name']}]"}
def type_name(n):
    e = top_elements.get(n)
    return e["type"].split(":")[-1] if e and e["type"] else "t" + n[0].upper() + n[1:]
required = {}
for label, names in REQUIRED_SETS.items():
    tag = SET_TAGS[label]
    if label == "di":
        prefixes = [f"/schema/{k}[@name={n}]" for n in names for k in ("element", "complexType")]
        prefixes += [f"/schema/complexType[@name=BPMN{n}]" for n in names]
        for x in declarations:
            if x["schema"] in DI_SCHEMAS and any(x["path"].startswith(q) for q in prefixes):
                required.setdefault((x["schema"], x["path"]), set()).add(tag)
        continue
    for n in names:
        t = type_name(n)
        chain = []
        while t:
            chain.append(t)
            t = semantic_base.get(t)
        if n in top_elements:
            required.setdefault((top_elements[n]["schema"], top_elements[n]["path"]), set()).add(tag)
        for x in declarations:
            if x["schema"] == "Semantic.xsd" and any(x["path"].startswith(f"/schema/complexType[@name={c}]") for c in chain):
                required.setdefault((x["schema"], x["path"]), set()).add(tag)
by_key = {(x["schema"], x["path"]): x for x in declarations}
simple_types = {x["name"]: x for x in declarations if x["kind"] == "simpleType" and x["schema"] == "Semantic.xsd"}
for key in list(required):
    t = (by_key[key]["type"] or "").split(":")[-1]
    if t in simple_types:
        required.setdefault((simple_types[t]["schema"], simple_types[t]["path"]), {"S"})
VISIBLE_ATTRS = {"name", "isExpanded", "isMarkerVisible", "isMessageVisible", "messageVisibleKind", "participantBandKind", "isHorizontal", "cancelActivity", "isCollection", "loopType", "sequential", "bpmnElement", "x", "y", "width", "height", "isInterrupting", "default", "initiatingParticipantRef", "instantiate", "eventGatewayType", "gatewayDirection", "isForCompensation", "triggeredByEvent"}
NOTATION = set(sum(REQUIRED_SETS.values(), [])) - {"definitions", "script", "conditionExpression", "documentation", "childLaneSet", "dataInputAssociation", "dataOutputAssociation", "ioSpecification", "inputSet", "outputSet", "loopCardinality", "completionCondition", "category", "categoryValue", "message", "messageEventDefinition", "timerEventDefinition", "terminateEventDefinition", "linkEventDefinition", "errorEventDefinition", "escalationEventDefinition", "signalEventDefinition", "conditionalEventDefinition", "participantMultiplicity", "collaboration", "process", "choreography", "globalChoreographyTask"}
def required_dims(x):
    if x["kind"] in ("complexType", "simpleType"):
        return ["model", "import", "export", "validate"]
    if x["schema"] in DI_SCHEMAS:
        return ["model", "render", "import", "export"]
    dims = ["model", "edit", "import", "export", "validate"]
    if (x["kind"] == "element" and x["name"] in NOTATION and x["path"].count("/") == 2) or (x["kind"] == "attribute" and x["name"] in VISIBLE_ATTRS):
        dims.append("render")
    return dims
for key, tags in required.items():
    by_key[key]["required"] = sorted(tags)
    by_key[key]["required_dims"] = required_dims(by_key[key])

# ---- Evidence sidecar: docs/research/bpmn-2-evidence/*.json fragments ----
EVIDENCE_DIR = ROOT / "docs/research/bpmn-2-evidence"
def validate_evidence(ref, where):
    file, sep, name = ref.partition(":")
    f = (ROOT / file).resolve()
    if not f.is_relative_to(ROOT) or not f.is_file():
        raise SystemExit(f"{where}: evidence file not found in repository: {file}")
    if sep and not re.fullmatch(r"\w+", name):
        raise SystemExit(f"{where}: invalid test name in {ref!r}")
    # A named test must be a Rust `fn` or Python `def` in that file.
    if sep and not re.search(rf"\b(?:fn|def)\s+{name}\b", f.read_text(encoding="utf-8", errors="replace")):
        raise SystemExit(f"{where}: no fn/def {name!r} in {file}")
seen = set()
fragments = sorted(EVIDENCE_DIR.glob("*.json")) if EVIDENCE_DIR.is_dir() else []
for frag in fragments:
    data = json.loads(frag.read_text(encoding="utf-8"))
    for cell in sorted(data.get("cells", []), key=lambda c: (c["schema"], c["path"], c["dimension"])):
        where = f"{frag.name} {cell.get('schema')} {cell.get('path')} {cell.get('dimension')}"
        key = (cell["schema"], cell["path"])
        if key not in by_key:
            raise SystemExit(f"{where}: unknown declaration path")
        if cell["dimension"] not in DIMENSIONS:
            raise SystemExit(f"{where}: unknown dimension")
        state = cell.get("state")
        if state not in ("verified", "not_applicable"):
            raise SystemExit(f"{where}: state must be 'verified' or 'not_applicable'")
        if state == "not_applicable":
            # Only for a validate/render cell with no normative constraint or notation; the note must cite the source.
            if cell["dimension"] not in ("validate", "render"):
                raise SystemExit(f"{where}: not_applicable is allowed only for validate and render")
            if len(cell.get("note", "")) < 40 or not re.search(r"§|Table|p\.\s*\d|XSD", cell["note"]):
                raise SystemExit(f"{where}: not_applicable needs a note citing the spec or XSD")
        elif not cell.get("evidence"):
            raise SystemExit(f"{where}: empty evidence list")
        if (key, cell["dimension"]) in seen:
            raise SystemExit(f"{where}: duplicate cell across fragments")
        seen.add((key, cell["dimension"]))
        for ref in cell.get("evidence", []):
            validate_evidence(ref, where)
        d = by_key[key]
        d["coverage"][cell["dimension"]] = state
        if cell.get("evidence"):
            d.setdefault("verified_evidence", {})[cell["dimension"]] = sorted(cell["evidence"])
        if cell.get("note"):
            d.setdefault("verified_notes", {})[cell["dimension"]] = cell["note"]

# ---- required_scope summary ----
def empty():
    return {"declarations": 0, "cells": 0, **{s: 0 for s in STATES}}
required_scope = {"total": empty(), "by_set": {}}
pending = []
for x in declarations:
    if "required" not in x:
        continue
    for bucket in [required_scope["total"]] + [required_scope["by_set"].setdefault(t, empty()) for t in x["required"]]:
        bucket["declarations"] += 1
        for dm in x["required_dims"]:
            bucket["cells"] += 1
            bucket[x["coverage"][dm]] += 1
    pending += [(x["schema"], x["path"], dm) for dm in x["required_dims"] if x["coverage"][dm] not in ("verified", "not_applicable")]
required_scope["by_set"] = dict(sorted(required_scope["by_set"].items()))

result = {"implementation_evidence_commit": "a69472b251de88a221a873476fa5cb51e60560db", "implementation_evidence_tree": "100cada0dd4910463cb2b7ae42d6f89c59583569", "profiles": profiles, "cmof_declarations": cmof, "version": "2.0.2", "coverage_policy": "unverified means no construct-level evidence recorded; raw retention is not modeling support", "sources": sources, "declarations": declarations, "required_scope": required_scope,
          "conformance": {"status": "not_assessed", "note": "OMG PDF conformance classes and semantic constraints require separate clause-by-clause mapping; XSD declarations do not establish conformance."}}
serialized = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
if "--gate" in sys.argv:
    print(json.dumps(required_scope, indent=2))
    if "--list" in sys.argv:
        groups = {}
        for schema, path, dm in pending:
            groups.setdefault((schema, path.split("/")[2]), []).append((path, dm))
        for (schema, owner), rows in sorted(groups.items()):
            print(f"{schema} {owner} ({len(rows)})")
            for path, dm in rows:
                print(f"  {path} {dm}")
    raise SystemExit(1 if pending else 0)
if "--check" in sys.argv:
    if not OUT.exists() or OUT.read_text(encoding="utf-8") != serialized:
        raise SystemExit("Inventory differs from pinned source files; inspect before regenerating")
else:
    OUT.write_text(serialized, encoding="utf-8")
print(len(declarations), "declarations")
