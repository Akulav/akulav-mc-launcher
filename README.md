<div align="center">

<img src="assets/icon.png" alt="Akulav Launcher" width="128" height="128">

# Akulav Launcher

**A fast, lightweight Minecraft modpack launcher, made by Akulav.**

No spyware. No telemetry. No bloat. One native executable.

[![Latest release](https://img.shields.io/github/v/release/Akulav/akulav-mc-launcher?style=for-the-badge&label=release&color=e5383b)](https://github.com/Akulav/akulav-mc-launcher/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Akulav/akulav-mc-launcher/total?style=for-the-badge&color=f4b740)](https://github.com/Akulav/akulav-mc-launcher/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/Akulav/akulav-mc-launcher/build.yml?branch=main&style=for-the-badge&label=build)](https://github.com/Akulav/akulav-mc-launcher/actions/workflows/build.yml)

![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6?style=flat-square&logo=windows&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-x64-FCC624?style=flat-square&logo=linux&logoColor=black)
![Rust](https://img.shields.io/badge/made%20with-Rust-B7410E?style=flat-square&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-source--available-5c6270?style=flat-square)

### [⬇ Download the latest release](https://github.com/Akulav/akulav-mc-launcher/releases/latest)

</div>

---

## ⬇️ Download

| Platform | File | Notes |
| :-- | :-- | :-- |
| 🪟 **Windows 10 / 11** (x64) | [`AkulavLauncher.exe`](https://github.com/Akulav/akulav-mc-launcher/releases/latest/download/AkulavLauncher.exe) | Run it, and it installs itself. No admin rights needed. |
| 🐧 **Linux** (x64) | [`AkulavLauncher-linux-x86_64.tar.gz`](https://github.com/Akulav/akulav-mc-launcher/releases/latest/download/AkulavLauncher-linux-x86_64.tar.gz) | glibc 2.28+, OpenGL, X11 or Wayland |

Every release also ships `SHA256SUMS.txt` so you can verify your download.

## ✨ Features

<table>
<tr>
<td width="50%" valign="top">

### 🎮 Play
- One click from pack to game. The Play button shows live progress (percent, MB/s, time left) and turns into **Cancel** on hover.
- **Mod loaders install automatically**: NeoForge, Forge, Fabric and Quilt.
- **Java is handled for you**: Mojang's own runtime for each Minecraft version (or bring your own).
- Your real skin as a 3D head (Mojang / Ely.by), or one generated from your name.

</td>
<td width="50%" valign="top">

### 🧰 Manage
- Per-pack settings: memory slider with a recommendation, window size, Java arguments, what happens after launch.
- **Repair** re-downloads a pack and re-verifies every file.
- Pack updates replace `mods` and `config` but **keep your saves**, screenshots and options.
- Play time and last-played tracking for every pack.

</td>
</tr>
<tr>
<td width="50%" valign="top">

### ⚡ Fast & light
- A single native binary written in Rust. No .NET, no Electron.
- Parallel downloads with SHA-1 checks and retries.
- Libraries and assets are shared between packs, so nothing downloads twice.
- Works offline: installed packs still launch when the server is unreachable.

</td>
<td width="50%" valign="top">

### 🔄 Always up to date
- Installs itself on first run (Desktop + Start-menu shortcuts, or an app-menu entry on Linux).
- **One-click in-place self-update**, verified with SHA-256.
- Live **Console** with game, installer and launcher output, filters, search and a crash banner.
- Toast notifications instead of message boxes.

</td>
</tr>
</table>

## 🚀 Getting started

### Windows
1. Download **`AkulavLauncher.exe`** and run it from anywhere.
2. It installs itself to `%LOCALAPPDATA%\Programs\AkulavLauncher`, adds Desktop and Start-menu shortcuts and an entry in *Settings → Apps* (uninstall from there), then restarts from the installed copy.

> 💡 Want it portable? Put an empty `portable.txt` next to the exe.

### Linux
```bash
tar -xzf AkulavLauncher-linux-x86_64.tar.gz
./AkulavLauncher-linux-x86_64
```
It installs itself to `~/.local/share/AkulavLauncher/bin/`, adds an app-menu entry with icon, a Desktop shortcut and an
`akulav-launcher` terminal command. Uninstall from *Settings → Storage* or with `akulav-launcher --uninstall`.

Needs glibc 2.28+ (Ubuntu 20.04, Debian 10, Fedora 29, Mint 20 or newer). Mojang ships no ARM Linux Java, so ARM needs a custom Java path.

## 📁 Where your data lives

Everything is in `%APPDATA%\AkulavLauncher` (Linux: `~/.local/share/AkulavLauncher`):

| Path | Contents |
| :-- | :-- |
| `settings.json` | launcher and per-pack settings |
| `installed.json` | installed pack versions |
| `shared/` | versions, libraries, assets, Java runtimes |
| `instances/<pack>/` | each pack's game folder (mods, config, saves…) |
| `cache/` | temporary downloads |

Set `AKULAV_DATA_DIR` to use a different data folder.

## 🖥️ Command line

For troubleshooting (output goes to the terminal):

```bash
AkulavLauncher.exe --cli list
AkulavLauncher.exe --cli launch "GoldenEra - Ultimate" [--user NAME] [--ram 6144] [--repair] [--prepare-only]
```

<details>
<summary><b>🛠️ For maintainers: modpacks, updates and releases</b></summary>

### Remote files

- Modpack list: `https://akulav.com/shared/aa7e873115b2344cc291045b` (can be overridden in Settings)
- Launcher version: `https://akulav.com/shared/5ce6926c3dda5f322df1ac59`

`modpacks.json` entries can optionally include a `"Description"` shown on the pack page:

```json
[
  {
    "Name": "GoldenEra - Ultimate",
    "Version": "7.2.0",
    "API": "neoforge-21.1.248",
    "URL": "https://akulav.com/shared/77a416c8106b356988b3f49b",
    "Description": "Optional text shown under the pack name."
  }
]
```

Supported `API` values: a vanilla id (`1.21.1`), `neoforge-<version>`, `<mc>-forge-<version>`,
`fabric-loader-<loader>-<mc>`, `quilt-loader-<loader>-<mc>`.

### Publishing a release

Releases are fully automated by [`.github/workflows/build.yml`](.github/workflows/build.yml):

1. Bump `version` in `Cargo.toml`.
2. Push to `main`.
3. If no release with that version exists, CI tests and builds Windows and Linux, tags the commit and publishes a GitHub
   release with `AkulavLauncher.exe`, `AkulavLauncher-linux-x86_64`, the `.tar.gz`, `SHA256SUMS.txt` and a ready-made **`version.txt`**.
4. Upload that `version.txt` to the launcher-version URL above. Installed launchers pick up the update within 2 hours.

### `version.txt` format

```
10.2.0
https://github.com/Akulav/akulav-mc-launcher/releases/download/10.2.0/AkulavLauncher.exe
<optional SHA-256 of the exe>
linux https://github.com/Akulav/akulav-mc-launcher/releases/download/10.2.0/AkulavLauncher-linux-x86_64 <optional SHA-256>
```

Line 2 is the **Windows launcher exe itself**, not an installer (it's also what 9.x launchers download). The `linux` line points
at the bare Linux binary. A Linux launcher only updates when that line exists; old launchers ignore it. When line 1 is higher
than the running version, the title bar shows *Update available*. One click downloads and verifies the new binary, and
*Restart & update* swaps it in place.

**Upgrading 9.x users:** the old C# launcher downloads line 2 as its "installer" and runs it; the new launcher then installs
itself, replaces the old desktop shortcut and removes `C:\AkulavLauncher`.

### Building from source

```bash
cargo build --release
```

The launcher is `target/release/AkulavLauncher(.exe)`. For a Linux binary that runs on older distros:
`pip install ziglang && cargo install cargo-zigbuild`, then `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28`.
Debug builds never install themselves. `cargo test` runs the unit tests.

</details>

## 🙏 Credits

Icons by [Phosphor Icons](https://phosphoricons.com) (MIT). Inter font by Rasmus Andersson (SIL OFL 1.1, see `assets/fonts`).
DejaVu Sans Mono (Bitstream Vera license). Launcher icon by [Smashicons](https://www.flaticon.com/authors/smashicons).

---

<div align="center">

**Made with ❤️ by [Akulav](https://github.com/Akulav)**

<sub>© 2026 Akulav. Source-available under the <a href="LICENSE">Akulav Source-Available License</a>. Not affiliated with Mojang or Microsoft.</sub>

</div>
