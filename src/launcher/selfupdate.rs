//! Self-install and in-place self-update.
//!
//! **Install.** Run from anywhere (Downloads, or as `update.exe` from the old 9.x launcher), the
//! exe copies itself to `%LOCALAPPDATA%\Programs\AkulavLauncher\AkulavLauncher.exe`, adds
//! Desktop and Start-menu shortcuts and an "Apps & features" entry (per-user, no admin), removes
//! the old `C:\AkulavLauncher` install, then restarts from the installed location.
//! A `portable.txt` file next to the exe (or `--portable`) skips this.
//!
//! On **Linux** the binary installs to `~/.local/share/AkulavLauncher/bin/AkulavLauncher` and adds
//! an app-menu entry, an icon, a desktop shortcut (when a Desktop folder exists) and an
//! `akulav-launcher` command in `~/.local/bin`. Uninstall from Settings or `--uninstall`.
//!
//! **Update.** The new exe is downloaded next to the running one as `AkulavLauncher.new.exe`,
//! checked (PE header, optional SHA-256 from `version.txt`), then swapped in: Windows allows
//! *renaming* a running exe, so the current one becomes `AkulavLauncher.old.exe`, the new one
//! takes its name, and it is started with `--updated-from <old version>`. The leftover `.old.exe`
//! is deleted on the next start.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use sha2::Digest;

use super::events::Cancel;
use super::net::{self, DownloadTask};
use super::update::UpdateInfo;

pub const EXE_NAME: &str = if cfg!(windows) { "AkulavLauncher.exe" } else { "AkulavLauncher" };
const NEW_SUFFIX: &str = ".new";
const OLD_SUFFIX: &str = ".old";
#[cfg_attr(not(windows), allow(dead_code))]
pub const LEGACY_DIR: &str = r"C:\AkulavLauncher";
#[cfg_attr(not(windows), allow(dead_code))]
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\AkulavLauncher";

/// `AkulavLauncher.exe` → `AkulavLauncher.old.exe`
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "AkulavLauncher".into());
    let name = match exe.extension() {
        Some(ext) => format!("{stem}{suffix}.{}", ext.to_string_lossy()),
        None => format!("{stem}{suffix}"),
    };
    exe.with_file_name(name)
}

pub fn install_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("AKULAV_INSTALL_DIR").filter(|d| !d.is_empty()) {
        return Some(d.into());
    }
    if cfg!(windows) {
        dirs::data_local_dir().map(|d| d.join("Programs").join("AkulavLauncher"))
    } else if cfg!(target_os = "linux") {
        dirs::data_local_dir().map(|d| d.join("AkulavLauncher").join("bin"))
    } else {
        None
    }
}

pub fn is_portable(exe: &Path, args: &[String]) -> bool {
    args.iter().any(|a| a == "--portable")
        || std::env::var_os("AKULAV_PORTABLE").is_some()
        || exe.parent().is_some_and(|d| d.join("portable.txt").exists())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (canon(a), canon(b));
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = sha2::Sha256::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        use std::io::Read;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => matches!((sha256_file(a), sha256_file(b)), (Ok(x), Ok(y)) if x == y),
        _ => false,
    }
}

/// Replace `target` with `source` even if `target` is the running executable.
fn swap_in(source: &Path, target: &Path) -> Result<()> {
    let old = sibling(target, OLD_SUFFIX);
    let _ = std::fs::remove_file(&old);
    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, &old).with_context(|| format!("could not move {} aside", target.display()))?;
    }
    if let Err(e) = std::fs::rename(source, target).or_else(|_| std::fs::copy(source, target).map(|_| ())) {
        if had_target {
            let _ = std::fs::rename(&old, target); // roll back
        }
        return Err(e).with_context(|| format!("could not write {}", target.display()));
    }
    let _ = std::fs::remove_file(source);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// What `main` should do after startup housekeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    Run { installed: bool, updated_from: Option<String> },
    Exit,
}

