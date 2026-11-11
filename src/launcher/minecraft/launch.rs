//! Turning a resolved version into a Java command line.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use super::version::{Env, ResolvedVersion};

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub username: String,
    pub ram_mb: u32,
    pub extra_jvm_args: String,
    pub resolution: Option<(u32, u32)>,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub natives_dir: PathBuf,
    /// Java's temp folder, inside the launcher's folder.
    pub temp_dir: PathBuf,
    pub classpath: Vec<PathBuf>,
}

/// The UUID Minecraft servers expect for an offline player:
/// `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` (MD5, version 3).
pub fn offline_uuid(name: &str) -> String {
    let mut hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    let h = hex::encode(hash);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

/// Split user-provided JVM arguments, honouring double quotes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    if !arg.contains("${") {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// Build the full argument list (everything after the `java` executable).
pub fn build_arguments(rv: &ResolvedVersion, opts: &LaunchOptions, env: &Env) -> anyhow::Result<Vec<String>> {
    let v = &rv.json;
    let main_class = v.main_class.clone().ok_or_else(|| anyhow::anyhow!("version {} has no main class", v.id))?;
    let sep = classpath_separator();
    let classpath = opts.classpath.iter().map(|p| path_str(p)).collect::<Vec<_>>().join(sep);
    let asset_index = v.asset_index.as_ref().map(|a| a.id.clone()).or_else(|| v.assets.clone()).unwrap_or_else(|| "legacy".into());
    let legacy_assets = opts.assets_dir.join("virtual").join(if asset_index == "pre-1.6" { "legacy" } else { &asset_index });
    let game_assets = if legacy_assets.is_dir() { legacy_assets } else { opts.assets_dir.clone() };
    let uuid = offline_uuid(&opts.username);
    let (w, h) = opts.resolution.unwrap_or((854, 480));

    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", opts.username.clone()),
        ("version_name", v.id.clone()),
        ("game_directory", path_str(&opts.game_dir)),
        ("assets_root", path_str(&opts.assets_dir)),
        ("game_assets", path_str(&game_assets)),
        ("assets_index_name", asset_index.clone()),
        ("auth_uuid", uuid.clone()),
        ("auth_access_token", "0".into()),
        ("auth_session", "0".into()),
        ("auth_xuid", "0".into()),
        ("clientid", "0".into()),
        ("user_type", "legacy".into()),
        ("user_properties", "{}".into()),
        ("version_type", v.kind.clone().unwrap_or_else(|| "release".into())),
        ("natives_directory", path_str(&opts.natives_dir)),
        ("launcher_name", "AkulavLauncher".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.clone()),
        ("classpath_separator", sep.into()),
        ("library_directory", path_str(&opts.libraries_dir)),
        ("resolution_width", w.to_string()),
        ("resolution_height", h.to_string()),
    ]);

    let mut args = vec![
        format!("-Xmx{}M", opts.ram_mb),
        format!("-Xms{}M", opts.ram_mb.min(1024)),
        format!("-Djava.io.tmpdir={}", path_str(&opts.temp_dir)),
    ];
    args.extend(split_args(&opts.extra_jvm_args));

    match v.arguments.as_ref().filter(|a| !a.jvm.is_empty()) {
        Some(a) => {
            for arg in &a.jvm {
                args.extend(arg.values(env).iter().map(|s| substitute(s, &vars)));
            }
        }
        None => {
            // Pre-1.13 versions don't specify JVM arguments.
            args.push(format!("-Djava.library.path={}", path_str(&opts.natives_dir)));
            if env.os == "windows" {
                args.push("-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump".into());
            }
            args.push("-cp".into());
            args.push(classpath.clone());
        }
    }

    args.push(main_class);

    let mut game_args: Vec<String> = Vec::new();
    if let Some(legacy) = &v.minecraft_arguments {
        game_args.extend(legacy.split_whitespace().map(|s| substitute(s, &vars)));
    }
    if let Some(a) = &v.arguments {
        for arg in &a.game {
            game_args.extend(arg.values(env).iter().map(|s| substitute(s, &vars)));
        }
    }
    // Legacy versions have no `has_custom_resolution` rule; pass it explicitly.
    if opts.resolution.is_some() && !game_args.iter().any(|a| a == "--width") {
        game_args.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
    }
    args.extend(game_args);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::minecraft::version::VersionJson;

    #[test]
    fn offline_uuid_matches_java() {
        // Values produced by Java's UUID.nameUUIDFromBytes("OfflinePlayer:<name>")
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("Steve"), "5627dd98-e6be-3c21-b8a8-e92344183641");
    }

    #[test]
    fn splits_quoted_args() {
        assert_eq!(split_args(r#"-Xss2M  -Dfoo="a b"  -XX:+UseG1GC"#), vec!["-Xss2M", "-Dfoo=a b", "-XX:+UseG1GC"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn substitutes_variables() {
        let vars = HashMap::from([("a", "1".to_string()), ("b", "2".to_string())]);
        assert_eq!(substitute("x${a}y${b}z", &vars), "x1y2z");
        assert_eq!(substitute("${unknown}", &vars), "${unknown}");
        assert_eq!(substitute("broken${a", &vars), "broken${a");
    }

    #[test]
    fn builds_modern_command_line() {
        let json: VersionJson = serde_json::from_str(r#"{"id":"neoforge-21.1.248","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "assetIndex":{"id":"17","url":"u"},
            "arguments":{"game":["--username","${auth_player_name}","--assetIndex","${assets_index_name}",
               {"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}","--height","${resolution_height}"]}],
             "jvm":["-Djava.library.path=${natives_directory}","-DlibraryDirectory=${library_directory}","-cp","${classpath}"]}}"#).unwrap();
        let rv = ResolvedVersion { json, chain: vec![], jar_source: "1.21.1".into() };
        let opts = LaunchOptions {
            username: "Steve".into(),
            ram_mb: 6144,
            extra_jvm_args: "-XX:+UseG1GC".into(),
            resolution: Some((1600, 900)),
            game_dir: "/g".into(),
            assets_dir: "/a".into(),
            libraries_dir: "/l".into(),
            natives_dir: "/n".into(),
            temp_dir: "/t".into(),
            classpath: vec!["/l/x.jar".into(), "/v/c.jar".into()],
        };
        let env = Env::current().with_feature("has_custom_resolution", true);
        let args = build_arguments(&rv, &opts, &env).unwrap();
        assert_eq!(args[0], "-Xmx6144M");
        assert!(args.contains(&"-XX:+UseG1GC".to_string()));
        assert!(args.contains(&"-Djava.library.path=/n".to_string()));
        let main = args.iter().position(|a| a == "cpw.mods.bootstraplauncher.BootstrapLauncher").unwrap();
        assert_eq!(args[main - 1], format!("/l/x.jar{}/v/c.jar", classpath_separator()));
        assert_eq!(&args[main + 1..main + 3], &["--username".to_string(), "Steve".to_string()]);
        assert!(args.ends_with(&["--width".into(), "1600".into(), "--height".into(), "900".into()]));
    }
}
