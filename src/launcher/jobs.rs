//! Long-running jobs: preparing and launching a pack, repairing it, loading the pack list.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};

use super::events::{is_cancelled_error, Cancel, Event, LogSource, OutputClassifier, Reporter};
use super::minecraft::install::GameInstaller;
use super::minecraft::java;
use super::minecraft::launch::{build_arguments, LaunchOptions};
use super::minecraft::loader::{hide_console, install_loader, LoaderSpec};
use super::minecraft::version::Env;
use super::modpacks::{install_pack, InstalledDb, InstalledPack, Modpack, PackStatus};
use super::net;
use super::paths::Paths;
use super::settings::PackSettings;

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub pack: Modpack,
    pub username: String,
    pub settings: PackSettings,
    pub java_override: Option<PathBuf>,
    pub concurrency: usize,
    /// Re-download the pack and re-verify every file.
    pub repair: bool,
    /// Start the game when preparation succeeds (false = repair only).
    pub launch: bool,
}

struct Prepared {
    java: PathBuf,
    args: Vec<String>,
    game_dir: PathBuf,
}

/// Run a launch/repair job on the runtime. Progress and results arrive as [`Event`]s.
pub fn spawn_job(rt: &tokio::runtime::Handle, req: LaunchRequest, paths: Paths, reporter: Reporter, cancel: Cancel) {
    rt.spawn(async move {
        let started = Instant::now();
        let result = prepare(&req, &paths, &reporter, &cancel).await;
        reporter.clear_progress();
        match result {
            Err(e) => {
                let cancelled = is_cancelled_error(&e);
                if cancelled {
                    reporter.warn("Cancelled");
                } else {
                    reporter.error(format!("{e:#}"));
                }
                reporter.send(Event::JobFailed { message: friendly_error(&e), cancelled });
            }
            Ok(prepared) if !req.launch => {
                let _ = prepared;
                let verb = if req.repair { "repaired" } else { "is ready" };
                reporter.send(Event::JobDone { message: format!("{} {verb} ({:.0}s)", req.pack.name, started.elapsed().as_secs_f32()) });
            }
            Ok(prepared) => {
                let reporter2 = reporter.clone();
                let _ = tokio::task::spawn_blocking(move || run_game(prepared, &reporter2)).await;
            }
        }
    });
}

fn friendly_error(e: &anyhow::Error) -> String {
    let text = format!("{e:#}");
    let lower = text.to_ascii_lowercase();
    if lower.contains("dns") || lower.contains("connect") || lower.contains("timed out") {
        format!("Network problem — check your internet connection.\n{text}")
    } else {
        text
    }
}

