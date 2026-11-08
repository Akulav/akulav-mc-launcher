//! Linux and macOS: the downloaded binary installs itself into a folder the user picks.
//!
//! * Linux: `<folder>/AkulavLauncher`, plus an app-menu entry, icon, Desktop shortcut and an
//!   `akulav-launcher` command in `~/.local/bin`.
//! * macOS: `<folder>/Akulav Launcher.app` (ad-hoc signed).
//!
//! The folder is remembered in `<config dir>/akulav-launcher/install-dir` so a newer download can
//! offer to update it, and launcher data goes into the same folder.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::{installed_root, installed_version, is_install_root, same_path, sibling, swap_in, write_marker, Offer, Startup, StartupInfo, EXE_NAME, NEW_SUFFIX};
use crate::launcher::update::{is_newer, CURRENT_VERSION};

fn pointer_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("akulav-launcher").join("install-dir"))
}

pub fn remembered_root() -> Option<PathBuf> {
    let text = std::fs::read_to_string(pointer_file()?).ok()?;
    let dir = text.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

fn remember(root: &Path) -> Result<()> {
    let file = pointer_file().context("no config folder")?;
    std::fs::create_dir_all(file.parent().unwrap())?;
    std::fs::write(&file, root.to_string_lossy().as_bytes())?;
    Ok(())
}

fn forget() {
    if let Some(f) = pointer_file() {
        let _ = std::fs::remove_file(&f);
        if let Some(d) = f.parent() {
            let _ = std::fs::remove_dir(d);
        }
    }
}

pub fn default_install_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    if cfg!(target_os = "macos") {
        home.join("Applications").join("Akulav Launcher")
    } else {
        dirs::data_local_dir().unwrap_or_else(|| home.join(".local/share")).join("AkulavLauncher")
    }
}

/// The program inside an install folder.
pub fn installed_bin(root: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        macos::bundle(root).join("Contents/MacOS").join(EXE_NAME)
    } else {
        root.join(EXE_NAME)
    }
}

pub fn startup_offer() -> Offer {
    match installed_root() {
        Some(root) => {
            let installed = installed_version(&root);
            let missing = !installed_bin(&root).is_file();
            if missing || installed.as_deref().is_none_or(|v| is_newer(CURRENT_VERSION, v)) {
                Offer::Update { root, installed }
            } else {
                Offer::None
            }
        }
        None => Offer::Install { suggested: default_install_dir() },
    }
}

/// Launchers 10.x installed to `~/.local/share/AkulavLauncher/bin/` and update by swapping that
/// binary. When this version arrives there, move it up into the folder (which already holds the
/// data) and continue from there without asking.
pub fn before_run(exe: &Path, info: &StartupInfo) -> Option<Startup> {
    if !cfg!(target_os = "linux") || cfg!(debug_assertions) {
        return None;
    }
    let bin_dir = exe.parent()?;
    let root = bin_dir.parent()?;
    let legacy_root = dirs::data_local_dir()?.join("AkulavLauncher");
    if bin_dir.file_name()? != "bin" || !same_path(root, &legacy_root) {
        return None;
    }
    let mut args = vec![];
    if let Some(v) = &info.updated_from {
        args.extend(["--updated-from", v.as_str()]);
    }
    match install_and_launch(root, &args) {
        Ok(()) => Some(Startup::Exit),
        Err(e) => {
            eprintln!("could not move the launcher to {}: {e:#}", root.display());
            None
        }
    }
}

pub fn on_installed_start(root: &Path) {
    // Keep the marker's version current after in-app updates, and the folder remembered.
    if super::installed_version(root).as_deref() != Some(CURRENT_VERSION) {
        let _ = write_marker(root);
    }
    if remembered_root().is_none_or(|r| !same_path(&r, root)) {
        let _ = remember(root);
    }
}

/// Copy this binary into `root`, mark and remember the folder, add menu entries.
fn install_to(root: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(root).with_context(|| format!("could not create {}", root.display()))?;
    let me = std::env::current_exe()?;
    let bin = installed_bin(root);
    if !same_path(&me, &bin) {
        std::fs::create_dir_all(bin.parent().unwrap())?;
        let staged = sibling(&bin, NEW_SUFFIX);
        std::fs::copy(&me, &staged).with_context(|| format!("could not copy the launcher to {}", bin.display()))?;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        swap_in(&staged, &bin)?;
    }
    #[cfg(target_os = "linux")]
    {
        linux::integrate(&bin);
        // The 10.x layout kept the binary in a bin/ subfolder. Deleting it is fine even while it
        // runs: Linux keeps the file alive until the process exits.
        if root.join("bin").join(EXE_NAME).is_file() {
            let _ = std::fs::remove_dir_all(root.join("bin"));
        }
    }
    #[cfg(target_os = "macos")]
    macos::write_bundle(root)?;
    write_marker(root)?;
    remember(root)?;
    Ok(bin)
}

