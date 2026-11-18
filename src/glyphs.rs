//! Music symbols on the score page. Since F-03 every symbol is a glyph of the Bravura font (SMuFL), placed by the
//! font's metadata (see `smufl.rs`); what isn't a glyph stays drawn here as vector shapes: ties and slurs, hairpins,
//! trill lines, dashed lines, braces and brackets.
use crate::smufl as s;
use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{pos2, vec2, Color32, Painter, Pos2, Shape, Stroke, Vec2};

/// The SMuFL notehead for a value: 0 double whole, 1 whole, 2 half, 3 and shorter black.
pub fn notehead_name(value: u8) -> &'static str {
    match value {
        0 => "noteheadDoubleWhole",
        1 => "noteheadWhole",
        2 => "noteheadHalf",
        _ => "noteheadBlack",
    }
}

pub fn notehead_width(sp: f32, value: u8) -> f32 {
    s::advance(notehead_name(value)) * sp
}

/// A notehead centred on `c` (c.y is its line or space).
pub fn notehead(p: &Painter, c: Pos2, sp: f32, value: u8, col: Color32) {
    let w = notehead_width(sp, value);
    s::draw(p, notehead_name(value), pos2(c.x - w / 2.0, c.y), sp, col);
}

pub fn x_notehead(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    let w = s::advance("noteheadXBlack") * sp;
    s::draw(p, "noteheadXBlack", pos2(c.x - w / 2.0, c.y), sp, col);
}

/// Where a stem meets a notehead centred on `c`: (x of the stem's centre line, y where it starts). The stem's edge
/// lies on the font's anchor (`stemUpSE` / `stemDownNW`).
pub fn stem_attach(c: Pos2, sp: f32, value: u8, up: bool, stem_w: f32) -> Pos2 {
    let name = notehead_name(value);
    let left = c.x - notehead_width(sp, value) / 2.0;
    if up {
        let a = s::anchor(name, "stemUpSE").unwrap_or(vec2(1.18, 0.168));
        pos2(left + a.x * sp - stem_w / 2.0, c.y - a.y * sp)
    } else {
        let a = s::anchor(name, "stemDownNW").unwrap_or(vec2(0.0, -0.168));
        pos2(left + a.x * sp + stem_w / 2.0, c.y - a.y * sp)
    }
}

/// The flag of an unbeamed note: `count` 1 (eighth) to 5 (128th), on the stem ending at `stem_end` (the stem's
/// centre line; stem 0.12 spaces wide). Joined by the flag's anchor.
pub fn flag(p: &Painter, stem_end: Pos2, sp: f32, count: u8, up: bool, col: Color32) {
    const UP: [&str; 5] = ["flag8thUp", "flag16thUp", "flag32ndUp", "flag64thUp", "flag128thUp"];
    const DOWN: [&str; 5] = ["flag8thDown", "flag16thDown", "flag32ndDown", "flag64thDown", "flag128thDown"];
    let i = (count.clamp(1, 5) - 1) as usize;
    let stem_left = stem_end.x - sp * 0.06;
    let (name, key) = if up { (UP[i], "stemUpNW") } else { (DOWN[i], "stemDownSW") };
    let a = s::anchor(name, key).unwrap_or(Vec2::ZERO);
    s::draw(p, name, pos2(stem_left - a.x * sp, stem_end.y + a.y * sp), sp, col);
}

fn accidental_name(alter: i8) -> &'static str {
    match alter {
        2 => "accidentalDoubleSharp",
        1 => "accidentalSharp",
        -1 => "accidentalFlat",
        -2 => "accidentalDoubleFlat",
        _ => "accidentalNatural",
    }
}

/// An accidental centred horizontally on `c.x`, on the line or space `c.y`.
pub fn accidental(p: &Painter, c: Pos2, sp: f32, alter: i8, col: Color32) {
    s::draw_hcentered(p, accidental_name(alter), c.x, c.y, sp, col);
}

