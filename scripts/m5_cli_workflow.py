#!/usr/bin/env python3
"""M5.6 repeatable CLI qualification: attributes, conventions, RACI, overlay, BPMN round trip.

Usage: python scripts/m5_cli_workflow.py [--bin PATH] [--out DIR]
Needs a built processcraft-cli (default target/debug/processcraft-cli[.exe]). Exit 0 only if
every check passes. Fixtures are read from tests/fixtures; nothing there is modified.
"""
import argparse, json, pathlib, subprocess, sys, xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
FX = ROOT / "tests/fixtures"
DOC = "11111111-1111-4111-8111-111111111111"  # raci.json document
ATTR_DOC = "22222222-2222-4222-8222-222222222222"  # attributes.json document
CONV_DOC = "44444444-4444-4444-8444-444444444444"  # conventions/pass.json document
results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin")
    ap.add_argument("--out", default=str(ROOT / "target/m5-workflow"))
    a = ap.parse_args()
    exe = pathlib.Path(a.bin) if a.bin else next(
        p for p in [ROOT / "target/debug/processcraft-cli.exe", ROOT / "target/debug/processcraft-cli"] if p.exists())
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)

    def cli(*args, ok=(0,)):
        r = subprocess.run([str(exe), *map(str, args)], capture_output=True, text=True, encoding="utf-8")
        if r.returncode not in ok:
            sys.exit(f"cli {args} -> {r.returncode}\n{r.stderr}")
        return r

    def jq(*args):
        return json.loads(cli(*args).stdout)

    def run_batch(base, batch, dest):
        base_args = ["--in", base] if base else ["--new", "Procurement"]
        r = jq("run", *base_args, "--batch", batch, "--out", dest, "--overwrite")
        return bytes(r["contentId"]).hex()

    # Project P: conventions fixture (diagram, Cost attribute, stored profile) plus the RACI procurement.
    p = out / "p.processcraft"
    run_batch(None, FX / "conventions/pass.json", out / "p1.processcraft")
    saved_id = run_batch(out / "p1.processcraft", FX / "commands/raci.json", p)
    check("save then reopen keeps content identity (P)", jq("info", p)["contentId"] == saved_id, saved_id[:16])

    # Project Q: every attribute value type (attributes.json) on top of the RACI procurement.
    q = out / "q.processcraft"
    run_batch(None, FX / "commands/raci.json", out / "q1.processcraft")
    saved_q = run_batch(out / "q1.processcraft", FX / "commands/attributes.json", q)
    check("save then reopen keeps content identity (Q)", jq("info", q)["contentId"] == saved_q, saved_q[:16])

    # Conventions: stored profile of P, and the recommended profile as an external file on Q.
    v = jq("validate", p, "--json")
    sev = lambda r: sorted({(d["code"], d["severity"]) for d in r["diagnostics"]})
    check("validate P with stored profile: no errors", not any(s == "error" for _, s in sev(v)), str(sev(v)))
    rec = {"rules": [
        {"type": "labelRequired", "severity": "warning", "kinds": ["Activity", "Event", "Lane", "Participant"]},
        {"type": "activityVerbObject", "severity": "warning", "verbs": []},
        {"type": "gatewayFlowsLabelled", "severity": "warning"},
        {"type": "noImplicitSplitJoin", "severity": "warning"},
        {"type": "singleStartEnd", "severity": "warning"},
        {"type": "maxElements", "severity": "warning", "limit": 50},
        {"type": "requiredAttributes", "severity": "warning"},
        {"type": "dictionaryTerms", "severity": "warning"}]}
    prof = out / "recommended.json"
    prof.write_text(json.dumps(rec))
    v = jq("validate", q, "--json", "--profile", prof)
    check("validate Q with recommended profile file: no errors", not any(s == "error" for _, s in sev(v)), str(sev(v)))
    # Failing fixtures must exit 3 under their warning threshold: one example.
    fail = out / "fail.processcraft"
    run_batch(None, FX / "conventions/fail-activity-verb-object.json", fail)
    r = cli("validate", fail, "--json", "--fail-on", "warning", ok=(0, 3))
    check("a failing convention fixture exits 3 at --fail-on warning", r.returncode == 3, f"exit {r.returncode}")

    # RACI.
    raci = jq("query", p, "raci", DOC, "procurement")
    csv = out / "raci.csv"
    cli("export-raci", p, DOC, "procurement", "--out", csv, "--overwrite")
    expected = (FX / "commands/raci.expected.csv").read_bytes()
    check("export-raci equals the pinned CSV", csv.read_bytes() == expected,
          f"{len(raci['raci']['rows'])} rows x {len(raci['raci']['columns'])} columns")

    # Overlay: Cost definition of P, SVG with legend vs without.
    attrs = jq("query", p, "attributes", CONV_DOC)
    cost = next(d["definition"]["id"] for d in attrs["definitions"] if d["definition"]["name"] == "Cost")
    diagram = jq("info", p)["documents"]
    diag = next(d["diagrams"][0] for d in diagram if d["id"] == CONV_DOC)
    plain, over = out / "plain.svg", out / "overlay.svg"
    for dest, extra in ((plain, []), (over, ["--overlay-attribute", cost])):
        cli("export-image", p, "--document-id", CONV_DOC, "--diagram-id", diag, "--format", "svg",
            "--out", dest, "--overwrite", *extra)
    check("overlay SVG differs from plain and grows", over.stat().st_size > plain.stat().st_size,
          f"{plain.stat().st_size} -> {over.stat().st_size} bytes")

    # BPMN round trip of the attribute document of Q.
    bpmn = out / "q.bpmn"
    cli("export-bpmn", q, "--document-id", ATTR_DOC, "--out", bpmn, "--overwrite", "--acknowledge-native-fields")
    ns = {"pc": "urn:processcraft:attributes:1"}
    xml = ET.parse(bpmn).getroot()
    n_xml = len(list(xml.iter("{urn:processcraft:attributes:1}attributes")))
    back = out / "q-back.processcraft"
    cli("import-bpmn", bpmn, "--out", back, "--overwrite")
    back_doc = jq("info", back)["documents"][0]["id"]

    def values(path, doc):
        d = jq("query", path, "attributes", doc)
        defs = {x["definition"]["id"]: x["definition"] for x in d["definitions"]}
        rows = {}
        for row in d["rows"]:
            for val in row["values"]:
                rows[(row["key"]["xml_id"], val["name"])] = json.dumps(val["value"], sort_keys=True)
        return {x["definition"]["name"]: (x["definition"]["value_type"], x["usage_count"]) for x in d["definitions"]}, rows

    d0, v0 = values(q, ATTR_DOC)
    d1, v1 = values(back, back_doc)
    used = {n for _, n in v0}  # definitions used only by other documents are not exported
    d0 = {k: x for k, x in d0.items() if k in used}
    ref = lambda r: {k: x for k, x in r.items() if "DictionaryRefs" not in x}  # entry IDs are native-only
    check("BPMN carries pc:attributes blocks", n_xml > 0, f"{n_xml} blocks")
    check("re-import keeps definitions (name, type, usage)", d0 == d1, f"{len(d0)} definitions")
    check("re-import keeps non-reference attribute values", ref(v0) == ref(v1),
          f"{len(ref(v0))} of {len(v0)} values compared; dictionary references excluded (native-only)")
    print("  definition diff:", {k: (d0.get(k), d1.get(k)) for k in set(d0) | set(d1) if d0.get(k) != d1.get(k)})
    bad = [k for k in ref(v0) if v0[k] != v1.get(k)]
    if bad:
        print("  differing:", bad[:5])
    failed = [n for n, ok, _ in results if not ok]
    print(f"{len(results) - len(failed)}/{len(results)} checks passed")
    sys.exit(1 if failed else 0)


main()