pub fn on_startup(args: &[String]) -> Startup {
    let Ok(exe) = std::env::current_exe() else { return Startup::Run { installed: false, updated_from: None } };

    // Leftover from the previous update.
    let _ = std::fs::remove_file(sibling(&exe, OLD_SUFFIX));
    let _ = std::fs::remove_file(sibling(&exe, NEW_SUFFIX));

    if args.iter().any(|a| a == "--uninstall") {
        uninstall(&exe);
        return Startup::Exit;
    }
    let installed = args.iter().any(|a| a == "--installed");
    let updated_from = args.iter().position(|a| a == "--updated-from").and_then(|i| args.get(i + 1)).cloned();

    // Debug builds (cargo run) never install themselves.
    let supported = cfg!(any(windows, target_os = "linux")) && !cfg!(debug_assertions);
    if supported && !installed && !is_portable(&exe, args) {
        if let Some(dir) = install_dir() {
            let target = dir.join(EXE_NAME);
            if !same_path(&exe, &target) {
                match install_to(&exe, &dir) {
                    Ok(installed_exe) => {
                        if spawn_detached(&installed_exe, &["--installed".to_string()]).is_ok() {
                            return Startup::Exit;
                        }
                    }
                    Err(e) => eprintln!("self-install failed: {e:#}"),
                }
            }
        }
    }
    Startup::Run { installed, updated_from }
}

/// Copy `exe` into `dir` and register it. Returns the installed exe path.
pub fn install_to(exe: &Path, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join(EXE_NAME);
    if !same_content(exe, &target) {
        let staged = sibling(&target, NEW_SUFFIX);
        std::fs::copy(exe, &staged).context("could not copy the launcher")?;
        swap_in(&staged, &target)?;
    }
    #[cfg(windows)]
    {
        if let Err(e) = windows::create_shortcuts(&target) {
            eprintln!("shortcuts: {e:#}");
        }
        if let Err(e) = windows::register_uninstall(&target) {
            eprintln!("uninstall entry: {e:#}");
        }
        windows::remove_legacy_install(exe);
    }
    #[cfg(target_os = "linux")]
    linux::integrate(&target);
    Ok(target)
}

pub fn spawn_detached(exe: &Path, args: &[String]) -> std::io::Result<()> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map(|_| ())
}

// ---------------------------------------------------------------------------------------------
// Updating
// ---------------------------------------------------------------------------------------------

fn looks_like_executable(bytes: &[u8]) -> bool {
    if cfg!(windows) {
        bytes.starts_with(b"MZ")
    } else if cfg!(target_os = "macos") {
        bytes.len() >= 4 && matches!(&bytes[..4], [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
    } else {
        bytes.starts_with(b"\x7fELF")
    }
}

/// Download the new launcher next to the running one. `on_bytes(done, total)` reports progress.
pub async fn download_update(
    client: &reqwest::Client,
    info: &UpdateInfo,
    on_bytes: &(dyn Fn(u64, u64) + Send + Sync),
    cancel: &Cancel,
) -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let staged = sibling(&exe, NEW_SUFFIX);
    let total = client.head(&info.url).send().await.ok().and_then(|r| r.content_length()).unwrap_or(0);
    let done = std::sync::atomic::AtomicU64::new(0);
    let cb = |d: i64| {
        let now = if d >= 0 {
            done.fetch_add(d as u64, std::sync::atomic::Ordering::Relaxed) + d as u64
        } else {
            done.fetch_sub((-d) as u64, std::sync::atomic::Ordering::Relaxed) - (-d) as u64
        };
        on_bytes(now, total.max(now));
    };
    net::download_file(client, &DownloadTask::new(info.url.clone(), staged.clone()), &cb, cancel).await?;
    verify_update(&staged, info.sha256.as_deref())?;
    Ok(staged)
}

pub fn verify_update(file: &Path, sha256: Option<&str>) -> Result<()> {
    let mut head = [0u8; 4];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(file)?;
        let _ = f.read(&mut head)?;
    }
    if !looks_like_executable(&head) {
        let _ = std::fs::remove_file(file);
        bail!("the downloaded update is not a launcher executable (is the link in version.txt pointing at the .exe?)");
    }
    if let Some(expected) = sha256.filter(|s| !s.is_empty()) {
        let actual = sha256_file(file)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(file);
            bail!("update checksum mismatch");
        }
    }
    Ok(())
}

