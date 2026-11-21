//! Custom-drawn widgets: buttons, chips, toggles, avatars, pack tiles.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Margin, Rect, Response, Sense, Stroke, StrokeKind, Ui};

use super::theme::*;

pub fn hash32(s: &str) -> u32 {
    // FNV-1a: stable across runs/platforms.
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Pleasant saturated color derived from a string.
pub fn hash_color(s: &str, sat: f32, val: f32) -> Color32 {
    let hue = (hash32(s) % 360) as f32 / 360.0;
    let c: egui::ecolor::Hsva = egui::ecolor::Hsva::new(hue, sat, val, 1.0);
    c.into()
}

pub fn vgradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    for (pos, c) in [(rect.left_top(), top), (rect.right_top(), top), (rect.right_bottom(), bottom), (rect.left_bottom(), bottom)] {
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color: c });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

pub fn hgradient(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    for (pos, c) in [(rect.left_top(), left), (rect.right_top(), right), (rect.right_bottom(), right), (rect.left_bottom(), left)] {
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color: c });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Square tile with the pack's initials over a gradient seeded by its name.
pub fn paint_pack_icon(painter: &egui::Painter, rect: Rect, name: &str) {
    let a = hash_color(name, 0.65, 0.55);
    let b = hash_color(&format!("{name}~"), 0.75, 0.25);
    let r = CornerRadius::same(R_MD);
    painter.rect_filled(rect, r, b);
    // diagonal-ish gradient: vertical gradient inset
    vgradient(painter, rect.shrink(1.0), with_alpha(a, 220), with_alpha(b, 0));
    let initials: String =
        name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).take(2).filter_map(|w| w.chars().next()).collect::<String>().to_uppercase();
    painter.text(rect.center(), Align2::CENTER_CENTER, initials, black(rect.height() * 0.36), TEXT);
    painter.rect_stroke(rect, r, Stroke::new(1.0_f32, Color32::from_white_alpha(18)), StrokeKind::Inside);
}

/// Small rounded label, e.g. "NeoForge 21.1.248".
pub fn chip(ui: &mut Ui, icon: &str, text: &str, color: Color32) -> Response {
    let label = if icon.is_empty() { text.to_string() } else { format!("{icon}  {text}") };
    let galley = ui.painter().layout_no_wrap(label, semibold(12.0), color);
    let size = galley.size() + vec2(20.0, 10.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), with_alpha(color, 26));
    p.rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, with_alpha(color, 70)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Filled accent button (Windows 11's accent button: flat, black text).
pub fn primary_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id, enabled && resp.hovered(), 0.12);
    let p = ui.painter();
    let r = CornerRadius::same(R_MD);
    if enabled {
        p.rect_filled(rect, r, lerp_color(ACCENT, Color32::from_rgb(90, 188, 233), hover));
    } else {
        p.rect_filled(rect, r, SURFACE_3);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, semibold(15.0), if enabled { ON_ACCENT } else { FAINT });
    if enabled {
        resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Secondary button: subtle surface, optional icon.
pub fn ghost_button(ui: &mut Ui, icon: &str, text: &str) -> Response {
    let label = if icon.is_empty() {
        text.to_string()
    } else if text.is_empty() {
        icon.to_string()
    } else {
        format!("{icon}  {text}")
    };
    let galley = ui.painter().layout_no_wrap(label, semibold(13.0), TEXT);
    let size = vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let p = ui.painter();
    // Windows 11's standard button: a light layer with a hairline border, a step lighter on hover.
    p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (15.0 + 6.0 * hover) as u8));
    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, 24)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Destructive variant of [`ghost_button`].
pub fn danger_button(ui: &mut Ui, icon: &str, text: &str) -> Response {
    let label = format!("{icon}  {text}");
    let galley = ui.painter().layout_no_wrap(label, semibold(13.0), RED);
    let size = vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(RED, (14.0 + 18.0 * hover) as u8));
    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(RED, 90)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, RED);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Square icon-only button.
pub fn icon_button(ui: &mut Ui, icon: &str, size: f32, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(R_SM), with_alpha(Color32::WHITE, (hover * 16.0) as u8));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, icon, regular(size * 0.5), lerp_color(MUTED, TEXT, hover));
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// Windows 11 toggle switch: an outlined track when off, the accent with a black knob when on.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let p = ui.painter();
    let size = vec2(40.0, 20.0);
    let rect = Rect::from_center_size(rect.center(), size);
    p.rect_filled(rect, CornerRadius::same(255), lerp_color(with_alpha(Color32::BLACK, 40), ACCENT, t));
    p.rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, (160.0 * (1.0 - t)) as u8)), StrokeKind::Inside);
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    p.circle_filled(pos2(x, rect.center().y), 6.0, lerp_color(MUTED, ON_ACCENT, t));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A labelled settings row: title + description on the left, control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - 300.0).max(220.0));
            ui.label(egui::RichText::new(title).font(semibold(14.0)).color(TEXT));
            if !desc.is_empty() {
                ui.label(egui::RichText::new(desc).font(regular(12.5)).color(MUTED));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

/// Card container for settings sections.
pub fn card<R>(ui: &mut Ui, title: &str, icon: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    glass_frame()
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon).font(regular(18.0)).color(ACCENT));
                ui.label(egui::RichText::new(title).font(semibold(16.0)).color(TEXT));
            });
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0_f32, BORDER));
}

/// Thin progress bar. `fraction = None` draws an indeterminate shimmer.
pub fn progress_bar(ui: &mut Ui, fraction: Option<f32>, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let r = CornerRadius::same(255);
    p.rect_filled(rect, r, with_alpha(Color32::WHITE, 18));
    match fraction {
        Some(f) => {
            let w = rect.width() * f.clamp(0.0, 1.0);
            if w > 1.0 {
                let fill = Rect::from_min_size(rect.min, vec2(w.max(height), height));
                p.rect_filled(fill, r, ACCENT);
            }
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.3;
            let x = rect.left() + ((t * 0.8).fract() * (rect.width() + seg)) - seg;
            let fill = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + seg).min(rect.right()), rect.bottom()));
            if fill.width() > 0.0 {
                p.rect_filled(fill, r, ACCENT);
            }
            ui.ctx().request_repaint();
        }
    }
}

pub fn label_kicker(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(12.0)).color(MUTED));
}
