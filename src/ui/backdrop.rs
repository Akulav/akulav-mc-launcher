//! The living wallpaper on the Play page: a slow "Ken Burns" drift over the redstone-ore art,
//! mouse parallax, pulsing ore glow and redstone embers rising from the bottom.
//! Everything is a pure function of time (no per-frame state), so it costs nothing when idle.

use egui::{pos2, vec2, Color32, Pos2, Rect};

use super::theme::*;
use super::widgets::{hgradient, vgradient, with_alpha};

fn rnd(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x & 0x00FF_FFFF) as f32 / 16_777_215.0
}

pub struct BackdropParams {
    pub time: f64,
    pub animate: bool,
    /// Pointer position relative to the rect centre, in -1..1.
    pub pointer: egui::Vec2,
}

/// Where the glowing ore sits in the wallpaper (texture UV), used for the pulse.
const ORE_UV: Pos2 = pos2(0.56, 0.42);

pub fn paint(painter: &egui::Painter, rect: Rect, texture: Option<&egui::TextureHandle>, p: &BackdropParams) {
    let t = if p.animate { p.time as f32 } else { 0.0 };

    // --- Wallpaper with drift + parallax ------------------------------------------------
    let mut ore_screen = rect.center();
    if let Some(bg) = texture {
        let tex = bg.size_vec2();
        let (ta, ra) = (tex.x / tex.y, rect.width() / rect.height());
        // Base "cover" crop.
        let (mut w, mut h) = if ra > ta { (1.0, ta / ra) } else { (ra / ta, 1.0) };
        // Zoom in a little so there is room to drift.
        let zoom = 0.9 - 0.025 * (t * 0.05).sin();
        w *= zoom;
        h *= zoom;
        let drift = vec2((t * 0.031).sin() * 0.5, (t * 0.023).cos() * 0.5);
        let par = p.pointer * 0.35;
        let cx = (0.55 + (1.0 - w) * 0.0) + (drift.x + par.x) * (1.0 - w) * 0.5;
        let cy = 0.42 + (drift.y + par.y) * (1.0 - h) * 0.5;
        let cx = cx.clamp(w / 2.0, 1.0 - w / 2.0);
        let cy = cy.clamp(h / 2.0, 1.0 - h / 2.0);
        let uv = Rect::from_center_size(pos2(cx, cy), vec2(w, h));
        painter.image(bg.id(), rect, uv, Color32::from_gray(200));
        ore_screen = pos2(
            rect.left() + (ORE_UV.x - uv.left()) / uv.width() * rect.width(),
            rect.top() + (ORE_UV.y - uv.top()) / uv.height() * rect.height(),
        );
    }

    // --- Ore pulse: a soft red bloom breathing over the ore -----------------------------------
    let pulse = 0.5 + 0.5 * (t * 1.3).sin() * (t * 0.37).cos();
    let base_r = rect.height() * 0.42;
    for i in 0..10 {
        let k = i as f32 / 10.0;
        let a = ((1.0 - k) * (10.0 + 14.0 * pulse)) as u8;
        painter.circle_filled(ore_screen, base_r * (0.25 + k * 0.9), with_alpha(Color32::from_rgb(255, 30, 30), a / 3));
    }

    // --- Legibility gradients ------------------------------------------------------------
    hgradient(painter, Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * 0.7, rect.bottom())), with_alpha(BG, 235), with_alpha(BG, 0));
    vgradient(painter, Rect::from_min_max(pos2(rect.left(), rect.bottom() - rect.height() * 0.55), rect.max), with_alpha(BG, 0), with_alpha(BG, 250));
    vgradient(painter, Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + 70.0)), with_alpha(BG, 160), with_alpha(BG, 0));

    // --- Embers --------------------------------------------------------------------------
    let count = ((rect.width() * rect.height()) / 9_000.0).clamp(40.0, 130.0) as u32;
    let travel = rect.height() + 60.0;
    for i in 0..count {
        let speed = 14.0 + rnd(i, 1) * 34.0; // px/s
        let phase = rnd(i, 2) * travel;
        let progress = ((t * speed + phase) % travel) / travel; // 0 bottom → 1 top
        let sway_amp = 6.0 + rnd(i, 3) * 18.0;
        let sway = (t * (0.4 + rnd(i, 4)) + rnd(i, 5) * std::f32::consts::TAU).sin() * sway_amp;
        // spawn biased towards the right side (behind the ore), away from the text
        let x0 = rect.left() + rect.width() * (0.25 + 0.75 * rnd(i, 6).powf(0.7));
        let x = x0 + sway + p.pointer.x * -10.0 * rnd(i, 9);
        let y = rect.bottom() + 20.0 - progress * travel;
        let size = 1.0 + rnd(i, 7).powf(2.0) * 3.2;
        let flicker = 0.65 + 0.35 * (t * (3.0 + rnd(i, 8) * 5.0) + i as f32).sin();
        // fade in at the bottom, out near the top
        let life = (progress * 6.0).min(1.0) * (1.0 - progress).powf(1.4);
        let alpha = (life * flicker * 255.0).clamp(0.0, 255.0);
        if alpha < 4.0 {
            continue;
        }
        let pos = pos2(x, y);
        let hot = rnd(i, 10) > 0.7;
        let core = if hot { Color32::from_rgb(255, 170, 120) } else { Color32::from_rgb(255, 70, 60) };
        painter.circle_filled(pos, size * 5.0, with_alpha(ACCENT, (alpha * 0.07) as u8));
        painter.circle_filled(pos, size * 2.2, with_alpha(ACCENT_HI, (alpha * 0.25) as u8));
        painter.circle_filled(pos, size, with_alpha(core, alpha as u8));
    }

    // --- Vignette ----------------------------------------------------------------------
    let v = 90.0;
    hgradient(painter, Rect::from_min_max(pos2(rect.right() - v, rect.top()), rect.max), with_alpha(BG, 0), with_alpha(BG, 140));
}

#[cfg(test)]
mod tests {
    use super::rnd;

    #[test]
    fn rnd_is_in_range_and_varies() {
        let vals: Vec<f32> = (0..100).map(|i| rnd(i, 1)).collect();
        assert!(vals.iter().all(|v| (0.0..=1.0).contains(v)));
        let mean = vals.iter().sum::<f32>() / 100.0;
        assert!((0.35..0.65).contains(&mean));
    }
}
