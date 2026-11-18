//! Music symbols on the score page from the Bravura font (SMuFL), placed by the font's own metadata (F-03): glyph
//! boxes, advance widths and anchors (where a stem meets a notehead or a flag). One staff space is a quarter of the
//! font size, as SMuFL defines it. A glyph's origin is SMuFL's: the left edge, on the baseline (a notehead's
//! baseline is the line or space it sits on). Lines, beams, ties, hairpins and braces stay drawn in `glyphs.rs`.
use crate::palettes::{BRAVURA_FAMILY, BRAVURA_META};
use egui::{pos2, vec2, Color32, FontFamily, FontId, Painter, Pos2, Vec2};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Every glyph the page draws: SMuFL name and code point. The browser's font subset carries these
/// (tools/subset_web_fonts.sh reads this table).
pub const PAGE_GLYPHS: &[(&str, char)] = &[
    ("noteheadDoubleWhole", '\u{E0A0}'),
    ("noteheadWhole", '\u{E0A2}'),
    ("noteheadHalf", '\u{E0A3}'),
    ("noteheadBlack", '\u{E0A4}'),
    ("noteheadXBlack", '\u{E0A9}'),
    ("flag8thUp", '\u{E240}'),
    ("flag8thDown", '\u{E241}'),
    ("flag16thUp", '\u{E242}'),
    ("flag16thDown", '\u{E243}'),
    ("flag32ndUp", '\u{E244}'),
    ("flag32ndDown", '\u{E245}'),
    ("flag64thUp", '\u{E246}'),
    ("flag64thDown", '\u{E247}'),
    ("flag128thUp", '\u{E248}'),
    ("flag128thDown", '\u{E249}'),
    ("restDoubleWhole", '\u{E4E2}'),
    ("restWhole", '\u{E4E3}'),
    ("restHalf", '\u{E4E4}'),
    ("restQuarter", '\u{E4E5}'),
    ("rest8th", '\u{E4E6}'),
    ("rest16th", '\u{E4E7}'),
    ("rest32nd", '\u{E4E8}'),
    ("rest64th", '\u{E4E9}'),
    ("rest128th", '\u{E4EA}'),
    ("accidentalFlat", '\u{E260}'),
    ("accidentalNatural", '\u{E261}'),
    ("accidentalSharp", '\u{E262}'),
    ("accidentalDoubleSharp", '\u{E263}'),
    ("accidentalDoubleFlat", '\u{E264}'),
    ("gClef", '\u{E050}'),
    ("gClef8vb", '\u{E052}'),
    ("gClef8va", '\u{E053}'),
    ("cClef", '\u{E05C}'),
    ("fClef", '\u{E062}'),
    ("fClef8vb", '\u{E064}'),
    ("unpitchedPercussionClef1", '\u{E069}'),
    ("timeSig0", '\u{E080}'),
    ("timeSig1", '\u{E081}'),
    ("timeSig2", '\u{E082}'),
    ("timeSig3", '\u{E083}'),
    ("timeSig4", '\u{E084}'),
    ("timeSig5", '\u{E085}'),
    ("timeSig6", '\u{E086}'),
    ("timeSig7", '\u{E087}'),
    ("timeSig8", '\u{E088}'),
    ("timeSig9", '\u{E089}'),
    ("dynamicPiano", '\u{E520}'),
    ("dynamicMezzo", '\u{E521}'),
    ("dynamicForte", '\u{E522}'),
    ("dynamicRinforzando", '\u{E523}'),
    ("dynamicSforzando", '\u{E524}'),
    ("dynamicZ", '\u{E525}'),
    ("dynamicNiente", '\u{E526}'),
    ("articAccentAbove", '\u{E4A0}'),
    ("articAccentBelow", '\u{E4A1}'),
    ("articStaccatoAbove", '\u{E4A2}'),
    ("articStaccatoBelow", '\u{E4A3}'),
    ("articTenutoAbove", '\u{E4A4}'),
    ("articTenutoBelow", '\u{E4A5}'),
    ("articStaccatissimoAbove", '\u{E4A6}'),
    ("articStaccatissimoBelow", '\u{E4A7}'),
    ("articMarcatoAbove", '\u{E4AC}'),
    ("articMarcatoBelow", '\u{E4AD}'),
    ("fermataAbove", '\u{E4C0}'),
    ("fermataBelow", '\u{E4C1}'),
    ("breathMarkComma", '\u{E4CE}'),
    ("caesura", '\u{E4D1}'),
    ("segno", '\u{E047}'),
    ("coda", '\u{E048}'),
    ("ornamentTrill", '\u{E566}'),
    ("ornamentTurn", '\u{E567}'),
    ("ornamentShortTrill", '\u{E56C}'),
    ("ornamentMordent", '\u{E56D}'),
    ("stringsDownBow", '\u{E610}'),
    ("stringsUpBow", '\u{E612}'),
    ("stringsHarmonic", '\u{E614}'),
    ("brassMuteClosed", '\u{E5E5}'),
    ("tremolo1", '\u{E220}'),
    ("tremolo2", '\u{E221}'),
    ("tremolo3", '\u{E222}'),
    ("fingering0", '\u{ED10}'),
    ("fingering1", '\u{ED11}'),
    ("fingering2", '\u{ED12}'),
    ("fingering3", '\u{ED13}'),
    ("fingering4", '\u{ED14}'),
    ("fingering5", '\u{ED15}'),
    ("keyboardPedalPed", '\u{E650}'),
    ("keyboardPedalUp", '\u{E655}'),
];

