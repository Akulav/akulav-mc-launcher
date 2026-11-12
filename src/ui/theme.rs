//! Colors, fonts and the global egui style. The look follows Akulav's newer apps (Hyve, Lumen): a deep blue-black
//! backdrop with soft glows and glass panels with hairline borders, with the launcher's redstone-red accent.

use std::sync::Arc;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Pos2, Rect, Shadow, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(9, 11, 20);
pub const SURFACE: Color32 = Color32::from_rgb(16, 19, 32);
pub const SURFACE_2: Color32 = Color32::from_rgb(22, 25, 40);
pub const SURFACE_3: Color32 = Color32::from_rgb(30, 34, 52);
pub const BORDER: Color32 = Color32::from_rgb(36, 40, 58);
pub const BORDER_HI: Color32 = Color32::from_rgb(58, 63, 88);
/// Glass: translucent white over the backdrop, so the glows show through panels and cards.
pub const GLASS: Color32 = Color32::from_rgba_premultiplied(14, 14, 15, 14);
pub const GLASS_BORDER: Color32 = Color32::from_rgba_premultiplied(24, 24, 26, 24);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 246);
pub const MUTED: Color32 = Color32::from_rgb(143, 149, 178);
pub const FAINT: Color32 = Color32::from_rgb(93, 99, 128);
pub const ACCENT: Color32 = Color32::from_rgb(229, 56, 59);
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 92, 95);
pub const ACCENT_LO: Color32 = Color32::from_rgb(150, 30, 34);
pub const GOLD: Color32 = Color32::from_rgb(251, 191, 36);
pub const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
pub const BLUE: Color32 = Color32::from_rgb(34, 211, 238);

/// Translucent panel color used on top of the wallpaper.
pub fn glass() -> Color32 {
    Color32::from_rgba_unmultiplied(14, 17, 29, 214)
}

pub const R_SM: u8 = 8;
pub const R_MD: u8 = 10;
pub const R_LG: u8 = 16;
/// Gap between the window edge, the sidebar and the content.
pub const GAP: f32 = 14.0;

/// A glass panel: translucent fill, hairline border, large radius.
pub fn glass_frame() -> Frame {
    Frame::new().fill(GLASS).stroke(Stroke::new(1.0_f32, GLASS_BORDER)).corner_radius(CornerRadius::same(R_LG))
}

/// Soft radial glow: `color` in the middle, fading to nothing at `radius`.
fn radial(mesh: &mut Mesh, center: Pos2, radius: egui::Vec2, color: Color32) {
    let base = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex { pos: center, uv: WHITE_UV, color });
    let n = 48;
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let pos = center + egui::vec2(a.cos() * radius.x, a.sin() * radius.y);
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color: Color32::TRANSPARENT });
    }
    for i in 0..n {
        mesh.add_triangle(base, base + 1 + i, base + 2 + i);
    }
}

/// The window backdrop: the base color with a red glow top-left, a cyan glow top-right and a faint one below.
pub fn paint_backdrop(ctx: &egui::Context) {
    let r: Rect = ctx.screen_rect();
    let p = ctx.layer_painter(egui::LayerId::background());
    p.rect_filled(r, CornerRadius::ZERO, BG);
    let (w, h) = (r.width(), r.height());
    let mut mesh = Mesh::default();
    radial(&mut mesh, r.min + egui::vec2(w * 0.08, -h * 0.10), egui::vec2(w * 0.62, h * 0.85), ACCENT.gamma_multiply(0.13));
    radial(&mut mesh, r.min + egui::vec2(w * 1.05, h * 0.10), egui::vec2(w * 0.48, h * 0.75), BLUE.gamma_multiply(0.12));
    radial(&mut mesh, r.min + egui::vec2(w * 0.5, h * 1.2), egui::vec2(w * 0.45, h * 0.6), ACCENT_LO.gamma_multiply(0.08));
    p.add(mesh);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn black(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("black".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "inter", include_bytes!("../../assets/fonts/Inter-Regular.otf"));
    add(&mut fonts, "inter-semibold", include_bytes!("../../assets/fonts/Inter-SemiBold.otf"));
    add(&mut fonts, "inter-bold", include_bytes!("../../assets/fonts/Inter-Bold.otf"));
    add(&mut fonts, "inter-black", include_bytes!("../../assets/fonts/Inter-ExtraBold.otf"));
    add(&mut fonts, "mono", include_bytes!("../../assets/fonts/DejaVuSansMono.ttf"));

    // Icons first registered into Proportional by egui_phosphor; we copy that fallback everywhere.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    let fallbacks: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // The bundled Inter files have their Private Use Area glyphs stripped (see assets/fonts),
    // otherwise they would shadow Phosphor's icons, which live in that range.
    let others: Vec<String> = fallbacks.into_iter().filter(|f| !f.starts_with("inter")).collect();

    let family = |primary: &str| {
        let mut v = vec![primary.to_string()];
        v.extend(others.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("inter-semibold"));
    fonts.families.insert(FontFamily::Name("bold".into()), family("inter-bold"));
    fonts.families.insert(FontFamily::Name("black".into()), family("inter-black"));
    fonts.families.insert(FontFamily::Monospace, family("mono"));
    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.style_mut(|style| {
        let mut v = Visuals::dark();
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = Color32::from_rgb(7, 8, 15); // text fields: darker than the glass around them
        v.faint_bg_color = SURFACE_2;
        v.code_bg_color = SURFACE_2;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(R_LG);
        v.menu_corner_radius = CornerRadius::same(R_MD);
        v.window_shadow = Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) };
        v.popup_shadow = Shadow { offset: [0, 8], blur: 20, spread: 0, color: Color32::from_black_alpha(120) };
        v.selection.bg_fill = ACCENT.linear_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0_f32, ACCENT); // also the focused text field border
        v.hyperlink_color = ACCENT_HI;
        v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT_HI);
        v.slider_trailing_fill = true;

        let r = CornerRadius::same(R_MD);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE_2;
        w.noninteractive.weak_bg_fill = SURFACE_2;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.noninteractive.corner_radius = r;

        w.inactive.bg_fill = SURFACE_2;
        w.inactive.weak_bg_fill = SURFACE_2;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_HI);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = r;

        w.hovered.bg_fill = SURFACE_3;
        w.hovered.weak_bg_fill = SURFACE_3;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(64, 70, 84));
        w.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.hovered.corner_radius = r;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = ACCENT_LO;
        w.active.weak_bg_fill = SURFACE_3;
        w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.active.corner_radius = r;
        w.active.expansion = 0.0;

        w.open = w.hovered;
        style.visuals = v;

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.slider_width = 240.0;
        style.spacing.window_margin = Margin::same(16);
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = true;

        use egui::TextStyle::*;
        style.text_styles = [(Small, regular(11.5)), (Body, regular(14.0)), (Button, semibold(13.5)), (Heading, bold(22.0)), (Monospace, mono(12.5))].into();
    });
}
