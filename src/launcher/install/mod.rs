//! Where the launcher lives: installing, updating and removing it.
//!
//! **One file per OS does both install and update.**
//!
//! * **Windows** — `AkulavLauncher-setup.exe` (Inno Setup, `installer/akulav-launcher.iss`). The
//!   first run asks for the install folder. Inno remembers it, so running a newer setup later, or
//!   the in-app update (which starts the setup with `/UPDATE`), installs into the same folder
//!   without asking.
//! * **Linux / macOS** — the bare binary. Started from anywhere, it offers to install itself into a
//!   folder the user picks; the folder is remembered in `<config dir>/akulav-launcher/install-dir`.
//!   A newer download started later offers to update that folder. In-app updates swap the binary in
//!   place.
//!
//! **Files go there.** The install folder holds the program *and* all launcher data (settings,
//! modpacks, worlds, Java runtimes). A [`MARKER`] file identifies an install folder. Data left in
//! the pre-11 location (`%APPDATA%\AkulavLauncher`, `~/.local/share/AkulavLauncher`, …) is moved
//! in on the first start of the installed copy.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::Digest;

use super::events::Cancel;
use super::net::{self, DownloadTask};
use super::update::{UpdateInfo, CURRENT_VERSION};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg_attr(windows, allow(dead_code))]
pub const EXE_NAME: &str = if cfg!(windows) { "AkulavLauncher.exe" } else { "AkulavLauncher" };
/// Present in every install folder; holds the installed version.
pub const MARKER: &str = ".akulav-install";
/// Everything the launcher keeps in its data folder. Settings files come last so a half-finished
/// move never looks like a complete data folder.
pub const DATA_ENTRIES: &[&str] =
    &["shared", "instances", "cache", "logs", "modpacks_cache.json", "window.ron", "installed.json", "settings.json"];
/// Window size and position, kept by the UI toolkit in the data folder (not in the OS's app-data folder).
pub const WINDOW_STATE_FILE: &str = "window.ron";
const NEW_SUFFIX: &str = ".new";
const OLD_SUFFIX: &str = ".old";

// ---------------------------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------------------------

/// `AkulavLauncher.exe` → `AkulavLauncher.old.exe`
pub(crate) fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "AkulavLauncher".into());
    let name = match exe.extension() {
        Some(ext) => format!("{stem}{suffix}.{}", ext.to_string_lossy()),
        None => format!("{stem}{suffix}"),
    };
    exe.with_file_name(name)
}

/// Folder a program belongs to: its own folder, or for `X.app/Contents/MacOS/bin` the folder that
/// holds `X.app`.
pub fn root_of_exe(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let bundle = dir
        .parent()
        .filter(|_| dir.file_name().is_some_and(|n| n == "MacOS"))
        .filter(|contents| contents.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|b| b.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(b) => b.parent().map(Path::to_path_buf),
        None => Some(dir.to_path_buf()),
    }
}

/// Folder of the running program (see [`root_of_exe`]).
pub fn program_root() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| root_of_exe(&e))
}

pub fn is_install_root(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

/// Version recorded in an install folder's marker.
pub fn installed_version(root: &Path) -> Option<String> {
    let v = std::fs::read_to_string(root.join(MARKER)).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

pub(crate) fn write_marker(root: &Path) -> std::io::Result<()> {
    std::fs::write(root.join(MARKER), CURRENT_VERSION)
}

/// True when this process runs from an install folder.
pub fn running_installed() -> bool {
    program_root().is_some_and(|r| is_install_root(&r))
}

/// The install folder: the running copy's, otherwise the remembered one.
pub fn installed_root() -> Option<PathBuf> {
    program_root()
        .filter(|r| is_install_root(r))
        .or_else(|| platform::remembered_root().filter(|r| is_install_root(r)))
}

/// `portable.txt` next to the program, `--portable` or `AKULAV_PORTABLE`: keep data next to it.
pub fn portable() -> bool {
    std::env::var_os("AKULAV_PORTABLE").is_some() || program_root().is_some_and(|r| r.join("portable.txt").exists())
}

/// Where launchers before 11.0 kept their data.
pub fn legacy_data_root() -> Option<PathBuf> {
    dirs::data_dir().or_else(dirs::home_dir).map(|d| d.join("AkulavLauncher"))
}

fn has_data(dir: &Path) -> bool {
    dir.join("settings.json").is_file() || dir.join("installed.json").is_file()
}

/// The launcher's data folder (`AKULAV_DATA_DIR` is handled by `Paths::detect`). Always the
/// folder the launcher lives in: the install folder when installed, otherwise next to the
/// program. Nothing is ever kept in `%APPDATA%`, `~/.local/share` or other system folders.
pub fn data_root() -> Option<PathBuf> {
    if portable() {
        return program_root();
    }
    installed_root().or_else(program_root)
}

pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (canon(a), canon(b));
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Check a folder typed into the install dialog. `~` expands to the home folder.
pub fn parse_install_dir(text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Choose a folder.".into());
    }
    let path = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            dirs::home_dir().ok_or("Could not find your home folder.")?.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    };
    if !path.is_absolute() {
        return Err("Use a full path, for example ~/Games/AkulavLauncher.".into());
    }
    if path.parent().is_none() {
        return Err("Pick a folder, not the root of a drive.".into());
    }
    if path.is_file() {
        return Err("That's a file, not a folder.".into());
    }
    Ok(path)
}

