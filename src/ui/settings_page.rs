//! Global launcher settings.

use egui::{vec2, RichText, ScrollArea, Ui};
use egui_phosphor::regular as ph;

use super::app::{Confirm, LauncherApp, ToastKind};
use super::theme::*;
use super::widgets::*;
use crate::launcher::modpacks::DEFAULT_MODPACK_LIST_URL;
use crate::launcher::settings::validate_username;
use crate::launcher::system::{self, format_bytes};
use crate::launcher::update;

impl LauncherApp {
    pub fn settings_page(&mut self, ui: &mut Ui) {
        let outer = ui.max_rect().shrink2(vec2(32.0, 26.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));
        label_kicker(&mut ui, "Preferences");
        ui.label(RichText::new("Settings").font(black(30.0)));
        ui.add_space(12.0);

        ScrollArea::vertical().auto_shrink([false, false]).show(&mut ui, |ui| {
            ui.set_max_width(820.0);
            let mut changed = false;

            card(ui, "Account", ph::USER, |ui| {
                ui.horizontal(|ui| {
                    // Big 3D head: drag to spin it, it settles back to a three-quarter view.
                    let (rect, resp) = ui.allocate_exact_size(vec2(132.0, 132.0), egui::Sense::drag());
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(R_LG), SURFACE_2);
                    ui.painter().circle_filled(rect.center() + vec2(0.0, 6.0), 52.0, with_alpha(ACCENT, 22));
                    if resp.dragged() {
                        self.head_drag += resp.drag_delta() * 0.012;
                        self.head_drag.y = self.head_drag.y.clamp(-0.9, 0.9);
                    } else if self.head_drag.length() > 0.001 {
                        self.head_drag *= 0.9;
                        ui.ctx().request_repaint();
                    }
                    let t = ui.input(|i| i.time) as f32;
                    let idle = if self.settings.animated_background { (t * 0.7).sin() * 0.12 } else { 0.0 };
                    if self.settings.animated_background {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
                    }
                    let (tex, source) = self.head_texture(ui.ctx());
                    super::head::paint_head_3d(ui.painter(), rect.shrink(10.0), tex, -0.6 + idle + self.head_drag.x, 0.35 + self.head_drag.y);
                    let resp = resp.on_hover_cursor(egui::CursorIcon::Grab);
                    let _ = resp;

                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Nickname").font(semibold(14.0)));
                        let valid = validate_username(&self.settings.username).is_ok();
                        let r = ui.add_enabled(
                            self.game.is_none() && !self.busy(),
                            egui::TextEdit::singleline(&mut self.settings.username)
                                .margin(egui::Margin::symmetric(10, 7))
                                .font(semibold(15.0))
                                .char_limit(16)
                                .desired_width(260.0)
                                .text_color(if valid { TEXT } else { ACCENT_HI }),
                        );
                        if r.changed() {
                            self.name_changed();
                        }
                        ui.add_space(2.0);
                        let (icon, text, color) = match (validate_username(&self.settings.username), self.skin_status(), source) {
                            (Err(e), _, _) => (ph::WARNING, e.to_string(), ACCENT_HI),
                            (_, _, Some(src)) => (ph::CHECK_CIRCLE, format!("Skin found on {}", src.label()), GREEN),
                            (_, Some(crate::ui::app::SkinEntry::Loading), _) | (_, None, _) => (ph::CIRCLE_NOTCH, "Looking up skin…".into(), MUTED),
                            _ => (ph::USER_CIRCLE, "No skin for this name yet — showing a generated head".into(), MUTED),
                        };
                        ui.label(RichText::new(format!("{icon}  {text}")).font(regular(12.5)).color(color));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Offline account. Skins are looked up by name on Mojang and Ely.by — the same places your pack's skin loader uses in game.")
                                .font(regular(12.0))
                                .color(FAINT),
                        );
                    });
                });
            });
            ui.add_space(12.0);

            card(ui, "Modpack source", ph::GLOBE, |ui| {
                ui.label(RichText::new("Where the launcher gets the list of modpacks. Leave empty to use the official Akulav list.").color(MUTED));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.list_url_edit).margin(egui::Margin::symmetric(10, 7))
                            .hint_text(DEFAULT_MODPACK_LIST_URL)
                            .desired_width((ui.available_width() - 230.0).max(200.0)),
                    );
                    if primary_button(ui, "Apply", vec2(90.0, 32.0), true).clicked() {
                        self.click();
                        let v = self.list_url_edit.trim().to_string();
                        self.settings.modpack_list_url = (!v.is_empty()).then_some(v);
                        changed = true;
                        self.reload_modpacks();
                        self.toast(ToastKind::Info, "Reloading modpacks", self.list_url());
                    }
                    if ghost_button(ui, ph::ARROW_COUNTER_CLOCKWISE, "Default").clicked() {
                        self.list_url_edit.clear();
                        self.settings.modpack_list_url = None;
                        changed = true;
                        self.reload_modpacks();
                    }
                });
            });
            ui.add_space(12.0);

            card(ui, "Java", ph::COFFEE, |ui| {
                let mut custom = self.settings.java_path.is_some();
                setting_row(ui, "Use my own Java", "By default the launcher downloads the exact Java version Mojang ships for each Minecraft release.", |ui| {
                    if toggle(ui, &mut custom).changed() {
                        self.settings.java_path = if custom { Some(self.java_edit.clone()) } else { None };
                        changed = true;
                    }
                });
                if custom {
                    ui.horizontal(|ui| {
                        let hint = if cfg!(windows) { r"C:\Program Files\Java\jdk-21\bin\java.exe" } else { "/usr/bin/java" };
                        if ui.add(egui::TextEdit::singleline(&mut self.java_edit).margin(egui::Margin::symmetric(10, 7)).hint_text(hint).desired_width(ui.available_width())).changed() {
                            self.settings.java_path = Some(self.java_edit.clone());
                            changed = true;
                        }
                    });
                    let ok = std::path::Path::new(self.java_edit.trim()).is_file();
                    ui.label(
                        RichText::new(if ok { format!("{}  Found", ph::CHECK_CIRCLE) } else { format!("{}  File not found", ph::WARNING) })
                            .font(regular(12.5))
                            .color(if ok { GREEN } else { GOLD }),
                    );
                }
            });
            ui.add_space(12.0);

            card(ui, "Downloads", ph::DOWNLOAD_SIMPLE, |ui| {
                setting_row(ui, "Parallel downloads", "More is faster on good connections. Lower it if downloads keep failing.", |ui| {
                    let mut n = self.settings.concurrent_downloads as u32;
                    if ui.add(egui::Slider::new(&mut n, 2..=48)).changed() {
                        self.settings.concurrent_downloads = n as usize;
                        changed = true;
                    }
                });
            });
            ui.add_space(12.0);

            card(ui, "Launcher", ph::SLIDERS_HORIZONTAL, |ui| {
                setting_row(ui, "Click sound", "The lever sound from the original launcher.", |ui| {
                    if toggle(ui, &mut self.settings.click_sound).changed() {
                        changed = true;
                        self.click();
                    }
                });
                separator(ui);
                setting_row(ui, "Animated background", "Drifting wallpaper, glowing ore and rising embers on the Play page. Paused automatically while you play.", |ui| {
                    if toggle(ui, &mut self.settings.animated_background).changed() {
                        changed = true;
                    }
                });
            });
            ui.add_space(12.0);

            card(ui, "Storage", ph::HARD_DRIVES, |ui| {
                setting_row(ui, "Data folder", &self.paths.root.display().to_string(), |ui| {
                    if ghost_button(ui, ph::FOLDER_OPEN, "Open").clicked() {
                        system::open_path(&self.paths.root);
                    }
                });
                separator(ui);
                let cache = self.paths.cache.clone();
                setting_row(ui, "Download cache", "Leftover archives and installers from interrupted downloads.", |ui| {
                    if ui.add_enabled(!self.busy(), egui::Button::new(format!("{}  Clear", ph::BROOM))).clicked() {
                        let size = system::folder_size(&cache);
                        let _ = std::fs::remove_dir_all(&cache);
                        let _ = std::fs::create_dir_all(&cache);
                        self.toast(ToastKind::Success, "Cache cleared", format!("Freed {}", format_bytes(size)));
                    }
                });
                separator(ui);
                setting_row(ui, "Reset settings", "Restore defaults for the launcher and every pack. Installed packs and worlds are kept.", |ui| {
                    if danger_button(ui, ph::ARROW_COUNTER_CLOCKWISE, "Reset").clicked() {
                        self.confirm = Some(Confirm::ResetSettings);
                    }
                });
                if cfg!(target_os = "linux") && crate::launcher::selfupdate::running_installed() {
                    separator(ui);
                    setting_row(ui, "Uninstall launcher", "Remove the launcher, its app-menu entry and shortcuts.", |ui| {
                        if danger_button(ui, ph::TRASH, "Uninstall").clicked() {
                            self.confirm = Some(Confirm::UninstallLauncher);
                        }
                    });
                }
            });
            ui.add_space(12.0);

            card(ui, "About", ph::INFO, |ui| {
                setting_row(ui, &format!("Akulav Launcher {}", update::CURRENT_VERSION), "Open source, no telemetry. Rewritten in Rust.", |ui| {
                    if ghost_button(ui, ph::GITHUB_LOGO, "GitHub").clicked() {
                        let _ = open::that_detached("https://github.com/Akulav/AkulavMCLauncher");
                    }
                    match self.update.info().cloned() {
                        Some(info) => {
                            if primary_button(ui, &format!("Update to {}", info.version), vec2(160.0, 34.0), true).clicked() {
                                self.show_update_dialog = true;
                            }
                        }
                        None => {
                            if ghost_button(ui, ph::ARROW_CLOCKWISE, "Check for updates").clicked() {
                                self.last_update_check = std::time::Instant::now();
                                crate::launcher::jobs::spawn_update_check(self.rt.handle(), self.reporter.clone());
                                self.toast(ToastKind::Info, "Checking for updates…", "");
                            }
                        }
                    }
                });
            });
            ui.add_space(24.0);

            if changed {
                self.mark_dirty();
            }
        });
    }
}
