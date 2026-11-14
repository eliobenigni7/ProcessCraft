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
