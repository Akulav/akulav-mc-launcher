//! Colors, fonts and the global egui style. The palette is built around the launcher's
//! redstone-ore wallpaper: charcoal surfaces with a redstone-red accent.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(11, 12, 15);
pub const SURFACE: Color32 = Color32::from_rgb(18, 20, 25);
pub const SURFACE_2: Color32 = Color32::from_rgb(26, 29, 36);
pub const SURFACE_3: Color32 = Color32::from_rgb(36, 40, 49);
pub const BORDER: Color32 = Color32::from_rgb(40, 44, 54);
pub const TEXT: Color32 = Color32::from_rgb(236, 238, 242);
pub const MUTED: Color32 = Color32::from_rgb(138, 145, 160);
pub const FAINT: Color32 = Color32::from_rgb(92, 98, 112);
pub const ACCENT: Color32 = Color32::from_rgb(229, 56, 59);
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 92, 95);
pub const ACCENT_LO: Color32 = Color32::from_rgb(150, 30, 34);
pub const GOLD: Color32 = Color32::from_rgb(244, 183, 64);
pub const GREEN: Color32 = Color32::from_rgb(61, 214, 140);
pub const BLUE: Color32 = Color32::from_rgb(96, 165, 250);

/// Translucent panel color used on top of the wallpaper.
pub fn glass() -> Color32 {
    Color32::from_rgba_unmultiplied(16, 18, 23, 214)
}

pub const R_SM: u8 = 6;
pub const R_MD: u8 = 10;
pub const R_LG: u8 = 14;

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
        v.extreme_bg_color = Color32::from_rgb(9, 10, 13);
        v.faint_bg_color = SURFACE_2;
        v.code_bg_color = SURFACE_2;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(R_LG);
        v.menu_corner_radius = CornerRadius::same(R_MD);
        v.window_shadow = Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) };
        v.popup_shadow = Shadow { offset: [0, 8], blur: 20, spread: 0, color: Color32::from_black_alpha(120) };
        v.selection.bg_fill = ACCENT.linear_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0_f32, TEXT);
        v.hyperlink_color = ACCENT_HI;
        v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT_HI);
        v.slider_trailing_fill = true;

        let r = CornerRadius::same(R_SM);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE_2;
        w.noninteractive.weak_bg_fill = SURFACE_2;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.noninteractive.corner_radius = r;

        w.inactive.bg_fill = SURFACE_3;
        w.inactive.weak_bg_fill = SURFACE_2;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
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
        style.text_styles = [
            (Small, regular(11.5)),
            (Body, regular(14.0)),
            (Button, semibold(13.5)),
            (Heading, bold(22.0)),
            (Monospace, mono(12.5)),
        ]
        .into();
    });
}
