//! Launcher update check via `version.txt`:
//! line 1 = latest version, line 2 = direct URL of the new `AkulavLauncher.exe`,
//! optional line 3 = its SHA-256. Installing is done by [`super::selfupdate`].

use anyhow::{anyhow, Result};

use super::net;

pub const VERSION_URL: &str = "https://akulav.com/shared/5ce6926c3dda5f322df1ac59";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
}

/// Name of this platform in `version.txt` platform lines.
pub fn platform_tag() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse `version.txt` for this platform:
///
/// ```text
/// 10.1.0                                   <- latest version
/// https://…/AkulavLauncher.exe             <- Windows exe (also what the old 9.x launcher downloads)
/// <sha256 of the exe>                      <- optional
/// linux https://…/AkulavLauncher-linux-x86_64 <sha256>   <- optional per-platform lines
/// ```
///
/// Windows uses a `windows …` line if present, otherwise line 2. Other platforms only update
/// when their own line exists, so a Linux launcher is never offered the Windows exe.
pub fn parse_version_file(text: &str) -> Option<UpdateInfo> {
    parse_version_file_for(text, platform_tag())
}

pub fn parse_version_file_for(text: &str, platform: &str) -> Option<UpdateInfo> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let version = lines.first()?.trim_start_matches('\u{feff}').to_string();
    let default_url = lines.get(1)?.to_string();
    let default_sha = lines.get(2).filter(|l| is_sha256(l)).map(|s| s.to_string());

    for line in lines.iter().skip(2) {
        let mut parts = line.split_whitespace();
        if parts.next().is_some_and(|tag| tag.eq_ignore_ascii_case(platform)) {
            let url = parts.next()?.to_string();
            let sha256 = parts.next().filter(|s| is_sha256(s)).map(str::to_string);
            return Some(UpdateInfo { version, url, sha256 });
        }
    }
    (platform == "windows").then_some(UpdateInfo { version, url: default_url, sha256: default_sha })
}

/// Compare dotted versions numerically ("10.0.0" > "9.0.0"). Non-numeric parts compare as 0.
pub fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(remote), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub async fn check(client: &reqwest::Client) -> Result<Option<UpdateInfo>> {
    // AKULAV_UPDATE_URL lets you point a test build at a staging version file.
    let url = std::env::var("AKULAV_UPDATE_URL").unwrap_or_else(|_| VERSION_URL.to_string());
    let text = net::get_text(client, &url).await?;
    if text.lines().filter(|l| !l.trim().is_empty()).count() < 2 {
        return Err(anyhow!("version file is malformed"));
    }
    // None = no build published for this platform yet.
    let Some(info) = parse_version_file(&text) else { return Ok(None) };
    Ok(is_newer(&info.version, CURRENT_VERSION).then_some(info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_existing_version_file() {
        let info = parse_version_file_for("9.0.0\r\nhttps://github.com/Akulav/AkulavMCLauncher/releases/download/9.0.0/AkulavLauncherInstaller.exe\r\n", "windows").unwrap();
        assert_eq!(info.version, "9.0.0");
        assert!(info.url.ends_with("AkulavLauncherInstaller.exe"));
        assert!(parse_version_file_for("9.0.0", "windows").is_none());
        let sha = "a".repeat(64);
        let with_hash = parse_version_file_for(&format!("10.1.0\nhttps://x/AkulavLauncher.exe\n{sha}\n"), "windows").unwrap();
        assert_eq!(with_hash.sha256.as_deref(), Some(sha.as_str()));
    }

    #[test]
    fn platform_lines() {
        let sha = "b".repeat(64);
        let text = format!("10.1.0\nhttps://x/AkulavLauncher.exe\n{}\nlinux https://x/AkulavLauncher-linux-x86_64 {sha}\n", "a".repeat(64));
        let win = parse_version_file_for(&text, "windows").unwrap();
        assert!(win.url.ends_with(".exe"));
        assert_eq!(win.sha256.as_deref(), Some("a".repeat(64).as_str()));
        let lin = parse_version_file_for(&text, "linux").unwrap();
        assert!(lin.url.ends_with("linux-x86_64"));
        assert_eq!(lin.sha256.as_deref(), Some(sha.as_str()));
        // No linux line: Linux gets no update rather than the Windows exe.
        assert!(parse_version_file_for("10.1.0\nhttps://x/AkulavLauncher.exe\n", "linux").is_none());
        // Linux line without hash, right after the URL.
        let t = "10.1.0\nhttps://x/a.exe\nlinux https://x/b\n";
        assert_eq!(parse_version_file_for(t, "linux").unwrap().url, "https://x/b");
        assert_eq!(parse_version_file_for(t, "windows").unwrap().sha256, None);
    }

    #[test]
    fn version_ordering() {
        assert!(is_newer("10.0.1", "10.0.0"));
        assert!(is_newer("10.1", "10.0.9"));
        assert!(!is_newer("9.0.0", "10.0.0"), "the old C# release must not 'update' the new launcher");
        assert!(!is_newer("10.0.0", "10.0.0"));
        assert!(is_newer("v11", "10.5.5"));
    }
}