async fn prepare(req: &LaunchRequest, paths: &Paths, reporter: &Reporter, cancel: &Cancel) -> Result<Prepared> {
    paths.ensure()?;
    let client = net::client()?;
    let pack = &req.pack;
    let mut db = InstalledDb::load(paths);

    // 1. Modpack files
    let status = db.status(pack);
    if req.repair || status != PackStatus::UpToDate {
        match install_pack(&client, pack, paths, reporter, cancel).await {
            Ok(()) => {
                let previous = db.packs.get(&pack.name).cloned();
                db.packs.insert(
                    pack.name.clone(),
                    InstalledPack {
                        version: pack.version.clone(),
                        api: pack.api.clone(),
                        installed_at: chrono::Utc::now(),
                        // Keep the resolved loader id only if the loader didn't change.
                        resolved_version: previous.filter(|p| p.api == pack.api).and_then(|p| p.resolved_version),
                    },
                );
                db.save(paths)?;
                reporter.send(Event::InstalledChanged);
            }
            Err(e) if !is_cancelled_error(&e) && matches!(status, PackStatus::UpdateAvailable { .. }) && !req.repair => {
                reporter.warn(format!("Could not update the pack ({e:#}). Launching the installed version."));
            }
            Err(e) => return Err(e),
        }
    }
    cancel.check()?;

    // 2. Game version / mod loader
    let custom_res = req.settings.custom_resolution;
    let env = Env::current().with_feature("has_custom_resolution", custom_res);
    let gi = GameInstaller::new(&client, paths, reporter, cancel, req.concurrency, req.repair);
    let spec = LoaderSpec::parse(&pack.api);
    let mut version_id = db.packs.get(&pack.name).and_then(|p| p.resolved_version.clone()).unwrap_or_else(|| pack.api.clone());
    let java_override = req.java_override.as_deref().filter(|p| !p.as_os_str().is_empty());

    if !paths.version_json(&version_id).is_file() && !gi.is_vanilla_version(&version_id).await {
        reporter.info(format!("{} is not installed yet", spec.display()));
        version_id = install_loader(&gi, &spec, &pack.api, &env, java_override).await?;
        remember_resolved(paths, &pack.name, &version_id);
    }

    reporter.stage(format!("Resolving {version_id}"));
    let mut rv = gi.resolve(&version_id).await?;

    // Forge/NeoForge need installer-generated files; (re)run the installer if they're missing.
    if spec.needs_installer() && !gi.missing_local_libraries(&rv, &env).is_empty() {
        reporter.warn("Some loader files are missing, running the installer again");
        version_id = install_loader(&gi, &spec, &pack.api, &env, java_override).await?;
        remember_resolved(paths, &pack.name, &version_id);
        rv = gi.resolve(&version_id).await?;
    }
    cancel.check()?;

    // 3. Java
    let java = match java_override {
        Some(p) => {
            if !p.is_file() {
                anyhow::bail!("custom Java path {} does not exist", p.display());
            }
            p.to_path_buf()
        }
        None => {
            let component = rv.json.java_version.as_ref().map(|j| j.component.clone()).unwrap_or_else(|| "jre-legacy".into());
            java::ensure_runtime(&client, paths, &component, req.concurrency, req.repair, reporter, cancel).await?
        }
    };
    cancel.check()?;

    // 4. Libraries, client jar, assets
    gi.install_files(&rv, &env).await?;
    cancel.check()?;

    // 5. Natives & classpath
    reporter.stage("Preparing game");
    let jar = gi.launch_jar(&rv)?;
    let natives_dir = paths.version_dir(&rv.json.id).join("natives");
    let n = gi.extract_natives(&rv, &env, &natives_dir)?;
    if n > 0 {
        reporter.info(format!("Extracted {n} native libraries"));
    }
    let mut classpath: Vec<PathBuf> = Vec::new();
    for f in GameInstaller::library_files(&rv, &env).into_iter().filter(|f| f.classpath) {
        let p = paths.libraries().join(&f.path);
        if !classpath.contains(&p) {
            classpath.push(p);
        }
    }
    classpath.push(jar);

    let game_dir = paths.instance_dir(&pack.name);
    std::fs::create_dir_all(&game_dir)?;
    std::fs::create_dir_all(paths.java_tmp())?;
    let opts = LaunchOptions {
        username: req.username.clone(),
        ram_mb: req.settings.ram_mb,
        extra_jvm_args: req.settings.jvm_args.clone(),
        resolution: custom_res.then_some((req.settings.width, req.settings.height)),
        game_dir: game_dir.clone(),
        assets_dir: paths.assets(),
        libraries_dir: paths.libraries(),
        natives_dir,
        temp_dir: paths.java_tmp(),
        classpath,
    };
    let args = build_arguments(&rv, &opts, &env)?;
    Ok(Prepared { java, args, game_dir })
}

fn remember_resolved(paths: &Paths, pack: &str, id: &str) {
    let mut db = InstalledDb::load(paths);
    if let Some(p) = db.packs.get_mut(pack) {
        p.resolved_version = Some(id.to_string());
        let _ = db.save(paths);
    }
}

fn run_game(p: Prepared, reporter: &Reporter) {
    reporter.info(format!("Starting Java: {}", p.java.display()));
    let mut cmd = Command::new(&p.java);
    cmd.args(&p.args).current_dir(&p.game_dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console(&mut cmd);
    let started_at = SystemTime::now();
    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            reporter.error(format!("Could not start the game: {e}"));
            reporter.send(Event::JobFailed { message: format!("Could not start Java: {e}"), cancelled: false });
            return;
        }
    };
    reporter.send(Event::GameStarted { pid: child.id() });

    let mut readers = Vec::new();
    for (stream, is_err) in [
        (child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), false),
        (child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), true),
    ] {
        let Some(stream) = stream else { continue };
        let reporter = reporter.clone();
        readers.push(std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            let mut classify = OutputClassifier::new(LogSource::Game, is_err);
            // read_until + lossy decode: game output isn't always valid UTF-8 on Windows.
            while reader.read_until(b'\n', &mut buf).map(|n| n > 0).unwrap_or(false) {
                let line = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                buf.clear();
                reporter.log(classify.line(&line));
            }
        }));
    }
    let code = child.wait().ok().and_then(|s| s.code());
    for r in readers {
        let _ = r.join();
    }
    let crash_report = if code == Some(0) { None } else { find_crash_report(&p.game_dir, started_at) };
    reporter.send(Event::GameExited { code, crash_report, played_secs: started.elapsed().as_secs() });
}

