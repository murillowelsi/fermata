//! The 11 floating palettes (F-13, direction A): rounded panels with a dot-and-name header, borderless cells and
//! symbols that always fit whole inside their cells.
//!
//! Fit rule (spec §2.3): each cell has an equal inner margin `m = max(3 pt, 12% of its shorter side)`; every group
//! of symbols in a palette (rests, notes, accidentals, clefs...) uses one staff space, the group's design size
//! reduced until the group's largest ink box fits the inner area; each symbol is centred by its own ink box, with
//! the origin rounded to the physical pixel. Nothing is clipped.
//!
//! Music symbols are SMuFL glyphs from Bravura (ink boxes from `Bravura.json`, the font's metadata); lines, shapes,
//! the staff behind the clefs and the text labels (Inter) are drawn in code.
use crate::app::*;
use crate::commands::PALETTES;
use crate::model::*;
use crate::theme;
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use std::collections::HashMap;
use std::sync::OnceLock;

// ---------------- Bravura ----------------

#[cfg(not(target_arch = "wasm32"))]
pub const BRAVURA: &[u8] = include_bytes!("../assets/fonts/Bravura.otf");
#[cfg(not(target_arch = "wasm32"))]
const BRAVURA_META: &str = include_str!("../assets/fonts/Bravura.json");
// the browser: only the palettes' glyphs and their boxes, a Bravura subset renamed "Fermata Palette Glyphs" as the
// OFL asks of a modified version (tools/subset_web_fonts.sh)
#[cfg(target_arch = "wasm32")]
pub const BRAVURA: &[u8] = include_bytes!("../assets/fonts/web/FermataPaletteGlyphs.otf");
#[cfg(target_arch = "wasm32")]
const BRAVURA_META: &str = include_str!("../assets/fonts/web/FermataPaletteGlyphs.json");
pub const BRAVURA_FAMILY: &str = "bravura";

/// The SMuFL glyphs the palettes use: code point and SMuFL name (the key into the metadata).
const GLYPHS: &[(char, &str)] = &[
    ('\u{E4E2}', "restDoubleWhole"),
    ('\u{E4E3}', "restWhole"),
    ('\u{E4E4}', "restHalf"),
    ('\u{E4E5}', "restQuarter"),
    ('\u{E4E6}', "rest8th"),
    ('\u{E4E7}', "rest16th"),
    ('\u{E4E8}', "rest32nd"),
    ('\u{E4E9}', "rest64th"),
    ('\u{E4EA}', "rest128th"),
    ('\u{E1D0}', "noteDoubleWhole"),
    ('\u{E1D2}', "noteWhole"),
    ('\u{E1D3}', "noteHalfUp"),
    ('\u{E1D5}', "noteQuarterUp"),
    ('\u{E1D7}', "note8thUp"),
    ('\u{E1D9}', "note16thUp"),
    ('\u{E1DB}', "note32ndUp"),
    ('\u{E1DD}', "note64thUp"),
    ('\u{E1DF}', "note128thUp"),
    ('\u{E263}', "accidentalDoubleSharp"),
    ('\u{E262}', "accidentalSharp"),
    ('\u{E264}', "accidentalDoubleFlat"),
    ('\u{E260}', "accidentalFlat"),
    ('\u{E261}', "accidentalNatural"),
    ('\u{E050}', "gClef"),
    ('\u{E062}', "fClef"),
    ('\u{E05C}', "cClef"),
    ('\u{E053}', "gClef8va"),
    ('\u{E052}', "gClef8vb"),
    ('\u{E064}', "fClef8vb"),
    ('\u{E069}', "unpitchedPercussionClef1"),
    ('\u{E520}', "dynamicPiano"),
    ('\u{E521}', "dynamicMezzo"),
    ('\u{E522}', "dynamicForte"),
    ('\u{E523}', "dynamicRinforzando"),
    ('\u{E524}', "dynamicSforzando"),
    ('\u{E525}', "dynamicZ"),
    ('\u{E566}', "ornamentTrill"),
    ('\u{E567}', "ornamentTurn"),
    ('\u{E56D}', "ornamentMordent"),
    ('\u{E56C}', "ornamentShortTrill"),
    ('\u{E5E5}', "brassMuteClosed"),
    ('\u{E614}', "stringsHarmonic"),
    ('\u{E612}', "stringsUpBow"),
    ('\u{E610}', "stringsDownBow"),
    ('\u{E4A4}', "articTenutoAbove"),
    ('\u{E4A2}', "articStaccatoAbove"),
    ('\u{E4A0}', "articAccentAbove"),
    ('\u{E4AC}', "articMarcatoAbove"),
    ('\u{E4A6}', "articStaccatissimoAbove"),
    ('\u{E4C0}', "fermataAbove"),
    ('\u{E4C1}', "fermataBelow"),
    ('\u{E220}', "tremolo1"),
    ('\u{E221}', "tremolo2"),
    ('\u{E222}', "tremolo3"),
    ('\u{ED10}', "fingering0"),
    ('\u{ED11}', "fingering1"),
    ('\u{ED12}', "fingering2"),
    ('\u{ED13}', "fingering3"),
    ('\u{ED14}', "fingering4"),
    ('\u{ED15}', "fingering5"),
    ('\u{E047}', "segno"),
    ('\u{E048}', "coda"),
    ('\u{E4CE}', "breathMarkComma"),
    ('\u{E4D1}', "caesura"),
    ('\u{E650}', "keyboardPedalPed"),
    ('\u{E655}', "keyboardPedalUp"),
    ('\u{E511}', "ottavaAlta"),
    ('\u{E51C}', "ottavaBassaVb"),
];

/// Ink box of a SMuFL glyph in staff spaces, y up, from the font's metadata: (south-west, north-east).
pub fn glyph_bbox(c: char) -> Option<(Vec2, Vec2)> {
    static MAP: OnceLock<HashMap<char, (Vec2, Vec2)>> = OnceLock::new();
    MAP.get_or_init(|| {
        let meta: serde_json::Value = serde_json::from_str(BRAVURA_META).unwrap_or_default();
        let boxes = &meta["glyphBBoxes"];
        let mut m = HashMap::new();
        for (c, name) in GLYPHS {
            let b = &boxes[*name];
            let num = |k: &str, i: usize| b[k][i].as_f64().map(|v| v as f32);
            if let (Some(a), Some(bb), Some(c2), Some(d)) = (num("bBoxSW", 0), num("bBoxSW", 1), num("bBoxNE", 0), num("bBoxNE", 1)) {
                m.insert(*c, (vec2(a, bb), vec2(c2, d)));
            }
        }
        m
    })
    .get(&c)
    .copied()
}

const REST: [char; 9] = ['\u{E4E2}', '\u{E4E3}', '\u{E4E4}', '\u{E4E5}', '\u{E4E6}', '\u{E4E7}', '\u{E4E8}', '\u{E4E9}', '\u{E4EA}'];
const NOTE: [char; 9] = ['\u{E1D0}', '\u{E1D2}', '\u{E1D3}', '\u{E1D5}', '\u{E1D7}', '\u{E1D9}', '\u{E1DB}', '\u{E1DD}', '\u{E1DF}'];

