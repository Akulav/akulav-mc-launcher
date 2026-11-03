//! Downloading and verifying everything a version needs: version JSONs, libraries,
//! the client jar, assets and native libraries.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use tokio::sync::OnceCell;

use super::version::{merge, Env, LibraryFile, ResolvedVersion, VersionJson};
use crate::launcher::events::{Cancel, Reporter};
use crate::launcher::net::{self, DownloadTask};
use crate::launcher::paths::Paths;

pub const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES: &str = "https://resources.download.minecraft.net/";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub struct GameInstaller<'a> {
    pub client: &'a reqwest::Client,
    pub paths: &'a Paths,
    pub reporter: &'a Reporter,
    pub cancel: &'a Cancel,
    pub concurrency: usize,
    /// Re-hash every existing file instead of only checking sizes.
    pub verify: bool,
    manifest: OnceCell<Manifest>,
}

impl<'a> GameInstaller<'a> {
    pub fn new(client: &'a reqwest::Client, paths: &'a Paths, reporter: &'a Reporter, cancel: &'a Cancel, concurrency: usize, verify: bool) -> Self {
        Self { client, paths, reporter, cancel, concurrency, verify, manifest: OnceCell::new() }
    }

    pub async fn manifest(&self) -> Result<&Manifest> {
        self.manifest
            .get_or_try_init(|| async {
                net::get_json::<Manifest>(self.client, VERSION_MANIFEST)
                    .await
                    .context("could not reach Mojang's version list")
            })
            .await
    }

    pub async fn is_vanilla_version(&self, id: &str) -> bool {
        match self.manifest().await {
            Ok(m) => m.versions.iter().any(|v| v.id == id),
            Err(_) => false,
        }
    }

    /// Read a version JSON from disk, downloading it first if it's a vanilla version.
    pub async fn load_version_json(&self, id: &str) -> Result<VersionJson> {
        let path = self.paths.version_json(id);
        if path.is_file() {
            let bytes = std::fs::read(&path)?;
            return serde_json::from_slice(&bytes).with_context(|| format!("{} is not a valid version file", path.display()));
        }
        let manifest = self.manifest().await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| anyhow!("version '{id}' is not installed and is not a Minecraft release"))?;
        let task = DownloadTask::new(entry.url.clone(), path.clone()).sha1(entry.sha1.clone());
        net::download_file(self.client, &task, &|_| {}, self.cancel).await?;
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Load `id` and merge its whole `inheritsFrom` chain.
    pub async fn resolve(&self, id: &str) -> Result<ResolvedVersion> {
        let mut chain = vec![id.to_string()];
        let mut current = self.load_version_json(id).await?;
        // The jar comes from the deepest version that defines a client download.
        let mut jar_source = current.downloads.as_ref().and_then(|d| d.client.as_ref()).map(|_| current.id.clone());
        while let Some(parent_id) = current.inherits_from.clone() {
            if chain.contains(&parent_id) || chain.len() > 8 {
                bail!("version inheritance loop at '{parent_id}'");
            }
            let parent = self.load_version_json(&parent_id).await?;
            if jar_source.is_none() && parent.downloads.as_ref().and_then(|d| d.client.as_ref()).is_some() {
                jar_source = Some(parent.id.clone());
            }
            chain.push(parent_id);
            current = merge(current, parent);
        }
        let jar_source = current.jar.clone().or(jar_source).unwrap_or_else(|| id.to_string());
        Ok(ResolvedVersion { json: current, chain, jar_source })
    }

    pub fn library_files(rv: &ResolvedVersion, env: &Env) -> Vec<LibraryFile> {
        rv.json.libraries.iter().flat_map(|l| l.files(env)).collect()
    }

    /// Libraries with no download URL that aren't on disk (normally produced by a loader installer).
    pub fn missing_local_libraries(&self, rv: &ResolvedVersion, env: &Env) -> Vec<String> {
        Self::library_files(rv, env)
            .into_iter()
            .filter(|f| f.url.is_none())
            .filter(|f| !self.paths.libraries().join(&f.path).is_file())
            .map(|f| f.path)
            .collect()
    }

    /// Download libraries, the client jar and assets.
    pub async fn install_files(&self, rv: &ResolvedVersion, env: &Env) -> Result<()> {
        let libs_dir = self.paths.libraries();
        let mut tasks = Vec::new();
        for f in Self::library_files(rv, env) {
            if let Some(url) = &f.url {
                tasks.push(DownloadTask::new(url.clone(), libs_dir.join(&f.path)).sha1(f.sha1.clone()).size(f.size));
            }
        }
        // Client jar
        let jar_owner = self.load_version_json(&rv.jar_source).await.ok();
        if let Some(client) = jar_owner.as_ref().and_then(|v| v.downloads.as_ref()).and_then(|d| d.client.as_ref()) {
            if let Some(url) = &client.url {
                tasks.push(
                    DownloadTask::new(url.clone(), self.paths.version_jar(&rv.jar_source))
                        .sha1(client.sha1.clone())
                        .size(client.size),
                );
            }
        }

        self.reporter.stage("Checking libraries");
        let n = net::download_all(self.client, tasks, self.concurrency, self.verify, self.reporter, self.cancel).await?;
        if n > 0 {
            self.reporter.info(format!("Downloaded {n} libraries"));
        }

        let missing = self.missing_local_libraries(rv, env);
        if !missing.is_empty() {
            bail!(
                "{} required libraries are missing and can't be downloaded (for example {}). The mod loader may not be fully installed — try Repair.",
                missing.len(),
                missing[0]
            );
        }

        self.install_assets(rv).await
    }

