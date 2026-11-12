"""Regenerate the ProcessCraft app icon with VectorCraft.

The icon is drawn by VectorCraft from `assets/app-icon/processcraft-icon.commands.json` (a
VectorCraft command batch): this script runs that batch headlessly, saves the native
`processcraft.vectorcraft` document and the SVG master, exports every PNG size with
`vectorcraft-cli convert`, and packs the Windows `.ico` from those PNGs (VectorCraft does not
write .ico; Pillow only packs the already rendered images).

Usage:  python scripts/generate_app_icon.py [path/to/vectorcraft-cli]
Default CLI location: %LOCALAPPDATA%/Programs/ArtCraft/VectorCraft/vectorcraft-cli(.exe), or on PATH.
Requires VectorCraft and Pillow.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "app-icon"
BATCH = OUT / "processcraft-icon.commands.json"
SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)


def vectorcraft_cli() -> str:
    if len(sys.argv) > 1:
        return sys.argv[1]
    exe = "vectorcraft-cli.exe" if os.name == "nt" else "vectorcraft-cli"
    local = Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "ArtCraft" / "VectorCraft" / exe
    if local.exists():
        return str(local)
    found = shutil.which("vectorcraft-cli")
    if not found:
        raise SystemExit("vectorcraft-cli not found; pass its path as the first argument")
    return found


def run(*args: str) -> None:
    subprocess.run(args, check=True, capture_output=True)


def main() -> None:
    cli = vectorcraft_cli()
    native = OUT / "processcraft.vectorcraft"
    run(cli, "run", "--commands", str(BATCH), "--export", str(native), "--export", str(OUT / "processcraft.svg"))
    # The document is 512 units wide: --scale is output pixels per unit.
    for size in SIZES:
        run(cli, "convert", str(native), str(OUT / f"processcraft-{size}.png"), "--scale", str(size / 512))
    # Every .ico entry is VectorCraft's own render at that size; Pillow only packs them.
    images = [Image.open(OUT / f"processcraft-{s}.png").convert("RGBA") for s in reversed(ICO_SIZES)]
    images[0].save(
        OUT / "processcraft.ico",
        sizes=[(s, s) for s in ICO_SIZES],
        append_images=images[1:],
    )
    print("wrote", OUT)


if __name__ == "__main__":
    main()
