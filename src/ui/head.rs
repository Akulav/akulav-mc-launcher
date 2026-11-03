//! Minecraft head rendering: a textured, shaded 3D cube (with the hat overlay) drawn from a
//! 64×64 skin, plus a flat face variant and a procedural skin for names without one.

use egui::epaint::{Mesh, Vertex};
use egui::{pos2, Color32, Pos2, Rect, TextureId};

use super::widgets::{hash32, hash_color, lerp_color};

/// One cube face: 4 corners (TL, TR, BR, BL) in model space and its texture rect in skin pixels.
struct Face {
    corners: [[f32; 3]; 4],
    uv: [f32; 4], // x0, y0, x1, y1
    normal: [f32; 3],
    light: f32,
}

/// Head faces in Minecraft's skin layout. The model faces +Z (towards the viewer at yaw 0);
/// +X is the viewer's right, i.e. the player's left side.
fn head_faces(u0: f32) -> [Face; 6] {
    let n = -1.0;
    let p = 1.0;
    [
        Face { corners: [[n, p, p], [p, p, p], [p, n, p], [n, n, p]], uv: [u0 + 8.0, 8.0, u0 + 16.0, 16.0], normal: [0.0, 0.0, 1.0], light: 1.0 },
        Face { corners: [[n, p, n], [n, p, p], [n, n, p], [n, n, n]], uv: [u0, 8.0, u0 + 8.0, 16.0], normal: [-1.0, 0.0, 0.0], light: 0.72 },
        Face { corners: [[p, p, p], [p, p, n], [p, n, n], [p, n, p]], uv: [u0 + 16.0, 8.0, u0 + 24.0, 16.0], normal: [1.0, 0.0, 0.0], light: 0.72 },
        Face { corners: [[p, p, n], [n, p, n], [n, n, n], [p, n, n]], uv: [u0 + 24.0, 8.0, u0 + 32.0, 16.0], normal: [0.0, 0.0, -1.0], light: 0.6 },
        Face { corners: [[n, p, n], [p, p, n], [p, p, p], [n, p, p]], uv: [u0 + 8.0, 0.0, u0 + 16.0, 8.0], normal: [0.0, 1.0, 0.0], light: 1.12 },
        Face { corners: [[n, n, p], [p, n, p], [p, n, n], [n, n, n]], uv: [u0 + 16.0, 0.0, u0 + 24.0, 8.0], normal: [0.0, -1.0, 0.0], light: 0.5 },
    ]
}

fn rotate(v: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // yaw around Y, then pitch around X
    let x = v[0] * cy + v[2] * sy;
    let z = -v[0] * sy + v[2] * cy;
    let y = v[1] * cp - z * sp;
    let z = v[1] * sp + z * cp;
    [x, y, z]
}

