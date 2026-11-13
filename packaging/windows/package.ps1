<#
.SYNOPSIS
  Build and package ProcessCraft for Windows x64.

.DESCRIPTION
  Produces, in $env:DIST (default: dist\release):
    processcraft-<version>-windows-x64.msi            per-user installer (WiX v5, no admin)
    processcraft-<version>-windows-x64-portable.zip   processcraft.exe, processcraft-cli.exe, licences

  Binaries link the C runtime statically (+crt-static): no VC++ redistributable needed.
  Signing is optional: set PROCESSCRAFT_SIGN to a script that takes the files to sign
  (e.g. a SignPath or signtool wrapper); without it the packages are unsigned.

  Needs: Rust (MSVC), the Windows SDK, and WiX v5:
    dotnet tool install --global wix --version 5.0.2
    wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2

.EXAMPLE
  powershell -File packaging/windows/package.ps1
  powershell -File packaging/windows/package.ps1 -SkipBuild
#>
param([switch] $SkipBuild)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}
function Invoke-Sign([string[]] $Files) {
  if ($env:PROCESSCRAFT_SIGN) { Invoke-Native "sign $($Files -join ', ')" { & $env:PROCESSCRAFT_SIGN @Files } }
  else { Write-Warning "PROCESSCRAFT_SIGN not set: $($Files -join ', ') left unsigned" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $null
$inPkg = $false
foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
  if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
  if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
$MsiVersion = ($Version -split '-')[0]

$Target = 'x86_64-pc-windows-msvc'
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not $SkipBuild) {
  # Scoped to the target so host build scripts and proc-macros are unaffected.
  $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p processcraft -p processcraft-cli --target $Target }
}
$Bin = Join-Path $TargetDir "$Target\release"

# PE header checks: x64 machine; the app is GUI subsystem (2, no console window), the CLI console (3).
foreach ($check in @(@('processcraft.exe', 2), @('processcraft-cli.exe', 3))) {
  $bytes = [IO.File]::ReadAllBytes((Join-Path $Bin $check[0]))
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  $machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
  $subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C)
  if ($machine -ne 0x8664) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $machine), expected x64" }
  if ($subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $subsystem, expected $($check[1])" }
  Write-Output "ok $($check[0]): x64, PE subsystem $subsystem"
}

$Stage = Join-Path $TargetDir 'windows-package\x64'
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
foreach ($exe in 'processcraft.exe', 'processcraft-cli.exe') { Copy-Item (Join-Path $Bin $exe) $Stage }
Invoke-Sign @((Join-Path $Stage 'processcraft.exe'), (Join-Path $Stage 'processcraft-cli.exe'))

# Wizard art from the app icon (24-bit BMPs, as WixUI requires): a 493x58 banner across the inner
# pages (their title is drawn over its left side, so that stays white) and a 493x312 backdrop for
# the Welcome and Finish pages (their text sits on the right; the left 164 px are ours).
Add-Type -AssemblyName System.Drawing
$Banner = Join-Path $Stage 'installer-banner.bmp'
$Dialog = Join-Path $Stage 'installer-dialog.bmp'
$icon = [System.Drawing.Image]::FromFile((Join-Path $Root 'assets\app-icon\processcraft-256.png'))
try {
  foreach ($spec in @(@($Banner, 58), @($Dialog, 312))) {
    $bmp = New-Object System.Drawing.Bitmap 493, $spec[1], ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
      $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
      $g.Clear([System.Drawing.Color]::White)
      if ($spec[1] -eq 58) {
        $g.DrawImage($icon, 493 - 46 - 8, 6, 46, 46)
      } else {
        # ProcessCraft paper panel with an amber edge (assets/app-icon/README.md palette).
        $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#efe9dc'))), 0, 0, 164, 312)
        $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#f2a516'))), 160, 0, 4, 312)
        $g.DrawImage($icon, 18, 64, 124, 124)
      }
    } finally { $g.Dispose() }
    $bmp.Save($spec[0], [System.Drawing.Imaging.ImageFormat]::Bmp)
    $bmp.Dispose()
  }
} finally { $icon.Dispose() }

$Msi = Join-Path $Dist "processcraft-$Version-windows-x64.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'processcraft.wxs') -arch x64 `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -culture en-US -loc (Join-Path $PSScriptRoot 'processcraft.en-us.wxl') `
    -d "BannerBmp=$Banner" -d "DialogBmp=$Dialog" `
    -d "Version=$MsiVersion" -d "DisplayVersion=$Version" -d "BinDir=$Stage" -d "RootDir=$Root" `
    -d "IconPath=$(Join-Path $Root 'assets\app-icon\processcraft.ico')" `
    -o $Msi
}
Invoke-Native 'MSI validation' {
  wix msi validate $Msi -intermediateFolder (Join-Path $Stage 'msi-validation')
}
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
Invoke-Sign @($Msi)

# Portable zip: the same binaries plus licences.
$Portable = Join-Path $TargetDir "windows-package\processcraft-$Version-windows-x64-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path (Join-Path $Portable 'licenses') | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
Copy-Item (Join-Path $Root 'LICENSE'), (Join-Path $Root 'README.md') $Portable
Copy-Item (Join-Path $Root 'assets\fonts\*.txt'), (Join-Path $Root 'assets\fonts\ATTRIBUTION.md') (Join-Path $Portable 'licenses')
$Zip = Join-Path $Dist "processcraft-$Version-windows-x64-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

Invoke-Native 'processcraft-cli --version' { & (Join-Path $Stage 'processcraft-cli.exe') --version | Out-Host }
Get-Item $Msi, $Zip | Format-Table Name, Length
