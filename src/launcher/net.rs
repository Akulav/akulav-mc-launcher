//! HTTP helpers: a shared client, verified file downloads and parallel batch downloads.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures::{StreamExt, TryStreamExt};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use super::events::{Cancel, ProgressUnit, Reporter};

pub const USER_AGENT: &str = concat!("AkulavLauncher/", env!("CARGO_PKG_VERSION"));

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(32)
        .build()?)
}

pub async fn get_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
        }
        match client.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last_err = Some(e),
            },
            Err(e) => {
                // Don't retry 4xx responses, they won't get better.
                if e.status().is_some_and(|s| s.is_client_error()) {
                    return Err(e).with_context(|| format!("GET {url}"));
                }
                last_err = Some(e)
            }
        }
    }
    Err(last_err.unwrap()).with_context(|| format!("GET {url}"))
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let bytes = get_bytes(client, url).await?;
    Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

pub async fn get_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let text = get_text(client, url).await?;
    serde_json::from_str(&text).with_context(|| format!("invalid JSON from {url}"))
}

pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub executable: bool,
}

impl DownloadTask {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self { url: url.into(), dest: dest.into(), sha1: None, size: None, executable: false }
    }
    pub fn sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1.filter(|s| !s.is_empty());
        self
    }
    pub fn size(mut self, size: Option<u64>) -> Self {
        self.size = size.filter(|s| *s > 0);
        self
    }
}

/// Whether `task.dest` must be (re)downloaded. A quick check compares size only;
/// a full check (`verify`) also compares the SHA-1.
pub fn needs_download(task: &DownloadTask, verify: bool) -> bool {
    let Ok(meta) = std::fs::metadata(&task.dest) else { return true };
    if !meta.is_file() {
        return true;
    }
    if let Some(size) = task.size {
        if meta.len() != size {
            return true;
        }
    } else if meta.len() == 0 {
        return true;
    }
    if verify {
        if let Some(expected) = &task.sha1 {
            return match sha1_file(&task.dest) {
                Ok(actual) => !actual.eq_ignore_ascii_case(expected),
                Err(_) => true,
            };
        }
    }
    false
}

/// Download one URL to `dest`, verifying SHA-1 if known. Reports streamed bytes via `on_bytes`.
pub async fn download_file(
    client: &reqwest::Client,
    task: &DownloadTask,
    on_bytes: &(dyn Fn(i64) + Send + Sync),
    cancel: &Cancel,
) -> Result<()> {
    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(&task.dest);
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..4u32 {
        cancel.check()?;
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400 * 2u64.pow(attempt))).await;
        }
        let mut received: i64 = 0;
        let result: Result<()> = async {
            let resp = client.get(&task.url).send().await?.error_for_status()?;
            let mut file = tokio::fs::File::create(&part).await?;
            let mut hasher = Sha1::new();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                cancel.check()?;
                let chunk = chunk?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                received += chunk.len() as i64;
                on_bytes(chunk.len() as i64);
            }
            file.flush().await?;
            drop(file);
            if let Some(expected) = &task.sha1 {
                let actual = hex::encode(hasher.finalize());
                if !actual.eq_ignore_ascii_case(expected) {
                    bail!("checksum mismatch (expected {expected}, got {actual})");
                }
            }
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                replace_file(&part, &task.dest).await?;
                #[cfg(unix)]
                if task.executable {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&task.dest, std::fs::Permissions::from_mode(0o755));
                }
                return Ok(());
            }
            Err(e) => {
                on_bytes(-received); // roll back progress for this attempt
                let _ = tokio::fs::remove_file(&part).await;
                if super::events::is_cancelled_error(&e) {
                    return Err(e);
                }
                let fatal = e
                    .downcast_ref::<reqwest::Error>()
                    .and_then(|r| r.status())
                    .is_some_and(|s| s.is_client_error());
                last_err = Some(e);
                if fatal {
                    break;
                }
            }
        }
    }
    Err(last_err.unwrap()).with_context(|| format!("downloading {}", task.url))
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

async fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(_) => {
            // On Windows rename fails if the target exists and is in use / read-only.
            let _ = tokio::fs::remove_file(to).await;
            tokio::fs::rename(from, to).await
        }
    }
}

/// Download many files in parallel with an aggregated byte progress bar.
pub async fn download_all(
    client: &reqwest::Client,
    tasks: Vec<DownloadTask>,
    concurrency: usize,
    verify: bool,
    reporter: &Reporter,
    cancel: &Cancel,
) -> Result<usize> {
    // Checking thousands of files (assets) is IO bound: do it off the async threads.
    let pending: Vec<DownloadTask> = tokio::task::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        tasks
            .into_iter()
            .filter(|t| seen.insert(t.dest.clone()))
            .filter(|t| needs_download(t, verify))
            .collect()
    })
    .await?;
    cancel.check()?;
    if pending.is_empty() {
        return Ok(0);
    }

    let count = pending.len();
    let total: u64 = pending.iter().map(|t| t.size.unwrap_or(0)).sum();
    let done = Arc::new(AtomicU64::new(0));
    let files_done = Arc::new(AtomicU64::new(0));
    let unit = if total > 0 { ProgressUnit::Bytes } else { ProgressUnit::Items };

    let ticker = {
        let done = done.clone();
        let files_done = files_done.clone();
        let reporter = reporter.clone();
        tokio::spawn(async move {
            loop {
                match unit {
                    ProgressUnit::Bytes => reporter.progress(done.load(Ordering::Relaxed).min(total), total, unit),
                    ProgressUnit::Items => reporter.progress(files_done.load(Ordering::Relaxed), count as u64, unit),
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
        })
    };

    let on_bytes = {
        let done = done.clone();
        move |delta: i64| {
            if delta >= 0 {
                done.fetch_add(delta as u64, Ordering::Relaxed);
            } else {
                done.fetch_sub((-delta) as u64, Ordering::Relaxed);
            }
        }
    };

    let result = futures::stream::iter(pending.into_iter().map(Ok::<_, anyhow::Error>))
        .try_for_each_concurrent(concurrency.max(1), |task| {
            let on_bytes = &on_bytes;
            let files_done = &files_done;
            async move {
                download_file(client, &task, on_bytes, cancel).await?;
                files_done.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await;

    ticker.abort();
    match unit {
        ProgressUnit::Bytes => reporter.progress(total, total, unit),
        ProgressUnit::Items => reporter.progress(count as u64, count as u64, unit),
    }
    result.map(|_| count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_download_checks_size_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, b"hello").unwrap();
        let sha = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"; // sha1("hello")
        let t = DownloadTask::new("x", &p).size(Some(5)).sha1(Some(sha.into()));
        assert!(!needs_download(&t, true));
        let wrong = DownloadTask::new("x", &p).size(Some(5)).sha1(Some("00".into()));
        assert!(!needs_download(&wrong, false));
        assert!(needs_download(&wrong, true));
        let size = DownloadTask::new("x", &p).size(Some(6));
        assert!(needs_download(&size, false));
        let missing = DownloadTask::new("x", dir.path().join("nope"));
        assert!(needs_download(&missing, false));
    }

    #[test]
    fn part_paths() {
        assert_eq!(part_path(Path::new("/a/b.jar")), PathBuf::from("/a/b.jar.part"));
    }
}