pub fn sharp(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    accidental(p, c, sp, 1, col);
}
pub fn flat(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    accidental(p, c, sp, -1, col);
}
pub fn natural(p: &Painter, c: Pos2, sp: f32, col: Color32) {
    accidental(p, c, sp, 0, col);
}

/// An accidental's ink: how far it reaches above and below its line or space, and its width.
pub fn accidental_box(sp: f32, alter: i8) -> (f32, f32, f32) {
    let (sw, ne) = s::bbox(accidental_name(alter));
    (ne.y * sp, -sw.y * sp, (ne.x - sw.x) * sp)
}

/// The ink rectangle of a rest drawn by `rest` at `c`.
pub fn rest_rect(c: Pos2, sp: f32, value: u8) -> egui::Rect {
    const NAMES: [&str; 9] = ["restDoubleWhole", "restWhole", "restHalf", "restQuarter", "rest8th", "rest16th", "rest32nd", "rest64th", "rest128th"];
    let (sw, ne) = s::bbox(NAMES[(value as usize).min(8)]);
    let y = if value == 1 { c.y - sp } else { c.y };
    let w = (ne.x - sw.x) * sp;
    egui::Rect::from_min_max(pos2(c.x - w / 2.0, y - ne.y * sp), pos2(c.x + w / 2.0, y - sw.y * sp))
}

/// How far an accidental's ink reaches above its line or space.
pub fn accidental_height_above(sp: f32, alter: i8) -> f32 {
    s::bbox(accidental_name(alter)).1.y * sp
}

/// The ink width of an accidental.
pub fn accidental_width(sp: f32, alter: i8) -> f32 {
    let (sw, ne) = s::bbox(accidental_name(alter));
    (ne.x - sw.x) * sp
}

/// A clef glyph centred horizontally on `x`, its origin on the line it names (`y`).
pub fn clef(p: &Painter, name: &str, x: f32, y: f32, sp: f32, col: Color32) {
    s::draw_hcentered(p, name, x, y, sp, col);
}

/// A rest centred on `c.x`; `c.y` is the middle line (or where a voice's rests sit). The whole rest hangs from the
/// line above, the half rest stands on the line, the double whole fills the space above.
pub fn rest(p: &Painter, c: Pos2, sp: f32, value: u8, col: Color32) {
    const NAMES: [&str; 9] = ["restDoubleWhole", "restWhole", "restHalf", "restQuarter", "rest8th", "rest16th", "rest32nd", "rest64th", "rest128th"];
    let name = NAMES[(value as usize).min(8)];
    let y = if value == 1 { c.y - sp } else { c.y };
    s::draw_hcentered(p, name, c.x, y, sp, col);
}

/// A time signature number (or a multi-measure rest's count) centred on `c` (its middle on `c.y`).
pub fn time_sig_digit(p: &Painter, c: Pos2, sp: f32, txt: &str, col: Color32) {
    const D: [&str; 10] = ["timeSig0", "timeSig1", "timeSig2", "timeSig3", "timeSig4", "timeSig5", "timeSig6", "timeSig7", "timeSig8", "timeSig9"];
    let names: Vec<&str> = txt.chars().filter_map(|ch| ch.to_digit(10)).map(|d| D[d as usize]).collect();
    s::draw_run(p, &names, c.x, c.y, sp, col);
}

/// The width of a dynamic as `dynamic` draws it.
pub fn dynamic_width(sp: f32, t: &str) -> f32 {
    let names: Option<Vec<&str>> = t.chars().map(s::dynamic_glyph).collect();
    match names {
        Some(n) if !n.is_empty() => n.iter().map(|g| s::advance(g)).sum::<f32>() * sp,
        _ => t.chars().count() as f32 * sp * 0.9,
    }
}