fn acc_glyph(a: i8) -> char {
    match a {
        2 => '\u{E263}',
        1 => '\u{E262}',
        -2 => '\u{E264}',
        -1 => '\u{E260}',
        _ => '\u{E261}',
    }
}

fn dyn_glyphs(d: &str) -> String {
    d.chars()
        .map(|c| match c {
            'p' => '\u{E520}',
            'm' => '\u{E521}',
            'f' => '\u{E522}',
            'r' => '\u{E523}',
            's' => '\u{E524}',
            _ => '\u{E525}',
        })
        .collect()
}

fn mark_glyph(m: Mark) -> Option<char> {
    Some(match m {
        Mark::Trill => '\u{E566}',
        Mark::Turn => '\u{E567}',
        Mark::Mordent => '\u{E56D}',
        Mark::InvMordent => '\u{E56C}',
        Mark::Plus => '\u{E5E5}',
        Mark::Harmonic => '\u{E614}',
        Mark::UpBow => '\u{E612}',
        Mark::DownBow => '\u{E610}',
        Mark::Tenuto => '\u{E4A4}',
        Mark::Staccato => '\u{E4A2}',
        Mark::Accent => '\u{E4A0}',
        Mark::Marcato => '\u{E4AC}',
        Mark::Staccatissimo => '\u{E4A6}',
        Mark::Fermata => '\u{E4C0}',
        Mark::FermataBelow => '\u{E4C1}',
        Mark::Tremolo1 => '\u{E220}',
        Mark::Tremolo2 => '\u{E221}',
        Mark::Tremolo3 => '\u{E222}',
        Mark::Finger(n) => char::from_u32(0xED10 + n.min(5) as u32)?,
        Mark::Segno => '\u{E047}',
        Mark::Coda => '\u{E048}',
        Mark::Breath => '\u{E4CE}',
        Mark::Caesura => '\u{E4D1}',
        Mark::PedalDown => '\u{E650}',
        Mark::PedalUp => '\u{E655}',
        Mark::Arpeggio => return None,
    })
}

// ---------------- the symbols ----------------

/// What a palette cell shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Sym {
    Rest(u8),
    Note(u8),
    Acc(i8),
    Clef(Clef),
    GLine,
    GText,
    GChord,
    GGrid,
    GBox,
    GBracket,
    GRound,
    GEllipse,
    TSlur,
    TCresc,
    TDecresc,
    TTrill,
    TArp,
    TTempo,
    TOttava(bool),
    TLyric,
    TMidi,
    Dyn(String),
    Mark(Mark),
    Label(String),
    Swatch(Rgb),
}

/// Records the shapes of one symbol and their ink box. Coordinates are in staff spaces from `o`, y down.
pub struct Pen<'a> {
    painter: &'a egui::Painter,
    col: Color32,
    sp: f32,
    o: Pos2,
    pub shapes: Vec<Shape>,
    pub ink: Rect,
}

impl<'a> Pen<'a> {
    pub fn new(painter: &'a egui::Painter, col: Color32, sp: f32, o: Pos2) -> Self {
        Pen { painter, col, sp, o, shapes: vec![], ink: Rect::NOTHING }
    }
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.o + vec2(x, y) * self.sp
    }
    fn st(&self, w: f32) -> Stroke {
        Stroke::new(w * self.sp, self.col)
    }
    fn push(&mut self, s: Shape) {
        self.ink |= s.visual_bounding_rect();
        self.shapes.push(s);
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), w: f32) {
        let s = Shape::line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], self.st(w));
        self.push(s);
    }
    fn poly(&mut self, pts: &[(f32, f32)], w: f32, closed: bool) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        let s = if closed { Shape::closed_line(v, self.st(w)) } else { Shape::line(v, self.st(w)) };
        self.push(s);
    }
    fn circle(&mut self, c: (f32, f32), r: f32) {
        let s = Shape::circle_filled(self.p(c.0, c.1), r * self.sp, self.col);
        self.push(s);
    }
    fn ring(&mut self, c: (f32, f32), r: f32, w: f32) {
        let s = Shape::circle_stroke(self.p(c.0, c.1), r * self.sp, self.st(w));
        self.push(s);
    }
    fn srect(&mut self, a: (f32, f32), b: (f32, f32), w: f32, radius: f32) {
        let r = Rect::from_two_pos(self.p(a.0, a.1), self.p(b.0, b.1));
        let s = Shape::rect_stroke(r, radius * self.sp, self.st(w), egui::StrokeKind::Middle);
        self.push(s);
    }
    fn ellipse_stroke(&mut self, c: (f32, f32), rx: f32, ry: f32, w: f32) {
        let s = Shape::ellipse_stroke(self.p(c.0, c.1), vec2(rx, ry) * self.sp, self.st(w));
        self.push(s);
    }
    fn bez_pts(&self, p: [(f32, f32); 4]) -> [Pos2; 4] {
        [self.p(p[0].0, p[0].1), self.p(p[1].0, p[1].1), self.p(p[2].0, p[2].1), self.p(p[3].0, p[3].1)]
    }
    fn bez(&mut self, p: [(f32, f32); 4], w: f32) {
        let s = egui::epaint::CubicBezierShape::from_points_stroke(self.bez_pts(p), false, Color32::TRANSPARENT, self.st(w));
        self.push(Shape::CubicBezier(s));
    }
    fn wave(&mut self, ax: f32, ay: f32, bx: f32) {
        let n = (((bx - ax) / 0.4).floor() as usize).max(2);
        let pts: Vec<(f32, f32)> = (0..=n).map(|i| (ax + (bx - ax) * i as f32 / n as f32, ay + if i % 2 == 1 { 0.2 } else { -0.2 })).collect();
        self.poly(&pts, 0.12, false);
    }
    fn dashed(&mut self, ax: f32, ay: f32, bx: f32, w: f32, dash: f32) {
        let mut t = 0.0;
        while t < bx - ax {
            self.line((ax + t, ay), (ax + (t + dash).min(bx - ax), ay), w);
            t += dash * 2.0;
        }
    }
    /// Interface text (Inter) centred on `at`; `size` in staff spaces. The ink box is the glyphs' mesh bounds.
    fn text(&mut self, at: (f32, f32), s: &str, size: f32, weight: u16, align: egui::Align2) {
        let font = match weight {
            0..=450 => theme::font(size * self.sp),
            451..=550 => theme::font_medium(size * self.sp),
            _ => theme::font_semibold(size * self.sp),
        };
        let g = self.painter.layout_no_wrap(s.to_string(), font, self.col);
        let r = align.anchor_size(self.p(at.0, at.1), g.size());
        self.ink |= g.mesh_bounds.translate(r.min.to_vec2());
        self.shapes.push(Shape::galley(r.min, g, self.col));
    }
    /// SMuFL glyphs from Bravura (1 em = 4 staff spaces) with the first glyph's origin (baseline, left) at `at`.
    /// The ink box comes from the font's metadata, placed at each glyph's laid-out position.
    fn glyph(&mut self, s: &str, at: (f32, f32)) {
        let font = egui::FontId::new(4.0 * self.sp, egui::FontFamily::Name(BRAVURA_FAMILY.into()));
        let g = self.painter.layout_no_wrap(s.to_string(), font, self.col);
        let Some(row) = g.rows.first() else { return };
        let Some(first) = row.row.glyphs.first() else { return };
        let origin = row.pos + first.pos.to_vec2();
        let top_left = self.p(at.0, at.1) - origin.to_vec2();
        for gl in &row.row.glyphs {
            if let Some((sw, ne)) = glyph_bbox(gl.chr) {
                let base = top_left + row.pos.to_vec2() + gl.pos.to_vec2();
                self.ink |= Rect::from_min_max(base + vec2(sw.x, -ne.y) * self.sp, base + vec2(ne.x, -sw.y) * self.sp);
            }
        }
        self.shapes.push(Shape::galley(top_left, g, self.col));
    }
    fn glyph_c(&mut self, c: char, at: (f32, f32)) {
        self.glyph(&c.to_string(), at);
    }
    /// A short five-line staff behind a clef, faint, so alto and tenor (and the octave clefs) read apart.
    fn staff(&mut self) {
        let full = self.col;
        self.col = theme::alpha(full, 0.38);
        for i in -2..=2 {
            self.line((-2.3, i as f32), (2.5, i as f32), 0.1);
        }
        self.col = full;
    }
    fn chord_symbol(&mut self) {
        self.text((-0.25, 0.15), "C", 2.2, 600, egui::Align2::CENTER_CENTER);
        self.text((0.75, -0.55), "7", 1.3, 600, egui::Align2::CENTER_CENTER);
    }
    fn arpeggio(&mut self) {
        let pts: Vec<(f32, f32)> = (0..8).map(|i| (if i % 2 == 1 { 0.2 } else { -0.2 }, (i as f32 - 3.5) * 0.4)).collect();
        self.poly(&pts, 0.12, false);
    }
}

