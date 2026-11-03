use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::paths::{write_atomic, Paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AfterLaunch {
    /// Minimize the launcher while playing and bring it back when the game closes.
    #[default]
    Minimize,
    /// Leave the launcher open (useful to watch the console).
    KeepOpen,
    /// Close the launcher as soon as the game starts.
    Close,
}

impl AfterLaunch {
    pub const ALL: [AfterLaunch; 3] = [AfterLaunch::Minimize, AfterLaunch::KeepOpen, AfterLaunch::Close];
    pub fn label(self) -> &'static str {
        match self {
            AfterLaunch::Minimize => "Minimize launcher",
            AfterLaunch::KeepOpen => "Keep launcher open",
            AfterLaunch::Close => "Close launcher",
        }
    }
}

pub const DEFAULT_JVM_ARGS: &str = "-XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackSettings {
    pub ram_mb: u32,
    pub jvm_args: String,
    pub custom_resolution: bool,
    pub width: u32,
    pub height: u32,
    pub after_launch: AfterLaunch,
    /// Total time spent in game with this pack.
    pub play_seconds: u64,
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for PackSettings {
    fn default() -> Self {
        Self {
            ram_mb: 4096,
            jvm_args: DEFAULT_JVM_ARGS.to_string(),
            custom_resolution: false,
            width: 1280,
            height: 720,
            after_launch: AfterLaunch::Minimize,
            play_seconds: 0,
            last_played: None,
        }
    }
}

impl PackSettings {
    /// Sensible default RAM for a modpack given the machine's memory.
    pub fn with_recommended_ram(total_ram_mb: u64) -> Self {
        Self { ram_mb: recommended_ram_mb(total_ram_mb), ..Default::default() }
    }
}

/// Half of system memory, clamped to 2–8 GB and rounded to 512 MB.
pub fn recommended_ram_mb(total_ram_mb: u64) -> u32 {
    let half = (total_ram_mb / 2).clamp(2048, 8192);
    ((half / 512) * 512) as u32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub username: String,
    pub selected_pack: Option<String>,
    /// Overrides the default modpack list URL when set.
    pub modpack_list_url: Option<String>,
    /// Use this Java executable instead of the automatically downloaded runtime.
    pub java_path: Option<String>,
    pub click_sound: bool,
    /// Animated wallpaper with embers on the Play page.
    pub animated_background: bool,
    pub concurrent_downloads: usize,
    pub packs: HashMap<String, PackSettings>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: "Steve".into(),
            selected_pack: None,
            modpack_list_url: None,
            java_path: None,
            click_sound: true,
            animated_background: true,
            concurrent_downloads: 16,
            packs: HashMap::new(),
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read(paths.settings_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        write_atomic(&paths.settings_file(), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn pack(&mut self, name: &str, total_ram_mb: u64) -> &mut PackSettings {
        self.packs
            .entry(name.to_string())
            .or_insert_with(|| PackSettings::with_recommended_ram(total_ram_mb))
    }
}

/// Validates a Minecraft offline nickname. Returns a user-facing error message on failure.
pub fn validate_username(name: &str) -> Result<(), &'static str> {
    let len = name.chars().count();
    if len < 3 {
        return Err("Nickname must be at least 3 characters.");
    }
    if len > 16 {
        return Err("Nickname can be at most 16 characters.");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Only letters, digits and underscores are allowed.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert!(validate_username("Steve").is_ok());
        assert!(validate_username("a_b_1").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("this_name_is_way_too_long").is_err());
        assert!(validate_username("bad name").is_err());
        assert!(validate_username("Ünï").is_err());
    }

    #[test]
    fn ram_recommendation() {
        assert_eq!(recommended_ram_mb(16 * 1024), 8192);
        assert_eq!(recommended_ram_mb(8 * 1024), 4096);
        assert_eq!(recommended_ram_mb(2 * 1024), 2048);
        assert_eq!(recommended_ram_mb(64 * 1024), 8192);
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"username":"Alex"}"#).unwrap();
        assert_eq!(s.username, "Alex");
        assert!(s.click_sound);
        assert_eq!(s.concurrent_downloads, 16);
    }
}
