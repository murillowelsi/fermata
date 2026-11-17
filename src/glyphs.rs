//! Music symbols drawn as vector shapes (no music font is bundled).
use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{pos2, vec2, Color32, Painter, Pos2, Shape, Stroke};

fn ellipse_pts(c: Pos2, rx: f32, ry: f32, rot: f32, n: usize) -> Vec<Pos2> {
    let (s, co) = rot.sin_cos();
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            pos2(c.x + x * co - y * s, c.y + x * s + y * co)
        })
        .collect()
}

pub fn notehead_width(sp: f32, value: u8) -> f32 {
    if value <= 1 { sp * 1.45 } else { sp * 1.18 }
}

/// value: 0 double whole, 1 whole, 2 half, >=3 filled.
pub fn notehead(p: &Painter, c: Pos2, sp: f32, value: u8, col: Color32) {
    notehead_on(p, c, sp, value, col, Color32::WHITE);
}

/// A notehead whose hollow (whole, half) is filled with `paper`: white on the page, the cell colour in a palette.
pub fn notehead_on(p: &Painter, c: Pos2, sp: f32, value: u8, col: Color32, paper: Color32) {
    let w = notehead_width(sp, value) / 2.0;
    let h = sp * 0.5;
    match value {
        0 | 1 => {
            p.add(Shape::convex_polygon(ellipse_pts(c, w, h, 0.0, 28), col, Stroke::NONE));
            p.add(Shape::convex_polygon(ellipse_pts(c, w * 0.42, h * 0.78, 1.0, 20), paper, Stroke::NONE));
            if value == 0 {
                let x0 = c.x - w - sp * 0.2;
                let x1 = c.x + w + sp * 0.2;
                for x in [x0, x0 + sp * 0.12, x1, x1 - sp * 0.12] {
                    p.line_segment([pos2(x, c.y - sp * 0.65), pos2(x, c.y + sp * 0.65)], Stroke::new(sp * 0.09, col));
                }
            }
        }
        2 => {
            p.add(Shape::convex_polygon(ellipse_pts(c, w, h * 0.92, -0.38, 28), col, Stroke::NONE));
            p.add(Shape::convex_polygon(ellipse_pts(c, w * 0.78, h * 0.34, -0.5, 20), paper, Stroke::NONE));
        }
        _ => {
            p.add(Shape::convex_polygon(ellipse_pts(c, w, h * 0.92, -0.38, 28), col, Stroke::NONE));
        }
    }
}

pub fn x_notehead(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let r = sp * 0.45;
    let st = Stroke::new(sp * 0.14, col);
    p.line_segment([c + vec2(-r, -r), c + vec2(r, r)], st);
    p.line_segment([c + vec2(-r, r), c + vec2(r, -r)], st);
}

fn bez(p: &Painter, pts: [Pos2; 4], w: f32, col: Color32) {
    p.add(CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(w, col)));
}

/// Filled lens between two cubic curves (ties, slurs, flags).
pub fn lens(p: &Painter, a: Pos2, b: Pos2, height: f32, thick: f32, col: Color32) {
    let d = b - a;
    let n = vec2(d.y, -d.x).normalized();
    let up = n * -height; // positive height bows upward (screen -y) for left->right
    let c1 = a + d * 0.25 + up * 1.33;
    let c2 = a + d * 0.75 + up * 1.33;
    let c1b = a + d * 0.25 + up * 1.33 * (1.0 - thick / height.abs().max(0.01));
    let c2b = a + d * 0.75 + up * 1.33 * (1.0 - thick / height.abs().max(0.01));
    let top = CubicBezierShape::from_points_stroke([a, c1, c2, b], false, Color32::TRANSPARENT, Stroke::NONE).flatten(Some(0.3));
    let bot = CubicBezierShape::from_points_stroke([b, c2b, c1b, a], false, Color32::TRANSPARENT, Stroke::NONE).flatten(Some(0.3));
    let mut pts = top;
    pts.extend(bot);
    // non-convex: draw as thick polyline fallback plus fill using triangles
    let mesh_pts = pts.clone();
    let mut mesh = egui::Mesh::default();
    let half = mesh_pts.len() / 2;
    for i in 0..half {
        mesh.colored_vertex(mesh_pts[i], col);
    }
    for i in 0..half {
        mesh.colored_vertex(mesh_pts[mesh_pts.len() - 1 - i], col);
    }
    for i in 0..half.saturating_sub(1) {
        let a0 = i as u32;
        let b0 = (half + i) as u32;
        mesh.add_triangle(a0, a0 + 1, b0);
        mesh.add_triangle(a0 + 1, b0 + 1, b0);
    }
    p.add(Shape::mesh(mesh));
}