/// Draws one symbol around the pen's origin.
pub fn draw_sym(pen: &mut Pen, sym: &Sym) {
    use egui::Align2;
    match sym {
        Sym::Rest(v) => pen.glyph_c(REST[(*v as usize).min(8)], (0.0, 0.0)),
        Sym::Note(v) => pen.glyph_c(NOTE[(*v as usize).min(8)], (0.0, 0.0)),
        Sym::Acc(a) => pen.glyph_c(acc_glyph(*a), (0.0, 0.0)),
        Sym::Clef(c) => {
            pen.staff();
            // the clef's origin sits on its reference line (y down: +1 = the line below the middle one)
            let (g, y) = match c {
                Clef::Treble => ('\u{E050}', 1.0),
                Clef::Bass => ('\u{E062}', -1.0),
                Clef::Alto => ('\u{E05C}', 0.0),
                Clef::Tenor => ('\u{E05C}', -1.0),
                Clef::Treble8va => ('\u{E053}', 1.0),
                Clef::Treble8vb => ('\u{E052}', 1.0),
                Clef::Bass8vb => ('\u{E064}', -1.0),
                Clef::Percussion => ('\u{E069}', 0.0),
            };
            pen.glyph_c(g, (-1.3, y));
        }
        Sym::GLine => pen.line((-1.4, 1.0), (1.4, -1.0), 0.16),
        Sym::GText => pen.text((0.0, 0.0), "T", 2.4, 500, Align2::CENTER_CENTER),
        Sym::GChord => pen.chord_symbol(),
        Sym::GGrid => {
            let (x0, x1, y0, y1) = (-1.1f32, 1.1f32, -1.2f32, 1.3f32);
            pen.line((x0, y0), (x1, y0), 0.35);
            for i in 0..4 {
                let x = x0 + (x1 - x0) * i as f32 / 3.0;
                pen.line((x, y0), (x, y1), 0.1);
            }
            for j in 1..4 {
                let y = y0 + (y1 - y0) * j as f32 / 3.3;
                pen.line((x0, y), (x1, y), 0.1);
            }
            pen.circle((x0 + (x1 - x0) / 3.0, y0 + (y1 - y0) * 0.5 / 3.3 + 0.35), 0.28);
        }
        Sym::GBox => pen.srect((-1.4, -1.0), (1.4, 1.0), 0.16, 0.0),
        Sym::GBracket => pen.poly(&[(-1.4, 0.6), (-1.4, -0.5), (1.4, -0.5), (1.4, 0.6)], 0.16, false),
        Sym::GRound => pen.srect((-1.4, -1.0), (1.4, 1.0), 0.16, 0.45),
        Sym::GEllipse => pen.ellipse_stroke((0.0, 0.0), 1.4, 1.0, 0.16),
        Sym::TSlur => pen.bez([(-2.0, 0.5), (-1.0, -0.6), (1.0, -0.6), (2.0, 0.5)], 0.16),
        Sym::TCresc => {
            pen.line((-1.8, 0.0), (1.8, -0.75), 0.12);
            pen.line((-1.8, 0.0), (1.8, 0.75), 0.12);
        }
        Sym::TDecresc => {
            pen.line((-1.8, -0.75), (1.8, 0.0), 0.12);
            pen.line((-1.8, 0.75), (1.8, 0.0), 0.12);
        }
        Sym::TTrill => {
            pen.glyph_c('\u{E566}', (-2.0, 0.4));
            pen.wave(-0.1, -0.1, 2.0);
        }
        Sym::TArp => pen.arpeggio(),
        Sym::TTempo => {
            pen.glyph_c('\u{E1D5}', (-1.2, 0.9));
            pen.text((0.9, 0.2), "=", 1.8, 500, Align2::CENTER_CENTER);
        }
        Sym::TOttava(down) => {
            pen.glyph_c(if *down { '\u{E51C}' } else { '\u{E511}' }, (-2.4, 0.4));
            pen.dashed(-0.1, -0.2, 1.6, 0.1, 0.25);
            pen.line((1.65, -0.25), (1.65, if *down { -1.0 } else { 0.6 }), 0.1);
        }
        Sym::TLyric => {
            pen.text((0.0, -0.1), "la", 2.0, 500, Align2::CENTER_CENTER);
            pen.line((-0.8, 0.9), (0.8, 0.9), 0.12);
        }
        Sym::TMidi => {
            pen.ring((0.0, 0.0), 1.3, 0.14);
            for a in [180.0f32, 225.0, 270.0, 315.0, 0.0] {
                let r = a.to_radians();
                pen.circle((r.cos() * 0.75, r.sin() * 0.75 + 0.1), 0.14);
            }
            pen.line((-0.25, 1.05), (0.25, 1.05), 0.18);
        }
        Sym::Dyn(d) => pen.glyph(&dyn_glyphs(d), (0.0, 0.0)),
        Sym::Mark(m) => match mark_glyph(*m) {
            Some(g) => pen.glyph_c(g, (0.0, 0.0)),
            None => pen.arpeggio(),
        },
        Sym::Label(t) => pen.text((0.0, 0.0), t, 1.0, 500, Align2::CENTER_CENTER),
        Sym::Swatch(_) => {}
    }
}