fn launch(root: &Path, bin: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("/usr/bin/open");
        c.arg("-n").arg(macos::bundle(root));
        if !args.is_empty() {
            c.arg("--args");
        }
        c
    } else {
        Command::new(bin)
    };
    cmd.args(args).current_dir(root);
    cmd.spawn().map(|_| ()).context("installed, but could not start it")
}

pub fn install_and_launch(root: &Path, args: &[&str]) -> Result<()> {
    let bin = install_to(root)?;
    launch(root, &bin, args)
}

pub fn staging_path() -> Result<PathBuf> {
    Ok(sibling(&std::env::current_exe()?, NEW_SUFFIX))
}

/// Swap the verified binary in place and start it.
pub fn apply_update(staged: &Path) -> Result<()> {
    let exe = std::env::current_exe()?;
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(0o755))?;
    swap_in(staged, &exe)?;
    let root = super::root_of_exe(&exe).context("unknown install folder")?;
    if cfg!(target_os = "macos") && is_install_root(&root) {
        macos::sign(&macos::bundle(&root));
        return launch(&root, &exe, &["--updated-from", CURRENT_VERSION]);
    }
    Command::new(&exe)
        .args(["--updated-from", CURRENT_VERSION])
        .spawn()
        .map(|_| ())
        .context("updated, but could not restart")
}

/// Remove the install (and with `delete_data` the launcher's data in it).
pub fn uninstall(delete_data: bool) -> Result<()> {
    let root = installed_root().context("Akulav Launcher is not installed")?;
    #[cfg(target_os = "linux")]
    linux::remove_integration();
    if cfg!(target_os = "macos") {
        let _ = std::fs::remove_dir_all(macos::bundle(&root));
    } else {
        // A running binary can be deleted on Linux; the process keeps its copy until it exits.
        let bin = installed_bin(&root);
        for p in [bin.clone(), sibling(&bin, ".old"), sibling(&bin, ".new")] {
            let _ = std::fs::remove_file(p);
        }
    }
    let _ = std::fs::remove_file(root.join(super::MARKER));
    forget();
    if delete_data {
        super::delete_data(&root);
        if let Some(legacy) = super::legacy_data_root() {
            super::delete_data(&legacy);
        }
    }
    let _ = std::fs::remove_dir(&root);
    Ok(())
}