// ---------------------------------------------------------------------------------------------
// Moving old data into the install folder
// ---------------------------------------------------------------------------------------------

fn remove_any(p: &Path) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// Copy a file or a whole folder tree.
fn copy_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_all(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// Move one file or folder. A rename is instant; when the two folders are on different drives
/// (e.g. `C:` → `D:`) it is copied and then the original is deleted. A failed copy leaves the
/// original untouched and removes the partial copy.
pub(crate) fn move_entry(src: &Path, dst: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if let Err(e) = copy_all(src, dst) {
        let _ = remove_any(dst);
        return Err(e);
    }
    let _ = remove_any(src);
    Ok(())
}

/// Move [`DATA_ENTRIES`] from `from` to `to`, across drives too. If anything fails, everything
/// moved so far goes back, so data is never split between two folders. Entries already present in
/// `to` are left. Returns how many entries moved.
fn move_data(from: &Path, to: &Path) -> Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut moved: Vec<&str> = Vec::new();
    for name in DATA_ENTRIES {
        let (src, dst) = (from.join(name), to.join(name));
        if !src.exists() || dst.exists() {
            continue;
        }
        if let Err(e) = move_entry(&src, &dst) {
            for n in moved.iter().rev() {
                let _ = move_entry(&to.join(n), &from.join(n));
            }
            return Err(e).with_context(|| format!("could not move {} to {}", src.display(), to.display()));
        }
        moved.push(name);
    }
    let _ = std::fs::remove_dir(from); // only succeeds when nothing else is left
    Ok(moved.len())
}

/// On the start of an installed copy, bring data from the pre-11 location (`%APPDATA%\AkulavLauncher`,
/// `~/.local/share/AkulavLauncher`, …) into the install folder, even when that is on another drive.
fn migrate_legacy_data(root: &Path) -> Option<String> {
    let legacy = legacy_data_root()?;
    if same_path(&legacy, root) || !has_data(&legacy) {
        return None;
    }
    match move_data(&legacy, root) {
        Ok(0) => None,
        Ok(_) => Some(format!("Moved launcher data from {} to {}", legacy.display(), root.display())),
        Err(e) => Some(format!("Could not move old launcher data from {} ({e:#}); it was left there", legacy.display())),
    }
}

// ---------------------------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------------------------

/// Installing offered on Linux/macOS when the binary runs from outside its install folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
pub enum Offer {
    #[default]
    None,
    /// Not installed yet: pick a folder.
    Install { suggested: PathBuf },
    /// An older copy is installed in `root` (the remembered folder).
    Update { root: PathBuf, installed: Option<String> },
}

/// Passed from `main` to the UI.
#[derive(Debug, Clone, Default)]
pub struct StartupInfo {
    /// Started right after a fresh install.
    pub installed: bool,
    pub updated_from: Option<String>,
    pub offer: Offer,
    /// Things worth a console line (e.g. data moved).
    pub notes: Vec<String>,
}

pub enum Startup {
    Run(StartupInfo),
    Exit,
}