pub fn flag(p: &Painter, stem_end: Pos2, sp: f32, count: u8, up: bool, col: Color32) {
    for i in 0..count {
        let dir = if up { 1.0 } else { -1.0 };
        let y0 = stem_end.y + dir * i as f32 * sp * 0.8;
        let s = pos2(stem_end.x, y0);
        let pts = [
            s,
            s + vec2(sp * 0.2, dir * sp * 1.0),
            s + vec2(sp * 1.4, dir * sp * 1.2),
            s + vec2(sp * 0.9, dir * sp * 2.9),
        ];
        bez(p, pts, sp * 0.22, col);
    }
}

pub fn sharp(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let thin = Stroke::new(sp * 0.1, col);
    let thick = Stroke::new(sp * 0.36, col);
    for dx in [-0.22, 0.22] {
        p.line_segment([c + vec2(dx * sp, -1.3 * sp), c + vec2(dx * sp, 1.3 * sp)], thin);
    }
    for dy in [-0.45, 0.45] {
        p.line_segment([c + vec2(-0.5 * sp, (dy + 0.15) * sp), c + vec2(0.5 * sp, (dy - 0.15) * sp)], thick);
    }
}

pub fn flat(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let st = Stroke::new(sp * 0.12, col);
    let x = c.x - 0.3 * sp;
    p.line_segment([pos2(x, c.y - 1.7 * sp), pos2(x, c.y + 0.5 * sp)], st);
    bez(
        p,
        [pos2(x, c.y + 0.5 * sp), pos2(x + 0.9 * sp, c.y - 0.1 * sp), pos2(x + 0.8 * sp, c.y - 0.7 * sp), pos2(x, c.y - 0.15 * sp)],
        sp * 0.2,
        col,
    );
}

pub fn natural(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let thin = Stroke::new(sp * 0.11, col);
    let thick = Stroke::new(sp * 0.3, col);
    p.line_segment([c + vec2(-0.3 * sp, -1.3 * sp), c + vec2(-0.3 * sp, 0.55 * sp)], thin);
    p.line_segment([c + vec2(0.3 * sp, -0.55 * sp), c + vec2(0.3 * sp, 1.3 * sp)], thin);
    p.line_segment([c + vec2(-0.3 * sp, -0.25 * sp), c + vec2(0.3 * sp, -0.5 * sp)], thick);
    p.line_segment([c + vec2(-0.3 * sp, 0.5 * sp), c + vec2(0.3 * sp, 0.25 * sp)], thick);
}

pub fn double_sharp(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let st = Stroke::new(sp * 0.14, col);
    let r = sp * 0.38;
    p.line_segment([c + vec2(-r, -r), c + vec2(r, r)], st);
    p.line_segment([c + vec2(-r, r), c + vec2(r, -r)], st);
    for (dx, dy) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
        p.circle_filled(c + vec2(dx * r, dy * r), sp * 0.15, col);
    }
}

pub fn accidental(p: &Painter, c: Pos2, sp: f32, alter: i8, col: Color32) {
    match alter {
        2 => double_sharp(p, c, sp, col),
        1 => sharp(p, c, sp, col),
        -1 => flat(p, c, sp, col),
        -2 => {
            flat(p, c + vec2(-0.35 * sp, 0.0), sp, col);
            flat(p, c + vec2(0.4 * sp, 0.0), sp, col);
        }
        _ => natural(p, c, sp, col),
    }
}

pub fn accidental_width(sp: f32, alter: i8) -> f32 {
    match alter {
        -2 => sp * 1.9,
        2 => sp * 1.1,
        _ => sp * 1.15,
    }
}

