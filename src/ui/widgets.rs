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

/// Soft glow: concentric translucent rounded rects.
pub fn glow(painter: &egui::Painter, rect: Rect, radius: u8, color: Color32, strength: f32) {
    for i in 1u8..=6 {
        let grow = i as f32 * 2.5;
        let a = (strength * (1.0 - i as f32 / 7.0) * 38.0) as u8;
        painter.rect_filled(rect.expand(grow), CornerRadius::same(radius.saturating_add(i * 2)), color.gamma_multiply_u8(a));
    }
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

/// Filled accent button with glow on hover.
pub fn primary_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id, enabled && resp.hovered(), 0.12);
    let p = ui.painter();
    let r = CornerRadius::same(R_MD);
    if enabled {
        glow(p, rect, R_MD, ACCENT, 0.3 + 0.5 * hover);
        vgradient_rounded(p, rect, lerp_color(ACCENT, ACCENT_HI, hover), lerp_color(ACCENT_LO, ACCENT, hover * 0.5), R_MD);
        p.rect_stroke(rect, r, Stroke::new(1.0_f32, with_alpha(ACCENT_HI, 150)), StrokeKind::Inside);
    } else {
        p.rect_filled(rect, r, SURFACE_3);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, bold(15.0), if enabled { Color32::WHITE } else { FAINT });
    if enabled {
        resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Rounded rect with a vertical gradient: rounded caps in the end colors, gradient mesh between.
pub fn vgradient_rounded(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32, radius: u8) {
    let r = radius as f32;
    if rect.height() <= 2.0 * r {
        p.rect_filled(rect, CornerRadius::same(radius), lerp_color(top, bottom, 0.5));
        return;
    }
    p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + 2.0 * r)), CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }, top);
    p.rect_filled(Rect::from_min_max(pos2(rect.min.x, rect.max.y - 2.0 * r), rect.max), CornerRadius { nw: 0, ne: 0, sw: radius, se: radius }, bottom);
    vgradient(p, Rect::from_min_max(pos2(rect.min.x, rect.min.y + r), pos2(rect.max.x, rect.max.y - r)), top, bottom);
}

/// Rounded rect with a horizontal gradient. Each cap is clipped to its own strip so translucent colors don't
/// overlap (no visible bands).
pub fn hgradient_rounded(p: &egui::Painter, rect: Rect, left: Color32, right: Color32, radius: u8) {
    let r = radius as f32;
    if rect.width() <= 2.0 * r {
        p.rect_filled(rect, CornerRadius::same(radius), lerp_color(left, right, 0.5));
        return;
    }
    let left_strip = Rect::from_min_max(rect.min, pos2(rect.min.x + r, rect.max.y));
    let right_strip = Rect::from_min_max(pos2(rect.max.x - r, rect.min.y), rect.max);
    p.with_clip_rect(left_strip.intersect(p.clip_rect())).rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.min.x + 2.0 * r, rect.max.y)),
        CornerRadius { nw: radius, sw: radius, ne: 0, se: 0 },
        left,
    );
    p.with_clip_rect(right_strip.intersect(p.clip_rect())).rect_filled(
        Rect::from_min_max(pos2(rect.max.x - 2.0 * r, rect.min.y), rect.max),
        CornerRadius { nw: 0, sw: 0, ne: radius, se: radius },
        right,
    );
    hgradient(p, Rect::from_min_max(pos2(rect.min.x + r, rect.min.y), pos2(rect.max.x - r, rect.max.y)), left, right);
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
    // Hyve's .btn: translucent white, a bit brighter on hover.
    p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (15.0 + 14.0 * hover) as u8));
    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, (38.0 + 22.0 * hover) as u8)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Destructive variant of [`ghost_button`].
pub fn danger_button(ui: &mut Ui, icon: &str, text: &str) -> Response {
    let label = format!("{icon}  {text}");
    let galley = ui.painter().layout_no_wrap(label, semibold(13.0), ACCENT_HI);
    let size = vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(ACCENT, (20.0 + 30.0 * hover) as u8));
    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(ACCENT, 110)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, ACCENT_HI);
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

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), lerp_color(with_alpha(Color32::WHITE, 34), ACCENT, t));
    p.rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, (46.0 * (1.0 - t)) as u8)), StrokeKind::Inside);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
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
                ui.label(egui::RichText::new(icon).font(regular(18.0)).color(ACCENT_HI));
                ui.label(egui::RichText::new(title).font(bold(16.0)).color(TEXT));
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
                hgradient(p, fill.shrink2(vec2(height / 2.0, 0.0)), with_alpha(ACCENT_LO, 0), with_alpha(ACCENT_HI, 160));
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
    ui.label(egui::RichText::new(text.to_uppercase()).font(bold(11.0)).color(ACCENT_HI).extra_letter_spacing(1.6));
}