// ---------------- palette layout ----------------

/// One cell: what it picks, what it shows, its fit group, how many columns it spans and its tip.
pub struct Cell {
    pub item: Item,
    pub sym: Sym,
    pub group: &'static str,
    pub span: u8,
    pub tip: String,
}

/// A palette: columns, cell size in pt, and its cells in order (`None` = space between groups).
pub struct PalDef {
    pub cols: usize,
    pub w: f32,
    pub h: f32,
    pub cells: Vec<Option<Cell>>,
}

/// Design size per group (staff space in pt; for text groups the font size in pt). The fit rule only shrinks it.
pub fn nominal(group: &str) -> f32 {
    match group {
        "rests" | "notes" => 6.0,
        "accidentals" => 7.5,
        "clefs" => 4.6,
        "shapes" | "texttools" | "grid" | "lines" | "tempo" => 6.5,
        "ottava" | "midi" | "dynamics" => 7.0,
        "ornaments" | "tremolo" => 10.0,
        "articulations" => 11.0,
        "symbols" => 9.5,
        "fingering" | "pedal" => 9.0,
        "noteLabels" => 15.0,
        "guitar" => 14.0,
        "expressions" => 13.5,
        _ => 6.0,
    }
}

/// The inner margin of a cell: max(3 pt, 12% of its shorter side), in quarter points.
pub fn margin(w: f32, h: f32) -> f32 {
    ((0.12 * w.min(h) * 4.0).round() / 4.0).max(3.0)
}

pub const GAP: f32 = 2.0;
pub const GROUP_GAP: f32 = 8.0;

fn cell(item: Item, sym: Sym, group: &'static str, tip: impl Into<String>) -> Option<Cell> {
    Some(Cell { item, sym, group, span: 1, tip: tip.into() })
}

/// The 11 palettes, with the same items, order and relative positions as before.
pub fn palette_def(i: usize, tuplet: (u8, u8)) -> PalDef {
    let names = ["Double whole", "Whole", "Half", "Quarter", "Eighth", "16th", "32nd", "64th", "128th"];
    match i {
        0 => {
            let mut c = vec![];
            for v in 0..=8u8 {
                c.push(cell(Item::Rest(v), Sym::Rest(v), "rests", format!("{} rest ({})", names[v as usize], v)));
                c.push(cell(Item::Note(v), Sym::Note(v), "notes", format!("{} note ({})", names[v as usize], v)));
            }
            c.push(None);
            c.push(cell(Item::Accidental(2), Sym::Acc(2), "accidentals", "Double sharp (Shift+S)"));
            c.push(cell(Item::Accidental(1), Sym::Acc(1), "accidentals", "Sharp (S)"));
            c.push(cell(Item::Accidental(-2), Sym::Acc(-2), "accidentals", "Double flat (Shift+F)"));
            c.push(cell(Item::Accidental(-1), Sym::Acc(-1), "accidentals", "Flat (F)"));
            c.push(cell(Item::Courtesy, Sym::Label("( )".into()), "noteLabels", "( )"));
            c.push(cell(Item::Accidental(0), Sym::Acc(0), "accidentals", "Natural (N)"));
            c.push(None);
            c.push(cell(Item::Dot(2), Sym::Label("··".into()), "noteLabels", "··"));
            c.push(cell(Item::Dot(1), Sym::Label("·".into()), "noteLabels", "·"));
            let mut t = cell(Item::Tuplet, Sym::Label(format!("{}:{}", tuplet.0, tuplet.1)), "noteLabels", "Tuplet. Double-click tool to change value");
            if let Some(t) = t.as_mut() {
                t.span = 2;
            }
            c.push(t);
            PalDef { cols: 2, w: 36.0, h: 38.0, cells: c }
        }
        1 => PalDef { cols: 2, w: 40.0, h: 50.0, cells: Clef::ALL.iter().map(|&k| cell(Item::Clef(k), Sym::Clef(k), "clefs", format!("{} clef", k.name()))).collect() },
        2 => PalDef {
            cols: 2,
            w: 36.0,
            h: 32.0,
            cells: vec![
                cell(Item::Graphic(GraphicKind::Line), Sym::GLine, "shapes", "Line"),
                cell(Item::Graphic(GraphicKind::Text), Sym::GText, "texttools", "Text"),
                cell(Item::Chord, Sym::GChord, "texttools", "Chord symbol"),
                cell(Item::ChordGrid, Sym::GGrid, "grid", "Chord grid"),
                cell(Item::Graphic(GraphicKind::Rect), Sym::GBox, "shapes", "Box"),
                cell(Item::Graphic(GraphicKind::Bracket), Sym::GBracket, "shapes", "Bracket"),
                cell(Item::Graphic(GraphicKind::RoundRect), Sym::GRound, "shapes", "Rounded box"),
                cell(Item::Graphic(GraphicKind::Ellipse), Sym::GEllipse, "shapes", "Ellipse"),
            ],
        },
        3 => {
            let mut c = vec![
                cell(Item::Graphic(GraphicKind::Slur), Sym::TSlur, "lines", "Slur (drag from note to note)"),
                cell(Item::Graphic(GraphicKind::Crescendo), Sym::TCresc, "lines", "Crescendo hairpin (Shift flips)"),
                cell(Item::TempoTool, Sym::TTempo, "tempo", "Tempo"),
                cell(Item::Graphic(GraphicKind::Decrescendo), Sym::TDecresc, "lines", "Decrescendo hairpin"),
                cell(Item::Graphic(GraphicKind::TrillLine), Sym::TTrill, "lines", "Trill line"),
                cell(Item::Mark(Mark::Arpeggio), Sym::TArp, "lines", "Arpeggio"),
                cell(Item::Graphic(GraphicKind::Ottava), Sym::TOttava(false), "ottava", "8va"),
                cell(Item::Graphic(GraphicKind::OttavaBassa), Sym::TOttava(true), "ottava", "8vb"),
                cell(Item::Lyric, Sym::TLyric, "texttools", "Lyrics"),
                cell(Item::Chord, Sym::GChord, "texttools", "Chord symbol"),
            ];
            let mut m = cell(Item::MidiTool, Sym::TMidi, "midi", "MIDI attributes");
            if let Some(m) = m.as_mut() {
                m.span = 2;
            }
            c.push(m);
            PalDef { cols: 2, w: 36.0, h: 32.0, cells: c }
        }
        4 => PalDef {
            cols: 1,
            w: 64.0,
            h: 28.0,
            cells: DYNAMICS.iter().map(|d| cell(Item::Dynamic(d.to_string()), Sym::Dyn(d.to_string()), "dynamics", format!("Dynamic {d}"))).collect(),
        },
        5..=7 => {
            let marks: Vec<Mark> = match i {
                5 => vec![Mark::Trill, Mark::Turn, Mark::Mordent, Mark::InvMordent, Mark::Plus, Mark::Harmonic, Mark::UpBow, Mark::DownBow],
                6 => vec![Mark::Tenuto, Mark::Staccato, Mark::Accent, Mark::Marcato, Mark::Staccatissimo],
                _ => vec![
                    Mark::Fermata,
                    Mark::FermataBelow,
                    Mark::Tremolo1,
                    Mark::Tremolo2,
                    Mark::Tremolo3,
                    Mark::Finger(0),
                    Mark::Finger(1),
                    Mark::Finger(2),
                    Mark::Finger(3),
                    Mark::Finger(4),
                    Mark::Finger(5),
                    Mark::Segno,
                    Mark::Coda,
                    Mark::Breath,
                    Mark::Caesura,
                    Mark::PedalDown,
                    Mark::PedalUp,
                ],
            };
            let group = |m: Mark| match (i, m) {
                (5, _) => "ornaments",
                (6, _) => "articulations",
                (_, Mark::Tremolo1 | Mark::Tremolo2 | Mark::Tremolo3) => "tremolo",
                (_, Mark::Finger(_)) => "fingering",
                (_, Mark::PedalDown | Mark::PedalUp) => "pedal",
                _ => "symbols",
            };
            PalDef { cols: 2, w: 36.0, h: 32.0, cells: marks.into_iter().map(|m| cell(Item::Mark(m), Sym::Mark(m), group(m), m.label())).collect() }
        }
        8 => PalDef { cols: 2, w: 36.0, h: 28.0, cells: GUITAR.iter().map(|t| cell(Item::Guitar(t.to_string()), Sym::Label(t.to_string()), "guitar", *t)).collect() },
        9 => PalDef { cols: 1, w: 96.0, h: 26.0, cells: EXPRESSIONS.iter().map(|t| cell(Item::Expression(t.to_string()), Sym::Label(t.to_string()), "expressions", *t)).collect() },
        _ => PalDef { cols: 2, w: 30.0, h: 30.0, cells: COLORS.iter().map(|&c| cell(Item::Color(c), Sym::Swatch(c), "colors", "Color (applies to selection)")).collect() },
    }
}

