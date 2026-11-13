"""Fail if a tracked text file carries a BOM, NUL or other control characters.

Catches files corrupted by shell rewrites (PowerShell BOM/NUL, escape sequences such as \a turned
into BEL). Pinned fixtures and fonts are byte-exact by contract and skipped.
"""
import re
import subprocess
import sys

TEXT = (".rs", ".toml", ".md", ".json", ".py", ".ps1", ".wxs", ".wxi", ".wxl", ".yml", ".yaml",
        ".html", ".css", ".js", ".svg", ".txt", ".sh")
SKIP = ("tests/fixtures/", "assets/fonts/")
BAD = re.compile(rb"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")

files = subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout.split(b"\0")
problems = []
for name in (f.decode() for f in files if f):
    if not name.endswith(TEXT) or name.startswith(SKIP):
        continue
    data = open(name, "rb").read()
    if data.startswith(b"\xef\xbb\xbf"):
        problems.append(f"{name}: UTF-8 BOM")
    for n, line in enumerate(data.split(b"\n"), 1):
        m = BAD.search(line)
        if m:
            problems.append(f"{name}:{n}: control byte 0x{m.group()[0]:02x}")
print("\n".join(problems) or "text hygiene ok")
sys.exit(1 if problems else 0)
