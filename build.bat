@echo off
setlocal enabledelayedexpansion
title Winterview Enhanced — Build

echo ============================================================
echo  Winterview Enhanced — local build script
echo ============================================================
echo.

:: ── Prerequisite check: cargo ───────────────────────────────────────────────
where cargo >nul 2>&1
if errorlevel 1 (
    echo ERROR: Rust/cargo not found.
    echo.
    echo Install Rust from https://rustup.rs/ then re-run this script.
    echo Make sure to select the MSVC toolchain during setup.
    pause
    exit /b 1
)

for /f "tokens=*" %%v in ('cargo --version') do echo Found: %%v
echo.

:: ── Add x86 target for 32-bit DLL ───────────────────────────────────────────
echo [1/4] Adding i686-pc-windows-msvc target...
rustup target add i686-pc-windows-msvc
if errorlevel 1 (
    echo ERROR: Could not add x86 target.
    pause
    exit /b 1
)

:: ── Build 32-bit payload DLL ─────────────────────────────────────────────────
echo.
echo [2/4] Building 32-bit payload DLL (utils32.dll)...
cargo build -p payload --release --target i686-pc-windows-msvc
if errorlevel 1 (
    echo ERROR: 32-bit payload build failed.
    pause
    exit /b 1
)
copy /Y "target\i686-pc-windows-msvc\release\utils.dll" "target\i686-pc-windows-msvc\release\utils32.dll"
echo     OK: utils32.dll

:: ── Build 64-bit main executable + 64-bit DLL ──────────────────────────────
echo.
echo [3/4] Building 64-bit Winterview.exe + utils.dll...
cargo build --release
if errorlevel 1 (
    echo ERROR: 64-bit build failed.
    pause
    exit /b 1
)
echo     OK: Winterview.exe
echo     OK: utils.dll

:: ── Assemble portable zip ───────────────────────────────────────────────────
echo.
echo [4/4] Creating portable zip bundle...

if not exist "dist" mkdir dist
copy /Y "target\release\Winterview.exe"                           "dist\Winterview.exe"  >nul
copy /Y "target\release\utils.dll"                                "dist\utils.dll"       >nul
copy /Y "target\i686-pc-windows-msvc\release\utils32.dll"         "dist\utils32.dll"     >nul

where 7z >nul 2>&1
if errorlevel 1 (
    echo WARN: 7-Zip not found — skipping zip. Files are in the dist\ folder.
) else (
    7z a -tzip WinterviewEnhanced.zip ".\hide.ahk" ".\dist\*.dll" ".\dist\*.exe" >nul
    echo     OK: WinterviewEnhanced.zip
)

:: ── Optional: build installer with InnoSetup ───────────────────────────────
where iscc >nul 2>&1
if errorlevel 1 (
    echo.
    echo NOTE: InnoSetup (iscc) not found — skipping installer.
    echo       Download from https://jrsoftware.org/isdl.php to build the .exe installer.
) else (
    echo.
    echo Building WinterviewEnhancedInstaller.exe...
    iscc ".\Misc\inno.iss"
    if errorlevel 1 (
        echo WARN: Installer build failed, but portable zip is ready.
    ) else (
        echo     OK: Misc\Output\WinterviewEnhancedInstaller.exe
    )
)

:: ── Summary ──────────────────────────────────────────────────────────────────
echo.
echo ============================================================
echo  Build complete. Output files:
echo.
echo  Portable:   WinterviewEnhanced.zip
echo              (extract anywhere, run Winterview.exe)
echo.
if exist "Misc\Output\WinterviewEnhancedInstaller.exe" (
    echo  Installer:  Misc\Output\WinterviewEnhancedInstaller.exe
    echo              (run to install with Start Menu entry)
    echo.
)
echo  Raw files:  dist\Winterview.exe
echo              dist\utils.dll
echo              dist\utils32.dll
echo ============================================================
echo.
pause
