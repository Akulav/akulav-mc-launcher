//! Small platform helpers: memory size, opening folders, the click sound.

use std::path::Path;

/// Total physical memory in MiB.
pub fn total_ram_mb() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    (sys.total_memory() / (1024 * 1024)).max(1024)
}

pub fn open_path(path: &Path) {
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    let _ = open::that_detached(path);
}

pub fn folder_size(path: &Path) -> u64 {
    fn walk(p: &Path) -> u64 {
        let Ok(rd) = std::fs::read_dir(p) else { return 0 };
        rd.flatten()
            .map(|e| match e.file_type() {
                Ok(t) if t.is_dir() => walk(&e.path()),
                Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => 0,
            })
            .sum()
    }
    walk(path)
}

pub fn format_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// The lever click from the original launcher.
#[cfg(windows)]
pub fn play_click() {
    static SOUND: &[u8] = include_bytes!("../../assets/lever.wav");
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(psz: *const u16, hmod: *mut core::ffi::c_void, flags: u32) -> i32;
    }
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;
    // SAFETY: SOUND is a 'static in-memory WAV, which SND_MEMORY|SND_ASYNC requires.
    unsafe {
        PlaySoundW(SOUND.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub fn play_click() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
