#!/usr/bin/env python3
"""Reproduce bounded collaboration/native/XML checks on fixed synthetic inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from lxml import etree as E

ROOT = Path(__file__).resolve().parents[1]
BPMN = "http://www.omg.org/spec/BPMN/20100524/MODEL"
FIXTURES = ["collaboration-multiple", "choreography-inherited"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reverse_roots(root):
    children = list(root)
    positions = [index for index, child in enumerate(children)
                 if isinstance(child.tag, str) and child.tag.startswith("{" + BPMN + "}")
                 and E.QName(child).localname not in {
                     "import", "extension", "documentation", "extensionElements", "relationship"}]
    replacements = list(reversed([children[index] for index in positions]))
    for index, child in zip(positions, replacements):
        children[index] = child
    root[:] = children


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
    negatives = args.output / "negatives"
    for directory in [inputs, outputs, negatives]:
        directory.mkdir()
    source_records = []
    for name in FIXTURES:
        source = ROOT / f"tests/fixtures/bpmn/{name}.bpmn"
        original = source.read_bytes()
        source_records.append({"file": str(source.relative_to(ROOT)), "sha256": digest(source)})
        (inputs / f"{name}-original.bpmn").write_bytes(original)
        # Rename only the fixed fixtures' model-reference namespace aliases.
        aliases = original
        for before in [b"tns", b"t"]:
            aliases = aliases.replace(b"xmlns:" + before + b"=", b"xmlns:model=")
            aliases = aliases.replace(b'"' + before + b":", b'"model:')
            aliases = aliases.replace(b">" + before + b":", b">model:")
        (inputs / f"{name}-aliases.bpmn").write_bytes(aliases)
        for variant in ["optional-ids", "reversed-roots"]:
            root = E.fromstring(original, E.XMLParser(resolve_entities=False, no_network=True))
            if variant == "optional-ids":
                for element in root.iter():
                    if element.get("id") in {"Definitions", "Global", "Diagram", "Plane"}:
                        del element.attrib["id"]
            else:
                reverse_roots(root)
            (inputs / f"{name}-{variant}.bpmn").write_bytes(
                E.tostring(root, xml_declaration=True, encoding="UTF-8"))
    (args.output / "manifest.json").write_text(json.dumps({
        "sourceFixtures": source_records, "binary": str(binary), "binarySha256": digest(binary),
        "inputs": [{"file": path.name, "sha256": digest(path)} for path in sorted(inputs.glob("*.bpmn"))],
        "schemas": {name: digest(ROOT / "tests/fixtures/bpmn20-schema" / name)
                    for name in ["BPMN20.xsd", "Semantic.xsd", "BPMNDI.xsd", "DI.xsd", "DC.xsd"]},
        "limits": "Fixed synthetic collaboration fixtures only; no full M3, vendor, Windows file-safety or accessibility qualification.",
    }, indent=2) + "\n")
    checker = [sys.executable, str(ROOT / "scripts/check_bpmn_metadata_source.py"),
               "--activity-fields", "--diagram-fields", "--collaboration-fields"]
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
    source = inputs / "collaboration-multiple-original.bpmn"
    exported = outputs / source.stem / "output.bpmn"
    negative_results = []
    if exported.exists():
        for name in ["omitted-false", "changed-qname-uri", "reversed-root-order"]:
            tree = E.parse(str(exported), E.XMLParser(resolve_entities=False, no_network=True))
            root = tree.getroot()
            if name == "omitted-false":
                del root.find("{" + BPMN + "}collaboration").attrib["isClosed"]
            elif name == "changed-qname-uri":
                group = root.find(".//{" + BPMN + "}group")
                # In this fixed fixture the unprefixed QName uses the BPMN
                # default namespace, changing its URI while retaining local part.
                group.set("categoryValueRef", group.get("categoryValueRef").split(":")[-1])
            changed = negatives / f"{name}.bpmn"
            if name == "reversed-root-order":
                # Reuse the independently XSD-checked source variant. Reparenting
                # output DOM nodes can alter their nested default namespace scope.
                changed.write_bytes((inputs / "collaboration-multiple-reversed-roots.bpmn").read_bytes())
            else:
                tree.write(str(changed), encoding="UTF-8", xml_declaration=True)
            comparison = subprocess.run([*checker, str(source), str(changed)], cwd=ROOT,
                                        text=True, capture_output=True, timeout=60)
            (negatives / f"{name}.json").write_text(comparison.stdout)
            differences = json.loads(comparison.stdout).get("differences", []) if comparison.stdout else []
            passed = comparison.returncode == 1 and bool(differences)
            negative_results.append({"case": name, "pass": passed,
                                     "sourceComparisonExitCode": comparison.returncode, "stderr": comparison.stderr})
            print(name, "PASS (difference detected)" if passed else "FAIL", flush=True)
    (args.output / "results.json").write_text(json.dumps({"positive": results, "negative": negative_results}, indent=2) + "\n")
    return int(any(row["exitCode"] != 0 or row.get("sourceComparisonExitCode") != 0 for row in results)
               or len(negative_results) != 3 or any(not row["pass"] for row in negative_results))


if __name__ == "__main__":
    raise SystemExit(main())
