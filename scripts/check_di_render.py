"""Independently check the explicit bpmn_render_qualify PDF/PNG artifacts."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
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
    original = Image.open(path.with_suffix(".png")).convert("RGB")
    # Export uses row-major A4 landscape tiles with 24-point margins. At
    # 96 dpi each scene pixel is one device pixel at the selected export scale.
    # Check every page against its source viewport; a first-page crop cannot
    # qualify a larger scene and Pillow would silently pad that crop with black.
    scale = int(re.search(r"-(\d+)x\.pdf$", path.name).group(1))
    width, height = (float(value) * scale for value in sample["logicalSize"])
    usable_width = (841.89 - 48) * 96 / 72
    usable_height = (595.28 - 48) * 96 / 72
    columns = max(1, math.ceil(width / usable_width))
    rows = max(1, math.ceil(height / usable_height))
    page_means = []
    stitched = Image.new("RGB", original.size)
    for index in range(min(len(reader.pages), columns * rows)):
        prefix = str(path.with_suffix("")) + f"-page-{index + 1}-poppler"
        subprocess.run(["pdftoppm", "-f", str(index + 1), "-l", str(index + 1),
                        "-singlefile", "-r", "96", "-png", str(path), prefix],
                       check=True, capture_output=True)
        raster = Image.open(prefix + ".png").convert("RGB")
        column, row = index % columns, index // columns
        left, top = round(column * usable_width), round(row * usable_height)
        right = min(original.width, round((column + 1) * usable_width))
        bottom = min(original.height, round((row + 1) * usable_height))
        crop = raster.crop((32, 32, 32 + right - left, 32 + bottom - top))
        expected = original.crop((left, top, right, bottom))
        page_means.append(sum(ImageStat.Stat(ImageChops.difference(crop, expected)).mean) / 3)
        stitched.paste(crop, (left, top))
    mean = sum(ImageStat.Stat(ImageChops.difference(stitched, original)).mean) / 3
    resources = [page["/Resources"] for page in reader.pages]
    fonts = any(resource.get("/Font") for resource in resources)
    subtypes = [str(ref.get_object().get("/Subtype"))
                for resource in resources for ref in resource.get("/XObject", {}).values()]
    values = {
        "completePageGrid": len(reader.pages) == columns * rows,
        "pngDimensionsMatchScale": original.size == (math.ceil(width), math.ceil(height)),
        "a4Landscape": all(abs(float(page.mediabox.width) - 841.89) < .02
                           and abs(float(page.mediabox.height) - 595.28) < .02 for page in reader.pages),
        "outlinedVector": bool(subtypes) and all(value == "/Form" for value in subtypes) and not fonts,
        "noSearchableText": all(not (page.extract_text() or "").strip() for page in reader.pages),
        "noJavaScript": "JavaScript:      no" in info,
        "acknowledgmentRequired": sample["unacknowledgedRefused"],
        "pngParity": bool(page_means) and mean < 3 and max(page_means) < 3,
        "artifactHashMatches": hashlib.sha256(path.read_bytes()).hexdigest() == sample["sha256"],
    }
    checks.append({"file": path.name, "checks": values, "pages": len(reader.pages),
                   "expectedGrid": [columns, rows], "pageMeanAbsoluteRgbDifferences": page_means,
                   "meanAbsoluteRgbDifference": mean})
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
