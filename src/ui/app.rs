//! Application state, event handling and the window chrome (title bar, sidebar, toasts, dialogs).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle};
use egui_phosphor::regular as ph;

use super::theme::*;
use super::widgets::*;
use crate::launcher::events::{Cancel, Event, LogLevel, LogLine, LogSource, Progress, ProgressUnit, Reporter};
use crate::launcher::install::{self, Offer, StartupInfo};
use crate::launcher::jobs::{self, LaunchRequest};
use crate::launcher::modpacks::{InstalledDb, Modpack, DEFAULT_MODPACK_LIST_URL};
use crate::launcher::paths::Paths;
use crate::launcher::settings::{validate_username, AfterLaunch, Settings};
use crate::launcher::skins::SkinSource;
use crate::launcher::system;
use crate::launcher::update::{self, UpdateInfo};

pub const MAX_LOG_LINES: usize = 25_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Console,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Launch,
    Repair,
}

pub struct JobState {
    pub cancel: Cancel,
    pub pack: String,
    pub kind: JobKind,
    pub stage: String,
    pub progress: Option<Progress>,
    pub rate: RateMeter,
}

pub struct GameState {
    pub pack: String,
    #[allow(dead_code)]
    pub pid: u32,
    pub started: Instant,
    pub after_launch: AfterLaunch,
}

/// Smoothed bytes/second for the download readout.
#[derive(Default)]
pub struct RateMeter {
    last: Option<(Instant, u64)>,
    pub bytes_per_sec: f64,
}

