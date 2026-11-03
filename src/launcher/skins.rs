//! Player skin lookup for the avatar.
//!
//! Offline players don't log in, so we look the nickname up the same way the pack's
//! CustomSkinLoader does: first the official Mojang profile with that name, then Ely.by.
//! Results (including "no skin") are cached on disk for a few hours.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use base64::Engine;
use serde::Deserialize;

use super::paths::Paths;

const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const MISSING_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinSource {
    Mojang,
    ElyBy,
}

impl SkinSource {
    pub fn label(self) -> &'static str {
        match self {
            SkinSource::Mojang => "Mojang",
            SkinSource::ElyBy => "Ely.by",
        }
    }
    fn tag(self) -> &'static str {
        match self {
            SkinSource::Mojang => "mojang",
            SkinSource::ElyBy => "elyby",
        }
    }
    fn from_tag(s: &str) -> Option<Self> {
        match s {
            "mojang" => Some(SkinSource::Mojang),
            "elyby" => Some(SkinSource::ElyBy),
            _ => None,
        }
    }
}

/// A decoded skin, normalised to 64×64 RGBA.
#[derive(Debug, Clone)]
pub struct Skin {
    pub rgba: Vec<u8>,
    pub source: SkinSource,
    /// Alex-style (3px arms) model; kept for a future full-body preview.
    #[allow(dead_code)]
    pub slim: bool,
}

#[derive(Deserialize)]
struct MojangProfile {
    id: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    properties: Vec<SessionProperty>,
}

#[derive(Deserialize)]
struct SessionProperty {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct TexturesPayload {
    textures: Textures,
}

#[derive(Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<TextureRef>,
}

#[derive(Deserialize)]
struct TextureRef {
    url: String,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn cache_paths(paths: &Paths, name: &str) -> (PathBuf, PathBuf) {
    let dir = paths.cache.join("skins");
    let key = name.to_ascii_lowercase();
    (dir.join(format!("{key}.png")), dir.join(format!("{key}.txt")))
}

fn fresh(path: &Path, ttl: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() < ttl)
        .unwrap_or(false)
}

/// Look up a skin for `name`. `Ok(None)` means the name has no skin anywhere.
pub async fn lookup(client: &reqwest::Client, paths: &Paths, name: &str) -> Result<Option<Skin>> {
    let (png_path, meta_path) = cache_paths(paths, name);
    // meta file: "<source>[ slim]" or "none"
    if let Ok(meta) = std::fs::read_to_string(&meta_path) {
        let meta = meta.trim();
        if meta == "none" && fresh(&meta_path, MISSING_TTL) {
            return Ok(None);
        }
        if let Some(source) = meta.split_whitespace().next().and_then(SkinSource::from_tag) {
            if fresh(&meta_path, CACHE_TTL) {
                if let Ok(bytes) = std::fs::read(&png_path) {
                    if let Ok(rgba) = normalize_skin(&bytes) {
                        return Ok(Some(Skin { rgba, source, slim: meta.contains("slim") }));
                    }
                }
            }
        }
    }

    let mojang = from_mojang(client, name).await;
    let found = match mojang {
        Ok(Some(s)) => Some(s),
        _ => match from_elyby(client, name).await {
            Ok(found) => found,
            Err(e) if mojang.is_err() => {
                // Both lookups failed (offline?): don't remember "no skin", try again next time.
                return Err(e);
            }
            Err(_) => None,
        },
    };

    let _ = std::fs::create_dir_all(png_path.parent().unwrap());
    match &found {
        Some((bytes, source, slim)) => {
            let _ = std::fs::write(&png_path, bytes);
            let _ = std::fs::write(&meta_path, format!("{}{}", source.tag(), if *slim { " slim" } else { "" }));
        }
        None => {
            let _ = std::fs::write(&meta_path, "none");
        }
    }
    match found {
        Some((bytes, source, slim)) => Ok(Some(Skin { rgba: normalize_skin(&bytes)?, source, slim })),
        None => Ok(None),
    }
}

