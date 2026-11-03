//! Downloads the exact Java runtime Mojang ships for each Minecraft version.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::launcher::events::{Cancel, Reporter};
use crate::launcher::net::{self, DownloadTask};
use crate::launcher::paths::Paths;

const RUNTIME_INDEX: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: RuntimeManifestRef,
    #[serde(default)]
    version: Option<RuntimeVersion>,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifestRef {
    url: String,
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
struct RuntimeFile {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    executable: bool,
    #[serde(default)]
    downloads: Option<RuntimeDownloads>,
    #[serde(default)]
    #[cfg_attr(not(unix), allow(dead_code))]
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeDownloads {
    raw: RawDownload,
}

#[derive(Debug, Deserialize)]
struct RawDownload {
    url: String,
    sha1: String,
    size: u64,
}

/// Mojang's name for the current platform in the runtime index.
pub fn platform_key() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", _) => "windows-x64",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("linux", "x86") => "linux-i386",
        // Mojang ships no ARM Linux runtime; this name makes the lookup fail with a clear
        // "set a custom Java path" message instead of downloading x86 binaries.
        ("linux", "aarch64") => "linux-arm64",
        _ => "linux",
    }
}

/// Path of the `java` executable inside an installed runtime folder.
pub fn java_executable(runtime_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        runtime_dir.join("jre.bundle/Contents/Home/bin/java")
    } else if cfg!(windows) {
        // java.exe (not javaw.exe) so stdout/stderr can be captured; the console window
        // is suppressed with CREATE_NO_WINDOW when spawning.
        runtime_dir.join("bin").join("java.exe")
    } else {
        runtime_dir.join("bin").join("java")
    }
}

/// Make sure runtime `component` is installed and return its java executable.
pub async fn ensure_runtime(
    client: &reqwest::Client,
    paths: &Paths,
    component: &str,
    concurrency: usize,
    verify: bool,
    reporter: &Reporter,
    cancel: &Cancel,
) -> Result<PathBuf> {
    let dir = paths.runtimes().join(component);
    let marker = dir.join(".akulav-installed");
    let java = java_executable(&dir);
    if !verify && marker.is_file() && java.is_file() {
        return Ok(java);
    }

    reporter.stage(format!("Preparing Java ({component})"));
    let index: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        net::get_json(client, RUNTIME_INDEX).await.context("could not reach Mojang's Java runtime list")?;
    let entry = index
        .get(platform_key())
        .and_then(|p| p.get(component))
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow!("Mojang doesn't provide Java runtime '{component}' for {}. Set a custom Java path in Settings.", platform_key()))?;
    if let Some(v) = &entry.version {
        reporter.info(format!("Java runtime {component} → {}", v.name));
    }

    let manifest_text = net::get_text(client, &entry.manifest.url).await?;
    if let Some(expected) = &entry.manifest.sha1 {
        use sha1::Digest;
        let actual = hex::encode(sha1::Sha1::digest(manifest_text.as_bytes()));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(anyhow!("Java runtime manifest checksum mismatch"));
        }
    }
    let manifest: RuntimeManifest = serde_json::from_str(&manifest_text)?;

    let mut tasks = Vec::new();
    #[cfg_attr(not(unix), allow(unused_mut, unused_variables))]
    let mut links = Vec::new();
    for (rel, f) in &manifest.files {
        let path = dir.join(rel);
        match f.kind.as_str() {
            "directory" => std::fs::create_dir_all(&path)?,
            "file" => {
                if let Some(d) = &f.downloads {
                    let mut t = DownloadTask::new(d.raw.url.clone(), path).sha1(Some(d.raw.sha1.clone())).size(Some(d.raw.size));
                    t.executable = f.executable;
                    tasks.push(t);
                }
            }
            "link" => links.push((path, f.target.clone())),
            _ => {}
        }
    }
    net::download_all(client, tasks, concurrency, verify, reporter, cancel).await?;

    #[cfg(unix)]
    for (path, target) in links {
        if let Some(target) = target {
            let _ = std::fs::remove_file(&path);
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p)?;
            }
            let _ = std::os::unix::fs::symlink(target, &path);
        }
    }

    if !java.is_file() {
        return Err(anyhow!("Java runtime installed but {} is missing", java.display()));
    }
    std::fs::write(&marker, entry.manifest.sha1.clone().unwrap_or_default())?;
    Ok(java)
}
