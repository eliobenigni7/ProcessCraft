#!/usr/bin/env python3
"""Compare actual BPMN XML metadata at source owner paths, independently of the Rust model.

Also validates both files against the pinned OMG schemas. This focused check
compares BaseElement prefixes and mixed bodies, not the entire BPMN standard.
"""
import argparse
import json
from pathlib import Path
from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
BPMN = "http://www.omg.org/spec/BPMN/20100524/MODEL"
XSI_TYPE = "{http://www.w3.org/2001/XMLSchema-instance}type"
XML = "http://www.w3.org/XML/1998/namespace"
DI = "http://www.omg.org/spec/DD/20100524/DI"
BPMNDI = "http://www.omg.org/spec/BPMN/20100524/DI"
DC = "http://www.omg.org/spec/DD/20100524/DC"
STANDARD_NAMESPACES = {BPMN, BPMNDI, DI, DC}
OPAQUE = {"documentation", "extensionElements", "script", "text"}
BODIES = {"script", "text", "conditionExpression", "condition", "timeDate", "timeDuration",
          "timeCycle", "loopCondition", "loopCardinality", "completionCondition",
          "activationCondition", "transformation", "from", "to"}
BODIES |= {"messagePath", "dataPath", "expression"}
# Fixed independently of the editable parser. This optional focused check keeps
# omission-sensitive activity attributes and QName-valued text visible even if
# both sides of a model round trip accidentally discard the same field.
ACTIVITY_OWNERS = {
    "process", "callableElement", "globalTask", "globalManualTask", "globalUserTask",
    "globalScriptTask", "globalBusinessRuleTask", "task", "userTask", "manualTask",
    "scriptTask", "businessRuleTask", "sendTask", "receiveTask", "serviceTask",
    "callActivity", "subProcess", "transaction", "adHocSubProcess", "sequenceFlow",
    "exclusiveGateway", "inclusiveGateway", "complexGateway", "parallelGateway",
    "eventBasedGateway", "standardLoopCharacteristics", "multiInstanceLoopCharacteristics",
    "complexBehaviorDefinition", "event", "implicitThrowEvent", "property", "ioBinding",
    "resourceRole", "performer", "humanPerformer", "potentialOwner", "rendering",
    "resourceAssignmentExpression", "resourceParameterBinding", "correlationProperty",
    "correlationPropertyRetrievalExpression", "correlationKey", "correlationSubscription",
    "correlationPropertyBinding", "auditing", "monitoring",
}
QNAME_ATTRIBUTES = {"calledElement", "definitionalCollaborationRef", "operationRef",
                    "messageRef", "itemSubjectRef", "parameterRef", "correlationKeyRef",
                    "correlationPropertyRef", "oneBehaviorEventRef", "noneBehaviorEventRef"}
BOOLEAN_ATTRIBUTES = {"isClosed", "isExecutable", "testBefore", "isSequential",
                      "isForCompensation", "instantiate", "triggeredByEvent",
                      "cancelRemainingInstances", "isImmediate"}
QNAME_TEXT_PARENTS = {
    "supportedInterfaceRef": {"process", "callableElement", "globalTask", "globalManualTask",
                               "globalUserTask", "globalScriptTask", "globalBusinessRuleTask"},
    "supports": {"process"},
    "resourceRef": {"resourceRole", "performer", "humanPerformer", "potentialOwner"},
    "correlationPropertyRef": {"correlationKey"},
    "loopDataInputRef": {"multiInstanceLoopCharacteristics"},
    "loopDataOutputRef": {"multiInstanceLoopCharacteristics"},
    "categoryValueRef": ACTIVITY_OWNERS | {"dataObject", "dataObjectReference", "dataStoreReference",
        "startEvent", "endEvent", "boundaryEvent", "intermediateCatchEvent", "intermediateThrowEvent",
        "choreographyTask", "callChoreography", "subChoreography"},
}


def expanded_qname(element, value):
    value = value.strip()
    prefix, local = value.split(":", 1) if ":" in value else ("", value)
    return [namespace_map(element).get(prefix), local]


