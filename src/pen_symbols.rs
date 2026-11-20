//! F-41: the rest of the score with the pen, in the Transcribe mode: clefs, key and time signatures, ties and slurs,
//! articulations (staccato, accent, tenuto, fermata), dynamics and hairpins. The classifier reads them
//! (`pen_model`); here they are drawn in the preview and attached to the notes they belong to when Accept writes.
use crate::model::{Clef, Mark};
use crate::pen_model as pm;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect};

/// Virtual classes made by the transcription (not read by the model): a key signature of `n` sharps (or −n flats),
/// and a staccato (a dot above or below a head).
pub const KEY_SIG: usize = 100;
pub const STACCATO: usize = 101;

pub fn clef_of(class: usize) -> Option<Clef> {
    Some(match class {
        20 => Clef::Treble,
        21 => Clef::Bass,
        22 => Clef::Alto,
        _ => return None,
    })
}

pub fn mark_of(class: usize, above: bool) -> Option<Mark> {
    Some(match class {
        pm::ACCENT => Mark::Accent,
        pm::TENUTO => Mark::Tenuto,
        pm::FERMATA => if above { Mark::Fermata } else { Mark::FermataBelow },
        STACCATO => Mark::Staccato,
        _ => return None,
    })
}

/// A short name for the status line and the alternatives.
pub fn name(class: usize, n: i8) -> String {
    match class {
        20 => "treble clef".into(),
        21 => "bass clef".into(),
        22 => "alto clef".into(),
        pm::TIME_FIRST..=pm::TIME_LAST => match class {
            31 => "common time".into(),
            32 => "cut time".into(),
            _ => {
                let (a, b) = pm::time_of(class).unwrap_or((4, 4));
                format!("{a}/{b} time")
            }
        },
        pm::BARLINE => "barline".into(),
        pm::DYN_FIRST..=pm::DYN_LAST => pm::dynamic_of(class).unwrap_or("p").into(),
        pm::ACCENT => "accent".into(),
        pm::TENUTO => "tenuto".into(),
        pm::FERMATA => "fermata".into(),
        pm::CRESC => "crescendo".into(),
        pm::DIM => "decrescendo".into(),
        pm::ARC => "tie or slur".into(),
        STACCATO => "staccato".into(),
        KEY_SIG => {
            if n == 0 {
                "key signature (no accidentals)".into()
            } else {
                format!("key signature, {} {}", n.unsigned_abs(), if n > 0 { "sharps" } else { "flats" })
            }
        }
        _ => "symbol".into(),
    }
}

fn bravura(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into()))
}

/// A symbol in a 44-point cell (the alternatives).
pub fn figure(p: &Painter, cell: Rect, class: usize, n: i8, col: Color32) {
    let c = cell.center();
    let glyph = |ch: char, size: f32, dy: f32| {
        p.text(c + vec2(0.0, dy), Align2::CENTER_CENTER, ch, bravura(size), col);
    };
    let text = |t: &str, size: f32| {
        p.text(c, Align2::CENTER_CENTER, t, FontId::proportional(size), col);
    };
    match class {
        20 => {
            glyph('\u{E050}', 22.0, 4.0);
        }
        21 => {
            glyph('\u{E062}', 24.0, -2.0);
        }
        22 => {
            glyph('\u{E05C}', 24.0, 0.0);
        }
        31 => {
            glyph('\u{E08A}', 24.0, 0.0);
        }
        32 => {
            glyph('\u{E08B}', 24.0, 0.0);
        }
        pm::TIME_FIRST..=pm::TIME_LAST => {
            let (a, b) = pm::time_of(class).unwrap_or((4, 4));
            text(&format!("{a}\n{b}"), 13.0);
        }
        pm::DYN_FIRST..=pm::DYN_LAST => {
            p.text(c, Align2::CENTER_CENTER, pm::dynamic_of(class).unwrap_or("p"), FontId::new(17.0, FontFamily::Proportional), col);
        }
        pm::ACCENT => {
            glyph('\u{E4A0}', 26.0, 0.0);
        }
        pm::TENUTO => {
            glyph('\u{E4A4}', 26.0, 0.0);
        }
        pm::FERMATA => {
            glyph('\u{E4C0}', 26.0, 4.0);
        }
        STACCATO => {
            glyph('\u{E4A2}', 26.0, 0.0);
        }
        pm::CRESC => {
            text("<", 22.0);
        }
        pm::DIM => {
            text(">", 22.0);
        }
        pm::ARC => {
            text("⌒", 22.0);
        }
        pm::BARLINE => {
            text("|", 22.0);
        }
        KEY_SIG => {
            text(&format!("{}{}", n.unsigned_abs(), if n >= 0 { "♯" } else { "♭" }), 16.0);
        }
        _ => text("?", 18.0),
    }
}

/// A symbol drawn where it was written, in engraving size (the preview): `a`–`b` its ends (hairpins, arcs), `up`
/// an arc bowing upward.
#[allow(clippy::too_many_arguments)]
pub fn draw(p: &Painter, class: usize, rect: Rect, a: Pos2, b: Pos2, up: bool, n: i8, staff_mid: Option<f32>, sp: f32, col: Color32) {
    use crate::glyphs as g;
    let c = rect.center();
    match class {
        20..=22 => {
            let (name, line) = match class {
                20 => ("gClef", 1.0),
                21 => ("fClef", -1.0),
                _ => ("cClef", 0.0),
            };
            let y = staff_mid.map(|m| m - line * sp).unwrap_or(c.y);
            g::clef(p, name, c.x, y, sp, col);
        }
        31 | 32 => {
            let y = staff_mid.unwrap_or(c.y);
            p.text(pos2(c.x, y), Align2::CENTER_CENTER, if class == 31 { '\u{E08A}' } else { '\u{E08B}' }, bravura(sp * 4.0), col);
        }
        pm::TIME_FIRST..=pm::TIME_LAST => {
            let (num, den) = pm::time_of(class).unwrap_or((4, 4));
            let y = staff_mid.unwrap_or(c.y);
            g::time_sig_digit(p, pos2(c.x, y - sp), sp, &num.to_string(), col);
            g::time_sig_digit(p, pos2(c.x, y + sp), sp, &den.to_string(), col);
        }
        pm::DYN_FIRST..=pm::DYN_LAST => g::dynamic(p, c, sp, pm::dynamic_of(class).unwrap_or("p"), col),
        pm::ACCENT | pm::TENUTO | pm::FERMATA | STACCATO => {
            let above = staff_mid.is_none_or(|m| c.y < m);
            if let Some(m) = mark_of(class, above) {
                g::mark(p, c, sp, m, above, col);
            }
        }
        pm::CRESC | pm::DIM => g::hairpin(p, pos2(rect.left(), c.y), pos2(rect.right(), c.y), sp, class == pm::CRESC, col),
        pm::ARC => g::tie_or_slur(p, a, b, sp, up, col),
        KEY_SIG => {
            let y = staff_mid.unwrap_or(c.y);
            // treble positions of the sharps and flats, from the middle line (half spaces up)
            const SHARPS: [i32; 7] = [4, 1, 5, 2, -1, 3, 0];
            const FLATS: [i32; 7] = [0, 3, -1, 2, -2, 1, -3];
            for k in 0..n.unsigned_abs().min(7) as usize {
                let pos = if n > 0 { SHARPS[k] } else { FLATS[k] };
                g::accidental(p, pos2(rect.left() + sp * (0.6 + k as f32), y - pos as f32 * sp * 0.5), sp, if n > 0 { 1 } else { -1 }, col);
            }
        }
        _ => {}
    }
}
