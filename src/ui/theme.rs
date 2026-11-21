//! Colors, fonts and the global egui style: Windows 11's dark theme, like Akulav's other apps (Hyve): a flat #202020
//! base, #2b2b2b cards with hairline borders, the Windows accent blue, Segoe UI on Windows.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Rect, Shadow, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(32, 32, 32);
pub const SURFACE: Color32 = Color32::from_rgb(43, 43, 43);
pub const SURFACE_2: Color32 = Color32::from_rgb(45, 45, 45);
pub const SURFACE_3: Color32 = Color32::from_rgb(56, 56, 56);
pub const BORDER: Color32 = Color32::from_rgb(58, 58, 58);
pub const BORDER_HI: Color32 = Color32::from_rgb(76, 76, 76);
/// Cards and panels: a flat layer one step above the window (Windows 11's card fill, no blur).
pub const GLASS: Color32 = SURFACE;
pub const GLASS_BORDER: Color32 = BORDER;
pub const TEXT: Color32 = Color32::from_rgb(255, 255, 255);
pub const MUTED: Color32 = Color32::from_rgb(207, 207, 207);
pub const FAINT: Color32 = Color32::from_rgb(157, 157, 157);
/// Windows 11's default accent on dark: light blue, with black text on it.
pub const ACCENT: Color32 = Color32::from_rgb(96, 205, 255);
pub const ACCENT_HI: Color32 = Color32::from_rgb(153, 235, 255);
pub const ACCENT_LO: Color32 = Color32::from_rgb(0, 120, 212);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0, 0, 0);
pub const GOLD: Color32 = Color32::from_rgb(252, 225, 0);
pub const GREEN: Color32 = Color32::from_rgb(108, 203, 95);
pub const BLUE: Color32 = Color32::from_rgb(96, 205, 255);
pub const RED: Color32 = Color32::from_rgb(255, 153, 164);

/// Panel color drawn over the Play page's wallpaper.
pub fn glass() -> Color32 {
    Color32::from_rgba_unmultiplied(32, 32, 32, 232)
}

/// Windows 11: 4 px controls, 8 px cards and dialogs.
pub const R_SM: u8 = 4;
pub const R_MD: u8 = 4;
pub const R_LG: u8 = 8;
/// Gap between the window edge, the sidebar and the content.
pub const GAP: f32 = 12.0;

/// A card: the layer fill, a hairline border, 8 px corners.
pub fn glass_frame() -> Frame {
    Frame::new().fill(GLASS).stroke(Stroke::new(1.0_f32, GLASS_BORDER)).corner_radius(CornerRadius::same(R_LG))
}

/// The window background: Windows 11's flat dark base.
pub fn paint_backdrop(ctx: &egui::Context) {
    let r: Rect = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::background()).rect_filled(r, CornerRadius::ZERO, BG);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
/// Display text (page titles, the pack name): Segoe UI Semibold on Windows, like Windows 11's titles.
pub fn black(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("black".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// A font from the Windows fonts folder (Segoe UI has no Private Use Area glyphs, so the icons stay visible).
fn system_font(file: &str) -> Option<Vec<u8>> {
    if !cfg!(windows) {
        return None;
    }
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::fs::read(std::path::Path::new(&windir).join("Fonts").join(file)).ok()
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    // Windows 11's UI font, read from Windows (it can't be shipped); Inter, bundled, everywhere else.
    let pick = |system: &str, bundled: &'static [u8]| match system_font(system) {
        Some(bytes) => FontData::from_owned(bytes),
        None => FontData::from_static(bundled),
    };
    let mut put = |name: &str, data: FontData| {
        fonts.font_data.insert(name.to_owned(), Arc::new(data));
    };
    put("inter", pick("segoeui.ttf", include_bytes!("../../assets/fonts/Inter-Regular.otf")));
    put("inter-semibold", pick("seguisb.ttf", include_bytes!("../../assets/fonts/Inter-SemiBold.otf")));
    put("inter-bold", pick("segoeuib.ttf", include_bytes!("../../assets/fonts/Inter-Bold.otf")));
    put("inter-black", pick("seguisb.ttf", include_bytes!("../../assets/fonts/Inter-ExtraBold.otf")));
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
        v.extreme_bg_color = Color32::from_rgb(28, 28, 28); // text fields, like Windows 11's text boxes
        v.faint_bg_color = SURFACE_2;
        v.code_bg_color = SURFACE_2;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(R_LG);
        v.menu_corner_radius = CornerRadius::same(R_MD);
        v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(90) };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(80) };
        v.selection.bg_fill = ACCENT_LO;
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
        w.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_HI);
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
        style.text_styles = [(Small, regular(11.5)), (Body, regular(14.0)), (Button, semibold(13.5)), (Heading, semibold(24.0)), (Monospace, mono(12.5))].into();
    });
}
