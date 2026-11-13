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
`v*` tags), uploads the MSI and ZIP as artifacts, and drafts a GitHub release for tags. It has not
run yet; the repository is public, which SignPath's open-source programme requires.

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
| Sizes (0.1.0) | MSI 55.5 MB, portable ZIP 64.5 MB |

The same-package case is not a context test: re-running the installed MSI with `ALLUSERS=1` enters
maintenance mode (exit 0, nothing changed), so the refusal was checked with a rebuilt package.

A first build with `Scope="perUser"` failed with error 1303: without `ALLUSERS=2` +
`MSIINSTALLPERUSER=1`, Windows Installer does not redirect Program Files to the user profile.

## Not established

- The packages are **unsigned**; SmartScreen warns about an unknown publisher. The intended free
  route is the SignPath Foundation open-source programme (application and CI wiring pending).
- Not run: a per-machine install (needs elevation), the interactive wizard past its first page
  (WixUI buttons are not reachable through UI Automation here; the welcome/finish art and version
  strings were not seen rendered), cancellation and rollback mid-install, an upgrade from a lower
  version number, the CI workflow, x86/arm64 builds, machines other than the development PC.
- No automated policy checker like FigCraft's `check-install-policy.ps1`; the cases above were run
  by hand.
