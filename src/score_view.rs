//! Page layout, drawing and hit-testing of the score.
use crate::collide;
use crate::glyphs as g;
use crate::model::*;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke};
use std::collections::{BTreeSet, HashMap, HashSet};

pub const SEL: Color32 = Color32::from_rgb(30, 90, 230);
/// F-37: what a click of the Eraser would remove, while the pointer is on it.
pub const ERASE: Color32 = Color32::from_rgb(220, 40, 40);

/// F-37: the Eraser's target under the pointer: a whole note or rest (the hit notehead, if any, is what the click
/// passes on), one note of a chord, or a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EraseHover {
    Event(NoteRef, Option<usize>),
    Pitch(NoteRef, usize),
    Graphic(usize),
}
pub const STAFF_LINE: f32 = 0.11;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteRef {
    pub staff: usize,
    pub measure: usize,
    pub idx: usize,
}

#[derive(Clone, Debug)]
pub struct NoteHit {
    pub r: NoteRef,
    pub pitch: Option<usize>,
    pub rect: Rect,
}

#[derive(Clone, Debug)]
pub struct MeasGeo {
    pub m: usize,
    pub x0: f32,
    pub content_x: f32,
    pub x1: f32,
    pub slots: Vec<(u32, f32)>,
    pub header_clef: bool,
    pub header_key: bool,
    pub header_time: bool,
    /// How many measures this one stands for: 1, or the length of a compressed multi-measure rest.
    pub span: usize,
}

impl MeasGeo {
    /// Is measure `m` drawn here (itself, or inside the multi-measure rest it stands for)?
    pub fn covers(&self, m: usize) -> bool {
        m >= self.m && m < self.m + self.span
    }
}

#[derive(Clone, Debug)]
pub struct SysGeo {
    pub page: usize,
    pub m0: usize,
    pub m1: usize,
    /// (staff index, middle line y)
    pub staves: Vec<(usize, f32)>,
    pub left: f32,
    pub right: f32,
    pub measures: Vec<MeasGeo>,
    pub first: bool,
    /// How far each staff's content reaches (same order as `staves`), F-05.
    pub ext: Vec<Ext>,
    /// Heights of the system's marks above the top staff.
    pub marks: SysMarks,
}

/// How far a staff's content reaches beyond its outer lines in one system, in staff spaces (F-05).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ext {
    /// Notes, stems, flags, accidentals and marks above the top line and below the bottom line.
    pub notes_above: f32,
    pub notes_below: f32,
    /// Everything: also chord symbols, expressions, dynamics, lyrics and the system's marks.
    pub above: f32,
    pub below: f32,
    /// Rows above the top line (chord symbols, expressions) and below the bottom line (dynamics, first lyric line),
    /// in staff spaces, the same in the whole system.
    pub chord_y: f32,
    pub expr_y: f32,
    pub dyn_y: f32,
    pub lyric_y: f32,
}

/// Rows of the marks above a system's top staff, in staff spaces above its top line: volta brackets, tempo mark (the
/// notehead's centre) and rehearsal marks, stacked away from the staff so they don't touch notes or each other.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SysMarks {
    pub ending: f32,
    pub tempo: f32,
    pub rehearsal: f32,
}

impl SysGeo {
    /// The content rows of staff `s` in this system.
    pub fn ext_of(&self, s: usize) -> Ext {
        self.staves.iter().position(|x| x.0 == s).and_then(|k| self.ext.get(k).copied()).unwrap_or_default()
    }
    pub fn staff_y(&self, s: usize) -> Option<f32> {
        self.staves.iter().find(|x| x.0 == s).map(|x| x.1)
    }
    pub fn top(&self, sp: f32) -> f32 {
        self.staves.first().map(|x| x.1 - 2.0 * sp).unwrap_or(0.0)
    }
    pub fn bottom(&self, sp: f32) -> f32 {
        self.staves.last().map(|x| x.1 + 2.0 * sp).unwrap_or(0.0)
    }
}

#[derive(Default, Clone, Debug)]
pub struct Geometry {
    pub pages: Vec<Rect>,
    pub systems: Vec<SysGeo>,
    pub notes: Vec<NoteHit>,
    pub graphics: Vec<(usize, Rect)>,
    pub size: egui::Vec2,
    pub sp: f32,
    pub title_rects: Vec<(&'static str, Rect)>,
}

pub struct Locate {
    pub staff: usize,
    pub measure: usize,
    pub tick: u32,
    /// Staff position relative to middle line (half spaces, up positive).
    pub pos: i32,
    pub step: i32,
    pub x: f32,
}

pub struct DrawState<'a> {
    pub selected: &'a HashSet<NoteRef>,
    pub sel_graphics: &'a HashSet<usize>,
    pub cursor: Option<(usize, usize, u32)>,
    pub play_tick: Option<(usize, u32)>,
    pub show_voice: Option<u8>,
    pub show_control_points: bool,
    pub show_hidden: bool,
    pub sel_measures: Option<(usize, usize)>,
    /// Pencil preview: where, note value, rest?, fits in the bar?
    pub hover_ghost: Option<(Locate, u8, bool, bool)>,
    /// A measure flashing red after something didn't fit: staff, measure, strength 0..1.
    pub flash: Option<(usize, usize, f32)>,
    /// F-37: drawn red, what the Eraser would remove.
    pub erase: Option<EraseHover>,
}