async fn from_mojang(client: &reqwest::Client, name: &str) -> Result<Option<(Vec<u8>, SkinSource, bool)>> {
    let resp = client
        .get(format!("https://api.mojang.com/users/profiles/minecraft/{name}"))
        .timeout(Duration::from_secs(8))
        .send()
        .await?;
    if resp.status().as_u16() == 204 || resp.status().as_u16() == 404 {
        return Ok(None);
    }
    let profile: MojangProfile = serde_json::from_slice(&resp.error_for_status()?.bytes().await?)?;
    let session_bytes = client
        .get(format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", profile.id))
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let session: SessionProfile = serde_json::from_slice(&session_bytes)?;
    let Some(prop) = session.properties.into_iter().find(|p| p.name == "textures") else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD.decode(prop.value.as_bytes()).context("bad textures payload")?;
    let payload: TexturesPayload = serde_json::from_slice(&decoded)?;
    let Some(skin) = payload.textures.skin else { return Ok(None) };
    let slim = skin
        .metadata
        .as_ref()
        .and_then(|m| m.get("model"))
        .and_then(|m| m.as_str())
        .is_some_and(|m| m == "slim");
    // textures.minecraft.net serves http URLs; https works too.
    let url = skin.url.replacen("http://", "https://", 1);
    let bytes = client.get(url).timeout(Duration::from_secs(10)).send().await?.error_for_status()?.bytes().await?;
    Ok(Some((bytes.to_vec(), SkinSource::Mojang, slim)))
}

async fn from_elyby(client: &reqwest::Client, name: &str) -> Result<Option<(Vec<u8>, SkinSource, bool)>> {
    let resp = client
        .get(format!("https://skinsystem.ely.by/skins/{name}.png"))
        .timeout(Duration::from_secs(8))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    // Ely.by redirects unknown names to a default skin hosted elsewhere; only accept its own files.
    let final_host = resp.url().host_str().unwrap_or("").to_string();
    if !final_host.ends_with("ely.by") {
        return Ok(None);
    }
    let bytes = resp.bytes().await?;
    if bytes.len() < 8 || &bytes[1..4] != b"PNG" {
        return Ok(None);
    }
    Ok(Some((bytes.to_vec(), SkinSource::ElyBy, false)))
}

/// Decode a skin PNG to 64×64 RGBA. Legacy 64×32 skins are padded (bottom half transparent);
/// HD skins (128×128 etc.) are downscaled with nearest-neighbour.
pub fn normalize_skin(png: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(png).context("not an image")?.to_rgba8();
    let (w, h) = img.dimensions();
    if w < 64 || w % 64 != 0 || (h != w && h * 2 != w) {
        anyhow::bail!("unexpected skin size {w}x{h}");
    }
    let scale = w / 64;
    let mut out = vec![0u8; 64 * 64 * 4];
    for y in 0..(h / scale).min(64) {
        for x in 0..64 {
            let p = img.get_pixel(x * scale, y * scale);
            let i = ((y * 64 + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&p.0);
        }
    }
    // Head base layer must be opaque (some editors leave stray alpha).
    for y in 0..16 {
        for x in 0..32 {
            let i = ((y * 64 + x) * 4) as usize;
            if (8..16).contains(&y) || (8..24).contains(&x) {
                out[i + 3] = 255;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y)));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn normalizes_legacy_and_hd() {
        let legacy = normalize_skin(&png(64, 32, |x, _| [x as u8, 0, 0, 255])).unwrap();
        assert_eq!(legacy.len(), 64 * 64 * 4);
        assert_eq!(legacy[(10 * 64 + 9) * 4], 9);
        assert_eq!(legacy[(40 * 64 + 9) * 4 + 3], 0, "missing bottom half is transparent");

        let hd = normalize_skin(&png(128, 128, |x, y| [(x / 2) as u8, (y / 2) as u8, 0, 255])).unwrap();
        assert_eq!(hd[(12 * 64 + 10) * 4], 10);
        assert_eq!(hd[(12 * 64 + 10) * 4 + 1], 12);

        assert!(normalize_skin(&png(50, 50, |_, _| [0; 4])).is_err());
    }

    #[test]
    fn head_is_forced_opaque() {
        let s = normalize_skin(&png(64, 64, |_, _| [1, 2, 3, 0])).unwrap();
        assert_eq!(s[(9 * 64 + 9) * 4 + 3], 255);
        assert_eq!(s[(9 * 64 + 41) * 4 + 3], 0, "hat layer keeps its transparency");
    }
}