pub const DYNAMICS: [&str; 23] = ["pppppp", "ppppp", "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fffff", "ffffff", "sf", "sfp", "sfpp", "fp", "rf", "rfz", "sfz", "sffz", "fz"];
pub const GUITAR: [&str; 20] = ["p", "i", "m", "a", "c", "P", "H", "T", "(1)", "(2)", "(3)", "(4)", "(5)", "(6)", "(7)", "(8)", "B", "1/2", "Rel.", "vib."];
pub const EXPRESSIONS: [&str; 23] = [
    "cresc.", "decresc.", "poco a poco", "rit.", "legato", "staccato", "dolce", "ritard.", "accel.", "dim.", "leggiero", "Tempo I", "Largo", "Adagio", "Andante", "Allegro", "Presto", "a tempo", "attacca",
    "espress.", "sotto voce", "perdendosi", "Tranquillo",
];
pub const COLORS: [Rgb; 16] = [
    Rgb(128, 0, 0), Rgb(255, 0, 0), Rgb(0, 0, 128), Rgb(0, 0, 255), Rgb(0, 128, 128), Rgb(0, 255, 255), Rgb(0, 128, 0), Rgb(0, 255, 0),
    Rgb(128, 128, 0), Rgb(100, 255, 200), Rgb(128, 0, 128), Rgb(255, 0, 255), Rgb(128, 128, 128), Rgb(200, 200, 200), Rgb(255, 255, 255), Rgb(0, 0, 0),
];

/// Cell rectangles relative to the grid's top-left, and the grid's size.
pub fn grid_layout(def: &PalDef) -> (Vec<Option<Rect>>, Vec2) {
    let mut out = vec![];
    let (mut col, mut y) = (0usize, 0.0f32);
    let mut width = 0.0f32;
    let row_h = def.h + GAP;
    for c in &def.cells {
        match c {
            None => {
                if col > 0 {
                    col = 0;
                    y += row_h;
                }
                y += GROUP_GAP - GAP;
                out.push(None);
            }
            Some(c) => {
                let span = (c.span as usize).max(1);
                if col + span > def.cols {
                    col = 0;
                    y += row_h;
                }
                let x = col as f32 * (def.w + GAP);
                let w = def.w * span as f32 + GAP * (span - 1) as f32;
                let r = Rect::from_min_size(pos2(x, y), vec2(w, def.h));
                width = width.max(r.right());
                out.push(Some(r));
                col += span;
                if col >= def.cols {
                    col = 0;
                    y += row_h;
                }
            }
        }
    }
    let height = if col > 0 { y + def.h } else { y - GAP };
    (out, vec2(width.max(def.cols as f32 * (def.w + GAP) - GAP), height))
}

/// Ink box of a symbol drawn at the reference staff space around (0,0) (cached per pixels-per-point).
fn ref_box(painter: &egui::Painter, sym: &Sym) -> Rect {
    const REF: f32 = 20.0;
    thread_local! {
        static CACHE: std::cell::RefCell<HashMap<String, Rect>> = Default::default();
    }
    let key = format!("{sym:?}@{}", painter.pixels_per_point());
    if let Some(r) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return r;
    }
    let mut pen = Pen::new(painter, Color32::BLACK, REF, Pos2::ZERO);
    draw_sym(&mut pen, sym);
    let r = Rect::from_min_max((pen.ink.min.to_vec2() / REF).to_pos2(), (pen.ink.max.to_vec2() / REF).to_pos2());
    CACHE.with(|c| c.borrow_mut().insert(key, r));
    r
}

/// The staff space of every group in a palette: its design size, shrunk so the group's largest ink box fits.
pub fn group_scales(painter: &egui::Painter, def: &PalDef) -> HashMap<&'static str, f32> {
    let m = margin(def.w, def.h);
    let (rects, _) = grid_layout(def);
    let mut out: HashMap<&'static str, f32> = HashMap::new();
    for (c, r) in def.cells.iter().zip(rects) {
        let (Some(c), Some(r)) = (c, r) else { continue };
        if matches!(c.sym, Sym::Swatch(_)) {
            continue;
        }
        let b = ref_box(painter, &c.sym);
        let (iw, ih) = (r.width() - 2.0 * m, r.height() - 2.0 * m);
        let fit = (iw / b.width()).min(ih / b.height());
        let e = out.entry(c.group).or_insert(nominal(c.group));
        *e = e.min(fit);
    }
    out
}

/// The shapes of a symbol fitted to a cell (centred by its ink box, origin on the physical pixel), and its box.
pub fn fitted(painter: &egui::Painter, sym: &Sym, cell: Rect, sp: f32, col: Color32) -> (Vec<Shape>, Rect) {
    let mut pen = Pen::new(painter, col, sp, Pos2::ZERO);
    draw_sym(&mut pen, sym);
    let ppp = painter.pixels_per_point();
    let d = cell.center() - pen.ink.center();
    let d = vec2((d.x * ppp).round() / ppp, (d.y * ppp).round() / ppp);
    let mut shapes = pen.shapes;
    for s in &mut shapes {
        s.translate(d);
    }
    (shapes, pen.ink.translate(d))
}