pub fn uninstall_interactive() {
    use std::io::{IsTerminal, Write};
    let ask = |q: &str| {
        print!("{q} [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).is_ok() && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    };
    if !std::io::stdin().is_terminal() {
        eprintln!("Run `akulav-launcher --uninstall` from a terminal, or use Settings → Storage → Uninstall launcher.");
        return;
    }
    let Some(root) = installed_root() else {
        println!("Akulav Launcher is not installed.");
        return;
    };
    if !ask(&format!("Uninstall Akulav Launcher from {}?", root.display())) {
        return;
    }
    let delete_data = ask("Also delete downloaded modpacks, worlds and settings?");
    match uninstall(delete_data) {
        Ok(()) => println!("Akulav Launcher has been uninstalled."),
        Err(e) => eprintln!("Uninstall failed: {e:#}"),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};

    const APP_ID: &str = "akulav-launcher";
    static ICON: &[u8] = include_bytes!("../../../assets/icon.png");

    fn applications_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("applications"))
    }
    fn icon_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("icons/hicolor/256x256/apps").join(format!("{APP_ID}.png")))
    }
    fn bin_link() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".local/bin").join(APP_ID))
    }
    fn desktop_shortcut() -> Option<PathBuf> {
        dirs::desktop_dir().filter(|d| d.is_dir()).map(|d| d.join(format!("{APP_ID}.desktop")))
    }

    /// Quote a path for a .desktop `Exec=` key (freedesktop spec: escape " ` $ \ inside quotes).
    fn exec_quote(p: &Path) -> String {
        let mut out = String::from("\"");
        for c in p.to_string_lossy().chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }

    pub fn desktop_entry(exe: &Path) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Akulav Launcher\nGenericName=Minecraft Launcher\nComment=Play Akulav Minecraft modpacks - made by Akulav\nExec={}\nIcon={APP_ID}\nTerminal=false\nCategories=Game;\nKeywords=minecraft;modpack;launcher;\nStartupWMClass={APP_ID}\n",
            exec_quote(exe)
        )
    }

    fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, contents)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }

    /// App menu entry, icon, desktop shortcut and terminal command. Each step is best-effort.
    pub fn integrate(exe: &Path) {
        let entry = desktop_entry(exe);
        if let Some(icon) = icon_path() {
            let _ = std::fs::create_dir_all(icon.parent().unwrap());
            if let Err(e) = std::fs::write(&icon, ICON) {
                eprintln!("icon: {e}");
            }
        }
        if let Some(dir) = applications_dir() {
            if let Err(e) = write_executable(&dir.join(format!("{APP_ID}.desktop")), &entry) {
                eprintln!("menu entry: {e}");
            }
            let _ = std::process::Command::new("update-desktop-database").arg(&dir).output();
        }
        if let Some(shortcut) = desktop_shortcut() {
            if write_executable(&shortcut, &entry).is_ok() {
                // GNOME only launches desktop files marked trusted.
                let _ = std::process::Command::new("gio").args(["set"]).arg(&shortcut).args(["metadata::trusted", "true"]).output();
            }
        }
        if let Some(link) = bin_link() {
            let _ = std::fs::create_dir_all(link.parent().unwrap());
            let _ = std::fs::remove_file(&link);
            let _ = std::os::unix::fs::symlink(exe, &link);
        }
    }

    pub fn remove_integration() {
        if let Some(dir) = applications_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{APP_ID}.desktop")));
            let _ = std::process::Command::new("update-desktop-database").arg(&dir).output();
        }
        for p in [icon_path(), desktop_shortcut(), bin_link()].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
        // Window size/position remembered by the UI toolkit. It is written again when the window
        // closes, so delete it once more a moment after we've exited.
        if let Some(d) = dirs::data_local_dir() {
            let dir = d.join(APP_ID);
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::process::Command::new("sh")
                .args(["-c", "sleep 2; rm -rf -- \"$1\"", "sh"])
                .arg(&dir)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_entry_quotes_exec() {
            let e = desktop_entry(Path::new("/home/a b/$x/AkulavLauncher"));
            assert!(e.contains(r#"Exec="/home/a b/\$x/AkulavLauncher""#), "{e}");
            assert!(e.contains("Icon=akulav-launcher"));
            assert!(e.starts_with("[Desktop Entry]\n"));
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod macos {
    use std::path::{Path, PathBuf};

    use anyhow::Result;

    use crate::launcher::install::EXE_NAME;
    use crate::launcher::update::CURRENT_VERSION;

    static ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

    pub fn bundle(root: &Path) -> PathBuf {
        root.join("Akulav Launcher.app")
    }

    /// `.icns` holding the 256×256 PNG icon (`ic08`).
    pub fn icns() -> Vec<u8> {
        let entry_len = 8 + ICON_PNG.len() as u32;
        let mut out = Vec::with_capacity(8 + entry_len as usize);
        out.extend_from_slice(b"icns");
        out.extend_from_slice(&(8 + entry_len).to_be_bytes());
        out.extend_from_slice(b"ic08");
        out.extend_from_slice(&entry_len.to_be_bytes());
        out.extend_from_slice(ICON_PNG);
        out
    }

    fn info_plist() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Akulav Launcher</string>
  <key>CFBundleDisplayName</key><string>Akulav Launcher</string>
  <key>CFBundleIdentifier</key><string>com.akulav.launcher</string>
  <key>CFBundleExecutable</key><string>{EXE_NAME}</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{CURRENT_VERSION}</string>
  <key>CFBundleVersion</key><string>{CURRENT_VERSION}</string>
  <key>NSHumanReadableCopyright</key><string>Made by Akulav</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#
        )
    }

    pub fn sign(bundle: &Path) {
        let _ = std::process::Command::new("/usr/bin/codesign").args(["--force", "--deep", "-s", "-"]).arg(bundle).output();
    }

    /// Info.plist and icon around the binary already copied into `Contents/MacOS`, then sign.
    pub fn write_bundle(root: &Path) -> Result<()> {
        let app = bundle(root);
        std::fs::create_dir_all(app.join("Contents/Resources"))?;
        std::fs::write(app.join("Contents/Resources/icon.icns"), icns())?;
        std::fs::write(app.join("Contents/Info.plist"), info_plist())?;
        sign(&app);
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn icns_header() {
            let i = super::icns();
            assert_eq!(&i[..4], b"icns");
            assert_eq!(u32::from_be_bytes(i[4..8].try_into().unwrap()) as usize, i.len());
            assert_eq!(&i[8..12], b"ic08");
            assert_eq!(&i[16..20], b"\x89PNG");
        }
    }
}