/// Treble clef: c is the position of the G line.
pub fn treble_clef(p: &Painter, g: Pos2, sp: f32, col: Color32) {
    let w = sp * 0.17;
    let x = g.x;
    let pt = |dx: f32, dy: f32| pos2(x + dx * sp, g.y + dy * sp);
    // spiral around G line
    bez(p, [pt(0.15, 0.55), pt(-0.9, 0.4), pt(-0.6, -0.8), pt(0.2, -0.6)], w * 1.4, col);
    bez(p, [pt(0.2, -0.6), pt(1.4, -0.3), pt(1.1, 1.3), pt(-0.1, 1.2)], w * 1.8, col);
    bez(p, [pt(-0.1, 1.2), pt(-1.6, 1.1), pt(-1.5, -0.8), pt(-0.1, -1.9)], w * 1.6, col);
    bez(p, [pt(-0.1, -1.9), pt(0.9, -2.8), pt(0.9, -4.3), pt(0.25, -4.3)], w * 1.3, col);
    bez(p, [pt(0.25, -4.3), pt(-0.5, -4.2), pt(-0.6, -2.8), pt(-0.25, -1.5)], w, col);
    p.line_segment([pt(-0.25, -1.5), pt(0.35, 2.4)], Stroke::new(w, col));
    bez(p, [pt(0.35, 2.4), pt(0.45, 3.2), pt(-0.3, 3.4), pt(-0.55, 2.9)], w, col);
    p.circle_filled(pt(-0.45, 2.85), sp * 0.32, col);
}

/// Bass clef: f is the position of the F line.
pub fn bass_clef(p: &Painter, f: Pos2, sp: f32, col: Color32) {
    let pt = |dx: f32, dy: f32| pos2(f.x + dx * sp, f.y + dy * sp);
    p.circle_filled(pt(-0.6, 0.0), sp * 0.36, col);
    bez(p, [pt(-0.8, -0.1), pt(-0.6, -1.3), pt(1.5, -1.3), pt(1.2, 0.4)], sp * 0.24, col);
    bez(p, [pt(1.2, 0.4), pt(1.0, 1.6), pt(-0.2, 2.4), pt(-0.9, 2.7)], sp * 0.2, col);
    p.circle_filled(pt(1.75, -0.5), sp * 0.15, col);
    p.circle_filled(pt(1.75, 0.5), sp * 0.15, col);
}

/// C clef: c is the position of the middle C line.
pub fn c_clef(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let pt = |dx: f32, dy: f32| pos2(c.x + dx * sp, c.y + dy * sp);
    p.rect_filled(egui::Rect::from_min_max(pt(-1.0, -2.0), pt(-0.6, 2.0)), 0.0, col);
    p.line_segment([pt(-0.4, -2.0), pt(-0.4, 2.0)], Stroke::new(sp * 0.12, col));
    for s in [-1.0f32, 1.0] {
        bez(p, [pt(-0.4, 0.0), pt(0.2, s * 0.3), pt(0.3, s * 0.6), pt(0.5, s * 0.9)], sp * 0.15, col);
        bez(p, [pt(0.5, s * 0.9), pt(1.6, s * 0.5), pt(1.6, s * 2.1), pt(0.6, s * 2.0)], sp * 0.3, col);
    }
}

pub fn percussion_clef(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    for dx in [-0.35, 0.35] {
        p.rect_filled(egui::Rect::from_center_size(c + vec2(dx * sp, 0.0), vec2(sp * 0.35, sp * 2.0)), 0.0, col);
    }
}

pub fn small_8(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    p.text(c, egui::Align2::CENTER_CENTER, "8", egui::FontId::proportional(sp * 1.3), col);
}

pub fn rest(p: &Painter, c: Pos2, sp: f32, value: u8, col: Color32) {
    match value {
        0 => {
            p.rect_filled(egui::Rect::from_min_max(c + vec2(-0.3 * sp, -1.0 * sp), c + vec2(0.3 * sp, 0.0)), 0.0, col);
        }
        1 => {
            p.rect_filled(egui::Rect::from_min_max(c + vec2(-0.6 * sp, -1.0 * sp), c + vec2(0.6 * sp, -0.5 * sp)), 0.0, col);
        }
        2 => {
            p.rect_filled(egui::Rect::from_min_max(c + vec2(-0.6 * sp, -0.5 * sp), c + vec2(0.6 * sp, 0.0)), 0.0, col);
        }
        3 => {
            let pts = [
                c + vec2(-0.3 * sp, -1.5 * sp),
                c + vec2(0.35 * sp, -0.7 * sp),
                c + vec2(-0.25 * sp, 0.0),
                c + vec2(0.35 * sp, 0.75 * sp),
            ];
            p.add(PathShape::line(pts.to_vec(), PathStroke::new(sp * 0.28, col)));
            bez(p, [c + vec2(0.35 * sp, 0.75 * sp), c + vec2(-0.6 * sp, 0.3 * sp), c + vec2(-0.5 * sp, 1.2 * sp), c + vec2(0.05 * sp, 1.5 * sp)], sp * 0.18, col);
        }
        _ => {
            let n = value as i32 - 3; // number of hooks
            let top = c + vec2(0.35 * sp, -0.9 * sp);
            let bottom = top + vec2(-0.55 * sp * n as f32 * 0.6 - 0.4 * sp, (n as f32 + 0.6) * sp);
            p.line_segment([top, bottom], Stroke::new(sp * 0.13, col));
            for i in 0..n {
                let t = i as f32 / (n as f32 + 0.6);
                let hp = top + (bottom - top) * t;
                p.circle_filled(hp + vec2(-0.75 * sp, 0.05 * sp), sp * 0.25, col);
                bez(p, [hp + vec2(-0.75 * sp, 0.15 * sp), hp + vec2(-0.4 * sp, 0.35 * sp), hp + vec2(-0.1 * sp, 0.25 * sp), hp], sp * 0.1, col);
            }
        }
    }
}