pub fn on_startup(args: &[String]) -> Startup {
    let flag = |f: &str| args.iter().any(|a| a == f);
    if flag("--portable") {
        std::env::set_var("AKULAV_PORTABLE", "1");
    }
    let Ok(exe) = std::env::current_exe() else { return Startup::Run(StartupInfo::default()) };

    // Leftovers from the previous update.
    let _ = std::fs::remove_file(sibling(&exe, OLD_SUFFIX));
    let _ = std::fs::remove_file(sibling(&exe, NEW_SUFFIX));

    if flag("--uninstall") {
        platform::uninstall_interactive();
        return Startup::Exit;
    }

    let mut info = StartupInfo {
        installed: flag("--installed"),
        updated_from: args
            .iter()
            .position(|a| a == "--updated-from")
            .and_then(|i| args.get(i + 1))
            .filter(|v| !v.trim().is_empty() && !v.starts_with("--"))
            .cloned(),
        ..Default::default()
    };

    if let Some(exit) = platform::before_run(&exe, &info) {
        return exit;
    }

    if let Some(root) = program_root().filter(|r| is_install_root(r)) {
        platform::on_installed_start(&root);
        info.notes.extend(migrate_legacy_data(&root));
    } else if !cfg!(debug_assertions) && !flag("--here") && !portable() {
        info.offer = platform::startup_offer();
    }
    Startup::Run(info)
}

// ---------------------------------------------------------------------------------------------
// Installing (Linux/macOS) and uninstalling
// ---------------------------------------------------------------------------------------------

/// Install this binary into `root` (or update the copy there), then start it with `args`.
/// On success the caller must exit. Windows installs with the setup instead.
pub fn install_and_launch(root: &Path, args: &[&str]) -> Result<()> {
    platform::install_and_launch(root, args)
}

/// Whether the UI asks about deleting data itself (Windows' uninstaller asks on its own).
pub const UNINSTALL_ASKS_ABOUT_DATA: bool = cfg!(windows);

/// Uninstall the running installed copy. On success the caller must exit.
pub fn uninstall(delete_data: bool) -> Result<()> {
    platform::uninstall(delete_data)
}

/// Delete [`DATA_ENTRIES`] in `dir` (never anything else), then `dir` itself if it is empty.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn delete_data(dir: &Path) {
    for name in DATA_ENTRIES {
        let p = dir.join(name);
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    let _ = std::fs::remove_dir(dir);
}

