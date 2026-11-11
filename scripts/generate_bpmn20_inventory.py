"""Generate the BPMN normative declaration inventory from pinned OMG schemas."""
import hashlib
import json
import sys
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
FIX = ROOT / "tests/fixtures/bpmn20-schema"
OUT = ROOT / "docs/research/bpmn-2-coverage.json"
XSD = "{http://www.w3.org/2001/XMLSchema}"
files = sorted(FIX.iterdir())
sources = [{"file": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
            "url": "https://www.omg.org/spec/BPMN/2.0.2/PDF" if p.suffix == ".pdf" else "https://www.omg.org/spec/BPMN/20100501/" + p.name}
           for p in files if p.suffix in (".xsd", ".cmof", ".pdf")]
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
    "choreography": {"elements": ["choreography", "choreographyTask"], "evidence": ["tests/fixtures/bpmn20-schema/choreography.bpmn"], "limit": "Simple tasks, local participants and message flows; fixture and CLI/XSD checks only; gateways/events not covered."},
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
            "crates/processcraft-io/tests/interchange.rs:expression_mixed_content_remains_preserved",
            "tests/fixtures/bpmn/structured-expressions.manifest.json",
        ]
        declaration["limitations"] = (
            "Structured text, selected owners, optional type/language and namespace context only; "
            "mixed XML, inherited documentation/extensions and all-owner editing/validation remain unverified or preserved."
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
            "ancestor-scope references, renderer/UI and semantic qualification remain unverified or preserved."
        )
result = {"profiles": profiles, "cmof_declarations": cmof, "version": "2.0.2", "coverage_policy": "unverified means no construct-level evidence recorded; raw retention is not modeling support", "sources": sources, "declarations": declarations,
          "conformance": {"status": "not_assessed", "note": "OMG PDF conformance classes and semantic constraints require separate clause-by-clause mapping; XSD declarations do not establish conformance."}}
serialized = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
if "--check" in sys.argv:
    if not OUT.exists() or OUT.read_text(encoding="utf-8") != serialized:
        raise SystemExit("Inventory differs from pinned source files; inspect before regenerating")
else:
    OUT.write_text(serialized, encoding="utf-8")
print(len(declarations), "declarations")
