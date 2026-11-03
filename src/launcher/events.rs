//! Messages sent from background work to the UI.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;

use super::modpacks::Modpack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Launcher,
    Game,
    Installer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub time: chrono::DateTime<chrono::Local>,
    pub source: LogSource,
    pub level: LogLevel,
    pub text: String,
}

impl LogLine {
    pub fn new(source: LogSource, level: LogLevel, text: impl Into<String>) -> Self {
        Self { time: chrono::Local::now(), source, level, text: text.into() }
    }

}

/// Turns raw process output into log lines. Keeps per-stream state so indented continuation
/// lines (stack frames, the mod list) inherit the level of the line they belong to.
pub struct OutputClassifier {
    source: LogSource,
    is_stderr: bool,
    last: LogLevel,
}

impl OutputClassifier {
    pub fn new(source: LogSource, is_stderr: bool) -> Self {
        Self { source, is_stderr, last: LogLevel::Info }
    }

    pub fn line(&mut self, raw: &str) -> LogLine {
        let text = strip_ansi(raw);
        let level = match guess_level(&text) {
            Some(l) => l,
            None if is_continuation(&text) => self.last,
            None => if self.is_stderr { LogLevel::Warn } else { LogLevel::Info },
        };
        self.last = level;
        LogLine::new(self.source, level, text)
    }
}

fn is_continuation(text: &str) -> bool {
    text.starts_with('\t') || text.starts_with("  ") || text.starts_with("Caused by:") || text.trim_start().starts_with("at ") || text.trim_start().starts_with("...")
}

/// Remove ANSI escape sequences (NeoForge colors its console output).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: parameters until a final byte in @..~
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn guess_level(text: &str) -> Option<LogLevel> {
    // Typical formats: "[12:00:00] [main/INFO] ...", "[main/WARN] [mixin/]: ...", "ERROR ..."
    let head: String = text.chars().take(120).collect::<String>().to_ascii_uppercase();
    if head.contains("/ERROR]") || head.contains("/FATAL]") || head.starts_with("ERROR") || head.contains("EXCEPTION IN THREAD") {
        Some(LogLevel::Error)
    } else if head.contains("/WARN]") || head.starts_with("WARN") {
        Some(LogLevel::Warn)
    } else if head.contains("/DEBUG]") || head.contains("/TRACE]") {
        Some(LogLevel::Debug)
    } else if head.contains("/INFO]") {
        Some(LogLevel::Info)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressUnit {
    Bytes,
    Items,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    pub unit: ProgressUnit,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The current step of the running job (e.g. "Downloading libraries").
    Stage(String),
    Progress(Option<Progress>),
    Log(LogLine),
    ModpacksLoaded { result: Result<Vec<Modpack>, String>, from_cache: bool },
    UpdateAvailable(super::update::UpdateInfo),
    UpdateProgress { done: u64, total: u64 },
    UpdateDownloaded(PathBuf),
    UpdateFailed(String),
    /// Skin lookup finished for `name` (`None` = no skin found or offline).
    SkinLoaded { name: String, skin: Option<super::skins::Skin> },
    /// Preparation finished; the game process has started.
    GameStarted { pid: u32 },
    /// Preparation failed or was cancelled before the game started.
    JobFailed { message: String, cancelled: bool },
    /// A job that does not launch the game (e.g. repair) finished.
    JobDone { message: String },
    GameExited { code: Option<i32>, crash_report: Option<PathBuf>, played_secs: u64 },
    InstalledChanged,
}

/// Cheap, cloneable handle used by background tasks to talk to the UI.
#[derive(Clone)]
pub struct Reporter {
    tx: Sender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Reporter {
    pub fn new(tx: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { tx, wake }
    }

    pub fn send(&self, event: Event) {
        let _ = self.tx.send(event);
        (self.wake)();
    }

    pub fn stage(&self, text: impl Into<String>) {
        let text = text.into();
        self.info(format!("{text}…"));
        self.send(Event::Stage(text));
    }

    pub fn progress(&self, done: u64, total: u64, unit: ProgressUnit) {
        self.send(Event::Progress(Some(Progress { done, total, unit })));
    }

    pub fn clear_progress(&self) {
        self.send(Event::Progress(None));
    }

    pub fn log(&self, line: LogLine) {
        self.send(Event::Log(line));
    }

    pub fn info(&self, text: impl Into<String>) {
        self.log(LogLine::new(LogSource::Launcher, LogLevel::Info, text));
    }

    pub fn warn(&self, text: impl Into<String>) {
        self.log(LogLine::new(LogSource::Launcher, LogLevel::Warn, text));
    }

    pub fn error(&self, text: impl Into<String>) {
        self.log(LogLine::new(LogSource::Launcher, LogLevel::Error, text));
    }
}

/// Cooperative cancellation flag shared between the UI and a job.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn check(&self) -> anyhow::Result<()> {
        if self.is_cancelled() {
            Err(Cancelled.into())
        } else {
            Ok(())
        }
    }
}

pub fn is_cancelled_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<Cancelled>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_detection() {
        assert_eq!(guess_level("[12:00:00] [main/INFO]: hi"), Some(LogLevel::Info));
        assert_eq!(guess_level("[12:00:00] [Render thread/WARN]: x"), Some(LogLevel::Warn));
        assert_eq!(guess_level("[12:00:00] [main/ERROR]: boom"), Some(LogLevel::Error));
        assert_eq!(guess_level("plain"), None);
        let mut c = OutputClassifier::new(LogSource::Game, false);
        assert_eq!(c.line("\u{1b}[31m[12:00:00] [main/ERROR]: boom\u{1b}[m").level, LogLevel::Error);
        assert_eq!(c.line("\tat net.minecraft.Foo.bar(Foo.java:1)").level, LogLevel::Error);
        assert_eq!(c.line("[12:00:01] [main/INFO]: Mods:").level, LogLevel::Info);
        let l = c.line("\t\tAppleSkin 3.0.9 (appleskin)");
        assert_eq!(l.level, LogLevel::Info);
        assert_eq!(c.line("\u{1b}[32mhello\u{1b}[m").text, "hello");
    }

    #[test]
    fn cancel_error_roundtrip() {
        let c = Cancel::default();
        assert!(c.check().is_ok());
        c.cancel();
        let err = c.check().unwrap_err().context("while downloading");
        assert!(is_cancelled_error(&err));
    }
}
