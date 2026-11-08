//! Launcher update check against GitHub releases.
//!
//! Every release carries exactly one file per OS plus `SHA256SUMS.txt`:
//!
//! | OS            | asset                        | applied by                                   |
//! | ------------- | ---------------------------- | -------------------------------------------- |
//! | Windows x64   | `AkulavLauncher-setup.exe`   | running the setup silently in update mode    |
//! | Linux x64     | `AkulavLauncher-linux-x64`   | swapping the binary in the install folder    |
//! | macOS arm64   | `AkulavLauncher-macos-arm64` | swapping the binary inside the app bundle    |
//!
//! An update is only offered when the release has a checksum for this OS's file. Downloading and
//! applying is done by [`super::install`].

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::net;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = "Akulav";
pub const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");
pub const LATEST_RELEASE_API: &str = "https://api.github.com/repos/Akulav/akulav-mc-launcher/releases/latest";

pub const ASSET_WINDOWS: &str = "AkulavLauncher-setup.exe";
pub const ASSET_LINUX: &str = "AkulavLauncher-linux-x64";
pub const ASSET_MACOS: &str = "AkulavLauncher-macos-arm64";
pub const ASSET_SUMS: &str = "SHA256SUMS.txt";

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

/// The release file for this OS, or `None` where no build is published.
pub fn this_os_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some(ASSET_WINDOWS)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(ASSET_LINUX)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(ASSET_MACOS)
    } else {
        None
    }
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

/// Find `name` in a `SHA256SUMS.txt` (`<hex>  <name>` or `<hex> *<name>`).
pub fn find_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// Asset URLs of `release` for `asset`, or `None` when the release isn't usable.
fn pick(release: &GhRelease, asset: &str) -> Option<(String, String, String)> {
    if release.draft || release.prerelease {
        return None;
    }
    let file = release.assets.iter().find(|a| a.name == asset)?;
    let sums = release.assets.iter().find(|a| a.name == ASSET_SUMS)?;
    let version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    Some((version, file.browser_download_url.clone(), sums.browser_download_url.clone()))
}

/// Newer release for this OS, if any.
pub async fn check(client: &reqwest::Client) -> Result<Option<UpdateInfo>> {
    let Some(asset) = this_os_asset() else { return Ok(None) };
    // AKULAV_UPDATE_URL points a test build at another releases/latest-style JSON document.
    let url = std::env::var("AKULAV_UPDATE_URL").unwrap_or_else(|_| LATEST_RELEASE_API.to_string());
    let release: GhRelease = net::get_json(client, &url).await.context("could not reach GitHub")?;
    let Some((version, file_url, sums_url)) = pick(&release, asset) else { return Ok(None) };
    if !is_newer(&version, CURRENT_VERSION) {
        return Ok(None);
    }
    let sums = net::get_text(client, &sums_url).await?;
    let sha256 = find_sum(&sums, asset).ok_or_else(|| anyhow!("release {version} has no checksum for {asset}"))?;
    Ok(Some(UpdateInfo { version, url: file_url, sha256 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("10.0.1", "10.0.0"));
        assert!(is_newer("10.1", "10.0.9"));
        assert!(!is_newer("9.0.0", "10.0.0"), "the old C# release must not 'update' the new launcher");
        assert!(!is_newer("10.0.0", "10.0.0"));
        assert!(is_newer("v11", "10.5.5"));
    }

    #[test]
    fn sums_parsing() {
        let a = "a".repeat(64);
        let s = format!("{a}  {ASSET_WINDOWS}\n{} *{ASSET_LINUX}\n", "B".repeat(64));
        assert_eq!(find_sum(&s, ASSET_WINDOWS), Some(a));
        assert_eq!(find_sum(&s, ASSET_LINUX), Some("b".repeat(64)));
        assert_eq!(find_sum(&s, ASSET_MACOS), None);
        assert_eq!(find_sum("zz  x\n", "x"), None);
    }

    #[test]
    fn picks_release_assets() {
        let json = format!(
            r#"{{"tag_name":"10.3.0","assets":[
                {{"name":"{ASSET_WINDOWS}","browser_download_url":"https://x/setup.exe"}},
                {{"name":"{ASSET_SUMS}","browser_download_url":"https://x/sums"}}]}}"#
        );
        let r: GhRelease = serde_json::from_str(&json).unwrap();
        let (v, file, sums) = pick(&r, ASSET_WINDOWS).unwrap();
        assert_eq!((v.as_str(), file.as_str(), sums.as_str()), ("10.3.0", "https://x/setup.exe", "https://x/sums"));
        assert!(pick(&r, ASSET_LINUX).is_none(), "no file for this OS: no update");
        let pre: GhRelease = serde_json::from_str(&json.replace("\"tag_name\"", "\"prerelease\":true,\"tag_name\"")).unwrap();
        assert!(pick(&pre, ASSET_WINDOWS).is_none());
    }
}
