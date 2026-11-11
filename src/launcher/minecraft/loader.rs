//! Mod loader detection and automatic installation (NeoForge, Forge, Fabric, Quilt).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use super::install::GameInstaller;
use super::java;
use super::version::Env;
use crate::launcher::events::{Cancel, LogSource, OutputClassifier, Reporter};
use crate::launcher::net::{self, DownloadTask};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderSpec {
    Vanilla(String),
    NeoForge { version: String },
    Forge { mc: String, forge: String },
    Fabric { loader: String, mc: String },
    Quilt { loader: String, mc: String },
}

impl LoaderSpec {
    /// Interpret the `API` field of a modpack.
    pub fn parse(api: &str) -> Self {
        let api = api.trim();
        let lower = api.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("neoforge-") {
            return LoaderSpec::NeoForge { version: api[api.len() - v.len()..].to_string() };
        }
        if let Some(rest) = lower.strip_prefix("fabric-loader-") {
            if let Some((loader, mc)) = rest.split_once('-') {
                return LoaderSpec::Fabric { loader: loader.into(), mc: api[api.len() - mc.len()..].to_string() };
            }
        }
        if let Some(rest) = lower.strip_prefix("quilt-loader-") {
            if let Some((loader, mc)) = rest.split_once('-') {
                return LoaderSpec::Quilt { loader: loader.into(), mc: api[api.len() - mc.len()..].to_string() };
            }
        }
        // "1.20.1-forge-47.3.0" (official launcher id) or "forge-1.20.1-47.3.0"
        if let Some(idx) = lower.find("-forge-") {
            return LoaderSpec::Forge { mc: api[..idx].to_string(), forge: api[idx + 7..].to_string() };
        }
        if let Some(rest) = lower.strip_prefix("forge-") {
            if let Some((mc, forge)) = rest.split_once('-') {
                return LoaderSpec::Forge { mc: mc.into(), forge: api[api.len() - forge.len()..].to_string() };
            }
        }
        LoaderSpec::Vanilla(api.to_string())
    }

    pub fn display(&self) -> String {
        match self {
            LoaderSpec::Vanilla(v) => format!("Minecraft {v}"),
            LoaderSpec::NeoForge { version } => format!("NeoForge {version}"),
            LoaderSpec::Forge { forge, .. } => format!("Forge {forge}"),
            LoaderSpec::Fabric { loader, .. } => format!("Fabric {loader}"),
            LoaderSpec::Quilt { loader, .. } => format!("Quilt {loader}"),
        }
    }

    /// Minecraft version this loader targets, when it can be derived from the id.
    pub fn minecraft_version(&self) -> Option<String> {
        match self {
            LoaderSpec::Vanilla(v) => Some(v.clone()),
            LoaderSpec::Forge { mc, .. } | LoaderSpec::Fabric { mc, .. } | LoaderSpec::Quilt { mc, .. } => Some(mc.clone()),
            LoaderSpec::NeoForge { version } => neoforge_mc_version(version),
        }
    }

    pub fn needs_installer(&self) -> bool {
        matches!(self, LoaderSpec::NeoForge { .. } | LoaderSpec::Forge { .. })
    }
}

/// NeoForge versions encode the Minecraft version: 21.1.x → 1.21.1, 21.0.x → 1.21,
/// and from 2026 on year-based versions: 26.1.0.x → 26.1, 26.1.2.x → 26.1.2.
pub fn neoforge_mc_version(version: &str) -> Option<String> {
    let base = version.split(['-', '+']).next()?;
    let parts: Vec<u32> = base.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let (a, b) = (*parts.first()?, *parts.get(1)?);
    if a < 25 {
        Some(if b == 0 { format!("1.{a}") } else { format!("1.{a}.{b}") })
    } else {
        match parts.get(2) {
            Some(&c) if parts.len() >= 4 && c != 0 => Some(format!("{a}.{b}.{c}")),
            _ => Some(format!("{a}.{b}")),
        }
    }
}

pub fn installer_url(spec: &LoaderSpec) -> Option<String> {
    match spec {
        LoaderSpec::NeoForge { version } => Some(format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
        )),
        LoaderSpec::Forge { mc, forge } => {
            let full = format!("{mc}-{forge}");
            Some(format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"))
        }
        _ => None,
    }
}

