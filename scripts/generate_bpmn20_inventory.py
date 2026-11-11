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
result = {"profiles": profiles, "cmof_declarations": cmof, "version": "2.0.2", "coverage_policy": "unverified means no construct-level evidence recorded; raw retention is not modeling support", "sources": sources, "declarations": declarations,
          "conformance": {"status": "not_assessed", "note": "OMG PDF conformance classes and semantic constraints require separate clause-by-clause mapping; XSD declarations do not establish conformance."}}
serialized = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
if "--check" in sys.argv:
    if not OUT.exists() or OUT.read_text(encoding="utf-8") != serialized:
        raise SystemExit("Inventory differs from pinned source files; inspect before regenerating")
else:
    OUT.write_text(serialized, encoding="utf-8")
print(len(declarations), "declarations")