/// A palette is as wide as its cells or its name (after the dot), whichever is wider; the cells sit in the middle.
fn panel_width(ctx: &egui::Context, i: usize, grid_w: f32) -> f32 {
    let name_w = ctx.fonts_mut(|f| f.layout_no_wrap(PALETTES[i].to_string(), theme::font_semibold(11.0), Color32::BLACK).size().x);
    grid_w.max(name_w + 22.0)
}

/// One drawn cell, kept for the pixel check of `--palette-sheet`.
#[derive(Clone, Debug)]
pub struct CellLog {
    pub name: String,
    pub rect: Rect,
    pub inner: Rect,
    pub fill: Option<Color32>,
}

impl App {
    pub fn palettes_ui(&mut self, ctx: &egui::Context) {
        if self.floating_hidden {
            return;
        }
        let gen = self.palette_layout_gen;
        let k = theme::ctx_tokens(ctx);
        // under the toolbar: Notes, Clefs, Graphics, Tools and Colors on the left; the others along the right edge
        // (x < 0 counts from the right edge), as in the mockup
        let default_pos = [(12.0, 80.0), (106.0, 80.0), (106.0, 330.0), (106.0, 320.0), (-190.0, 80.0), (-282.0, 80.0), (-96.0, 80.0), (-96.0, 230.0), (-282.0, 260.0), (-398.0, 80.0), (12.0, 560.0)];
        let screen = ctx.content_rect();
        // the window can still be small in its first frames; place the palettes once it has its real size
        let settled = screen.width() >= 900.0 || ctx.cumulative_frame_nr() > 30;
        let shadow = egui::Shadow { offset: [0, 8], blur: 26, spread: 0, color: k.shadow };
        let frame = egui::Frame::new()
            .fill(k.pal_bg)
            .corner_radius(11.0)
            .shadow(shadow)
            .stroke(Stroke::new(0.5, if k.dark { theme::alpha(Color32::WHITE, 0.10) } else { theme::alpha(Color32::BLACK, 0.10) }))
            .inner_margin(egui::Margin { left: 5, right: 5, top: 0, bottom: 5 });
        for i in 0..11 {
            if !self.palettes[i] {
                continue;
            }
            let pos = if let Some(p) = self.palette_sheet_pos.as_ref().and_then(|v| v.get(i)) {
                *p
            } else if self.palette_cascade && gen > 0 {
                pos2(60.0 + i as f32 * 28.0, 120.0 + i as f32 * 24.0)
            } else {
                let (x, y) = default_pos[i];
                pos2(if x < 0.0 { screen.right() + x } else { x }, y)
            };
            #[allow(deprecated)]
            let id = egui::Id::new(("palette", i, gen));
            // a fixed size, so egui never guesses a default window size (it would push right-hand palettes inwards)
            let (_, grid) = grid_layout(&palette_def(i, self.tuplet_ratio));
            let pw = panel_width(ctx, i, grid.x);
            let ph = if self.pal_collapsed[i] { 24.0 } else { 24.0 + grid.y };
            let mut w = egui::Window::new(PALETTES[i]).id(id).title_bar(false).resizable(false).movable(true).default_pos(pos).fixed_size(vec2(pw, ph)).frame(frame);
            // egui's own keep-on-screen rule pushes right-hand palettes towards the middle on the first frames;
            // the positions above are already on screen, and a palette may be dragged partly off it, like a Mac window
            w = w.constrain(false);
            if gen > 0 || self.palette_sheet_pos.is_some() || !self.pal_placed[i] {
                w = w.current_pos(pos);
            }
            if settled {
                self.pal_placed[i] = true;
            }
            w.show(ctx, |ui| self.palette_panel(ui, i));
        }
    }

    /// Header (dot and name) and the cells of one palette.
    fn palette_panel(&mut self, ui: &mut Ui, i: usize) {
        let k = theme::t(ui);
        let def = palette_def(i, self.tuplet_ratio);
        let (_, grid_size) = grid_layout(&def);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let size = vec2(panel_width(ui.ctx(), i, grid_size.x), grid_size.y);
        // header: the dot closes the palette (it shows × under the mouse); a double-click on the name folds it
        let (hr, hresp) = ui.allocate_exact_size(vec2(size.x, 24.0), Sense::click());
        let dot_c = pos2(hr.left() + 7.5, hr.center().y);
        let dot = ui.interact(Rect::from_center_size(dot_c, vec2(15.0, 15.0)), ui.scope_id().with("close"), Sense::click());
        let p = ui.painter();
        if dot.hovered() {
            p.circle_filled(dot_c, 5.5, k.text_3);
            let d = 2.0;
            let s = Stroke::new(1.3, k.pal_bg_opaque());
            p.line_segment([dot_c + vec2(-d, -d), dot_c + vec2(d, d)], s);
            p.line_segment([dot_c + vec2(-d, d), dot_c + vec2(d, -d)], s);
        } else {
            p.circle_filled(dot_c, 4.5, k.pal_dot);
        }
        p.text(pos2(dot_c.x + 10.0, hr.center().y), egui::Align2::LEFT_CENTER, PALETTES[i], theme::font_semibold(11.0), k.text_2);
        let dot = crate::ui::tip(dot, "Close", &theme::shortcut_glyphs(crate::commands::Cmd::Palette(i)));
        if dot.clicked() {
            self.palettes[i] = false;
        }
        if hresp.double_clicked() {
            self.pal_collapsed[i] = !self.pal_collapsed[i];
        }
        if self.pal_collapsed[i] {
            return;
        }
        self.palette_cells(ui, i, &def, size.x);
    }

