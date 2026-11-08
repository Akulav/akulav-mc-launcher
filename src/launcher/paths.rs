//! Filesystem layout of the launcher.
//!
//! Everything lives under one data folder: the install folder the user chose (see
//! [`super::install::data_root`]), next to the program itself:
//!
//! ```text
//! AkulavLauncher/
//!   settings.json          launcher + per-pack settings
//!   installed.json         which pack versions are installed
//!   modpacks_cache.json    last good modpack list (offline fallback)
//!   shared/                versions, libraries, assets, Java runtimes (shared by all packs)
//!   instances/<pack>/      game directory of each pack (mods, config, saves, ...)
//!   cache/                 temporary downloads
//!   logs/                  launcher logs
//! ```

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub shared: PathBuf,
    pub instances: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

impl Paths {
    /// `AKULAV_DATA_DIR` overrides the location (portable installs, testing).
    pub fn detect() -> anyhow::Result<Self> {
        if let Some(dir) = std::env::var_os("AKULAV_DATA_DIR").filter(|d| !d.is_empty()) {
            return Ok(Self::at(dir.into()));
        }
        let root = super::install::data_root().ok_or_else(|| anyhow::anyhow!("could not locate the user data folder"))?;
        Ok(Self::at(root))
    }

    pub fn at(root: PathBuf) -> Self {
        Self {
            shared: root.join("shared"),
            instances: root.join("instances"),
            cache: root.join("cache"),
            logs: root.join("logs"),
            root,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [&self.root, &self.shared, &self.instances, &self.cache, &self.logs] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn installed_file(&self) -> PathBuf {
        self.root.join("installed.json")
    }
    pub fn modpack_cache_file(&self) -> PathBuf {
        self.root.join("modpacks_cache.json")
    }
    pub fn versions(&self) -> PathBuf {
        self.shared.join("versions")
    }
    pub fn libraries(&self) -> PathBuf {
        self.shared.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.shared.join("assets")
    }
    pub fn runtimes(&self) -> PathBuf {
        self.shared.join("runtime")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn instance_dir(&self, pack_name: &str) -> PathBuf {
        self.instances.join(sanitize_dir_name(pack_name))
    }
}

/// Make a pack name safe to use as a folder name on every OS.
pub fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        "pack".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Write a file atomically (write to a temp file, then rename over the target).
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_dir_name("GoldenEra - Ultimate"), "GoldenEra - Ultimate");
        assert_eq!(sanitize_dir_name("a:b/c?"), "a_b_c_");
        assert_eq!(sanitize_dir_name("  ..  "), "pack");
        assert_eq!(sanitize_dir_name("Pack."), "Pack");
    }
}