pub fn fermata(p: &Painter, c: Pos2, sp: f32, below: bool, col: Color32) {
    let s = if below { -1.0 } else { 1.0 };
    bez(
        p,
        [c + vec2(-1.1 * sp, 0.0), c + vec2(-1.0 * sp, -1.4 * sp * s), c + vec2(1.0 * sp, -1.4 * sp * s), c + vec2(1.1 * sp, 0.0)],
        sp * 0.17,
        col,
    );
    p.circle_filled(c + vec2(0.0, -0.25 * sp * s), sp * 0.2, col);
}

pub fn segno(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    bez(p, [c + vec2(0.6 * sp, -0.9 * sp), c + vec2(-0.2 * sp, -1.6 * sp), c + vec2(-1.2 * sp, -0.4 * sp), c], sp * 0.18, col);
    bez(p, [c, c + vec2(1.2 * sp, 0.4 * sp), c + vec2(0.2 * sp, 1.6 * sp), c + vec2(-0.6 * sp, 0.9 * sp)], sp * 0.18, col);
    p.line_segment([c + vec2(-0.8 * sp, 1.0 * sp), c + vec2(0.8 * sp, -1.0 * sp)], Stroke::new(sp * 0.1, col));
    p.circle_filled(c + vec2(-0.7 * sp, -0.2 * sp), sp * 0.13, col);
    p.circle_filled(c + vec2(0.7 * sp, 0.2 * sp), sp * 0.13, col);
}

pub fn coda(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    p.add(Shape::convex_polygon(ellipse_pts(c, sp * 0.7, sp * 0.9, 0.0, 24), Color32::TRANSPARENT, Stroke::new(sp * 0.15, col)));
    let st = Stroke::new(sp * 0.1, col);
    p.line_segment([c + vec2(-1.2 * sp, 0.0), c + vec2(1.2 * sp, 0.0)], st);
    p.line_segment([c + vec2(0.0, -1.3 * sp), c + vec2(0.0, 1.3 * sp)], st);
}

pub fn brace(p: &Painter, top: Pos2, bottom: Pos2, sp: f32, col: Color32) {
    let mid = pos2(top.x, (top.y + bottom.y) / 2.0);
    let w = sp * 0.9;
    for (a, b, s) in [(top, mid, 1.0f32), (bottom, mid, -1.0)] {
        let h = (b.y - a.y).abs();
        let _ = s;
        let dir = if b.y > a.y { 1.0 } else { -1.0 };
        bez(
            p,
            [a, pos2(a.x - w, a.y + dir * h * 0.25), pos2(a.x + w * 0.4, b.y - dir * h * 0.35), pos2(b.x - w * 0.9, b.y)],
            sp * 0.3,
            col,
        );
    }
}

pub fn bracket(p: &Painter, top: Pos2, bottom: Pos2, sp: f32, col: Color32) {
    let x = top.x;
    p.rect_filled(egui::Rect::from_min_max(pos2(x - sp * 0.5, top.y - sp * 0.3), pos2(x, bottom.y + sp * 0.3)), 0.0, col);
    bez(p, [pos2(x - 0.3 * sp, top.y - 0.3 * sp), pos2(x + 0.4 * sp, top.y - 0.3 * sp), pos2(x + 0.9 * sp, top.y - 0.6 * sp), pos2(x + 1.2 * sp, top.y - 1.0 * sp)], sp * 0.15, col);
    bez(p, [pos2(x - 0.3 * sp, bottom.y + 0.3 * sp), pos2(x + 0.4 * sp, bottom.y + 0.3 * sp), pos2(x + 0.9 * sp, bottom.y + 0.6 * sp), pos2(x + 1.2 * sp, bottom.y + 1.0 * sp)], sp * 0.15, col);
}

