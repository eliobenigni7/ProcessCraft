"""Validate supplied BPMN fixtures with the locally pinned OMG 2.0.2 XSD.
Requires lxml; this checks XML schema only, not BPMN semantic conformance.
"""
import sys
from pathlib import Path
from lxml import etree

root = Path(__file__).resolve().parent.parent
schema = etree.XMLSchema(etree.parse(str(root / "tests/fixtures/bpmn20-schema/BPMN20.xsd")))
paths = [Path(p) for p in sys.argv[1:]] or sorted((root / "tests/fixtures/bpmn20-schema").glob("*.bpmn"))
for path in paths:
    schema.assertValid(etree.parse(str(path)))
    print(f"XSD valid: {path}")