def activity_kind(element):
    name = E.QName(element).localname
    if (name == "rootElement" and XSI_TYPE in element.attrib
            and expanded_qname(element, element.get(XSI_TYPE)) == [BPMN, "tCallableElement"]):
        return "callableElement"
    return name


def activity_attributes(element):
    result = attributes(element)
    for name, value in result.items():
        if name in QNAME_ATTRIBUTES or name == "type" and E.QName(element).localname == "correlationProperty":
            result[name] = expanded_qname(element, value)
        elif name in BOOLEAN_ATTRIBUTES:
            result[name] = value.strip() in {"true", "1"}
        elif name in {"startQuantity", "completionQuantity", "loopMaximum"}:
            result[name] = int(value)
    return result


def diagram_attributes(element):
    result = attributes(element)
    tag = E.QName(element)
    qnames = {"bpmnElement", "sourceElement", "targetElement", "labelStyle", "choreographyActivityShape"}
    if tag.namespace == BPMN and tag.localname == "messageFlow":
        qnames = {"sourceRef", "targetRef", "messageRef"}
    if tag.namespace == BPMN and tag.localname == "participant":
        qnames = {"processRef"}
    for name, value in result.items():
        if name in qnames:
            result[name] = expanded_qname(element, value)
        elif name in {"isHorizontal", "isExpanded", "isMarkerVisible", "isMessageVisible",
                      "isBold", "isItalic", "isUnderline", "isStrikeThrough"}:
            result[name] = value.strip() in {"true", "1"}
        elif tag.namespace == BPMN and name in {"minimum", "maximum"}:
            result[name] = int(value)
        elif tag.namespace in {DC, DI, BPMNDI} and name in {"x", "y", "width", "height", "size", "resolution"}:
            result[name] = float(value)
    return result


def namespace_map(element):
    bindings = {prefix or "": uri or "" for prefix, uri in element.nsmap.items()}
    bindings.setdefault("", "")
    bindings["xml"] = XML
    return bindings


def attributes(element):
    result = dict(element.attrib)
    if XSI_TYPE in result:
        name = result[XSI_TYPE]
        prefix, local = name.split(":", 1) if ":" in name else ("", name)
        result[XSI_TYPE] = [namespace_map(element).get(prefix), local]
    return result


def body(element):
    if isinstance(element, E._Comment):
        return {"comment": element.text or ""}
    if isinstance(element, E._ProcessingInstruction):
        return {"instruction": element.target, "data": element.text or ""}
    return {"tag": element.tag, "attributes": attributes(element),
            "namespaces": namespace_map(element), "text": element.text or "",
            "children": [{"node": body(child), "tail": child.tail or ""} for child in element]}


