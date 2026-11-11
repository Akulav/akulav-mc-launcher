# Changelog

All notable changes to Akulav Launcher are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Older versions are described on the
[releases page](https://github.com/Akulav/akulav-mc-launcher/releases).

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