/// The font's metadata for the page glyphs, in staff spaces (SMuFL's y points up).
struct Meta {
    chars: HashMap<&'static str, char>,
    /// South-west and north-east corners of each glyph's ink.
    boxes: HashMap<String, (Vec2, Vec2)>,
    advance: HashMap<String, f32>,
    anchors: HashMap<String, HashMap<String, Vec2>>,
}

fn meta() -> &'static Meta {
    static M: OnceLock<Meta> = OnceLock::new();
    M.get_or_init(|| {
        let j: serde_json::Value = serde_json::from_str(BRAVURA_META).unwrap_or_default();
        let pt = |v: &serde_json::Value| -> Option<Vec2> { Some(vec2(v.get(0)?.as_f64()? as f32, v.get(1)?.as_f64()? as f32)) };
        let mut boxes = HashMap::new();
        let mut advance = HashMap::new();
        let mut anchors = HashMap::new();
        for (name, _) in PAGE_GLYPHS {
            let b = &j["glyphBBoxes"][*name];
            if let (Some(sw), Some(ne)) = (pt(&b["bBoxSW"]), pt(&b["bBoxNE"])) {
                boxes.insert(name.to_string(), (sw, ne));
            }
            if let Some(a) = j["glyphAdvanceWidths"][*name].as_f64() {
                advance.insert(name.to_string(), a as f32);
            }
            if let Some(obj) = j["glyphsWithAnchors"][*name].as_object() {
                let m: HashMap<String, Vec2> = obj.iter().filter_map(|(k, v)| Some((k.clone(), pt(v)?))).collect();
                anchors.insert(name.to_string(), m);
            }
        }
        Meta { chars: PAGE_GLYPHS.iter().copied().collect(), boxes, advance, anchors }
    })
}

/// The glyph's ink box in staff spaces (SMuFL coordinates: y up); a unit box when the metadata lacks it.
pub fn bbox(name: &str) -> (Vec2, Vec2) {
    meta().boxes.get(name).copied().unwrap_or((vec2(0.0, -0.5), vec2(1.0, 0.5)))
}

/// Advance width in staff spaces (the ink width when the metadata has no advance).
pub fn advance(name: &str) -> f32 {
    meta().advance.get(name).copied().unwrap_or_else(|| {
        let (sw, ne) = bbox(name);
        ne.x - sw.x
    })
}

