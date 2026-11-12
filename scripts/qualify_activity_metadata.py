#!/usr/bin/env python3
"""Reproduce the focused synthetic activity/native/XML checks.

Requires lxml and the built common_metadata_qualify example. This is a bounded
fixture qualification, not a BPMN conformance or external-consumer test.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ["activity-correlation", "activity-callable-roles", "activity-loop-qnames"]


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    binary = arguments.binary.resolve(strict=True)
    output = arguments.output
    if not output.is_absolute() or output.exists():
        parser.error("output must be a new absolute directory")
    output.mkdir(parents=True)
    (output / "outputs").mkdir()
    sources = [ROOT / f"tests/fixtures/bpmn/{name}.bpmn" for name in FIXTURES]
    manifest = {
        "binary": str(binary), "binarySha256": sha256(binary),
        "fixtures": [{"path": str(path.relative_to(ROOT)), "sha256": sha256(path)} for path in sources],
        "schemaSha256": {name: sha256(ROOT / "tests/fixtures/bpmn20-schema" / name)
                         for name in ["BPMN20.xsd", "Semantic.xsd", "BPMNDI.xsd", "DI.xsd", "DC.xsd"]},
        "limits": "Exact native codec identity and XML-representable model/source fields; no Windows file safety, consumer or complete modeling claim.",
    }
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    results = []
    for name, source in zip(FIXTURES, sources):
        destination = output / "outputs" / name
        run = subprocess.run([str(binary), str(source), str(destination)], cwd=ROOT,
                             text=True, capture_output=True, timeout=60)
        result = {"fixture": name, "exitCode": run.returncode, "stdout": run.stdout, "stderr": run.stderr}
        if run.returncode == 0:
            compared = subprocess.run([sys.executable, str(ROOT / "scripts/check_bpmn_metadata_source.py"),
                "--activity-fields", str(source), str(destination / "output.bpmn")],
                cwd=ROOT, text=True, capture_output=True, timeout=60)
            (destination / "source-comparison.json").write_text(compared.stdout)
            result.update(sourceComparisonExitCode=compared.returncode, sourceComparisonError=compared.stderr)
        results.append(result)
        print(name, "PASS" if result["exitCode"] == 0 and result.get("sourceComparisonExitCode") == 0 else "FAIL")
    (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    return int(any(result["exitCode"] != 0 or result.get("sourceComparisonExitCode") != 0 for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
