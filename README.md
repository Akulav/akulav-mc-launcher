# Akulav Launcher 10 (Rust)

An open-source, lightweight Minecraft modpack launcher. No spyware, no telemetry, no bloat.
Version 10 is a ground-up rewrite of the C# launcher in Rust: a single native `.exe` with no .NET dependency.

## What's new compared to 9.x

**Interface**
- New dark interface built around the redstone wallpaper: pack hero, library strip, glass play bar.
- Play button shows live progress (percent, MB/s, time left) and turns into **Cancel** on hover.
- Live **Console** page with game, installer and launcher output, level filter, search, copy, crash banner.
- Per-pack settings drawer: memory slider with a recommendation, window size, Java arguments, what happens after launch.
- Toast notifications instead of message boxes; inline nickname validation; procedurally drawn avatar.

**Under the hood**
- Pure Rust launch engine (no CmlLib): `inheritsFrom` merging, OS rules, natives, assets, parallel downloads with SHA-1 checks and retries.
- Mojang's own Java runtime for each Minecraft version is downloaded automatically (or point to your own Java).
- **Automatic mod loader install** from the `API` field when a pack zip doesn't ship it: NeoForge, Forge, Fabric, Quilt.
- Shared libraries/assets between packs (no duplicate downloads); each pack gets its own game folder.
- Pack updates wipe `mods` and `config` like before, but keep saves, screenshots and options.
- If the modpack server is unreachable, the last known list is used and installed packs still launch.
- **Repair** re-downloads the pack and re-verifies every file.
- Play time and last played tracking per pack.
- Self-installing exe and one-click in-place self-update (see below).

## Installing and updating

**Install.** There is no installer: run `AkulavLauncher.exe` from anywhere and it installs itself to
`%LOCALAPPDATA%\Programs\AkulavLauncher` (no admin rights), adds Desktop and Start-menu shortcuts and an
entry in *Settings → Apps* (uninstall from there), then restarts from the installed copy. Put an empty
`portable.txt` next to the exe to run it portable instead.

**Linux.** Download `AkulavLauncher-linux-x86_64.tar.gz`, extract it and run `AkulavLauncher-linux-x86_64`
(or download the bare binary and `chmod +x` it). It installs itself to
`~/.local/share/AkulavLauncher/bin/`, adds an app-menu entry with icon, a Desktop shortcut and an
`akulav-launcher` terminal command, then restarts from there. Uninstall from *Settings → Storage* or with
`akulav-launcher --uninstall`. Needs glibc 2.28+ (Ubuntu 20.04, Debian 10, Fedora 29, Mint 20 or newer),
OpenGL and X11 or Wayland. Mojang ships no ARM Linux Java, so ARM needs a custom Java path.

**Updates.** The launcher reads `version.txt` at startup and every 2 hours:

```
10.1.0
https://github.com/Akulav/AkulavMCLauncher/releases/download/10.1.0/AkulavLauncher.exe
<optional SHA-256 of the exe>
linux https://github.com/Akulav/AkulavMCLauncher/releases/download/10.1.0/AkulavLauncher-linux-x86_64 <optional SHA-256>
```

Line 2 is the **Windows launcher exe itself**, not an installer (it's also what 9.x launchers download).
The `linux` line points at the bare Linux binary (not the .tar.gz). A Linux launcher only updates when that
line exists, so it is never offered the Windows exe; old launchers ignore the extra line. When line 1 is higher than the running
version, the title bar shows *Update available*; one click downloads the exe in the background and
verifies it (executable header, plus the SHA-256 when given). *Restart & update* swaps the files in place
(Windows lets a running exe be renamed, so the old one becomes `AkulavLauncher.old.exe` and is deleted on
the next start) and relaunches the new version.

**Upgrading 9.x users.** Publish 10.x with line 2 pointing at the new `AkulavLauncher.exe`. The old C#
launcher downloads that file as its "installer" and runs it; the new launcher then installs itself,
replaces the old desktop shortcut and removes `C:\AkulavLauncher`.

To publish a release: build, upload `AkulavLauncher.exe` and `AkulavLauncher-linux-x86_64`, then update
`version.txt` (`sha256sum <file>` or `certutil -hashfile AkulavLauncher.exe SHA256` for the hashes).

## Skins

The avatar shows the player's real skin as a 3D head when the nickname has one on Mojang or Ely.by
(the same sources CustomSkinLoader uses in game); otherwise a head is generated from the name.

## Data locations

Everything lives in `%APPDATA%\AkulavLauncher` (Linux: `~/.local/share/AkulavLauncher`):

| Path | Contents |
| --- | --- |
| `settings.json` | launcher + per-pack settings |
| `installed.json` | installed pack versions |
| `shared/` | versions, libraries, assets, Java runtimes |
| `instances/<pack>/` | each pack's game folder (mods, config, saves…) |
| `cache/` | temporary downloads |

## Remote files

- Modpack list: `https://akulav.com/shared/aa7e873115b2344cc291045b` (same format as before; can be overridden in Settings)
- Launcher version: `https://akulav.com/shared/5ce6926c3dda5f322df1ac59` (line 1: version, line 2: installer URL)

`modpacks.json` entries can now optionally include a `"Description"` shown on the pack page:

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

> When you publish this version, bump `version.txt` to `10.0.0` (or higher) with the new installer URL.
> The launcher only offers updates whose version is higher than its own.

## Building

1. Install Rust from <https://rustup.rs>.
2. `cargo build --release`
3. The launcher is `target\release\AkulavLauncher.exe` (Linux: `target/release/AkulavLauncher`).

For a Linux binary that runs on older distros: `pip install ziglang && cargo install cargo-zigbuild`, then
`cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28`. Debug builds never install themselves.

`cargo test` runs the unit tests. Pushing to GitHub runs `.github/workflows/build.yml`, which tests and builds
Windows and Linux binaries and attaches the `.exe` to tagged releases.

## Command line

For troubleshooting (output goes to the terminal):

```
AkulavLauncher.exe --cli list
AkulavLauncher.exe --cli launch "GoldenEra - Ultimate" [--user NAME] [--ram 6144] [--repair] [--prepare-only]
```

Set `AKULAV_DATA_DIR` to use a different data folder (portable installs).

## Credits

Icons by Phosphor Icons (MIT). Inter font by Rasmus Andersson (SIL OFL 1.1, see `assets/fonts`; private-use glyphs removed so they don't shadow the icon font).
DejaVu Sans Mono (Bitstream Vera license). Launcher icon by https://www.flaticon.com/authors/smashicons.