/// A dynamic ("p", "mf", "sfz") with the font's dynamic letters, centred on `c`; other words in italic-looking text.
pub fn dynamic(p: &Painter, c: Pos2, sp: f32, t: &str, col: Color32) {
    let names: Option<Vec<&str>> = t.chars().map(s::dynamic_glyph).collect();
    match names {
        Some(n) if !n.is_empty() => {
            // the letters sit on the baseline: lower it so the word's middle is on c.y
            s::draw_run(p, &n, c.x, c.y + sp * 0.3, sp, col);
        }
        _ => {
            let f = crate::score_view::page_font(sp * 1.8);
            p.text(c, egui::Align2::CENTER_CENTER, t, f, col);
        }
    }
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

/// A mark: articulations (the above or below form), fermatas, ornaments, bowings, fingering, segno, coda, pedal
/// marks. A mark above the note has the bottom of its ink half a space below `c.y`, one below has its top half a
/// space above, so a small mark sits on `c` and a tall one (coda, segno) grows away from the note. Tremolos,
/// breath marks and caesuras are centred on `c`; the arpeggio stays a drawn wavy line. Returns the mark's height.
pub fn mark(p: &Painter, c: Pos2, sp: f32, m: crate::model::Mark, above: bool, col: Color32) -> f32 {
    use crate::model::Mark::*;
    let pick = |a: &'static str, b: &'static str| if above { a } else { b };
    let name: &str = match m {
        Staccato => pick("articStaccatoAbove", "articStaccatoBelow"),
        Staccatissimo => pick("articStaccatissimoAbove", "articStaccatissimoBelow"),
        Tenuto => pick("articTenutoAbove", "articTenutoBelow"),
        Accent => pick("articAccentAbove", "articAccentBelow"),
        Marcato => pick("articMarcatoAbove", "articMarcatoBelow"),
        Fermata => "fermataAbove",
        FermataBelow => "fermataBelow",
        Trill => "ornamentTrill",
        Turn => "ornamentTurn",
        Mordent => "ornamentMordent",
        InvMordent => "ornamentShortTrill",
        UpBow => "stringsUpBow",
        DownBow => "stringsDownBow",
        Harmonic => "stringsHarmonic",
        Plus => "brassMuteClosed",
        Breath => "breathMarkComma",
        Caesura => "caesura",
        Tremolo1 => "tremolo1",
        Tremolo2 => "tremolo2",
        Tremolo3 => "tremolo3",
        Finger(n) => ["fingering0", "fingering1", "fingering2", "fingering3", "fingering4", "fingering5"][(n as usize).min(5)],
        Segno => "segno",
        Coda => "coda",
        PedalDown => "keyboardPedalPed",
        PedalUp => "keyboardPedalUp",
        Arpeggio => {
            let mut pts = vec![];
            for i in 0..8 {
                pts.push(c + vec2(if i % 2 == 0 { -0.2 * sp } else { 0.2 * sp }, (i as f32 - 3.5) * sp * 0.4));
            }
            p.add(PathShape::line(pts, PathStroke::new(sp * 0.12, col)));
            return sp * 3.2;
        }
    };
    // segno and coda at the size of text marks (about 1.75 spaces tall), not at the font's full size
    let sp = if matches!(m, Segno | Coda) {
        let (sw, ne) = s::bbox(name);
        sp * 1.75 / (ne.y - sw.y)
    } else {
        sp
    };
    let (sw, ne) = s::bbox(name);
    let h = (ne.y - sw.y) * sp;
    if matches!(m, Tremolo1 | Tremolo2 | Tremolo3 | Breath | Caesura) {
        s::draw_centered(p, name, c, sp, col);
    } else if above {
        // ink bottom at c.y + sp/2: the baseline goes sw.y spaces below it (y up in the font)
        s::draw_hcentered(p, name, c.x, c.y + sp * 0.5 + sw.y * sp, sp, col);
    } else {
        s::draw_hcentered(p, name, c.x, c.y - sp * 0.5 + ne.y * sp, sp, col);
    }
    h
}