pub fn time_sig_digit(p: &Painter, c: Pos2, sp: f32, txt: &str, col: Color32) {
    // bold-ish by drawing twice slightly offset
    let f = egui::FontId::proportional(sp * 2.6);
    for dx in [-0.04, 0.04] {
        p.text(c + vec2(dx * sp, 0.0), egui::Align2::CENTER_CENTER, txt, f.clone(), col);
    }
}

pub fn tie_or_slur(p: &Painter, a: Pos2, b: Pos2, sp: f32, above: bool, col: Color32) {
    let len = (b - a).length();
    let h = (sp * 0.5 + len * 0.08).min(sp * 2.0);
    let (a2, b2) = if b.x >= a.x { (a, b) } else { (b, a) };
    // lens bows downward for a positive height, so a tie or slur above takes a negative one
    lens(p, a2, b2, if above { -h } else { h }, sp * 0.18, col);
}

pub fn hairpin(p: &Painter, a: Pos2, b: Pos2, sp: f32, cresc: bool, col: Color32) {
    let st = Stroke::new(sp * 0.12, col);
    let open = sp * 0.75;
    if cresc {
        p.line_segment([a, b + vec2(0.0, -open)], st);
        p.line_segment([a, b + vec2(0.0, open)], st);
    } else {
        p.line_segment([a + vec2(0.0, -open), b], st);
        p.line_segment([a + vec2(0.0, open), b], st);
    }
}

pub fn trill_wave(p: &Painter, a: Pos2, b: Pos2, sp: f32, col: Color32) {
    let mut pts = vec![];
    let n = ((b.x - a.x) / (sp * 0.4)).max(2.0) as usize;
    for i in 0..=n {
        let x = a.x + (b.x - a.x) * i as f32 / n as f32;
        let y = a.y + if i % 2 == 0 { -sp * 0.2 } else { sp * 0.2 };
        pts.push(pos2(x, y));
    }
    p.add(PathShape::line(pts, PathStroke::new(sp * 0.12, col)));
}

pub fn dashed_line(p: &Painter, a: Pos2, b: Pos2, w: f32, dash: f32, col: Color32) {
    let d = b - a;
    let len = d.length();
    if len < 0.5 {
        return;
    }
    let dir = d / len;
    let mut t = 0.0;
    while t < len {
        let e = (t + dash).min(len);
        p.line_segment([a + dir * t, a + dir * e], Stroke::new(w, col));
        t += dash * 2.0;
    }
}

