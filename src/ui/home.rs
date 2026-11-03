//! The Play page: hero for the selected pack, library strip, play bar, and the pack settings drawer.

use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder};
use egui_phosphor::regular as ph;

use super::app::{Confirm, JobKind, LauncherApp, LoadState, Page};
use super::theme::*;
use super::widgets::*;
use crate::launcher::events::ProgressUnit;
use crate::launcher::minecraft::loader::LoaderSpec;
use crate::launcher::modpacks::PackStatus;
use crate::launcher::settings::{recommended_ram_mb, AfterLaunch, DEFAULT_JVM_ARGS};
use crate::launcher::system::{self, format_bytes};

pub fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{secs}s")
    }
}

fn time_ago(t: chrono::DateTime<chrono::Utc>) -> String {
    let d = chrono::Utc::now().signed_duration_since(t);
    if d.num_days() > 0 {
        format!("{}d ago", d.num_days())
    } else if d.num_hours() > 0 {
        format!("{}h ago", d.num_hours())
    } else if d.num_minutes() > 0 {
        format!("{}m ago", d.num_minutes())
    } else {
        "just now".into()
    }
}

impl LauncherApp {
    pub fn home_page(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let animate = self.settings.animated_background && self.game.is_none();
        let pointer = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|p| full.contains(*p))
            .map(|p| (p - full.center()) / (full.size() / 2.0))
            .unwrap_or(egui::Vec2::ZERO);
        // Ease the parallax so the image glides instead of snapping to the cursor.
        let pointer = egui::vec2(
            ui.ctx().animate_value_with_time(egui::Id::new("par_x"), pointer.x, 0.6),
            ui.ctx().animate_value_with_time(egui::Id::new("par_y"), pointer.y, 0.6),
        );
        super::backdrop::paint(
            &ui.painter_at(full),
            full,
            self.background.as_ref(),
            &super::backdrop::BackdropParams { time: ui.input(|i| i.time), animate, pointer: if animate { pointer } else { egui::Vec2::ZERO } },
        );

        let content = full.shrink2(vec2(40.0, 32.0));
        let play_bar_h = 96.0;
        let library_h = 116.0;
        let bar_rect = Rect::from_min_max(pos2(content.left(), content.bottom() - play_bar_h), content.max);
        let library_rect = Rect::from_min_max(
            pos2(content.left(), bar_rect.top() - 18.0 - library_h),
            pos2(content.right(), bar_rect.top() - 18.0),
        );
        let hero_rect = Rect::from_min_max(content.min, pos2(content.right(), library_rect.top() - 12.0));