/// Install the loader described by `spec` into the shared store and return the version id to launch.
pub async fn install_loader(
    gi: &GameInstaller<'_>,
    spec: &LoaderSpec,
    api: &str,
    env: &Env,
    java_override: Option<&Path>,
) -> Result<String> {
    let paths = gi.paths;
    let reporter = gi.reporter;
    match spec {
        LoaderSpec::Vanilla(id) => Ok(id.clone()),
        LoaderSpec::Fabric { loader, mc } | LoaderSpec::Quilt { loader, mc } => {
            let (name, url) = match spec {
                LoaderSpec::Fabric { .. } => ("Fabric", format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/profile/json")),
                _ => ("Quilt", format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{loader}/profile/json")),
            };
            reporter.stage(format!("Installing {name} {loader}"));
            let text = net::get_text(gi.client, &url).await.with_context(|| format!("could not fetch the {name} profile"))?;
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let id = value.get("id").and_then(|v| v.as_str()).unwrap_or(api).to_string();
            let path = paths.version_json(&id);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, text)?;
            Ok(id)
        }
        LoaderSpec::NeoForge { .. } | LoaderSpec::Forge { .. } => {
            let url = installer_url(spec).unwrap();
            let display = spec.display();

            // The installer patches the vanilla jar, so install the base game first.
            let mc = spec.minecraft_version();
            let mut component = "java-runtime-delta".to_string();
            if let Some(mc) = mc.as_deref() {
                if gi.is_vanilla_version(mc).await {
                    reporter.stage(format!("Installing Minecraft {mc}"));
                    let base = gi.resolve(mc).await?;
                    gi.install_files(&base, env).await?;
                    if let Some(j) = &base.json.java_version {
                        component = j.component.clone();
                    }
                }
            }
            let java = match java_override {
                Some(j) => j.to_path_buf(),
                None => java::ensure_runtime(gi.client, paths, &component, gi.concurrency, false, reporter, gi.cancel).await?,
            };

            reporter.stage(format!("Downloading {display} installer"));
            let installer = paths.cache.join(url.rsplit('/').next().unwrap_or("installer.jar"));
            net::download_file(gi.client, &DownloadTask::new(url.clone(), installer.clone()), &|_| {}, gi.cancel)
                .await
                .with_context(|| format!("could not download the {display} installer"))?;

            // Forge/NeoForge installers refuse to run without a launcher profile file.
            let profiles = paths.shared.join("launcher_profiles.json");
            if !profiles.is_file() {
                std::fs::write(&profiles, r#"{"profiles":{},"selectedProfile":null,"clientToken":"akulav"}"#)?;
            }

            reporter.stage(format!("Running {display} installer"));
            let target = paths.shared.to_string_lossy().to_string();
            std::fs::create_dir_all(paths.java_tmp())?;
            let tmpdir = format!("-Djava.io.tmpdir={}", paths.java_tmp().to_string_lossy());
            let mut ok = false;
            let mut last_output = String::new();
            for flag in ["--installClient", "--install-client"] {
                let args = vec![tmpdir.clone(), "-jar".to_string(), installer.to_string_lossy().to_string(), flag.to_string(), target.clone()];
                let (code, tail) = run_logged(&java, &args, &paths.cache, reporter, gi.cancel, LogSource::Installer).await?;
                last_output = tail;
                if code == 0 {
                    ok = true;
                    break;
                }
                if !last_output.to_ascii_lowercase().contains("recognized option") && !last_output.contains("UnrecognizedOption") {
                    break;
                }
            }
            let _ = std::fs::remove_file(&installer);
            if !ok {
                bail!("the {display} installer failed. Last output:\n{last_output}");
            }
            find_installed_id(&paths.versions(), api, spec)
                .ok_or_else(|| anyhow!("the {display} installer finished but no matching version was found"))
        }
    }
}

/// After a Forge-style installer runs, find the id it created.
fn find_installed_id(versions: &Path, api: &str, spec: &LoaderSpec) -> Option<String> {
    if versions.join(api).join(format!("{api}.json")).is_file() {
        return Some(api.to_string());
    }
    let needle = match spec {
        LoaderSpec::NeoForge { version } => version.clone(),
        LoaderSpec::Forge { forge, .. } => forge.clone(),
        _ => api.to_string(),
    };
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for entry in std::fs::read_dir(versions).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains(&needle) || !entry.path().join(format!("{name}.json")).is_file() {
            continue;
        }
        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, name));
        }
    }
    best.map(|(_, n)| n)
}

pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Run a Java process, streaming its output to the console. Returns the exit code and
/// the last lines of output (for error messages).
pub async fn run_logged(
    java: &Path,
    args: &[String],
    cwd: &Path,
    reporter: &Reporter,
    cancel: &Cancel,
    source: LogSource,
) -> Result<(i32, String)> {
    let java = java.to_path_buf();
    let args = args.to_vec();
    let cwd: PathBuf = cwd.to_path_buf();
    let reporter = reporter.clone();
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || -> Result<(i32, String)> {
        let mut cmd = Command::new(&java);
        cmd.args(&args).current_dir(&cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        hide_console(&mut cmd);
        let mut child = cmd.spawn().with_context(|| format!("could not start {}", java.display()))?;
        let tail = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::<String>::new()));
        let mut readers = Vec::new();
        for (stream, is_err) in [
            (child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), false),
            (child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), true),
        ] {
            let Some(stream) = stream else { continue };
            let reporter = reporter.clone();
            let tail = tail.clone();
            readers.push(std::thread::spawn(move || {
                let mut classify = OutputClassifier::new(source, is_err);
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    {
                        let mut t = tail.lock().unwrap();
                        t.push_back(line.clone());
                        if t.len() > 15 {
                            t.pop_front();
                        }
                    }
                    reporter.log(classify.line(&line));
                }
            }));
        }
        let status = loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::launcher::events::Cancelled.into());
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        for r in readers {
            let _ = r.join();
        }
        let tail = tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
        Ok((status.code().unwrap_or(-1), tail))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_strings() {
        assert_eq!(LoaderSpec::parse("neoforge-21.1.248"), LoaderSpec::NeoForge { version: "21.1.248".into() });
        assert_eq!(LoaderSpec::parse("1.21.1"), LoaderSpec::Vanilla("1.21.1".into()));
        assert_eq!(
            LoaderSpec::parse("1.20.1-forge-47.3.0"),
            LoaderSpec::Forge { mc: "1.20.1".into(), forge: "47.3.0".into() }
        );
        assert_eq!(
            LoaderSpec::parse("forge-1.20.1-47.3.0"),
            LoaderSpec::Forge { mc: "1.20.1".into(), forge: "47.3.0".into() }
        );
        assert_eq!(
            LoaderSpec::parse("fabric-loader-0.16.9-1.21.1"),
            LoaderSpec::Fabric { loader: "0.16.9".into(), mc: "1.21.1".into() }
        );
        assert_eq!(
            LoaderSpec::parse("quilt-loader-0.27.1-1.20.4"),
            LoaderSpec::Quilt { loader: "0.27.1".into(), mc: "1.20.4".into() }
        );
    }

    #[test]
    fn neoforge_versions_map_to_minecraft() {
        assert_eq!(neoforge_mc_version("21.1.248").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_mc_version("21.0.167").as_deref(), Some("1.21"));
        assert_eq!(neoforge_mc_version("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_mc_version("21.4.10-beta").as_deref(), Some("1.21.4"));
        assert_eq!(neoforge_mc_version("26.1.0.5").as_deref(), Some("26.1"));
        assert_eq!(neoforge_mc_version("garbage"), None);
    }

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(&LoaderSpec::parse("neoforge-21.1.248")).unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-installer.jar"
        );
        assert_eq!(
            installer_url(&LoaderSpec::parse("1.20.1-forge-47.3.0")).unwrap(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar"
        );
    }

    #[test]
    fn finds_installed_versions() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path();
        std::fs::create_dir_all(v.join("1.20.1-forge-47.3.0")).unwrap();
        std::fs::write(v.join("1.20.1-forge-47.3.0/1.20.1-forge-47.3.0.json"), "{}").unwrap();
        let spec = LoaderSpec::parse("forge-1.20.1-47.3.0");
        assert_eq!(find_installed_id(v, "forge-1.20.1-47.3.0", &spec).as_deref(), Some("1.20.1-forge-47.3.0"));
    }
}