/// An anchor of a glyph (for example `stemUpSE` of a notehead), in staff spaces, y up.
pub fn anchor(name: &str, key: &str) -> Option<Vec2> {
    meta().anchors.get(name).and_then(|m| m.get(key)).copied()
}

/// Draws a glyph with its SMuFL origin at `origin` (staff space `sp`).
pub fn draw(p: &Painter, name: &str, origin: Pos2, sp: f32, col: Color32) {
    let Some(&c) = meta().chars.get(name) else { return };
    let font = FontId::new(4.0 * sp, FontFamily::Name(BRAVURA_FAMILY.into()));
    let g = p.layout_no_wrap(c.to_string(), font, col);
    let Some(row) = g.rows.first() else { return };
    let Some(first) = row.row.glyphs.first() else { return };
    // the galley's own coordinates of the glyph's baseline origin
    let o = row.pos + first.pos.to_vec2();
    p.galley(origin - o.to_vec2(), g, col);
}

/// Draws a glyph with the centre of its ink box at `c`.
pub fn draw_centered(p: &Painter, name: &str, c: Pos2, sp: f32, col: Color32) {
    let (sw, ne) = bbox(name);
    let mid = (sw + ne) / 2.0;
    draw(p, name, pos2(c.x - mid.x * sp, c.y + mid.y * sp), sp, col);
}

/// Draws a glyph centred horizontally on `x`, with its origin's baseline at `y`.
pub fn draw_hcentered(p: &Painter, name: &str, x: f32, y: f32, sp: f32, col: Color32) {
    let (sw, ne) = bbox(name);
    draw(p, name, pos2(x - (sw.x + ne.x) / 2.0 * sp, y), sp, col);
}

/// The SMuFL name of a dynamic letter, if it is one.
pub fn dynamic_glyph(ch: char) -> Option<&'static str> {
    Some(match ch {
        'p' => "dynamicPiano",
        'm' => "dynamicMezzo",
        'f' => "dynamicForte",
        'r' => "dynamicRinforzando",
        's' => "dynamicSforzando",
        'z' => "dynamicZ",
        'n' => "dynamicNiente",
        _ => return None,
    })
}

/// A run of glyphs side by side (time signature digits, dynamics), centred on `x`, baseline `y`. Returns the run's
/// width.
pub fn draw_run(p: &Painter, names: &[&str], x: f32, y: f32, sp: f32, col: Color32) -> f32 {
    let w: f32 = names.iter().map(|n| advance(n)).sum::<f32>() * sp;
    let mut ox = x - w / 2.0;
    for n in names {
        draw(p, n, pos2(ox, y), sp, col);
        ox += advance(n) * sp;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_glyph_has_metadata() {
        for (name, _) in PAGE_GLYPHS {
            assert!(meta().boxes.contains_key(*name), "{name} has no box in the metadata");
        }
    }

    #[test]
    fn notehead_and_flag_anchors() {
        // the stem of an up-stem black note joins on the right, of a down-stem note on the left, as SMuFL defines
        let up = anchor("noteheadBlack", "stemUpSE").unwrap();
        let down = anchor("noteheadBlack", "stemDownNW").unwrap();
        assert!(up.x > 1.0 && up.y > 0.0);
        assert!(down.x.abs() < 1e-3 && down.y < 0.0);
        assert!(anchor("flag8thUp", "stemUpNW").is_some());
        assert!(anchor("flag8thDown", "stemDownSW").is_some());
    }

    #[test]
    fn whole_rest_hangs_and_half_rest_sits() {
        // the whole rest hangs below its origin (from the 4th line); the half rest stands on it (the 3rd line)
        let (sw, ne) = bbox("restWhole");
        assert!(ne.y < 0.1 && sw.y < -0.4);
        let (sw, ne) = bbox("restHalf");
        assert!(sw.y > -0.1 && ne.y > 0.4);
    }
}