impl RateMeter {
    fn sample(&mut self, p: &Progress) {
        if p.unit != ProgressUnit::Bytes {
            return;
        }
        let now = Instant::now();
        match self.last {
            Some((t, d)) if p.done >= d => {
                let dt = now.duration_since(t).as_secs_f64();
                if dt >= 0.25 {
                    let inst = (p.done - d) as f64 / dt;
                    self.bytes_per_sec = if self.bytes_per_sec == 0.0 { inst } else { self.bytes_per_sec * 0.7 + inst * 0.3 };
                    self.last = Some((now, p.done));
                }
            }
            _ => {
                self.last = Some((now, p.done));
                self.bytes_per_sec = 0.0;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub enum ToastAction {
    OpenFile(PathBuf),
    ShowConsole,
    ShowUpdate,
}

pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    pub created: Instant,
    pub action: Option<(String, ToastAction)>,
}

pub enum Confirm {
    Uninstall(String),
    ResetSettings,
    /// Remove the launcher itself (Windows: starts the setup's uninstaller).
    UninstallLauncher,
}

/// Lifecycle of a launcher self-update.
pub enum UpdateState {
    None,
    Available(UpdateInfo),
    Downloading { info: UpdateInfo, done: u64, total: u64 },
    Ready { info: UpdateInfo, path: PathBuf },
    Failed { info: UpdateInfo, error: String },
}

impl UpdateState {
    pub fn info(&self) -> Option<&UpdateInfo> {
        match self {
            UpdateState::None => None,
            UpdateState::Available(i) => Some(i),
            UpdateState::Downloading { info, .. } | UpdateState::Ready { info, .. } | UpdateState::Failed { info, .. } => Some(info),
        }
    }
}

pub enum SkinEntry {
    Loading,
    Ready { texture: TextureHandle, source: SkinSource },
    Missing,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum LoadState {
    Loading,
    Ready,
    Failed,
}

pub struct LauncherApp {
    pub paths: Paths,
    pub settings: Settings,
    pub settings_dirty: Option<Instant>,
    pub installed: InstalledDb,
    pub total_ram_mb: u64,

    pub packs: Vec<Modpack>,
    pub packs_state: LoadState,
    pub packs_error: Option<String>,
    pub packs_from_cache: bool,

    pub page: Page,
    pub drawer_open: bool,
    pub job: Option<JobState>,
    pub game: Option<GameState>,
    pub last_crash: Option<PathBuf>,

    pub logs: VecDeque<LogLine>,
    pub log_filter: String,
    pub log_min_level: LogLevel,
    pub log_autoscroll: bool,
    pub log_cache: Option<(usize, u64, String, LogLevel, Vec<usize>)>,
    pub log_generation: u64,
    pub unseen_errors: usize,

    pub toasts: Vec<Toast>,
    pub confirm: Option<Confirm>,
    pub update: UpdateState,
    pub show_update_dialog: bool,
    pub last_update_check: Instant,
    pub page_since: Instant,
    pub skins: HashMap<String, SkinEntry>,
    pub generated_heads: HashMap<String, TextureHandle>,
    pub name_edited_at: Option<Instant>,
    pub head_drag: egui::Vec2,
    pub hero_anim: Option<(String, Instant)>,
    pub uninstall_delete_data: bool,
    /// Set after uninstalling, or after handing over to an installed copy, so nothing is written
    /// back on exit.
    pub skip_save: bool,
    /// Linux/macOS: install (or update the installed copy) before anything else.
    pub install_offer: Offer,
    pub install_dir_edit: String,
    pub install_error: Option<String>,
    pub list_url_edit: String,
    pub java_edit: String,

    pub background: Option<TextureHandle>,
    pub app_icon: Option<TextureHandle>,

    pub rt: tokio::runtime::Runtime,
    #[allow(dead_code)]
    pub tx: Sender<Event>,
    pub rx: Receiver<Event>,
    pub reporter: Reporter,
}

impl LauncherApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: StartupInfo) -> Self {
        install_fonts(&cc.egui_ctx);
        apply_style(&cc.egui_ctx);

        let paths = Paths::detect().unwrap_or_else(|_| Paths::at(std::env::temp_dir().join("AkulavLauncher")));
        let settings = Settings::load(&paths);
        let installed = InstalledDb::load(&paths);

        let (tx, rx) = crossbeam_channel::unbounded();
        let ctx = cc.egui_ctx.clone();
        let reporter = Reporter::new(tx.clone(), Arc::new(move || ctx.request_repaint()));

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("akulav-worker")
            .build()
            .expect("failed to start the async runtime");

        let background = load_texture(&cc.egui_ctx, "background", include_bytes!("../../assets/background.jpg"));
        let app_icon = load_texture(&cc.egui_ctx, "icon", include_bytes!("../../assets/icon.png"));

        let mut app = Self {
            list_url_edit: settings.modpack_list_url.clone().unwrap_or_default(),
            java_edit: settings.java_path.clone().unwrap_or_default(),
            paths,
            settings,
            settings_dirty: None,
            installed,
            total_ram_mb: system::total_ram_mb(),
            packs: Vec::new(),
            packs_state: LoadState::Loading,
            packs_error: None,
            packs_from_cache: false,
            page: Page::Home,
            drawer_open: false,
            job: None,
            game: None,
            last_crash: None,
            logs: VecDeque::new(),
            log_filter: String::new(),
            log_min_level: LogLevel::Info,
            log_autoscroll: true,
            log_cache: None,
            log_generation: 0,
            unseen_errors: 0,
            toasts: Vec::new(),
            confirm: None,
            update: UpdateState::None,
            show_update_dialog: false,
            last_update_check: Instant::now(),
            page_since: Instant::now(),
            skins: HashMap::new(),
            generated_heads: HashMap::new(),
            name_edited_at: None,
            head_drag: egui::Vec2::ZERO,
            hero_anim: None,
            uninstall_delete_data: false,
            skip_save: false,
            install_dir_edit: match &startup.offer {
                Offer::Install { suggested } => suggested.display().to_string(),
                _ => String::new(),
            },
            install_offer: startup.offer.clone(),
            install_error: None,
            background,
            app_icon,
            rt,
            tx,
            rx,
            reporter,
        };
        app.reporter.info(format!("Akulav Launcher {} by {} — {}", update::CURRENT_VERSION, update::AUTHOR, update::REPO_URL));
        for note in &startup.notes {
            app.reporter.info(note.clone());
        }
        // While the install offer is up nothing is created or downloaded: if the user installs,
        // the installed copy starts with its own data folder.
        if app.install_offer == Offer::None {
            app.start_normally();
        }
        if let Some(from) = &startup.updated_from {
            app.toast(ToastKind::Success, format!("Updated to {}", update::CURRENT_VERSION), format!("from version {from}. Enjoy!"));
        } else if startup.installed {
            let where_ = if cfg!(windows) { "Find it on your Desktop and in the Start menu." } else { "Find it in your app menu and on your Desktop." };
            app.toast(ToastKind::Success, "Akulav Launcher is installed", where_);
        }
        app
    }

    fn start_normally(&mut self) {
        let _ = self.paths.ensure();
        self.reporter.info(format!("Data folder: {}", self.paths.root.display()));
        self.reload_modpacks();
        jobs::spawn_update_check(self.rt.handle(), self.reporter.clone());
        self.last_update_check = Instant::now();
    }

    pub fn list_url(&self) -> String {
        self.settings
            .modpack_list_url
            .clone()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODPACK_LIST_URL.to_string())
    }

    pub fn reload_modpacks(&mut self) {
        self.packs_state = LoadState::Loading;
        jobs::spawn_load_modpacks(self.rt.handle(), self.list_url(), self.paths.clone(), self.reporter.clone());
    }

    pub fn mark_dirty(&mut self) {
        self.settings_dirty.get_or_insert_with(Instant::now);
    }

    pub fn click(&self) {
        if self.settings.click_sound {
            system::play_click();
        }
    }

    pub fn selected_pack(&self) -> Option<&Modpack> {
        let name = self.settings.selected_pack.as_deref();
        self.packs.iter().find(|p| Some(p.name.as_str()) == name).or_else(|| self.packs.first())
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn toast(&mut self, kind: ToastKind, title: impl Into<String>, body: impl Into<String>) {
        self.toasts.push(Toast { kind, title: title.into(), body: body.into(), created: Instant::now(), action: None });
    }

    pub fn toast_with(&mut self, kind: ToastKind, title: impl Into<String>, body: impl Into<String>, label: &str, action: ToastAction) {
        self.toasts.push(Toast {
            kind,
            title: title.into(),
            body: body.into(),
            created: Instant::now(),
            action: Some((label.to_string(), action)),
        });
    }

    /// Start preparing (and optionally launching) the selected pack.
    pub fn start_job(&mut self, ctx: &egui::Context, kind: JobKind) {
        if self.busy() {
            return;
        }
        let Some(pack) = self.selected_pack().cloned() else { return };
        if kind == JobKind::Launch {
            if let Err(msg) = validate_username(&self.settings.username) {
                self.toast(ToastKind::Warning, "Check your nickname", msg);
                return;
            }
            if self.game.is_some() {
                self.toast(ToastKind::Info, "Already playing", "Close the running game first.");
                return;
            }
        }
        let total = self.total_ram_mb;
        let pack_settings = self.settings.pack(&pack.name, total).clone();
        let cancel = Cancel::default();
        let req = LaunchRequest {
            pack: pack.clone(),
            username: self.settings.username.clone(),
            settings: pack_settings.clone(),
            java_override: self.settings.java_path.clone().filter(|p| !p.trim().is_empty()).map(PathBuf::from),
            concurrency: self.settings.concurrent_downloads.clamp(1, 64),
            repair: kind == JobKind::Repair,
            launch: kind == JobKind::Launch,
        };
        self.job = Some(JobState {
            cancel: cancel.clone(),
            pack: pack.name.clone(),
            kind,
            stage: "Starting".into(),
            progress: None,
            rate: RateMeter::default(),
        });
        self.last_crash = None;
        self.reporter.info(format!(
            "{} {} {} as {}",
            if kind == JobKind::Repair { "Repairing" } else { "Launching" },
            pack.name,
            pack.version,
            self.settings.username
        ));
        jobs::spawn_job(self.rt.handle(), req, self.paths.clone(), self.reporter.clone(), cancel);
        ctx.request_repaint();
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Stage(s) => {
                    if let Some(j) = &mut self.job {
                        j.stage = s;
                        j.progress = None;
                        j.rate = RateMeter::default();
                    }
                }
                Event::Progress(p) => {
                    if let Some(j) = &mut self.job {
                        if let Some(p) = &p {
                            j.rate.sample(p);
                        }
                        j.progress = p;
                    }
                }
                Event::Log(line) => self.push_log(line),
                Event::ModpacksLoaded { result, from_cache } => match result {
                    Ok(list) => {
                        self.packs = list;
                        self.packs_state = LoadState::Ready;
                        self.packs_error = None;
                        self.packs_from_cache = from_cache;
                        if from_cache {
                            self.toast(ToastKind::Warning, "Offline", "Couldn't reach the modpack server. Showing the last known list.");
                        }
                    }
                    Err(e) => {
                        self.packs_state = LoadState::Failed;
                        self.packs_error = Some(e);
                    }
                },
                Event::UpdateAvailable(info) => {
                    // Don't interrupt a download that's already in flight.
                    if matches!(self.update, UpdateState::None | UpdateState::Available(_) | UpdateState::Failed { .. }) {
                        self.reporter.info(format!("Launcher update available: {}", info.version));
                        if !matches!(&self.update, UpdateState::Available(i) if i.version == info.version) {
                            self.toast_with(ToastKind::Info, format!("Launcher {} is available", info.version), "Update in one click, no installer needed.", "See update", ToastAction::ShowUpdate);
                        }
                        self.update = UpdateState::Available(info);
                    }
                }
                Event::UpdateProgress { done, total } => {
                    if let UpdateState::Downloading { done: d, total: t, .. } = &mut self.update {
                        *d = done;
                        *t = total;
                    }
                }
                Event::UpdateDownloaded(path) => {
                    if let Some(info) = self.update.info().cloned() {
                        self.reporter.info(format!("Launcher {} downloaded and verified", info.version));
                        self.update = UpdateState::Ready { info, path };
                        self.show_update_dialog = true;
                    }
                }
                Event::UpdateFailed(error) => {
                    if let Some(info) = self.update.info().cloned() {
                        self.reporter.error(format!("Launcher update failed: {error}"));
                        self.update = UpdateState::Failed { info, error };
                    }
                }
                Event::SkinLoaded { name, skin } => {
                    let key = name.to_ascii_lowercase();
                    let entry = match skin {
                        Some(s) => {
                            let img = egui::ColorImage::from_rgba_unmultiplied([64, 64], &s.rgba);
                            let texture = ctx.load_texture(format!("skin-{key}"), img, egui::TextureOptions::NEAREST);
                            SkinEntry::Ready { texture, source: s.source }
                        }
                        None => SkinEntry::Missing,
                    };
                    self.skins.insert(key, entry);
                }
                Event::InstalledChanged => self.installed = InstalledDb::load(&self.paths),
                Event::GameStarted { pid } => {
                    let Some(job) = self.job.take() else { continue };
                    let total = self.total_ram_mb;
                    let after = self.settings.pack(&job.pack, total).after_launch;
                    self.settings.pack(&job.pack, total).last_played = Some(chrono::Utc::now());
                    self.mark_dirty();
                    self.game = Some(GameState { pack: job.pack, pid, started: Instant::now(), after_launch: after });
                    self.installed = InstalledDb::load(&self.paths);
                    match after {
                        AfterLaunch::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                        AfterLaunch::Close => {
                            self.save_settings_now();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        AfterLaunch::KeepOpen => {}
                    }
                }
                Event::JobFailed { message, cancelled } => {
                    self.job = None;
                    if cancelled {
                        self.toast(ToastKind::Info, "Cancelled", "");
                    } else {
                        self.toast_with(ToastKind::Error, "Something went wrong", message, "Open console", ToastAction::ShowConsole);
                    }
                }
                Event::JobDone { message } => {
                    self.job = None;
                    self.installed = InstalledDb::load(&self.paths);
                    self.toast(ToastKind::Success, "Done", message);
                }
                Event::GameExited { code, crash_report, played_secs } => {
                    if let Some(g) = self.game.take() {
                        let total = self.total_ram_mb;
                        self.settings.pack(&g.pack, total).play_seconds += played_secs;
                        self.mark_dirty();
                        if g.after_launch == AfterLaunch::Minimize {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        }
                    }
                    self.reporter.info(format!("Game exited with code {}", code.map(|c| c.to_string()).unwrap_or("?".into())));
                    let crashed = code.is_some_and(|c| c != 0);
                    if crashed {
                        self.last_crash = crash_report.clone();
                        match crash_report {
                            Some(path) => self.toast_with(
                                ToastKind::Error,
                                "The game crashed",
                                format!("Exit code {}. A crash report was saved.", code.unwrap_or(-1)),
                                "Open crash report",
                                ToastAction::OpenFile(path),
                            ),
                            None => self.toast_with(
                                ToastKind::Error,
                                "The game crashed",
                                format!("Exit code {}. Check the console for details.", code.unwrap_or(-1)),
                                "Open console",
                                ToastAction::ShowConsole,
                            ),
                        }
                    }
                }
            }
        }
    }

    fn push_log(&mut self, line: LogLine) {
        if line.level >= LogLevel::Error && self.page != Page::Console && line.source != LogSource::Game {
            self.unseen_errors += 1;
        }
        self.logs.push_back(line);
        if self.logs.len() > MAX_LOG_LINES {
            let excess = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(..excess);
            self.log_generation += 1; // indices shifted: invalidate the filter cache
        }
    }

    pub fn save_settings_now(&mut self) {
        if let Err(e) = self.settings.save(&self.paths) {
            self.reporter.error(format!("Could not save settings: {e:#}"));
        }
        self.settings_dirty = None;
    }

    // -----------------------------------------------------------------------------------------
    // Chrome
    // -----------------------------------------------------------------------------------------

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let drag = ui.interact(rect, Id::new("title_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }

        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            if let Some(icon) = &self.app_icon {
                ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(20.0, 20.0)));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("AKULAV").font(black(13.5)).color(TEXT).extra_letter_spacing(1.5));
            ui.label(RichText::new("LAUNCHER").font(semibold(13.0)).color(MUTED).extra_letter_spacing(1.5));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let close = window_button(ui, ph::X, true);
                if close.clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                if window_button(ui, if max { ph::CORNERS_IN } else { ph::CORNERS_OUT }, false).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                if window_button(ui, ph::MINUS, false).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                ui.add_space(10.0);
                let label = match &self.update {
                    UpdateState::None => None,
                    UpdateState::Available(i) => Some((format!("Update {} available", i.version), GOLD)),
                    UpdateState::Downloading { done, total, .. } => Some((
                        if *total > 0 { format!("Downloading update {:.0}%", *done as f32 * 100.0 / *total as f32) } else { "Downloading update…".into() },
                        BLUE,
                    )),
                    UpdateState::Ready { .. } => Some(("Restart to update".into(), GREEN)),
                    UpdateState::Failed { .. } => Some(("Update failed".into(), ACCENT_HI)),
                };
                if let Some((text, color)) = label {
                    let r = chip(ui, ph::ARROW_CIRCLE_UP, &text, color)
                        .interact(Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        self.show_update_dialog = true;
                    }
                }
            });
        });
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, BORDER));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        let errors = self.unseen_errors;
        let playing = self.game.is_some();
        for (page, icon, label) in [(Page::Home, ph::HOUSE, "Play"), (Page::Console, ph::TERMINAL_WINDOW, "Console"), (Page::Settings, ph::GEAR_SIX, "Settings")] {
            let badge = match page {
                Page::Console if errors > 0 => Some((errors.min(99).to_string(), ACCENT)),
                Page::Console if playing => Some(("●".to_string(), GREEN)),
                _ => None,
            };
            if nav_item(ui, icon, label, self.page == page, badge).clicked() && self.page != page {
                self.click();
                self.page = page;
                self.page_since = Instant::now();
                if page == Page::Console {
                    self.unseen_errors = 0;
                }
            }
            ui.add_space(4.0);
        }

        // Profile card pinned to the bottom.
        let bottom = ui.max_rect().bottom();
        let card_h = 68.0;
        let card = Rect::from_min_max(pos2(ui.max_rect().left() + 12.0, bottom - card_h - 34.0), pos2(ui.max_rect().right() - 12.0, bottom - 34.0));
        ui.painter().rect_filled(card, CornerRadius::same(R_MD), SURFACE_2);
        ui.painter().rect_stroke(card, CornerRadius::same(R_MD), Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
        let avatar = Rect::from_min_size(card.min + vec2(8.0, 8.0), vec2(52.0, 52.0));
        let head_resp = ui.interact(avatar, Id::new("sidebar_head"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(head_resp.id, head_resp.hovered(), 0.3);
        let t = ui.input(|i| i.time) as f32;
        let yaw = -0.6 + hover * (t * 1.6).sin() * 0.55;
        let (tex, _) = self.head_texture(ui.ctx());
        super::head::paint_head_3d(ui.painter(), avatar, tex, yaw, 0.35);
        if hover > 0.0 {
            ui.ctx().request_repaint();
        }
        if head_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Edit profile").clicked() {
            self.page = Page::Settings;
            self.page_since = Instant::now();
        }
        let avatar = Rect::from_min_size(card.min + vec2(12.0, 14.0), vec2(40.0, 40.0));

        let field_rect = Rect::from_min_max(pos2(avatar.right() + 10.0, avatar.top() - 2.0), pos2(card.right() - 10.0, avatar.top() + 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field_rect));
        let valid = validate_username(&self.settings.username);
        let editable = !self.busy() && self.game.is_none();
        let edit = egui::TextEdit::singleline(&mut self.settings.username)
            .font(semibold(14.5))
            .frame(false)
            .char_limit(16)
            .hint_text("Nickname")
            .desired_width(field_rect.width())
            .text_color(if valid.is_ok() { TEXT } else { ACCENT_HI });
        let resp = child.add_enabled(editable, edit).on_hover_text("Your in-game name (3–16 letters, digits or _)");
        if resp.changed() {
            self.name_changed();
        }
        let sub = match valid {
            Ok(()) => ("Offline account", MUTED),
            Err(e) => (e, ACCENT_HI),
        };
        let sub_rect = Rect::from_min_max(pos2(field_rect.left(), avatar.top() + 24.0), pos2(card.right() - 10.0, card.bottom() - 6.0));
        let galley = ui.painter().layout(sub.0.to_string(), regular(11.5), sub.1, sub_rect.width());
        ui.painter().galley(sub_rect.min, galley, sub.1);

        // Credit line: version and author, links to the repository.
        let credit = Rect::from_center_size(pos2(ui.max_rect().center().x, bottom - 16.0), vec2(ui.max_rect().width() - 24.0, 20.0));
        let credit_resp = ui.interact(credit, Id::new("sidebar_credit"), Sense::click());
        let credit_color = if credit_resp.hovered() { MUTED } else { FAINT };
        ui.painter().text(
            credit.center(),
            Align2::CENTER_CENTER,
            format!("v{}  ·  made by {} {}", update::CURRENT_VERSION, update::AUTHOR, ph::HEART),
            regular(11.0),
            if credit_resp.hovered() { ACCENT_HI } else { credit_color },
        );
        if credit_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Akulav Launcher on GitHub").clicked() {
            let _ = open::that_detached(update::REPO_URL);
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        const LIFETIME: f32 = 7.0;
        self.toasts.retain(|t| t.created.elapsed().as_secs_f32() < LIFETIME);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.screen_rect();
        let mut y = screen.top() + 56.0;
        let mut remove = None;
        let mut action_taken = None;
        for (i, t) in self.toasts.iter().enumerate().rev().take(4) {
            let age = t.created.elapsed().as_secs_f32();
            let alpha = ((LIFETIME - age) / 0.4).clamp(0.0, 1.0) * (age / 0.15).clamp(0.0, 1.0);
            let (color, icon) = match t.kind {
                ToastKind::Info => (BLUE, ph::INFO),
                ToastKind::Success => (GREEN, ph::CHECK_CIRCLE),
                ToastKind::Warning => (GOLD, ph::WARNING),
                ToastKind::Error => (ACCENT_HI, ph::WARNING_CIRCLE),
            };
            let width = 360.0;
            let body_galley = ctx.fonts(|f| f.layout(t.body.clone(), regular(12.5), MUTED, width - 60.0));
            let h = 46.0 + if t.body.is_empty() { 0.0 } else { body_galley.size().y + 4.0 } + if t.action.is_some() { 30.0 } else { 0.0 };
            let rect = Rect::from_min_size(pos2(screen.right() - width - 20.0, y), vec2(width, h));
            y += h + 10.0;
            egui::Area::new(Id::new(("toast", t.created)))
                .order(egui::Order::Tooltip)
                .fixed_pos(rect.min)
                .show(ctx, |ui| {
                    ui.set_opacity(alpha);
                    let p = ui.painter();
                    p.rect_filled(rect.translate(vec2(0.0, 4.0)), CornerRadius::same(R_MD), Color32::from_black_alpha(90));
                    p.rect_filled(rect, CornerRadius::same(R_MD), SURFACE_2);
                    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
                    p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(3.0, rect.height() - 20.0)), CornerRadius::same(2), color);
                    p.text(rect.min + vec2(18.0, 23.0), Align2::LEFT_CENTER, icon, regular(18.0), color);
                    p.text(rect.min + vec2(46.0, 23.0), Align2::LEFT_CENTER, &t.title, semibold(14.0), TEXT);
                    if !t.body.is_empty() {
                        p.galley(rect.min + vec2(46.0, 38.0), body_galley.clone(), MUTED);
                    }
                    let close = Rect::from_center_size(pos2(rect.right() - 20.0, rect.top() + 23.0), vec2(22.0, 22.0));
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(close));
                    if icon_button(&mut child, ph::X, 22.0, "").clicked() {
                        remove = Some(i);
                    }
                    if let Some((label, action)) = &t.action {
                        let r = Rect::from_min_size(pos2(rect.left() + 46.0, rect.bottom() - 34.0), vec2(200.0, 26.0));
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
                        let resp = child.add(egui::Button::new(RichText::new(label).font(semibold(12.5)).color(color)).frame(false));
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            action_taken = Some((i, action.clone()));
                        }
                    }
                    // keep hovered toasts alive
                });
        }
        if let Some((i, action)) = action_taken {
            match action {
                ToastAction::OpenFile(p) => system::open_path(&p),
                ToastAction::ShowUpdate => self.show_update_dialog = true,
                ToastAction::ShowConsole => {
                    self.page = Page::Console;
                    self.unseen_errors = 0;
                }
            }
            self.toasts.remove(i);
        } else if let Some(i) = remove {
            self.toasts.remove(i);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(confirm) = &self.confirm else { return };
        let (title, body, danger, label) = match confirm {
            Confirm::Uninstall(name) => (
                format!("Uninstall {name}?"),
                "This deletes the pack's folder, including worlds, screenshots and settings saved inside it. This can't be undone.".to_string(),
                true,
                "Uninstall",
            ),
            Confirm::ResetSettings => ("Reset all settings?".into(), "Nickname, modpack source and every pack's settings go back to defaults. Installed packs are kept.".into(), true, "Reset"),
            Confirm::UninstallLauncher if install::UNINSTALL_ASKS_ABOUT_DATA => (
                "Uninstall Akulav Launcher?".into(),
                "The launcher closes and its uninstaller starts. It removes the launcher and its shortcuts, and asks whether to keep your modpacks and worlds.".into(),
                true,
                "Uninstall",
            ),
            Confirm::UninstallLauncher => (
                "Uninstall Akulav Launcher?".into(),
                "Removes the launcher and its shortcuts. The launcher closes afterwards.".into(),
                true,
                "Uninstall",
            ),
        };
        let uninstalling = matches!(confirm, Confirm::UninstallLauncher) && !install::UNINSTALL_ASKS_ABOUT_DATA;
        let delete_data = &mut self.uninstall_delete_data;
        let mut result: Option<bool> = None;
        let modal = egui::Modal::new(Id::new("confirm")).frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(CornerRadius::same(R_LG)).inner_margin(Margin::same(24)),
        );
        let resp = modal.show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(RichText::new(title).font(bold(18.0)));
            ui.add_space(4.0);
            ui.label(RichText::new(body).color(MUTED));
            if uninstalling {
                ui.add_space(8.0);
                ui.checkbox(delete_data, "Also delete modpacks, worlds and settings");
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ok = if danger { danger_button(ui, ph::TRASH, label) } else { primary_button(ui, label, vec2(130.0, 34.0), true) };
                    if ok.clicked() {
                        result = Some(true);
                    }
                    if ghost_button(ui, "", "Cancel").clicked() {
                        result = Some(false);
                    }
                });
            });
        });
        if resp.should_close() && result.is_none() {
            result = Some(false);
        }
        if let Some(yes) = result {
            let confirm = self.confirm.take().unwrap();
            if yes {
                self.click();
                match confirm {
                    Confirm::Uninstall(name) => match jobs::uninstall_pack(&self.paths, &name) {
                        Ok(()) => {
                            self.installed = InstalledDb::load(&self.paths);
                            self.drawer_open = false;
                            self.toast(ToastKind::Success, "Uninstalled", name);
                        }
                        Err(e) => self.toast(ToastKind::Error, "Could not uninstall", format!("{e:#}")),
                    },
                    Confirm::ResetSettings => {
                        let selected = self.settings.selected_pack.clone();
                        self.settings = Settings { selected_pack: selected, ..Default::default() };
                        self.list_url_edit.clear();
                        self.java_edit.clear();
                        self.save_settings_now();
                        self.reload_modpacks();
                        self.toast(ToastKind::Success, "Settings reset", "");
                    }
                    Confirm::UninstallLauncher => {
                        // Windows keeps data unless its uninstaller is told otherwise: save first.
                        if install::UNINSTALL_ASKS_ABOUT_DATA {
                            self.save_settings_now();
                        }
                        match install::uninstall(self.uninstall_delete_data) {
                            Ok(()) => {
                                self.skip_save = true;
                                self.settings_dirty = None;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            Err(e) => self.toast(ToastKind::Error, "Could not uninstall", format!("{e:#}")),
                        }
                    }
                }
            }
        }
    }

    pub fn name_changed(&mut self) {
        self.name_edited_at = Some(Instant::now());
        self.mark_dirty();
    }

    /// Texture to draw the current player's head with: the real skin when found, otherwise a
    /// generated one. Also returns where the real skin came from.
    pub fn head_texture(&mut self, ctx: &egui::Context) -> (egui::TextureId, Option<SkinSource>) {
        let key = self.settings.username.to_ascii_lowercase();
        if let Some(SkinEntry::Ready { texture, source }) = self.skins.get(&key) {
            return (texture.id(), Some(*source));
        }
        let tex = self.generated_heads.entry(key.clone()).or_insert_with(|| {
            let rgba = super::head::procedural_skin(&key);
            ctx.load_texture(format!("gen-{key}"), egui::ColorImage::from_rgba_unmultiplied([64, 64], &rgba), egui::TextureOptions::NEAREST)
        });
        (tex.id(), None)
    }

    pub fn skin_status(&self) -> Option<&SkinEntry> {
        self.skins.get(&self.settings.username.to_ascii_lowercase())
    }

    /// Look the nickname's skin up once the user stops typing.
    fn tick_skin_lookup(&mut self, ctx: &egui::Context) {
        if validate_username(&self.settings.username).is_err() {
            return;
        }
        if let Some(t) = self.name_edited_at {
            let wait = Duration::from_millis(650);
            if t.elapsed() < wait {
                ctx.request_repaint_after(wait - t.elapsed());
                return;
            }
        }
        let key = self.settings.username.to_ascii_lowercase();
        if let std::collections::hash_map::Entry::Vacant(e) = self.skins.entry(key) {
            e.insert(SkinEntry::Loading);
            jobs::spawn_skin_lookup(self.rt.handle(), self.settings.username.clone(), self.paths.clone(), self.reporter.clone());
        }
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_update_dialog {
            return;
        }
        let Some(info) = self.update.info().cloned() else {
            self.show_update_dialog = false;
            return;
        };
        let busy = self.job.is_some() || self.game.is_some();
        let mut close = false;
        let modal = egui::Modal::new(Id::new("update_dialog")).frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(CornerRadius::same(R_LG)).inner_margin(Margin::same(26)),
        );
        let resp = modal.show(ctx, |ui| {
            ui.set_width(430.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(R_MD), with_alpha(ACCENT, 40));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, ph::ROCKET_LAUNCH, regular(24.0), ACCENT_HI);
                ui.vertical(|ui| {
                    label_kicker(ui, "Launcher update");
                    ui.label(RichText::new(format!("Version {}", info.version)).font(bold(20.0)));
                });
            });
            ui.add_space(6.0);
            let how = if cfg!(windows) {
                "The update downloads in the background, then installs into the same folder and restarts the launcher. Nothing to click through."
            } else {
                "The update downloads in the background and replaces this launcher in place. Nothing to click through."
            };
            ui.label(RichText::new(format!("You have {}. {how}", update::CURRENT_VERSION)).color(MUTED));
            ui.add_space(14.0);
            match &self.update {
                UpdateState::Available(_) | UpdateState::None => {
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Download update", ph::DOWNLOAD_SIMPLE), vec2(190.0, 38.0), true).clicked() {
                                self.update = UpdateState::Downloading { info: info.clone(), done: 0, total: 0 };
                                jobs::spawn_update_download(self.rt.handle(), info.clone(), self.reporter.clone());
                            }
                            if ghost_button(ui, "", "Later").clicked() {
                                close = true;
                            }
                        });
                    });
                }
                UpdateState::Downloading { done, total, .. } => {
                    let frac = (*total > 0).then(|| *done as f32 / *total as f32);
                    ui.label(RichText::new(match frac {
                        Some(f) => format!("Downloading… {:.0}%  ·  {} / {}", f * 100.0, system::format_bytes(*done), system::format_bytes(*total)),
                        None => format!("Downloading… {}", system::format_bytes(*done)),
                    }).font(semibold(13.5)));
                    progress_bar(ui, frac, 6.0);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ghost_button(ui, "", "Hide").clicked() {
                                close = true;
                            }
                        });
                    });
                }
                UpdateState::Ready { path, .. } => {
                    ui.label(RichText::new(format!("{}  Downloaded and verified", ph::CHECK_CIRCLE)).font(semibold(14.0)).color(GREEN));
                    if busy {
                        ui.label(RichText::new("Finish playing first — restarting now would stop tracking the running game.").font(regular(12.5)).color(GOLD));
                    }
                    ui.add_space(8.0);
                    let path = path.clone();
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Restart & update", ph::ARROW_CLOCKWISE), vec2(190.0, 38.0), !busy).clicked() {
                                self.save_settings_now();
                                match install::apply_update(&path) {
                                    Ok(()) => {
                                        self.skip_save = true;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                                    }
                                    Err(e) => {
                                        self.update = UpdateState::Failed { info: info.clone(), error: format!("{e:#}") };
                                    }
                                }
                            }
                            if ghost_button(ui, "", "Later").clicked() {
                                close = true;
                            }
                        });
                    });
                }
                UpdateState::Failed { error, .. } => {
                    ui.label(RichText::new(error).color(ACCENT_HI).font(regular(12.5)));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Try again", ph::ARROW_CLOCKWISE), vec2(150.0, 38.0), true).clicked() {
                                self.update = UpdateState::Downloading { info: info.clone(), done: 0, total: 0 };
                                jobs::spawn_update_download(self.rt.handle(), info.clone(), self.reporter.clone());
                            }
                            if ghost_button(ui, "", "Close").clicked() {
                                close = true;
                            }
                        });
                    });
                }
            }
        });
        if close || resp.should_close() {
            self.show_update_dialog = false;
        }
    }

    /// Linux/macOS, started outside the install folder: install (or update the installed copy)
    /// into a folder of the user's choice, or just run from here.
    fn install_screen(&mut self, ui: &mut egui::Ui) {
        let offer = self.install_offer.clone();
        let updating = matches!(offer, Offer::Update { .. });
        let (mut go, mut skip) = (false, false);
        ui.add_space((ui.available_height() * 0.5 - 200.0).max(24.0));
        ui.vertical_centered(|ui| {
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0_f32, BORDER))
                .corner_radius(CornerRadius::same(R_LG))
                .inner_margin(Margin::same(28))
                .show(ui, |ui| {
                    ui.set_width(520.0_f32.min(ui.available_width()));
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            if let Some(icon) = &self.app_icon {
                                ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(46.0, 46.0)));
                            }
                            ui.vertical(|ui| {
                                label_kicker(ui, &format!("Version {}  ·  made by {}", update::CURRENT_VERSION, update::AUTHOR));
                                let title = if updating { "Update Akulav Launcher" } else { "Install Akulav Launcher" };
                                ui.label(RichText::new(title).font(bold(20.0)));
                            });
                        });
                        ui.add_space(10.0);
                        match &offer {
                            Offer::Install { .. } => {
                                ui.label(
                                    RichText::new("Choose a folder. The launcher and everything it downloads (modpacks, worlds, Java) go there, and updates keep using it.")
                                        .color(MUTED),
                                );
                                ui.add_space(14.0);
                                ui.label(RichText::new("Install folder").font(semibold(13.0)));
                                ui.add_space(4.0);
                                let edit = egui::TextEdit::singleline(&mut self.install_dir_edit)
                                    .desired_width(f32::INFINITY)
                                    .font(regular(13.5))
                                    .margin(Margin::symmetric(10, 8));
                                if ui.add(edit).changed() {
                                    self.install_error = None;
                                }
                                ui.add_space(4.0);
                                let extras = if cfg!(target_os = "macos") {
                                    "Creates Akulav Launcher.app in this folder."
                                } else {
                                    "Also adds an app-menu entry, a Desktop shortcut and the akulav-launcher command."
                                };
                                ui.label(RichText::new(extras).font(regular(12.0)).color(FAINT));
                            }
                            Offer::Update { root, installed } => {
                                let from = installed.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "an older version".into());
                                ui.label(
                                    RichText::new(format!(
                                        "Akulav Launcher {from} is installed in {}. Update it to {}? Your modpacks, worlds and settings stay where they are.",
                                        root.display(),
                                        update::CURRENT_VERSION
                                    ))
                                    .color(MUTED),
                                );
                            }
                            Offer::None => {}
                        }
                        if let Some(err) = &self.install_error {
                            ui.add_space(8.0);
                            ui.label(RichText::new(err).font(regular(12.5)).color(ACCENT_HI));
                        }
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let label = if updating { format!("{}  Update", ph::ARROW_CIRCLE_UP) } else { format!("{}  Install", ph::DOWNLOAD_SIMPLE) };
                                if primary_button(ui, &label, vec2(150.0, 38.0), true).clicked() {
                                    go = true;
                                }
                                if ghost_button(ui, "", "Just run it").on_hover_text("Run this copy without installing it").clicked() {
                                    skip = true;
                                }
                            });
                        });
                    });
                });
        });
        if go {
            self.click();
            self.run_install(ui.ctx());
        } else if skip {
            self.click();
            self.install_offer = Offer::None;
            self.start_normally();
        }
    }

    fn run_install(&mut self, ctx: &egui::Context) {
        let result = match self.install_offer.clone() {
            Offer::Install { .. } => install::parse_install_dir(&self.install_dir_edit)
                .and_then(|dir| install::install_and_launch(&dir, &["--installed"]).map_err(|e| format!("Could not install: {e:#}"))),
            Offer::Update { root, installed } => {
                let from = installed.unwrap_or_default();
                let args: Vec<&str> = if from.is_empty() { Vec::new() } else { vec!["--updated-from", from.as_str()] };
                install::install_and_launch(&root, &args).map_err(|e| format!("Could not update: {e:#}"))
            }
            Offer::None => return,
        };
        match result {
            Ok(()) => {
                self.skip_save = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.install_error = Some(e),
        }
    }

    fn resize_handles(&self, ctx: &egui::Context) {
        use egui::viewport::ResizeDirection as D;
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let r = ctx.screen_rect();
        let t = 5.0;
        let zones = [
            (Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + t)), D::North, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(pos2(r.left(), r.bottom() - t), r.right_bottom()), D::South, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(r.left_top(), pos2(r.left() + t, r.bottom())), D::West, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(pos2(r.right() - t, r.top()), r.right_bottom()), D::East, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_size(r.left_top(), vec2(t * 2.0, t * 2.0)), D::NorthWest, egui::CursorIcon::ResizeNwSe),
            (Rect::from_min_size(pos2(r.right() - t * 2.0, r.top()), vec2(t * 2.0, t * 2.0)), D::NorthEast, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_size(pos2(r.left(), r.bottom() - t * 2.0), vec2(t * 2.0, t * 2.0)), D::SouthWest, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_size(r.right_bottom() - vec2(t * 3.0, t * 3.0), vec2(t * 3.0, t * 3.0)), D::SouthEast, egui::CursorIcon::ResizeNwSe),
        ];
        egui::Area::new(Id::new("resize_handles")).order(egui::Order::Debug).fixed_pos(r.min).interactable(true).show(ctx, |ui| {
            for (i, (rect, dir, cursor)) in zones.into_iter().enumerate().rev() {
                let resp = ui.interact(rect, Id::new(("resize", i)), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ctx.set_cursor_icon(cursor);
                }
                if resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            }
        });
    }
}