/// Draw a 3D head centred in `rect`. `yaw`/`pitch` in radians; a pleasant default is
/// yaw = -0.6, pitch = 0.35 (three-quarter view from slightly above).
pub fn paint_head_3d(painter: &egui::Painter, rect: Rect, texture: TextureId, yaw: f32, pitch: f32) {
    let center = rect.center();
    // fit the rotated cube (worst case diagonal ≈ √3) with the 1.125× hat layer into the rect
    let scale = rect.width().min(rect.height()) / (2.0 * 1.125 * 1.55);
    let mut quads: Vec<(f32, [Pos2; 4], [Pos2; 4], Color32)> = Vec::with_capacity(12);

    for (layer, size) in [(0.0, 1.0f32), (32.0, 1.125f32)] {
        for face in head_faces(layer) {
            let n = rotate(face.normal, yaw, pitch);
            if n[2] <= 0.02 {
                continue; // back-face culling
            }
            let mut pts = [Pos2::ZERO; 4];
            let mut depth = 0.0;
            for (i, c) in face.corners.iter().enumerate() {
                let r = rotate([c[0] * size, c[1] * size, c[2] * size], yaw, pitch);
                pts[i] = pos2(center.x + r[0] * scale, center.y - r[1] * scale);
                depth += r[2];
            }
            let [x0, y0, x1, y1] = face.uv;
            // Inset UVs by a hair so nearest sampling never bleeds into neighbouring faces.
            let e = 0.02;
            let uv = [
                pos2((x0 + e) / 64.0, (y0 + e) / 64.0),
                pos2((x1 - e) / 64.0, (y0 + e) / 64.0),
                pos2((x1 - e) / 64.0, (y1 - e) / 64.0),
                pos2((x0 + e) / 64.0, (y1 - e) / 64.0),
            ];
            // Light from the upper left, plus the face's base shade.
            let l = (face.light * (0.78 + 0.22 * n[2])).clamp(0.0, 1.0);
            let g = (l * 255.0) as u8;
            // Outer layer is drawn after the inner one at the same orientation.
            quads.push((depth / 4.0 + layer, pts, uv, Color32::from_rgb(g, g, g)));
        }
    }
    quads.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut mesh = Mesh::with_texture(texture);
    for (_, pts, uv, color) in quads {
        let base = mesh.vertices.len() as u32;
        for i in 0..4 {
            mesh.vertices.push(Vertex { pos: pts[i], uv: uv[i], color });
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(mesh);
}

/// A 64×64 skin generated from the nickname, so names without a real skin still get a head.
pub fn procedural_skin(name: &str) -> Vec<u8> {
    let h = hash32(name);
    let tones = [
        Color32::from_rgb(247, 206, 168),
        Color32::from_rgb(224, 172, 128),
        Color32::from_rgb(190, 132, 92),
        Color32::from_rgb(140, 92, 62),
        Color32::from_rgb(98, 64, 44),
    ];
    let skin = tones[(h % 5) as usize];
    let hair = hash_color(&format!("{name}#hair"), 0.55, 0.42);
    let hair_dark = lerp_color(hair, Color32::BLACK, 0.25);
    let eyes = hash_color(&format!("{name}#eyes"), 0.7, 0.85);
    let shade = lerp_color(skin, Color32::BLACK, 0.18);
    let mouth = lerp_color(skin, Color32::from_rgb(90, 40, 40), 0.55);
    let style = (h >> 8) % 3;

    let mut px = vec![0u8; 64 * 64 * 4];
    let mut set = |x: u32, y: u32, c: Color32| {
        let i = ((y * 64 + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&[c.r(), c.g(), c.b(), 255]);
    };
    // Base: skin everywhere on the head
    for y in 0..16 {
        for x in 0..32 {
            if y >= 8 || (8..24).contains(&x) {
                set(x, y, skin);
            }
        }
    }
    // Top + back hair
    for y in 0..8 {
        for x in 8..16 {
            set(x, y, if (x + y + h).is_multiple_of(5) { hair_dark } else { hair });
        }
    }
    for y in 8..16 {
        for x in 24..32 {
            set(x, y, if (x * 3 + y + h).is_multiple_of(7) { hair_dark } else { hair });
        }
    }
    // Sides: hair on the upper rows, longer at the back
    for (x0, back_left) in [(0u32, true), (16u32, false)] {
        for x in x0..x0 + 8 {
            let local = x - x0;
            let toward_back = if back_left { 7 - local } else { local };
            let rows = 2 + (toward_back / 2) + if style == 1 { 2 } else { 0 };
            for y in 8..(8 + rows.min(7)) {
                set(x, y, hair);
            }
        }
    }
    // Front: fringe, eyes, nose, mouth
    for x in 8..16 {
        set(x, 8, hair);
        if style != 2 || !(10..14).contains(&x) {
            set(x, 9, hair);
        }
    }
    if style == 1 {
        for y in 10..12 {
            set(8, y, hair);
            set(15, y, hair);
        }
    }
    set(9, 12, Color32::WHITE);
    set(10, 12, eyes);
    set(13, 12, eyes);
    set(14, 12, Color32::WHITE);
    set(11, 13, shade);
    set(12, 13, shade);
    for x in 10..14 {
        set(x, 14, mouth);
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_faces_visible_in_three_quarter_view() {
        let visible = head_faces(0.0).iter().filter(|f| rotate(f.normal, -0.6, 0.35)[2] > 0.02).count();
        assert_eq!(visible, 3, "front, side and top");
        let front_only = head_faces(0.0).iter().filter(|f| rotate(f.normal, 0.0, 0.0)[2] > 0.02).count();
        assert_eq!(front_only, 1);
    }

    #[test]
    fn procedural_skin_is_deterministic_and_opaque() {
        let a = procedural_skin("Steve");
        assert_eq!(a, procedural_skin("Steve"));
        assert_ne!(a, procedural_skin("Alex"));
        assert_eq!(a[(12 * 64 + 12) * 4 + 3], 255);
    }
}