def records(tree, activity_fields=False, diagram_fields=False):
    result = {}

    def visit(element, path, opaque):
        if not isinstance(element.tag, str):
            return
        tag = E.QName(element)
        semantic = tag.namespace == BPMN and not opaque
        if diagram_fields and not opaque:
            if (tag.namespace in {BPMNDI, DC, DI}
                    or tag.namespace == BPMN and tag.localname in {"participant", "participantMultiplicity", "messageFlow"}):
                result[path + "/diagramAttributes"] = diagram_attributes(element)
            if tag.namespace == BPMNDI and tag.localname in {"BPMNPlane", "BPMNShape", "BPMNEdge", "BPMNLabel"}:
                extension = element.find("{" + DI + "}extension")
                foreign = {key: value for key, value in element.attrib.items()
                           if key.startswith("{") and key != XSI_TYPE}
                if extension is not None or foreign:
                    result[path + "/diagramMetadata"] = {
                        "extension": None if extension is None else body(extension),
                        "foreignAttributes": foreign,
                        "foreignAttributeContext": namespace_map(element) if foreign else None,
                    }
                if tag.localname == "BPMNPlane":
                    result[path + "/planeChildren"] = [
                        {"tag": child.tag, "idPresent": "id" in child.attrib, "id": child.get("id")}
                        for child in element if isinstance(child.tag, str) and child.tag != "{" + DI + "}extension"]
        if not opaque and tag.namespace in STANDARD_NAMESPACES:
            result[path + "/xmlIdentity"] = {"idPresent": "id" in element.attrib,
                                              "id": element.get("id")}
        if semantic:
            if activity_fields:
                if activity_kind(element) in ACTIVITY_OWNERS:
                    result[path + "/activityAttributes"] = activity_attributes(element)
                parent = element.getparent()
                if (parent is not None and parent.tag.startswith("{" + BPMN + "}")
                        and activity_kind(parent) in QNAME_TEXT_PARENTS.get(tag.localname, set())):
                    result[path + "/activityQName"] = expanded_qname(element, element.text or "")
            docs = element.findall("{" + BPMN + "}documentation")
            extension = element.find("{" + BPMN + "}extensionElements")
            foreign = {key: value for key, value in element.attrib.items()
                       if key.startswith("{") and E.QName(key).namespace != BPMN and key != XSI_TYPE}
            if docs or extension is not None or foreign or tag.localname in {"laneSet", "childLaneSet"}:
                result[path + "/metadata"] = {
                    "idPresent": "id" in element.attrib, "id": element.get("id"),
                    "documentation": [body(doc) for doc in docs],
                    "extension": None if extension is None else body(extension),
                    "foreignAttributes": foreign,
                    "foreignAttributeContext": namespace_map(element) if foreign else None,
                }
            if tag.localname in BODIES:
                result[path + "/mixedBody"] = body(element)
        opaque = (opaque or tag.namespace not in STANDARD_NAMESPACES
                  or tag.namespace == BPMN and tag.localname in OPAQUE
                  or tag.namespace == DI and tag.localname == "extension")
        ordinal = {}
        for child in element:
            if not isinstance(child.tag, str):
                continue
            ordinal[child.tag] = ordinal.get(child.tag, 0) + 1
            visit(child, path + "/" + child.tag + "[" + str(ordinal[child.tag]) + "]", opaque)

    root = tree.getroot()
    visit(root, "/" + root.tag + "[1]", False)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--activity-fields", action="store_true",
                        help="Also compare selected activity attributes and scoped QName text")
    parser.add_argument("--diagram-fields", action="store_true",
                        help="Also compare DI metadata, child order, geometry and participant/message fields")
    arguments = parser.parse_args()
    schema = E.XMLSchema(E.parse(str(ROOT / "tests/fixtures/bpmn20-schema/BPMN20.xsd")))
    trees = []
    for path in (arguments.source, arguments.output):
        tree = E.parse(str(path), E.XMLParser(resolve_entities=False, no_network=True))
        if tree.docinfo.doctype:
            parser.error("DTD content is not accepted")
        schema.assertValid(tree)
        trees.append(tree)
    expected, actual = [records(tree, arguments.activity_fields, arguments.diagram_fields) for tree in trees]
    differences = [{"path": path, "before": expected.get(path), "after": actual.get(path)}
                   for path in sorted(expected.keys() | actual.keys()) if expected.get(path) != actual.get(path)]
    print(json.dumps({"source": str(arguments.source), "output": str(arguments.output),
                      "sourceOwnerAndBodyRecords": sum(path.endswith(("/metadata", "/mixedBody")) for path in expected),
                      "standardElementIdentityRecords": sum(path.endswith("/xmlIdentity") for path in expected),
                      "activityFieldRecords": sum(path.endswith(("/activityAttributes", "/activityQName")) for path in expected),
                      "diagramFieldRecords": sum(path.endswith(("/diagramAttributes", "/diagramMetadata", "/planeChildren")) for path in expected),
                      "anonymousOwnerRecords": sum(path.endswith("/metadata") and not value["idPresent"]
                                                   for path, value in expected.items()),
                      "differences": differences}, indent=2))
    return int(bool(differences))


if __name__ == "__main__":
    raise SystemExit(main())