impl eframe::App for LauncherApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [BG.r() as f32 / 255.0, BG.g() as f32 / 255.0, BG.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events(ctx);
        if self.install_offer != Offer::None {
            egui::TopBottomPanel::top("title_bar")
                .exact_height(42.0)
                .frame(Frame::new().fill(SURFACE))
                .show(ctx, |ui| self.title_bar(ui));
            egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| self.install_screen(ui));
            self.toasts_ui(ctx);
            self.resize_handles(ctx);
            return;
        }
        self.tick_skin_lookup(ctx);
        if self.last_update_check.elapsed() > Duration::from_secs(2 * 3600) {
            self.last_update_check = Instant::now();
            jobs::spawn_update_check(self.rt.handle(), self.reporter.clone());
        }

        egui::TopBottomPanel::top("title_bar")
            .exact_height(42.0)
            .frame(Frame::new().fill(SURFACE))
            .show(ctx, |ui| self.title_bar(ui));

        egui::SidePanel::left("sidebar")
            .exact_width(228.0)
            .resizable(false)
            .frame(Frame::new().fill(SURFACE).inner_margin(Margin { left: 12, right: 12, top: 0, bottom: 0 }))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().vline(r.right() + 11.5, r.y_range(), Stroke::new(1.0_f32, BORDER));
                self.sidebar(ui)
            });

        if self.page == Page::Home {
            let open = self.drawer_open && self.selected_pack().is_some();
            egui::SidePanel::right("pack_drawer")
                .exact_width(360.0)
                .resizable(false)
                .frame(Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, BORDER)).inner_margin(Margin::same(22)))
                .show_animated(ctx, open, |ui| self.pack_drawer(ui));
        }

        // Pages fade in when switched.
        let fade = (self.page_since.elapsed().as_secs_f32() / 0.22).min(1.0);
        let fade = 1.0 - (1.0 - fade).powi(3);
        if fade < 1.0 {
            ctx.request_repaint();
        }
        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| {
            ui.set_opacity(fade);
            match self.page {
                Page::Home => self.home_page(ui),
                Page::Console => self.console_page(ui),
                Page::Settings => self.settings_page(ui),
            }
        });
        self.update_dialog(ctx);

        // Keep the living wallpaper moving only while someone can see it.
        let visible = ctx.input(|i| i.focused && !i.viewport().minimized.unwrap_or(false));
        if self.page == Page::Home && self.settings.animated_background && visible && self.game.is_none() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        self.toasts_ui(ctx);
        self.confirm_ui(ctx);
        self.resize_handles(ctx);

        if self.job.is_some() || self.game.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if let Some(t) = self.settings_dirty.filter(|_| !self.skip_save) {
            if t.elapsed() > Duration::from_millis(600) {
                self.save_settings_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(700));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(j) = &self.job {
            j.cancel.cancel();
        }
        if !self.skip_save && self.install_offer == Offer::None {
            self.save_settings_now();
        }
    }
}