/// Newest crash report (or JVM error log) written since the game started.
pub fn find_crash_report(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();
    let mut scan = |dir: &Path, filter: &dyn Fn(&str) -> bool| {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !filter(&name) {
                    continue;
                }
                if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                    if m >= since {
                        candidates.push((m, e.path()));
                    }
                }
            }
        }
    };
    scan(&game_dir.join("crash-reports"), &|n| n.ends_with(".txt"));
    scan(game_dir, &|n| n.starts_with("hs_err_pid") && n.ends_with(".log"));
    candidates.sort_by_key(|(t, _)| *t);
    candidates.pop().map(|(_, p)| p)
}

/// Load the modpack list in the background.
pub fn spawn_load_modpacks(rt: &tokio::runtime::Handle, url: String, paths: Paths, reporter: Reporter) {
    rt.spawn(async move {
        let client = match net::client() {
            Ok(c) => c,
            Err(e) => {
                reporter.send(Event::ModpacksLoaded { result: Err(e.to_string()), from_cache: false });
                return;
            }
        };
        let (result, from_cache) = super::modpacks::fetch_modpacks(&client, &url, &paths).await;
        if from_cache {
            reporter.warn("Could not reach the modpack server; showing the last known list");
        }
        reporter.send(Event::ModpacksLoaded { result, from_cache });
    });
}

/// Check GitHub for a newer launcher release.
pub fn spawn_update_check(rt: &tokio::runtime::Handle, reporter: Reporter) {
    rt.spawn(async move {
        let Ok(client) = net::client() else { return };
        match super::update::check(&client).await {
            Ok(Some(info)) => reporter.send(Event::UpdateAvailable(info)),
            Ok(None) => {}
            Err(e) => reporter.warn(format!("Update check failed: {e:#}")),
        }
    });
}

/// Delete a pack's instance folder and forget it was installed.
pub fn uninstall_pack(paths: &Paths, name: &str) -> Result<()> {
    let dir = paths.instance_dir(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("could not delete {}", dir.display()))?;
    }
    let mut db = InstalledDb::load(paths);
    db.packs.remove(name);
    db.save(paths)
}

/// Look up the skin for a nickname in the background.
pub fn spawn_skin_lookup(rt: &tokio::runtime::Handle, name: String, paths: Paths, reporter: Reporter) {
    rt.spawn(async move {
        let skin = match net::client() {
            Ok(client) => super::skins::lookup(&client, &paths, &name).await.unwrap_or_else(|e| {
                reporter.log(super::events::LogLine::new(LogSource::Launcher, super::events::LogLevel::Debug, format!("Skin lookup for {name} failed: {e:#}")));
                None
            }),
            Err(_) => None,
        };
        reporter.send(Event::SkinLoaded { name, skin });
    });
}

/// Download a launcher update in the background.
pub fn spawn_update_download(rt: &tokio::runtime::Handle, info: super::update::UpdateInfo, reporter: Reporter) {
    rt.spawn(async move {
        let result = async {
            let client = net::client()?;
            let r = reporter.clone();
            let last = std::sync::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
            let on_bytes = move |done: u64, total: u64| {
                let mut l = last.lock().unwrap();
                if l.elapsed().as_millis() >= 100 || done == total {
                    *l = std::time::Instant::now();
                    r.send(Event::UpdateProgress { done, total });
                }
            };
            super::install::download_update(&client, &info, &on_bytes, &Cancel::default()).await
        }
        .await;
        match result {
            Ok(path) => reporter.send(Event::UpdateDownloaded(path)),
            Err(e) => reporter.send(Event::UpdateFailed(format!("{e:#}"))),
        }
    });
}
