# CLAUDE.md

Notes for working on **Akulav Launcher** (repo `akulav-mc-launcher`, crate `akulav-launcher`), a lightweight
Minecraft modpack launcher made by Akulav. Rust + eframe/egui. One file per OS that installs and updates itself.

## Commands

```bash
cargo run
cargo test                                   # CI runs: cargo test --release
cargo clippy --all-targets -- -D warnings
cargo build --release
```

git is not on PATH on the dev machine: use GitHub Desktop's
`%LOCALAPPDATA%\GitHubDesktop\app-*\resources\app\git\cmd\git.exe`.

## Layout

- `src/main.rs`, `src/cli.rs` — entry point and CLI flags
- `src/launcher/` — backend: `minecraft/` (versions, loaders, Java, install, launch args), `modpacks.rs`,
  `skins.rs`, `jobs.rs` (background work), `events.rs`, `paths.rs`, `settings.rs`, `net.rs`, `system.rs`,
  `update.rs` (GitHub releases + `SHA256SUMS.txt`), `install/` (Windows / Unix self-install)
- `src/ui/` — egui app (`app.rs`), pages (`home.rs`, `settings_page.rs`, `console.rs`), `theme.rs`, `widgets.rs`
- `installer/akulav-launcher.iss` — Inno Setup script. **Never change its AppId.**
- `.github/workflows/build.yml` — tests, then builds and publishes the release

## Conventions & gotchas

- This is the reference install model the sibling apps copy: **everything the app writes lives in the install
  folder** (see `paths.rs` / `install/`), not the OS config or temp folders.
- Release asset names (`AkulavLauncher-setup.exe`, `AkulavLauncher-linux-x64`, `AkulavLauncher-macos-arm64`,
  `SHA256SUMS.txt`) are read by installed copies' updater: keep them.
- Never block the UI thread on network or disk work; it goes through `jobs.rs`.
- Version lives in `Cargo.toml`; note changes in `CHANGELOG.md` in the same commit. `Cargo.lock` is committed.
- Linux/macOS-only code can't be built on the Windows dev machine: keep it small, review by hand, let CI check it.
- Branding: "Made by Akulav".

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships. It is local only (`graphify-out/` is git-ignored).

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost). If graphify-out/ is missing, rebuild it the same way.