fn window_button(ui: &mut egui::Ui, icon: &str, is_close: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, 42.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let bg = if is_close { Color32::from_rgb(196, 43, 28) } else { Color32::from_white_alpha(14) };
    if hover > 0.0 {
        ui.painter().rect_filled(rect, 0.0, with_alpha(bg, (bg.a() as f32 * hover) as u8));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, regular(15.0), lerp_color(MUTED, TEXT, hover));
    resp
}

fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, active: bool, badge: Option<(String, Color32)>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let act = ui.ctx().animate_bool_with_time(resp.id.with("a"), active, 0.15);
    let p = ui.painter();
    let bg = lerp_color(with_alpha(Color32::WHITE, (hover * 8.0) as u8), with_alpha(ACCENT, 34), act);
    p.rect_filled(rect, CornerRadius::same(R_MD), bg);
    if act > 0.0 {
        let bar = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.0 * act), vec2(3.0, 20.0 * act));
        p.rect_filled(bar, CornerRadius::same(2), ACCENT_HI);
    }
    let fg = lerp_color(lerp_color(MUTED, TEXT, hover), TEXT, act);
    p.text(rect.left_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, icon, regular(19.0), if active { ACCENT_HI } else { fg });
    p.text(rect.left_center() + vec2(48.0, 0.0), Align2::LEFT_CENTER, label, semibold(14.5), fg);
    if let Some((text, color)) = badge {
        let g = p.layout_no_wrap(text, bold(11.0), Color32::WHITE);
        let w = (g.size().x + 12.0).max(20.0);
        let b = Rect::from_center_size(rect.right_center() - vec2(12.0 + w / 2.0, 0.0), vec2(w, 20.0));
        if color == GREEN {
            p.circle_filled(b.center(), 4.0, GREEN);
        } else {
            p.rect_filled(b, CornerRadius::same(255), color);
            p.galley(b.center() - g.size() / 2.0, g, Color32::WHITE);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}