    /// The cells of one palette, centred in `width`. True when a cell was picked.
    fn palette_cells(&mut self, ui: &mut Ui, i: usize, def: &PalDef, width: f32) -> bool {
        let k = theme::t(ui);
        let (rects, grid_size) = grid_layout(def);
        let mut picked = false;
        let (area, _) = ui.allocate_exact_size(vec2(width.max(grid_size.x), grid_size.y), Sense::hover());
        let grid = Rect::from_min_size(pos2(area.center().x - grid_size.x / 2.0, area.top()), grid_size);
        let painter = ui.painter().clone();
        let scales = group_scales(&painter, def);
        let m = margin(def.w, def.h);
        let guides = std::env::var("FERMATA_PALETTE_GUIDES").is_ok_and(|v| v == "1");
        for (n, (c, r)) in def.cells.iter().zip(rects).enumerate() {
            let (Some(c), Some(r)) = (c, r) else { continue };
            let r = r.translate(grid.min.to_vec2());
            let resp = ui.interact(r, ui.scope_id().with(("cell", n)), Sense::click());
            let selected = match c.item {
                Item::Tuplet => self.cur_tuplet,
                Item::Dot(d) => self.cur_dots == d,
                _ => self.item == c.item,
            };
            let fill = if selected {
                Some(if resp.hovered() { k.accent_hover } else { k.accent })
            } else if resp.is_pointer_button_down_on() {
                Some(k.cell_press)
            } else if resp.hovered() {
                Some(k.cell_hover)
            } else {
                None
            };
            if let Some(f) = fill {
                painter.rect_filled(r, 7.0, f);
            }
            let ink = if selected { k.accent_fg } else { k.text };
            let inner = r.shrink(m);
            if let Sym::Swatch(rgb) = c.sym {
                painter.rect_filled(inner, 0.0, Color32::from_rgb(rgb.0, rgb.1, rgb.2));
                painter.rect_stroke(inner, 0.0, Stroke::new(1.0, theme::alpha(k.text, 0.45)), egui::StrokeKind::Inside);
            } else {
                let sp = scales.get(c.group).copied().unwrap_or(6.0);
                let (shapes, b) = fitted(&painter, &c.sym, r, sp, ink);
                painter.extend(shapes);
                debug_assert!(inner.expand(0.75).contains_rect(b), "{} {:?}: ink {b:?} outside {inner:?}", PALETTES[i], c.sym);
                if guides {
                    painter.rect_stroke(inner, 0.0, Stroke::new(0.5, Color32::from_rgb(230, 40, 140)), egui::StrokeKind::Middle);
                    painter.rect_stroke(b, 0.0, Stroke::new(0.5, Color32::from_rgb(0, 150, 255)), egui::StrokeKind::Middle);
                }
            }
            if let Some(log) = self.palette_cell_log.as_mut() {
                log.push(CellLog { name: format!("{}/{:?}", PALETTES[i], c.sym), rect: r, inner, fill: if matches!(c.sym, Sym::Swatch(_)) { None } else { fill.or(Some(Color32::TRANSPARENT)) } });
            }
            // the tip: the name, then the key in a key cap ("Sharp  S", "Double sharp  ⇧S")
            let (name, key) = crate::kit::split_tip(&c.tip);
            let resp = crate::kit::tip(resp, name, &key.replace("Shift+", "⇧"));
            if resp.clicked() {
                self.pick_item(c.item.clone());
                picked = true;
            }
            if c.item == Item::Tuplet && (resp.double_clicked() || resp.secondary_clicked()) {
                let (a, b) = self.tuplet_ratio;
                self.cur_tuplet = true;
                self.dialog = Some(Dialog::Tuplet { notes: a, in_time: b });
            }
        }
        picked
    }

    /// Tablet (F-25): the rail on the left with the 11 palettes, same order and names. A tap opens a palette
    /// (or closes the open one).
    pub fn palette_rail(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..11 {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
                let on = self.rail_sel == Some(i);
                let p = ui.painter();
                let tile = r.shrink2(vec2(6.0, 2.0));
                if on {
                    p.rect_filled(tile, 10.0, k.accent_soft);
                    p.rect_filled(Rect::from_min_size(pos2(r.left(), tile.top() + 10.0), vec2(3.0, tile.height() - 20.0)), 1.5, k.accent);
                } else if resp.is_pointer_button_down_on() {
                    p.rect_filled(tile, 10.0, k.cell_press);
                }
                let ink = if on { k.accent } else { k.text };
                // the palette's own symbol, the same drawing as in its cells
                let def = palette_def(i, self.tuplet_ratio);
                if let Some(Some(c)) = def.cells.get(RAIL_ICON[i]) {
                    let cell = Rect::from_center_size(pos2(r.center().x, r.top() + 24.0), vec2(34.0, 30.0));
                    if let Sym::Swatch(rgb) = c.sym {
                        p.circle_filled(cell.center(), 9.0, Color32::from_rgb(rgb.0, rgb.1, rgb.2));
                    } else {
                        let sp = (nominal(c.group) * 0.8).min(group_scales(p, &def).get(c.group).copied().unwrap_or(6.0));
                        let (shapes, _) = fitted(p, &c.sym, cell, sp, ink);
                        p.extend(shapes);
                    }
                }
                p.text(pos2(r.center().x, r.bottom() - 12.0), egui::Align2::CENTER_CENTER, RAIL_NAMES[i], theme::font_medium(11.0), if on { k.accent } else { k.text_2 });
                if resp.clicked() {
                    self.rail_sel = if on { None } else { Some(i) };
                }
            }
        });
    }

    /// Tablet: the open palette beside the rail, pinned (`pinned`: lying on its side, or "Pin" pressed) or over
    /// the page. True when a cell was picked (a palette over the page then closes).
    pub fn rail_palette(&mut self, ui: &mut Ui, i: usize, pinned: bool, landscape: bool) -> bool {
        let k = theme::t(ui);
        let def = touch_def(palette_def(i, self.tuplet_ratio));
        let (_, grid) = grid_layout(&def);
        ui.spacing_mut().item_spacing = vec2(0.0, 6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.set_min_height(36.0);
            ui.label(egui::RichText::new(PALETTES[i]).font(theme::font_semibold(14.0)).color(k.text_2));
            ui.label(egui::RichText::new(theme::shortcut_glyphs(crate::commands::Cmd::Palette(i))).font(theme::font(12.0)).color(k.text_3));
            if landscape {
                ui.label(egui::RichText::new("·  fixada").font(theme::font(12.0)).color(k.text_3));
            } else {
                let (txt, to) = if pinned { ("Soltar", false) } else { ("Fixar ⇥", true) };
                if ui.add(egui::Label::new(egui::RichText::new(txt).font(theme::font_semibold(14.0)).color(k.accent)).sense(Sense::click())).clicked() {
                    self.rail_pinned = to;
                }
            }
        });
        let mut picked = false;
        egui::ScrollArea::vertical().auto_shrink([true, false]).show(ui, |ui| {
            picked = self.palette_cells(ui, i, &def, grid.x);
            ui.add_space(12.0);
        });
        picked
    }
}

/// The rail's short names (the menus and shortcuts keep the full ones).
const RAIL_NAMES: [&str; 11] = ["Notes", "Clefs", "Graphics", "Tools", "Dynam.", "Marks 1", "Marks 2", "Symbols", "Guitar", "Expr.", "Colors"];
/// The cell each palette shows as its icon in the rail: quarter note, treble clef, text, crescendo, f, trill,
/// accent, fermata, i, dolce, red.
const RAIL_ICON: [usize; 11] = [7, 0, 1, 1, 8, 0, 2, 0, 1, 6, 1];

/// A palette for fingers: cells of at least 44 × 44 pt, symbols fitted by the same rule.
pub fn touch_def(mut d: PalDef) -> PalDef {
    d.w = d.w.max(44.0);
    d.h = d.h.max(44.0);
    d
}

/// Width of the pinned palette beside the rail.
pub fn rail_palette_width(i: usize, tuplet: (u8, u8)) -> f32 {
    let (_, grid) = grid_layout(&touch_def(palette_def(i, tuplet)));
    grid.x.max(150.0) + 24.0
}

