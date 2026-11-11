<div align="center">

<img src="assets/icon.png" alt="Akulav Launcher" width="128" height="128">

# Akulav Launcher

**A fast, lightweight Minecraft modpack launcher, made by Akulav.**

No spyware. No telemetry. No bloat. One file per system that installs and updates itself.

[![Latest release](https://img.shields.io/github/v/release/Akulav/akulav-mc-launcher?style=for-the-badge&label=release&color=e5383b)](https://github.com/Akulav/akulav-mc-launcher/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Akulav/akulav-mc-launcher/total?style=for-the-badge&color=f4b740)](https://github.com/Akulav/akulav-mc-launcher/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/Akulav/akulav-mc-launcher/build.yml?branch=main&style=for-the-badge&label=build)](https://github.com/Akulav/akulav-mc-launcher/actions/workflows/build.yml)

![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6?style=flat-square&logo=windows&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-x64-FCC624?style=flat-square&logo=linux&logoColor=black)
![macOS](https://img.shields.io/badge/macOS-Apple%20Silicon-000000?style=flat-square&logo=apple&logoColor=white)
![Rust](https://img.shields.io/badge/made%20with-Rust-B7410E?style=flat-square&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-source--available-5c6270?style=flat-square)

### [⬇ Download the latest release](https://github.com/Akulav/akulav-mc-launcher/releases/latest)

</div>

---

## ⬇️ Download

One file per system. The same file installs the launcher the first time and updates it later.

| Platform | File | Notes |
| :-- | :-- | :-- |
| 🪟 **Windows 10 / 11** (x64) | [`AkulavLauncher-setup.exe`](https://github.com/Akulav/akulav-mc-launcher/releases/latest/download/AkulavLauncher-setup.exe) | Asks where to install. No admin rights needed. |
| 🐧 **Linux** (x64) | [`AkulavLauncher-linux-x64`](https://github.com/Akulav/akulav-mc-launcher/releases/latest/download/AkulavLauncher-linux-x64) | glibc 2.28+, OpenGL, X11 or Wayland |
| 🍎 **macOS** (Apple Silicon) | [`AkulavLauncher-macos-arm64`](https://github.com/Akulav/akulav-mc-launcher/releases/latest/download/AkulavLauncher-macos-arm64) | macOS 12+ |

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
- **You choose the install folder** once; the launcher and all its downloads live there.
- **One-click updates** from GitHub, verified with SHA-256, into the same folder.
- Live **Console** with game, installer and launcher output, filters, search and a crash banner.
- Toast notifications instead of message boxes.

</td>
</tr>
</table>

## 🚀 Getting started

### Windows
1. Download **`AkulavLauncher-setup.exe`** and run it.
2. Pick the install folder (default `%LOCALAPPDATA%\Programs\AkulavLauncher`, any folder you can write to works,
   e.g. `D:\Games\AkulavLauncher`). It adds Start-menu and (optional) Desktop shortcuts and an entry in
   *Settings → Apps*.
3. Updates install into the same folder without asking: in-app with one click, or by running a newer setup.

Uninstall from *Settings → Apps* or *Settings → Storage* in the launcher. The uninstaller asks whether to keep your
modpacks and worlds.

### Linux
```bash
chmod +x AkulavLauncher-linux-x64
./AkulavLauncher-linux-x64
```
It asks for an install folder (default `~/.local/share/AkulavLauncher`), copies itself there and adds an app-menu
entry with icon, a Desktop shortcut and an `akulav-launcher` terminal command. *Just run it* skips installing.
Running a newer download later offers to update the installed copy in its folder.
Uninstall from *Settings → Storage* or with `akulav-launcher --uninstall`.

Needs glibc 2.28+ (Ubuntu 20.04, Debian 10, Fedora 29, Mint 20 or newer). Mojang ships no ARM Linux Java, so ARM needs a custom Java path.

### macOS (Apple Silicon)
```bash
chmod +x AkulavLauncher-macos-arm64
xattr -d com.apple.quarantine AkulavLauncher-macos-arm64
./AkulavLauncher-macos-arm64
```
It asks for an install folder (default `~/Applications/Akulav Launcher`) and creates `Akulav Launcher.app` there.

> 💡 Want it portable? Put an empty `portable.txt` next to the program (or start it with `--portable`): it then keeps
> its data next to itself and never offers to install.

## 📁 Where your data lives

**In the install folder you chose**, next to the program. Install on `D:\Games\AkulavLauncher` and every modpack, world,
Java runtime and download goes to `D:\Games\AkulavLauncher` too; nothing goes to `%APPDATA%`, `~/.local/share` or other
system folders. The folder is remembered (Windows: by the setup; Linux/macOS: in `~/.config/akulav-launcher/install-dir`
or `~/Library/Application Support/akulav-launcher/install-dir`), so updates keep everything in place. Data from
launchers before 11.0 (`%APPDATA%\AkulavLauncher`) is moved in on the first start, also when the install folder is on
another drive.

| Path | Contents |
| :-- | :-- |
| `settings.json` | launcher and per-pack settings |
| `installed.json` | installed pack versions |
| `shared/` | versions, libraries, assets, Java runtimes |
| `instances/<pack>/` | each pack's game folder (mods, config, saves…) |
| `cache/` | temporary downloads (including a downloaded launcher update) and Java's temp folder |
| `window.ron` | window size and position |
| `.akulav-install` | marks the install folder (holds the installed version) |

Set `AKULAV_DATA_DIR` to use a different data folder.

## 🖥️ Command line

For troubleshooting (output goes to the terminal):

```bash
AkulavLauncher.exe --cli list
AkulavLauncher.exe --cli launch "GoldenEra - Ultimate" [--user NAME] [--ram 6144] [--repair] [--prepare-only]
```

| Flag | Meaning |
| :-- | :-- |
| `--version` | Print the version and exit |
| `--here` | Run without offering to install (Linux/macOS) |
| `--portable` | Keep data next to the program, never install |
| `--uninstall` | Remove the installed launcher (Windows: starts the uninstaller) |

<details>
<summary><b>🛠️ For maintainers: modpacks, updates and releases</b></summary>

### Remote files

- Modpack list: `https://akulav.com/shared/aa7e873115b2344cc291045b` (can be overridden in Settings)
- Launcher updates (11.0+): the latest GitHub release of this repository, verified against its `SHA256SUMS.txt`
- Launcher version for launchers before 11.0: `https://akulav.com/shared/5ce6926c3dda5f322df1ac59`

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

1. Bump `version` in `Cargo.toml` (and add a section to [`CHANGELOG.md`](CHANGELOG.md)).
2. Push to `main`.
3. If no release with that version exists, CI tests and builds all three systems, tags the commit and publishes a GitHub
   release with exactly three programs, `AkulavLauncher-setup.exe`, `AkulavLauncher-linux-x64` and
   `AkulavLauncher-macos-arm64`, plus `SHA256SUMS.txt` and `version.txt`.
4. Launchers 11.0+ find the release on GitHub by themselves (checked at start and every 2 hours, or with
   *Check for updates*). Only while launchers older than 11.0 are still around: upload that `version.txt` to the
   old launcher-version URL above.

### How updates are applied

When the latest release is newer than the running launcher, the title bar shows *Update available*. One click downloads
this system's file and checks it against `SHA256SUMS.txt`; *Restart & update* then

- **Windows:** runs `AkulavLauncher-setup.exe /VERYSILENT /UPDATE` with the current install folder. The setup closes the
  launcher, installs into the same folder and starts the new version.
- **Linux / macOS:** swaps the binary in the install folder in place (re-signing the app on macOS) and restarts.

### Upgrading from launchers before 11.0

`version.txt` points old launchers at the new files:

```
11.0.0
https://github.com/Akulav/akulav-mc-launcher/releases/download/11.0.0/AkulavLauncher-setup.exe
<SHA-256 of the setup>
linux https://github.com/Akulav/akulav-mc-launcher/releases/download/11.0.0/AkulavLauncher-linux-x64 <SHA-256>
```

- **10.x on Windows** swaps the setup in as its own `AkulavLauncher.exe` and starts it. The setup notices, restarts
  from the temp folder and shows the normal wizard, so the user picks the install folder once. Old data in
  `%APPDATA%\AkulavLauncher` is moved into it, and the old *Apps* entry and `C:\AkulavLauncher` (9.x) are removed.
- **10.x on Linux** swaps in the new binary in `~/.local/share/AkulavLauncher/bin/`; it moves itself up into
  `~/.local/share/AkulavLauncher` (where the data already is) and carries on.
- **9.x** downloads line 2 as its installer and runs it.

### Building from source

```bash
cargo build --release
```

The launcher is `target/release/AkulavLauncher(.exe)`. The Windows setup needs [Inno Setup 6](https://jrsoftware.org/isinfo.php):

```bash
iscc /DAppVersion=11.0.1 /DSourceExe=%CD%\target\release\AkulavLauncher.exe installer\akulav-launcher.iss
```

It writes `dist\AkulavLauncher-setup.exe`. For a Linux binary that runs on older distros:
`pip install ziglang && cargo install cargo-zigbuild`, then `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28`.
Debug builds never offer to install themselves (pass `--here` to a release build for the same). `cargo test` runs the unit tests.

</details>

## 🙏 Credits

Icons by [Phosphor Icons](https://phosphoricons.com) (MIT). Inter font by Rasmus Andersson (SIL OFL 1.1, see `assets/fonts`).
DejaVu Sans Mono (Bitstream Vera license). Launcher icon by [Smashicons](https://www.flaticon.com/authors/smashicons).

---

<div align="center">

**Made with ❤️ by [Akulav](https://github.com/Akulav)**

<sub>© 2026 Akulav. Source-available under the <a href="LICENSE">Akulav Source-Available License</a>. Not affiliated with Mojang or Microsoft.</sub>

</div>
