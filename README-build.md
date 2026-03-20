# Building Winterview Enhanced

## Prerequisites

| Tool | Version | Where to get |
|---|---|---|
| Rust (nightly) | as per `rust-toolchain.toml` | https://rustup.rs |
| MSVC build tools | VS 2019+ or Build Tools | https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022 |
| InnoSetup 6 | 6.x | https://jrsoftware.org/isdl.php (only needed for `-Installer`) |

> **Windows only.** The project uses Win32 APIs exclusively — cross-compilation is not supported.

---

## Option A — Local build (PowerShell)

```powershell
# Basic build (produces dist\ folder + WinterviewEnhanced.zip)
.\build-local.ps1

# Build + installer .exe
.\build-local.ps1 -Installer

# Full clean build + installer
.\build-local.ps1 -Installer -Clean
```

Output locations:

| File | What it is |
|---|---|
| `dist\Winterview.exe` | Main application — run this directly for a portable install |
| `dist\utils.dll` | 64-bit payload DLL (auto-injected, do not run manually) |
| `dist\utils32.dll` | 32-bit payload DLL (for hiding 32-bit processes) |
| `dist\hide.ahk` | Optional AutoHotkey script |
| `WinterviewEnhanced.zip` | Portable bundle of the above four files |
| `Misc\Output\WinterviewEnhancedInstaller.exe` | Full installer (only when `-Installer` flag used) |

---

## Option B — GitHub Actions (no local toolchain needed)

1. Fork or push this repo to GitHub.
2. GitHub Actions triggers automatically on every push.
3. Go to **Actions → Build & Release → latest run → Artifacts** to download the `installer` artifact (contains both the zip and the `.exe` installer).

To publish a proper release, push a version tag:

```bash
git tag v2.1.0
git push origin v2.1.0
```

This triggers the tagged release job and creates a GitHub Release with the installer attached.

---

## Antivirus false positives

Winterview uses DLL injection (`CreateRemoteThread` + `LoadLibrary`) which most antivirus engines flag as malware behaviour. This is a well-known false positive for legitimate screen-capture tools.

**If your build gets quarantined:**
1. Add an exclusion for the `dist\` folder in Windows Defender.
2. Or build in a folder that is already excluded (e.g. `C:\Users\<you>\projects\`).
3. Submit the binary to your AV vendor as a false positive if needed.

The payload (`utils.dll`) is the most commonly flagged file — it exports only two functions:
- `SetWindowVisibility` → calls `SetWindowDisplayAffinity`
- `HideFromTaskbar` → toggles `WS_EX_TOOLWINDOW` / `WS_EX_APPWINDOW`

Both are standard Win32 calls with no network access, no persistence, and no privilege escalation.
