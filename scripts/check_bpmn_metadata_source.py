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
STANDARD_NAMESPACES = {BPMN, "http://www.omg.org/spec/BPMN/20100524/DI", DI,
                       "http://www.omg.org/spec/DD/20100524/DC"}
OPAQUE = {"documentation", "extensionElements", "script", "text"}
BODIES = {"script", "text", "conditionExpression", "condition", "timeDate", "timeDuration",
          "timeCycle", "loopCondition", "loopCardinality", "completionCondition",
          "activationCondition", "transformation", "from", "to"}


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


def records(tree):
    result = {}

    def visit(element, path, opaque):
        if not isinstance(element.tag, str):
            return
        tag = E.QName(element)
        semantic = tag.namespace == BPMN and not opaque
        if not opaque and tag.namespace in STANDARD_NAMESPACES:
            result[path + "/xmlIdentity"] = {"idPresent": "id" in element.attrib,
                                              "id": element.get("id")}
        if semantic:
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
    arguments = parser.parse_args()
    schema = E.XMLSchema(E.parse(str(ROOT / "tests/fixtures/bpmn20-schema/BPMN20.xsd")))
    trees = []
    for path in (arguments.source, arguments.output):
        tree = E.parse(str(path), E.XMLParser(resolve_entities=False, no_network=True))
        if tree.docinfo.doctype:
            parser.error("DTD content is not accepted")
        schema.assertValid(tree)
        trees.append(tree)
    expected, actual = map(records, trees)
    differences = [{"path": path, "before": expected.get(path), "after": actual.get(path)}
                   for path in sorted(expected.keys() | actual.keys()) if expected.get(path) != actual.get(path)]
    print(json.dumps({"source": str(arguments.source), "output": str(arguments.output),
                      "sourceOwnerAndBodyRecords": sum(not path.endswith("/xmlIdentity") for path in expected),
                      "standardElementIdentityRecords": sum(path.endswith("/xmlIdentity") for path in expected),
                      "anonymousOwnerRecords": sum(path.endswith("/metadata") and not value["idPresent"]
                                                   for path, value in expected.items()),
                      "differences": differences}, indent=2))
    return int(bool(differences))


if __name__ == "__main__":
    raise SystemExit(main())