// ---------------------------------------------------------------------------------------------
// Updating
// ---------------------------------------------------------------------------------------------

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut hasher = sha2::Sha256::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Replace `target` with `source` even if `target` is the running executable (both OSes allow
/// renaming a running program). The previous file stays as `.old` until the next start.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn swap_in(source: &Path, target: &Path) -> Result<()> {
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

fn looks_like_executable(bytes: &[u8]) -> bool {
    if cfg!(windows) {
        bytes.starts_with(b"MZ")
    } else if cfg!(target_os = "macos") {
        bytes.len() >= 4 && matches!(&bytes[..4], [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
    } else {
        bytes.starts_with(b"\x7fELF")
    }
}

/// Reject anything that isn't a program for this OS or doesn't match its published SHA-256.
pub fn verify_update(file: &Path, sha256: &str) -> Result<()> {
    let mut head = [0u8; 4];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(file)?;
        let _ = f.read(&mut head)?;
    }
    if !looks_like_executable(&head) {
        let _ = std::fs::remove_file(file);
        bail!("the download is not a launcher program for this system");
    }
    if !sha256_file(file)?.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(file);
        bail!("the download did not match its SHA-256 checksum, so it was deleted");
    }
    Ok(())
}

/// Download and verify this OS's release file. `on_bytes(done, total)` reports progress.
pub async fn download_update(
    client: &reqwest::Client,
    info: &UpdateInfo,
    on_bytes: &(dyn Fn(u64, u64) + Send + Sync),
    cancel: &Cancel,
) -> Result<PathBuf> {
    let staged = platform::staging_path()?;
    let _ = std::fs::remove_file(&staged);
    let total = client.head(&info.url).send().await.ok().and_then(|r| r.content_length()).unwrap_or(0);
    let done = std::sync::atomic::AtomicU64::new(0);
    let cb = |d: i64| {
        use std::sync::atomic::Ordering::Relaxed;
        let now = if d >= 0 { done.fetch_add(d as u64, Relaxed) + d as u64 } else { done.fetch_sub((-d) as u64, Relaxed) - (-d) as u64 };
        on_bytes(now, total.max(now));
    };
    net::download_file(client, &DownloadTask::new(info.url.clone(), staged.clone()), &cb, cancel).await?;
    verify_update(&staged, &info.sha256)?;
    Ok(staged)
}

/// Apply a verified download: Windows runs the setup in update mode (same folder), Linux/macOS
/// swap the binary in place. Either way the launcher restarts; the caller must exit right away.
pub fn apply_update(staged: &Path) -> Result<()> {
    platform::apply_update(staged)
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
    fn program_roots() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(root_of_exe(&p("/opt/akulav/AkulavLauncher")), Some(p("/opt/akulav")));
        assert_eq!(
            root_of_exe(&p("/Users/a/Applications/Akulav Launcher/Akulav Launcher.app/Contents/MacOS/AkulavLauncher")),
            Some(p("/Users/a/Applications/Akulav Launcher"))
        );
        // Not a bundle: a folder that merely happens to be called MacOS.
        assert_eq!(root_of_exe(&p("/x/MacOS/AkulavLauncher")), Some(p("/x/MacOS")));
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
    fn verifies_downloads() {
        let d = tempfile::tempdir().unwrap();
        let bad = d.path().join("bad");
        std::fs::write(&bad, b"<html>404</html>").unwrap();
        assert!(verify_update(&bad, "00").is_err());
        assert!(!bad.exists(), "rejected downloads are deleted");

        let header: &[u8] = if cfg!(windows) {
            b"MZ\x90\x00"
        } else if cfg!(target_os = "macos") {
            &[0xcf, 0xfa, 0xed, 0xfe]
        } else {
            b"\x7fELF"
        };
        let good = d.path().join("good");
        std::fs::write(&good, header).unwrap();
        let sum = sha256_file(&good).unwrap();
        assert!(verify_update(&good, &sum.to_uppercase()).is_ok());
        assert!(verify_update(&good, &"0".repeat(64)).is_err());
        assert!(!good.exists());
    }

    #[test]
    fn moves_data_and_rolls_back() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        std::fs::create_dir_all(old.join("instances/Pack")).unwrap();
        std::fs::write(old.join("settings.json"), b"{}").unwrap();
        std::fs::write(old.join("unrelated.txt"), b"keep").unwrap();
        move_data(&old, &new).unwrap();
        assert!(new.join("settings.json").is_file() && new.join("instances/Pack").is_dir());
        assert!(old.join("unrelated.txt").is_file(), "only launcher data moves");

        // An entry that already exists at the destination is left where it is.
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::create_dir_all(a.join("shared")).unwrap();
        std::fs::write(a.join("settings.json"), b"{}").unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(b.join("settings.json")).unwrap(); // occupied → skipped, not an error
        move_data(&a, &b).unwrap();
        assert!(b.join("shared").is_dir() && a.join("settings.json").is_file());
    }

    #[test]
    fn copies_trees_when_renaming_is_not_possible() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src");
        std::fs::create_dir_all(src.join("a/b")).unwrap();
        std::fs::write(src.join("a/b/f.txt"), b"x").unwrap();
        copy_all(&src, &d.path().join("dst")).unwrap();
        assert_eq!(std::fs::read(d.path().join("dst/a/b/f.txt")).unwrap(), b"x");
        move_entry(&d.path().join("dst"), &d.path().join("moved")).unwrap();
        assert!(d.path().join("moved/a/b/f.txt").is_file() && !d.path().join("dst").exists());
    }

    #[test]
    fn deletes_only_launcher_data() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Games");
        std::fs::create_dir_all(root.join("shared/x")).unwrap();
        std::fs::write(root.join("settings.json"), b"{}").unwrap();
        std::fs::write(root.join("my-notes.txt"), b"mine").unwrap();
        delete_data(&root);
        assert!(!root.join("shared").exists() && !root.join("settings.json").exists());
        assert!(root.join("my-notes.txt").is_file());
    }

    #[test]
    fn install_dir_input() {
        assert!(parse_install_dir("").is_err());
        assert!(parse_install_dir("relative/dir").is_err());
        if cfg!(unix) {
            assert!(parse_install_dir("/").is_err());
            assert_eq!(parse_install_dir(" /opt/akulav ").unwrap(), PathBuf::from("/opt/akulav"));
            assert!(parse_install_dir("~/Games/Akulav").unwrap().ends_with("Games/Akulav"));
        } else {
            assert!(parse_install_dir(r"C:\").is_err());
            assert_eq!(parse_install_dir(r"D:\Games\Akulav").unwrap(), PathBuf::from(r"D:\Games\Akulav"));
        }
    }
}
