#!/usr/bin/env python3
"""Reproduce raw-source and native/XML checks on a fixed synthetic graph fixture.

The binary is the caller-supplied qualification executable accepting INPUT OUTPUT_DIR.
All XML parsing disables DTD/entity expansion and network access. No source is executed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
BPMN = "http://www.omg.org/spec/BPMN/20100524/MODEL"
FIXTURE = ROOT / "tests/fixtures/bpmn/choreography-graph/nested.bpmn"
FIXTURE_MANIFEST = FIXTURE.with_name("manifest.md")
EVENT_FIXTURE = FIXTURE.with_name("boundary-event-io.bpmn")
SUBPLANE_FIXTURE = FIXTURE.with_name("subplane.bpmn")
EXPECTED_SUBPLANE_SHA = "9b88a93da5c587ec70c0fe5be16a3c78b0a6375c88e92cfa724d4e48aee07c63"
EXPECTED_EVENT_SHA = "8b2236cda849334a7208aa55f1c6b616cfa6653dfb5f2380c4360e4cc9b9c102"
SCHEMA_DIR = ROOT / "tests/fixtures/bpmn20-schema"
SCHEMAS = {
    "BPMN20.xsd": "a07c159cb0594573dd7c97b1370dd116112378f377e43c89a8bf512ac5030705",
    "Semantic.xsd": "c4318842f7d2bbc262d7954c9452c501db16f0868eac0b8732ec5d7fb384d9a7",
    "BPMNDI.xsd": "f0dff1cd559d1514d8ebfc8c646f58402bcaced27ec22e2aa6456c2dcc80b038",
    "DI.xsd": "8220b179c175572df74e08a51bffabe957867962035cee7b5fee0b6acb4c4498",
    "DC.xsd": "a2f90e5ad9bb48c6915e4e034b4e27ac838264a1d4f27bfc70dbdfc69351312d",
}
EXPECTED_FIXTURE_SHA = "c693cace1398f49fa3e0be82604b03f78be26d8572c4f757972e0d63bafa6f1a"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_bytes(data):
    root = E.fromstring(data, E.XMLParser(resolve_entities=False, no_network=True))
    if root.getroottree().docinfo.doctype:
        raise ValueError("DTD content is not accepted")
    return root


def write_xml(path, root):
    path.write_bytes(E.tostring(root, xml_declaration=True, encoding="UTF-8"))


def reorder(root):
    parent = root.find("{" + BPMN + "}choreography")
    first = parent.find("{" + BPMN + "}startEvent")
    second = parent.find("{" + BPMN + "}subChoreography")
    first_index, second_index = parent.index(first), parent.index(second)
    parent.remove(second)
    parent.insert(first_index, second)
    assert parent.index(first) == second_index


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not args.output.is_absolute() or args.output.exists():
        parser.error("output must be a new absolute directory")
    manifest_text = FIXTURE_MANIFEST.read_text()
    for fixture, expected in [(FIXTURE, EXPECTED_FIXTURE_SHA), (EVENT_FIXTURE, EXPECTED_EVENT_SHA),
                              (SUBPLANE_FIXTURE, EXPECTED_SUBPLANE_SHA)]:
        if digest(fixture) != expected or expected not in manifest_text:
            parser.error(f"fixed fixture hash does not match its manifest: {fixture.name}")
    if any(digest(SCHEMA_DIR / name) != expected for name, expected in SCHEMAS.items()):
        parser.error("local schema hash does not match the pinned OMG sources")
    original = FIXTURE.read_bytes()
    args.output.mkdir(parents=True)
    inputs, outputs, negatives = (args.output / name for name in ("inputs", "outputs", "negatives"))
    for directory in (inputs, outputs, negatives):
        directory.mkdir()
    subplane_original = SUBPLANE_FIXTURE.read_bytes()
    (inputs / "subplane-original.bpmn").write_bytes(subplane_original)
    subplane_aliases = subplane_original.replace(b"xmlns:t=", b"xmlns:model=").replace(b'"t:', b'"model:').replace(b">t:", b">model:")
    (inputs / "subplane-aliases.bpmn").write_bytes(subplane_aliases)
    subplane_root = parse_bytes(subplane_original)
    for element in subplane_root.iter():
        if element.get("id") in {"defs", "diagram", "plane", "startShape", "buyerBand", "sellerBand", "innerStartShape", "innerEndShape", "innerFlowEdge", "outerFlowEdge"}:
            del element.attrib["id"]
    write_xml(inputs / "subplane-optional-ids.bpmn", subplane_root)
    event_original = EVENT_FIXTURE.read_bytes()
    (inputs / "boundary-original.bpmn").write_bytes(event_original)
    event_aliases = event_original.replace(b"xmlns:t=", b"xmlns:model=").replace(b'"t:', b'"model:').replace(b">t:", b">model:")
    (inputs / "boundary-aliases.bpmn").write_bytes(event_aliases)
    event_root = parse_bytes(event_original)
    for element in event_root.iter():
        if element.get("id") in {"defs", "association1", "association2", "set1", "set2", "timer1", "signal2"}:
            del element.attrib["id"]
    write_xml(inputs / "boundary-optional-ids.bpmn", event_root)
    (inputs / "original.bpmn").write_bytes(original)
    aliases = original.replace(b"xmlns:t=", b"xmlns:model=").replace(b'"t:', b'"model:').replace(b">t:", b">model:")
    (inputs / "aliases.bpmn").write_bytes(aliases)
    root = parse_bytes(original)
    for element in root.iter():
        if element.get("id") in {"defs", "signalDefinition", "mapping", "note"}:
            del element.attrib["id"]
    write_xml(inputs / "optional-ids.bpmn", root)
    root = parse_bytes(original)
    reorder(root)
    write_xml(inputs / "interleaved.bpmn", root)
    for value in [None, "None", "Standard", "MultiInstanceSequential", "MultiInstanceParallel"]:
        root = parse_bytes(original)
        sub = root.find(".//{" + BPMN + "}subChoreography")
        if value is None:
            del sub.attrib["loopType"]
        else:
            sub.set("loopType", value)
        write_xml(inputs / f"loop-{value if value is not None else 'absent'}.bpmn", root)
    schema = E.XMLSchema(E.parse(str(SCHEMA_DIR / "BPMN20.xsd"), E.XMLParser(resolve_entities=False, no_network=True)))
    for path in sorted(inputs.glob("*.bpmn")):
        schema.assertValid(parse_bytes(path.read_bytes()))
    (args.output / "manifest.json").write_text(json.dumps({
        "fixtures": [{"path": str(fixture.relative_to(ROOT)), "sha256": digest(fixture), "license": "CC0-1.0"}
                     for fixture in (FIXTURE, EVENT_FIXTURE, SUBPLANE_FIXTURE)],
        "fixtureManifest": {"path": str(FIXTURE_MANIFEST.relative_to(ROOT)), "sha256": digest(FIXTURE_MANIFEST)},
        "binary": str(binary), "binarySha256": digest(binary),
        "schemas": {name: digest(SCHEMA_DIR / name) for name in SCHEMAS},
        "inputs": [{"file": path.name, "sha256": digest(path)} for path in sorted(inputs.glob("*.bpmn"))],
        "limits": "Fixed synthetic graph variants only; no full M3, vendor, accessibility or conformance qualification.",
    }, indent=2) + "\n")
    checker = [sys.executable, str(ROOT / "scripts/check_bpmn_metadata_source.py"),
               "--activity-fields", "--diagram-fields", "--collaboration-fields", "--choreography-fields"]
    results = []
    for source in sorted(inputs.glob("*.bpmn")):
        destination = outputs / source.stem
        run = subprocess.run([str(binary), str(source), str(destination)], cwd=ROOT,
                             text=True, capture_output=True, timeout=60)
        row = {"fixture": source.stem, "exitCode": run.returncode, "stdout": run.stdout, "stderr": run.stderr}
        if run.returncode == 0:
            comparison = subprocess.run([*checker, str(source), str(destination / "output.bpmn")],
                                        cwd=ROOT, text=True, capture_output=True, timeout=60)
            (destination / "source-comparison.json").write_text(comparison.stdout)
            row.update(sourceComparisonExitCode=comparison.returncode, sourceComparisonError=comparison.stderr)
        results.append(row)
        print(source.stem, "PASS" if row["exitCode"] == row.get("sourceComparisonExitCode") == 0 else "FAIL", flush=True)
    negative_sources = {
        "omitted-explicit-loop-none": (inputs / "loop-absent.bpmn").read_bytes(),
        "external-uri-same-local": original.replace(b'xmlns:ext="urn:external"', b'xmlns:ext="urn:changed"'),
        "sibling-order": (inputs / "interleaved.bpmn").read_bytes(),
    }
    expected_record = {"omitted-explicit-loop-none": "/choreographyAttributes",
                       "external-uri-same-local": "/choreographyQName",
                       "sibling-order": "/choreographyChildren"}
    negative_results = []
    for name, data in negative_sources.items():
        changed = negatives / f"{name}.bpmn"
        changed.write_bytes(data)
        schema.assertValid(parse_bytes(data))
        comparison = subprocess.run([*checker, str(inputs / "original.bpmn"), str(changed)],
                                    cwd=ROOT, text=True, capture_output=True, timeout=60)
        (negatives / f"{name}.json").write_text(comparison.stdout)
        differences = json.loads(comparison.stdout).get("differences", []) if comparison.stdout else []
        passed = (comparison.returncode == 1
                  and any(item["path"].endswith(expected_record[name]) for item in differences))
        negative_results.append({"case": name, "pass": passed, "sourceComparisonExitCode": comparison.returncode,
                                 "stderr": comparison.stderr, "sha256": digest(changed)})
        print(name, "PASS (difference detected)" if passed else "FAIL", flush=True)
    (args.output / "results.json").write_text(json.dumps({"positive": results, "negative": negative_results}, indent=2) + "\n")
    return int(any(row["exitCode"] != 0 or row.get("sourceComparisonExitCode") != 0 for row in results)
               or any(not row["pass"] for row in negative_results))


if __name__ == "__main__":
    raise SystemExit(main())
