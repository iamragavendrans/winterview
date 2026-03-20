<#
.SYNOPSIS
    Local build script for Invisiwind Enhanced.
    Produces Invisiwind.exe, utils.dll, utils32.dll and optionally the installer.

.PARAMETER Installer
    If set, also builds InvisiwindEnhancedInstaller.exe using InnoSetup.
    Requires InnoSetup 6 to be installed (https://jrsoftware.org/isdl.php).

.PARAMETER Clean
    Delete the target/ directory before building.

.EXAMPLE
    .\build-local.ps1
    .\build-local.ps1 -Installer
    .\build-local.ps1 -Installer -Clean
#>
param(
    [switch]$Installer,
    [switch]$Clean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = $PSScriptRoot
Set-Location $root

# ── Helpers ──────────────────────────────────────────────────────────────────

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Ok($msg)   { Write-Host "    OK: $msg" -ForegroundColor Green }
function Fail($msg) { Write-Host "    FAIL: $msg" -ForegroundColor Red; exit 1 }

function Require($cmd) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Fail "'$cmd' not found. See prerequisites in README-build.md."
    }
}

# ── Prerequisites check ───────────────────────────────────────────────────────

Step "Checking prerequisites"
Require "rustup"
Require "cargo"

$toolchain = (Get-Content "$root\rust-toolchain.toml" -Raw) -match 'channel\s*=\s*"(.+)"'
if ($Matches) { Ok "Toolchain: $($Matches[1])" } else { Ok "Toolchain: (default)" }

# Ensure nightly is installed
rustup show active-toolchain 2>&1 | Out-Null

# ── Optional clean ────────────────────────────────────────────────────────────

if ($Clean) {
    Step "Cleaning target/"
    if (Test-Path "$root\target") { Remove-Item "$root\target" -Recurse -Force }
    Ok "Cleaned"
}

# ── Build 32-bit payload DLL ─────────────────────────────────────────────────

Step "Adding x86 target (needed for 32-bit process injection)"
rustup target add i686-pc-windows-msvc
Ok "Target ready"

Step "Building utils32.dll (32-bit payload)"
cargo build -p payload --release --target i686-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { Fail "payload x86 build failed" }

$dll32src  = "$root\target\i686-pc-windows-msvc\release\utils.dll"
$dll32dest = "$root\target\i686-pc-windows-msvc\release\utils32.dll"
if (Test-Path $dll32dest) { Remove-Item $dll32dest -Force }
Rename-Item -Path $dll32src -NewName "utils32.dll"
Ok "utils32.dll built"

# ── Build 64-bit executable + DLL ────────────────────────────────────────────

Step "Building Invisiwind.exe + utils.dll (64-bit)"
cargo build --release
if ($LASTEXITCODE -ne 0) { Fail "x64 build failed" }
Ok "x64 build complete"

# ── Collect artifacts ─────────────────────────────────────────────────────────

Step "Collecting artifacts into .\dist\"
$dist = "$root\dist"
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null

Copy-Item "$root\target\release\Invisiwind.exe"                          $dist
Copy-Item "$root\target\release\utils.dll"                               $dist
Copy-Item "$root\target\i686-pc-windows-msvc\release\utils32.dll"        $dist
Copy-Item "$root\hide.ahk"                                               $dist

Ok "Artifacts:"
Get-ChildItem $dist | ForEach-Object { Write-Host "    $_" }

# ── Portable zip ─────────────────────────────────────────────────────────────

Step "Creating portable zip bundle"
$zipPath = "$root\InvisiwindEnhanced.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath }
$toZip = @("$dist\Invisiwind.exe","$dist\utils.dll","$dist\utils32.dll","$dist\hide.ahk")
Compress-Archive -Path $toZip -DestinationPath $zipPath
Ok "InvisiwindEnhanced.zip created"

# ── Installer (optional) ──────────────────────────────────────────────────────

if ($Installer) {
    Step "Building installer with InnoSetup"

    # Try common InnoSetup install paths
    $iscc = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        (Get-Command iscc -ErrorAction SilentlyContinue)?.Source
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1

    if (-not $iscc) {
        Fail "InnoSetup (ISCC.exe) not found.`nInstall from https://jrsoftware.org/isdl.php then re-run with -Installer."
    }

    & $iscc "$root\Misc\inno.iss"
    if ($LASTEXITCODE -ne 0) { Fail "InnoSetup build failed" }

    $installerOut = "$root\Misc\Output\InvisiwindEnhancedInstaller.exe"
    if (Test-Path $installerOut) {
        Ok "Installer: $installerOut"
    } else {
        Fail "Installer not found at expected path after ISCC run"
    }
}

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "  Portable zip : $root\InvisiwindEnhanced.zip"
Write-Host "  Run directly : $dist\Invisiwind.exe"
if ($Installer) {
    Write-Host "  Installer    : $root\Misc\Output\InvisiwindEnhancedInstaller.exe"
}
Write-Host ""
Write-Host "Note: If antivirus flags Invisiwind.exe or utils.dll, add an exclusion" -ForegroundColor Yellow
Write-Host "for the dist\ folder. DLL injection is a known false-positive trigger."  -ForegroundColor Yellow