        match self.packs_state {
            LoadState::Loading if self.packs.is_empty() => {
                let mut ui = ui.new_child(UiBuilder::new().max_rect(hero_rect));
                ui.add_space(40.0);
                label_kicker(&mut ui, "Loading");
                ui.label(RichText::new("Fetching modpacks…").font(black(40.0)));
                ui.add_space(12.0);
                ui.set_max_width(380.0);
                progress_bar(&mut ui, None, 4.0);
                return;
            }
            LoadState::Failed if self.packs.is_empty() => {
                let mut ui = ui.new_child(UiBuilder::new().max_rect(hero_rect));
                ui.add_space(40.0);
                label_kicker(&mut ui, "Offline");
                ui.label(RichText::new("Couldn't load modpacks").font(black(40.0)));
                ui.add_space(6.0);
                ui.set_max_width(560.0);
                ui.label(RichText::new(self.packs_error.clone().unwrap_or_default()).color(MUTED));
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, &format!("{}  Try again", ph::ARROW_CLOCKWISE), vec2(150.0, 40.0), true).clicked() {
                        self.click();
                        self.reload_modpacks();
                    }
                    if ghost_button(ui, ph::GEAR_SIX, "Modpack source").clicked() {
                        self.page = Page::Settings;
                    }
                });
                return;
            }
            _ => {}
        }
        let Some(pack) = self.selected_pack().cloned() else {
            let mut ui = ui.new_child(UiBuilder::new().max_rect(hero_rect));
            ui.add_space(40.0);
            ui.label(RichText::new("No modpacks yet").font(black(40.0)));
            ui.label(RichText::new("The modpack list is empty.").color(MUTED));
            return;
        };
        let total = self.total_ram_mb;
        let status = self.installed.status(&pack);
        let spec = LoaderSpec::parse(&pack.api);
        let ps = self.settings.pack(&pack.name, total).clone();

        // ---- Hero ----------------------------------------------------------------------
        if self.hero_anim.as_ref().is_none_or(|(n, _)| *n != pack.name) {
            self.hero_anim = Some((pack.name.clone(), std::time::Instant::now()));
        }
        let hero_t = {
            let t = (self.hero_anim.as_ref().unwrap().1.elapsed().as_secs_f32() / 0.55).min(1.0);
            if t < 1.0 {
                ui.ctx().request_repaint();
            }
            1.0 - (1.0 - t).powi(3)
        };
        {
            let rect = hero_rect.translate(vec2(0.0, (1.0 - hero_t) * 24.0));
            let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::bottom_up(Align::Min)));
            ui.multiply_opacity(hero_t);
            ui.set_max_width(hero_rect.width().min(720.0));
            // bottom-up: chips, description, title, kicker
            ui.horizontal_wrapped(|ui| {
                chip(ui, ph::CUBE, &spec.display(), TEXT);
                if let Some(mc) = spec.minecraft_version().filter(|_| !matches!(spec, LoaderSpec::Vanilla(_))) {
                    chip(ui, ph::GAME_CONTROLLER, &format!("Minecraft {mc}"), TEXT);
                }
                chip(ui, ph::TAG, &format!("v{}", pack.version), TEXT);
                match &status {
                    PackStatus::UpToDate => chip(ui, ph::CHECK_CIRCLE, "Installed", GREEN),
                    PackStatus::UpdateAvailable { installed } => chip(ui, ph::ARROW_CIRCLE_UP, &format!("Update {installed} → {}", pack.version), GOLD),
                    PackStatus::NotInstalled => chip(ui, ph::DOWNLOAD_SIMPLE, "Not installed", MUTED),
                };
                if ps.play_seconds > 0 {
                    chip(ui, ph::CLOCK, &format!("{} played", format_duration(ps.play_seconds)), MUTED);
                }
            });
            ui.add_space(10.0);
            let desc = pack.description.clone().unwrap_or_else(|| match &status {
                PackStatus::NotInstalled => format!("Press Install to download {} and everything it needs. Java and Minecraft are set up automatically.", pack.name),
                PackStatus::UpdateAvailable { .. } => "A new version of this pack is out. Your worlds, screenshots and options are kept when updating.".into(),
                PackStatus::UpToDate => match ps.last_played {
                    Some(t) => format!("Last played {}. Everything is up to date.", time_ago(t)),
                    None => "Everything is installed and ready to go.".into(),
                },
            });
            ui.label(RichText::new(desc).font(regular(15.0)).color(Color32::from_rgb(196, 200, 210)));
            ui.add_space(2.0);
            ui.add(egui::Label::new(RichText::new(&pack.name).font(black(52.0)).color(TEXT)).wrap());
            ui.add_space(-6.0);
            label_kicker(&mut ui, if self.packs_from_cache { "Modpack · offline" } else { "Modpack" });
        }

        // ---- Library strip ---------------------------------------------------------------
        {
            let mut ui = ui.new_child(UiBuilder::new().max_rect(library_rect));
            ui.horizontal(|ui| {
                label_kicker(ui, "Library");
                ui.label(RichText::new(format!("{}", self.packs.len())).font(semibold(11.0)).color(FAINT));
            });
            ui.add_space(2.0);
            let packs = self.packs.clone();
            let selected = pack.name.clone();
            ScrollArea::horizontal().id_salt("library").show(&mut ui, |ui| {
                ui.horizontal(|ui| {
                    for p in &packs {
                        let st = self.installed.status(p);
                        if pack_card(ui, p, &st, p.name == selected, !self.busy()).clicked() && p.name != selected {
                            self.click();
                            self.settings.selected_pack = Some(p.name.clone());
                            self.mark_dirty();
                        }
                    }
                });
            });
        }

        // ---- Play bar ----------------------------------------------------------------------
        let p = ui.painter();
        p.rect_filled(bar_rect, CornerRadius::same(R_LG), glass());
        p.rect_stroke(bar_rect, CornerRadius::same(R_LG), Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);

        let play_w = (bar_rect.width() * 0.3).clamp(170.0, 230.0);
        let play_rect = Rect::from_min_size(pos2(bar_rect.right() - 20.0 - play_w, bar_rect.center().y - 30.0), vec2(play_w, 60.0));
        let gear_rect = Rect::from_center_size(pos2(play_rect.left() - 34.0, bar_rect.center().y), vec2(44.0, 44.0));
        let folder_rect = Rect::from_center_size(pos2(gear_rect.left() - 28.0, bar_rect.center().y), vec2(44.0, 44.0));
        let status_rect = Rect::from_min_max(pos2(bar_rect.left() + 24.0, bar_rect.top() + 16.0), pos2(folder_rect.left() - 24.0, bar_rect.bottom() - 16.0));

        {
            let mut ui = ui.new_child(UiBuilder::new().max_rect(gear_rect));
            if icon_button(&mut ui, ph::SLIDERS_HORIZONTAL, 44.0, "Pack settings").clicked() {
                self.click();
                self.drawer_open = !self.drawer_open;
            }
        }
        {
            let mut ui = ui.new_child(UiBuilder::new().max_rect(folder_rect));
            if icon_button(&mut ui, ph::FOLDER_OPEN, 44.0, "Open pack folder").clicked() {
                system::open_path(&self.paths.instance_dir(&pack.name));
            }
        }

        // The player's head on the left of the play bar.
        let head_rect = Rect::from_center_size(pos2(status_rect.left() + 26.0, bar_rect.center().y), vec2(54.0, 54.0));
        let (head_tex, _) = self.head_texture(ui.ctx());
        let t = ui.input(|i| i.time) as f32;
        let bob = if animate { (t * 0.9).sin() * 0.18 } else { 0.0 };
        super::head::paint_head_3d(ui.painter(), head_rect, head_tex, -0.55 + bob, 0.32);
        let status_rect = Rect::from_min_max(pos2(head_rect.right() + 16.0, status_rect.top()), status_rect.max);
        self.status_block(ui, status_rect, &pack.name, &status, &ps);

        let mut play_ui = ui.new_child(UiBuilder::new().max_rect(play_rect));
        self.play_button(&mut play_ui, &status);
    }

    fn status_block(&self, ui: &mut Ui, rect: Rect, _pack: &str, status: &PackStatus, ps: &crate::launcher::settings::PackSettings) {
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        if let Some(job) = &self.job {
            let (frac, detail) = match &job.progress {
                Some(p) if p.total > 0 => {
                    let f = p.done as f32 / p.total as f32;
                    let detail = match p.unit {
                        ProgressUnit::Bytes => {
                            let mut s = format!("{} / {}", format_bytes(p.done), format_bytes(p.total));
                            if job.rate.bytes_per_sec > 1.0 {
                                let eta = (p.total.saturating_sub(p.done)) as f64 / job.rate.bytes_per_sec;
                                s.push_str(&format!("  ·  {}/s  ·  {} left", format_bytes(job.rate.bytes_per_sec as u64), super::home::format_duration(eta as u64)));
                            }
                            s
                        }
                        ProgressUnit::Items => format!("{} / {} files", p.done, p.total),
                    };
                    (Some(f), detail)
                }
                Some(p) if p.unit == ProgressUnit::Bytes && p.done > 0 => (None, format_bytes(p.done)),
                _ => (None, String::new()),
            };
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(&job.stage).font(semibold(15.0))).truncate());
                if let Some(f) = frac {
                    ui.label(RichText::new(format!("{:.0}%", f * 100.0)).font(semibold(15.0)).color(ACCENT_HI));
                }
            });
            ui.add_space(2.0);
            progress_bar(&mut ui, frac, 5.0);
            ui.add_space(2.0);
            ui.add(egui::Label::new(RichText::new(detail).font(regular(12.0)).color(MUTED)).truncate());
        } else if let Some(game) = &self.game {
            let t = ui.input(|i| i.time) as f32;
            let pulse = 0.55 + 0.45 * (t * 3.0).sin().abs();
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 5.0, with_alpha(GREEN, (255.0 * pulse) as u8));
                ui.label(RichText::new(format!("Playing {}", game.pack)).font(semibold(15.0)));
            });
            ui.label(
                RichText::new(format!("Session {}  ·  as {}", format_duration(game.started.elapsed().as_secs()), self.settings.username))
                    .font(regular(12.5))
                    .color(MUTED),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            let (title, color) = match status {
                PackStatus::UpToDate => ("Ready to play".to_string(), TEXT),
                PackStatus::UpdateAvailable { .. } => ("Update available".to_string(), GOLD),
                PackStatus::NotInstalled => ("Not installed yet".to_string(), TEXT),
            };
            ui.add(egui::Label::new(RichText::new(title).font(semibold(15.0)).color(color)).truncate());
            ui.add(egui::Label::new(
                RichText::new(format!(
                    "{}  ·  {:.1} GB RAM  ·  {}",
                    self.settings.username,
                    ps.ram_mb as f32 / 1024.0,
                    if ps.custom_resolution { format!("{}×{}", ps.width, ps.height) } else { "default window".into() }
                ))
                .font(regular(12.5))
                .color(MUTED),
            ).truncate());
        }
    }

    fn play_button(&mut self, ui: &mut Ui, status: &PackStatus) {
        let rect = ui.max_rect();
        let ctx = ui.ctx().clone();
        if let Some(job) = &self.job {
            // Progress-filled button; hover to cancel.
            let resp = ui.interact(rect, ui.id().with("play"), Sense::click());
            let p = ui.painter();
            let r = CornerRadius::same(R_MD);
            p.rect_filled(rect, r, SURFACE_3);
            let frac = job.progress.as_ref().filter(|p| p.total > 0).map(|p| p.done as f32 / p.total as f32);
            if let Some(f) = frac {
                let w = (rect.width() * f).max(R_MD as f32 * 2.0);
                p.rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), r, ACCENT_LO);
            }
            p.rect_stroke(rect, r, Stroke::new(1.0_f32, with_alpha(ACCENT, 120)), StrokeKind::Inside);
            let text = if resp.hovered() {
                format!("{}  CANCEL", ph::X)
            } else if job.kind == JobKind::Repair {
                "REPAIRING…".to_string()
            } else {
                match frac {
                    Some(f) => format!("{:.0}%", f * 100.0),
                    None => "PREPARING…".to_string(),
                }
            };
            p.text(rect.center(), Align2::CENTER_CENTER, text, bold(16.0), TEXT);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                job.cancel.cancel();
            }
            return;
        }
        if self.game.is_some() {
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(GREEN, 30));
            p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(GREEN, 120)), StrokeKind::Inside);
            p.text(rect.center(), Align2::CENTER_CENTER, format!("{}  RUNNING", ph::GAME_CONTROLLER), bold(16.0), GREEN);
            return;
        }
        let label = match status {
            PackStatus::UpToDate => format!("{}  PLAY", ph::PLAY),
            PackStatus::UpdateAvailable { .. } => format!("{}  UPDATE & PLAY", ph::ARROW_CIRCLE_UP),
            PackStatus::NotInstalled => format!("{}  INSTALL", ph::DOWNLOAD_SIMPLE),
        };
        let resp = primary_button(ui, &label, rect.size(), true);
        // Breathing glow and a periodic light sweep across the button.
        if self.settings.animated_background {
            let t = ui.input(|i| i.time) as f32;
            let p = ui.painter();
            let breath = 0.5 + 0.5 * (t * 2.2).sin();
            glow(p, rect, R_MD, ACCENT, 0.35 * breath);
            let cycle = (t % 4.0) / 1.1; // sweep for ~1s every 4s
            if cycle < 1.0 {
                let x = rect.left() - 60.0 + cycle * (rect.width() + 120.0);
                let clip = p.with_clip_rect(rect.shrink(1.0));
                let mut mesh = egui::epaint::Mesh::default();
                let c0 = Color32::TRANSPARENT;
                let c1 = Color32::from_white_alpha(70);
                for (dx, c) in [(0.0, c0), (22.0, c1), (44.0, c0)] {
                    mesh.vertices.push(egui::epaint::Vertex { pos: pos2(x + dx + 18.0, rect.top()), uv: egui::epaint::WHITE_UV, color: c });
                    mesh.vertices.push(egui::epaint::Vertex { pos: pos2(x + dx, rect.bottom()), uv: egui::epaint::WHITE_UV, color: c });
                }
                for k in 0..2u32 {
                    let b = k * 2;
                    mesh.add_triangle(b, b + 1, b + 2);
                    mesh.add_triangle(b + 1, b + 3, b + 2);
                }
                clip.add(mesh);
            }
        }
        if resp.clicked() {
            self.click();
            self.start_job(&ctx, JobKind::Launch);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Pack settings drawer
    // ---------------------------------------------------------------------------------------

    pub fn pack_drawer(&mut self, ui: &mut Ui) {
        let Some(pack) = self.selected_pack().cloned() else { return };
        let total = self.total_ram_mb;
        let locked = self.busy() || self.game.as_ref().is_some_and(|g| g.pack == pack.name);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                label_kicker(ui, "Pack settings");
                ui.label(RichText::new(&pack.name).font(bold(19.0)));
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if icon_button(ui, ph::X, 32.0, "Close").clicked() {
                    self.drawer_open = false;
                }
            });
        });
        ui.add_space(10.0);

        let mut changed = false;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_enabled_ui(!locked, |ui| {
                let ps = self.settings.pack(&pack.name, total);

                // Memory
                ui.label(RichText::new(format!("{}  Memory", ph::MEMORY)).font(semibold(14.0)));
                let max_mb = (total.saturating_sub(1024)).max(2048) as u32;
                let mut gb = ps.ram_mb as f32 / 1024.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{gb:.1} GB")).font(black(26.0)).color(TEXT));
                    ui.label(RichText::new(format!("of {:.0} GB", total as f32 / 1024.0)).color(MUTED));
                });
                ui.spacing_mut().slider_width = ui.available_width() - 8.0;
                if ui
                    .add(egui::Slider::new(&mut gb, 1.0..=(max_mb as f32 / 1024.0)).step_by(0.5).show_value(false))
                    .changed()
                {
                    ps.ram_mb = (gb * 1024.0) as u32;
                    changed = true;
                }
                let rec = recommended_ram_mb(total);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Recommended {:.1} GB", rec as f32 / 1024.0)).font(regular(12.0)).color(MUTED));
                    if ps.ram_mb != rec && ui.small_button("Use").clicked() {
                        ps.ram_mb = rec;
                        changed = true;
                    }
                });
                if ps.ram_mb as u64 > total * 3 / 4 {
                    ui.label(RichText::new(format!("{}  That leaves little memory for Windows; the game may stutter.", ph::WARNING)).font(regular(12.0)).color(GOLD));
                }

                separator(ui);

                // Window
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  Custom window size", ph::MONITOR)).font(semibold(14.0)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if toggle(ui, &mut ps.custom_resolution).changed() {
                            changed = true;
                        }
                    });
                });
                if ps.custom_resolution {
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(&mut ps.width).range(640..=7680).suffix(" px")).changed();
                        ui.label("×");
                        changed |= ui.add(egui::DragValue::new(&mut ps.height).range(480..=4320).suffix(" px")).changed();
                    });
                    ui.horizontal(|ui| {
                        for (w, h) in [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)] {
                            let sel = ps.width == w && ps.height == h;
                            if ui.selectable_label(sel, format!("{w}×{h}")).clicked() {
                                ps.width = w;
                                ps.height = h;
                                changed = true;
                            }
                        }
                    });
                }

                separator(ui);

                // After launch
                ui.label(RichText::new(format!("{}  When the game starts", ph::ROCKET_LAUNCH)).font(semibold(14.0)));
                ui.horizontal_wrapped(|ui| {
                    for opt in AfterLaunch::ALL {
                        if ui.selectable_label(ps.after_launch == opt, opt.label()).clicked() {
                            ps.after_launch = opt;
                            changed = true;
                        }
                    }
                });

                separator(ui);

                // JVM args
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  Java arguments", ph::COFFEE)).font(semibold(14.0)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ps.jvm_args != DEFAULT_JVM_ARGS && ui.small_button("Reset").clicked() {
                            ps.jvm_args = DEFAULT_JVM_ARGS.into();
                            changed = true;
                        }
                    });
                });
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut ps.jvm_args)
                            .font(mono(12.0))
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
                ui.label(RichText::new("Memory (-Xmx) is set from the slider above.").font(regular(11.5)).color(FAINT));
            });

            separator(ui);

            // Info + actions
            if let Some(inst) = self.installed.packs.get(&pack.name) {
                let ps = self.settings.pack(&pack.name, total);
                egui::Grid::new("pack_info").num_columns(2).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                    let row = |ui: &mut Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).color(MUTED));
                        ui.label(v);
                        ui.end_row();
                    };
                    row(ui, "Installed", format!("v{}", inst.version));
                    row(ui, "Loader", inst.resolved_version.clone().unwrap_or(inst.api.clone()));
                    row(ui, "Updated", inst.installed_at.with_timezone(&chrono::Local).format("%d %b %Y, %H:%M").to_string());
                    row(ui, "Play time", format_duration(ps.play_seconds));
                });
                ui.add_space(10.0);
            }
            ui.horizontal_wrapped(|ui| {
                if ghost_button(ui, ph::FOLDER_OPEN, "Open folder").clicked() {
                    system::open_path(&self.paths.instance_dir(&pack.name));
                }
                ui.add_enabled_ui(!locked, |ui| {
                    if ghost_button(ui, ph::WRENCH, "Repair")
                        .on_hover_text("Re-download the pack and verify every game file")
                        .clicked()
                    {
                        self.click();
                        let ctx = ui.ctx().clone();
                        self.start_job(&ctx, JobKind::Repair);
                    }
                    if self.installed.packs.contains_key(&pack.name) && danger_button(ui, ph::TRASH, "Uninstall").clicked() {
                        self.confirm = Some(Confirm::Uninstall(pack.name.clone()));
                    }
                });
            });
        });
        if changed {
            self.mark_dirty();
        }
    }
}

