//! F-40: write everything, transcribe at the end. With the Pencil and the pen the ink stays on the page (kept in
//! score units, so it follows scrolling and zoom); "Transcribe (N)" reads it all at once:
//! 1. the strokes are grouped into symbols by overlap and closeness (a stem takes the head near its end, even a
//!    space apart; dots stay alone), not by the pause between them;
//! 2. each symbol is read by the trained model and the rules together (`pen_model::ensemble`; beamed groups,
//!    chords and accidentals drawn with their note by the rules), placed on its staff and bar;
//! 3. dots go to the note before them, accidentals to the note after them, heads in one vertical become a chord;
//! 4. in each bar, among close readings, the ones whose durations fill the bar are preferred;
//! 5. a preview shows what was read over the faded ink, the doubtful ones marked; a tap offers the other readings
//!    (a pick teaches the writer's models); Accept writes everything as one undo step, Cancel goes back to the ink.
use crate::app::App;
use crate::model::*;
use crate::pen::{self, Col, Ctx, Reading};
use crate::pen_model;
use crate::pen_ui::{Feedback, FeedbackKind, INK, INK_WIDTH};
use crate::score_view as sv;
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, Pos2, Rect, Sense, Stroke as EStroke, Ui};

/// One stroke of ink waiting to be transcribed, in score units (screen points at zoom 1 from the first page).
#[derive(Clone, Debug, PartialEq)]
pub struct InkStroke {
    pub pts: Vec<Pos2>,
}

/// One read symbol in the preview.
#[derive(Clone, Debug)]
pub struct Item {
    pub strokes: Vec<usize>,
    /// its ink's box, in score units
    pub rect: Rect,
    pub staff: usize,
    pub measure: usize,
    pub x: f32,
    /// the readings, best first, with their probability; empty = not music (nothing is written)
    pub cands: Vec<(Reading, f32)>,
    pub chosen: usize,
    /// the writer picked it: the bar's fit doesn't change it
    pub locked: bool,
}

impl Item {
    pub fn reading(&self) -> Option<&Reading> {
        self.cands.get(self.chosen).map(|c| &c.0)
    }
    pub fn uncertain(&self) -> bool {
        if self.cands.is_empty() || self.locked {
            return false;
        }
        let p = self.cands[self.chosen].1;
        let second = self.cands.iter().enumerate().filter(|(i, _)| *i != self.chosen).map(|(_, c)| c.1).fold(0.0, f32::max);
        p < 0.55 || p - second < 0.25
    }
}

#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub items: Vec<Item>,
    /// the item whose alternatives are open
    pub open: Option<usize>,
}

/// Ticks of a reading (a beamed group: all its notes).
fn ticks_of(r: &Reading) -> u32 {
    match r {
        Reading::Notes { cols, .. } => cols.iter().map(|c| dur_ticks(c.value, c.dots, None)).sum(),
        Reading::Rest { value, dots, .. } => dur_ticks(*value, *dots, None),
        _ => 0,
    }
}

fn with_dots(r: &Reading, add: u8) -> Reading {
    match r {
        Reading::Notes { cols, beamed } => Reading::Notes { cols: cols.iter().map(|c| Col { dots: (c.dots + add).min(2), ..c.clone() }).collect(), beamed: *beamed },
        Reading::Rest { value, dots, at } => Reading::Rest { value: *value, dots: (dots + add).min(2), at: *at },
        r => r.clone(),
    }
}

fn ink_rect(pts: &[Pos2]) -> Rect {
    let mut r = Rect::NOTHING;
    for p in pts {
        r.extend_with(*p);
    }
    r
}

/// Where the ink's head is, whatever it looks like: the heavier end of the ink (a stem leaves its head at one end).
fn head_guess(strokes: &[Vec<Pos2>]) -> Pos2 {
    let all: Vec<Pos2> = strokes.iter().flatten().copied().collect();
    let r = ink_rect(&all);
    if r.height() < 1.6 * r.width().max(1e-3) {
        return r.center();
    }
    let band = r.height() * 0.3;
    let (mut top, mut bot) = (vec![], vec![]);
    for s in strokes {
        let rs = pen::resample(s, ((pen::path_len(s) / 0.5).max(2.0) as usize).min(300));
        for p in rs {
            if p.y <= r.top() + band {
                top.push(p);
            } else if p.y >= r.bottom() - band {
                bot.push(p);
            }
        }
    }
    let pick = if bot.len() >= top.len() { bot } else { top };
    if pick.is_empty() {
        return r.center();
    }
    let n = pick.len() as f32;
    (pick.iter().fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2()) / n).to_pos2()
}

