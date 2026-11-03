//! The remote modpack list, the local "installed" database and pack installation.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::events::{Cancel, ProgressUnit, Reporter};
use super::net::{self, DownloadTask};
use super::paths::{sanitize_dir_name, write_atomic, Paths};

pub const DEFAULT_MODPACK_LIST_URL: &str = "https://akulav.com/shared/aa7e873115b2344cc291045b";

/// Folders wiped before a pack update so removed mods/configs don't linger
/// (same behaviour as the original launcher).
pub const WIPE_ON_UPDATE: &[&str] = &["mods", "config"];

/// Top-level folders in a pack archive that belong in the shared store rather than the instance.
const SHARED_DIRS: &[&str] = &["versions", "libraries", "assets"];

/// One entry of `modpacks.json`. Field names match the existing file.
/// `Description` is optional and new: add it to the JSON to show a blurb in the launcher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modpack {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
    /// Version id to launch, e.g. `neoforge-21.1.248`, `1.21.1`, `fabric-loader-0.16.9-1.21.1`.
    #[serde(rename = "API")]
    pub api: String,
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "Description", default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPack {
    pub version: String,
    pub api: String,
    pub installed_at: chrono::DateTime<chrono::Utc>,
    /// Version id actually installed for this pack (may differ from `api` for Forge installers).
    #[serde(default)]
    pub resolved_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledDb {
    pub packs: HashMap<String, InstalledPack>,
}

impl InstalledDb {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read(paths.installed_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        write_atomic(&paths.installed_file(), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn status(&self, pack: &Modpack) -> PackStatus {
        match self.packs.get(&pack.name) {
            None => PackStatus::NotInstalled,
            Some(p) if p.version == pack.version => PackStatus::UpToDate,
            Some(p) => PackStatus::UpdateAvailable { installed: p.version.clone() },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackStatus {
    NotInstalled,
    UpToDate,
    UpdateAvailable { installed: String },
}

/// Fetch the modpack list; on success cache it, on failure fall back to the cache.
pub async fn fetch_modpacks(client: &reqwest::Client, url: &str, paths: &Paths) -> (Result<Vec<Modpack>, String>, bool) {
    match net::get_json::<Vec<Modpack>>(client, url).await {
        Ok(list) => {
            if let Ok(bytes) = serde_json::to_vec_pretty(&list) {
                let _ = write_atomic(&paths.modpack_cache_file(), &bytes);
            }
            (Ok(list), false)
        }
        Err(e) => {
            let cached = std::fs::read(paths.modpack_cache_file())
                .ok()
                .and_then(|b| serde_json::from_slice::<Vec<Modpack>>(&b).ok());
            match cached {
                Some(list) if !list.is_empty() => (Ok(list), true),
                _ => (Err(format!("{e:#}")), false),
            }
        }
    }
}

/// Download a pack archive and install it into its instance folder.
pub async fn install_pack(
    client: &reqwest::Client,
    pack: &Modpack,
    paths: &Paths,
    reporter: &Reporter,
    cancel: &Cancel,
) -> Result<()> {
    reporter.stage(format!("Downloading {} {}", pack.name, pack.version));
    let archive = paths.cache.join(format!("{}-{}.zip", sanitize_dir_name(&pack.name), sanitize_dir_name(&pack.version)));
    let _ = std::fs::remove_file(&archive);

    // Unknown size up front: stream and show bytes as they arrive.
    let task = DownloadTask::new(pack.url.clone(), archive.clone());
    let total = client
        .head(&pack.url)
        .send()
        .await
        .ok()
        .and_then(|r| r.content_length())
        .unwrap_or(0);
    let done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let progress_reporter = reporter.clone();
    let last = std::sync::Arc::new(throttle::Throttle::default());
    {
        let done = done.clone();
        let on_bytes = move |delta: i64| {
            use std::sync::atomic::Ordering;
            let now = if delta >= 0 {
                done.fetch_add(delta as u64, Ordering::Relaxed) + delta as u64
            } else {
                done.fetch_sub((-delta) as u64, Ordering::Relaxed) - (-delta) as u64
            };
            if last.ready() {
                progress_reporter.progress(now, total.max(now), ProgressUnit::Bytes);
            }
        };
        net::download_file(client, &task, &on_bytes, cancel)
            .await
            .with_context(|| format!("could not download {}", pack.name))?;
    }
    cancel.check()?;

    reporter.stage(format!("Installing {}", pack.name));
    let instance = paths.instance_dir(&pack.name);
    let shared = paths.shared.clone();
    let reporter2 = reporter.clone();
    let cancel2 = cancel.clone();
    let archive2 = archive.clone();
    let files = tokio::task::spawn_blocking(move || extract_pack(&archive2, &instance, &shared, &reporter2, &cancel2))
        .await??;
    reporter.info(format!("Extracted {files} files"));
    let _ = std::fs::remove_file(&archive);
    Ok(())
}

/// Tiny time-based throttle so byte callbacks don't flood the UI channel.
mod throttle {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    pub struct Throttle(Mutex<Option<Instant>>);

    impl Throttle {
        pub fn ready(&self) -> bool {
            let mut last = self.0.lock().unwrap();
            let now = Instant::now();
            match *last {
                Some(t) if now.duration_since(t) < Duration::from_millis(100) => false,
                _ => {
                    *last = Some(now);
                    true
                }
            }
        }
    }
}

/// Where a file from the archive should go, after stripping any wrapper folder.
fn route_entry(rel: &Path, instance: &Path, shared: &Path) -> PathBuf {
    let first = rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or("");
    if SHARED_DIRS.iter().any(|d| d.eq_ignore_ascii_case(first)) {
        shared.join(rel)
    } else {
        instance.join(rel)
    }
}

/// If every entry sits under one folder that isn't itself a game folder (e.g. `MyPack/mods/...`
/// or `.minecraft/mods/...`), return that folder so it can be stripped.
fn wrapper_prefix(entries: &[(PathBuf, bool)]) -> Option<PathBuf> {
    const GAME_DIRS: &[&str] = &[
        "mods", "config", "versions", "libraries", "assets", "resourcepacks", "shaderpacks", "saves",
        "kubejs", "defaultconfigs", "scripts",
    ];
    let mut root: Option<&std::ffi::OsStr> = None;
    for (name, is_dir) in entries {
        let mut comps = name.components();
        let first = comps.next()?.as_os_str();
        if comps.next().is_none() && !is_dir {
            return None; // a file at the top level means there is no wrapper folder
        }
        match root {
            None => root = Some(first),
            Some(r) if r == first => {}
            Some(_) => return None,
        }
    }
    let root = root?;
    let lower = root.to_string_lossy().to_ascii_lowercase();
    if GAME_DIRS.contains(&lower.as_str()) {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

pub fn extract_pack(archive: &Path, instance: &Path, shared: &Path, reporter: &Reporter, cancel: &Cancel) -> Result<usize> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).context("the downloaded pack is not a valid zip file")?;

    let names: Vec<(PathBuf, bool)> = (0..zip.len())
        .filter_map(|i| zip.by_index_raw(i).ok().and_then(|f| f.enclosed_name().map(|n| (n, f.is_dir()))))
        .collect();
    let prefix = wrapper_prefix(&names);
    if let Some(p) = &prefix {
        reporter.info(format!("Archive is wrapped in '{}', unwrapping", p.display()));
    }

    for dir in WIPE_ON_UPDATE {
        let d = instance.join(dir);
        if d.exists() {
            std::fs::remove_dir_all(&d).with_context(|| format!("could not clear {}", d.display()))?;
        }
    }
    std::fs::create_dir_all(instance)?;

    let total = zip.len() as u64;
    let mut written = 0usize;
    let mut buf = vec![0u8; 256 * 1024];
    for i in 0..zip.len() {
        if i % 64 == 0 {
            cancel.check()?;
            reporter.progress(i as u64, total, ProgressUnit::Items);
        }
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else { continue };
        let rel = match &prefix {
            Some(p) => match name.strip_prefix(p) {
                Ok(r) if r.components().next().is_some() => r.to_path_buf(),
                _ => continue,
            },
            None => name,
        };
        if rel.components().any(|c| matches!(c, Component::ParentDir)) {
            continue;
        }
        let dest = route_entry(&rel, instance, shared);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest).with_context(|| format!("could not write {}", dest.display()))?;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
        written += 1;
    }
    reporter.progress(total, total, ProgressUnit::Items);
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_existing_modpacks_json() {
        let json = r#"[
          {
            "Name": "GoldenEra - Ultimate",
            "Version": "7.2.0",
            "API": "neoforge-21.1.248",
            "URL": "https://akulav.com/shared/77a416c8106b356988b3f49b"
          }
        ]"#;
        let packs: Vec<Modpack> = serde_json::from_str(json).unwrap();
        assert_eq!(packs[0].api, "neoforge-21.1.248");
        assert_eq!(packs[0].description, None);
    }

    #[test]
    fn detects_wrapper_folder() {
        let p = |s: &str| PathBuf::from(s);
        let f = |s: &str| (p(s), false);
        assert_eq!(wrapper_prefix(&[f("mods/a.jar"), f("config/b.toml")]), None);
        assert_eq!(
            wrapper_prefix(&[(p("Pack"), true), f("Pack/mods/a.jar"), f("Pack/config/b.toml")]),
            Some(p("Pack"))
        );
        assert_eq!(wrapper_prefix(&[f(".minecraft/mods/a.jar")]), Some(p(".minecraft")));
        assert_eq!(wrapper_prefix(&[f("mods/a.jar")]), None);
        assert_eq!(wrapper_prefix(&[f("Pack/mods/a.jar"), f("readme.txt")]), None);
    }

    #[test]
    fn routes_shared_folders() {
        let i = Path::new("/inst");
        let s = Path::new("/shared");
        assert_eq!(route_entry(Path::new("mods/a.jar"), i, s), PathBuf::from("/inst/mods/a.jar"));
        assert_eq!(route_entry(Path::new("versions/x/x.json"), i, s), PathBuf::from("/shared/versions/x/x.json"));
        assert_eq!(route_entry(Path::new("libraries/a/b.jar"), i, s), PathBuf::from("/shared/libraries/a/b.jar"));
    }

    #[test]
    fn extracts_zip_and_wipes_mods() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("p.zip");
        {
            let f = std::fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("Pack/mods/new.jar", o).unwrap();
            w.write_all(b"new").unwrap();
            w.start_file("Pack/versions/neo/neo.json", o).unwrap();
            w.write_all(b"{}").unwrap();
            w.start_file("Pack/options.txt", o).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let inst = dir.path().join("inst");
        let shared = dir.path().join("shared");
        std::fs::create_dir_all(inst.join("mods")).unwrap();
        std::fs::write(inst.join("mods/old.jar"), b"old").unwrap();
        std::fs::create_dir_all(inst.join("saves/w")).unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let rep = Reporter::new(tx, std::sync::Arc::new(|| {}));
        let n = extract_pack(&zip_path, &inst, &shared, &rep, &Cancel::default()).unwrap();
        assert_eq!(n, 3);
        assert!(!inst.join("mods/old.jar").exists());
        assert!(inst.join("mods/new.jar").exists());
        assert!(inst.join("options.txt").exists());
        assert!(inst.join("saves/w").exists(), "saves must survive updates");
        assert!(shared.join("versions/neo/neo.json").exists());
    }
}
