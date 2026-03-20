# Building & Shipping Winterview Enhanced

Two ways to get a distributable `.exe`. Pick whichever suits you.

---

## Option A — GitHub Actions (recommended, no local setup needed)

Every push to `main` automatically builds and publishes a pre-release.
Every version tag (`v2.1.0`, `v2.2.0` …) publishes a proper release.

### Steps

1. Fork or push this repo to your GitHub account.
2. Go to **Settings → Actions → General** and make sure Actions are enabled.
3. Push to `main` (or tag a version):
   ```
   git tag v2.1.0
   git push origin v2.1.0
   ```
4. Go to **Actions** tab → watch the `Build & Release` workflow run (~5 min).
5. Go to **Releases** → download either:
   - `WinterviewEnhanced.zip` — portable, extract and run
   - `WinterviewEnhancedInstaller.exe` — installer with Start Menu entry

The workflow handles everything: x86 + x64 DLLs, portable zip, InnoSetup installer, GitHub release upload.

---

## Option B — Local Windows build

### Prerequisites

| Tool | Download | Notes |
|---|---|---|
| Rust (MSVC toolchain) | https://rustup.rs | Pick "Customize" → MSVC on install |
| Visual Studio Build Tools | https://aka.ms/vs/17/release/vs_BuildTools.exe | Select "C++ build tools" workload |
| 7-Zip *(optional)* | https://7-zip.org | For portable zip output |
| InnoSetup *(optional)* | https://jrsoftware.org/isdl.php | For `.exe` installer output |

### Build

```bat
build.bat
```

That's it. The script handles all four steps:
1. Adds the `i686-pc-windows-msvc` target for the 32-bit DLL
2. Builds `utils32.dll` (injected into 32-bit apps like older Electron apps)
3. Builds `Winterview.exe` + `utils.dll` (64-bit)
4. Packages `WinterviewEnhanced.zip` and optionally the installer

### Output

```
WinterviewEnhanced.zip             ← send this to anyone
Misc/Output/WinterviewEnhancedInstaller.exe   ← or this
dist/
  Winterview.exe
  utils.dll
  utils32.dll
```

---

## What to ship

### Portable (ZIP)
Extract anywhere. No install needed. Run `Winterview.exe`.
Suitable for users who don't want an installer or work in restricted environments.

### Installer (.exe)
Runs InnoSetup wizard. Installs to `Program Files`, adds Start Menu entry,
optional "Start on Windows boot" checkbox, optional Desktop shortcut.
Suitable for everyday users.

---

## Antivirus false positives

Winterview uses DLL injection to call `SetWindowDisplayAffinity` from within
target processes. This is the only way the Windows API permits that call.
As a result, most antivirus products will flag the build as a trojan or
generic malware. This is a false positive.

To reduce detections:
- Sign the binaries with a code-signing certificate (EV certificates have
  the best reputation with Windows Defender SmartScreen).
- Submit false positives to your antivirus vendor.
- For internal/team use, add a folder exclusion in Windows Security settings.

---

## System requirements

- Windows 10 version 2004 (build 19041) or later
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` was added in that build
- Earlier Windows versions show a black rectangle instead of hiding the window