    async fn install_assets(&self, rv: &ResolvedVersion) -> Result<()> {
        let Some(index_ref) = rv.json.asset_index.clone() else {
            self.reporter.warn("This version has no asset index; skipping assets");
            return Ok(());
        };
        let assets = self.paths.assets();
        let index_path = assets.join("indexes").join(format!("{}.json", index_ref.id));
        let index_task = DownloadTask::new(index_ref.url.clone(), index_path.clone()).sha1(index_ref.sha1.clone()).size(index_ref.size);
        if net::needs_download(&index_task, true) {
            net::download_file(self.client, &index_task, &|_| {}, self.cancel).await?;
        }
        let index: AssetIndex = serde_json::from_slice(&std::fs::read(&index_path)?).context("invalid asset index")?;

        let tasks: Vec<DownloadTask> = index
            .objects
            .values()
            .map(|o| {
                let prefix = &o.hash[..2.min(o.hash.len())];
                DownloadTask::new(format!("{RESOURCES}{prefix}/{}", o.hash), assets.join("objects").join(prefix).join(&o.hash))
                    .sha1(Some(o.hash.clone()))
                    .size(Some(o.size))
            })
            .collect();

        self.reporter.stage("Checking assets");
        let n = net::download_all(self.client, tasks, self.concurrency.max(8) * 2, self.verify, self.reporter, self.cancel).await?;
        if n > 0 {
            self.reporter.info(format!("Downloaded {n} assets"));
        }

        // Very old versions read assets from a flat folder.
        if index.is_virtual || index.map_to_resources {
            let target = if index.is_virtual { assets.join("virtual").join(&index_ref.id) } else { assets.join("virtual").join("legacy") };
            for (name, o) in &index.objects {
                let src = assets.join("objects").join(&o.hash[..2]).join(&o.hash);
                let dst = target.join(name);
                if !dst.is_file() {
                    if let Some(p) = dst.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::copy(&src, &dst)?;
                }
            }
        }
        Ok(())
    }

    /// The jar that goes on the classpath. Like the official launcher, modded versions get a copy
    /// of the vanilla jar under their own id (Forge/NeoForge ignore-lists refer to it by that name).
    pub fn launch_jar(&self, rv: &ResolvedVersion) -> Result<PathBuf> {
        let source = self.paths.version_jar(&rv.jar_source);
        if rv.json.id == rv.jar_source {
            return Ok(source);
        }
        let target = self.paths.version_jar(&rv.json.id);
        let src_len = std::fs::metadata(&source).with_context(|| format!("client jar {} is missing", source.display()))?.len();
        let up_to_date = std::fs::metadata(&target).map(|m| m.len() == src_len).unwrap_or(false);
        if !up_to_date {
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::copy(&source, &target)?;
        }
        Ok(target)
    }

    /// Extract native libraries (.dll/.so/.dylib) into a fresh folder.
    pub fn extract_natives(&self, rv: &ResolvedVersion, env: &Env, dir: &Path) -> Result<usize> {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(dir);
        }
        std::fs::create_dir_all(dir)?;
        let mut count = 0;
        for f in Self::library_files(rv, env).into_iter().filter(|f| f.natives) {
            let jar = self.paths.libraries().join(&f.path);
            count += extract_native_jar(&jar, dir, &f.exclude).with_context(|| format!("extracting natives from {}", jar.display()))?;
        }
        Ok(count)
    }
}

const NATIVE_EXTS: &[&str] = &["dll", "so", "dylib", "jnilib"];

fn extract_native_jar(jar: &Path, dir: &Path, exclude: &[String]) -> Result<usize> {
    let file = std::fs::File::open(jar)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))?;
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name.starts_with("META-INF/") || exclude.iter().any(|e| name.starts_with(e.as_str())) {
            continue;
        }
        let file_name = name.rsplit('/').next().unwrap_or(&name).to_string();
        let ext = file_name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !NATIVE_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        std::fs::write(dir.join(&file_name), buf)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_only_native_files() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("n.jar");
        {
            let f = std::fs::File::create(&jar).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default();
            for (name, data) in [("META-INF/x.dll", "no"), ("windows/x64/org/lwjgl/lwjgl.dll", "yes"), ("org/lwjgl/Foo.class", "no"), ("liblwjgl.so", "yes")] {
                w.start_file(name, o).unwrap();
                w.write_all(data.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        let out = dir.path().join("natives");
        std::fs::create_dir_all(&out).unwrap();
        let n = extract_native_jar(&jar, &out, &["META-INF/".into()]).unwrap();
        assert_eq!(n, 2);
        assert!(out.join("lwjgl.dll").is_file());
        assert!(out.join("liblwjgl.so").is_file());
    }
}
