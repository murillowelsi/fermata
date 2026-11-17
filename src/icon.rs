//! The app icon: a white fermata (hold sign) on a rounded blue square, drawn in code (no image file needed).

/// Distance from point to a line segment.
#[allow(dead_code)]
fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (cx, cy) = (ax + dx * t, ay + dy * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Coverage (0..1) of the fermata (a hold sign: an arch over a dot) at a point in unit coordinates.
fn note_coverage(x: f32, y: f32) -> f32 {
    let (cx, cy) = (0.5, 0.68);
    // the arch: inside the outer half-ellipse, outside the inner one (thick at the top, thin at the ends)
    let outer = ((x - cx) / 0.34).powi(2) + ((y - cy) / 0.36).powi(2);
    let inner = ((x - cx) / 0.30).powi(2) + ((y - cy) / 0.25).powi(2);
    if y <= cy && outer <= 1.0 && inner >= 1.0 {
        return 1.0;
    }
    // rounded ends of the arch
    for ex in [cx - 0.32, cx + 0.32] {
        if (x - ex).powi(2) + (y - cy).powi(2) <= 0.022f32.powi(2) {
            return 1.0;
        }
    }
    // the dot under the arch
    if (x - cx).powi(2) + (y - (cy - 0.04)).powi(2) <= 0.065f32.powi(2) {
        return 1.0;
    }
    0.0
}

pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut out = vec![0u8; (size * size * 4) as usize];
    let ss = 4; // supersampling per axis
    for py in 0..size {
        for px in 0..size {
            let mut bg_cov = 0.0;
            let mut note_cov = 0.0;
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (px as f32 + (sx as f32 + 0.5) / ss as f32) / s;
                    let y = (py as f32 + (sy as f32 + 0.5) / ss as f32) / s;
                    // rounded square with a small margin
                    let m = 0.06;
                    let r = 0.20;
                    let qx = (x - 0.5).abs() - (0.5 - m - r);
                    let qy = (y - 0.5).abs() - (0.5 - m - r);
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
                    if d <= 0.0 {
                        bg_cov += 1.0;
                        note_cov += note_coverage(x, y);
                    }
                }
            }
            let n = (ss * ss) as f32;
            let bg = bg_cov / n;
            let nc = note_cov / n;
            let t = py as f32 / s;
            // blue-to-indigo gradient
            let (r0, g0, b0) = (52.0 + 30.0 * t, 110.0 - 40.0 * t, 220.0 - 40.0 * t);
            let a = bg;
            let mix = if bg > 0.0 { nc / bg } else { 0.0 };
            let r = r0 * (1.0 - mix) + 255.0 * mix;
            let g = g0 * (1.0 - mix) + 255.0 * mix;
            let b = b0 * (1.0 - mix) + 255.0 * mix;
            let i = ((py * size + px) * 4) as usize;
            out[i] = r as u8;
            out[i + 1] = g as u8;
            out[i + 2] = b as u8;
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    out
}

pub fn icon_data() -> egui::IconData {
    let size = 256;
    egui::IconData { rgba: rgba(size), width: size, height: size }
}