pub fn mark(p: &Painter, c: Pos2, sp: f32, m: crate::model::Mark, above: bool, col: Color32) {
    use crate::model::Mark::*;
    let s = if above { 1.0 } else { -1.0 };
    match m {
        Staccato => {
            p.circle_filled(c, sp * 0.2, col);
        }
        Staccatissimo => {
            p.add(Shape::convex_polygon(
                vec![c + vec2(-0.2 * sp, -0.5 * sp * s), c + vec2(0.2 * sp, -0.5 * sp * s), c + vec2(0.0, 0.3 * sp * s)],
                col,
                Stroke::NONE,
            ));
        }
        Tenuto => {
            p.line_segment([c + vec2(-0.6 * sp, 0.0), c + vec2(0.6 * sp, 0.0)], Stroke::new(sp * 0.16, col));
        }
        Accent => {
            let st = Stroke::new(sp * 0.15, col);
            p.line_segment([c + vec2(-0.7 * sp, -0.4 * sp), c + vec2(0.7 * sp, 0.0)], st);
            p.line_segment([c + vec2(-0.7 * sp, 0.4 * sp), c + vec2(0.7 * sp, 0.0)], st);
        }
        Marcato => {
            let st = Stroke::new(sp * 0.2, col);
            p.line_segment([c + vec2(-0.5 * sp, 0.4 * sp * s), c + vec2(0.0, -0.6 * sp * s)], st);
            p.line_segment([c + vec2(0.5 * sp, 0.4 * sp * s), c + vec2(0.0, -0.6 * sp * s)], st);
        }
        Fermata => fermata(p, c + vec2(0.0, 0.3 * sp * s), sp, false, col),
        FermataBelow => fermata(p, c, sp, true, col),
        Trill => {
            p.text(c, egui::Align2::CENTER_CENTER, "tr", egui::FontId::proportional(sp * 1.8), col);
        }
        Turn => {
            bez(p, [c + vec2(-1.0 * sp, 0.2 * sp), c + vec2(-1.0 * sp, -0.8 * sp), c + vec2(0.0, -0.4 * sp), c], sp * 0.15, col);
            bez(p, [c, c + vec2(0.0, 0.4 * sp), c + vec2(1.0 * sp, 0.8 * sp), c + vec2(1.0 * sp, -0.2 * sp)], sp * 0.15, col);
        }
        Mordent | InvMordent => {
            let pts = vec![
                c + vec2(-1.0 * sp, 0.2 * sp),
                c + vec2(-0.5 * sp, -0.4 * sp),
                c + vec2(0.0, 0.2 * sp),
                c + vec2(0.5 * sp, -0.4 * sp),
                c + vec2(1.0 * sp, 0.2 * sp),
            ];
            p.add(PathShape::line(pts, PathStroke::new(sp * 0.18, col)));
            if m == Mordent {
                p.line_segment([c + vec2(0.0, -0.8 * sp), c + vec2(0.0, 0.7 * sp)], Stroke::new(sp * 0.1, col));
            }
        }
        UpBow => {
            let st = Stroke::new(sp * 0.13, col);
            p.line_segment([c + vec2(-0.5 * sp, -0.7 * sp), c + vec2(0.0, 0.6 * sp)], st);
            p.line_segment([c + vec2(0.5 * sp, -0.7 * sp), c + vec2(0.0, 0.6 * sp)], st);
        }
        DownBow => {
            let r = egui::Rect::from_center_size(c, vec2(sp * 1.2, sp * 1.0));
            p.rect_filled(egui::Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + sp * 0.3)), 0.0, col);
            p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(sp * 0.12, col));
            p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(sp * 0.12, col));
        }
        Harmonic => {
            p.circle_stroke(c, sp * 0.35, Stroke::new(sp * 0.12, col));
        }
        Plus => {
            let st = Stroke::new(sp * 0.15, col);
            p.line_segment([c + vec2(-0.5 * sp, 0.0), c + vec2(0.5 * sp, 0.0)], st);
            p.line_segment([c + vec2(0.0, -0.5 * sp), c + vec2(0.0, 0.5 * sp)], st);
        }
        Breath => {
            p.text(c, egui::Align2::CENTER_CENTER, ",", egui::FontId::proportional(sp * 2.5), col);
        }
        Caesura => {
            let st = Stroke::new(sp * 0.16, col);
            p.line_segment([c + vec2(-0.5 * sp, 0.6 * sp), c + vec2(0.0, -0.8 * sp)], st);
            p.line_segment([c + vec2(0.0, 0.6 * sp), c + vec2(0.5 * sp, -0.8 * sp)], st);
        }
        Tremolo1 | Tremolo2 | Tremolo3 => {
            let n = match m {
                Tremolo1 => 1,
                Tremolo2 => 2,
                _ => 3,
            };
            for i in 0..n {
                let y = (i as f32 - (n - 1) as f32 / 2.0) * sp * 0.6;
                p.line_segment([c + vec2(-0.5 * sp, y + 0.25 * sp), c + vec2(0.5 * sp, y - 0.25 * sp)], Stroke::new(sp * 0.25, col));
            }
        }
        Finger(n) => {
            p.text(c, egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(sp * 1.5), col);
        }
        Segno => segno(p, c, sp * 0.8, col),
        Coda => coda(p, c, sp * 0.8, col),
        PedalDown => {
            p.text(c, egui::Align2::CENTER_CENTER, "Ped.", egui::FontId::proportional(sp * 1.5), col);
        }
        PedalUp => {
            p.text(c, egui::Align2::CENTER_CENTER, "*", egui::FontId::proportional(sp * 2.0), col);
        }
        Arpeggio => {
            let mut pts = vec![];
            for i in 0..8 {
                pts.push(c + vec2(if i % 2 == 0 { -0.2 * sp } else { 0.2 * sp }, (i as f32 - 3.5) * sp * 0.4));
            }
            p.add(PathShape::line(pts, PathStroke::new(sp * 0.12, col)));
        }
    }
}
