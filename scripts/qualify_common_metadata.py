#!/usr/bin/env python3
"""Reproduce focused common-metadata qualification without editing source fixtures.

Requires lxml and the built common_metadata_qualify Rust example. This focused
fixture set is not a BPMN coverage inventory or a conformance percentage.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
BPMN = "http://www.omg.org/spec/BPMN/20100524/MODEL"
VENDOR = "urn:processcraft:qualification"
FIXTURES = [
    "interop-process", "event-ownership", "data-metadata", "data-optional",
    "root-only", "supporting-catalog", "m2-flat-lanes", "activity-bindings",
    "empty-namespace-choreography", "empty-namespace-conversation",
]
# BaseElement-derived owner tags exercised by this focused fixture family.
# This list is fixed independently of the parser's admission whitelist.
OWNERS = {
    'adHocSubProcess',
    'assignment',
    'association',
    'boundaryEvent',
    'businessRuleTask',
    'callActivity',
    'callConversation',
    'cancelEventDefinition',
    'category',
    'categoryValue',
    'childLaneSet',
    'choreography',
    'choreographyTask',
    'collaboration',
    'compensateEventDefinition',
    'complexGateway',
    'conditionalEventDefinition',
    'conversation',
    'conversationLink',
    'dataInput',
    'dataInputAssociation',
    'dataObject',
    'dataObjectReference',
    'dataOutput',
    'dataOutputAssociation',
    'dataState',
    'dataStore',
    'dataStoreReference',
    'endEvent',
    'error',
    'errorEventDefinition',
    'escalation',
    'escalationEventDefinition',
    'eventBasedGateway',
    'exclusiveGateway',
    'group',
    'inclusiveGateway',
    'inputSet',
    'interface',
    'intermediateCatchEvent',
    'intermediateThrowEvent',
    'ioSpecification',
    'itemDefinition',
    'lane',
    'laneSet',
    'linkEventDefinition',
    'manualTask',
    'message',
    'messageEventDefinition',
    'messageFlow',
    'multiInstanceLoopCharacteristics',
    'operation',
    'outputSet',
    'parallelGateway',
    'participant',
    'process',
    'property',
    'receiveTask',
    'relationship',
    'resource',
    'resourceParameter',
    'scriptTask',
    'sendTask',
    'sequenceFlow',
    'serviceTask',
    'signal',
    'signalEventDefinition',
    'standardLoopCharacteristics',
    'startEvent',
    'subConversation',
    'subProcess',
    'task',
    'terminateEventDefinition',
    'textAnnotation',
    'timerEventDefinition',
    'transaction',
    'userTask',
}


def generate(name, output):
    source = ROOT / "tests/fixtures/bpmn" / (name + ".bpmn")
    root = E.fromstring(source.read_bytes(), E.XMLParser(resolve_entities=False, no_network=True))
    E.register_namespace("m3", VENDOR)
    used = {element.get("id") for element in root.iter() if isinstance(element.tag, str)}
    expected = []
    for element in list(root.iter()):
        if not isinstance(element.tag, str):
            continue
        tag = E.QName(element)
        if tag.namespace != BPMN or tag.localname not in OWNERS or not element.get("id"):
            continue
        if any(isinstance(parent.tag, str) and E.QName(parent).namespace == BPMN
               and E.QName(parent).localname in {"documentation", "extensionElements", "script", "text"}
               for parent in element.iterancestors()):
            continue
        identity = "qualification_doc_" + str(len(expected))
        while identity in used:
            identity += "_x"
        used.add(identity)
        element.set("{" + VENDOR + "}flag", "m3:inert")
        documentation = E.Element("{" + BPMN + "}documentation", id=identity, textFormat="text/plain")
        documentation.text = "before"
        detail = E.SubElement(documentation, "{" + VENDOR + "}detail", ref="m3:item")
        detail.text, detail.tail = "inside", "after"
        documentation.append(E.Comment("inert comment"))
        documentation.append(E.ProcessingInstruction("inspect", "inert"))
        element.insert(0, documentation)
        extension = element.find("{" + BPMN + "}extensionElements")
        if extension is None:
            extension = E.Element("{" + BPMN + "}extensionElements")
            index = 0
            while index < len(element) and element[index].tag == "{" + BPMN + "}documentation":
                index += 1
            element.insert(index, extension)
        E.SubElement(extension, "{" + VENDOR + "}payload", owner=element.get("id")).text = "inert"
        expected.append({"ownerId": element.get("id"), "ownerTag": tag.localname,
                         "documentationId": identity})
    target = output / "inputs" / (name + ".bpmn")
    target.write_bytes(E.tostring(root, encoding="UTF-8", xml_declaration=True))
    return {"fixture": name, "origin": str(source.relative_to(ROOT)),
            "originSha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "derivedSha256": hashlib.sha256(target.read_bytes()).hexdigest(),
            "expected": expected}


def check_source_values(tree, expected):
    for value in expected:
        owners = tree.xpath("//*[@id=$identity]", identity=value["ownerId"])
        assert len(owners) == 1, (value, "owner")
        owner = owners[0]
        assert owner.get("{" + VENDOR + "}flag") == "m3:inert"
        assert owner.nsmap.get("m3") == VENDOR
        docs = [doc for doc in owner.findall("{" + BPMN + "}documentation")
                if doc.get("id") == value["documentationId"]]
        assert len(docs) == 1, (value, "documentation")
        doc = docs[0]
        assert doc.get("textFormat") == "text/plain" and doc.text == "before" and len(doc) == 3
        detail, comment, instruction = doc
        assert detail.tag == "{" + VENDOR + "}detail" and detail.get("ref") == "m3:item"
        assert detail.text == "inside" and detail.tail == "after" and detail.nsmap.get("m3") == VENDOR
        assert isinstance(comment, E._Comment) and comment.text == "inert comment"
        assert isinstance(instruction, E._ProcessingInstruction)
        assert instruction.target == "inspect" and instruction.text == "inert"
        extension = owner.find("{" + BPMN + "}extensionElements")
        assert extension is not None
        assert any(child.tag == "{" + VENDOR + "}payload" and child.get("owner") == value["ownerId"]
                   and child.text == "inert" for child in extension)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    binary = arguments.binary.resolve(strict=True)
    output = arguments.output
    if not output.is_absolute() or output.exists():
        parser.error("--output must be a new absolute directory")
    output.mkdir()
    (output / "inputs").mkdir()
    (output / "outputs").mkdir()
    schema = E.XMLSchema(E.parse(str(ROOT / "tests/fixtures/bpmn20-schema/BPMN20.xsd")))
    manifest, results = [], []
    for name in FIXTURES:
        record = generate(name, output)
        manifest.append(record)
        source = output / "inputs" / (name + ".bpmn")
        schema.assertValid(E.parse(str(source)))
        run = subprocess.run([str(binary), str(source), str(output / "outputs" / name)],
                             capture_output=True, text=True, check=False)
        result = {"fixture": name, "exitCode": run.returncode, "stdout": run.stdout, "stderr": run.stderr}
        if run.returncode == 0:
            exported = E.parse(str(output / "outputs" / name / "output.bpmn"))
            schema.assertValid(exported)
            check_source_values(exported, record["expected"])
            result["sourceExpectedOwnersVerified"] = len(record["expected"])
        results.append(result)
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2))
        (output / "results.json").write_text(json.dumps(results, indent=2))
        print(name, "PASS" if run.returncode == 0 else "REFUSED", flush=True)
    return int(any(result["exitCode"] != 0 for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
