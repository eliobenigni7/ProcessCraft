# M4 evidence — Windows installer

Recorded 2026-11-13 on Windows 11 Pro 10.0.26300 (x64), WiX Toolset 5.0.2, Rust 1.98.1.

## What exists

`packaging/windows/` follows the Crafting Apps' FigCraft installer (`storytold/figcraft`
`packaging/windows/`), reduced to what ProcessCraft needs today:

- `processcraft.wxs`: one MSI, `Scope="perUserOrMachine"`. The default is a per-user install
  (`ALLUSERS=2`, `MSIINSTALLPERUSER=1`) into `%LOCALAPPDATA%\Programs\ProcessCraft`, with no
  administrator prompt. It contains `processcraft.exe`, `processcraft-cli.exe`, `LICENSE.txt` and the
  font licences (`licenses\`). It registers a Start menu shortcut, `.processcraft` as the default
  handler, `.bpmn` under "Open with" only, `App Paths` (Win+R `processcraft`), and
  `Software\ProcessCraft\InstallDir`. `MajorUpgrade` with a fixed `UpgradeCode`. Wizard art is drawn
  from the app icon in the icon palette; page titles name the full version (`processcraft.en-us.wxl`).
- `install-policy.wxi` (adapted from FigCraft's, MIT): the first wizard page asks "Only for me"
  (default) or "Everyone"; an update reuses the folder the installed copy recorded; a copy in the
  other context is refused with a message.
- `package.ps1`: static-CRT release build, PE checks (x64; app GUI subsystem 2, CLI console 3),
  `wix build`, `wix msi validate`, portable ZIP, CLI `--version` smoke test. Signing runs only when
  `PROCESSCRAFT_SIGN` names a signing script; otherwise packages are unsigned with a warning.
- `apps/processcraft/build.rs` embeds `assets/app-icon/processcraft.ico` and version info.

## CI

`.github/workflows/release-windows.yml` runs `package.ps1` on `windows-latest` (manual runs and
`v*` tags), uploads the MSI and ZIP as artifacts, and drafts a GitHub release for tags. The tag must
name the `Cargo.toml` version (`v0.1.0`, or `v0.1.0-rc.1`, which is drafted as a prerelease).

First run, 2026-11-13, tag `v0.1.0-rc.1` (run 43434865783): the reused CI gates and the Windows
job passed, and the draft prerelease holds `processcraft-0.1.0-windows-x64.msi` (55.9 MB) and the
portable ZIP (65.6 MB), both unsigned. `v0.1.0` is kept for the first signed build.

Second run, 2026-11-13, tag `v0.2.0` (run 43493991318, CI on `main` run 43493964667): gates and the
Windows job passed; `processcraft-0.2.0-windows-x64.msi` (56.5 MB) and
`processcraft-0.2.0-windows-x64-portable.zip` (66.5 MB). By the maintainer's decision this release
was published (not a prerelease) while still **unsigned**; the release notes say so. Signing remains
open (roadmap, M4).

Third run, 2026-11-14, tag `v0.2.1` (run 43609469293): gates and the Windows job passed;
`processcraft-0.2.1-windows-x64.msi` (56.8 MB) and `processcraft-0.2.1-windows-x64-portable.zip`
(66.7 MB), published unsigned like 0.2.0. A first `v0.2.1` tag pushed on the wrong commit was
deleted and its run (43609412278) cancelled before it built anything.

Fourth run, 2026-11-14, tag `v0.2.2` (run 43616598917): gates and the Windows job passed;
`processcraft-0.2.2-windows-x64.msi` (56.7 MB) and `processcraft-0.2.2-windows-x64-portable.zip`
(66.7 MB), published unsigned.

## Install policy checker

`packaging/windows/check-install-policy.ps1` runs the policy cases with real `msiexec` calls on
MSIs it builds from the binaries `package.ps1` staged (0.0.9 and 0.1.0, so every upgrade is a real
major upgrade). It refuses to start when ProcessCraft is already installed and uninstalls what it
installs. The per-machine cases run only when it is elevated. Run 2026-11-13, standard user and
then elevated: all checks passed.

## Measured

| Check | Result |
|---|---|
| `wix msi validate` | Passes. Two warnings: ICE57 (HKMU registry with a file KeyPath, expected for a dual-scope package) and ICE61 (same-version upgrades are allowed on purpose) |
| Silent per-user install (`msiexec /i … /qn`), standard user | Exit 0, no elevation; files under `%LOCALAPPDATA%\Programs\ProcessCraft` |
| Registrations after install | `.processcraft` → `ProcessCraft.Project`, open command quoted; `.bpmn\OpenWithProgids` has `ProcessCraft.Bpmn`; App Paths and Start menu `.lnk` present |
| Opening a `.processcraft` through the shell | Starts the installed `processcraft.exe` with the file as its argument |
| Embedded resources | `ProductName`/`FileDescription` ProcessCraft, `ProductVersion` 0.1.0 |
| Major upgrade (rebuilt MSI, new ProductCode) over the installed one | Exit 0; one registered product afterwards |
| Install with `INSTALLFOLDER=%LOCALAPPDATA%\PcCustomTest\`, then a silent upgrade with no folder | Upgrade stays in the custom folder; default folder not created; one product |
| Silent `ALLUSERS=1` install of a new package over a per-user copy | Refused, 1603, with "ProcessCraft is already installed for your account…" in the log; nothing written to HKLM |
| Interactive wizard, first page | Shows "Who should use ProcessCraft?" with Only for me selected (captured with PrintWindow) |
| Wizard bitmaps | 493×312 and 493×58, 24-bit, generated from `processcraft-256.png` |
| Silent uninstall | Exit 0; install folder, file associations, App Paths and shortcut removed; no product left |
| Upgrade from a lower version (checker) | 0.0.9 → 0.1.0, per-user and per-machine: exit 0, one product, version 0.1.0 |
| Downgrade (checker) | 0.0.9 over 0.1.0 refused; 0.1.0 stays installed |
| Per-machine install (checker, elevated) | `ALLUSERS=1`: files under `C:\Program Files\ProcessCraft`, `HKLM\Software\ProcessCraft\InstallDir` recorded; uninstall removes both |
| Per-user over a per-machine copy (checker, elevated) | Refused with "ProcessCraft is already installed for everyone…" |
| Failure mid-install (checker) | A file named `licenses` in the target folder fails `CreateFolders` with error 1312 after both executables were copied; exit 1603, rollback removes them, no product, registry key, association or shortcut left |
| Interactive wizard, every page | Driven by `WM_COMMAND` clicks on the WixUI buttons, captured with PrintWindow: scope → "Welcome to the ProcessCraft 0.1.0 Setup Wizard" (dialog art) → Destination Folder (`%LOCALAPPDATA%\Programs\ProcessCraft\`) → "Ready to install ProcessCraft 0.1.0" (banner art) → "Completed the ProcessCraft 0.1.0 Setup Wizard"; Finish with "Launch ProcessCraft" ticked starts the installed app |
| Cancel on the first page | "ProcessCraft Setup Wizard was interrupted … Your system has not been modified"; nothing installed |
| Sizes (0.1.0) | MSI 55.5 MB, portable ZIP 64.5 MB |

The same-package case is not a context test: re-running the installed MSI with `ALLUSERS=1` enters
maintenance mode (exit 0, nothing changed), so the refusal was checked with a rebuilt package.

A first build with `Scope="perUser"` failed with error 1303: without `ALLUSERS=2` +
`MSIINSTALLPERUSER=1`, Windows Installer does not redirect Program Files to the user profile.

## Not established

- The packages are **unsigned**; SmartScreen warns about an unknown publisher. The intended free
  route is the SignPath Foundation open-source programme (application and CI wiring pending).
- Not run: the interactive "Everyone" path through the wizard with a real UAC prompt (the
  per-machine cases ran silently from an elevated shell), cancelling while files are being copied
  (rollback was triggered by a failure instead), x86/arm64 builds, machines other than the
  development PC.
- The wizard was driven by window messages, not by keyboard or screen-reader use; that belongs to
  the M4.2 accessibility checks.
- On the Completed page the "Launch ProcessCraft" checkbox draws a grey band over the white panel
  (stock WixUI `ExitDialog` behaviour); cosmetic, left as is.
- The interactive checks are not in CI; `check-install-policy.ps1` is run by hand.

## Canvas rendering performance

Probe: `CANVAS_PROBE=1 cargo run --release -p processcraft-cli --example workbench_qualify -- --out DIR`
writes `canvas-probe.json`. It times `BpmnViewport::build` (culling, scene build, label layout,
tiny-skia raster) on the medium fixture's active diagram (500 shapes) at 1600×900 logical pixels,
canvas light appearance, one long-lived `TextEngine` like the render worker, 3 warm-ups: 30 pans of
100 px and a 30-step zoom ramp 0.5→2.0, at 1.0 and 1.5 pixels per point. "Convert" copies the
RGBA bytes into 4-byte texels, the work of `ColorImage::from_rgba_premultiplied` on an opaque
pixmap. Release build, development PC (Windows 11), wall time; repeated runs differ by up to ~30%
at p95.

Baseline (before the canvas changes), p50 / p95 in ms:

| Series | ppp 1.0 | ppp 1.5 |
|---|---|---|
| Pan, 100 px steps | 21.7 / 30.0 | 26.3 / 38.3 |
| Zoom ramp 0.5→2.0 | 31.7 / 45.0 | 46.8 / 53.8 |
| Convert (1.44 / 3.24 Mpx) | 0.8 / 0.8 | 1.8 / 2.0 |

Every pan or zoom step needed one such build before the canvas showed the new view; the stale
texture was shifted and scaled meanwhile, and the result waited up to 30 ms more for the next
polled frame.

Changes, in order: the render worker wakes the UI when a result is ready (no 30 ms poll), takes
the pixmap buffer instead of copying it and converts it to an egui image off the UI thread; the UI
updates one texture in place. `TextEngine` keeps a bounded layout cache (two generations of 4096
layouts, keyed by every layout input); a viewport build clones only the active diagram and builds
the semantic index once instead of cloning the document and indexing it twice. The canvas then asks
for the viewport plus half its size on every side (less when 16384 px per axis or 64 MiB would be
exceeded) and stops putting the camera in the render key: while the latest raster covers the canvas
and the zoom stays within 0.8–1.25 of it, pan and zoom only move and scale the textured quad on the
GPU. A new raster is asked for when coverage or the zoom band runs out, when the generation, theme,
overlay, size or DPI change, and 100 ms after the last camera change, at the exact scale.

After (same probe, release), p50 / p95 in ms:

| Series | ppp 1.0 | ppp 1.5 |
|---|---|---|
| Pan, viewport only | 16.4 / 24.3 | 22.3 / 30.3 |
| Zoom ramp, viewport only | 23.1 / 35.0 | 38.6 / 44.1 |
| Pan, overscanned raster (what the desktop renders) | 44.7 / 49.8 | 58.9 / 65.9 |
| Zoom ramp, overscanned raster | 51.1 / 57.3 | 74.0 / 98.1 |
| Convert, overscanned (5.76 / 12.96 Mpx), now on the worker | 3.2 / 3.3 | 7.4 / 8.1 |

The cache and the removed copy cut a viewport build by roughly a quarter (pan p95 30.0 → 24.3 ms at
ppp 1.0). Overscan makes each raster about 2–2.5× slower, but it is no longer on the pan/zoom path:
with an 800 px margin a 100 px-per-step pan needs a raster only every eighth step, the zoom ramp
(×1.049 per step) every four to five steps, plus one settle raster when input stops; in between,
frames cost the GPU quad only. The overscanned rasters stay under the 150 ms viewport target.

`PROCESSCRAFT_CANVAS_TRACE=1` (stderr; run `processcraft.exe 2> trace.txt` for the release
build) logs per shown raster its size, the submit-to-display latency and the UI-thread texture
update time.

Limits: no GUI frame-time probe; the GPU upload itself happens in the eframe painter and is not
timed (the trace times only queuing the texture update); at ppp 1.5 an overscanned raster holds
about 52 MB in the pixmap plus the same in the egui image while it is handed over; a pan faster
than the margin per raster time shows canvas background at the leading edge until the raster
arrives; very large or high-DPI canvases get a smaller margin (none beyond the raster limits, as
before).