/// Groups strokes (screen points) into symbols: strokes that overlap or come within half a space, a stem and what
/// sits at its end (up to 1.2 spaces away), and strokes drawn one after the other within a space. Dots stay alone.
pub fn group(strokes: &[Vec<Pos2>], sp: f32) -> Vec<Vec<usize>> {
    let n = strokes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let rects: Vec<Rect> = strokes.iter().map(|s| ink_rect(s)).collect();
    let tiny = |i: usize| rects[i].width().max(rects[i].height()) <= 0.45 * sp && pen::path_len(&strokes[i]) <= 1.2 * sp;
    let stem_like = |i: usize| {
        let s = &strokes[i];
        let r = rects[i];
        let l = pen::path_len(s);
        r.height() >= 1.5 * sp && r.width() <= 0.45 * r.height() && l > 0.0 && s[0].distance(*s.last().unwrap()) / l >= 0.85
    };
    let min_dist = |a: usize, b: usize| {
        let (ra, rb) = (rects[a].expand(0.6 * sp), rects[b]);
        if !ra.intersects(rb) {
            return f32::MAX;
        }
        let sb = pen::resample(&strokes[b], 48);
        let sa = pen::resample(&strokes[a], 48);
        sa.iter().map(|p| sb.iter().map(|q| p.distance(*q)).fold(f32::MAX, f32::min)).fold(f32::MAX, f32::min)
    };
    for a in 0..n {
        for b in (a + 1)..n {
            if tiny(a) || tiny(b) {
                continue;
            }
            let shrunk = |r: Rect| r.shrink(0.05 * sp);
            let overlap = shrunk(rects[a]).intersects(shrunk(rects[b])) && min_dist(a, b) <= 0.8 * sp;
            let close = min_dist(a, b) <= 0.5 * sp;
            let next = b == a + 1 && min_dist(a, b) <= 1.0 * sp;
            let stem_end = |s: usize, o: usize| {
                if !stem_like(s) || stem_like(o) {
                    return false;
                }
                let st = &strokes[s];
                let ends = [st[0], *st.last().unwrap()];
                let c = rects[o].center();
                ends.iter().any(|e| (c.y - e.y).abs() <= 1.2 * sp && (c.x - e.x).abs() <= 1.6 * sp) && min_dist(s, o) <= 1.2 * sp
            };
            if overlap || close || next || stem_end(a, b) || stem_end(b, a) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                parent[ra] = rb;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().collect();
    out.sort_by(|a, b| rects[a[0]].left().partial_cmp(&rects[b[0]].left()).unwrap());
    out
}

/// Picks one reading per item so the bar fills: the sum of log probabilities, plus a bonus when the durations
/// make exactly the time left, minus a penalty when they run over. `items` in bar order; returns the choices.
pub fn fit_bar(items: &[Vec<(u32, f32)>], room: u32) -> Vec<usize> {
    let mut states: Vec<(u32, f32, Vec<usize>)> = vec![(0, 0.0, vec![])];
    for opts in items {
        let mut next: Vec<(u32, f32, Vec<usize>)> = vec![];
        for (t, s, ch) in &states {
            for (k, &(d, p)) in opts.iter().enumerate() {
                let mut c = ch.clone();
                c.push(k);
                next.push((t + d, s + p.max(1e-4).ln(), c));
            }
        }
        // keep the best few per total
        next.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let mut seen: std::collections::HashMap<u32, usize> = Default::default();
        next.retain(|x| {
            let e = seen.entry(x.0).or_insert(0);
            *e += 1;
            *e <= 2
        });
        next.truncate(300);
        states = next;
    }
    let value = |t: u32, s: f32| s + if t == room { 4f32.ln() } else if t > room { -(30f32.ln()) } else { 0.0 };
    states.into_iter().max_by(|a, b| value(a.0, a.1).partial_cmp(&value(b.0, b.1)).unwrap()).map(|x| x.2).unwrap_or_default()
}

impl App {
    fn ink_origin(&self) -> Pos2 {
        self.geo.pages.first().map(|r| r.min).unwrap_or(Pos2::ZERO)
    }

    pub fn ink_to_screen(&self, p: Pos2) -> Pos2 {
        self.ink_origin() + p.to_vec2() * self.zoom
    }

    fn ink_from_screen(&self, p: Pos2) -> Pos2 {
        ((p - self.ink_origin()) / self.zoom.max(1e-3)).to_pos2()
    }

    /// The ink's strokes on screen now.
    pub fn ink_screen(&self) -> Vec<Vec<Pos2>> {
        self.pen.ink.iter().map(|s| s.pts.iter().map(|p| self.ink_to_screen(*p)).collect()).collect()
    }

    /// A finished stroke in the "transcribe at the end" mode: a scribble deletes what it covers (notes, or ink);
    /// anything else stays as ink.
    pub fn pen_batch_stroke(&mut self, pts: Vec<Pos2>, now: f64) {
        let sp = self.geo.sp;
        if pts.len() < 2 && self.pen.ink.is_empty() && pts.is_empty() {
            return;
        }
        if let Some(r) = pen::scribble_rect(std::slice::from_ref(&pts), sp) {
            let (hits, gs) = self.pen_covered_pub(r, std::slice::from_ref(&pts));
            if !hits.is_empty() || !gs.is_empty() {
                self.pen_delete_pub(&hits, &gs);
                self.pen.feedback.push(Feedback { strokes: vec![pts], kind: FeedbackKind::Recognized { head: r.center() }, start: now });
                self.status = "Scribbled out. Two-finger tap undoes it.".into();
                return;
            }
            // over ink: that ink goes
            let ink = self.ink_screen();
            let er = r.expand(0.2 * sp);
            let gone: Vec<usize> = (0..ink.len()).filter(|&i| ink[i].iter().any(|p| er.contains(*p)) && ink[i].iter().filter(|p| er.contains(**p)).count() * 2 >= ink[i].len()).collect();
            if !gone.is_empty() {
                for &i in gone.iter().rev() {
                    self.pen.ink.remove(i);
                }
                self.pen.preview = None;
                self.pen.feedback.push(Feedback { strokes: vec![pts], kind: FeedbackKind::Vanish, start: now });
                self.status = format!("Erased {} ink {}.", gone.len(), if gone.len() == 1 { "stroke" } else { "strokes" });
                return;
            }
            // a big scribble over nothing: it goes (a quarter rest zigzags too: that one stays)
            let rest = pen::recognize(std::slice::from_ref(&pts), sp, &Ctx::default()).first().is_some_and(|c| c.score >= 0.5 && matches!(c.reading, Reading::Rest { value: 3..=7, .. }));
            if r.width().max(r.height()) > 2.2 * sp && !rest {
                self.pen.feedback.push(Feedback { strokes: vec![pts], kind: FeedbackKind::Vanish, start: now });
                self.status = "A scribble over nothing: nothing changed.".into();
                return;
            }
        }
        let s = InkStroke { pts: pts.iter().map(|p| self.ink_from_screen(*p)).collect() };
        self.pen.ink.push(s);
        self.pen.preview = None;
    }

    /// How many symbols the ink makes now.
    pub fn ink_symbols(&self) -> usize {
        if self.pen.ink.is_empty() {
            return 0;
        }
        group(&self.ink_screen(), self.geo.sp).len()
    }

    pub fn pen_clear_ink(&mut self) {
        self.pen.ink.clear();
        self.pen.preview = None;
        self.status = "Ink cleared.".into();
    }

    /// Reads one group of strokes (screen points): its readings with probabilities, best first.
    fn read_group(&self, strokes: &[Vec<Pos2>]) -> Vec<(Reading, f32)> {
        let sp = self.geo.sp;
        let r = ink_rect(&strokes.concat());
        let mid = self.pen_staff_mid(r.center());
        let personal = &self.pen.personal;
        let rules = pen::recognize(strokes, sp, &Ctx { staff_mid: mid, remaining: None, neighbor: None, personal: Some(personal) });
        // groups, chords and accidentals drawn with their note: the rules' structure
        let structured = rules.first().is_some_and(|c| matches!(&c.reading, Reading::Notes { cols, .. } if cols.len() > 1 || cols[0].heads.len() > 1 || cols[0].acc.is_some()));
        if structured {
            let s: f32 = rules.iter().map(|c| c.score).sum::<f32>().max(1e-4);
            return rules.into_iter().take(pen::ALTERNATIVES_MAX).map(|c| (c.reading, c.score / s)).collect();
        }
        // one symbol: the model and the rules together
        let s_sp = pen::to_sp(strokes, sp);
        let p = pen_model::ensemble(&s_sp, &Ctx { staff_mid: mid.map(|m| m / sp), remaining: None, neighbor: None, personal: Some(personal) });
        let mut order: Vec<usize> = (0..p.len()).collect();
        order.sort_by(|&a, &b| p[b].partial_cmp(&p[a]).unwrap());
        if order.first() == Some(&pen_model::OTHER) && p[pen_model::OTHER] > 0.6 {
            return vec![];
        }
        let head = |v: u8| -> Pos2 {
            rules.iter().find_map(|c| match &c.reading {
                Reading::Notes { cols, .. } if cols[0].value == v => cols[0].heads.first().copied(),
                _ => None,
            })
            .or_else(|| rules.iter().find_map(|c| match &c.reading {
                Reading::Notes { cols, .. } => cols[0].heads.first().copied(),
                _ => None,
            }))
            .unwrap_or_else(|| head_guess(strokes))
        };
        let mut out = vec![];
        for k in order.into_iter().take(6) {
            let pk = p[k];
            if pk < 0.01 && !out.is_empty() {
                break;
            }
            let reading = match k {
                0..=6 => Reading::Notes { cols: vec![Col { heads: vec![head(k as u8 + 1)], value: k as u8 + 1, dots: 0, acc: None, acc_head: 0 }], beamed: false },
                7 => {
                    // the block: hanging from the 4th line = whole rest, sitting on the 3rd = half rest
                    let whole = mid.is_some_and(|m| r.center().y < m - 0.5 * sp);
                    let (a, b) = if whole { (1, 2) } else { (2, 1) };
                    out.push((Reading::Rest { value: a, dots: 0, at: r.center() }, pk * 0.8));
                    Reading::Rest { value: b, dots: 0, at: r.center() }
                }
                8..=12 => Reading::Rest { value: (k - 5) as u8, dots: 0, at: r.center() },
                13..=17 => Reading::Accidental { alter: [1, -1, 0, 2, -2][k - 13], at: r.center(), right: r.right() },
                pen_model::DOT => Reading::Dot { at: r.center() },
                _ => continue,
            };
            let w = if k == 7 { pk * 0.2 } else { pk };
            out.push((reading, w));
        }
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        out
    }

    fn pen_staff_mid(&self, p: Pos2) -> Option<f32> {
        let sp = self.geo.sp;
        let sys = self.geo.systems.iter().find(|s| p.x >= s.left - sp * 2.0 && p.x <= s.right + sp && p.y >= s.top(sp) - sp * 8.0 && p.y <= s.bottom(sp) + sp * 8.0)?;
        sys.staves.iter().map(|s| s.1).min_by(|a, b| (a - p.y).abs().partial_cmp(&(b - p.y).abs()).unwrap())
    }

    /// Transcribe: reads all the ink and opens the preview.
    pub fn pen_transcribe(&mut self) {
        self.pen_load();
        let sp = self.geo.sp;
        let ink = self.ink_screen();
        if ink.is_empty() {
            self.status = "No ink to transcribe: write with the pen first.".into();
            return;
        }
        let groups = group(&ink, sp);
        let mut items: Vec<Item> = vec![];
        for g in groups {
            let strokes: Vec<Vec<Pos2>> = g.iter().map(|&i| ink[i].clone()).collect();
            let r = ink_rect(&strokes.concat());
            let cands = self.read_group(&strokes);
            let anchor = cands.first().map(|c| c.0.anchor()).unwrap_or(r.center());
            let (staff, measure) = sv::locate(&self.score, &self.geo, anchor, self.grid()).map(|l| (l.staff, l.measure)).unwrap_or((usize::MAX, usize::MAX));
            let sr = Rect::from_min_max(self.ink_from_screen(r.min), self.ink_from_screen(r.max));
            let cands = if staff == usize::MAX { vec![] } else { cands };
            items.push(Item { strokes: g, rect: sr, staff, measure, x: anchor.x, cands, chosen: 0, locked: false });
        }
        // dots go to the note before them; accidentals to the note after them; heads in one vertical make a chord
        let is = |it: &Item, f: &dyn Fn(&Reading) -> bool| it.reading().is_some_and(f);
        let n = items.len();
        let mut drop = vec![false; n];
        for i in 0..n {
            if is(&items[i], &|r| matches!(r, Reading::Dot { .. })) {
                let at = items[i].x;
                let target = (0..n).filter(|&j| j != i && !drop[j] && items[j].staff == items[i].staff && items[j].measure == items[i].measure && items[j].x < at && at - items[j].x <= 3.0 * sp && is(&items[j], &|r| matches!(r, Reading::Notes { .. } | Reading::Rest { .. }))).max_by(|&a, &b| items[a].x.partial_cmp(&items[b].x).unwrap());
                if let Some(j) = target {
                    let strokes = items[i].strokes.clone();
                    let rect = items[i].rect;
                    let it = &mut items[j];
                    it.cands = it.cands.iter().map(|(r, p)| (with_dots(r, 1), *p)).collect();
                    it.strokes.extend(strokes);
                    it.rect = it.rect.union(rect);
                    drop[i] = true;
                }
            }
        }
        for i in 0..n {
            if drop[i] {
                continue;
            }
            let alter = match items[i].reading() {
                Some(Reading::Accidental { alter, .. }) => *alter,
                _ => continue,
            };
            let (at, y) = (items[i].x, self.ink_to_screen(items[i].rect.center()).y);
            let target = (0..n).filter(|&j| j != i && !drop[j] && items[j].staff == items[i].staff && items[j].x > at && items[j].x - at <= 3.5 * sp && is(&items[j], &|r| matches!(r, Reading::Notes { .. }))).min_by(|&a, &b| items[a].x.partial_cmp(&items[b].x).unwrap());
            if let Some(j) = target {
                let strokes = items[i].strokes.clone();
                let rect = items[i].rect;
                let it = &mut items[j];
                it.cands = it.cands.iter().map(|(r, p)| {
                    let r = match r {
                        Reading::Notes { cols, beamed } => {
                            let mut cols = cols.clone();
                            let k = (0..cols[0].heads.len()).min_by(|&a, &b| (cols[0].heads[a].y - y).abs().partial_cmp(&(cols[0].heads[b].y - y).abs()).unwrap()).unwrap_or(0);
                            cols[0].acc = Some(alter);
                            cols[0].acc_head = k;
                            Reading::Notes { cols, beamed: *beamed }
                        }
                        r => r.clone(),
                    };
                    (r, *p)
                }).collect();
                it.strokes.extend(strokes);
                it.rect = it.rect.union(rect);
                drop[i] = true;
            }
        }
        for i in 0..n {
            if drop[i] || !is(&items[i], &|r| matches!(r, Reading::Notes { cols, .. } if cols.len() == 1)) {
                continue;
            }
            for j in (i + 1)..n {
                if drop[j] || items[j].staff != items[i].staff || items[j].measure != items[i].measure || (items[j].x - items[i].x).abs() > 0.9 * sp || !is(&items[j], &|r| matches!(r, Reading::Notes { cols, .. } if cols.len() == 1)) {
                    continue;
                }
                let heads: Vec<Pos2> = match items[j].reading() {
                    Some(Reading::Notes { cols, .. }) => cols[0].heads.clone(),
                    _ => vec![],
                };
                let strokes = items[j].strokes.clone();
                let rect = items[j].rect;
                let it = &mut items[i];
                it.cands = it.cands.iter().map(|(r, p)| match r {
                    Reading::Notes { cols, beamed } => {
                        let mut cols = cols.clone();
                        cols[0].heads.extend(heads.iter().copied());
                        (Reading::Notes { cols, beamed: *beamed }, *p)
                    }
                    r => (r.clone(), *p),
                }).collect();
                it.strokes.extend(strokes);
                it.rect = it.rect.union(rect);
                drop[j] = true;
            }
        }
        let items: Vec<Item> = items.into_iter().zip(drop).filter(|(_, d)| !d).map(|(i, _)| i).collect();
        self.pen.preview = Some(Preview { items, open: None });
        self.pen_fit_bars();
        let p = self.pen.preview.as_ref().unwrap();
        let unsure = p.items.iter().filter(|i| i.uncertain()).count();
        let n = p.items.iter().filter(|i| !i.cands.is_empty()).count();
        self.status = format!("Read {n} symbols{}. Accept writes them; tap one to change it.", if unsure > 0 { format!(", {unsure} to check") } else { String::new() });
    }

    /// Where the voice's free time starts in a bar (after its last note), and the bar's length.
    fn bar_room(&self, staff: usize, measure: usize) -> (u32, u32) {
        let mt = self.score.measures[measure].ticks();
        let voice = self.voice;
        let start = self.score.events[staff][measure].iter().filter(|e| e.voice == voice && !e.grace).map(|e| e.end()).max().unwrap_or(0);
        (start.min(mt), mt)
    }

    /// In each bar, the readings whose durations fill it (among close ones).
    pub fn pen_fit_bars(&mut self) {
        let Some(mut p) = self.pen.preview.take() else { return };
        let mut bars: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
        for (i, it) in p.items.iter().enumerate() {
            if it.staff != usize::MAX && it.cands.iter().any(|c| ticks_of(&c.0) > 0) {
                bars.entry((it.staff, it.measure)).or_default().push(i);
            }
        }
        for ((s, m), mut idx) in bars {
            idx.sort_by(|&a, &b| p.items[a].x.partial_cmp(&p.items[b].x).unwrap());
            let (start, mt) = self.bar_room(s, m);
            let opts: Vec<Vec<(u32, f32)>> = idx
                .iter()
                .map(|&i| {
                    let it = &p.items[i];
                    if it.locked {
                        vec![(ticks_of(&it.cands[it.chosen].0), 1.0)]
                    } else {
                        it.cands.iter().take(4).map(|c| (ticks_of(&c.0), c.1)).collect()
                    }
                })
                .collect();
            let choice = fit_bar(&opts, mt - start);
            for (k, &i) in idx.iter().enumerate() {
                if !p.items[i].locked {
                    p.items[i].chosen = choice.get(k).copied().unwrap_or(0);
                }
            }
        }
        self.pen.preview = Some(p);
    }

    /// Accept: writes every read symbol, bar by bar in order, as one undo step; the ink goes.
    pub fn pen_accept(&mut self) {
        let Some(p) = self.pen.preview.take() else { return };
        let mark = self.undo.len();
        let mut bars: std::collections::BTreeMap<(usize, usize), Vec<&Item>> = Default::default();
        for it in &p.items {
            if it.staff != usize::MAX && it.reading().is_some_and(|r| ticks_of(r) > 0) {
                bars.entry((it.staff, it.measure)).or_default().push(it);
            }
        }
        let (mut written, mut skipped) = (0, 0);
        for ((s, m), mut its) in bars {
            its.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
            let (mut tick, mt) = self.bar_room(s, m);
            for it in its {
                let r = it.reading().unwrap().clone();
                let d = ticks_of(&r);
                if tick + d > mt {
                    skipped += 1;
                    continue;
                }
                let ok = match &r {
                    Reading::Notes { cols, beamed } => {
                        let mut t = tick;
                        let mut starts = vec![];
                        for c in cols {
                            if self.pen_write_col_at(c, s, m, t).is_some() {
                                starts.push(t);
                            }
                            t += dur_ticks(c.value, c.dots, None);
                        }
                        if *beamed && starts.len() > 1 {
                            let voice = self.voice;
                            let n = starts.len();
                            for (k, st) in starts.iter().enumerate() {
                                if let Some(e) = self.score.events[s][m].iter_mut().find(|e| e.voice == voice && e.start == *st && !e.grace) {
                                    e.beam = if k + 1 < n { BeamMode::Join } else { BeamMode::Auto };
                                }
                            }
                        }
                        !starts.is_empty()
                    }
                    Reading::Rest { value, dots, .. } => {
                        let (ov, od) = (self.cur_value, self.cur_dots);
                        self.cur_value = *value;
                        self.cur_dots = *dots;
                        let r = self.enter_clicked(s, m, tick, None, *value, *dots);
                        self.cur_value = ov;
                        self.cur_dots = od;
                        r.idx != usize::MAX
                    }
                    _ => false,
                };
                if ok {
                    written += 1;
                    tick += d;
                } else {
                    skipped += 1;
                }
            }
        }
        // one undo step: back to before the transcription
        if self.undo.len() > mark + 1 {
            self.undo.truncate(mark + 1);
        }
        self.pen.ink.clear();
        self.status = if skipped > 0 { format!("Wrote {written} symbols; {skipped} didn't fit in their bar. Two-finger tap undoes it.") } else { format!("Wrote {written} symbols. Two-finger tap undoes it.") };
    }

    pub fn pen_cancel_preview(&mut self) {
        self.pen.preview = None;
        self.status = "Back to the ink.".into();
    }

    /// A tap in the preview: on a symbol, its alternatives open.
    pub fn pen_preview_tap(&mut self, p: Pos2) {
        let sp = self.geo.sp;
        let rects: Vec<Rect> = self.pen.preview.as_ref().map(|pr| pr.items.iter().map(|it| Rect::from_min_max(self.ink_to_screen(it.rect.min), self.ink_to_screen(it.rect.max)).expand(0.4 * sp)).collect()).unwrap_or_default();
        if let Some(pr) = self.pen.preview.as_mut() {
            pr.open = rects.iter().position(|r| r.contains(p));
        }
    }

    /// The writer picks reading `k` of item `i` (or none: `k` past the end = not music); the ink teaches the models.
    pub fn pen_preview_pick(&mut self, i: usize, k: usize) {
        let ink = self.ink_screen();
        let sp = self.geo.sp;
        let Some(pr) = self.pen.preview.as_mut() else { return };
        let Some(it) = pr.items.get_mut(i) else { return };
        if k >= it.cands.len() {
            it.cands.clear();
            it.chosen = 0;
        } else {
            it.chosen = k;
            it.locked = true;
            let strokes: Vec<Vec<Pos2>> = it.strokes.iter().filter_map(|&s| ink.get(s).cloned()).collect();
            let r = it.cands[k].0.clone();
            self.pen.personal.learn(&strokes, sp, &r);
        }
        if let Some(pr) = self.pen.preview.as_mut() {
            pr.open = None;
        }
        self.pen_save_personal_pub();
        self.pen_fit_bars();
    }

    // ---------------- drawing ----------------

    /// The ink, or the preview over the faded ink, and the floating buttons.
    pub fn pen_batch_draw(&mut self, ui: &mut Ui, painter: &egui::Painter) {
        let sp = self.geo.sp;
        let ink = self.ink_screen();
        let preview = self.pen.preview.is_some();
        let ink_col = if preview { Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 60) } else { INK };
        for s in &ink {
            if s.len() == 1 {
                painter.circle_filled(s[0], INK_WIDTH / 2.0, ink_col);
            } else {
                painter.add(egui::Shape::line(s.clone(), EStroke::new(INK_WIDTH, ink_col)));
            }
        }
        if let Some(pr) = self.pen.preview.clone() {
            let k = theme::t(ui);
            let read = Color32::from_rgb(0x12, 0x4F, 0xC0);
            let doubt = Color32::from_rgb(0xE8, 0x8A, 0x10);
            for it in &pr.items {
                let r = Rect::from_min_max(self.ink_to_screen(it.rect.min), self.ink_to_screen(it.rect.max)).expand(0.3 * sp);
                match it.reading() {
                    None => {
                        painter.text(r.right_top(), Align2::LEFT_BOTTOM, "✕", theme::font(12.0), Color32::from_gray(150));
                    }
                    Some(reading) => {
                        if it.uncertain() {
                            painter.rect_stroke(r, 6.0, EStroke::new(2.0, doubt), egui::StrokeKind::Outside);
                            painter.rect_filled(r, 6.0, Color32::from_rgba_unmultiplied(doubt.r(), doubt.g(), doubt.b(), 26));
                            painter.text(r.right_top() + vec2(2.0, 0.0), Align2::LEFT_BOTTOM, "?", theme::font_semibold(13.0), doubt);
                        }
                        draw_reading(painter, reading, sp, read);
                    }
                }
            }
            // the alternatives of the open item
            if let Some(i) = pr.open {
                if let Some(it) = pr.items.get(i) {
                    let r = Rect::from_min_max(self.ink_to_screen(it.rect.min), self.ink_to_screen(it.rect.max));
                    let mut pick = None;
                    egui::Area::new(egui::Id::unique("pen-preview-alts")).order(egui::Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(pos2(r.center().x, r.bottom() + sp * 1.2)).show(ui.ctx(), |ui| {
                        egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(12.0).inner_margin(6.0).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) }).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (j, (reading, _)) in it.cands.iter().enumerate().take(pen::ALTERNATIVES_MAX) {
                                    let on = j == it.chosen;
                                    let (cell, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                                    if on {
                                        ui.painter().rect_filled(cell, 9.0, k.accent);
                                    } else if resp.hovered() {
                                        ui.painter().rect_filled(cell, 9.0, k.cell_hover);
                                    }
                                    crate::pen_ui::reading_figure_pub(ui.painter(), cell, reading, if on { Color32::WHITE } else { k.text });
                                    if resp.on_hover_text(reading.describe()).clicked() {
                                        pick = Some(j);
                                    }
                                }
                                let (cell, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                                ui.painter().text(cell.center(), Align2::CENTER_CENTER, "✕", theme::font(18.0), k.text_2);
                                if resp.on_hover_text("Not music: write nothing for this ink").clicked() {
                                    pick = Some(usize::MAX);
                                }
                            });
                        });
                    });
                    if let Some(j) = pick {
                        self.pen_preview_pick(i, j);
                    }
                }
            }
        }
        // the floating buttons, bottom right of the page view
        let symbols = if preview { 0 } else { self.ink_symbols() };
        if preview || symbols > 0 {
            let k = theme::t(ui);
            let clip = ui.clip_rect();
            let mut act = None;
            egui::Area::new(egui::Id::unique("pen-transcribe")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(clip.right_bottom() - vec2(20.0, 16.0)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(14.0).inner_margin(8.0).shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(45) }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let big = |t: String, fill: Color32, fg: Color32| egui::Button::new(egui::RichText::new(t).font(theme::font_semibold(14.0)).color(fg)).fill(fill).corner_radius(9.0).min_size(vec2(0.0, 40.0));
                        if let Some(pr) = &self.pen.preview {
                            let n = pr.items.iter().filter(|i| i.reading().is_some_and(|r| ticks_of(r) > 0)).count();
                            let unsure = pr.items.iter().filter(|i| i.uncertain()).count();
                            if unsure > 0 {
                                ui.label(egui::RichText::new(format!("{unsure} to check · tap to change")).font(theme::font(12.5)).color(k.text_2));
                            }
                            if ui.add(big("Cancel".into(), k.cell, k.text)).clicked() {
                                act = Some(0);
                            }
                            if ui.add(big(format!("Accept ({n})"), k.accent, Color32::WHITE)).clicked() {
                                act = Some(1);
                            }
                        } else {
                            if ui.add(big("Clear ink".into(), k.cell, k.text)).clicked() {
                                act = Some(2);
                            }
                            if ui.add(big(format!("Transcribe ({symbols})"), k.accent, Color32::WHITE)).clicked() {
                                act = Some(3);
                            }
                        }
                    });
                });
            });
            match act {
                Some(0) => self.pen_cancel_preview(),
                Some(1) => self.pen_accept(),
                Some(2) => self.pen_clear_ink(),
                Some(3) => self.pen_transcribe(),
                _ => {}
            }
        }
    }
}

