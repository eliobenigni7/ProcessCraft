#!/usr/bin/env python3
"""Reproduce focused DI/native/XML checks using independently derived inputs.

Requires lxml and the built common_metadata_qualify example. This does not
establish complete DI/choreography coverage or vendor interoperability.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests/fixtures/bpmn/di-choreography-representation.bpmn"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not args.output.is_absolute() or args.output.exists():
        parser.error("output must be a new absolute directory")
    args.output.mkdir(parents=True)
    inputs = args.output / "inputs"
    outputs = args.output / "outputs"
    inputs.mkdir()
    outputs.mkdir()
    original = SOURCE.read_bytes()
    (inputs / "original.bpmn").write_bytes(original)
    aliases = original
    for before, after in [(b"bpmndi", b"diagram"), (b"dc", b"geometry"), (b"di", b"representation")]:
        aliases = aliases.replace(b"xmlns:" + before + b"=", b"xmlns:" + after + b"=")
        aliases = aliases.replace(b"<" + before + b":", b"<" + after + b":")
        aliases = aliases.replace(b"</" + before + b":", b"</" + after + b":")
    (inputs / "prefix-aliases.bpmn").write_bytes(aliases)
    tree = E.fromstring(original, E.XMLParser(resolve_entities=False, no_network=True))
    # These owners are unreferenced in this fixed fixture. Keep TaskShape's XML
    # ID because choreographyActivityShape actually references it.
    unreferenced = {"Definitions", "Diagram", "Plane", "FlowEdge", "BuyerBand", "SellerBand", "Multiplicity"}
    for element in tree.iter():
        if element.get("id") in unreferenced:
            del element.attrib["id"]
    (inputs / "optional-ids.bpmn").write_bytes(E.tostring(tree, xml_declaration=True, encoding="UTF-8"))
    manifest = {
        "source": str(SOURCE.relative_to(ROOT)), "sourceSha256": digest(SOURCE),
        "binary": str(binary), "binarySha256": digest(binary),
        "derivedInputs": [{"file": path.name, "sha256": digest(path)} for path in sorted(inputs.glob("*.bpmn"))],
        "schemaSha256": {name: digest(ROOT / "tests/fixtures/bpmn20-schema" / name)
            for name in ["BPMN20.xsd", "Semantic.xsd", "BPMNDI.xsd", "DI.xsd", "DC.xsd"]},
        "limits": "Focused synthetic fixtures; native codec identity, XML model meaning, actual source/output schema and selected raw DI fields only.",
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    results = []
    for source in sorted(inputs.glob("*.bpmn")):
        destination = outputs / source.stem
        run = subprocess.run([str(binary), str(source), str(destination)], cwd=ROOT, text=True,
                             capture_output=True, timeout=60)
        result = {"fixture": source.stem, "exitCode": run.returncode, "stdout": run.stdout, "stderr": run.stderr}
        if run.returncode == 0:
            compared = subprocess.run([sys.executable, str(ROOT / "scripts/check_bpmn_metadata_source.py"),
                "--diagram-fields", str(source), str(destination / "output.bpmn")], cwd=ROOT,
                text=True, capture_output=True, timeout=60)
            (destination / "source-comparison.json").write_text(compared.stdout)
            result.update(sourceComparisonExitCode=compared.returncode, sourceComparisonError=compared.stderr)
        results.append(result)
        print(source.stem, "PASS" if result["exitCode"] == 0 and result.get("sourceComparisonExitCode") == 0 else "FAIL")
    (args.output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    return int(any(row["exitCode"] != 0 or row.get("sourceComparisonExitCode") != 0 for row in results))


if __name__ == "__main__":
    raise SystemExit(main())