/// Swap the staged update in and start it. The caller must exit right after.
pub fn apply_update_and_restart(staged: &Path) -> Result<()> {
    let exe = std::env::current_exe()?;
    swap_in(staged, &exe)?;
    spawn_detached(&exe, &["--updated-from".into(), super::update::CURRENT_VERSION.into()])?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Uninstall
// ---------------------------------------------------------------------------------------------

fn uninstall(exe: &Path) {
    #[cfg(windows)]
    windows::uninstall(exe);
    #[cfg(target_os = "linux")]
    linux::uninstall_interactive(exe);
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = exe;
        eprintln!("Delete the executable to remove the launcher.");
    }
}

/// True when this process runs from the installed location (so it can be uninstalled).
pub fn running_installed() -> bool {
    match (std::env::current_exe(), install_dir()) {
        (Ok(exe), Some(dir)) => same_path(&exe, &dir.join(EXE_NAME)),
        _ => false,
    }
}

/// Remove the installed launcher (and optionally all data). Used by the Linux Settings page.
#[cfg(target_os = "linux")]
pub fn uninstall_now(delete_data: bool) -> Result<()> {
    linux::remove_integration();
    if let Some(dir) = install_dir() {
        // A running binary can be deleted on Linux; the process keeps its copy until it exits.
        std::fs::remove_dir_all(&dir).with_context(|| format!("could not remove {}", dir.display()))?;
    }
    if delete_data {
        let root = super::paths::Paths::detect()?.root;
        std::fs::remove_dir_all(&root).with_context(|| format!("could not remove {}", root.display()))?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{IsTerminal, Write};
    use std::path::{Path, PathBuf};

    const APP_ID: &str = "akulav-launcher";
    static ICON: &[u8] = include_bytes!("../../assets/icon.png");

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

    fn ask(question: &str) -> bool {
        print!("{question} [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).is_ok() && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }

    pub fn uninstall_interactive(_exe: &Path) {
        if !std::io::stdin().is_terminal() {
            eprintln!("Run `akulav-launcher --uninstall` from a terminal, or use Settings → Storage → Uninstall launcher.");
            return;
        }
        if !ask("Uninstall Akulav Launcher?") {
            return;
        }
        let delete_data = ask("Also delete downloaded modpacks, worlds and settings?");
        match super::uninstall_now(delete_data) {
            Ok(()) => println!("Akulav Launcher has been uninstalled."),
            Err(e) => eprintln!("Uninstall failed: {e:#}"),
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

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    use anyhow::Result;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    fn start_menu() -> Option<std::path::PathBuf> {
        dirs::data_dir().map(|d| d.join(r"Microsoft\Windows\Start Menu\Programs"))
    }

    fn link(target: &Path, dest: &Path) -> Result<()> {
        let mut sl = mslnk::ShellLink::new(target)?;
        sl.set_name(Some("Akulav Launcher".into()));
        sl.set_working_dir(target.parent().map(|p| p.to_string_lossy().to_string()));
        sl.set_icon_location(Some(target.to_string_lossy().to_string()));
        sl.create_lnk(dest)?;
        Ok(())
    }

    pub fn create_shortcuts(target: &Path) -> Result<()> {
        if let Some(desktop) = dirs::desktop_dir() {
            link(target, &desktop.join("Akulav Launcher.lnk"))?;
            // The 9.x installer's shortcut
            let _ = std::fs::remove_file(desktop.join("AkulavLauncher.lnk"));
        }
        if let Some(menu) = start_menu() {
            let _ = std::fs::create_dir_all(&menu);
            link(target, &menu.join("Akulav Launcher.lnk"))?;
        }
        Ok(())
    }

    pub fn register_uninstall(target: &Path) -> Result<()> {
        use winreg::enums::HKEY_CURRENT_USER;
        let (key, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(super::UNINSTALL_KEY)?;
        let exe = target.to_string_lossy().to_string();
        key.set_value("DisplayName", &"Akulav Launcher")?;
        key.set_value("DisplayVersion", &super::super::update::CURRENT_VERSION)?;
        key.set_value("Publisher", &"Akulav")?;
        key.set_value("DisplayIcon", &exe)?;
        key.set_value("InstallLocation", &target.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())?;
        key.set_value("UninstallString", &format!("\"{exe}\" --uninstall"))?;
        key.set_value("URLInfoAbout", &super::super::update::REPO_URL)?;
        key.set_value("HelpLink", &format!("{}/issues", super::super::update::REPO_URL))?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        let kb = std::fs::metadata(target).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
        key.set_value("EstimatedSize", &kb)?;
        Ok(())
    }

    /// Delete a folder a few seconds from now, after this process (which may live in it) exits.
    pub fn delete_later(dir: &Path) {
        let script = format!("ping 127.0.0.1 -n 4 > nul & rmdir /S /Q \"{}\"", dir.display());
        let _ = Command::new("cmd").args(["/C", &script]).creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn();
    }

    /// The C# launcher lived in C:\AkulavLauncher (and ran updates from there as update.exe).
    pub fn remove_legacy_install(_running_exe: &Path) {
        let legacy = Path::new(super::LEGACY_DIR);
        if legacy.join("AkulavLauncher.exe").is_file() || legacy.join("update.exe").is_file() {
            delete_later(legacy);
        }
    }

    fn message_box(text: &str, flags: u32) -> i32 {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, flags: u32) -> i32;
        }
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (t, c) = (w(text), w("Akulav Launcher"));
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags) }
    }

    pub fn uninstall(exe: &Path) {
        const MB_YESNO: u32 = 0x4;
        const MB_ICONQUESTION: u32 = 0x20;
        const IDYES: i32 = 6;
        if message_box("Uninstall Akulav Launcher?", MB_YESNO | MB_ICONQUESTION) != IDYES {
            return;
        }
        let delete_data = message_box(
            "Also delete downloaded modpacks, worlds and settings?\n\nChoose No to keep them for a future reinstall.",
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES;

        if let Some(desktop) = dirs::desktop_dir() {
            let _ = std::fs::remove_file(desktop.join("Akulav Launcher.lnk"));
        }
        if let Some(menu) = start_menu() {
            let _ = std::fs::remove_file(menu.join("Akulav Launcher.lnk"));
        }
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(super::UNINSTALL_KEY);
        if delete_data {
            if let Ok(paths) = crate::launcher::paths::Paths::detect() {
                let _ = std::fs::remove_dir_all(&paths.root);
            }
        }
        if let Some(dir) = exe.parent() {
            delete_later(dir);
        }
        message_box("Akulav Launcher has been uninstalled.", 0x40);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_names() {
        assert_eq!(sibling(Path::new("/x/AkulavLauncher.exe"), ".old"), PathBuf::from("/x/AkulavLauncher.old.exe"));
        assert_eq!(sibling(Path::new("/x/AkulavLauncher"), ".new"), PathBuf::from("/x/AkulavLauncher.new"));
    }

    #[test]
    fn swap_replaces_and_keeps_old() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("app.exe");
        let staged = d.path().join("app.new.exe");
        std::fs::write(&target, b"v1").unwrap();
        std::fs::write(&staged, b"v2").unwrap();
        swap_in(&staged, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"v2");
        assert_eq!(std::fs::read(d.path().join("app.old.exe")).unwrap(), b"v1");
        assert!(!staged.exists());
    }

    #[test]
    fn install_copies_once() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("download").join(EXE_NAME);
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, b"launcher").unwrap();
        let dir = d.path().join("Programs/AkulavLauncher");
        let t = install_to(&src, &dir).unwrap();
        assert_eq!(std::fs::read(&t).unwrap(), b"launcher");
        // second install with identical content doesn't create an .old copy
        install_to(&src, &dir).unwrap();
        assert!(!sibling(&t, OLD_SUFFIX).exists());
    }

    #[test]
    fn verifies_downloads() {
        let d = tempfile::tempdir().unwrap();
        let bad = d.path().join("bad");
        std::fs::write(&bad, b"<html>404</html>").unwrap();
        assert!(verify_update(&bad, None).is_err());
        assert!(!bad.exists(), "rejected downloads are deleted");

        let good = d.path().join("good");
        let header: &[u8] = if cfg!(windows) { b"MZ\x90\x00" } else { b"\x7fELF" };
        std::fs::write(&good, header).unwrap();
        assert!(verify_update(&good, None).is_ok());
        assert!(verify_update(&good, Some("00")).is_err());
    }
}
