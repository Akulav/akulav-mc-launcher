//! Headless mode for troubleshooting and scripting:
//!
//! ```text
//! AkulavLauncher --cli list
//! AkulavLauncher --cli launch "<pack name>" [--user NAME] [--ram MB] [--repair] [--prepare-only]
//! ```

use std::sync::Arc;

use crate::launcher::events::{Cancel, Event, LogLevel, Reporter};
use crate::launcher::jobs::{self, LaunchRequest};
use crate::launcher::modpacks::{fetch_modpacks, DEFAULT_MODPACK_LIST_URL};
use crate::launcher::paths::Paths;
use crate::launcher::settings::Settings;
use crate::launcher::{net, system};

pub fn run(args: &[String]) -> i32 {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let paths = Paths::detect().expect("data dir");
    paths.ensure().expect("create data dir");
    let mut settings = Settings::load(&paths);
    let url = settings.modpack_list_url.clone().unwrap_or_else(|| DEFAULT_MODPACK_LIST_URL.into());

    let packs = rt.block_on(async {
        let client = net::client().unwrap();
        fetch_modpacks(&client, &url, &paths).await.0
    });
    let packs = match packs {
        Ok(p) => p,
        Err(e) => {
            eprintln!("could not load modpacks: {e}");
            return 1;
        }
    };

    match args.first().map(String::as_str) {
        Some("list") => {
            for p in &packs {
                println!("{}\t{}\t{}", p.name, p.version, p.api);
            }
            0
        }
        Some("launch") => {
            let Some(name) = args.get(1) else {
                eprintln!("usage: --cli launch <pack> [--user NAME] [--ram MB] [--repair] [--prepare-only]");
                return 2;
            };
            let Some(pack) = packs.iter().find(|p| &p.name == name).cloned() else {
                eprintln!("no pack named {name}");
                return 2;
            };
            let flag = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
            let mut pack_settings = settings.pack(&pack.name, system::total_ram_mb()).clone();
            if let Some(ram) = flag("--ram").and_then(|r| r.parse().ok()) {
                pack_settings.ram_mb = ram;
            }
            let req = LaunchRequest {
                pack,
                username: flag("--user").unwrap_or_else(|| settings.username.clone()),
                settings: pack_settings,
                java_override: settings.java_path.clone().filter(|p| !p.is_empty()).map(Into::into),
                concurrency: settings.concurrent_downloads,
                repair: args.iter().any(|a| a == "--repair"),
                launch: !args.iter().any(|a| a == "--prepare-only"),
            };
            let (tx, rx) = crossbeam_channel::unbounded();
            let reporter = Reporter::new(tx, Arc::new(|| {}));
            jobs::spawn_job(rt.handle(), req, paths.clone(), reporter, Cancel::default());
            let mut last_pct = -1i64;
            for ev in rx.iter() {
                match ev {
                    Event::Log(l) => {
                        let tag = match l.level {
                            LogLevel::Error => "ERR ",
                            LogLevel::Warn => "WARN",
                            _ => "    ",
                        };
                        println!("{tag} [{:?}] {}", l.source, l.text);
                    }
                    Event::Progress(Some(p)) if p.total > 0 => {
                        let pct = (p.done * 100 / p.total) as i64;
                        if pct / 10 != last_pct / 10 {
                            println!("      … {pct}% ({}/{})", p.done, p.total);
                            last_pct = pct;
                        }
                    }
                    Event::Stage(_) => last_pct = -1,
                    Event::GameStarted { pid } => println!("==> game started (pid {pid})"),
                    Event::JobDone { message } => {
                        println!("==> {message}");
                        return 0;
                    }
                    Event::JobFailed { message, .. } => {
                        eprintln!("==> FAILED: {message}");
                        return 1;
                    }
                    Event::GameExited { code, crash_report, played_secs } => {
                        println!("==> game exited with {code:?} after {played_secs}s, crash report: {crash_report:?}");
                        settings.save(&paths).ok();
                        return code.unwrap_or(1);
                    }
                    _ => {}
                }
            }
            1
        }
        _ => {
            eprintln!("usage: AkulavLauncher --cli <list|launch> ...");
            2
        }
    }
}
