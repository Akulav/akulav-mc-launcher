# Changelog

All notable changes to Akulav Launcher are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Older versions are described on the
[releases page](https://github.com/Akulav/akulav-mc-launcher/releases).

## [11.2.0] - 2026-11-21

### Changed
- **New look:** Windows 11's dark style, like Akulav's other apps: a flat #202020 window, #2b2b2b cards with hairline
  borders, the Windows accent blue (black text on the Play button), 4 px controls and 8 px cards, Windows-style
  switches and navigation, and Segoe UI on Windows (Inter elsewhere). No more glows, gradients or the glowing Play
  button; errors are shown in red instead of the old red accent.

### Removed
- Upgrading straight from launchers older than 11.0 (the 9.x C# launcher and the self-updating 10.x launcher):
  the launcher no longer moves data from `%APPDATA%\AkulavLauncher` / `~/.local/share/AkulavLauncher`, no longer
  cleans up `C:\AkulavLauncher` or the old *Apps* entry, the 10.x Linux `bin/` layout is no longer moved, the setup
  no longer restarts itself when started as `AkulavLauncher.exe`, and releases no longer ship `version.txt`.
  Update to 11.1.1 first, or install fresh.
- Unused code: the skin model flag, the stored game PID and version chain, and other leftovers.

## [11.1.1] - 2026-11-20

### Changed
- Docs: a new `docs/ARCHITECTURE.md` (how the launcher prepares and starts a pack), linked from the README.

## [11.1.0] - 2026-11-12

### Changed
- **The system's own title bar and window buttons** (minimise, maximise, close, snapping, drag to move) instead of custom-drawn ones. The app's name and update status moved into the sidebar.
- **New look**, in the style of Akulav's Hyve and Lumen: a deep blue-black background with soft glows, the sidebar as a floating glass panel with the app name at the top, glass cards, the current page highlighted with a gradient, softer translucent buttons and a clear accent ring around the focused text field. The Play page keeps its redstone wallpaper.

## [11.0.1] - 2026-11-12

### Fixed
- Everything now really stays in the folder you installed to. When the launcher is installed on another drive
  (for example `D:\Games\AkulavLauncher`), modpacks, worlds and Java go there too; before, data from older launchers
  that could not be moved across drives kept being used from `%APPDATA%\AkulavLauncher`.
- Old launcher data is now moved into the install folder even when it is on a different drive (it is copied, then the
  old copy is removed).
- The window size and position are saved in the install folder (`window.ron`) instead of `%APPDATA%` /
  `~/.local/share`.
- In-app updates download the new setup into the install folder (`cache\update`) instead of the system temp folder.
- A copy that is not installed keeps its data next to the program instead of in `%APPDATA%`.
- Minecraft and the Forge/NeoForge installers use `cache\tmp` in the install folder as their temp folder instead of the
  system temp folder.

### Changed
- The uninstaller also removes the downloaded update and the saved window position.
