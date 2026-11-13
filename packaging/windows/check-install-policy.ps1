<#
  Runs the installer policy cases with real msiexec calls and fails on the first broken one.
  Builds its own MSIs from the binaries package.ps1 staged (target\windows-package\x64), so run
  package.ps1 first. Needs a machine with no ProcessCraft installed; uninstalls what it installs.

    powershell -File packaging/windows/check-install-policy.ps1             # per-user cases
    (elevated) powershell -File packaging/windows/check-install-policy.ps1  # adds per-machine cases

  Logs go to target\install-policy\*.log.
#>
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Stage = Join-Path $TargetDir 'windows-package\x64'
$Work = Join-Path $TargetDir 'install-policy'
$UserDir = Join-Path $env:LOCALAPPDATA 'Programs\ProcessCraft'
$MachineDir = Join-Path $env:ProgramW6432 'ProcessCraft'
$Elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not (Test-Path (Join-Path $Stage 'processcraft.exe'))) { throw "no staged binaries in ${Stage}: run package.ps1 first" }
New-Item -ItemType Directory -Force -Path $Work | Out-Null

# Registered copies, per-user and per-machine, from the Add/Remove Programs entries MSI writes.
function Products {
  @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
    Get-ItemProperty | Where-Object { $_.DisplayName -eq 'ProcessCraft' } | ForEach-Object { $_.PSChildName })
}
function Version([string] $code) {
  (Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$code", "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$code" -ErrorAction SilentlyContinue).DisplayVersion
}
function Assert([bool] $ok, [string] $what) { if (-not $ok) { throw "FAIL: $what" }; Write-Output "ok   $what" }

# Each version is a separate build, so each gets its own ProductCode (a real upgrade).
function Build([string] $version) {
  $msi = Join-Path $Work "processcraft-$version.msi"
  wix build (Join-Path $PSScriptRoot 'processcraft.wxs') -arch x64 `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -culture en-US -loc (Join-Path $PSScriptRoot 'processcraft.en-us.wxl') `
    -d "BannerBmp=$Stage\installer-banner.bmp" -d "DialogBmp=$Stage\installer-dialog.bmp" `
    -d "Version=$version" -d "DisplayVersion=$version" -d "BinDir=$Stage" -d "RootDir=$Root" `
    -d "IconPath=$Root\assets\app-icon\processcraft.ico" -o $msi
  if ($LASTEXITCODE -ne 0) { throw "wix build $version failed" }
  $msi
}

function Msi([string] $name, [string[]] $msiArgs) {
  $log = Join-Path $Work "$name.log"
  $p = Start-Process msiexec -ArgumentList (@($msiArgs) + @('/qn', '/l*v', "`"$log`"")) -Wait -PassThru
  $p.ExitCode
}

function Log-Has([string] $name, [string] $text) { (Get-Content -Raw -Encoding Unicode (Join-Path $Work "$name.log")).Contains($text) }

function Uninstall-All { foreach ($code in Products) { $null = Msi "cleanup-$code" @('/x', $code) } }

function Assert-Clean([string] $case) {
  Assert ((Products).Count -eq 0) "$case`: no registered product"
  Assert (-not (Test-Path HKCU:\Software\ProcessCraft)) "$case`: HKCU\Software\ProcessCraft removed"
  Assert (-not (Test-Path 'HKCU:\Software\Classes\.processcraft')) "$case`: .processcraft association removed"
  Assert (-not (Test-Path (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ProcessCraft.lnk'))) "$case`: Start menu shortcut removed"
}

if ((Products).Count -ne 0) { throw 'ProcessCraft is already installed here; uninstall it before running the checks' }
$Low = Build '0.0.9'
$High = Build '0.1.0'

try {
  # Upgrade from a lower version keeps one product and replaces the binaries.
  Assert ((Msi 'low-install' @('/i', "`"$Low`"")) -eq 0) 'per-user install of 0.0.9'
  Assert ((Msi 'upgrade' @('/i', "`"$High`"")) -eq 0) 'upgrade 0.0.9 -> 0.1.0'
  $p = @(Products)
  Assert ($p.Count -eq 1 -and (Version $p[0]) -eq '0.1.0') 'one product, version 0.1.0'
  Assert (Test-Path (Join-Path $UserDir 'processcraft.exe')) 'binaries in the per-user folder'

  # Downgrade is refused and leaves the newer copy in place.
  Assert ((Msi 'downgrade' @('/i', "`"$Low`"")) -ne 0) 'downgrade 0.1.0 -> 0.0.9 refused'
  $p = @(Products)
  Assert ($p.Count -eq 1 -and (Version $p[0]) -eq '0.1.0') 'still 0.1.0 after the refused downgrade'

  # A per-machine install over a per-user copy is refused before anything is written.
  Assert ((Msi 'cross-context' @('/i', "`"$Low`"", 'ALLUSERS=1')) -eq 1603) 'ALLUSERS=1 over a per-user copy refused (1603)'
  Assert (Log-Has 'cross-context' 'already installed for your account') 'refusal names the per-user copy'
  Assert (-not (Test-Path HKLM:\Software\ProcessCraft)) 'nothing written to HKLM'

  Assert ((Msi 'uninstall' @('/x', "`"$High`"")) -eq 0) 'uninstall'
  Assert (-not (Test-Path $UserDir)) 'install folder removed'
  Assert-Clean 'uninstall'

  # A custom folder survives an upgrade that names no folder.
  $Custom = Join-Path $env:LOCALAPPDATA 'PcPolicyCheck\'
  Assert ((Msi 'custom-install' @('/i', "`"$Low`"", "INSTALLFOLDER=`"$Custom`"")) -eq 0) 'install 0.0.9 into a custom folder'
  Assert ((Msi 'custom-upgrade' @('/i', "`"$High`"")) -eq 0) 'upgrade with no folder given'
  Assert ((Test-Path (Join-Path $Custom 'processcraft.exe')) -and -not (Test-Path $UserDir)) 'upgrade stayed in the custom folder'
  Uninstall-All
  Assert-Clean 'custom folder'

  # Failure mid-install rolls back: a file where the licenses folder must go breaks CreateFolders.
  $Blocked = Join-Path $env:LOCALAPPDATA 'PcPolicyRollback'
  New-Item -ItemType Directory -Force -Path $Blocked | Out-Null
  Set-Content -Path (Join-Path $Blocked 'licenses') -Value 'blocker'
  $code = Msi 'rollback' @('/i', "`"$High`"", "INSTALLFOLDER=`"$Blocked\`"")
  Assert ($code -ne 0) "install that fails mid-way returns an error ($code)"
  Assert (-not (Test-Path (Join-Path $Blocked 'processcraft.exe'))) 'rollback removed the copied binaries'
  Assert-Clean 'rollback'
  Remove-Item -Recurse -Force $Blocked

  if ($Elevated) {
    Assert ((Msi 'machine-install' @('/i', "`"$Low`"", 'ALLUSERS=1')) -eq 0) 'per-machine install of 0.0.9'
    Assert (Test-Path (Join-Path $MachineDir 'processcraft.exe')) 'binaries under Program Files'
    Assert ((Get-ItemProperty HKLM:\Software\ProcessCraft).InstallDir -like "$MachineDir*") 'HKLM InstallDir recorded'
    Assert ((Msi 'machine-upgrade' @('/i', "`"$High`"", 'ALLUSERS=1')) -eq 0) 'per-machine upgrade 0.0.9 -> 0.1.0'
    Assert ((Products).Count -eq 1) 'one product after the per-machine upgrade'
    Assert ((Msi 'user-over-machine' @('/i', "`"$Low`"")) -ne 0) 'per-user install over a per-machine copy refused'
    Assert (Log-Has 'user-over-machine' 'already installed for everyone') 'refusal names the per-machine copy'
    Assert ((Msi 'machine-uninstall' @('/x', "`"$High`"")) -eq 0) 'per-machine uninstall'
    Assert (-not (Test-Path $MachineDir) -and -not (Test-Path HKLM:\Software\ProcessCraft)) 'Program Files folder and HKLM key removed'
  } else {
    Write-Output 'skip per-machine cases (not elevated)'
  }
} finally {
  Uninstall-All
}
Write-Output 'install policy: all checks passed'
