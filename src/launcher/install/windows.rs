//! Windows: the Inno Setup installer (`installer/akulav-launcher.iss`) installs, updates and
//! uninstalls. The launcher only finds the folder Inno remembered, runs the setup in update mode,
//! and starts the Inno uninstaller.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use super::{installed_root, Offer, Startup, StartupInfo};
use crate::launcher::update::{ASSET_WINDOWS, CURRENT_VERSION};

/// Inno Setup `AppId` from the .iss file; Inno keeps the chosen folder in this uninstall key.
pub const INNO_UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{7FBF629D-D100-4E71-AE93-4977A99B250F}_is1";
/// Launchers before 11.0 installed themselves and registered this key.
const LEGACY_UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\AkulavLauncher";
/// The 9.x C# launcher lived here.
const LEGACY_DIR: &str = r"C:\AkulavLauncher";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Install folder remembered by the setup.
pub fn remembered_root() -> Option<PathBuf> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey(INNO_UNINSTALL_KEY).ok()?;
    let dir: String = key.get_value("InstallLocation").ok()?;
    let dir = dir.trim().trim_end_matches('\\');
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

pub fn before_run(_exe: &Path, _info: &StartupInfo) -> Option<Startup> {
    None
}

/// The setup does the installing on Windows.
pub fn startup_offer() -> Offer {
    Offer::None
}

pub fn on_installed_start(root: &Path) {
    if super::installed_version(root).as_deref() != Some(CURRENT_VERSION) {
        let _ = super::write_marker(root);
    }
    let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(LEGACY_UNINSTALL_KEY);
    let legacy = Path::new(LEGACY_DIR);
    if legacy.join("AkulavLauncher.exe").is_file() || legacy.join("update.exe").is_file() {
        delete_later(legacy);
    }
}

/// Delete a folder a few seconds from now, after any process running from it has exited.
fn delete_later(dir: &Path) {
    let script = format!("ping 127.0.0.1 -n 4 > nul & rmdir /S /Q \"{}\"", dir.display());
    let _ = Command::new("cmd").args(["/C", &script]).creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn();
}

pub fn install_and_launch(_root: &Path, _args: &[&str]) -> Result<()> {
    bail!("on Windows, install with {ASSET_WINDOWS}")
}

/// The setup is downloaded into the launcher's own folder (`cache\update`), never `%TEMP%`.
pub fn staging_path() -> Result<PathBuf> {
    let root = super::data_root().context("unknown launcher folder")?;
    let dir = root.join("cache").join("update");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(ASSET_WINDOWS))
}

/// Run the setup silently in update mode. It installs into the same folder, closes this launcher
/// and starts the new one with `--updated-from <this version>`.
pub fn apply_update(setup: &Path) -> Result<()> {
    let mut cmd = Command::new(setup);
    cmd.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/UPDATE"]);
    cmd.arg(format!("/FROM={CURRENT_VERSION}"));
    if let Some(root) = super::program_root().filter(|r| super::is_install_root(r)) {
        cmd.arg(format!("/DIR={}", root.display()));
    }
    cmd.spawn().map(|_| ()).context("could not start the setup")
}

fn uninstaller() -> Option<PathBuf> {
    installed_root().map(|r| r.join("unins000.exe")).filter(|u| u.is_file())
}

/// Start the Inno uninstaller; it asks whether to keep modpacks and worlds.
pub fn uninstall(_delete_data: bool) -> Result<()> {
    let unins = uninstaller().context("no uninstaller found; remove Akulav Launcher from Settings → Apps")?;
    Command::new(unins).spawn().map(|_| ()).context("could not start the uninstaller")
}

pub fn uninstall_interactive() {
    match uninstaller() {
        Some(u) => {
            let _ = Command::new(u).spawn();
        }
        None => message_box("Akulav Launcher is not installed here. Remove it from Settings → Apps.", 0x40),
    }
}

fn message_box(text: &str, flags: u32) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, c) = (w(text), w("Akulav Launcher"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags);
    }
}