fn col(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

pub fn slot_min_width(sp: f32, dur: u32) -> f32 {
    // Spacing grows with the logarithm of duration (estimated proportional spacing rule).
    let q = (dur.max(1) as f32 / TPQ as f32).log2();
    (sp * (3.1 + 1.25 * q)).max(sp * 1.9)
}

fn visible_staves(score: &Score) -> Vec<usize> {
    let v: Vec<usize> = (0..score.staves.len()).filter(|&i| !score.staves[i].hidden).collect();
    if v.is_empty() { vec![0] } else { v }
}

fn key_width(sp: f32, k: i8) -> f32 {
    if k == 0 { 0.0 } else { k.unsigned_abs() as f32 * sp * 1.0 + sp * 0.6 }
}

/// Natural spacing of one measure: list of (tick, x offset from content start), total width.
fn measure_slots(score: &Score, m: usize, staves: &[usize], sp: f32) -> (Vec<(u32, f32)>, f32) {
    let mt = score.measures[m].ticks();
    let mut starts: BTreeSet<u32> = BTreeSet::new();
    let mut shortest: HashMap<u32, u32> = HashMap::new();
    let mut acc_at: HashSet<u32> = HashSet::new();
    let mut grace_at: HashSet<u32> = HashSet::new();
    for &s in staves {
        for e in &score.events[s][m] {
            if e.grace {
                grace_at.insert(e.start);
                starts.insert(e.start);
                continue;
            }
            starts.insert(e.start);
            let d = e.ticks();
            let en = shortest.entry(e.start).or_insert(u32::MAX);
            *en = (*en).min(d);
            if e.pitches.iter().any(|p| p.alter != 0) {
                acc_at.insert(e.start);
            }
        }
    }
    let mut out = vec![];
    let mut x = sp * 0.5 * score.layout.measure_space_hs[0] as f32 + sp * 0.2;
    // F-36: beat 1 always has its place after the clef, key and time signature: in an empty measure (where the
    // Pencil and the cursor start) and in a measure whose first note comes later, whose empty stretch then takes
    // room like a rest of that length, so a note on beat 3 sits near the middle
    if starts.is_empty() {
        return (vec![(0, x)], sp * 8.0);
    }
    starts.insert(0);
    let starts: Vec<u32> = starts.into_iter().collect();
    for (i, &t) in starts.iter().enumerate() {
        if acc_at.contains(&t) {
            x += sp * 1.0;
        }
        if grace_at.contains(&t) {
            x += sp * 1.6;
        }
        out.push((t, x));
        let next = starts.get(i + 1).copied().unwrap_or(mt.max(t + 1));
        let d = shortest.get(&t).copied().unwrap_or(next - t).min(next.saturating_sub(t).max(1));
        let mut w = slot_min_width(sp, d);
        // a longer gap than the note gets proportional extra space
        if next > t + d {
            w += slot_min_width(sp, next - t - d) * 0.6;
        }
        x += w;
    }
    (out, x.max(sp * 5.0))
}

/// How far one staff's notes and texts reach in measures `m0..m1`, in staff spaces beyond its outer lines (F-05).
fn staff_ext(score: &Score, s: usize, m0: usize, m1: usize) -> Ext {
    let staff = &score.staves[s];
    let (mut na, mut nb) = (0.0f32, 0.0f32);
    let (mut chord, mut expr, mut dynamic, mut lyrics) = (false, false, false, 0usize);
    for m in m0..m1.min(score.measures.len()) {
        let mid = staff.clef_at(m).middle_step();
        let Some(evs) = score.events.get(s).and_then(|x| x.get(m)) else { continue };
        let multi = evs.iter().any(|e| e.voice != evs[0].voice);
        for e in evs {
            chord |= e.chord_symbol.is_some();
            expr |= e.expression.is_some();
            dynamic |= e.dynamic.is_some();
            lyrics = lyrics.max(e.lyric.iter().rposition(|l| !l.is_empty()).map(|i| i + 1).unwrap_or(0));
            if e.pitches.is_empty() {
                continue;
            }
            // half spaces from the middle line, as staff spaces (up positive)
            let hi = (e.pitches.iter().map(|p| p.step).max().unwrap_or(mid) - mid) as f32 * 0.5;
            let lo = (e.pitches.iter().map(|p| p.step).min().unwrap_or(mid) - mid) as f32 * 0.5;
            let k = if e.grace { 0.6 } else { 1.0 };
            // an octave of stem, longer for the flags of 32nds and shorter values
            let stem = if e.value >= 2 { (3.5 + 0.5 * (e.value as f32 - 5.0).max(0.0)) * k } else { 0.0 };
            let up = auto_stem_up(e, mid, multi);
            let (mut top, mut bot) = (hi + 0.5, lo - 0.5);
            if up {
                top = top.max(hi + stem);
            } else {
                bot = bot.min(lo - stem);
            }
            if e.pitches.iter().any(|p| p.alter != 0) {
                top = top.max(hi + 1.5);
            }
            if !e.marks.is_empty() {
                // marks above stack from a space over the top line or the stem, whichever is higher
                top = (top + 0.5).max(3.0) + 1.6 * e.marks.len() as f32;
            }
            na = na.max(top - 2.0);
            nb = nb.max(-bot - 2.0);
        }
    }
    let mut x = Ext { notes_above: na, notes_below: nb, ..Default::default() };
    // texts keep their usual rows and move out only when the notes reach them
    x.chord_y = 2.5f32.max(na + 1.6);
    x.expr_y = if chord { x.chord_y + 2.1 } else { 2.5f32.max(na + 1.6) }.max(4.6);
    x.dyn_y = 3.0f32.max(nb + 1.6);
    x.lyric_y = 5.5f32.max(nb + 2.4).max(if dynamic { x.dyn_y + 2.5 } else { 0.0 });
    x.above = na;
    if chord {
        x.above = x.above.max(x.chord_y + 1.0);
    }
    if expr {
        x.above = x.above.max(x.expr_y + 0.9);
    }
    x.below = nb;
    if dynamic {
        x.below = x.below.max(x.dyn_y + 1.1);
    }
    if lyrics > 0 {
        x.below = x.below.max(x.lyric_y + (lyrics - 1) as f32 * 2.0 + 1.0);
    }
    x
}

/// One system's vertical plan: each staff's middle line below the top line of the first staff, the height from the
/// first staff's top line to the last staff's bottom line, and how far content reaches above and below that (pixels).
struct SysVert {
    /// Gaps between the staves in staff spaces: the Page Setup gap (or more, when the content needs it) and the
    /// least the content allows, for a page that is short of room.
    gaps: Vec<(f32, f32)>,
    sp: f32,
    /// Height at the least gaps.
    h: f32,
    above: f32,
    below: f32,
    ext: Vec<Ext>,
    marks: SysMarks,
}

fn sys_vert(score: &Score, staves: &[usize], m0: usize, m1: usize, sp: f32) -> SysVert {
    let mut ext: Vec<Ext> = staves.iter().map(|&s| staff_ext(score, s, m0, m1)).collect();
    let ms = &score.measures[m0.min(score.measures.len())..m1.min(score.measures.len())];
    // the system's marks over the top staff, each row above the one before
    let mut level = ext[0].above.max(if score.layout.show_measure_numbers != MeasureNumbers::None { 2.4 } else { 0.0 });
    let ending = 2.8f32.max(level + 1.6);
    if ms.iter().any(|m| m.ending.is_some()) {
        level = ending + 0.3;
    }
    let tempo = 3.6f32.max(level + 1.4);
    if ms.iter().any(|m| m.tempo.is_some()) {
        level = tempo + 2.4;
    }
    let rehearsal = 5.5f32.max(level + 1.6);
    if ms.iter().any(|m| m.rehearsal.is_some()) {
        level = rehearsal + 1.3;
    }
    let marks = SysMarks { ending, tempo, rehearsal };
    ext[0].above = level;
    let n = ext.len();
    if ms.iter().any(|m| m.coda_phrase.is_some()) {
        ext[n - 1].below = ext[n - 1].below.max(5.4);
    }
    // between staves: the gap from Page Setup, wider when the content of the two staves needs more; on a full page
    // it may close down to what the content needs (at least 5 spaces)
    let gap = score.layout.staff_gap;
    let gaps: Vec<(f32, f32)> = (0..n.saturating_sub(1))
        .map(|k| {
            let need = ext[k].below + ext[k + 1].above + 1.0;
            (gap.max(need), gap.min(5.0).max(need))
        })
        .collect();
    let h = (4.0 * n as f32 + gaps.iter().map(|g| g.1).sum::<f32>()) * sp;
    SysVert { gaps, sp, h, above: ext[0].above * sp, below: ext[n - 1].below * sp, ext, marks }
}

impl SysVert {
    /// Extra height when the gaps open fully to the Page Setup gap.
    fn slack(&self) -> f32 {
        self.gaps.iter().map(|g| g.0 - g.1).sum::<f32>() * self.sp
    }
    /// Middle lines below the first staff's top line with the gaps opened by `t` (0 = least, 1 = Page Setup), and
    /// the height.
    fn mids(&self, t: f32) -> (Vec<f32>, f32) {
        let mut y = 2.0;
        let mut v = vec![y * self.sp];
        for g in &self.gaps {
            y += 4.0 + g.1 + (g.0 - g.1) * t;
            v.push(y * self.sp);
        }
        (v, (y + 2.0) * self.sp)
    }
}

pub fn layout(score: &Score, origin: Pos2, zoom: f32, linear: bool) -> Geometry {
    let sp = score.layout.staff_space * zoom;
    let page_w = score.layout.page_w * zoom;
    let page_h = score.layout.page_h * zoom;
    let [m_top, m_bottom, m_left, m_right] = score.layout.margins().map(|m| m * zoom);
    let staves = visible_staves(score);
    let sys_gap = score.layout.system_gap * sp;
    let name_indent = {
        let longest = staves.iter().map(|&s| score.staves[s].name.chars().count()).max().unwrap_or(0) as f32;
        (longest * sp * 0.75 + sp * 2.0).min(sp * 14.0)
    };
    let short_indent = if score.staves.len() > 1 { sp * 4.0 } else { sp * 1.0 };

    let mut geo = Geometry { sp, ..Default::default() };
    let system_ranges = if linear { vec![(0, score.measures.len())] } else { score.systems() };
    let spp = score.layout.systems_per_page.max(1);
    // each system's height and how far its content reaches above and below (F-05)
    let vert: Vec<SysVert> = system_ranges.iter().map(|&(m0, m1)| sys_vert(score, &staves, m0, m1, sp)).collect();
    // the title area grows with the subtitle and credit lines, and the first system's top line stays clear of the
    // last text line by its own content (tempo mark, chord symbols, high notes) and a space and a half
    let tb = title_block(&score.text);
    let title_h = (sp * tb.3.max(12.0)).max(sp * (tb.2 + 0.9 + 1.5) + vert.first().map(|v| v.above).unwrap_or(0.0));

    let mut page_idx = 0usize;
    let mut sys_on_page = 0usize;
    let mut page_cap = spp;
    let mut extra = 0.0f32;
    let mut open = 1.0f32;
    let page_gap = sp * 5.0;
    let mut cur_y = 0.0f32;
    // the least distance from one system's bottom line to the next one's top line
    let min_gap = |a: &SysVert, b: &SysVert| (sp * 4.0).max(a.below + b.above + sp * 1.5);

    let page_rect = |i: usize| -> Rect {
        let y = origin.y + i as f32 * (page_h + page_gap);
        Rect::from_min_size(pos2(origin.x, y), vec2(page_w, page_h))
    };

    for (si, &(m0, m1)) in system_ranges.iter().enumerate() {
        let first = si == 0;
        let v = &vert[si];
        let (left, right, top);
        if linear {
            left = origin.x + m_left + name_indent;
            right = f32::MAX;
            top = origin.y + m_top + (title_h * 0.4).max(v.above + sp);
        } else {
            if sys_on_page >= page_cap {
                page_idx += 1;
                sys_on_page = 0;
            }
            if sys_on_page == 0 {
                let pr = page_rect(page_idx);
                geo.pages.push(pr);
                cur_y = pr.top() + m_top + if page_idx == 0 { title_h } else { (sp * 4.0).max(v.above + sp) };
                // the systems this page takes: up to systems-per-page, while the last one's bottom line and its content
                // (up to two spaces into the bottom margin) stay on the page
                let limit = pr.bottom() - m_bottom;
                let mut used = v.h;
                let mut n = 1;
                while n < spp && si + n < vert.len() {
                    let (a, b) = (&vert[si + n - 1], &vert[si + n]);
                    let next = used + min_gap(a, b) + b.h;
                    if cur_y + next + (b.below - sp * 2.0).max(0.0) > limit {
                        break;
                    }
                    used = next;
                    n += 1;
                }
                page_cap = n;
                // the room left first opens the staff gaps back to the Page Setup gap, and the rest is shared evenly
                // between the systems
                let room = (limit - cur_y - used).max(0.0);
                let slack: f32 = vert[si..si + n].iter().map(|v| v.slack()).sum();
                open = if slack > 0.0 { (room / slack).min(1.0) } else { 1.0 };
                let left = (room - slack * open).max(0.0);
                extra = if n > 1 { left / (n - 1) as f32 } else { 0.0 };
            }
            let pr = page_rect(page_idx);
            left = pr.left() + m_left + if first { name_indent } else { short_indent };
            right = pr.right() - m_right;
            top = cur_y;
            if let Some(next) = vert.get(si + 1) {
                let g = min_gap(v, next);
                cur_y += v.mids(open).1 + (g + extra).min(g.max(sys_gap + sp * 10.0));
            }
            sys_on_page += 1;
        }
        let staves_y: Vec<(usize, f32)> = staves.iter().zip(&v.mids(if linear { 1.0 } else { open }).0).map(|(&s, &y)| (s, top + y)).collect();
        // natural widths; a compressed run becomes one measure
        let runs = score.compressed_runs();
        let mut nat = vec![];
        let mut total = 0.0;
        let mut m = m0;
        while m < m1 {
            let span = runs.iter().find(|r| r.0 == m).map(|r| r.1.min(m1) - m).unwrap_or(1).max(1);
            let hdr_clef = m == m0;
            let key = score.measures[m].key;
            let prev_key = if m > 0 { score.measures[m - 1].key } else { 0 };
            let hdr_key = (m == m0 && key != 0) || (m > m0 && key != prev_key);
            let prev_time = if m > 0 { Some((score.measures[m - 1].num, score.measures[m - 1].den)) } else { None };
            let hdr_time = (m == 0) || prev_time != Some((score.measures[m].num, score.measures[m].den));
            let mut h = sp * 0.5;
            if hdr_clef {
                h += sp * 3.6;
            } else if staves.iter().any(|&s| score.staves[s].clefs.iter().any(|c| c.0 == m && m > 0)) {
                h += sp * 2.6;
            }
            if hdr_key {
                h += key_width(sp, key.max(-7).min(7)).max(if key == 0 && prev_key != 0 { prev_key.unsigned_abs() as f32 * sp } else { 0.0 });
            }
            if hdr_time {
                h += sp * 2.8;
            }
            let (slots, w) = if span > 1 { (vec![], sp * 12.0) } else { measure_slots(score, m, &staves, sp) };
            total += h + w;
            nat.push((m, h, slots, w, hdr_clef, hdr_key, hdr_time, span));
            m += span;
        }
        let avail = if linear { total } else { right - left };
        let last_system = si + 1 == system_ranges.len();
        let stretch = if linear || (last_system && total < avail * 0.6) { 1.0 } else { (avail / total.max(1.0)).max(0.3) };
        let mut x = left;
        let mut measures = vec![];
        for (m, h, slots, w, hc, hk, ht, span) in nat {
            let x0 = x;
            let content_x = x0 + h;
            let k = stretch;
            let slots2: Vec<(u32, f32)> = slots.iter().map(|(t, sx)| (*t, content_x + sx * k)).collect();
            let x1 = content_x + w * k;
            measures.push(MeasGeo { m, x0, content_x, x1, slots: slots2, header_clef: hc, header_key: hk, header_time: ht, span });
            x = x1;
        }
        let right = if linear { x } else { right.min(x).max(x) };
        geo.systems.push(SysGeo { page: page_idx, m0, m1, staves: staves_y, left, right, measures, first, ext: v.ext.clone(), marks: v.marks });
    }
    if linear {
        let r = geo.systems.last().map(|s| s.right).unwrap_or(origin.x + page_w);
        let b = geo.systems.last().map(|s| s.bottom(sp)).unwrap_or(origin.y + page_h);
        let pr = Rect::from_min_max(origin, pos2(r + m_right, (b + sp * 12.0).max(origin.y + page_h * 0.5)));
        geo.pages.push(pr);
    }
    let mut size = vec2(0.0, 0.0);
    for p in &geo.pages {
        size.x = size.x.max(p.right() - origin.x);
        size.y = size.y.max(p.bottom() - origin.y);
    }
    geo.size = size + vec2(sp * 4.0, sp * 4.0);
    geo
}

pub fn x_for_tick(mg: &MeasGeo, tick: u32, mticks: u32) -> f32 {
    if mg.slots.is_empty() {
        return mg.content_x + (mg.x1 - mg.content_x) * tick as f32 / mticks.max(1) as f32;
    }
    let mut prev = (0u32, mg.content_x);
    if let Some(f) = mg.slots.first() {
        if tick <= f.0 {
            if f.0 == 0 {
                return f.1;
            }
            return mg.content_x + (f.1 - mg.content_x) * tick as f32 / f.0 as f32;
        }
        prev = *f;
    }
    for &(t, x) in &mg.slots {
        if t == tick {
            return x;
        }
        if t > tick {
            let f = (tick - prev.0) as f32 / (t - prev.0).max(1) as f32;
            return prev.1 + (x - prev.1) * f;
        }
        prev = (t, x);
    }
    let endx = mg.x1 - (mg.x1 - prev.1) * 0.15;
    let f = (tick.saturating_sub(prev.0)) as f32 / (mticks.saturating_sub(prev.0)).max(1) as f32;
    prev.1 + (endx - prev.1) * f.min(1.0)
}

/// Screen position → staff, measure, tick (snapped to `grid` ticks) and pitch line.
pub fn locate(score: &Score, geo: &Geometry, p: Pos2, grid: u32) -> Option<Locate> {
    let sp = geo.sp;
    for sys in &geo.systems {
        if p.x < sys.left - sp * 2.0 || p.x > sys.right + sp {
            continue;
        }
        // nearest staff vertically
        let mut best: Option<(usize, f32)> = None;
        for &(s, y) in &sys.staves {
            let d = (p.y - y).abs();
            if d < 9.0 * sp && best.map(|b| d < (p.y - b.1).abs()).unwrap_or(true) {
                best = Some((s, y));
            }
        }
        let (staff, mid) = match best {
            Some(b) => b,
            None => continue,
        };
        let mg = sys.measures.iter().find(|mg| p.x >= mg.x0 && p.x < mg.x1).or_else(|| {
            if p.x >= sys.right - 1.0 { sys.measures.last() } else { sys.measures.first() }
        })?;
        let m = mg.m;
        let mt = score.measures[m].ticks();
        // near an existing slot?
        let mut tick = None;
        for &(t, x) in &mg.slots {
            if (p.x - x).abs() < sp * 0.9 {
                tick = Some(t);
            }
        }
        let tick = tick.unwrap_or_else(|| {
            // interpolate, with the same mapping `x_for_tick` draws with (so the Pencil's preview is where the
            // pointer is)
            let mut lo = (0u32, mg.content_x);
            let mut hi = None;
            for &(t, x) in &mg.slots {
                if x <= p.x {
                    lo = (t, x);
                } else {
                    hi = Some((t, x));
                    break;
                }
            }
            let hi = hi.unwrap_or((mt, mg.x1 - (mg.x1 - lo.1) * 0.15));
            let f = ((p.x - lo.1) / (hi.1 - lo.1).max(1.0)).clamp(0.0, 1.0);
            let raw = lo.0 as f32 + (hi.0 - lo.0) as f32 * f;
            let g = grid.max(1) as f32;
            let t = ((raw / g).round() * g) as u32;
            t.min(mt.saturating_sub(1).max(0) / grid.max(1) * grid.max(1))
        });
        let pos = ((mid - p.y) / (sp * 0.5)).round() as i32;
        let clef = score.staves[staff].clef_at(m);
        let step = clef.middle_step() + pos;
        return Some(Locate { staff, measure: m, tick, pos, step, x: x_for_tick(mg, tick, mt) });
    }
    None
}

/// A note's chord offsets, its accidentals (pitch index, centre x, alteration, in parentheses) and where its dots go.
struct Placed {
    xoff: Vec<f32>,
    accs: Vec<(usize, f32, i8, bool)>,
    dot_x: f32,
}

#[cfg(test)]
thread_local!(static INK: std::cell::RefCell<Vec<(u8, Rect)>> = const { std::cell::RefCell::new(Vec::new()) });

/// Tests: what was drawn where (1 accidental, 2 dot), to check for collisions.
fn ink_log(_kind: u8, _r: Rect) {
    #[cfg(test)]
    INK.with(|v| v.borrow_mut().push((_kind, _r)));
}

struct NoteLayout {
    x: f32,
    ys: Vec<f32>,
    up: bool,
    stem_x: f32,
    stem_end: f32,
    beamed: bool,
}

fn auto_stem_up(e: &Event, mid_step: i32, multi_voice: bool) -> bool {
    match e.stem {
        Stem::Up => true,
        Stem::Down => false,
        Stem::Auto => {
            if multi_voice {
                return e.voice % 2 == 1;
            }
            if e.pitches.is_empty() {
                return true;
            }
            let hi = e.pitches.iter().map(|p| p.step).max().unwrap() - mid_step;
            let lo = e.pitches.iter().map(|p| p.step).min().unwrap() - mid_step;
            hi.abs() <= lo.abs() && (hi + lo) < 0 || (hi + lo) < 0
        }
    }
}

fn beat_len(mi: &MeasureInfo) -> u32 {
    let unit = TPQ * 4 / mi.den.max(1) as u32;
    if mi.den >= 8 && mi.num % 3 == 0 && mi.num > 3 { unit * 3 } else if mi.den >= 8 { unit * 2 } else { unit }
}

/// Group indices of events (one voice, sorted) into beam groups.
fn beam_groups(evs: &[(usize, &Event)], mi: &MeasureInfo) -> Vec<Vec<usize>> {
    let bl = beat_len(mi);
    let mut groups: Vec<Vec<usize>> = vec![];
    let mut cur: Vec<usize> = vec![];
    let mut cur_beat = u32::MAX;
    for (k, (_, e)) in evs.iter().enumerate() {
        let beamable = e.value >= 4 && !e.is_rest() && !e.grace && e.beam != BeamMode::None;
        let beat = e.start / bl;
        let breaks = !beamable
            || cur.is_empty()
            || e.beam == BeamMode::Break
            || (beat != cur_beat && !cur.last().map(|&j| evs[j].1.beam == BeamMode::Join).unwrap_or(false));
        if breaks {
            if cur.len() > 1 {
                groups.push(cur.clone());
            }
            cur.clear();
        }
        if beamable {
            if cur.is_empty() {
                cur_beat = beat;
            }
            cur.push(k);
        } else {
            cur.clear();
        }
    }
    if cur.len() > 1 {
        groups.push(cur);
    }
    groups
}

/// The typeface of every text on the score page (title, credits, chord symbols, lyrics, numbers, tempo). It is in this
/// one place so that changing the page's font is one line (F-06); the PDF embeds whatever font this family resolves to.
pub const PAGE_TEXT: FontFamily = FontFamily::Proportional;

pub fn page_font(size: f32) -> FontId {
    FontId::new(size, PAGE_TEXT)
}

pub fn draw(p: &Painter, score: &Score, geo: &Geometry, st: &DrawState, out_hits: &mut Vec<NoteHit>, out_graphics: &mut Vec<(usize, Rect)>) {
    let sp = geo.sp;
    let black = Color32::BLACK;
    let line_st = Stroke::new((sp * STAFF_LINE).max(0.6), Color32::from_gray(20));
    let font = page_font;

    for (i, pr) in geo.pages.iter().enumerate() {
        // a soft shadow under the sheet; the sheet itself is always white, in light and dark mode
        p.add(egui::Shadow { offset: [0, 3], blur: 18, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(*pr, 0.0));
        p.rect_filled(*pr, 0.0, crate::theme::PAGE);
        let t = &score.text;
        // F-06: the page's own margins (the title block starts at the top one; credits line up with the margins)
        let [mt, mb, ml, mr] = score.layout.margins().map(|m| m * sp / score.layout.staff_space);
        let (left, right) = (pr.left() + ml, pr.right() - mr);
        if i == 0 {
            let cx = pr.center().x;
            let top = pr.top() + mt;
            p.text(pos2(cx, top + sp * 1.5), Align2::CENTER_CENTER, &t.title, font(sp * 3.0), black);
            for (k, line) in t.subtitle.lines().enumerate() {
                p.text(pos2(cx, top + sp * (4.5 + k as f32 * 2.2)), Align2::CENTER_CENTER, line, font(sp * 1.8), black);
            }
            let (_, credits_y, _, _) = title_block(t);
            // composer and lyricist / arranger on the right, one line each; instructions on the left
            for (k, line) in t.composer.lines().chain(t.lyricist.lines()).filter(|l| !l.trim().is_empty()).enumerate() {
                p.text(pos2(right, top + sp * (credits_y + k as f32 * 1.8)), Align2::RIGHT_CENTER, line, font(sp * 1.4), black);
            }
            for (k, line) in t.instructions.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                p.text(pos2(left, top + sp * (credits_y + k as f32 * 1.8)), Align2::LEFT_CENTER, line, font(sp * 1.4), black);
            }
        }
        // Text Elements around the page, as the printed page shows them (F-06): the header centred at the top of every
        // page ("#P" is the page number), the footer centred at the bottom, the page number at the bottom right
        let foot_y = pr.bottom() - (mb * 0.5).max(sp * 1.5);
        if !t.header.is_empty() {
            let txt = t.header.replace("#P", &format!("{}", i + 1));
            p.text(pos2(pr.center().x, pr.top() + (mt * 0.5).max(sp * 1.5)), Align2::CENTER_CENTER, txt, font(sp * 1.3), Color32::DARK_GRAY);
        }
        if !t.footer.is_empty() {
            p.text(pos2(pr.center().x, foot_y), Align2::CENTER_CENTER, &t.footer, font(sp * 1.2), Color32::DARK_GRAY);
        }
        if i == 0 && !t.copyright.is_empty() {
            let y = if t.footer.is_empty() { foot_y } else { foot_y - sp * 1.8 };
            p.text(pos2(pr.center().x, y), Align2::CENTER_CENTER, &t.copyright, font(sp * 1.2), black);
        }
        if t.show_page_numbers {
            p.text(pos2(right, foot_y), Align2::RIGHT_CENTER, format!("{}", i + 1), font(sp * 1.3), black);
        }
    }

    let mut dyn_rects: Vec<Rect> = vec![];
    for sys in &geo.systems {
        // selection of measures
        if let Some((a, b)) = st.sel_measures {
            for mg in &sys.measures {
                if mg.m + mg.span > a && mg.m <= b {
                    let r = Rect::from_min_max(pos2(mg.x0, sys.top(sp) - sp), pos2(mg.x1, sys.bottom(sp) + sp));
                    p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(80, 130, 255, 40));
                }
            }
        }
        // staff lines
        for &(s, mid) in &sys.staves {
            let lines = score.staves[s].lines.clamp(1, 6) as i32;
            for l in 0..lines {
                let off = if lines == 5 { l - 2 } else if lines == 1 { 0 } else { l - lines / 2 };
                let y = mid + off as f32 * sp;
                p.line_segment([pos2(sys.left, y), pos2(sys.right, y)], line_st);
            }
            let name = if sys.first { &score.staves[s].name } else { &score.staves[s].abbrev };
            if score.staves.len() > 1 || sys.first {
                p.text(pos2(sys.left - sp * if score.staves.len() > 1 { 1.8 } else { 1.0 }, mid), Align2::RIGHT_CENTER, name, font(sp * 1.4), black);
            }
        }
        // system barline at left
        if sys.staves.len() > 1 {
            p.line_segment([pos2(sys.left, sys.top(sp)), pos2(sys.left, sys.bottom(sp))], Stroke::new(sp * 0.15, black));
        }
        // braces & brackets
        for (k, &(s, mid)) in sys.staves.iter().enumerate() {
            if let Some(&(_, mid2)) = sys.staves.get(k + 1) {
                if score.staves[s].brace_with_next {
                    g::brace(p, pos2(sys.left - sp * 0.4, mid - 2.0 * sp), pos2(sys.left - sp * 0.4, mid2 + 2.0 * sp), sp, black);
                } else if score.staves[s].bracket_with_next {
                    g::bracket(p, pos2(sys.left - sp * 0.6, mid - 2.0 * sp), pos2(sys.left - sp * 0.6, mid2 + 2.0 * sp), sp, black);
                }
            }
        }

        for mg in &sys.measures {
            let m = mg.m;
            let mi = &score.measures[m];
            let mt = mi.ticks();
            let mut multi_voice_by_staff = HashMap::new();
            for &(s, _) in &sys.staves {
                let vs: HashSet<u8> = score.events[s][m].iter().map(|e| e.voice).collect();
                multi_voice_by_staff.insert(s, vs.len() > 1);
            }
            // header: clef, key, time
            for &(s, mid) in &sys.staves {
                let mut x = mg.x0 + sp * 0.8;
                let clef = score.staves[s].clef_at(m);
                let clef_change = score.staves[s].clefs.iter().any(|c| c.0 == m && m > 0);
                if mg.header_clef || clef_change {
                    let k = if mg.header_clef { 1.0 } else { 0.75 };
                    draw_clef(p, pos2(x + sp * 1.2 * k, mid), sp * k, clef, black);
                    x += sp * 3.6 * k;
                }
                if mg.header_key {
                    let key = mi.key;
                    if key == 0 && m > 0 {
                        // cancellation naturals
                        let prev = score.measures[m - 1].key;
                        for i in 0..prev.unsigned_abs() {
                            let pos = key_pos(clef, prev.signum(), i as usize);
                            g::natural(p, pos2(x + sp * 0.5 + i as f32 * sp, mid - pos as f32 * sp * 0.5), sp, black);
                        }
                        x += prev.unsigned_abs() as f32 * sp;
                    }
                    for i in 0..key.unsigned_abs() {
                        let pos = key_pos(clef, key.signum(), i as usize);
                        let c = pos2(x + sp * 0.5 + i as f32 * sp, mid - pos as f32 * sp * 0.5);
                        if key > 0 { g::sharp(p, c, sp, black) } else { g::flat(p, c, sp, black) }
                    }
                    x += key_width(sp, key);
                }
                if mg.header_time {
                    g::time_sig_digit(p, pos2(x + sp * 1.2, mid - sp), sp, &mi.num.to_string(), black);
                    g::time_sig_digit(p, pos2(x + sp * 1.2, mid + sp), sp, &mi.den.to_string(), black);
                }
            }
            // a multi-measure rest: a thick bar on the middle line, ended by short verticals, and the count
            // above the staff in the style of a time signature
            if mg.span > 1 {
                for &(_, mid) in &sys.staves {
                    let (xa, xb) = (mg.content_x + sp * 1.2, mg.x1 - sp * 1.6);
                    p.rect_filled(Rect::from_min_max(pos2(xa, mid - sp * 0.45), pos2(xb, mid + sp * 0.45)), 0.0, black);
                    for x in [xa, xb] {
                        p.line_segment([pos2(x, mid - sp), pos2(x, mid + sp)], Stroke::new(sp * 0.16, black));
                    }
                    g::time_sig_digit(p, pos2((xa + xb) / 2.0, mid - sp * 3.6), sp, &mg.span.to_string(), black);
                }
            }
            // barline at end (of the last measure a multi-measure rest stands for)
            draw_barline(p, score, sys, mg, score.measures[mg.m + mg.span - 1].barline, sp, black);
            if m == 0 || mg.m == sys.m0 {
                if mi.barline == Barline::RepeatStart || mi.barline == Barline::RepeatBoth {
                    // begin repeat drawn at start
                }
            }
            if m > 0 {
                let prev = score.measures[m - 1].barline;
                if prev == Barline::RepeatStart || prev == Barline::RepeatBoth {
                    for &(_, mid) in &sys.staves {
                        let x = mg.content_x - sp * 0.4;
                        p.line_segment([pos2(x - sp * 0.6, mid - 2.0 * sp), pos2(x - sp * 0.6, mid + 2.0 * sp)], Stroke::new(sp * 0.45, black));
                        p.line_segment([pos2(x, mid - 2.0 * sp), pos2(x, mid + 2.0 * sp)], Stroke::new(sp * 0.12, black));
                        p.circle_filled(pos2(x + sp * 0.5, mid - sp * 0.5), sp * 0.2, black);
                        p.circle_filled(pos2(x + sp * 0.5, mid + sp * 0.5), sp * 0.2, black);
                    }
                }
            }
            // measure numbers
            // a pickup is measure 0 and shows no number (F-05)
            let num = score.measure_label(m);
            let show_num = num > 0
                && match score.layout.show_measure_numbers {
                    MeasureNumbers::None => false,
                    MeasureNumbers::EverySystem => m == sys.m0 && m > 0,
                    MeasureNumbers::EveryMeasure => true,
                    MeasureNumbers::Every5 => num.is_multiple_of(5),
                };
            if show_num {
                p.text(pos2(mg.x0 + sp * 0.2, sys.top(sp) - sp * 1.6), Align2::LEFT_CENTER, format!("{num}"), font(sp * 1.2), black);
            }
            // tempo
            if let Some(t) = mi.tempo_shown() {
                // over beat 1 (the first note of the measure), in the row above everything the staff holds (F-05); a
                // dotted quarter in compound meter when the tempo was given that way
                let y = sys.top(sp) - sp * sys.marks.tempo;
                let x = mg.slots.first().map(|s| s.1).unwrap_or(mg.content_x);
                let tw = draw_tempo(p, pos2(x, y), sp, mi.tempo_dotted, black);
                p.text(pos2(x + tw + sp * 0.5, y), Align2::LEFT_CENTER, format!("= {t}"), font(sp * 1.5), black);
            }
            if let Some(r) = &mi.rehearsal {
                let rr = p.text(pos2(mg.x0, sys.top(sp) - sp * sys.marks.rehearsal), Align2::LEFT_CENTER, r, font(sp * 1.8), black);
                p.rect_stroke(rr.expand(sp * 0.3), 0.0, Stroke::new(sp * 0.1, black), egui::StrokeKind::Outside);
            }
            if let Some(e) = &mi.ending {
                let y = sys.top(sp) - sp * sys.marks.ending;
                let x1 = mg.x1 - sp * 0.5;
                let st2 = Stroke::new(sp * 0.12, black);
                p.line_segment([pos2(mg.x0 + sp * 0.3, y + sp * 1.5), pos2(mg.x0 + sp * 0.3, y)], st2);
                p.line_segment([pos2(mg.x0 + sp * 0.3, y), pos2(x1, y)], st2);
                p.text(pos2(mg.x0 + sp * 0.8, y + sp * 0.8), Align2::LEFT_CENTER, e, font(sp * 1.3), black);
            }
            if let Some(c) = &mi.coda_phrase {
                p.text(pos2(mg.x1 - sp * 0.4, sys.bottom(sp) + sp * 4.5), Align2::RIGHT_CENTER, c, font(sp * 1.5), black);
            }

            for &(s, mid) in &sys.staves {
                let staff = &score.staves[s];
                let clef = staff.clef_at(m);
                let mid_step = clef.middle_step();
                let multi = *multi_voice_by_staff.get(&s).unwrap_or(&false);
                let evs = &score.events[s][m];
                // accidental state
                let mut acc_state: HashMap<i32, i8> = HashMap::new();
                let key_of = |step: i32| key_alter(mi.key, step.rem_euclid(7) as usize);
                // an empty measure stays blank, as in the original (no rest is drawn until one is entered)
                // layout notes per event
                let mut nl: HashMap<usize, NoteLayout> = HashMap::new();
                // two voices at one moment (F-04): the upper voice (stems up) moves right by a notehead on a second, a
                // crossing or a unison that can't share a head
                let mut shift: HashMap<usize, f32> = HashMap::new();
                if multi {
                    let mut starts: Vec<u32> = evs.iter().filter(|e| !e.grace && !e.is_rest()).map(|e| e.start).collect();
                    starts.sort_unstable();
                    starts.dedup();
                    for t in starts {
                        let at: Vec<(usize, &Event)> = evs.iter().enumerate().filter(|(_, e)| e.start == t && !e.grace && !e.is_rest()).collect();
                        let ups: Vec<&(usize, &Event)> = at.iter().filter(|(_, e)| auto_stem_up(e, mid_step, true)).collect();
                        let downs: Vec<&(usize, &Event)> = at.iter().filter(|(_, e)| !auto_stem_up(e, mid_step, true)).collect();
                        let (Some(u0), Some(d0)) = (ups.first(), downs.first()) else { continue };
                        let us: Vec<i32> = ups.iter().flat_map(|(_, e)| e.pitches.iter().map(|q| q.step)).collect();
                        let ds: Vec<i32> = downs.iter().flat_map(|(_, e)| e.pitches.iter().map(|q| q.step)).collect();
                        let k = collide::voice_shift(
                            &collide::VoiceAt { steps: &us, value: u0.1.value, dots: u0.1.dots },
                            &collide::VoiceAt { steps: &ds, value: d0.1.value, dots: d0.1.dots },
                        );
                        if k > 0 {
                            let w = g::notehead_width(sp, d0.1.value).max(g::notehead_width(sp, u0.1.value));
                            for (i, _) in &ups {
                                shift.insert(*i, w);
                            }
                        }
                    }
                }
                // voices
                let mut voices: Vec<u8> = evs.iter().map(|e| e.voice).collect();
                voices.sort();
                voices.dedup();
                for &v in &voices {
                    let ve: Vec<(usize, &Event)> = evs.iter().enumerate().filter(|(_, e)| e.voice == v && !e.grace).collect();
                    let groups = beam_groups(&ve, mi);
                    let mut in_group: HashMap<usize, usize> = HashMap::new();
                    for (gi, gr) in groups.iter().enumerate() {
                        for &k in gr {
                            in_group.insert(ve[k].0, gi);
                        }
                    }
                    // stem direction per group
                    let mut group_up: HashMap<usize, bool> = HashMap::new();
                    for (gi, gr) in groups.iter().enumerate() {
                        let first = ve[gr[0]].1;
                        if first.stem != Stem::Auto || multi {
                            group_up.insert(gi, auto_stem_up(first, mid_step, multi));
                        } else {
                            let sum: i32 = gr.iter().flat_map(|&k| ve[k].1.pitches.iter().map(|p| p.step - mid_step)).sum();
                            group_up.insert(gi, sum < 0);
                        }
                    }
                    for &(i, e) in &ve {
                        let x = x_for_tick(mg, e.start, mt) + e.dx * sp + shift.get(&i).copied().unwrap_or(0.0);
                        let ys: Vec<f32> = e.pitches.iter().map(|pp| mid - (pp.step - mid_step) as f32 * sp * 0.5).collect();
                        let up = in_group.get(&i).and_then(|gi| group_up.get(gi).copied()).unwrap_or_else(|| auto_stem_up(e, mid_step, multi));
                        // the stem joins the notehead where the font's anchor says (its edge on stemUpSE / stemDownNW)
                        let stem_x = g::stem_attach(pos2(x, 0.0), sp, e.value, up, sp * 0.12).x;
                        let (ytop, ybot) = ys.iter().fold((f32::MAX, f32::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
                        let mut len = sp * 0.5 * score.layout.stem_len_hs as f32;
                        if e.value >= 6 {
                            len += sp * (e.value - 5) as f32 * 0.8;
                        }
                        let mut stem_end = if up { ytop - len } else { ybot + len };
                        // stems reach the middle line at least
                        if up && stem_end > mid { stem_end = mid; }
                        if !up && stem_end < mid { stem_end = mid; }
                        nl.insert(i, NoteLayout { x, ys, up, stem_x, stem_end, beamed: in_group.contains_key(&i) });
                    }
                    // beams: straighten
                    for gr in &groups {
                        let idxs: Vec<usize> = gr.iter().map(|&k| ve[k].0).collect();
                        let up = nl[&idxs[0]].up;
                        let flat = evs[idxs[0]].flat_beam;
                        let (fx, fy) = (nl[&idxs[0]].stem_x, nl[&idxs[0]].stem_end);
                        let (lx, ly) = (nl[idxs.last().unwrap()].stem_x, nl[idxs.last().unwrap()].stem_end);
                        let mut slope = if flat || (lx - fx).abs() < 1.0 { 0.0 } else { ((ly - fy) / (lx - fx)).clamp(-0.25, 0.25) };
                        if flat {
                            slope = 0.0;
                        }
                        // ensure all stems are long enough
                        let mut offset = 0.0f32;
                        for &i in &idxs {
                            let n = &nl[&i];
                            let line_y = fy + slope * (n.stem_x - fx);
                            let (ytop, ybot) = n.ys.iter().fold((f32::MAX, f32::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
                            if up {
                                offset = offset.min(ytop - sp * 2.8 - line_y);
                            } else {
                                offset = offset.max(ybot + sp * 2.8 - line_y);
                            }
                        }
                        for &i in &idxs {
                            let n = nl.get_mut(&i).unwrap();
                            n.stem_end = fy + offset + slope * (n.stem_x - fx);
                        }
                        // draw beams
                        let thick = sp * 0.5 * score.layout.beam_thick_pct as f32 / 100.0;
                        let gap = sp * 0.75;
                        for w in idxs.windows(2) {
                            let (a, b) = (&nl[&w[0]], &nl[&w[1]]);
                            let la = evs[w[0]].value.saturating_sub(3);
                            let lb = evs[w[1]].value.saturating_sub(3);
                            let common = la.min(lb);
                            let c = col(evs[w[0]].color);
                            let c = if st.selected.contains(&NoteRef { staff: s, measure: m, idx: w[0] }) { SEL } else { c };
                            for lvl in 0..common {
                                let d = if up { lvl as f32 * gap } else { -(lvl as f32) * gap };
                                beam_quad(p, pos2(a.stem_x, a.stem_end + d), pos2(b.stem_x, b.stem_end + d), thick, up, c);
                            }
                            // stubs
                            if la > common {
                                for lvl in common..la {
                                    let d = if up { lvl as f32 * gap } else { -(lvl as f32) * gap };
                                    let bx = a.stem_x + sp * 1.2;
                                    let by = a.stem_end + slope * sp * 1.2;
                                    beam_quad(p, pos2(a.stem_x, a.stem_end + d), pos2(bx, by + d), thick, up, c);
                                }
                            }
                            if lb > common && w[1] == *idxs.last().unwrap() {
                                for lvl in common..lb {
                                    let d = if up { lvl as f32 * gap } else { -(lvl as f32) * gap };
                                    let ax = b.stem_x - sp * 1.2;
                                    let ay = b.stem_end - slope * sp * 1.2;
                                    beam_quad(p, pos2(ax, ay + d), pos2(b.stem_x, b.stem_end + d), thick, up, c);
                                }
                            }
                        }
                    }
                    // tuplet brackets
                    let mut k = 0;
                    while k < ve.len() {
                        if let Some((a, _)) = ve[k].1.tuplet {
                            let mut j = k;
                            let mut sum = 0;
                            let full = ve[k].1.tuplet.map(|(a2, n2)| base_ticks(ve[k].1.value) * n2 as u32 * a2 as u32 / a2 as u32).unwrap_or(0);
                            let group_len = base_ticks(ve[k].1.value) * ve[k].1.tuplet.unwrap().1 as u32;
                            let _ = full;
                            while j < ve.len() && ve[j].1.tuplet.is_some() && sum < group_len {
                                sum += ve[j].1.ticks();
                                j += 1;
                            }
                            let first = &nl[&ve[k].0];
                            let last = &nl[&ve[j - 1].0];
                            // the number goes on the stem side, clear of stems, beams, noteheads and the staff lines
                            // above the staff, as in the original; only a lower voice of two keeps it below
                            let up = !(multi && ve[k].1.voice % 2 == 0);
                            let mut y = if up { mid - sp * 2.0 } else { mid + sp * 2.0 };
                            for q in k..j {
                                let n = &nl[&ve[q].0];
                                for &hy in &n.ys {
                                    y = if up { y.min(hy - sp * 0.6) } else { y.max(hy + sp * 0.6) };
                                }
                                if n.up == up {
                                    y = if up { y.min(n.stem_end) } else { y.max(n.stem_end) };
                                }
                            }
                            let y = if up { y - sp * 1.6 } else { y + sp * 1.6 };
                            let x_a = first.x.min(first.stem_x) - sp * 0.5;
                            let x_b = last.x.max(last.stem_x) + sp * 0.5;
                            let cx = (x_a + x_b) / 2.0;
                            let font = FontId::new(sp * 1.6, egui::FontFamily::Proportional);
                            p.text(pos2(cx, y), Align2::CENTER_CENTER, a.to_string(), font, black);
                            let all_beamed = (k..j).all(|q| nl[&ve[q].0].beamed);
                            if !all_beamed && x_b - x_a > sp * 3.0 {
                                let st2 = Stroke::new(sp * 0.1, black);
                                let hook = if up { sp * 0.6 } else { -sp * 0.6 };
                                p.line_segment([pos2(x_a, y), pos2(cx - sp * 0.8, y)], st2);
                                p.line_segment([pos2(cx + sp * 0.8, y), pos2(x_b, y)], st2);
                                p.line_segment([pos2(x_a, y), pos2(x_a, y + hook)], st2);
                                p.line_segment([pos2(x_b, y), pos2(x_b, y + hook)], st2);
                            }
                            k = j;
                        } else {
                            k += 1;
                        }
                    }
                }
                // what shares a moment (F-04): the chord's seconds, the accidentals of every voice in columns left of
                // all the noteheads, and the dots in one column right of them
                let mut placed: HashMap<usize, Placed> = HashMap::new();
                {
                    let mut order: Vec<usize> = (0..evs.len()).filter(|&i| !evs[i].is_rest() && !evs[i].grace && nl.contains_key(&i)).collect();
                    order.sort_by_key(|&i| (evs[i].start, evs[i].voice));
                    for &i in &order {
                        let e = &evs[i];
                        let n = &nl[&i];
                        let steps: Vec<i32> = e.pitches.iter().map(|q| q.step).collect();
                        let xoff = collide::chord_offsets(&steps, n.up, g::notehead_width(sp, e.value) - sp * 0.12);
                        let mut accs = vec![];
                        for (k, pp) in e.pitches.iter().enumerate() {
                            let cur = *acc_state.get(&pp.step).unwrap_or(&key_of(pp.step));
                            let forced = e.courtesy.get(k).copied().unwrap_or(false);
                            if cur != pp.alter || forced {
                                accs.push((k, 0.0, pp.alter, forced));
                            }
                            acc_state.insert(pp.step, pp.alter);
                        }
                        placed.insert(i, Placed { xoff, accs, dot_x: 0.0 });
                    }
                    let mut k0 = 0;
                    while k0 < order.len() {
                        let t = evs[order[k0]].start;
                        let k1 = order[k0..].iter().position(|&i| evs[i].start != t).map(|d| k0 + d).unwrap_or(order.len());
                        let members = &order[k0..k1];
                        let (mut left, mut right) = (f32::MAX, f32::MIN);
                        for &i in members {
                            let w = g::notehead_width(sp, evs[i].value);
                            for &o in &placed[&i].xoff {
                                left = left.min(nl[&i].x + o - w / 2.0);
                                right = right.max(nl[&i].x + o + w / 2.0);
                            }
                        }
                        let mut items = vec![];
                        let mut who = vec![];
                        for &i in members {
                            for (j, &(k, _, alter, forced)) in placed[&i].accs.iter().enumerate() {
                                let (above, below, width) = g::accidental_box(sp, alter);
                                let width = if forced { width * 2.2 } else { width };
                                items.push(collide::Acc { y: nl[&i].ys[k], above, below, width });
                                who.push((i, j));
                            }
                        }
                        let cols = collide::accidental_columns(&items, sp * 0.15, sp * 0.1);
                        let base = left - sp * 0.2;
                        for (q, &(i, j)) in who.iter().enumerate() {
                            if let Some(pl) = placed.get_mut(&i) {
                                pl.accs[j].1 = base + cols[q] - items[q].width / 2.0;
                            }
                        }
                        for &i in members {
                            if let Some(pl) = placed.get_mut(&i) {
                                pl.dot_x = right + sp * 0.45;
                            }
                        }
                        k0 = k1;
                    }
                }
                // draw events
                for (i, e) in evs.iter().enumerate() {
                    if e.hidden && !st.show_hidden {
                        continue;
                    }
                    let r = NoteRef { staff: s, measure: m, idx: i };
                    let dimmed = st.show_voice.map(|v| v != e.voice).unwrap_or(false);
                    let mut c = if st.selected.contains(&r) { SEL } else { col(e.color) };
                    if matches!(st.erase, Some(EraseHover::Event(er, _)) if er == r) {
                        c = ERASE;
                    }
                    if dimmed {
                        c = c.gamma_multiply(0.3);
                    }
                    if e.hidden {
                        c = Color32::from_gray(170);
                    }
                    if e.grace {
                        let x = x_for_tick(mg, e.start, mt) - sp * 1.8 + e.dx * sp;
                        let gs = sp * 0.65;
                        for pp in &e.pitches {
                            let y = mid - (pp.step - mid_step) as f32 * sp * 0.5;
                            g::notehead(p, pos2(x, y), gs, 4, c);
                            ledger(p, x, mid, pp.step - mid_step, gs * 1.4, sp, c);
                            p.line_segment([pos2(x + gs * 0.55, y), pos2(x + gs * 0.55, y - sp * 2.5)], Stroke::new(sp * 0.09, c));
                            g::flag(p, pos2(x + gs * 0.55, y - sp * 2.5), gs, 1, true, c);
                            p.line_segment([pos2(x - gs * 0.2, y - sp * 0.9), pos2(x + gs * 1.5, y - sp * 2.0)], Stroke::new(sp * 0.08, c));
                            out_hits.push(NoteHit { r, pitch: Some(0), rect: Rect::from_center_size(pos2(x, y), vec2(gs * 1.6, gs * 1.2)) });
                        }
                        continue;
                    }
                    let x = x_for_tick(mg, e.start, mt) + e.dx * sp + shift.get(&i).copied().unwrap_or(0.0);
                    if e.is_rest() {
                        let up = if multi { e.voice % 2 == 1 } else { true };
                        let mut y = mid + if multi { if up { -sp * 2.0 } else { sp * 2.0 } } else { 0.0 };
                        // two voices: the rest moves a space at a time until it clears the other voice's notes sounding
                        // with it (F-04)
                        if multi {
                            let (t0, t1) = (e.start, e.end());
                            let others: Vec<f32> = evs
                                .iter()
                                .filter(|o| o.voice != e.voice && !o.grace && o.start < t1 && o.end() > t0)
                                .flat_map(|o| o.pitches.iter().map(|q| mid - (q.step - mid_step) as f32 * sp * 0.5))
                                .collect();
                            for _ in 0..8 {
                                let r = g::rest_rect(pos2(x, y), sp, e.value);
                                let hit = if up {
                                    others.iter().any(|&ny| r.bottom() > ny - sp * 0.75)
                                } else {
                                    others.iter().any(|&ny| r.top() < ny + sp * 0.75)
                                };
                                if !hit {
                                    break;
                                }
                                y += if up { -sp } else { sp };
                            }
                        }
                        g::rest(p, pos2(x, y), sp, e.value, c);
                        for d in 0..e.dots {
                            p.circle_filled(pos2(x + sp * (1.0 + d as f32 * 0.6), y - sp * 0.5), sp * 0.18, c);
                        }
                        out_hits.push(NoteHit { r, pitch: None, rect: Rect::from_center_size(pos2(x, y), vec2(sp * 1.6, sp * 3.0)) });
                        draw_event_extras(p, score, e, x, mid, y, y, true, sp, c, sys.ext_of(s), &mut dyn_rects);
                        continue;
                    }
                    let n = match nl.get(&i) {
                        Some(n) => n,
                        None => continue,
                    };
                    // the chord's seconds and the accidentals, placed above with the other voices at this moment
                    let pl = placed.get(&i);
                    let xoff = pl.map(|q| q.xoff.clone()).unwrap_or_else(|| vec![0.0; e.pitches.len()]);
                    let hw = g::notehead_width(sp, e.value);
                    let mut acc_top = f32::MAX;
                    for &(k, cx, alter, forced) in pl.map(|q| q.accs.as_slice()).unwrap_or(&[]) {
                        let y = n.ys[k];
                        acc_top = acc_top.min(y - g::accidental_height_above(sp, alter));
                        let w = g::accidental_width(sp, alter);
                        g::accidental(p, pos2(cx, y), sp, alter, c);
                        let (above, below, _) = g::accidental_box(sp, alter);
                        ink_log(1, Rect::from_min_max(pos2(cx - w / 2.0, y - above), pos2(cx + w / 2.0, y + below)));
                        if forced && !score.layout.c_score {
                            let pf = page_font(sp * 2.2);
                            p.text(pos2(cx - w * 0.75, y), Align2::CENTER_CENTER, "(", pf.clone(), c);
                            p.text(pos2(cx + w * 0.7, y), Align2::CENTER_CENTER, ")", pf, c);
                        }
                    }
                    let dot_x = pl.map(|q| q.dot_x).unwrap_or(x + hw / 2.0 + sp * 0.45);
                    for (k, pp) in e.pitches.iter().enumerate() {
                        let y = n.ys[k];
                        let nx = x + xoff[k];
                        // one note of a chord that the Eraser would take out alone
                        let hc = if st.erase == Some(EraseHover::Pitch(r, k)) { ERASE } else { c };
                        if staff.drum && false {
                            g::x_notehead(p, pos2(nx, y), sp, hc);
                        } else {
                            g::notehead(p, pos2(nx, y), sp, e.value, hc);
                        }
                        ledger(p, nx, mid, pp.step - mid_step, hw, sp, Color32::BLACK);
                        // dots: in a space (a note on a line puts it in the space above, the lower voice below), in one
                        // column right of every notehead at this moment
                        for d in 0..e.dots {
                            let ds = collide::dot_step(pp.step - mid_step, multi && !n.up);
                            let dc = pos2(dot_x + sp * 0.18 + d as f32 * sp * 0.6, mid - ds as f32 * sp * 0.5);
                            p.circle_filled(dc, sp * 0.18, c);
                            ink_log(2, Rect::from_center_size(dc, vec2(sp * 0.36, sp * 0.36)));
                        }
                        out_hits.push(NoteHit { r, pitch: Some(k), rect: Rect::from_center_size(pos2(nx, y), vec2(hw + 2.0, sp * 1.1)) });
                        // ties
                        if e.ties.get(k).copied().unwrap_or(false) {
                            // a chord's ties curve outwards: the upper notes over the top, the lower ones underneath
                            let natural = if e.pitches.len() > 1 {
                                let rank = e.pitches.iter().filter(|q| q.step < pp.step).count();
                                rank * 2 >= e.pitches.len()
                            } else {
                                !n.up
                            };
                            let above = if e.tie_flip { !natural } else { natural };
                            let (tx, same_sys) = next_event_x(score, sys, s, m, e, pp);
                            let tv = sp * 0.25 * score.layout.tie_v_qs as f32;
                            let yoff = if above { -tv } else { tv };
                            let end_x = if same_sys { tx - hw * 0.6 } else { sys.right - sp * 0.3 };
                            g::tie_or_slur(p, pos2(nx + hw * 0.6, y + yoff), pos2(end_x, y + yoff), sp, above, c);
                        }
                    }
                    // incoming tie from previous system
                    if mg.m == sys.m0 && m > 0 {
                        for pp in &e.pitches {
                            let prev_tied = score.events[s][m - 1].iter().any(|pe| pe.voice == e.voice && pe.end() >= score.measures[m - 1].ticks() && pe.pitches.iter().zip(pe.ties.iter()).any(|(q, t)| *t && q.midi() == pp.midi()));
                            if prev_tied && e.start == 0 {
                                let y = mid - (pp.step - mid_step) as f32 * sp * 0.5;
                                let above = if e.pitches.len() > 1 { e.pitches.iter().filter(|q| q.step < pp.step).count() * 2 >= e.pitches.len() } else { !n.up };
                                let off = if above { -sp * 0.6 } else { sp * 0.6 };
                                g::tie_or_slur(p, pos2(mg.content_x - sp * 1.5, y + off), pos2(x - hw * 0.6, y + off), sp, above, c);
                            }
                        }
                    }
                    // stem
                    if e.value >= 2 {
                        let (ytop, ybot) = n.ys.iter().fold((f32::MAX, f32::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
                        let from = g::stem_attach(pos2(x, if n.up { ybot } else { ytop }), sp, e.value, n.up, sp * 0.12).y;
                        p.line_segment([pos2(n.stem_x, from), pos2(n.stem_x, n.stem_end)], Stroke::new(sp * 0.12, c));
                        if e.value >= 4 && !n.beamed {
                            g::flag(p, pos2(n.stem_x, n.stem_end), sp, e.value - 3, n.up, c);
                        }
                    }
                    let (ytop, ybot) = n.ys.iter().fold((f32::MAX, f32::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
                    draw_marks(p, e, x, ytop, ybot, n.up, n.stem_end, mid, sp, c, acc_top);
                    draw_event_extras(p, score, e, x, mid, ytop, ybot, n.up, sp, c, sys.ext_of(s), &mut dyn_rects);
                }
                // cursor
                if let Some((cs, cm, ct)) = st.cursor {
                    if cs == s && mg.covers(cm) {
                        // F-36: a thin black line the height of the staff, as in Encore (it blinks: see `cursor_on`)
                        let x = if mg.span > 1 { mg.content_x + sp * 0.4 } else { x_for_tick(mg, ct, mt) - sp * 0.9 };
                        p.line_segment([pos2(x, mid - 2.0 * sp), pos2(x, mid + 2.0 * sp)], Stroke::new((sp * 0.1).max(1.0), Color32::BLACK));
                    }
                }
                // ghost note for pencil hover
                if let Some((fs, fm, k)) = st.flash {
                    if fs == s && mg.covers(fm) {
                        let r = Rect::from_min_max(pos2(mg.x0, mid - 3.0 * sp), pos2(mg.x1, mid + 3.0 * sp));
                        p.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(230, 40, 40, (70.0 * k) as u8));
                        p.rect_stroke(r, 2.0, Stroke::new(2.0, Color32::from_rgba_unmultiplied(210, 30, 30, (220.0 * k) as u8)), egui::StrokeKind::Inside);
                    }
                }
                if let Some((loc, value, rest, fits)) = &st.hover_ghost {
                    if loc.staff == s && loc.measure == m {
                        // red where the note wouldn't fit before the bar line
                        let gc = if *fits { Color32::from_rgba_unmultiplied(30, 90, 230, 110) } else { Color32::from_rgba_unmultiplied(220, 30, 30, 170) };
                        let y = mid - loc.pos as f32 * sp * 0.5;
                        if *rest {
                            g::rest(p, pos2(loc.x, mid), sp, *value, gc);
                        } else {
                            g::notehead(p, pos2(loc.x, y), sp, *value, gc);
                            ledger(p, loc.x, mid, loc.pos, g::notehead_width(sp, *value), sp, gc);
                        }
                    }
                }
            }
            // playback position
            if let Some((pm, pt)) = st.play_tick {
                if mg.covers(pm) {
                    // inside a multi-measure rest the line moves across the whole bar
                    let x = if mg.span > 1 {
                        mg.content_x + (mg.x1 - mg.content_x) * (((pm - mg.m) as u32 * mt + pt) as f32 / (mg.span as u32 * mt).max(1) as f32)
                    } else {
                        x_for_tick(mg, pt, mt)
                    };
                    p.rect_filled(
                        Rect::from_min_max(pos2(x - sp * 0.3, sys.top(sp) - sp * 1.5), pos2(x + sp * 0.3, sys.bottom(sp) + sp * 1.5)),
                        0.0,
                        Color32::from_rgba_unmultiplied(40, 160, 60, 90),
                    );
                }
            }
        }
    }

    // graphics
    for (gi, gr) in score.graphics.iter().enumerate() {
        let erase = st.erase == Some(EraseHover::Graphic(gi));
        if let Some(r) = draw_graphic(p, score, geo, gr, st.sel_graphics.contains(&gi), erase, st.show_control_points) {
            out_graphics.push((gi, r));
        }
    }
}

fn beam_quad(p: &Painter, a: Pos2, b: Pos2, thick: f32, up: bool, c: Color32) {
    let d = if up { thick } else { -thick };
    p.add(egui::Shape::convex_polygon(vec![a, b, pos2(b.x, b.y + d), pos2(a.x, a.y + d)], c, Stroke::NONE));
}

fn ledger(p: &Painter, x: f32, mid: f32, rel: i32, hw: f32, sp: f32, c: Color32) {
    let st = Stroke::new((sp * STAFF_LINE * 1.2).max(0.7), c);
    if rel >= 6 {
        let mut k = 6;
        while k <= rel {
            let y = mid - k as f32 * sp * 0.5;
            p.line_segment([pos2(x - hw * 0.8, y), pos2(x + hw * 0.8, y)], st);
            k += 2;
        }
    } else if rel <= -6 {
        let mut k = -6;
        while k >= rel {
            let y = mid - k as f32 * sp * 0.5;
            p.line_segment([pos2(x - hw * 0.8, y), pos2(x + hw * 0.8, y)], st);
            k -= 2;
        }
    }
}

fn next_event_x(score: &Score, sys: &SysGeo, s: usize, m: usize, e: &Event, _pp: &Pitch) -> (f32, bool) {
    // next event in the same voice
    let mg = sys.measures.iter().find(|x| x.covers(m)).unwrap();
    if let Some(n) = score.events[s][m].iter().filter(|x| x.voice == e.voice && x.start > e.start && !x.grace).min_by_key(|x| x.start) {
        return (x_for_tick(mg, n.start, score.measures[m].ticks()), true);
    }
    if m + 1 < sys.m1 {
        let mg2 = sys.measures.iter().find(|x| x.covers(m + 1)).unwrap();
        if let Some(n) = score.events[s][m + 1].iter().filter(|x| x.voice == e.voice && !x.grace).min_by_key(|x| x.start) {
            return (x_for_tick(mg2, n.start, score.measures[m + 1].ticks()), true);
        }
        return (mg2.content_x + geo_sp(mg2), true);
    }
    (sys.right, false)
}

fn geo_sp(mg: &MeasGeo) -> f32 {
    (mg.x1 - mg.content_x) * 0.05
}

/// A clef centred on `at.x`, on the staff whose middle line is `at.y`: the font's glyph on the line it names (G on
/// the 2nd line, F on the 4th, C on the 3rd or the 4th).
pub fn draw_clef(p: &Painter, at: Pos2, sp: f32, clef: Clef, c: Color32) {
    let (name, line) = match clef {
        Clef::Treble => ("gClef", 1.0),
        Clef::Treble8va => ("gClef8va", 1.0),
        Clef::Treble8vb => ("gClef8vb", 1.0),
        Clef::Bass => ("fClef", -1.0),
        Clef::Bass8vb => ("fClef8vb", -1.0),
        Clef::Alto => ("cClef", 0.0),
        Clef::Tenor => ("cClef", -1.0),
        Clef::Percussion => ("unpitchedPercussionClef1", 0.0),
    };
    g::clef(p, name, at.x, at.y + line * sp, sp, c);
}

/// Staff position (half spaces from middle) of the i-th key accidental.
pub fn key_pos(clef: Clef, sign: i8, i: usize) -> i32 {
    // Positions for treble clef; shifted for others.
    const SHARPS: [i32; 7] = [4, 1, 5, 2, -1, 3, 0];
    const FLATS: [i32; 7] = [0, 3, -1, 2, -2, 1, -3];
    let base = if sign > 0 { SHARPS[i % 7] } else { FLATS[i % 7] };
    let shift = match clef {
        Clef::Bass | Clef::Bass8vb => -2,
        Clef::Alto => -1,
        Clef::Tenor => if sign > 0 { -3 + 7 } else { -3 + 7 - 7 + 5 },
        _ => 0,
    };
    let mut v = base + shift;
    if clef == Clef::Tenor && sign > 0 && v > 5 {
        v -= 7;
    }
    v
}

fn draw_barline(p: &Painter, score: &Score, sys: &SysGeo, mg: &MeasGeo, b: Barline, sp: f32, c: Color32) {
    let thin = Stroke::new(sp * 0.13, c);
    let thick = Stroke::new(sp * 0.5, c);
    // segments: connected staves draw through
    let mut segs: Vec<(f32, f32)> = vec![];
    let mut k = 0;
    while k < sys.staves.len() {
        let top = sys.staves[k].1 - 2.0 * sp;
        let mut j = k;
        while j + 1 < sys.staves.len() && (score.staves[sys.staves[j].0].barline_to_next || score.staves[sys.staves[j].0].brace_with_next) {
            j += 1;
        }
        segs.push((top, sys.staves[j].1 + 2.0 * sp));
        k = j + 1;
    }
    let x = mg.x1;
    for (t, bt) in segs {
        match b {
            Barline::Single => {
                p.line_segment([pos2(x, t), pos2(x, bt)], thin);
            }
            Barline::Double => {
                p.line_segment([pos2(x - sp * 0.5, t), pos2(x - sp * 0.5, bt)], thin);
                p.line_segment([pos2(x, t), pos2(x, bt)], thin);
            }
            Barline::Final => {
                p.line_segment([pos2(x - sp * 0.8, t), pos2(x - sp * 0.8, bt)], thin);
                p.line_segment([pos2(x - sp * 0.25, t), pos2(x - sp * 0.25, bt)], thick);
            }
            Barline::RepeatEnd | Barline::RepeatBoth => {
                p.line_segment([pos2(x - sp * 0.8, t), pos2(x - sp * 0.8, bt)], thin);
                p.line_segment([pos2(x - sp * 0.25, t), pos2(x - sp * 0.25, bt)], thick);
                for &(_, mid) in &sys.staves {
                    if mid > t && mid < bt {
                        p.circle_filled(pos2(x - sp * 1.4, mid - sp * 0.5), sp * 0.2, c);
                        p.circle_filled(pos2(x - sp * 1.4, mid + sp * 0.5), sp * 0.2, c);
                    }
                }
            }
            Barline::RepeatStart => {
                p.line_segment([pos2(x, t), pos2(x, bt)], thin);
            }
            Barline::Dashed => g::dashed_line(p, pos2(x, t), pos2(x, bt), sp * 0.13, sp * 0.5, c),
            Barline::Invisible => {}
        }
    }
}

/// `acc_top`: the top of the note's highest accidental (f32::MAX without one): marks above clear it too.
#[allow(clippy::too_many_arguments)]
fn draw_marks(p: &Painter, e: &Event, x: f32, ytop: f32, ybot: f32, up: bool, stem_end: f32, mid: f32, sp: f32, c: Color32, acc_top: f32) {
    // above the stem, the staff and the accidentals (an ornament is wider than the note and would cross them)
    // (a mark's bottom is half a space below the point given, so this leaves half a space above the stem's end, or
    // above the notehead's top)
    let mut above_y = (if up { stem_end.min(ytop) - sp * 1.0 } else { ytop - sp * 1.5 }).min(mid - 3.0 * sp).min(acc_top - sp * 1.3);
    let mut below_y = (if up { ybot } else { stem_end.max(ybot) }).max(mid + 2.0 * sp) + sp * 1.0;
    // a quarter space clear of the notehead (the mark's near edge is half a space past these points)
    let close_above = ytop - sp * 1.25;
    let close_below = ybot + sp * 1.25;
    for &mk in &e.marks {
        use Mark::*;
        match mk {
            Staccato | Staccatissimo | Tenuto | Accent | Marcato => {
                // articulation goes on the notehead side
                if up {
                    g::mark(p, pos2(x, close_below.max(below_y - sp * 1.2).min(close_below + sp * 0.2)), sp, mk, false, c);
                } else {
                    g::mark(p, pos2(x, close_above), sp, mk, true, c);
                }
            }
            FermataBelow | PedalDown | PedalUp => {
                let h = g::mark(p, pos2(x, below_y + sp * 0.5), sp, mk, false, c);
                below_y += h + sp * 0.5;
            }
            Tremolo1 | Tremolo2 | Tremolo3 => {
                let y = if up { stem_end + sp * 1.5 } else { stem_end - sp * 1.5 };
                let sx = if up { x + sp * 0.6 } else { x - sp * 0.6 };
                g::mark(p, pos2(if e.value <= 1 { x } else { sx }, if e.value <= 1 { ytop - sp * 2.0 } else { y }), sp, mk, true, c);
            }
            Breath | Caesura => {
                g::mark(p, pos2(x + sp * 2.0, mid - 2.5 * sp), sp, mk, true, c);
            }
            Arpeggio => {
                g::mark(p, pos2(x - sp * 1.5, (ytop + ybot) / 2.0), sp, mk, true, c);
            }
            _ => {
                let h = g::mark(p, pos2(x, above_y), sp, mk, true, c);
                above_y -= h + sp * 0.5;
            }
        }
    }
}

/// `dyn_rects`: the dynamics already drawn on the page; a dynamic that would touch one goes a row lower.
#[allow(clippy::too_many_arguments)]
fn draw_event_extras(p: &Painter, score: &Score, e: &Event, x: f32, mid: f32, _ytop: f32, _ybot: f32, _up: bool, sp: f32, c: Color32, ext: Ext, dyn_rects: &mut Vec<Rect>) {
    let bottom = mid + 2.0 * sp;
    let top = mid - 2.0 * sp;
    // rows of the system, moved out when its notes reach them (F-05)
    if let Some(d) = &e.dynamic {
        let w = g::dynamic_width(sp, d);
        let mut y = bottom + sp * ext.dyn_y;
        let mut r = Rect::from_center_size(pos2(x, y), vec2(w, sp * 2.0));
        for _ in 0..3 {
            if !dyn_rects.iter().any(|o| o.expand2(vec2(sp * 1.2, 0.0)).intersects(r)) {
                break;
            }
            y += sp * 2.2;
            r = Rect::from_center_size(pos2(x, y), vec2(w, sp * 2.0));
        }
        dyn_rects.push(r);
        dynamic_text(p, pos2(x, y), sp, d, c);
    }
    for (i, l) in e.lyric.iter().enumerate() {
        if !l.is_empty() {
            p.text(pos2(x, bottom + sp * (ext.lyric_y + i as f32 * 2.0)), Align2::CENTER_CENTER, l, page_font(sp * 1.6), c);
        }
    }
    if let Some(ch) = &e.chord_symbol {
        p.text(pos2(x - sp * 0.5, top - sp * ext.chord_y), Align2::LEFT_CENTER, ch, page_font(sp * 1.8), c);
    }
    if let Some(ex) = &e.expression {
        p.text(pos2(x - sp * 0.5, top - sp * ext.expr_y), Align2::LEFT_CENTER, ex, page_font(sp * 1.5), c);
    }
    let _ = score;
}

/// The note of a tempo mark (a quarter, or a dotted quarter), its notehead centred on `c`, at 0.8 of the staff size.
/// Returns its width from `c.x`.
fn draw_tempo(p: &Painter, c: Pos2, sp: f32, dotted: bool, col: Color32) -> f32 {
    let k = sp * 0.8;
    g::notehead(p, c, k, 3, col);
    let w = g::notehead_width(k, 3);
    let stem = g::stem_attach(c, k, 3, true, k * 0.12);
    p.line_segment([stem, pos2(stem.x, c.y - k * 3.2)], Stroke::new(k * 0.12, col));
    if dotted {
        p.circle_filled(pos2(c.x + w * 0.5 + k * 0.55, c.y), k * 0.2, col);
        return w * 0.5 + k * 0.9;
    }
    w * 0.5
}

/// A dynamic centred on `c`, with the font's dynamic letters (F-03).
pub fn dynamic_text(p: &Painter, c: Pos2, sp: f32, t: &str, col: Color32) {
    g::dynamic(p, c, sp, t, col);
}

fn anchor_pos(score: &Score, geo: &Geometry, staff: usize, m: usize, tick: u32) -> Option<(Pos2, usize)> {
    for (si, sys) in geo.systems.iter().enumerate() {
        if m >= sys.m0 && m < sys.m1 {
            let mg = sys.measures.iter().find(|x| x.covers(m))?;
            let y = sys.staff_y(staff).or_else(|| sys.staves.first().map(|x| x.1))? - 2.0 * geo.sp;
            return Some((pos2(x_for_tick(mg, tick, score.measures[m].ticks()), y), si));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn draw_graphic(p: &Painter, score: &Score, geo: &Geometry, gr: &Graphic, selected: bool, erase: bool, ctrl: bool) -> Option<Rect> {
    let sp = geo.sp;
    let (a, sa) = anchor_pos(score, geo, gr.staff, gr.measure, gr.tick)?;
    let a = a + vec2(gr.dx * sp, gr.dy * sp);
    let (mut b, sb) = anchor_pos(score, geo, gr.staff, gr.end_measure.max(gr.measure), gr.end_tick)
        .map(|(b, s)| (b + vec2(gr.dx * sp, gr.end_dy * sp), s))
        .unwrap_or((a + vec2(sp * 6.0, 0.0), sa));
    if sb != sa {
        b = pos2(geo.systems[sa].right - sp, a.y + (gr.end_dy - gr.dy) * sp);
    }
    if (gr.end_measure, gr.end_tick) <= (gr.measure, gr.tick) && !matches!(gr.kind, GraphicKind::Text) {
        b = a + vec2(sp * 6.0, (gr.end_dy - gr.dy) * sp);
    }
    let c = if erase {
        ERASE
    } else if selected {
        SEL
    } else {
        col(gr.color)
    };
    let st = Stroke::new(sp * 0.13, c);
    let rect = match gr.kind {
        GraphicKind::Text => {
            let size = if gr.size > 0.0 { gr.size } else { 1.6 } * sp;
            let r = p.text(a, Align2::LEFT_CENTER, &gr.text, page_font(size), c);
            if gr.bold {
                p.text(a + vec2(sp * 0.05, 0.0), Align2::LEFT_CENTER, &gr.text, page_font(size), c);
            }
            r
        }
        GraphicKind::Line => {
            p.line_segment([a, b], st);
            Rect::from_two_pos(a, b)
        }
        GraphicKind::Rect => {
            let r = Rect::from_two_pos(a, b + vec2(0.0, sp * 3.0));
            p.rect_stroke(r, 0.0, st, egui::StrokeKind::Middle);
            r
        }
        GraphicKind::RoundRect => {
            let r = Rect::from_two_pos(a, b + vec2(0.0, sp * 3.0));
            p.rect_stroke(r, sp, st, egui::StrokeKind::Middle);
            r
        }
        GraphicKind::Ellipse => {
            let r = Rect::from_two_pos(a, b + vec2(0.0, sp * 3.0));
            let pts: Vec<Pos2> = (0..40)
                .map(|i| {
                    let t = i as f32 / 40.0 * std::f32::consts::TAU;
                    pos2(r.center().x + r.width() / 2.0 * t.cos(), r.center().y + r.height() / 2.0 * t.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, st));
            r
        }
        GraphicKind::Slur => {
            g::tie_or_slur(p, a, b, sp, !gr.flip, c);
            Rect::from_two_pos(a, b).expand(sp)
        }
        GraphicKind::Crescendo | GraphicKind::Decrescendo => {
            let cresc = (gr.kind == GraphicKind::Crescendo) != gr.flip;
            g::hairpin(p, a, pos2(b.x, a.y), sp, cresc, c);
            Rect::from_two_pos(a, pos2(b.x, a.y)).expand(sp)
        }
        GraphicKind::Ottava | GraphicKind::OttavaBassa => {
            let txt = if gr.kind == GraphicKind::Ottava { "8va" } else { "8vb" };
            let r = p.text(a, Align2::LEFT_CENTER, txt, page_font(sp * 1.5), c);
            g::dashed_line(p, pos2(r.right() + sp * 0.3, a.y), pos2(b.x, a.y), sp * 0.1, sp * 0.5, c);
            let hook = if gr.kind == GraphicKind::Ottava { sp } else { -sp };
            p.line_segment([pos2(b.x, a.y), pos2(b.x, a.y + hook)], st);
            Rect::from_two_pos(a, pos2(b.x, a.y)).expand(sp)
        }
        GraphicKind::TrillLine => {
            let r = p.text(a, Align2::LEFT_CENTER, "tr", page_font(sp * 1.7), c);
            g::trill_wave(p, pos2(r.right() + sp * 0.3, a.y), pos2(b.x, a.y), sp, c);
            Rect::from_two_pos(a, pos2(b.x, a.y)).expand(sp)
        }
        GraphicKind::Bracket => {
            let hook = if gr.flip { -sp } else { sp };
            p.line_segment([a, pos2(b.x, a.y)], st);
            p.line_segment([a, a + vec2(0.0, hook)], st);
            p.line_segment([pos2(b.x, a.y), pos2(b.x, a.y + hook)], st);
            Rect::from_two_pos(a, pos2(b.x, a.y)).expand(sp)
        }
    };
    if ctrl || selected {
        for q in [a, b] {
            p.rect_filled(Rect::from_center_size(q, vec2(sp * 0.6, sp * 0.6)), 0.0, Color32::from_rgb(230, 120, 0));
        }
    }
    Some(rect)
}

/// Title area in staff spaces from the page's top margin:
/// (subtitle end, first credit line, last text line, height to reserve before the first staff).
pub fn title_block(t: &TextElements) -> (f32, f32, f32, f32) {
    let subs = t.subtitle.lines().filter(|l| !l.trim().is_empty()).count() as f32;
    let sub_end = if subs > 0.0 { 4.5 + (subs - 1.0) * 2.2 + 1.2 } else { 3.0 };
    let credits_y = sub_end + 1.4;
    let right = t.composer.lines().chain(t.lyricist.lines()).filter(|l| !l.trim().is_empty()).count() as f32;
    let left = t.instructions.lines().filter(|l| !l.trim().is_empty()).count() as f32;
    let last = sub_end.max(credits_y + (right.max(left) - 1.0).max(0.0) * 1.8);
    // tempo mark, chord symbols and stems need about seven spaces above the first staff
    (sub_end, credits_y, last, last + 7.5)
}

#[cfg(test)]
mod compressed_rests_tests {
    use super::*;

    /// A one-staff score of `n` empty measures (one system holds them all when compressed).
    fn empty_score(n: usize) -> Score {
        let mut sc = Score::new(vec![Staff::new("Piano", Clef::Treble, 0)], n);
        sc.layout.compress_rests = true;
        sc
    }

    #[test]
    fn eight_empty_measures_are_one_rest_of_8() {
        let sc = empty_score(8);
        assert_eq!(sc.compressed_runs(), vec![(0, 8)]);
        let geo = layout(&sc, Pos2::ZERO, 1.0, false);
        let bars: Vec<(usize, usize)> = geo.systems.iter().flat_map(|s| s.measures.iter().map(|m| (m.m, m.span))).collect();
        assert_eq!(bars, vec![(0, 8)], "one measure standing for 8");
        assert!(geo.systems[0].measures[0].covers(5));
    }

    #[test]
    fn a_key_change_splits_the_rest_in_two() {
        let mut sc = empty_score(8);
        for m in 4..8 {
            sc.measures[m].key = 2;
        }
        assert_eq!(sc.compressed_runs(), vec![(0, 4), (4, 8)]);
    }

    #[test]
    fn marks_and_barlines_break_a_run() {
        let mut sc = empty_score(12);
        sc.measures[3].tempo = Some(90); // tempo change at measure 4
        sc.measures[6].barline = Barline::Double; // double bar after measure 7
        sc.measures[9].rehearsal = Some("A".into()); // rehearsal at measure 10
        assert_eq!(sc.compressed_runs(), vec![(0, 3), (3, 7), (7, 9), (9, 12)]);
    }

    #[test]
    fn a_note_in_any_visible_staff_or_compressed_rests_off_keep_the_measures() {
        let mut sc = empty_score(8);
        sc.events[0][3].push(Event::new(1, 0, 1, vec![])); // a whole rest
        assert_eq!(sc.compressed_runs(), vec![(0, 3), (4, 8)]);
        sc.layout.compress_rests = false;
        assert!(sc.compressed_runs().is_empty());
        let geo = layout(&sc, Pos2::ZERO, 1.0, false);
        assert_eq!(geo.systems.iter().map(|s| s.measures.len()).sum::<usize>(), 8);
    }

    #[test]
    fn playback_and_export_keep_every_measure() {
        let sc = empty_score(8);
        let mut off = sc.clone();
        off.layout.compress_rests = false;
        assert_eq!(crate::io::midi_bytes(&sc), crate::io::midi_bytes(&off));
        assert_eq!(crate::io::musicxml_string(&sc), crate::io::musicxml_string(&off));
        assert_eq!(crate::audio::play_order(&sc).len(), 8);
    }
}

/// F-03: a page with every symbol the score page draws (articulations, dynamics, ornaments, rests of every value,
/// accidentals, flags), to compare the drawing before and after. `cargo test -- --ignored write_symbol_demo`
/// writes it to the temp folder.
#[cfg(test)]
mod symbol_demo {
    use crate::app::App;
    use crate::model::*;

    #[test]
    #[ignore]
    fn write_symbol_demo() {
        let mut a = App::new(true);
        a.score.text.title = "Símbolos da página".into();
        a.score.text.subtitle = "dinâmicas, articulações, pausas e acidentes".into();
        a.score.measures[0].key = 2;
        let n = |m: i32| Some(Pitch::from_midi(m, false));
        let q = TPQ;
        // 1: quarter notes with articulations and dynamics
        for (k, (m, mk, d)) in [(74, Mark::Staccato, Some("p")), (72, Mark::Accent, None), (71, Mark::Tenuto, Some("mf")), (67, Mark::Marcato, Some("ff"))].into_iter().enumerate() {
            let r = a.enter(0, 0, k as u32 * q, n(m), 3, 0, false);
            if let Some(e) = a.ev_mut(r) {
                e.marks.push(mk);
                e.dynamic = d.map(str::to_string);
            }
        }
        // 2: an eighth, two sixteenths, a dotted quarter, staccatissimo, sfz
        let r = a.enter(0, 1, 0, n(76), 4, 0, false);
        if let Some(e) = a.ev_mut(r) {
            e.marks.push(Mark::Staccatissimo);
            e.dynamic = Some("sfz".into());
        }
        a.enter(0, 1, q / 2, n(77), 5, 0, false);
        a.enter(0, 1, q * 3 / 4, n(79), 5, 0, false);
        a.enter(0, 1, q, n(81), 2, 1, false);
        // 3: rests of every value from whole to 32nd... one per bar would be long: half, quarter, eighth, 16th, 32nd
        a.enter(0, 2, 0, None, 2, 0, false);
        a.enter(0, 2, q * 2, None, 3, 0, false);
        a.enter(0, 2, q * 3, None, 4, 0, false);
        a.enter(0, 2, q * 3 + q / 2, None, 5, 0, false);
        a.enter(0, 2, q * 3 + q * 3 / 4, None, 6, 0, false);
        a.enter(0, 2, q * 3 + q * 7 / 8, None, 6, 0, false);
        // 4: accidentals (natural, flat, double sharp, double flat) with ornaments
        let c5 = Pitch::from_midi(72, false).step;
        for (k, (step, alter, mk)) in [(0, 0, Mark::Trill), (1, -1, Mark::Mordent), (2, 2, Mark::Turn), (3, -2, Mark::InvMordent)].into_iter().enumerate() {
            let r = a.enter(0, 3, k as u32 * q, Some(Pitch { step: c5 + step, alter }), 3, 0, false);
            if let Some(e) = a.ev_mut(r) {
                e.marks.push(mk);
            }
        }
        // 5: whole note with a fermata; 6: segno and coda; flags up and down
        let r = a.enter(0, 4, 0, n(72), 1, 0, false);
        if let Some(e) = a.ev_mut(r) {
            e.marks.push(Mark::Fermata);
        }
        for (k, (m, v)) in [(64, 4), (65, 5), (77, 4), (79, 6)].into_iter().enumerate() {
            let r = a.enter(0, 5, k as u32 * q, n(m), v, 0, false);
            if let Some(e) = a.ev_mut(r) {
                e.beam = BeamMode::None;
                if k == 0 {
                    e.marks.push(Mark::Segno);
                }
                if k == 3 {
                    e.marks.push(Mark::Coda);
                }
            }
        }
        // bass staff: whole rest, half notes with dynamics, a double-whole-like whole note
        a.enter(1, 0, 0, None, 1, 0, false);
        let r = a.enter(1, 1, 0, n(43), 2, 0, false);
        if let Some(e) = a.ev_mut(r) {
            e.dynamic = Some("pp".into());
        }
        a.enter(1, 1, q * 2, n(50), 2, 0, false);
        let r = a.enter(1, 4, 0, n(36), 1, 0, false);
        if let Some(e) = a.ev_mut(r) {
            e.marks.push(Mark::FermataBelow);
        }
        let path = std::env::temp_dir().join("simbolos-da-pagina.fmt");
        std::fs::write(&path, crate::io::score_bytes(&a.score).unwrap()).unwrap();
        println!("wrote {}", path.display());
    }
}

/// F-04: an original four-part chorale (soprano and alto on the treble staff, tenor and bass on the bass staff) with a
/// unison, seconds between the voices, dotted notes, accidentals, and a chord of four notes with three accidentals.
#[cfg(test)]
pub(crate) fn satb_chorale() -> Score {
    use crate::app::App;
    let mut a = App::new(true);
    a.score.text.title = "Coral a quatro vozes".into();
    a.score.text.subtitle = "exemplo original do time Fermata".into();
    for m in a.score.measures.iter_mut() {
        m.key = 1;
    }
    // (staff, voice, bars of (midi notes, quarters)); a chord is several notes in one step
    type Bar = &'static [(&'static [i32], u32)];
    let parts: [(usize, u8, [Bar; 8]); 4] = [
        (0, 1, [&[(&[71], 1), (&[71], 1), (&[69], 1), (&[67], 1)], &[(&[69], 2), (&[71], 1), (&[72], 1)], &[(&[74], 1), (&[72], 1), (&[71], 1), (&[69], 1)], &[(&[67], 3), (&[67], 1)], &[(&[71], 1), (&[69], 1), (&[68], 1), (&[69], 1)], &[(&[74], 1), (&[72], 1), (&[71], 1), (&[69], 1)], &[(&[67], 1), (&[69], 1), (&[71], 2)], &[(&[67], 4)]]),
        (0, 2, [&[(&[67], 1), (&[67], 1), (&[66], 1), (&[62], 1)], &[(&[66], 2), (&[69], 1), (&[69], 1)], &[(&[67], 1), (&[72], 1), (&[67], 1), (&[66], 1)], &[(&[62], 3), (&[62], 1)], &[(&[64], 1), (&[64], 1), (&[64], 1), (&[64], 1)], &[(&[66], 1), (&[66], 1), (&[67], 1), (&[66], 1)], &[(&[62], 1), (&[66], 1), (&[67], 2)], &[(&[62], 4)]]),
        (1, 1, [&[(&[62], 1), (&[62], 1), (&[62], 1), (&[59], 1)], &[(&[62], 2), (&[62], 1), (&[64], 1)], &[(&[59], 1), (&[64], 1), (&[62], 1), (&[62], 1)], &[(&[59], 3), (&[59], 1)], &[(&[59], 1), (&[61], 1), (&[59], 1), (&[57], 1)], &[(&[62], 1), (&[60], 1), (&[62], 1), (&[60], 1)], &[(&[59], 1), (&[60], 1), (&[62], 2)], &[(&[59], 4)]]),
        (1, 2, [&[(&[55], 1), (&[55], 1), (&[50], 1), (&[55], 1)], &[(&[50], 2), (&[55], 1), (&[57], 1)], &[(&[55], 1), (&[57], 1), (&[50], 1), (&[50], 1)], &[(&[43], 3), (&[43], 1)], &[(&[56], 1), (&[57], 1), (&[52], 1), (&[45], 1)], &[(&[50], 1), (&[45], 1), (&[43], 1), (&[50], 1)], &[(&[47], 1), (&[48], 1), (&[43], 2)], &[(&[43], 4)]]),
    ];
    let value_of = |q: u32| -> (u8, u8) {
        match q {
            4 => (1, 0),
            3 => (2, 1),
            2 => (2, 0),
            _ => (3, 0),
        }
    };
    while a.score.measures.len() < 9 {
        a.score.measures.push(a.score.measures[0].clone());
        for st in a.score.events.iter_mut() {
            st.push(vec![]);
        }
    }
    for (staff, voice, bars) in parts {
        a.voice = voice;
        for (m, bar) in bars.iter().enumerate() {
            let mut t = 0;
            for (notes, q) in bar.iter() {
                let (v, d) = value_of(*q);
                for &n in notes.iter() {
                    a.enter(staff, m, t, Some(Pitch::from_midi(n, false)), v, d, false);
                }
                t += q * TPQ;
            }
        }
    }
    // bar 9: C sharp, E flat, F natural (the key has F sharp) and A in one chord: a second inside and three
    // accidentals; then a rest
    a.voice = 1;
    let c5 = Pitch::from_midi(72, false).step;
    for (st, al) in [(0, 1), (2, -1), (3, 0), (5, 0)] {
        a.enter(0, 8, 0, Some(Pitch { step: c5 + st, alter: al }), 2, 1, false);
    }
    a.enter(0, 8, TPQ * 3, None, 3, 0, false);
    a.score
}

#[cfg(test)]
mod collisions {
    use super::*;

    /// Lays out and draws a score, returning the noteheads (with their event) and the accidentals and dots drawn.
    fn ink(score: &Score) -> (Vec<(NoteRef, Rect)>, Vec<(u8, Rect)>) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut heads = vec![];
        INK.with(|v| v.borrow_mut().clear());
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
            let geo = layout(score, pos2(0.0, 0.0), 1.35, false);
            let sel = HashSet::new();
            let selg = HashSet::new();
            let st = DrawState {
                selected: &sel,
                sel_graphics: &selg,
                cursor: None,
                play_tick: None,
                show_voice: None,
                show_control_points: false,
                show_hidden: false,
                sel_measures: None,
                hover_ghost: None,
                flash: None,
        erase: None,
            };
            let mut hits = vec![];
            let mut gh = vec![];
            draw(&p, score, &geo, &st, &mut hits, &mut gh);
            heads = hits.into_iter().filter(|h| h.pitch.is_some()).map(|h| (h.r, h.rect)).collect();
        });
        (heads, INK.with(|v| v.borrow().clone()))
    }

    #[test]
    fn a_four_part_chorale_has_no_collisions() {
        let score = satb_chorale();
        let (heads, other) = ink(&score);
        assert!(heads.len() > 100, "the chorale has its notes ({})", heads.len());
        // noteheads of different notes never overlap, except a unison sharing one head (same place)
        for (i, (ra, a)) in heads.iter().enumerate() {
            for (rb, b) in heads.iter().skip(i + 1) {
                if ra == rb {
                    continue;
                }
                let same_place = (a.center() - b.center()).length() < 0.5;
                assert!(same_place || !a.shrink(1.5).intersects(b.shrink(1.5)), "heads of {ra:?} and {rb:?} overlap");
            }
        }
        let accs: Vec<Rect> = other.iter().filter(|o| o.0 == 1).map(|o| o.1).collect();
        let dots: Vec<Rect> = other.iter().filter(|o| o.0 == 2).map(|o| o.1).collect();
        assert!(accs.len() >= 6 && dots.len() >= 4, "{} accidentals, {} dots", accs.len(), dots.len());
        for a in &accs {
            for (r, h) in &heads {
                assert!(!a.shrink(0.5).intersects(h.shrink(1.5)), "an accidental touches the head of {r:?}");
            }
        }
        for (i, a) in accs.iter().enumerate() {
            for b in accs.iter().skip(i + 1) {
                assert!(!a.shrink(0.5).intersects(b.shrink(0.5)), "two accidentals touch: {a:?} {b:?}");
            }
        }
        for d in &dots {
            for (r, h) in &heads {
                assert!(!d.intersects(h.shrink(1.5)), "a dot touches the head of {r:?}");
            }
        }
    }

    #[test]
    #[ignore]
    fn write_chorale() {
        let path = std::env::temp_dir().join("coral-a-quatro-vozes.fmt");
        std::fs::write(&path, crate::io::score_bytes(&satb_chorale()).unwrap()).unwrap();
        println!("wrote {}", path.display());
    }
}

/// F-05 test scores: a piano page with high and low ledger notes, chord symbols, dynamics, lyrics and a tempo mark;
/// and a 6/8 tune that starts with a pickup and counts its tempo in dotted quarters. Both are original.
#[cfg(test)]
pub(crate) mod page_demo {
    use super::*;

    pub fn piano_page() -> Score {
        let mut treble = Staff::new("Piano", Clef::Treble, 0);
        treble.brace_with_next = true;
        let bass = Staff::new("", Clef::Bass, 0);
        let mut sc = Score::new(vec![treble, bass], 16);
        sc.layout.measures_per_system = 4;
        sc.text.title = "Página de piano".into();
        sc.text.subtitle = "exemplo original do time Fermata".into();
        sc.text.composer = "Fermata Dev".into();
        sc.text.instructions = "Andante".into();
        sc.measures[0].tempo = Some(84);
        sc.measures[8].rehearsal = Some("A".into());
        let chords = ["C", "Am", "F", "G7"];
        for m in 0..16 {
            // system 2: the right hand climbs to the ledger lines above; system 3: the left hand goes below; system 4
            // has chord symbols
            let sys = m / 4;
            for b in 0..4u32 {
                let top = match sys {
                    1 => [81, 84, 83, 86][b as usize],
                    _ => [72, 74, 76, 77][b as usize],
                };
                let mut e = Event::new(1, b * TPQ, 3, vec![Pitch::from_midi(top - if top % 12 == 5 || top % 12 == 11 { 6 } else { 7 }, false), Pitch::from_midi(top, false)]);
                if b == 0 && sys == 3 {
                    e.chord_symbol = Some(chords[m % 4].into());
                }
                if b == 0 && m % 4 == 0 {
                    e.dynamic = Some(["p", "mf", "f", "mp"][sys].into());
                }
                sc.events[0][m].push(e);
                let low = match sys {
                    2 => [33, 35, 33, 31][b as usize],
                    _ => [48, 47, 45, 43][b as usize],
                };
                sc.events[1][m].push(Event::new(1, b * TPQ, 3, vec![Pitch::from_midi(low, false)]));
            }
        }
        sc
    }

    /// A jig in 6/8 (original): an eighth-note pickup, then full bars; "dotted quarter = 116".
    pub fn pickup_jig() -> Score {
        let mut sc = Score::new(vec![Staff::new("Flauta", Clef::Treble, 73)], 9);
        sc.text.title = "Giga da anacruse".into();
        sc.text.subtitle = "exemplo original do time Fermata".into();
        for m in sc.measures.iter_mut() {
            m.num = 6;
            m.den = 8;
            m.key = 1;
        }
        sc.measures[0].pickup = Some(TPQ / 2);
        sc.measures[0].tempo = Some(174);
        sc.measures[0].tempo_dotted = true;
        let e8 = TPQ / 2;
        sc.events[0][0].push(Event::new(1, 0, 4, vec![Pitch::from_midi(62, false)]));
        let tune: [[i32; 6]; 8] = [
            [67, 71, 74, 72, 71, 69],
            [67, 69, 71, 69, 67, 66],
            [64, 66, 67, 69, 71, 72],
            [74, 72, 71, 69, 67, 66],
            [67, 71, 74, 76, 74, 72],
            [71, 72, 74, 72, 71, 69],
            [67, 64, 66, 67, 69, 66],
            [67, 67, 67, 67, 67, 67],
        ];
        for (k, bar) in tune.iter().enumerate() {
            let m = k + 1;
            if m == 8 {
                sc.events[0][m].push(Event::new(1, 0, 2, vec![Pitch::from_midi(67, false)]));
                sc.events[0][m].last_mut().unwrap().dots = 1;
                continue;
            }
            for (i, &n) in bar.iter().enumerate() {
                sc.events[0][m].push(Event::new(1, i as u32 * e8, 4, vec![Pitch::from_midi(n, false)]));
            }
        }
        sc.measures[8].barline = Barline::Final;
        sc
    }

    /// `cargo test -- --ignored write_page_demos` writes both scores to the temp folder.
    #[test]
    #[ignore]
    fn write_page_demos() {
        for (name, sc) in [("pagina-de-piano.fmt", piano_page()), ("giga-da-anacruse.fmt", pickup_jig())] {
            let path = std::env::temp_dir().join(name);
            std::fs::write(&path, crate::io::score_bytes(&sc).unwrap()).unwrap();
            println!("wrote {}", path.display());
        }
    }
}

#[cfg(test)]
mod page_layout_tests {
    use super::page_demo::*;
    use super::*;

    fn page(sc: &mut Score, w: f32, h: f32) -> Geometry {
        sc.layout.page_w = w;
        sc.layout.page_h = h;
        layout(sc, Pos2::ZERO, 1.0, false)
    }

    #[test]
    fn a_four_system_piano_page_fits_letter_and_a4() {
        let corpus = || -> Score {
            let mut sc: Score = serde_json::from_slice(include_bytes!("../tests/corpus/piano.fmt")).unwrap();
            sc.text.title = "Piano".into();
            sc
        };
        for (w, h, name, mut sc) in [
            (612.0, 792.0, "Letter", piano_page()),
            (595.3, 841.9, "A4", piano_page()),
            (612.0, 792.0, "Letter (QA piano.fmt)", corpus()),
            (595.3, 841.9, "A4 (QA piano.fmt)", corpus()),
        ] {
            let geo = page(&mut sc, w, h);
            let on_first = geo.systems.iter().filter(|s| s.page == 0).count();
            assert!(on_first >= 4, "{name}: {on_first} systems on page 1");
            // nothing of the last system leaves the page (its content may reach two spaces into the margin)
            let sp = geo.sp;
            for s in geo.systems.iter().filter(|s| s.page == 0) {
                let low = s.bottom(sp) + s.ext.last().unwrap().below * sp;
                assert!(low <= geo.pages[0].bottom() - sc.layout.margins()[1] + sp * 2.0 + 0.5, "{name}: system {} runs off the page", s.m0);
            }
        }
    }

    #[test]
    fn the_title_block_is_clear_of_the_first_system() {
        let mut sc = piano_page();
        let geo = page(&mut sc, 612.0, 792.0);
        let sp = geo.sp;
        let tb = title_block(&sc.text);
        let text_bottom = geo.pages[0].top() + sc.layout.margins()[0] + sp * (tb.2 + 0.9);
        let s0 = &geo.systems[0];
        let reach = s0.top(sp) - s0.ext[0].above * sp;
        assert!(reach >= text_bottom + sp, "the first system reaches {reach}, the title block ends at {text_bottom}");
        // the tempo mark sits above the high notes and chord symbols of the first staff
        assert!(s0.marks.tempo > s0.ext[0].notes_above + 1.0);
        // and over chord symbols: the system with chords puts its tempo row above them
        let mut sc2 = piano_page();
        sc2.measures[12].tempo = Some(72);
        let g2 = page(&mut sc2, 612.0, 792.0);
        let s3 = g2.systems.iter().find(|s| s.m0 == 12).unwrap();
        assert!(s3.marks.tempo > s3.ext[0].chord_y + 1.0, "{:?} {:?}", s3.marks, s3.ext[0]);
    }

    #[test]
    fn systems_and_staves_keep_their_content_apart() {
        let mut sc = piano_page();
        let geo = page(&mut sc, 612.0, 792.0);
        let sp = geo.sp;
        for s in &geo.systems {
            for k in 0..s.staves.len() - 1 {
                let low = s.staves[k].1 + 2.0 * sp + s.ext[k].below * sp;
                let high = s.staves[k + 1].1 - 2.0 * sp - s.ext[k + 1].above * sp;
                assert!(high - low >= sp * 0.99, "staves {k} and {} of system {} touch", k + 1, s.m0);
            }
        }
        for w in geo.systems.windows(2) {
            if w[0].page != w[1].page {
                continue;
            }
            let low = w[0].bottom(sp) + w[0].ext.last().unwrap().below * sp;
            let high = w[1].top(sp) - w[1].ext[0].above * sp;
            assert!(high - low >= sp * 1.49, "systems {} and {} touch", w[0].m0, w[1].m0);
        }
    }

    #[test]
    fn ledger_notes_widen_the_gap_between_the_staves() {
        let mut sc = piano_page();
        let base = sc.layout.staff_gap;
        // the right hand far below its staff and the left hand far above its own
        sc.events[0][0][0].pitches = vec![Pitch::from_midi(48, false)];
        sc.events[1][0][0].pitches = vec![Pitch::from_midi(72, false)];
        let geo = page(&mut sc, 612.0, 792.0);
        let sp = geo.sp;
        let s0 = &geo.systems[0];
        let gap = (s0.staves[1].1 - s0.staves[0].1) / sp - 4.0;
        assert!(gap > base + 1.0, "gap {gap} spaces, Page Setup says {base}");
        let s1 = &geo.systems[1];
        let gap1 = (s1.staves[1].1 - s1.staves[0].1) / sp - 4.0;
        assert!((gap1 - base).abs() < 0.01, "a system without such notes keeps the Page Setup gap");
    }

    #[test]
    fn a_pickup_shows_its_signature_counts_as_measure_0_and_plays_its_length() {
        let sc = pickup_jig();
        let mi = &sc.measures[0];
        assert_eq!((mi.num, mi.den), (6, 8), "the real time signature");
        assert_eq!(mi.ticks(), TPQ / 2);
        assert_eq!(mi.full_ticks(), TPQ * 3);
        assert_eq!(sc.measure_label(0), 0);
        assert_eq!(sc.measure_label(1), 1);
        assert_eq!(sc.measure_from_label(1), 1);
        assert_eq!(sc.measure_from_label(0), 0);
        // playback: the first full bar starts one eighth after the start
        let (notes, _, _) = crate::audio::build(&sc, 0);
        let spt = 60.0 / 174.0 / TPQ as f64;
        let first_bar = notes.iter().map(|n| n.at).filter(|&t| t > 1e-6).fold(f64::MAX, f64::min);
        assert!((first_bar - spt * (TPQ / 2) as f64).abs() < 1e-6, "bar 1 starts at {first_bar}");
        // the pickup is narrow: it holds one eighth, not a full 6/8 bar
        let geo = layout(&sc, Pos2::ZERO, 1.0, false);
        let m0 = &geo.systems[0].measures[0];
        let m1 = &geo.systems[0].measures[1];
        assert!(m0.x1 - m0.content_x < (m1.x1 - m1.content_x) / 2.0);
        // without a pickup, the measures count from 1 again
        let mut full = sc.clone();
        full.measures[0].pickup = None;
        assert_eq!(full.measure_label(0), 1);
        assert_eq!(full.measures[0].ticks(), TPQ * 3);
    }

    #[test]
    fn a_dotted_quarter_tempo_is_shown_as_given() {
        let sc = pickup_jig();
        assert_eq!(sc.tempo_at(0), 174, "playback counts quarters");
        assert_eq!(sc.measures[0].tempo_shown(), Some(116), "the mark shows dotted quarter = 116");
        let xml = crate::io::musicxml_string(&sc);
        assert!(xml.contains("<beat-unit>quarter</beat-unit><beat-unit-dot/><per-minute>116</per-minute>"));
        assert!(xml.contains("<measure number=\"0\" implicit=\"yes\">"));
    }

    #[test]
    fn short_signatures_for_pickups() {
        assert_eq!(crate::model::short_signature(TPQ / 2), (1, 8));
        assert_eq!(crate::model::short_signature(TPQ), (1, 4));
        assert_eq!(crate::model::short_signature(TPQ * 3 / 2), (3, 8));
        assert_eq!(crate::model::short_signature(TPQ / 4), (1, 16));
    }

    #[test]
    fn the_file_keeps_pickup_and_dotted_tempo_and_old_files_still_open() {
        let sc = pickup_jig();
        let back: Score = serde_json::from_slice(&crate::io::score_bytes(&sc).unwrap()).unwrap();
        assert_eq!(back.measures[0].pickup, Some(TPQ / 2));
        assert!(back.measures[0].tempo_dotted);
        // a measure without them writes neither field, so files of earlier versions read the same
        let j = serde_json::to_string(&back.measures[1]).unwrap();
        assert!(!j.contains("pickup") && !j.contains("tempo_dotted"));
    }
}