/// A reading drawn where it was written, in engraving size.
fn draw_reading(p: &egui::Painter, r: &Reading, sp: f32, col: Color32) {
    use crate::glyphs as g;
    match r {
        Reading::Notes { cols, beamed } => {
            let mut tops = vec![];
            for c in cols {
                let mut lowest = None::<Pos2>;
                for h in &c.heads {
                    g::notehead(p, *h, sp, c.value, col);
                    lowest = Some(match lowest {
                        Some(l) if l.y > h.y => l,
                        _ => *h,
                    });
                    for d in 0..c.dots {
                        p.circle_filled(pos2(h.x + sp * (1.0 + d as f32 * 0.5), h.y - sp * 0.25), sp * 0.16, col);
                    }
                }
                if let (Some(alter), Some(h)) = (c.acc, c.heads.get(c.acc_head)) {
                    g::accidental(p, pos2(h.x - sp * 1.5, h.y), sp, alter, col);
                }
                if c.value >= 2 {
                    let top = c.heads.iter().map(|h| h.y).fold(f32::MAX, f32::min);
                    let base = lowest.unwrap_or(c.heads[0]);
                    let a = g::stem_attach(base, sp, c.value, true, sp * 0.12);
                    let end = pos2(a.x, top - sp * 3.3);
                    p.line_segment([a, end], EStroke::new(sp * 0.12, col));
                    if c.value >= 4 && !*beamed {
                        g::flag(p, end, sp, c.value - 3, true, col);
                    }
                    tops.push(end);
                }
            }
            if *beamed && tops.len() > 1 {
                let k = cols[0].value.saturating_sub(3).max(1);
                let y = tops.iter().map(|t| t.y).fold(f32::MAX, f32::min);
                for b in 0..k {
                    let yy = y + b as f32 * sp * 0.75;
                    p.add(egui::Shape::convex_polygon(vec![pos2(tops[0].x, yy), pos2(tops.last().unwrap().x, yy), pos2(tops.last().unwrap().x, yy + sp * 0.45), pos2(tops[0].x, yy + sp * 0.45)], col, EStroke::NONE));
                }
            }
        }
        Reading::Rest { value, dots, at } => {
            g::rest(p, *at, sp, *value, col);
            for d in 0..*dots {
                p.circle_filled(pos2(at.x + sp * (1.0 + d as f32 * 0.5), at.y - sp * 0.5), sp * 0.16, col);
            }
        }
        Reading::Accidental { alter, at, .. } => g::accidental(p, *at, sp, *alter, col),
        Reading::Dot { at } => {
            p.circle_filled(*at, sp * 0.16, col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::samples::*;
    use std::collections::HashSet;

    fn drawn(a: &mut App) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut geo = sv::layout(&a.score, Pos2::ZERO, 1.0, false);
        let (mut hits, mut ghits) = (vec![], vec![]);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
            let (sel, selg) = (HashSet::new(), HashSet::new());
            let st = sv::DrawState { selected: &sel, sel_graphics: &selg, cursor: None, play_tick: None, show_voice: None, show_control_points: false, show_hidden: false, sel_measures: None, hover_ghost: None, flash: None, erase: None };
            sv::draw(&p, &a.score, &geo, &st, &mut hits, &mut ghits);
        });
        out.textures_delta.clear();
        geo.notes = hits;
        geo.graphics = ghits;
        a.geo = geo;
    }

    /// The tablet in the default mode: the ink stays until Transcribe.
    fn app() -> App {
        let mut a = App::new(true);
        a.voice = 1;
        a.zoom = 1.0;
        a.prefs.pen_immediate = false;
        drawn(&mut a);
        a
    }

    fn spot(a: &App, m: usize, tick: u32, pos: i32) -> Pos2 {
        let sys = a.geo.systems.iter().find(|s| s.measures.iter().any(|g| g.covers(m))).unwrap();
        let mg = sys.measures.iter().find(|g| g.covers(m)).unwrap();
        pos2(sv::x_for_tick(mg, tick, a.score.measures[m].ticks()), sys.staves[0].1 - pos as f32 * a.geo.sp * 0.5)
    }

    /// Strokes written with the pen, each 0.2 s apart (time doesn't group them any more).
    fn write(a: &mut App, strokes: &[Vec<Pos2>], t0: f64) -> f64 {
        let mut t = t0;
        for s in strokes {
            a.pen_feed(0, s[0], t);
            for p in &s[1..] {
                a.pen_feed(1, *p, t + 0.05);
            }
            a.pen_feed(2, *s.last().unwrap(), t + 0.1);
            a.pen_tick(t + 0.1);
            t += 0.2;
        }
        t
    }

    fn notes(a: &App, m: usize) -> Vec<(u32, u8, u8, Vec<i32>)> {
        a.score.events[0][m].iter().map(|e| (e.start, e.value, e.dots, e.pitches.iter().map(|p| p.midi()).collect())).collect()
    }

    #[test]
    fn the_ink_stays_until_transcribe_and_accept_is_one_undo_step() {
        let mut a = app();
        let sp = a.geo.sp;
        let hands = hands();
        let mut t = 1.0;
        // four crooked quarters in bar 1: G4 A4 B4 C5, written slowly (seconds apart: still just ink)
        for (k, pos) in [(0u32, -2), (1, -1), (2, 0), (3, 1)] {
            let h = spot(&a, 0, k * TPQ, pos);
            t = write(&mut a, &note_t(h, sp, 3, 0, false, hands[k as usize % 3]), t) + 2.0;
            a.pen_tick(t);
        }
        assert!(notes(&a, 0).is_empty(), "nothing is written before Transcribe");
        assert_eq!(a.pen.ink.len(), 8);
        assert_eq!(a.ink_symbols(), 4);
        a.pen_transcribe();
        let p = a.pen.preview.as_ref().expect("the preview");
        assert_eq!(p.items.len(), 4);
        assert!(notes(&a, 0).is_empty(), "the preview writes nothing");
        // Cancel: back to the ink
        a.pen_cancel_preview();
        assert_eq!(a.pen.ink.len(), 8);
        a.pen_transcribe();
        let undo = a.undo.len();
        a.pen_accept();
        assert_eq!(notes(&a, 0), vec![(0, 3, 0, vec![67]), (TPQ, 3, 0, vec![69]), (2 * TPQ, 3, 0, vec![71]), (3 * TPQ, 3, 0, vec![72])]);
        assert!(a.pen.ink.is_empty());
        assert_eq!(a.undo.len(), undo + 1, "one undo step");
        a.do_undo();
        assert!(notes(&a, 0).is_empty());
    }

    #[test]
    fn strokes_group_by_place_not_by_time() {
        let a = app();
        let sp = a.geo.sp;
        let h = spot(&a, 0, 0, -2);
        let hand = hands()[1]; // the stem a space apart from the head
        let mut strokes = note_t(h, sp, 4, 0, false, hand);
        let n = strokes.len();
        strokes.extend(note_t(pos2(h.x + 3.0 * sp, h.y), sp, 2, 0, false, hands()[0]));
        strokes.push(dot(pos2(h.x + 4.3 * sp, h.y), sp, 1));
        let g = group(&strokes, sp);
        assert_eq!(g.len(), 3, "{g:?}");
        assert_eq!(g[0], (0..n).collect::<Vec<_>>());
        // a beamed group is one symbol
        let b = beamed_t(h, sp, 3, 2, false, hands()[0]);
        assert_eq!(group(&b, sp).len(), 1);
    }

    #[test]
    fn the_bar_prefers_readings_that_fill_it() {
        let q = TPQ;
        // three sure quarters and a doubtful one: half (0.5) or quarter (0.45) → the quarter fills 4/4
        let items = vec![vec![(q, 0.9)], vec![(q, 0.9)], vec![(q, 0.9)], vec![(2 * q, 0.5), (q, 0.45)]];
        assert_eq!(fit_bar(&items, 4 * q), vec![0, 0, 0, 1]);
        // a half, then whole (0.55) or half (0.45) → the half
        assert_eq!(fit_bar(&[vec![(2 * q, 0.9)], vec![(4 * q, 0.55), (2 * q, 0.45)]], 4 * q), vec![0, 1]);
        // nothing fills: the most likely ones stay
        assert_eq!(fit_bar(&[vec![(q, 0.6), (2 * q, 0.4)]], 4 * q), vec![0]);
    }

    #[test]
    fn dots_and_accidentals_find_their_notes_and_a_bar_is_completed() {
        let mut a = app();
        let sp = a.geo.sp;
        let mut t = 1.0;
        // a dotted half on G4, then ♯ and a quarter on F4
        let g4 = spot(&a, 0, 0, -2);
        t = write(&mut a, &note_t(g4, sp, 2, 0, false, hands()[0]), t);
        t = write(&mut a, &[dot(pos2(g4.x + 1.3 * sp, g4.y - 0.2 * sp), sp, 2)], t);
        let f4 = spot(&a, 0, 3 * TPQ, -3);
        t = write(&mut a, &sharp(pos2(f4.x - 2.2 * sp, f4.y), sp, 5), t);
        let _ = write(&mut a, &note_t(f4, sp, 3, 0, false, hands()[2]), t);
        a.pen_transcribe();
        a.pen_accept();
        assert_eq!(notes(&a, 0), vec![(0, 2, 1, vec![67]), (3 * TPQ, 3, 0, vec![66])]);
    }

    #[test]
    fn a_scribble_over_ink_erases_that_ink() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, 0, TPQ, -2);
        let t = write(&mut a, &note_t(h, sp, 3, 0, false, hands()[0]), 1.0);
        assert_eq!(a.pen.ink.len(), 2);
        let _ = write(&mut a, &[scribble_dir(pos2(h.x + 0.3 * sp, h.y - 1.5 * sp), sp, std::f32::consts::FRAC_PI_2, 4.0, 6, 3)], t);
        assert!(a.pen.ink.is_empty(), "left: {}", a.pen.ink.len());
        assert!(notes(&a, 0).is_empty());
    }

    #[test]
    fn a_pick_in_the_preview_sticks_and_teaches() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, 0, 0, -2);
        let _ = write(&mut a, &note_t(h, sp, 3, 0, false, hands()[0]), 1.0);
        a.pen_transcribe();
        let it = &a.pen.preview.as_ref().unwrap().items[0];
        let k = it.cands.iter().position(|c| matches!(&c.0, Reading::Notes { cols, .. } if cols[0].value == 4)).expect("an eighth offered");
        let before = a.pen.personal.count();
        a.pen_preview_pick(0, k);
        assert!(a.pen.personal.count() > before, "the pick taught a model");
        a.pen_accept();
        assert_eq!(notes(&a, 0), vec![(0, 4, 0, vec![67])]);
    }

    #[test]
    fn the_model_is_built_in_and_holds_up_on_ink_it_never_saw() {
        assert!(pen_model::model_loaded(), "assets/pen_model.bin is read");
        let report = pen_model::train::eval_report();
        // the "all" lines: model, rules, both
        let alls: Vec<Vec<f32>> = report.lines().filter(|l| l.starts_with("all")).map(|l| l.split_whitespace().skip(2).map(|v| v.trim_end_matches('%').parse::<f32>().unwrap()).collect()).collect();
        assert_eq!(alls.len(), 2, "{report}");
        assert!(alls[0][2] >= 92.0, "held-out generator, model+rules: {report}");
        assert!(alls[1][2] >= 95.0, "another family, model+rules: {report}");
        assert!(alls[0][2] > alls[0][1], "together better than the rules alone on varied hands: {report}");
    }
}
