//! Live log console for the launcher, installers and the game.

use egui::{vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, TextStyle, Ui};
use egui_phosphor::regular as ph;

use super::app::LauncherApp;
use super::theme::*;
use super::widgets::*;
use crate::launcher::events::{LogLevel, LogLine, LogSource};
use crate::launcher::system;

fn level_color(l: LogLevel) -> Color32 {
    match l {
        LogLevel::Debug => FAINT,
        LogLevel::Info => Color32::from_rgb(206, 210, 218),
        LogLevel::Warn => GOLD,
        LogLevel::Error => ACCENT_HI,
    }
}

fn source_tag(s: LogSource) -> (&'static str, Color32) {
    match s {
        LogSource::Launcher => ("LAUNCHER", ACCENT_HI),
        LogSource::Game => ("GAME", Color32::from_rgb(120, 128, 144)),
        LogSource::Installer => ("INSTALL", BLUE),
    }
}

pub fn format_line(l: &LogLine) -> String {
    format!("{} [{}] {}", l.time.format("%H:%M:%S"), source_tag(l.source).0, l.text)
}

impl LauncherApp {
    fn filtered_indices(&mut self) -> Vec<usize> {
        let key = (self.logs.len(), self.log_generation, self.log_filter.clone(), self.log_min_level);
        if let Some((len, gen, filter, level, idx)) = &self.log_cache {
            if (*len, *gen, filter.clone(), *level) == key {
                return idx.clone();
            }
        }
        let needle = self.log_filter.to_lowercase();
        let idx: Vec<usize> = self
            .logs
            .iter()
            .enumerate()
            .filter(|(_, l)| l.level >= self.log_min_level)
            .filter(|(_, l)| needle.is_empty() || l.text.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        self.log_cache = Some((key.0, key.1, key.2, key.3, idx.clone()));
        idx
    }

    pub fn console_page(&mut self, ui: &mut Ui) {
        self.unseen_errors = 0;
        let outer = ui.max_rect().shrink2(vec2(32.0, 26.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(outer));

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                label_kicker(ui, "Logs");
                ui.label(RichText::new("Console").font(black(30.0)));
            });
            ui.add_space(16.0);
            let errors = self.logs.iter().filter(|l| l.level == LogLevel::Error).count();
            let warns = self.logs.iter().filter(|l| l.level == LogLevel::Warn).count();
            ui.vertical(|ui| {
                ui.add_space(26.0);
                ui.horizontal(|ui| {
                    chip(ui, ph::LIST, &format!("{} {}", self.logs.len(), if self.logs.len() == 1 { "line" } else { "lines" }), MUTED);
                    if warns > 0 {
                        chip(ui, ph::WARNING, &format!("{warns}"), GOLD);
                    }
                    if errors > 0 {
                        chip(ui, ph::WARNING_CIRCLE, &format!("{errors}"), ACCENT_HI);
                    }
                });
            });
        });
        ui.add_space(10.0);

        // Toolbar
        ui.horizontal(|ui| {
            Frame::new()
                .fill(SURFACE_2)
                .stroke(Stroke::new(1.0_f32, BORDER))
                .corner_radius(CornerRadius::same(R_SM))
                .inner_margin(Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(ph::MAGNIFYING_GLASS).color(MUTED));
                        ui.add(egui::TextEdit::singleline(&mut self.log_filter).frame(false).hint_text("Filter…").desired_width(220.0));
                        if !self.log_filter.is_empty() && icon_button(ui, ph::X, 20.0, "Clear filter").clicked() {
                            self.log_filter.clear();
                        }
                    });
                });
            ui.add_space(6.0);
            for (lvl, label) in [(LogLevel::Debug, "All"), (LogLevel::Info, "Info+"), (LogLevel::Warn, "Warnings"), (LogLevel::Error, "Errors")] {
                if ui.selectable_label(self.log_min_level == lvl, label).clicked() {
                    self.log_min_level = lvl;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, ph::TRASH, 32.0, "Clear console").clicked() {
                    self.logs.clear();
                    self.log_generation += 1;
                }
                if icon_button(ui, ph::COPY, 32.0, "Copy visible lines").clicked() {
                    let idx = self.filtered_indices();
                    let text = idx.iter().map(|&i| format_line(&self.logs[i])).collect::<Vec<_>>().join("\n");
                    ui.ctx().copy_text(text);
                    self.toast(super::app::ToastKind::Success, "Copied to clipboard", "");
                }
                if icon_button(ui, ph::FOLDER_OPEN, 32.0, "Open the selected pack's logs folder").clicked() {
                    if let Some(p) = self.selected_pack() {
                        system::open_path(&self.paths.instance_dir(&p.name).join("logs"));
                    }
                }
                ui.add_space(8.0);
                toggle(ui, &mut self.log_autoscroll);
                ui.label(RichText::new("Follow").color(MUTED));
            });
        });
        ui.add_space(8.0);

        if let Some(crash) = self.last_crash.clone() {
            Frame::new()
                .fill(with_alpha(ACCENT, 26))
                .stroke(Stroke::new(1.0_f32, with_alpha(ACCENT, 110)))
                .corner_radius(CornerRadius::same(R_MD))
                .inner_margin(Margin::symmetric(14, 10))
                .show(&mut ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(ph::BUG).font(regular(18.0)).color(ACCENT_HI));
                        ui.label(RichText::new("The game crashed.").font(semibold(14.0)));
                        ui.label(RichText::new(crash.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, ph::X, 26.0, "Dismiss").clicked() {
                                self.last_crash = None;
                            }
                            if ghost_button(ui, ph::FILE_TEXT, "Open crash report").clicked() {
                                system::open_path(&crash);
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }

        let idx = self.filtered_indices();
        Frame::new()
            .fill(Color32::from_rgb(9, 10, 13))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(R_MD))
            .inner_margin(Margin::same(12))
            .show(&mut ui, |ui| {
                ui.set_min_size(ui.available_size());
                if idx.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(if self.logs.is_empty() { "Nothing here yet. Launch a pack to see its output." } else { "No lines match the filter." }).color(FAINT));
                    });
                    return;
                }
                let row_h = ui.text_style_height(&TextStyle::Monospace) + 3.0;
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.log_autoscroll)
                    .show_rows(ui, row_h, idx.len(), |ui, range| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for &i in &idx[range] {
                            let l = &self.logs[i];
                            let (tag, tag_color) = source_tag(l.source);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(RichText::new(l.time.format("%H:%M:%S").to_string()).font(mono(12.0)).color(FAINT));
                                ui.label(RichText::new(format!("{tag:<8}")).font(mono(12.0)).color(tag_color));
                                ui.add(egui::Label::new(RichText::new(&l.text).font(mono(12.5)).color(level_color(l.level))).extend());
                            });
                        }
                    });
            });
    }
}
