"""Independently check the explicit bpmn_render_qualify PDF/PNG artifacts."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from PIL import Image, ImageChops, ImageStat
import PIL
import pypdf
from pypdf import PdfReader

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory.resolve(strict=True)
manifest = json.loads((root / "report.json").read_text())
checks = []
for sample in manifest["exports"]:
    path = root / sample["file"]
    if path.suffix != ".pdf":
        continue
    reader = PdfReader(path, strict=True)
    info = subprocess.run(["pdfinfo", str(path)], check=True, capture_output=True, text=True).stdout
    prefix = path.with_suffix("")
    subprocess.run(["pdftoppm", "-singlefile", "-r", "96", "-png", str(path), str(prefix) + "-poppler"],
                   check=True, capture_output=True)
    raster = Image.open(str(prefix) + "-poppler.png").convert("RGB")
    original = Image.open(path.with_suffix(".png")).convert("RGB")
    crop = raster.crop((32, 32, 32 + original.width, 32 + original.height))
    mean = sum(ImageStat.Stat(ImageChops.difference(crop, original)).mean) / 3
    resources = [page["/Resources"] for page in reader.pages]
    fonts = any(resource.get("/Font") for resource in resources)
    subtypes = [str(ref.get_object().get("/Subtype"))
                for resource in resources for ref in resource.get("/XObject", {}).values()]
    values = {
        "singlePage": len(reader.pages) == 1,
        "a4Landscape": all(abs(float(page.mediabox.width) - 841.89) < .02
                           and abs(float(page.mediabox.height) - 595.28) < .02 for page in reader.pages),
        "outlinedVector": bool(subtypes) and all(value == "/Form" for value in subtypes) and not fonts,
        "noSearchableText": all(not (page.extract_text() or "").strip() for page in reader.pages),
        "noJavaScript": "JavaScript:      no" in info,
        "acknowledgmentRequired": sample["unacknowledgedRefused"],
        "pngParity": mean < 3,
        "artifactHashMatches": hashlib.sha256(path.read_bytes()).hexdigest() == sample["sha256"],
    }
    checks.append({"file": path.name, "checks": values, "meanAbsoluteRgbDifference": mean})
    print(path.name, "PASS" if all(values.values()) else "FAIL", round(mean, 3))
report = {"samples": checks, "tools": {
    "pypdf": pypdf.__version__, "Pillow": PIL.__version__,
    "Poppler": subprocess.run(["pdfinfo", "-v"], check=True, capture_output=True,
                              text=True).stderr.strip(),
}, "exportReportSha256": hashlib.sha256((root / "report.json").read_bytes()).hexdigest(),
    "limitations": [
    "Focused synthetic scenes only; Poppler and tiny-skia use different antialiasing.",
    "Outlined text is neither searchable nor tagged; this is not accessibility qualification."]}
(root / "independent-render-checks.json").write_text(json.dumps(report, indent=2) + "\n")
raise SystemExit(int(not checks or not all(all(row["checks"].values()) for row in checks)))