fn pack_card(ui: &mut Ui, pack: &crate::launcher::modpacks::Modpack, status: &PackStatus, selected: bool, enabled: bool) -> egui::Response {
    let size = vec2(250.0, 76.0);
    let (slot, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.12);
    let sel = ui.ctx().animate_bool_with_time(resp.id.with("sel"), selected, 0.15);
    let rect = slot.translate(vec2(0.0, -3.0 * hover));
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect.translate(vec2(0.0, 6.0)).shrink(4.0), CornerRadius::same(R_MD), Color32::from_black_alpha((90.0 * hover) as u8));
    }
    let r = CornerRadius::same(R_MD);
    p.rect_filled(rect, r, lerp_color(lerp_color(glass(), SURFACE_3, hover), with_alpha(ACCENT_LO, 150), sel * 0.55));
    p.rect_stroke(rect, r, Stroke::new(if selected { 1.5_f32 } else { 1.0 }, lerp_color(BORDER, ACCENT, sel)), StrokeKind::Inside);
    let icon = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(52.0, 52.0));
    paint_pack_icon(p, icon, &pack.name);
    let text_x = icon.right() + 12.0;
    let max_w = rect.right() - text_x - 10.0;
    let name = p.layout(pack.name.clone(), semibold(14.0), TEXT, max_w);
    let name_h = name.size().y.min(36.0);
    p.galley_with_override_text_color(pos2(text_x, rect.center().y - name_h / 2.0 - 8.0), name, TEXT);
    let (dot, sub) = match status {
        PackStatus::UpToDate => (GREEN, format!("v{} · installed", pack.version)),
        PackStatus::UpdateAvailable { .. } => (GOLD, format!("v{} · update", pack.version)),
        PackStatus::NotInstalled => (FAINT, format!("v{}", pack.version)),
    };
    let sub_y = rect.center().y + name_h / 2.0 - 2.0;
    p.circle_filled(pos2(text_x + 4.0, sub_y + 7.0), 3.5, dot);
    p.text(pos2(text_x + 13.0, sub_y + 7.0), Align2::LEFT_CENTER, sub, regular(12.0), MUTED);
    resp.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default })
}