/// Checks a screenshot of the palettes: in every cell, pixels between the cell's edge and its inner area (with 1
/// physical pixel of tolerance) must be the cell's background. Returns one line per cell with ink outside.
pub fn verify_pixels(img: &egui::ColorImage, ppp: f32, cells: &[CellLog], behind: Color32) -> (usize, Vec<String>) {
    let [w, h] = img.size;
    let mut bad = vec![];
    let mut checked = 0;
    for c in cells {
        let Some(fill) = c.fill else { continue };
        checked += 1;
        let r = Rect::from_min_max((c.rect.min.to_vec2() * ppp).to_pos2(), (c.rect.max.to_vec2() * ppp).to_pos2());
        let inner = Rect::from_min_max((c.inner.min.to_vec2() * ppp).to_pos2(), (c.inner.max.to_vec2() * ppp).to_pos2()).expand(1.0);
        let filled = theme::blend(behind, fill);
        let radius = 7.0 * ppp;
        let mut out = 0;
        for y in r.top().ceil() as usize..(r.bottom().floor() as usize).min(h) {
            for x in r.left().ceil() as usize..(r.right().floor() as usize).min(w) {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                if inner.contains(pos2(fx, fy)) {
                    continue;
                }
                // in the rounded corners of a filled cell the panel shows through
                let cx = fx.clamp(r.left() + radius, r.right() - radius);
                let cy = fy.clamp(r.top() + radius, r.bottom() - radius);
                let dist = (fx - cx).hypot(fy - cy);
                let in_corner = dist > radius - 1.5;
                let on_edge = (dist - radius).abs() < 1.5;
                let px = img.pixels[y * w + x];
                let near = |a: Color32| (px.r() as i32 - a.r() as i32).abs().max((px.g() as i32 - a.g() as i32).abs()).max((px.b() as i32 - a.b() as i32).abs()) <= 8;
                // on the antialiased edge of a rounded fill the pixel is a mix of the fill and the panel
                let mix = || {
                    let ch = |p: u8, a: u8, b: u8| (p as f32 - a as f32, b as f32 - a as f32);
                    let parts = [ch(px.r(), behind.r(), filled.r()), ch(px.g(), behind.g(), filled.g()), ch(px.b(), behind.b(), filled.b())];
                    let (dp, dd) = parts.iter().copied().fold((0.0f32, 0.0f32), |m, v| if v.1.abs() > m.1.abs() { v } else { m });
                    let t = if dd.abs() < 1.0 { 0.0 } else { (dp / dd).clamp(0.0, 1.0) };
                    parts.iter().all(|&(p, d)| (p - d * t).abs() <= 8.0)
                };
                let ok = near(filled) || (in_corner && near(behind)) || (on_edge && mix());
                if !ok {
                    out += 1;
                }
            }
        }
        if out > 0 {
            bad.push(format!("{} ({out} px)", c.name));
        }
    }
    (checked, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `f` inside an egui frame with the app's fonts installed.
    fn with_painter(dark: bool, f: impl FnOnce(&egui::Painter)) {
        let ctx = egui::Context::default();
        theme::install(&ctx, Some(if dark { egui::Theme::Dark } else { egui::Theme::Light }));
        let mut f = Some(f);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
            if let Some(f) = f.take() {
                f(&p);
            }
        });
    }

    #[test]
    fn every_glyph_has_a_metadata_box() {
        for (c, name) in GLYPHS {
            assert!(glyph_bbox(*c).is_some(), "{name} missing from Bravura.json");
        }
    }

    #[test]
    fn same_items_and_order_as_before() {
        let count = |i: usize| palette_def(i, (3, 2)).cells.iter().flatten().count();
        assert_eq!((0..11).map(count).collect::<Vec<_>>(), vec![18 + 6 + 3, 8, 8, 11, 23, 8, 5, 17, 20, 23, 16]);
        let clefs: Vec<Item> = palette_def(1, (3, 2)).cells.into_iter().flatten().map(|c| c.item).collect();
        assert_eq!(clefs, Clef::ALL.iter().map(|&c| Item::Clef(c)).collect::<Vec<_>>());
    }

    #[test]
    fn spec_cell_sizes() {
        let s = |i: usize| {
            let d = palette_def(i, (3, 2));
            (d.w, d.h)
        };
        assert_eq!(s(0), (36.0, 38.0));
        assert_eq!(s(1), (40.0, 50.0));
        for i in [2, 3, 5, 6, 7] {
            assert_eq!(s(i), (36.0, 32.0));
        }
        assert_eq!(s(4), (64.0, 28.0));
        assert_eq!(s(8), (36.0, 28.0));
        assert_eq!(s(9), (96.0, 26.0));
        assert_eq!(s(10), (30.0, 30.0));
    }

    /// Spec §6.3: every symbol's ink box lies inside its cell's inner area (0.5 pt tolerance), and every member
    /// of a group in a palette uses the same staff space.
    #[test]
    fn every_symbol_fits_its_cell() {
        for dark in [false, true] {
            with_painter(dark, |p| {
                for (i, name) in PALETTES.iter().enumerate() {
                    let def = palette_def(i, (3, 2));
                    let (rects, _) = grid_layout(&def);
                    let scales = group_scales(p, &def);
                    let m = margin(def.w, def.h);
                    for (c, r) in def.cells.iter().zip(rects) {
                        let (Some(c), Some(r)) = (c, r) else { continue };
                        if matches!(c.sym, Sym::Swatch(_)) {
                            continue;
                        }
                        let sp = scales[c.group];
                        assert!(sp <= nominal(c.group) + 1e-4);
                        let (_, b) = fitted(p, &c.sym, r, sp, Color32::BLACK);
                        assert!(r.shrink(m).expand(0.5).contains_rect(b), "{name} {:?}: {b:?} not in {:?}", c.sym, r.shrink(m));
                    }
                }
            });
        }
    }

    /// F-25: on a tablet every cell is at least 44 × 44 pt and every symbol still fits its cell.
    #[test]
    fn touch_cells_are_44_pt_and_symbols_fit() {
        with_painter(false, |p| {
            for (i, name) in PALETTES.iter().enumerate() {
                let def = touch_def(palette_def(i, (3, 2)));
                let (rects, _) = grid_layout(&def);
                let scales = group_scales(p, &def);
                let m = margin(def.w, def.h);
                for (c, r) in def.cells.iter().zip(rects) {
                    let (Some(c), Some(r)) = (c, r) else { continue };
                    assert!(r.width() >= 44.0 && r.height() >= 44.0, "{name}: {r:?}");
                    if matches!(c.sym, Sym::Swatch(_)) {
                        continue;
                    }
                    let (_, b) = fitted(p, &c.sym, r, scales[c.group], Color32::BLACK);
                    assert!(r.shrink(m).expand(0.5).contains_rect(b), "{name} {:?}", c.sym);
                }
            }
        });
    }

    #[test]
    fn group_gaps_and_cell_gaps() {
        let (rects, size) = grid_layout(&palette_def(0, (3, 2)));
        let r: Vec<Rect> = rects.into_iter().flatten().collect();
        assert_eq!(r[1].left() - r[0].right(), GAP);
        assert_eq!(r[2].top() - r[0].bottom(), GAP);
        // the accidentals start 8 pt below the last note row
        assert_eq!(r[18].top() - r[17].bottom(), GROUP_GAP);
        assert_eq!(size.x, 36.0 * 2.0 + GAP);
        // the tuplet spans both columns
        assert_eq!(r.last().unwrap().width(), 36.0 * 2.0 + GAP);
    }
}
