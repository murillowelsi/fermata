//! Page layout, drawing and hit-testing of the score.
use crate::glyphs as g;
use crate::model::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke};
use std::collections::{BTreeSet, HashMap, HashSet};

pub const SEL: Color32 = Color32::from_rgb(30, 90, 230);
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
}

impl SysGeo {
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
    if out.is_empty() {
        x = sp * 8.0;
    }
    (out, x.max(sp * 5.0))
}

pub fn layout(score: &Score, origin: Pos2, zoom: f32, linear: bool) -> Geometry {
    let sp = score.layout.staff_space * zoom;
    let page_w = score.layout.page_w * zoom;
    let page_h = score.layout.page_h * zoom;
    let margin = score.layout.margin * zoom;
    let staves = visible_staves(score);
    let staff_h = 4.0 * sp;
    let gap = score.layout.staff_gap * sp;
    let sys_gap = score.layout.system_gap * sp;
    let sys_h = staves.len() as f32 * staff_h + (staves.len() as f32 - 1.0).max(0.0) * gap;
    let name_indent = {
        let longest = staves.iter().map(|&s| score.staves[s].name.chars().count()).max().unwrap_or(0) as f32;
        (longest * sp * 0.75 + sp * 2.0).min(sp * 14.0)
    };
    let short_indent = if score.staves.len() > 1 { sp * 4.0 } else { sp * 1.0 };

    let mut geo = Geometry { sp, ..Default::default() };
    let system_ranges = if linear { vec![(0, score.measures.len())] } else { score.systems() };
    let spp = score.layout.systems_per_page.max(1);
    // the title area grows with the subtitle and credit lines, so they never reach the first staff
    let title_h = sp * title_block(&score.text).3.max(12.0);

    let mut page_idx = 0usize;
    let mut sys_on_page = 0usize;
    let mut page_cap = spp;
    let mut sys_space = sys_gap;
    let page_gap = sp * 5.0;
    let mut cur_y = 0.0f32;

    let page_rect = |i: usize| -> Rect {
        let y = origin.y + i as f32 * (page_h + page_gap);
        Rect::from_min_size(pos2(origin.x, y), vec2(page_w, page_h))
    };

    for (si, &(m0, m1)) in system_ranges.iter().enumerate() {
        let first = si == 0;
        let (left, right, top_mid);
        if linear {
            left = origin.x + margin + name_indent;
            right = f32::MAX;
            top_mid = origin.y + margin + title_h * 0.4 + 2.0 * sp;
        } else {
            // Start a new page when the page holds its systems-per-page, or the next system wouldn't fit.
            let min_gap = sp * 4.0;
            let need_new = sys_on_page > 0 && (sys_on_page >= page_cap || cur_y + sys_h > page_rect(page_idx).bottom() - margin);
            if need_new {
                page_idx += 1;
                sys_on_page = 0;
            }
            if sys_on_page == 0 {
                let pr = page_rect(page_idx);
                geo.pages.push(pr);
                cur_y = pr.top() + margin + if page_idx == 0 { title_h } else { sp * 4.0 };
                // how many systems this page takes, and an even gap between them
                let usable = (pr.bottom() - margin - cur_y).max(sys_h);
                let fit = (((usable + min_gap) / (sys_h + min_gap)).floor() as usize).max(1);
                page_cap = spp.min(fit).min(system_ranges.len() - si).max(1);
                sys_space = if page_cap > 1 {
                    ((usable - page_cap as f32 * sys_h) / (page_cap - 1) as f32).clamp(min_gap, sys_gap + sp * 10.0)
                } else {
                    sys_gap
                };
            }
            let pr = page_rect(page_idx);
            left = pr.left() + margin + if first { name_indent } else { short_indent };
            right = pr.right() - margin;
            top_mid = cur_y + 2.0 * sp;
            cur_y += sys_h + sys_space;
            sys_on_page += 1;
        }
        let mut staves_y = vec![];
        for (k, &s) in staves.iter().enumerate() {
            staves_y.push((s, top_mid + k as f32 * (staff_h + gap)));
        }
        // natural widths
        let mut nat = vec![];
        let mut total = 0.0;
        for m in m0..m1 {
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
            let (slots, w) = measure_slots(score, m, &staves, sp);
            total += h + w;
            nat.push((m, h, slots, w, hdr_clef, hdr_key, hdr_time));
        }
        let avail = if linear { total } else { right - left };
        let last_system = si + 1 == system_ranges.len();
        let stretch = if linear || (last_system && total < avail * 0.6) { 1.0 } else { (avail / total.max(1.0)).max(0.3) };
        let mut x = left;
        let mut measures = vec![];
        for (m, h, slots, w, hc, hk, ht) in nat {
            let x0 = x;
            let content_x = x0 + h;
            let k = stretch;
            let slots2: Vec<(u32, f32)> = slots.iter().map(|(t, sx)| (*t, content_x + sx * k)).collect();
            let x1 = content_x + w * k;
            measures.push(MeasGeo { m, x0, content_x, x1, slots: slots2, header_clef: hc, header_key: hk, header_time: ht });
            x = x1;
        }
        let right = if linear { x } else { right.min(x).max(x) };
        geo.systems.push(SysGeo { page: page_idx, m0, m1, staves: staves_y, left, right, measures, first });
    }
    if linear {
        let r = geo.systems.last().map(|s| s.right).unwrap_or(origin.x + page_w);
        let b = geo.systems.last().map(|s| s.bottom(sp)).unwrap_or(origin.y + page_h);
        let pr = Rect::from_min_max(origin, pos2(r + margin, (b + sp * 12.0).max(origin.y + page_h * 0.5)));
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
            // interpolate
            let mut lo = (0u32, mg.content_x);
            let mut hi = (mt, mg.x1);
            for &(t, x) in &mg.slots {
                if x <= p.x {
                    lo = (t, x);
                } else {
                    hi = (t, x);
                    break;
                }
            }
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

pub fn draw(p: &Painter, score: &Score, geo: &Geometry, st: &DrawState, out_hits: &mut Vec<NoteHit>, out_graphics: &mut Vec<(usize, Rect)>) {
    let sp = geo.sp;
    let black = Color32::BLACK;
    let line_st = Stroke::new((sp * STAFF_LINE).max(0.6), Color32::from_gray(20));
    let font = |s: f32| FontId::proportional(s);

    for (i, pr) in geo.pages.iter().enumerate() {
        p.rect_filled(pr.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(60));
        p.rect_filled(*pr, 0.0, Color32::WHITE);
        let t = &score.text;
        if i == 0 {
            let cx = pr.center().x;
            let top = pr.top() + score.layout.margin * sp / score.layout.staff_space;
            p.text(pos2(cx, top + sp * 1.5), Align2::CENTER_CENTER, &t.title, font(sp * 3.0), black);
            for (k, line) in t.subtitle.lines().enumerate() {
                p.text(pos2(cx, top + sp * (4.5 + k as f32 * 2.2)), Align2::CENTER_CENTER, line, font(sp * 1.8), black);
            }
            let (_, credits_y, _, _) = title_block(t);
            if !t.composer.is_empty() {
                // right-aligned credits under the title, one line per credit
                for (k, line) in t.composer.lines().enumerate() {
                    p.text(pos2(pr.right() - sp * 7.0, top + sp * (credits_y + k as f32 * 1.8)), Align2::RIGHT_CENTER, line, font(sp * 1.4), black);
                }
            }
            // left-hand texts stacked from the same line as the credits
            let mut ly = credits_y;
            for line in t.lyricist.lines().chain(t.instructions.lines()) {
                if !line.trim().is_empty() {
                    p.text(pos2(pr.left() + sp * 7.0, top + sp * ly), Align2::LEFT_CENTER, line, font(sp * 1.4), black);
                    ly += 1.8;
                }
            }
            if !t.copyright.is_empty() {
                p.text(pos2(cx, pr.bottom() - sp * 3.0), Align2::CENTER_CENTER, &t.copyright, font(sp * 1.2), black);
            }
        }
        // header: "#P" stands for the page number (the original prints it at the top right, page 1 included)
        let page_in_header = t.header.contains("#P");
        if !t.header.is_empty() {
            let txt = t.header.replace("#P", &format!("{}", i + 1));
            if page_in_header {
                p.text(pos2(pr.right() - sp * 5.0, pr.top() + sp * 4.0), Align2::RIGHT_CENTER, txt, font(sp * 1.5), black);
            } else if i > 0 {
                p.text(pos2(pr.center().x, pr.top() + sp * 3.0), Align2::CENTER_CENTER, txt, font(sp * 1.3), Color32::DARK_GRAY);
            }
        }
        if !t.footer.is_empty() {
            p.text(pos2(pr.center().x, pr.bottom() - sp * 1.5), Align2::CENTER_CENTER, &t.footer, font(sp * 1.2), Color32::DARK_GRAY);
        }
        if t.show_page_numbers && i > 0 && !page_in_header {
            p.text(pos2(pr.right() - sp * 5.0, pr.top() + sp * 3.0), Align2::RIGHT_CENTER, format!("{}", i + 1), font(sp * 1.6), black);
        }
    }

    for sys in &geo.systems {
        // selection of measures
        if let Some((a, b)) = st.sel_measures {
            for mg in &sys.measures {
                if mg.m >= a && mg.m <= b {
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
                        if key > 0 { g::sharp(p, c, sp * 0.9, black) } else { g::flat(p, c, sp * 0.9, black) }
                    }
                    x += key_width(sp, key);
                }
                if mg.header_time {
                    g::time_sig_digit(p, pos2(x + sp * 1.2, mid - sp), sp, &mi.num.to_string(), black);
                    g::time_sig_digit(p, pos2(x + sp * 1.2, mid + sp), sp, &mi.den.to_string(), black);
                }
            }
            // barline at end
            draw_barline(p, score, sys, mg, mi.barline, sp, black);
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
            let show_num = match score.layout.show_measure_numbers {
                MeasureNumbers::None => false,
                MeasureNumbers::EverySystem => m == sys.m0 && m > 0,
                MeasureNumbers::EveryMeasure => true,
                MeasureNumbers::Every5 => (m + 1) % 5 == 0,
            };
            if show_num {
                p.text(pos2(mg.x0 + sp * 0.2, sys.top(sp) - sp * 1.6), Align2::LEFT_CENTER, format!("{}", m + 1), font(sp * 1.2), black);
            }
            // tempo
            if let Some(t) = mi.tempo {
                // above the chord symbols when the bar has any
                let chords = score.events.iter().any(|st| st.get(m).map(|v| v.iter().any(|e| e.chord_symbol.is_some())).unwrap_or(false));
                let y = sys.top(sp) - if chords { sp * 5.8 } else { sp * 3.6 };
                let x = mg.content_x;
                g::notehead(p, pos2(x, y), sp * 0.8, 3, black);
                p.line_segment([pos2(x + sp * 0.45, y), pos2(x + sp * 0.45, y - sp * 2.4)], Stroke::new(sp * 0.1, black));
                p.text(pos2(x + sp * 1.2, y), Align2::LEFT_CENTER, format!("= {t}"), font(sp * 1.5), black);
            }
            if let Some(r) = &mi.rehearsal {
                let rr = p.text(pos2(mg.x0, sys.top(sp) - sp * 5.5), Align2::LEFT_CENTER, r, font(sp * 1.8), black);
                p.rect_stroke(rr.expand(sp * 0.3), 0.0, Stroke::new(sp * 0.1, black), egui::StrokeKind::Outside);
            }
            if let Some(e) = &mi.ending {
                let y = sys.top(sp) - sp * 2.8;
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
                        let x = x_for_tick(mg, e.start, mt) + e.dx * sp;
                        let ys: Vec<f32> = e.pitches.iter().map(|pp| mid - (pp.step - mid_step) as f32 * sp * 0.5).collect();
                        let up = in_group.get(&i).and_then(|gi| group_up.get(gi).copied()).unwrap_or_else(|| auto_stem_up(e, mid_step, multi));
                        let hw = g::notehead_width(sp, e.value) / 2.0;
                        let stem_x = if up { x + hw - sp * 0.06 } else { x - hw + sp * 0.06 };
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
                // draw events
                for (i, e) in evs.iter().enumerate() {
                    if e.hidden && !st.show_hidden {
                        continue;
                    }
                    let r = NoteRef { staff: s, measure: m, idx: i };
                    let dimmed = st.show_voice.map(|v| v != e.voice).unwrap_or(false);
                    let mut c = if st.selected.contains(&r) { SEL } else { col(e.color) };
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
                    let x = x_for_tick(mg, e.start, mt) + e.dx * sp;
                    if e.is_rest() {
                        let up = if multi { e.voice % 2 == 1 } else { true };
                        let y = mid + if multi { if up { -sp * 2.0 } else { sp * 2.0 } } else { 0.0 };
                        g::rest(p, pos2(x, y), sp, e.value, c);
                        for d in 0..e.dots {
                            p.circle_filled(pos2(x + sp * (1.0 + d as f32 * 0.6), y - sp * 0.5), sp * 0.18, c);
                        }
                        out_hits.push(NoteHit { r, pitch: None, rect: Rect::from_center_size(pos2(x, y), vec2(sp * 1.6, sp * 3.0)) });
                        draw_event_extras(p, score, e, x, mid, y, y, true, sp, c, s, m);
                        continue;
                    }
                    let n = match nl.get(&i) {
                        Some(n) => n,
                        None => continue,
                    };
                    // seconds displacement
                    let mut order: Vec<usize> = (0..e.pitches.len()).collect();
                    order.sort_by_key(|&k| e.pitches[k].step);
                    let mut xoff = vec![0.0f32; e.pitches.len()];
                    let hw = g::notehead_width(sp, e.value);
                    for w in 1..order.len() {
                        let (a, b) = (order[w - 1], order[w]);
                        if e.pitches[b].step - e.pitches[a].step == 1 && xoff[a] == 0.0 {
                            xoff[b] = if n.up { hw - sp * 0.12 } else { -(hw - sp * 0.12) };
                        }
                    }
                    // accidentals
                    let mut acc_x = x - hw / 2.0 - sp * 0.3;
                    let mut acc_list = vec![];
                    for (k, pp) in e.pitches.iter().enumerate() {
                        let cur = *acc_state.get(&pp.step).unwrap_or(&key_of(pp.step));
                        let forced = e.courtesy.get(k).copied().unwrap_or(false);
                        if cur != pp.alter || forced {
                            acc_list.push((k, pp.alter, forced));
                        }
                        acc_state.insert(pp.step, pp.alter);
                    }
                    acc_list.sort_by_key(|a| -e.pitches[a.0].step);
                    let mut last_y = f32::MIN;
                    for (k, alter, forced) in acc_list {
                        let y = n.ys[k];
                        if (y - last_y).abs() < sp * 2.5 && last_y != f32::MIN {
                            acc_x -= sp * 1.1;
                        } else {
                            acc_x = x - hw / 2.0 - sp * 0.3 + xoff[k].min(0.0);
                        }
                        let w = g::accidental_width(sp, alter);
                        let cx = acc_x - w / 2.0;
                        g::accidental(p, pos2(cx, y), sp, alter, c);
                        if forced && score.layout.c_score == false {
                            let pf = FontId::proportional(sp * 2.2);
                            p.text(pos2(cx - w * 0.75, y), Align2::CENTER_CENTER, "(", pf.clone(), c);
                            p.text(pos2(cx + w * 0.7, y), Align2::CENTER_CENTER, ")", pf, c);
                        }
                        last_y = y;
                    }
                    for (k, pp) in e.pitches.iter().enumerate() {
                        let y = n.ys[k];
                        let nx = x + xoff[k];
                        if staff.drum && false {
                            g::x_notehead(p, pos2(nx, y), sp, c);
                        } else {
                            g::notehead(p, pos2(nx, y), sp, e.value, c);
                        }
                        ledger(p, nx, mid, pp.step - mid_step, hw, sp, Color32::BLACK);
                        // dots
                        for d in 0..e.dots {
                            let rel = pp.step - mid_step;
                            let dy = if rel % 2 == 0 { -sp * 0.5 } else { 0.0 };
                            p.circle_filled(pos2(x + hw / 2.0 + sp * (0.6 + d as f32 * 0.6) + xoff[k].max(0.0), y + dy), sp * 0.18, c);
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
                        let from = if n.up { ybot } else { ytop };
                        p.line_segment([pos2(n.stem_x, from), pos2(n.stem_x, n.stem_end)], Stroke::new(sp * 0.12, c));
                        if e.value >= 4 && !n.beamed {
                            g::flag(p, pos2(n.stem_x, n.stem_end), sp, e.value - 3, n.up, c);
                        }
                    }
                    let (ytop, ybot) = n.ys.iter().fold((f32::MAX, f32::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
                    draw_marks(p, e, x, ytop, ybot, n.up, n.stem_end, mid, sp, c);
                    draw_event_extras(p, score, e, x, mid, ytop, ybot, n.up, sp, c, s, m);
                }
                // cursor
                if let Some((cs, cm, ct)) = st.cursor {
                    if cs == s && cm == m {
                        let x = x_for_tick(mg, ct, mt) - sp * 0.9;
                        p.line_segment([pos2(x, mid - 3.5 * sp), pos2(x, mid + 3.5 * sp)], Stroke::new(sp * 0.18, Color32::from_rgb(220, 40, 40)));
                        p.add(egui::Shape::convex_polygon(
                            vec![pos2(x - sp * 0.6, mid - 4.3 * sp), pos2(x + sp * 0.6, mid - 4.3 * sp), pos2(x, mid - 3.4 * sp)],
                            Color32::from_rgb(220, 40, 40),
                            Stroke::NONE,
                        ));
                    }
                }
                // ghost note for pencil hover
                if let Some((fs, fm, k)) = st.flash {
                    if fs == s && fm == m {
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
                if pm == m {
                    let x = x_for_tick(mg, pt, mt);
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
        if let Some(r) = draw_graphic(p, score, geo, gr, st.sel_graphics.contains(&gi), st.show_control_points) {
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
    let mg = sys.measures.iter().find(|x| x.m == m).unwrap();
    if let Some(n) = score.events[s][m].iter().filter(|x| x.voice == e.voice && x.start > e.start && !x.grace).min_by_key(|x| x.start) {
        return (x_for_tick(mg, n.start, score.measures[m].ticks()), true);
    }
    if m + 1 < sys.m1 {
        let mg2 = sys.measures.iter().find(|x| x.m == m + 1).unwrap();
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

pub fn draw_clef(p: &Painter, at: Pos2, sp: f32, clef: Clef, c: Color32) {
    match clef {
        Clef::Treble => g::treble_clef(p, at + vec2(0.0, sp), sp, c),
        Clef::Treble8va => {
            g::treble_clef(p, at + vec2(0.0, sp), sp, c);
            g::small_8(p, at + vec2(0.0, -sp * 4.2), sp * 0.8, c);
        }
        Clef::Treble8vb => {
            g::treble_clef(p, at + vec2(0.0, sp), sp, c);
            g::small_8(p, at + vec2(-sp * 0.3, sp * 5.0), sp * 0.8, c);
        }
        Clef::Bass => g::bass_clef(p, at + vec2(-sp * 0.3, -sp), sp, c),
        Clef::Bass8vb => {
            g::bass_clef(p, at + vec2(-sp * 0.3, -sp), sp, c);
            g::small_8(p, at + vec2(0.0, sp * 3.2), sp * 0.8, c);
        }
        Clef::Alto => g::c_clef(p, at, sp, c),
        Clef::Tenor => g::c_clef(p, at + vec2(0.0, -sp), sp, c),
        Clef::Percussion => g::percussion_clef(p, at, sp, c),
    }
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

fn draw_marks(p: &Painter, e: &Event, x: f32, ytop: f32, ybot: f32, up: bool, stem_end: f32, mid: f32, sp: f32, c: Color32) {
    let mut above_y = (if up { stem_end.min(ytop) } else { ytop }).min(mid - 2.0 * sp) - sp * 1.0;
    let mut below_y = (if up { ybot } else { stem_end.max(ybot) }).max(mid + 2.0 * sp) + sp * 1.0;
    let close_above = ytop - sp * 1.0;
    let close_below = ybot + sp * 1.0;
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
                g::mark(p, pos2(x, below_y + sp * 0.5), sp, mk, false, c);
                below_y += sp * 2.0;
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
                g::mark(p, pos2(x, above_y), sp, mk, true, c);
                above_y -= sp * 2.0;
            }
        }
    }
}

fn draw_event_extras(p: &Painter, score: &Score, e: &Event, x: f32, mid: f32, _ytop: f32, _ybot: f32, _up: bool, sp: f32, c: Color32, _s: usize, _m: usize) {
    let bottom = mid + 2.0 * sp;
    let top = mid - 2.0 * sp;
    if let Some(d) = &e.dynamic {
        dynamic_text(p, pos2(x, bottom + sp * 3.0), sp, d, c);
    }
    for (i, l) in e.lyric.iter().enumerate() {
        if !l.is_empty() {
            p.text(pos2(x, bottom + sp * (5.5 + i as f32 * 2.0)), Align2::CENTER_CENTER, l, FontId::proportional(sp * 1.6), c);
        }
    }
    if let Some(ch) = &e.chord_symbol {
        p.text(pos2(x - sp * 0.5, top - sp * 2.5), Align2::LEFT_CENTER, ch, FontId::proportional(sp * 1.8), c);
    }
    if let Some(ex) = &e.expression {
        p.text(pos2(x - sp * 0.5, top - sp * 4.6), Align2::LEFT_CENTER, ex, FontId::proportional(sp * 1.5), c);
    }
    let _ = score;
}

pub fn dynamic_text(p: &Painter, c: Pos2, sp: f32, t: &str, col: Color32) {
    // Bold-italic look made by shearing three passes (no italic font is bundled).
    let f = FontId::proportional(sp * 2.0);
    for (dx, dy) in [(0.0, 0.0), (sp * 0.06, 0.0), (sp * 0.03, sp * 0.03)] {
        p.text(c + vec2(dx, dy), Align2::CENTER_CENTER, t, f.clone(), col);
    }
}

fn anchor_pos(score: &Score, geo: &Geometry, staff: usize, m: usize, tick: u32) -> Option<(Pos2, usize)> {
    for (si, sys) in geo.systems.iter().enumerate() {
        if m >= sys.m0 && m < sys.m1 {
            let mg = sys.measures.iter().find(|x| x.m == m)?;
            let y = sys.staff_y(staff).or_else(|| sys.staves.first().map(|x| x.1))? - 2.0 * geo.sp;
            return Some((pos2(x_for_tick(mg, tick, score.measures[m].ticks()), y), si));
        }
    }
    None
}

pub fn draw_graphic(p: &Painter, score: &Score, geo: &Geometry, gr: &Graphic, selected: bool, ctrl: bool) -> Option<Rect> {
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
    let c = if selected { SEL } else { col(gr.color) };
    let st = Stroke::new(sp * 0.13, c);
    let rect = match gr.kind {
        GraphicKind::Text => {
            let size = if gr.size > 0.0 { gr.size } else { 1.6 } * sp;
            let r = p.text(a, Align2::LEFT_CENTER, &gr.text, FontId::proportional(size), c);
            if gr.bold {
                p.text(a + vec2(sp * 0.05, 0.0), Align2::LEFT_CENTER, &gr.text, FontId::proportional(size), c);
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
            let r = p.text(a, Align2::LEFT_CENTER, txt, FontId::proportional(sp * 1.5), c);
            g::dashed_line(p, pos2(r.right() + sp * 0.3, a.y), pos2(b.x, a.y), sp * 0.1, sp * 0.5, c);
            let hook = if gr.kind == GraphicKind::Ottava { sp } else { -sp };
            p.line_segment([pos2(b.x, a.y), pos2(b.x, a.y + hook)], st);
            Rect::from_two_pos(a, pos2(b.x, a.y)).expand(sp)
        }
        GraphicKind::TrillLine => {
            let r = p.text(a, Align2::LEFT_CENTER, "tr", FontId::proportional(sp * 1.7), c);
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
    let right = t.composer.lines().filter(|l| !l.trim().is_empty()).count() as f32;
    let left = t.lyricist.lines().chain(t.instructions.lines()).filter(|l| !l.trim().is_empty()).count() as f32;
    let last = sub_end.max(credits_y + (right.max(left) - 1.0).max(0.0) * 1.8);
    // tempo mark, chord symbols and stems need about seven spaces above the first staff
    (sub_end, credits_y, last, last + 7.5)
}
