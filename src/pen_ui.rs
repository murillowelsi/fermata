//! F-38/F-39: the pen on the score (web on tablets): ink, the guide while writing, reading the strokes after the
//! pause (`pen::DEBOUNCE`), writing what was read as the Pencil's click does (F-36), the short transition, "Not
//! recognized", the alternatives under a new figure (a pick there teaches the writer's models), the scribble that
//! deletes what it covers, the gesture sheet with "Teach Fermata your handwriting" and "Export pen samples…". The
//! recognizer itself is `pen.rs`.
use crate::app::App;
use crate::model::*;
use crate::pen::{self, Col, Ctx, Personal, Reading, Stroke};
use crate::score_view::{self as sv, NoteRef};
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke as EStroke, Ui};
use serde::{Deserialize, Serialize};

/// Ink: the same blue in both themes (the page is always white).
pub const INK: Color32 = Color32::from_rgb(0x1F, 0x6F, 0xEA);
pub const INK_WIDTH: f32 = 2.4;

/// Where the writer's models and samples are kept in the browser.
const KEY_PERSONAL: &str = "fermata.pen.personal.v1";
const KEY_SAMPLES: &str = "fermata.pen.samples.v1";

#[derive(Clone, Debug, PartialEq)]
pub enum FeedbackKind {
    /// Read and written: the ink shrinks into the note at `head` with a short glow.
    Recognized { head: Pos2 },
    /// Not read: grey ink and the balloon.
    Unrecognized,
    /// A scribble over nothing: the ink just fades.
    Vanish,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub strokes: Vec<Vec<Pos2>>,
    pub kind: FeedbackKind,
    pub start: f64,
}

/// The readings offered under a new figure for 3 s (the one written first).
#[derive(Clone, Debug, PartialEq)]
pub struct Alternatives {
    pub readings: Vec<Reading>,
    pub chosen: usize,
    /// the ink and the staff space it was read at (a pick teaches the models with it)
    pub strokes: Vec<Vec<Pos2>>,
    pub sp: f32,
    /// the undo depth before it was written: picking another reading goes back to it
    pub undo_mark: usize,
    pub sample: Option<usize>,
    pub at: Pos2,
    pub until: f64,
}

/// An accidental written alone, waiting for its head: what it is, its box, the staff's middle line, when, its ink,
/// and the other readings of that ink (if no head comes and no note is next to it, the best of them goes in).
#[derive(Clone, Debug, PartialEq)]
pub struct LoneAccidental {
    pub alter: i8,
    pub rect: Rect,
    pub mid: f32,
    pub t: f64,
    pub ink: Vec<Vec<Pos2>>,
    pub others: Vec<Reading>,
}
/// Two-finger tap: (when the second finger came, where each finger started, how far they moved).
pub type TwoTap = (f64, Vec<(u64, Pos2)>, f32);

/// One written symbol for "Export pen samples…": its strokes in staff spaces from the corner of its box, what it was
/// read as, and what the writer picked instead. Nothing else.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SampleRec {
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub read: String,
    #[serde(default)]
    pub corrected: Option<String>,
}

/// "Teach Fermata your handwriting": each figure twice on a practice staff.
#[derive(Clone, Debug, Default)]
pub struct Calib {
    pub step: usize,
    pub rep: u8,
    pub cur: Option<Vec<Pos2>>,
    pub strokes: Vec<Vec<Pos2>>,
    pub last_up: f64,
    pub learnt: usize,
    /// the pen (its own samples, not egui's pointer) is drawing on the practice staff
    pub pen_drawing: bool,
}

/// The figures of the calibration: (what it says, the model's label, the glyph shown).
pub const CALIB: [(&str, &str, char); 12] = [
    ("Quarter note", "n3.0", '\u{E1D5}'),
    ("Half note", "n2.0", '\u{E1D3}'),
    ("Whole note", "n1.0", '\u{E1D2}'),
    ("Eighth note", "n4.0", '\u{E1D7}'),
    ("16th note", "n5.0", '\u{E1D9}'),
    ("Dotted quarter", "n3.1", '\u{E1D5}'),
    ("Quarter rest", "r3.0", '\u{E4E5}'),
    ("Eighth rest", "r4.0", '\u{E4E6}'),
    ("Half rest", "r2.0", '\u{E4E4}'),
    ("Sharp", "a1", '\u{E262}'),
    ("Flat", "a-1", '\u{E260}'),
    ("Natural", "a0", '\u{E261}'),
];
/// The practice staff's space (points).
pub const CALIB_SP: f32 = 12.0;

#[derive(Clone, Debug, Default)]
pub struct PenState {
    /// The stroke being drawn.
    pub cur: Option<Stroke>,
    /// Strokes waiting for the pause to end (one symbol).
    pub pending: Vec<Stroke>,
    pub last_up: f64,
    pub feedback: Vec<Feedback>,
    pub alts: Option<Alternatives>,
    /// Where the alternatives were drawn last frame (a pen or finger there picks one, it doesn't write).
    pub alts_rect: Option<Rect>,
    pub acc: Option<LoneAccidental>,
    /// "Not recognized" balloon: where, until when.
    pub balloon: Option<(Pos2, f64)>,
    pub sheet: bool,
    /// A pen was used on this device (from then on, the finger scrolls).
    pub pen_seen: bool,
    /// The pen is down or just lifted (it then works the tool as a mouse, not as a scrolling finger).
    pub pen_active: bool,
    /// The pen's last touch started ink (on the page, nothing over it, the Pencil chosen).
    pub ink_down: bool,
    pub two_tap: Option<TwoTap>,
    pub max_fingers: usize,
    /// The writer's models and recent samples (loaded from the browser on first use).
    pub personal: Personal,
    pub samples: Vec<SampleRec>,
    pub loaded: bool,
    pub calib: Option<Calib>,
    /// Pen samples handed in by tests (the browser's come from `web::pen_take`).
    pub injected: Vec<(u8, f32, f32)>,
    /// This frame's pen samples that didn't go to the score's ink (the practice staff reads them).
    pub loose: Vec<(u8, Pos2)>,
}

/// Value → the grid the Pencil uses for it (as `App::grid` does for the palette's value).
fn grid_for(value: u8) -> u32 {
    dur_ticks(value, 0, None).min(base_ticks(4)).max(1)
}

fn ink_box(strokes: &[Vec<Pos2>]) -> Rect {
    let mut r = Rect::NOTHING;
    for p in strokes.iter().flatten() {
        r.extend_with(*p);
    }
    r
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static STORE: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
}

fn store_get(k: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::pen_store_get(k)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        STORE.with(|s| s.borrow().get(k).cloned().unwrap_or_default())
    }
}

fn store_set(k: &str, v: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::pen_store_set(k, v);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        STORE.with(|s| s.borrow_mut().insert(k.to_string(), v.to_string()));
    }
}

impl App {
    /// A pen sample: phase 0 down, 1 move, 2 up; position in screen points; time in seconds.
    pub fn pen_feed(&mut self, phase: u8, p: Pos2, t: f64) {
        match phase {
            0 => {
                // a stroke on the alternatives picks one; it never writes
                if self.pen.alts_rect.is_some_and(|r| r.contains(p)) && self.pen.alts.is_some() {
                    return;
                }
                // the next figure: the alternatives of the last one go
                if self.pen.pending.is_empty() {
                    self.pen.alts = None;
                }
                self.pen.balloon = None;
                self.pen.cur = Some(Stroke { pts: vec![p], t0: t, t1: t });
            }
            1 => {
                if let Some(s) = self.pen.cur.as_mut() {
                    if s.pts.last().map(|q| q.distance(p) >= 0.6).unwrap_or(true) {
                        s.pts.push(p);
                    }
                    s.t1 = t;
                }
            }
            _ => {
                if let Some(mut s) = self.pen.cur.take() {
                    s.pts.push(p);
                    s.t1 = t;
                    self.pen.pending.push(s);
                    self.pen.last_up = t;
                }
            }
        }
    }

    /// Drops the stroke being drawn (a second finger came: it was a scroll, not writing).
    pub fn pen_cancel_stroke(&mut self) {
        self.pen.cur = None;
    }

    /// The writer's models and samples, from the browser (once).
    pub fn pen_load(&mut self) {
        if self.pen.loaded {
            return;
        }
        self.pen.loaded = true;
        self.pen.personal = serde_json::from_str(&store_get(KEY_PERSONAL)).unwrap_or_default();
        self.pen.samples = serde_json::from_str(&store_get(KEY_SAMPLES)).unwrap_or_default();
    }

    fn pen_save_personal(&self) {
        store_set(KEY_PERSONAL, &serde_json::to_string(&self.pen.personal).unwrap_or_default());
    }

    fn pen_save_samples(&self) {
        store_set(KEY_SAMPLES, &serde_json::to_string(&self.pen.samples).unwrap_or_default());
    }

    /// Keeps a written symbol for "Export pen samples…" (points only, in staff spaces); its index.
    fn pen_record(&mut self, strokes: &[Vec<Pos2>], read: String) -> Option<usize> {
        let sp = self.geo.sp.max(1e-3);
        let r = ink_box(strokes);
        let st: Vec<Vec<[f32; 2]>> = strokes
            .iter()
            .map(|s| {
                let step = (s.len() / 64).max(1);
                s.iter().step_by(step).map(|p| [((p.x - r.left()) / sp * 100.0).round() / 100.0, ((p.y - r.top()) / sp * 100.0).round() / 100.0]).collect()
            })
            .collect();
        self.pen.samples.push(SampleRec { strokes: st, read, corrected: None });
        if self.pen.samples.len() > pen::SAMPLES_KEPT {
            self.pen.samples.remove(0);
            if let Some(a) = self.pen.alts.as_mut() {
                a.sample = a.sample.and_then(|i| i.checked_sub(1));
            }
        }
        self.pen_save_samples();
        Some(self.pen.samples.len() - 1)
    }

    /// The JSON "Export pen samples…" downloads.
    pub fn pen_export_json(&mut self) -> String {
        self.pen_load();
        let v = serde_json::json!({
            "format": "fermata-pen-samples",
            "version": 1,
            "units": "staff spaces, from the top left of each symbol",
            "samples": self.pen.samples,
        });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    fn pen_export(&mut self) {
        let json = self.pen_export_json();
        #[cfg(target_arch = "wasm32")]
        crate::web::export("fermata-pen-samples.json", json.as_bytes(), "json", "Pen samples");
        #[cfg(not(target_arch = "wasm32"))]
        let _ = json;
        self.status = format!("Exported {} pen samples (only the strokes and what they were read as).", self.pen.samples.len());
    }

    /// Reads the waiting strokes once the pen has stayed up `pen::DEBOUNCE` seconds; settles a lone accidental that
    /// waited too long for its head; ends the timed feedback.
    pub fn pen_tick(&mut self, now: f64) {
        if self.pen.cur.is_none() && !self.pen.pending.is_empty() && now - self.pen.last_up >= pen::DEBOUNCE {
            let strokes: Vec<Vec<Pos2>> = std::mem::take(&mut self.pen.pending).into_iter().map(|s| s.pts).collect();
            self.pen_apply(strokes, now);
        }
        if let Some(acc) = &self.pen.acc {
            if now - acc.t > pen::ACCIDENTAL_WAIT && self.pen.pending.is_empty() && self.pen.cur.is_none() {
                let acc = self.pen.acc.take().unwrap();
                // no new head came: it goes to the note just right of it; else the ink's best other reading (a
                // guess, with the alternatives under it), else "not recognized"
                if self.pen_alter_existing(acc.alter, acc.rect) {
                    self.pen.feedback.push(Feedback { strokes: acc.ink, kind: FeedbackKind::Recognized { head: acc.rect.center() }, start: now });
                } else if let Some(first) = acc.others.first().cloned() {
                    let ink = acc.ink.clone();
                    self.pen_write_offering(first, acc.others, acc.ink, ink, now);
                } else {
                    self.pen_unrecognized(acc.ink, now);
                }
            }
        }
        self.pen.feedback.retain(|f| match f.kind {
            FeedbackKind::Recognized { .. } => now - f.start < pen::RECOGNIZED_FADE + 0.15,
            FeedbackKind::Unrecognized => now - f.start < pen::UNRECOGNIZED_SHOW,
            FeedbackKind::Vanish => now - f.start < 0.5,
        });
        if self.pen.balloon.is_some_and(|(_, until)| now > until) {
            self.pen.balloon = None;
        }
        if self.pen.alts.as_ref().is_some_and(|a| now > a.until) {
            self.pen.alts = None;
        }
    }

    fn pen_unrecognized(&mut self, strokes: Vec<Vec<Pos2>>, now: f64) {
        let r = ink_box(&strokes);
        self.pen.balloon = Some((pos2(r.center().x, r.bottom() + 14.0), now + pen::UNRECOGNIZED_SHOW));
        self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Unrecognized, start: now });
        self.status = "Not recognized. Nothing was added.".into();
    }

    /// The staff whose middle line is nearest to `y` in the system around `p`, with that middle line.
    fn pen_staff_at(&self, p: Pos2) -> Option<(usize, f32)> {
        let sp = self.geo.sp;
        let sys = self.geo.systems.iter().find(|s| p.x >= s.left - sp * 2.0 && p.x <= s.right + sp && p.y >= s.top(sp) - sp * 8.0 && p.y <= s.bottom(sp) + sp * 8.0)?;
        sys.staves.iter().min_by(|a, b| (a.1 - p.y).abs().partial_cmp(&(b.1 - p.y).abs()).unwrap()).copied()
    }

    /// What around the ink helps the recognizer decide: the staff, the time left in the bar, the note before.
    fn pen_context(&self, r: Rect) -> (Option<f32>, Option<u32>, Option<u8>) {
        let sp = self.geo.sp;
        let mid = self.pen_staff_at(r.center()).map(|s| s.1);
        let Some(l) = sv::locate(&self.score, &self.geo, pos2(r.left() + sp * 0.5, r.center().y), grid_for(3)) else { return (mid, None, None) };
        let mt = self.score.measures[l.measure].ticks();
        let voice = self.voice;
        let before = |m: usize, upto: u32| self.score.events[l.staff][m].iter().filter(|e| e.voice == voice && !e.grace && e.start < upto).max_by_key(|e| e.start).map(|e| e.value);
        let neighbor = before(l.measure, l.tick).or_else(|| l.measure.checked_sub(1).and_then(|m| before(m, u32::MAX)));
        (mid, Some(mt.saturating_sub(l.tick)), neighbor)
    }

    /// Notes, rests and symbols under a scribble: an item counts when its middle is under the ink's box, the box
    /// covers half of it, or the ink passes over it.
    fn pen_covered(&self, r: Rect, ink: &[Vec<Pos2>]) -> (Vec<sv::NoteHit>, Vec<usize>) {
        let e = r.expand(self.geo.sp * 0.25);
        let covers = |x: Rect| {
            let i = x.intersect(r);
            e.contains(x.center()) || (i.is_positive() && i.area() >= 0.5 * x.area().max(1e-3)) || ink.iter().flatten().any(|p| x.contains(*p))
        };
        let notes = self.geo.notes.iter().filter(|h| covers(h.rect)).cloned().collect();
        let mut gs: Vec<usize> = self.geo.graphics.iter().filter(|(_, x)| covers(*x)).map(|(i, _)| *i).collect();
        gs.sort();
        gs.dedup();
        (notes, gs)
    }

    /// Deletes what a scribble covered, as one undo step: whole notes and rests, only the covered heads of a chord.
    fn pen_delete(&mut self, hits: &[sv::NoteHit], graphics: &[usize]) {
        self.checkpoint();
        let mut whole: std::collections::BTreeSet<(usize, usize, usize)> = Default::default();
        let mut heads: std::collections::BTreeMap<(usize, usize, usize), Vec<usize>> = Default::default();
        for h in hits {
            let key = (h.r.staff, h.r.measure, h.r.idx);
            match h.pitch {
                Some(k) => heads.entry(key).or_default().push(k),
                None => {
                    whole.insert(key);
                }
            }
        }
        for (key, mut ks) in heads {
            let r = NoteRef { staff: key.0, measure: key.1, idx: key.2 };
            ks.sort();
            ks.dedup();
            let n = self.ev(r).map(|e| e.pitches.len()).unwrap_or(0);
            if ks.len() >= n {
                whole.insert(key);
            } else if let Some(e) = self.ev_mut(r) {
                for &k in ks.iter().rev() {
                    if k < e.pitches.len() {
                        e.pitches.remove(k);
                        if k < e.courtesy.len() {
                            e.courtesy.remove(k);
                        }
                        if k < e.ties.len() {
                            e.ties.remove(k);
                        }
                    }
                }
                e.normalize();
            }
        }
        for &(s, m, i) in whole.iter().rev() {
            if i < self.score.events[s][m].len() {
                self.score.events[s][m].remove(i);
            }
        }
        for &g in graphics.iter().rev() {
            if g < self.score.graphics.len() {
                self.score.graphics.remove(g);
            }
        }
        self.selected.clear();
        self.sel_graphics.clear();
    }

    /// Reads one symbol and acts on it.
    pub fn pen_apply(&mut self, strokes: Vec<Vec<Pos2>>, now: f64) {
        self.pen_load();
        let sp = self.geo.sp;
        let r = ink_box(&strokes);
        // a scribble over notes, rests or symbols deletes everything it covers; over nothing, the ink just goes
        let scribble = pen::scribble_rect(&strokes, sp);
        if let Some(sr) = scribble {
            let (hits, gs) = self.pen_covered(sr, &strokes);
            if !hits.is_empty() || !gs.is_empty() {
                let n = hits.iter().map(|h| (h.r.staff, h.r.measure, h.r.idx)).collect::<std::collections::HashSet<_>>().len() + gs.len();
                self.pen_delete(&hits, &gs);
                self.pen.feedback.push(Feedback { strokes: strokes.clone(), kind: FeedbackKind::Recognized { head: r.center() }, start: now });
                self.pen_record(&strokes, "scribble".into());
                self.status = format!("Scribbled out {n} {}. Two-finger tap undoes it.", if n == 1 { "item" } else { "items" });
                return;
            }
        }
        let (staff_mid, remaining, neighbor) = self.pen_context(r);
        let personal = std::mem::take(&mut self.pen.personal);
        let mut cands = pen::recognize(&strokes, sp, &Ctx { staff_mid, remaining, neighbor, personal: Some(&personal) });
        self.pen.personal = personal;
        // a scribble bigger than a head over nothing: only a clear rest is kept (a quarter rest zigzags too)
        if let Some(sr) = scribble {
            let rest = cands.first().is_some_and(|c| c.score >= 0.5 && matches!(c.reading, Reading::Rest { value: 3..=7, .. }));
            if sr.width().max(sr.height()) > 2.2 * sp && !rest {
                cands.clear();
            }
        }
        let Some(first) = cands.first().cloned() else {
            self.pen_record(&strokes, "none".into());
            if scribble.is_some() {
                self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Vanish, start: now });
                self.status = "A scribble over nothing: nothing changed.".into();
            } else {
                self.pen_unrecognized(strokes, now);
            }
            return;
        };
        match first.reading.clone() {
            Reading::Accidental { alter, .. } => {
                // it waits for its head (the ink stays)
                let mid = self.pen_staff_at(r.center()).map(|s| s.1).unwrap_or(r.center().y);
                let others = cands.iter().filter(|c| matches!(c.reading, Reading::Notes { .. } | Reading::Rest { .. })).map(|c| c.reading.clone()).take(pen::ALTERNATIVES_MAX).collect();
                self.pen.acc = Some(LoneAccidental { alter, rect: r, mid, t: now, ink: strokes, others });
            }
            Reading::Dot { at } => {
                if self.pen_dot_existing(at) {
                    self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Recognized { head: at }, start: now });
                } else {
                    self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Vanish, start: now });
                }
            }
            mut reading => {
                // an accidental written just before, a little to the left, on the same staff
                let mut ink = strokes.clone();
                if let (Reading::Notes { cols, .. }, Some(acc)) = (&mut reading, self.pen.acc.clone()) {
                    if let (Some(c), Some((_, smid))) = (cols.first_mut(), self.pen_staff_at(r.center())) {
                        let head = c.heads[0];
                        let ar = acc.rect;
                        if c.acc.is_none() && now - acc.t <= pen::ACCIDENTAL_WAIT + pen::DEBOUNCE && (acc.mid - smid).abs() < 1.0 && ar.right() <= head.x && head.x - ar.right() <= pen::ACCIDENTAL_REACH * sp + sp {
                            c.acc = Some(acc.alter);
                            c.acc_head = (0..c.heads.len()).min_by(|&a, &b| (c.heads[a].y - ar.center().y).abs().partial_cmp(&(c.heads[b].y - ar.center().y).abs()).unwrap()).unwrap_or(0);
                            ink.extend(acc.ink);
                            self.pen.acc = None;
                        }
                    }
                }
                let others: Vec<Reading> = cands.iter().skip(1).map(|c| c.reading.clone()).collect();
                let mut all = vec![reading.clone()];
                all.extend(others);
                self.pen_write_offering(reading, all, strokes, ink, now);
            }
        }
    }

    /// Writes `reading` and offers the other readings under it (the first of `readings` is the one written).
    fn pen_write_offering(&mut self, reading: Reading, readings: Vec<Reading>, strokes: Vec<Vec<Pos2>>, ink: Vec<Vec<Pos2>>, now: f64) {
        let sp = self.geo.sp;
        let mark = self.undo.len();
        if !self.pen_write(&reading) {
            self.pen_unrecognized(ink, now);
            return;
        }
        let sample = self.pen_record(&strokes, reading.label());
        let at = reading.anchor();
        self.pen.feedback.push(Feedback { strokes: ink, kind: FeedbackKind::Recognized { head: at }, start: now });
        // the alternatives: the reading written, then the others, best first
        let mut offer = vec![reading.clone()];
        for r in readings {
            if offer.len() >= pen::ALTERNATIVES_MAX {
                break;
            }
            if matches!(r, Reading::Notes { .. } | Reading::Rest { .. }) && !offer.iter().any(|x| x.label() == r.label()) {
                offer.push(r);
            }
        }
        self.pen.alts = Some(Alternatives { readings: offer, chosen: 0, strokes, sp, undo_mark: mark, sample, at, until: now + pen::ALTERNATIVES_SHOW });
    }

    /// Writes a reading as the Pencil's click would (F-36): each column's pitch from the line or space of its heads,
    /// its beat from x on the grid; a head within `pen::CHORD_DX` of a note of the voice joins its chord; a beamed
    /// group is joined by its beam. False when nothing could be written.
    pub fn pen_write(&mut self, reading: &Reading) -> bool {
        match reading {
            Reading::Notes { cols, beamed } => {
                let mut written = vec![];
                for c in cols {
                    if let Some(w) = self.pen_write_col(c) {
                        written.push(w);
                    }
                }
                if written.is_empty() {
                    return false;
                }
                if *beamed && written.len() > 1 {
                    let voice = self.voice;
                    let n = written.len();
                    for (k, &(s, m, start)) in written.iter().enumerate() {
                        if let Some(e) = self.score.events[s][m].iter_mut().find(|e| e.voice == voice && e.start == start && !e.grace) {
                            e.beam = if k + 1 < n { BeamMode::Join } else { BeamMode::Auto };
                        }
                    }
                }
                let what = reading.describe();
                let first = cols.first().map(|c| c.heads.len()).unwrap_or(0);
                let name = if cols.len() == 1 && first == 1 { self.ev_name(written[0]) } else { String::new() };
                self.status = format!("Wrote {} {what}{name}. Two-finger tap undoes it.", article(&what));
                true
            }
            Reading::Rest { value, dots, at } => {
                let Some(l) = sv::locate(&self.score, &self.geo, *at, grid_for(*value)) else { return false };
                let (old_v, old_d) = (self.cur_value, self.cur_dots);
                self.cur_value = *value;
                self.cur_dots = *dots;
                let r = self.enter_clicked(l.staff, l.measure, l.tick, None, *value, *dots);
                self.cur_value = old_v;
                self.cur_dots = old_d;
                if r.idx == usize::MAX {
                    return false;
                }
                let what = reading.describe();
                self.status = format!("Wrote {} {what}. Two-finger tap undoes it.", article(&what));
                true
            }
            _ => false,
        }
    }

    fn ev_name(&self, (s, m, start): (usize, usize, u32)) -> String {
        self.score.events[s][m].iter().find(|e| e.voice == self.voice && e.start == start).and_then(|e| e.pitches.first()).map(|p| format!(" {}", p.name())).unwrap_or_default()
    }

    /// One column: its first head sets the beat; the others join it as a chord. Some((staff, measure, start)).
    fn pen_write_col(&mut self, c: &Col) -> Option<(usize, usize, u32)> {
        let sp = self.geo.sp;
        let head = *c.heads.first()?;
        let l = sv::locate(&self.score, &self.geo, head, grid_for(c.value))?;
        let voice = self.voice;
        // the chord: a note of this voice drawn at about the same x
        let mut tick = l.tick;
        let near = self.geo.notes.iter().filter(|h| h.r.staff == l.staff && h.r.measure == l.measure && h.pitch.is_some() && (h.rect.center().x - head.x).abs() <= pen::CHORD_DX * sp).filter_map(|h| self.score.events[l.staff][l.measure].get(h.r.idx)).find(|e| e.voice == voice && !e.grace && !e.is_rest()).map(|e| e.start);
        if let Some(t) = near {
            tick = t;
        }
        let key = self.score.measures[l.measure].key;
        let mut first: Option<NoteRef> = None;
        for (k, hp) in c.heads.iter().enumerate() {
            let step = if k == 0 { l.step } else { sv::locate(&self.score, &self.geo, pos2(head.x, hp.y), grid_for(c.value)).filter(|x| x.staff == l.staff).map(|x| x.step).unwrap_or(l.step) };
            let mut alter = key_alter(key, step.rem_euclid(7) as usize);
            // an accidental earlier in the bar on the same line carries through
            if let Some(prev) = self.score.events[l.staff][l.measure].iter().filter(|e| e.start < tick).flat_map(|e| e.pitches.iter()).rfind(|q| q.step == step) {
                alter = prev.alter;
            }
            let written = (c.acc.is_some() && k == c.acc_head).then_some(c.acc).flatten();
            let p = Pitch { step, alter: written.unwrap_or(alter) };
            let (old_v, old_d) = (self.cur_value, self.cur_dots);
            self.cur_value = c.value;
            self.cur_dots = c.dots;
            let at = first.and_then(|f| self.ev(f)).map(|e| e.start).unwrap_or(tick);
            let r = self.enter_clicked(l.staff, l.measure, at, Some(p), c.value, c.dots);
            self.cur_value = old_v;
            self.cur_dots = old_d;
            if r.idx == usize::MAX {
                // the first head must go in; a chord head that doesn't is left out
                first?;
                continue;
            }
            if written.is_some() {
                // a written accidental is shown even when the key would imply it
                if let Some(e) = self.ev_mut(r) {
                    if let Some(i) = e.pitches.iter().position(|q| *q == p) {
                        if i < e.courtesy.len() {
                            e.courtesy[i] = true;
                        }
                    }
                }
            }
            first.get_or_insert(r);
        }
        let f = first?;
        self.ev(f).map(|e| (l.staff, l.measure, e.start))
    }

    /// A dot written alone just right of a note: that note gets a dot (when it still fits before the next one).
    fn pen_dot_existing(&mut self, at: Pos2) -> bool {
        let sp = self.geo.sp;
        let Some(h) = self.geo.notes.iter().filter(|h| at.x >= h.rect.center().x && at.x - h.rect.right() <= 2.6 * sp && (at.y - h.rect.center().y).abs() <= sp).max_by(|a, b| a.rect.center().x.partial_cmp(&b.rect.center().x).unwrap()).cloned() else { return false };
        let Some(e) = self.ev(h.r).cloned() else { return false };
        if e.dots >= 2 {
            return false;
        }
        let end = e.start + dur_ticks(e.value, e.dots + 1, e.tuplet);
        let next = self.score.events[h.r.staff][h.r.measure].iter().filter(|x| x.voice == e.voice && x.start > e.start && !x.grace).map(|x| x.start).min();
        if end > self.score.measures[h.r.measure].ticks() || next.is_some_and(|n| n < end) {
            self.status = "No room for the dot before the next note.".into();
            return false;
        }
        self.checkpoint();
        if let Some(x) = self.ev_mut(h.r) {
            x.dots += 1;
        }
        self.status = "Dotted. Two-finger tap undoes it.".into();
        true
    }

    /// An accidental written alone, just left of an existing note at its height: that note takes it.
    fn pen_alter_existing(&mut self, alter: i8, r: Rect) -> bool {
        let sp = self.geo.sp;
        let Some(h) = self.geo.notes.iter().filter(|h| h.pitch.is_some() && h.rect.left() >= r.right() - 0.3 * sp && h.rect.left() - r.right() <= pen::ACCIDENTAL_REACH * sp + sp && (h.rect.center().y - r.center().y).abs() <= 0.8 * sp).min_by(|a, b| a.rect.left().partial_cmp(&b.rect.left()).unwrap()).cloned() else { return false };
        let Some(k) = h.pitch else { return false };
        self.checkpoint();
        if let Some(e) = self.ev_mut(h.r) {
            if k < e.pitches.len() {
                e.pitches[k].alter = alter;
                if k < e.courtesy.len() {
                    e.courtesy[k] = true;
                }
            }
        }
        self.status = format!("Added a {}. Two-finger tap undoes it.", pen::alter_name(alter));
        true
    }

    /// One of the alternatives: back to before the figure was written, write this reading instead, and learn the
    /// ink as this figure (the writer's own model, kept in the browser).
    pub fn pen_pick_alternative(&mut self, i: usize) {
        let Some(a) = self.pen.alts.clone() else { return };
        let Some(reading) = a.readings.get(i).cloned() else { return };
        while self.undo.len() > a.undo_mark {
            self.do_undo();
        }
        self.pen.alts = None;
        if self.pen_write(&reading) {
            let what = reading.describe();
            self.status = format!("Changed to {} {what}. Fermata will remember this handwriting.", article(&what));
        }
        self.pen.personal.learn(&a.strokes, a.sp, &reading);
        self.pen_save_personal();
        if let Some(s) = a.sample.and_then(|k| self.pen.samples.get_mut(k)) {
            s.corrected = Some(reading.label());
        }
        self.pen_save_samples();
    }

    fn pen_undo_figure(&mut self) {
        if let Some(a) = self.pen.alts.take() {
            while self.undo.len() > a.undo_mark {
                self.do_undo();
            }
        }
        self.status = "Undone.".into();
    }

    // ---------------- "Teach Fermata your handwriting" ----------------

    pub fn pen_calib_start(&mut self) {
        self.pen_load();
        self.pen.calib = Some(Calib::default());
    }

    /// A finished stroke on the practice staff (staff space `CALIB_SP`).
    pub fn pen_calib_stroke(&mut self, pts: Vec<Pos2>, now: f64) {
        if let Some(c) = self.pen.calib.as_mut() {
            if pts.len() >= 2 {
                c.strokes.push(pts);
                c.last_up = now;
            }
        }
    }

    /// After the pause, the drawing is learnt as the figure asked for; each figure twice, then the next.
    pub fn pen_calib_tick(&mut self, now: f64) {
        let Some(c) = self.pen.calib.as_mut() else { return };
        if c.step >= CALIB.len() || c.cur.is_some() || c.strokes.is_empty() || now - c.last_up < pen::DEBOUNCE {
            return;
        }
        let strokes = std::mem::take(&mut c.strokes);
        let label = CALIB[c.step].1;
        if let Some(reading) = pen::reading_for(label, &strokes, CALIB_SP) {
            self.pen.personal.learn(&strokes, CALIB_SP, &reading);
            c.learnt += 1;
            c.rep += 1;
            if c.rep >= 2 {
                c.rep = 0;
                c.step += 1;
            }
            self.pen_save_personal();
        }
    }

    // ---------------- drawing ----------------

    /// The ink, the guide while writing, the transition, "Not recognized" and the alternatives.
    pub fn pen_draw(&mut self, ui: &mut Ui, painter: &egui::Painter, now: f64) {
        let sp = self.geo.sp;
        // the guide while the pen writes: the beats of the measure, the target beat, the line or space, and the label
        // (from the symbol's first stroke, usually the head: drawing the stem upward must not move the reading)
        if let Some(s) = &self.pen.cur {
            let first = self.pen.pending.first().map(|f| &f.pts).unwrap_or(&s.pts);
            let r = ink_box(std::slice::from_ref(first));
            if r.is_positive() || first.len() == 1 {
                self.pen_guide(painter, if first.len() == 1 { first[0] } else { r.center() });
            }
        }
        for f in &self.pen.feedback {
            let age = (now - f.start) as f32;
            match &f.kind {
                FeedbackKind::Recognized { head } => {
                    // shrink toward the figure and fade, with a short blue glow on it
                    let t = (age / pen::RECOGNIZED_FADE as f32).clamp(0.0, 1.0);
                    let k = 1.0 - 0.5 * t;
                    let a = ((1.0 - t) * 255.0) as u8;
                    for s in &f.strokes {
                        let pts: Vec<Pos2> = s.iter().map(|p| *head + (*p - *head) * k).collect();
                        ink(painter, &pts, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), a));
                    }
                    let glow = (1.0 - (age / (pen::RECOGNIZED_FADE as f32 + 0.15)).clamp(0.0, 1.0)) * 70.0;
                    painter.circle_filled(*head, sp * 1.3, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), glow as u8));
                }
                FeedbackKind::Unrecognized => {
                    let fade = (1.0 - ((age - 1.6) / 0.4).clamp(0.0, 1.0)) * 255.0;
                    for s in &f.strokes {
                        ink(painter, s, Color32::from_rgba_unmultiplied(150, 150, 155, fade as u8));
                    }
                }
                FeedbackKind::Vanish => {
                    let fade = (1.0 - age / 0.5).clamp(0.0, 1.0) * 200.0;
                    for s in &f.strokes {
                        ink(painter, s, Color32::from_rgba_unmultiplied(150, 150, 155, fade as u8));
                    }
                }
            }
        }
        // ink still waiting (a lone accidental, the strokes of this symbol, and the stroke being drawn)
        if let Some(acc) = &self.pen.acc {
            for s in &acc.ink {
                ink(painter, s, INK);
            }
        }
        for s in &self.pen.pending {
            ink(painter, &s.pts, INK);
        }
        if let Some(s) = &self.pen.cur {
            ink(painter, &s.pts, INK);
        }
        if let Some((at, _)) = self.pen.balloon {
            self.pen_balloon(ui, at);
        }
        self.pen_alternatives(ui);
        if !self.pen.feedback.is_empty() || self.pen.cur.is_some() || !self.pen.pending.is_empty() || self.pen.alts.is_some() || self.pen.balloon.is_some() || self.pen.acc.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn pen_guide(&self, painter: &egui::Painter, p: Pos2) {
        let sp = self.geo.sp;
        let Some(l) = sv::locate(&self.score, &self.geo, p, self.grid()) else { return };
        let Some(sys) = self.geo.systems.iter().find(|s| s.measures.iter().any(|g| g.covers(l.measure)) && s.staff_y(l.staff).is_some()) else { return };
        let Some(mg) = sys.measures.iter().find(|g| g.covers(l.measure)) else { return };
        let mid = sys.staff_y(l.staff).unwrap_or(p.y);
        let mi = &self.score.measures[l.measure];
        let mt = mi.ticks();
        let beat = (TPQ * 4 / mi.den.max(1) as u32).max(1);
        let (top, bot) = (mid - sp * 3.0, mid + sp * 3.0);
        let faint = Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 70);
        let mut b = 0;
        while b < mt {
            let x = sv::x_for_tick(mg, b, mt);
            painter.extend(egui::Shape::dashed_line(&[pos2(x, top), pos2(x, bot)], EStroke::new(1.0, faint), 3.0, 3.0));
            b += beat;
        }
        // the target beat and the line or space
        let x = l.x;
        painter.rect_filled(Rect::from_center_size(pos2(x, mid), vec2(sp * 1.6, sp * 6.0)), 3.0, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 22));
        let y = mid - l.pos as f32 * sp * 0.5;
        painter.rect_filled(Rect::from_min_max(pos2(mg.content_x, y - sp * 0.25), pos2(mg.x1, y + sp * 0.25)), 0.0, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 26));
        // "G4 · beat 3"
        let clef = self.score.staves[l.staff].clef_at(l.measure);
        let key = mi.key;
        let pitch = Pitch { step: clef.middle_step() + l.pos, alter: key_alter(key, (clef.middle_step() + l.pos).rem_euclid(7) as usize) };
        let label = format!("{} · beat {}", pitch.name(), self.beat_of(l.measure, l.tick));
        let g = painter.layout_no_wrap(label, theme::font_semibold(13.0), Color32::WHITE);
        let r = Rect::from_min_size(pos2(x + sp * 1.6, top - 4.0), g.size() + vec2(18.0, 8.0));
        painter.rect_filled(r, r.height() / 2.0, INK);
        painter.galley(r.min + vec2(9.0, 4.0), g, Color32::WHITE);
    }

    fn pen_balloon(&mut self, ui: &mut Ui, at: Pos2) {
        let k = theme::t(ui);
        let mut open = false;
        egui::Area::new(egui::Id::unique("pen-balloon")).order(egui::Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(12, 7)).shadow(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(35) }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("? Not recognized ·").font(theme::font(14.0)).color(k.text));
                    if ui.add(egui::Label::new(egui::RichText::new("See gestures").font(theme::font(14.0)).color(k.accent)).sense(Sense::click())).clicked() {
                        open = true;
                    }
                });
            });
        });
        if open {
            self.pen.sheet = true;
            self.pen.balloon = None;
        }
    }

    fn pen_alternatives(&mut self, ui: &mut Ui) {
        let Some(a) = self.pen.alts.clone() else {
            self.pen.alts_rect = None;
            return;
        };
        let k = theme::t(ui);
        let sp = self.geo.sp;
        let at = pos2(a.at.x, a.at.y + sp * 3.0);
        let mut pick = None;
        let r = egui::Area::new(egui::Id::unique("pen-alternatives")).order(egui::Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(12.0).inner_margin(6.0).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (i, reading) in a.readings.iter().enumerate() {
                        let on = i == a.chosen;
                        let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                        if on {
                            ui.painter().rect_filled(r, 9.0, k.accent);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 9.0, k.cell_hover);
                        }
                        reading_figure(ui.painter(), r, reading, if on { Color32::WHITE } else { k.text });
                        let resp = resp.on_hover_text(reading.describe());
                        if resp.clicked() && !on {
                            pick = Some(i);
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "↶", theme::font(20.0), k.text);
                    if resp.clicked() {
                        pick = Some(usize::MAX);
                    }
                });
            });
        });
        self.pen.alts_rect = Some(r.response.rect);
        match pick {
            Some(usize::MAX) => self.pen_undo_figure(),
            Some(i) => self.pen_pick_alternative(i),
            None => {}
        }
    }

    /// Help ▸ Pencil Gestures, the status bar chip, or "See gestures"; with "Teach Fermata your handwriting" inside.
    pub fn pen_sheet(&mut self, ctx: &egui::Context) {
        if !self.pen.sheet {
            return;
        }
        self.pen_load();
        let k = theme::ctx_tokens(ctx);
        let mut open = true;
        let screen = ctx.content_rect();
        let now = ctx.input(|i| i.time);
        egui::Area::new(egui::Id::unique("pen-sheet")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(pos2(screen.right() - 16.0, screen.bottom() - 40.0)).show(ctx, |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(16.0).inner_margin(16.0).shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(50) }).show(ui, |ui| {
                ui.set_width(560.0f32.min(screen.width() - 40.0));
                let teaching = self.pen.calib.is_some();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(if teaching { "Teach Fermata your handwriting" } else { "Pencil gestures" }).font(theme::font_semibold(17.0)).color(k.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("×").font(theme::font(20.0)).color(k.text_2)).frame(false)).clicked() {
                            if teaching {
                                self.pen.calib = None;
                            } else {
                                open = false;
                            }
                        }
                    });
                });
                if teaching {
                    self.pen_calib_ui(ui, now);
                    return;
                }
                ui.add(egui::Label::new(egui::RichText::new("Write on the staff with the pen. Height = pitch, left–right = beat. Two-finger tap = Undo. Finger scrolls and zooms. Not what you meant? Tap the right figure under it: Fermata learns your handwriting.").font(theme::font(13.0)).color(k.text_2)).wrap());
                ui.add_space(6.0);
                let items: [(u8, &str, &str); 10] = [
                    (3, "Head + stem", "Quarter (filled) · half (open)"),
                    (1, "Open head, no stem", "Whole note"),
                    (5, "Stem + 1 to 4 flags", "Eighth to 64th, either side"),
                    (13, "Heads joined by beams", "1 beam 8ths · 2 16ths · 3 32nds"),
                    (14, "Dot after the head", "Dotted · double-dotted"),
                    (12, "Heads on one stem", "Chord"),
                    (0, "Zig-zag · 7 with hooks · block", "Rests, quarter to 64th · half · whole"),
                    (10, "♯ ♭ ♮ 𝄪 𝄫 before a head", "Accidental"),
                    (11, "Scribble over notes", "Deletes all it covers"),
                    (15, "Same vertical as a note", "Joins its chord"),
                ];
                for pair in items.chunks(2) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for (fig, title, sub) in pair.iter() {
                            egui::Frame::new().fill(k.cell).corner_radius(10.0).inner_margin(8.0).show(ui, |ui| {
                                ui.set_width(250.0);
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(56.0, 48.0), Sense::hover());
                                    ui.painter().rect_filled(r, 6.0, Color32::WHITE);
                                    for l in 0..5 {
                                        let y = r.top() + 8.0 + l as f32 * 8.0;
                                        ui.painter().line_segment([pos2(r.left() + 4.0, y), pos2(r.right() - 4.0, y)], EStroke::new(0.6, Color32::from_gray(190)));
                                    }
                                    sheet_figure(ui.painter(), r.center(), *fig);
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new(*title).font(theme::font_semibold(13.5)).color(k.text));
                                        ui.label(egui::RichText::new(*sub).font(theme::font(12.5)).color(k.text_3));
                                    });
                                });
                            });
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("✎  Teach Fermata your handwriting").font(theme::font_semibold(13.5)).color(Color32::WHITE)).fill(k.accent).corner_radius(8.0).min_size(vec2(0.0, 32.0))).clicked() {
                        self.pen_calib_start();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Label::new(egui::RichText::new("Export pen samples…").font(theme::font(12.5)).color(k.text_3).underline()).sense(Sense::click())).on_hover_text("Downloads your last strokes and what they were read as (only the points), to help tune the recognizer.").clicked() {
                            self.pen_export();
                        }
                    });
                });
                // the writer's models, kept in this browser only
                let n = self.pen.personal.count();
                if n > 0 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{n} models of your handwriting, kept only on this device ·")).font(theme::font(12.5)).color(k.text_3));
                        if ui.add(egui::Label::new(egui::RichText::new("Delete").font(theme::font(12.5)).color(k.danger)).sense(Sense::click())).clicked() {
                            self.pen.personal = Personal::default();
                            self.pen_save_personal();
                            self.status = "Your handwriting models were deleted.".into();
                        }
                    });
                }
            });
        });
        if !open {
            self.pen.sheet = false;
            self.pen.calib = None;
        }
    }

    /// The calibration inside the sheet: the figure to copy, the practice staff, and its buttons.
    fn pen_calib_ui(&mut self, ui: &mut Ui, now: f64) {
        let k = theme::t(ui);
        self.pen_calib_tick(now);
        let Some(c) = self.pen.calib.clone() else { return };
        let total = CALIB.len();
        if c.step >= total {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("All done: {} drawings learnt. Fermata now reads your handwriting first; corrections keep teaching it.", c.learnt)).font(theme::font(13.5)).color(k.text));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Teach again").clicked() {
                    self.pen.calib = Some(Calib::default());
                }
                if ui.button("Close").clicked() {
                    self.pen.calib = None;
                }
            });
            return;
        }
        let (name, _, glyph) = CALIB[c.step];
        ui.label(egui::RichText::new(format!("Draw each figure twice, as you normally write it. {} of {total} · {name} · {} time", c.step + 1, if c.rep == 0 { "1st" } else { "2nd" })).font(theme::font(13.0)).color(k.text_2));
        ui.add_space(6.0);
        // progress
        let (pr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
        ui.painter().rect_filled(pr, 2.0, k.cell);
        let done = (c.step * 2 + c.rep as usize) as f32 / (total * 2) as f32;
        ui.painter().rect_filled(Rect::from_min_size(pr.min, vec2(pr.width() * done, pr.height())), 2.0, k.accent);
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            // the figure to copy
            let (mr, _) = ui.allocate_exact_size(vec2(110.0, 150.0), Sense::hover());
            ui.painter().rect_filled(mr, 8.0, Color32::WHITE);
            staff_lines(ui.painter(), mr, CALIB_SP);
            ui.painter().text(pos2(mr.center().x, mr.center().y), Align2::CENTER_CENTER, glyph, FontId::new(CALIB_SP * 4.0, FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into())), Color32::from_gray(120));
            if name.starts_with("Dotted") {
                ui.painter().circle_filled(pos2(mr.center().x + CALIB_SP * 1.3, mr.center().y - CALIB_SP * 0.5), 2.2, Color32::from_gray(120));
            }
            ui.painter().text(pos2(mr.center().x, mr.bottom() - 10.0), Align2::CENTER_CENTER, name, theme::font(11.0), Color32::from_gray(110));
            // the practice staff
            let w = ui.available_width();
            let (pad, resp) = ui.allocate_exact_size(vec2(w, 150.0), Sense::drag());
            ui.painter().rect_filled(pad, 8.0, Color32::WHITE);
            ui.painter().rect_stroke(pad, 8.0, EStroke::new(1.0, theme::alpha(k.accent, 0.5)), egui::StrokeKind::Inside);
            staff_lines(ui.painter(), pad, CALIB_SP);
            if c.strokes.is_empty() && c.cur.is_none() {
                ui.painter().text(pos2(pad.center().x, pad.bottom() - 12.0), Align2::CENTER_CENTER, "Write here with the pen", theme::font(11.5), Color32::from_gray(150));
            }
            // the pen: its own samples (every point the browser gives); a finger or a mouse: egui's drag
            let pos = resp.interact_pointer_pos();
            let loose = std::mem::take(&mut self.pen.loose);
            let pen_active = self.pen.pen_active;
            let mut finished = vec![];
            if let Some(cal) = self.pen.calib.as_mut() {
                for (ph, p) in loose {
                    match ph {
                        0 if pad.contains(p) => {
                            cal.cur = Some(vec![p]);
                            cal.pen_drawing = true;
                        }
                        1 if cal.pen_drawing => {
                            if let Some(cur) = cal.cur.as_mut() {
                                if cur.last().map(|q| q.distance(p) >= 0.6).unwrap_or(true) {
                                    cur.push(p);
                                }
                            }
                        }
                        2 if cal.pen_drawing => {
                            cal.pen_drawing = false;
                            if let Some(mut cur) = cal.cur.take() {
                                cur.push(p);
                                finished.push(cur);
                            }
                        }
                        _ => {}
                    }
                }
                if !cal.pen_drawing && !pen_active {
                    if resp.drag_started() {
                        cal.cur = pos.map(|p| vec![p]);
                    } else if resp.dragged() {
                        if let (Some(cur), Some(p)) = (cal.cur.as_mut(), pos) {
                            if cur.last().map(|q| q.distance(p) >= 0.6).unwrap_or(true) {
                                cur.push(p);
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        if let Some(cur) = cal.cur.take() {
                            finished.push(cur);
                        }
                    }
                }
            }
            for cur in finished {
                self.pen_calib_stroke(cur, now);
            }
            if let Some(cal) = self.pen.calib.as_ref() {
                let painter = ui.painter_at(pad.shrink(2.0));
                for s in cal.strokes.iter().chain(cal.cur.iter()) {
                    ink(&painter, s, INK);
                }
                if !cal.strokes.is_empty() || cal.cur.is_some() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                if let Some(cal) = self.pen.calib.as_mut() {
                    cal.strokes.clear();
                    cal.cur = None;
                }
            }
            if ui.button("Skip").clicked() {
                if let Some(cal) = self.pen.calib.as_mut() {
                    cal.step += 1;
                    cal.rep = 0;
                    cal.strokes.clear();
                }
            }
            if c.step > 0 && ui.button("Back").clicked() {
                if let Some(cal) = self.pen.calib.as_mut() {
                    cal.step -= 1;
                    cal.rep = 0;
                    cal.strokes.clear();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    self.pen.calib = None;
                }
                let n = self.pen.personal.count();
                if n > 0 && ui.add(egui::Label::new(egui::RichText::new("Delete my handwriting").font(theme::font(12.5)).color(k.danger)).sense(Sense::click())).clicked() {
                    self.pen.personal = Personal::default();
                    self.pen_save_personal();
                    self.status = "Your handwriting models were deleted.".into();
                }
            });
        });
    }

    /// The status bar chip (tablets in the browser): what the pen and the finger do; a tap opens the gestures, or
    /// turns Pen Mode on where no pen has been seen.
    pub fn pen_chip(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        let mode = self.prefs.pen_mode;
        let text = if mode {
            "✎ Pen Mode: On · touch writes · 2 fingers scroll"
        } else if self.pen.pen_seen {
            "✎ Pencil writes · finger scrolls  ⓘ"
        } else {
            "✎ Pen Mode: Off · tap to write with a passive pen"
        };
        let g = ui.painter().layout_no_wrap(text.to_string(), theme::font_semibold(11.5), if mode { Color32::WHITE } else { k.accent });
        let (r, resp) = ui.allocate_exact_size(g.size() + vec2(18.0, 6.0), Sense::click());
        ui.painter().rect_filled(r, r.height() / 2.0, if mode { k.accent } else { theme::alpha(k.accent, 0.12) });
        ui.painter().galley(r.min + vec2(9.0, 3.0), g, Color32::WHITE);
        if resp.clicked() {
            if mode {
                self.prefs.pen_mode = false;
                crate::prefs::save(&self.prefs);
                self.status = "Pen Mode off: one finger works the tool again.".into();
            } else if self.pen.pen_seen {
                self.pen.sheet = !self.pen.sheet;
            } else {
                self.prefs.pen_mode = true;
                crate::prefs::save(&self.prefs);
                self.status = "Pen Mode on: touching the staff writes; scroll with two fingers.".into();
            }
        }
    }
}

/// "a" or "an" before a word ("an eighth note", "a 16th note").
fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u', '8']) || word.starts_with("11") {
        "an"
    } else {
        "a"
    }
}

fn ink(painter: &egui::Painter, pts: &[Pos2], col: Color32) {
    if pts.len() == 1 {
        painter.circle_filled(pts[0], INK_WIDTH / 2.0, col);
    } else if pts.len() > 1 {
        painter.add(egui::Shape::line(pts.to_vec(), EStroke::new(INK_WIDTH, col)));
    }
}

fn staff_lines(p: &egui::Painter, r: Rect, sp: f32) {
    for l in -2..=2 {
        let y = r.center().y + l as f32 * sp;
        p.line_segment([pos2(r.left() + 8.0, y), pos2(r.right() - 8.0, y)], EStroke::new(0.8, Color32::from_gray(180)));
    }
}

fn bravura(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into()))
}

fn note_glyph(value: u8) -> char {
    match value {
        1 => '\u{E1D2}',
        2 => '\u{E1D3}',
        4 => '\u{E1D7}',
        5 => '\u{E1D9}',
        6 => '\u{E1DB}',
        7 => '\u{E1DD}',
        _ => '\u{E1D5}',
    }
}

fn rest_glyph(value: u8) -> char {
    match value {
        1 => '\u{E4E3}',
        2 => '\u{E4E4}',
        4 => '\u{E4E6}',
        5 => '\u{E4E7}',
        6 => '\u{E4E8}',
        7 => '\u{E4E9}',
        _ => '\u{E4E5}',
    }
}

/// A reading as a small figure in a 44-point cell.
fn reading_figure(p: &egui::Painter, r: Rect, reading: &Reading, col: Color32) {
    let c = r.center();
    let dots = |n: u8, x: f32, y: f32| {
        for d in 0..n {
            p.circle_filled(pos2(x + 4.0 * d as f32, y), 1.8, col);
        }
    };
    match reading {
        Reading::Notes { cols, beamed: true } => {
            p.text(c + vec2(-3.0, 4.0), Align2::CENTER_CENTER, note_glyph(cols[0].value), bravura(22.0), col);
            p.text(pos2(r.right() - 9.0, r.top() + 10.0), Align2::CENTER_CENTER, format!("×{}", cols.len()), theme::font(10.0), col);
        }
        Reading::Notes { cols, .. } => {
            let v = cols[0].value;
            let x = if cols[0].dots > 0 { -4.0 } else { 0.0 };
            p.text(c + vec2(x, if v == 1 { 0.0 } else { 6.0 }), Align2::CENTER_CENTER, note_glyph(v), bravura(26.0), col);
            dots(cols[0].dots, c.x + x + 9.0, c.y + if v == 1 { 0.0 } else { 9.0 });
        }
        Reading::Rest { value, dots: d, .. } => {
            p.text(c + vec2(if *d > 0 { -4.0 } else { 0.0 }, 0.0), Align2::CENTER_CENTER, rest_glyph(*value), bravura(26.0), col);
            dots(*d, c.x + 7.0, c.y - 2.0);
        }
        Reading::Accidental { alter, .. } => {
            let g = match alter {
                2 => '\u{E263}',
                1 => '\u{E262}',
                -1 => '\u{E260}',
                -2 => '\u{E264}',
                _ => '\u{E261}',
            };
            p.text(c, Align2::CENTER_CENTER, g, bravura(26.0), col);
        }
        Reading::Dot { .. } => {
            p.circle_filled(c, 2.5, col);
        }
    }
}

/// The little drawings of the gesture sheet, in ink blue.
fn sheet_figure(p: &egui::Painter, c: Pos2, fig: u8) {
    let st = EStroke::new(2.0, INK);
    match fig {
        0 => {
            p.text(c, Align2::CENTER_CENTER, rest_glyph(4), bravura(26.0), INK);
        }
        1..=7 => {
            p.text(c + vec2(0.0, if fig == 1 { 0.0 } else { 6.0 }), Align2::CENTER_CENTER, note_glyph(fig), bravura(26.0), INK);
        }
        10 => {
            p.text(c + vec2(-10.0, 0.0), Align2::CENTER_CENTER, "♯", theme::font_semibold(22.0), INK);
            p.circle_filled(c + vec2(9.0, 2.0), 4.0, INK);
        }
        11 => {
            let pts: Vec<Pos2> = (0..=10).map(|i| c + vec2(-18.0 + i as f32 * 3.6, if i % 2 == 0 { -9.0 } else { 9.0 })).collect();
            p.circle_filled(c + vec2(-8.0, 0.0), 4.0, Color32::BLACK);
            p.circle_filled(c + vec2(8.0, 0.0), 4.0, Color32::BLACK);
            p.add(egui::Shape::line(pts, st));
        }
        12 => {
            for y in [8.0, 0.0, -8.0] {
                p.circle_filled(c + vec2(-3.0, y), 3.8, INK);
            }
            p.line_segment([c + vec2(1.0, 8.0), c + vec2(1.0, -20.0)], st);
        }
        13 => {
            for x in [-12.0, 0.0, 12.0] {
                p.circle_filled(c + vec2(x - 3.0, 10.0), 3.8, INK);
                p.line_segment([c + vec2(x + 0.5, 10.0), c + vec2(x + 0.5, -14.0)], st);
            }
            p.line_segment([c + vec2(-11.0, -14.0), c + vec2(12.5, -14.0)], EStroke::new(3.0, INK));
            p.line_segment([c + vec2(-11.0, -9.0), c + vec2(12.5, -9.0)], EStroke::new(3.0, INK));
        }
        14 => {
            p.text(c + vec2(-4.0, 6.0), Align2::CENTER_CENTER, note_glyph(3), bravura(26.0), INK);
            p.circle_filled(c + vec2(6.0, 8.0), 2.4, INK);
        }
        _ => {
            p.circle_filled(c + vec2(0.0, 6.0), 4.0, Color32::BLACK);
            p.circle_filled(c + vec2(0.0, -6.0), 4.0, INK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::samples::*;
    use std::collections::HashSet;

    /// A treble staff laid out and drawn as the score area does (so the pen sees the notes' hit areas).
    pub(crate) fn drawn(a: &mut App) {
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

    pub(crate) fn app() -> App {
        let mut a = App::new(true);
        a.voice = 1;
        drawn(&mut a);
        a
    }

    /// Where beat `tick` of measure `m` is, on the line or space `pos` half-spaces above the top staff's middle line.
    fn spot_m(a: &App, m: usize, tick: u32, pos: i32) -> Pos2 {
        let sys = a.geo.systems.iter().find(|s| s.measures.iter().any(|g| g.covers(m))).unwrap();
        let mg = sys.measures.iter().find(|g| g.covers(m)).unwrap();
        let x = sv::x_for_tick(mg, tick, a.score.measures[m].ticks());
        pos2(x, sys.staves[0].1 - pos as f32 * a.geo.sp * 0.5)
    }

    fn spot(a: &App, tick: u32, pos: i32) -> Pos2 {
        spot_m(a, 0, tick, pos)
    }

    /// Draws strokes with the pen: each takes 0.25 s, `gap` seconds apart. Returns when the pen last lifted.
    fn write(a: &mut App, strokes: &[Vec<Pos2>], t0: f64, gap: f64) -> f64 {
        let mut t = t0;
        for s in strokes {
            a.pen_feed(0, s[0], t);
            for p in &s[1..] {
                a.pen_feed(1, *p, t + 0.1);
            }
            a.pen_feed(2, *s.last().unwrap(), t + 0.25);
            a.pen_tick(t + 0.25);
            t += 0.25 + gap;
        }
        t - gap
    }

    /// Writes strokes as one symbol and lets the pause pass.
    fn symbol(a: &mut App, strokes: &[Vec<Pos2>], t0: f64) -> f64 {
        let up = write(a, strokes, t0, 0.2);
        a.pen_tick(up + pen::DEBOUNCE + 0.05);
        drawn(a);
        up + 1.0
    }

    fn notes(a: &App) -> Vec<(u32, u8, Vec<i32>)> {
        notes_m(a, 0)
    }

    fn notes_m(a: &App, m: usize) -> Vec<(u32, u8, Vec<i32>)> {
        a.score.events[0][m].iter().map(|e| (e.start, e.value, e.pitches.iter().map(|p| p.midi()).collect())).collect()
    }

    #[test]
    fn head_and_stem_half_a_second_apart_are_one_quarter_note_on_g4_beat_3() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, 2 * TPQ, -2);
        let up = write(&mut a, &[head(h, sp, true, 5), stem(h, sp, true, 5)], 10.0, 0.5);
        a.pen_tick(up + 0.5);
        assert!(notes(&a).is_empty(), "nothing before the pause ends");
        assert_eq!(a.pen.pending.len(), 2, "both strokes wait as one symbol");
        a.pen_tick(up + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a), vec![(2 * TPQ, 3, vec![67])]);
        assert!(a.pen.alts.is_some());
        a.pen_tick(up + pen::DEBOUNCE + pen::ALTERNATIVES_SHOW + 0.1);
        assert!(a.pen.alts.is_none());
    }

    #[test]
    fn a_second_apart_they_are_two_symbols() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, TPQ, -2);
        // the head alone (a filled head is a quarter note), then a stem alone a second later: nothing more
        let up = write(&mut a, &[head(h, sp, true, 2)], 1.0, 0.0);
        a.pen_tick(up + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a).len(), 1, "the head was read alone");
        let up = write(&mut a, &[stem(h, sp, true, 2)], up + 1.0, 0.0);
        a.pen_tick(up + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a).len(), 1, "a stem alone adds nothing");
        assert!(a.pen.feedback.iter().any(|f| f.kind == FeedbackKind::Unrecognized));
    }

    #[test]
    fn nothing_is_inserted_when_it_is_clearly_not_a_note() {
        let mut a = app();
        let sp = a.geo.sp;
        for (i, s) in [long_line(spot(&a, TPQ, 0), sp, 4), big_loop(spot(&a, TPQ, 0), sp, 4)].into_iter().enumerate() {
            let up = write(&mut a, &[s], 1.0 + i as f64 * 5.0, 0.0);
            a.pen_tick(up + pen::DEBOUNCE + 0.01);
            assert!(notes(&a).is_empty());
            assert!(a.pen.balloon.is_some(), "Not recognized · See gestures");
            a.pen_tick(up + pen::DEBOUNCE + pen::UNRECOGNIZED_SHOW + 0.1);
            assert!(a.pen.balloon.is_none(), "gone after 2 s");
        }
    }

    #[test]
    fn a_crooked_note_is_written_and_a_doubtful_one_is_guessed() {
        let mut a = app();
        let sp = a.geo.sp;
        // a crooked eighth: open-ish spiral head, stem apart and leaning, a short hook
        let g4 = spot(&a, 0, -2);
        let t = symbol(&mut a, &note_t(g4, sp, 4, 0, false, hands()[1]), 1.0);
        assert_eq!(notes(&a), vec![(0, 4, vec![67])]);
        // a note-sized curl: the best guess goes in, with the alternatives offered
        let c = loose_curl(spot(&a, 2 * TPQ, 0), sp, 3);
        let _ = symbol(&mut a, &[c], t);
        assert_eq!(notes(&a).len(), 2, "the guess was written");
        assert!(a.pen.alts.as_ref().is_some_and(|x| x.readings.len() >= 3));
    }

    #[test]
    fn chord_accidental_rest_dot_and_alternatives() {
        let mut a = app();
        let sp = a.geo.sp;
        let g4 = spot(&a, 0, -2);
        let t = symbol(&mut a, &[head(g4, sp, true, 1), stem(g4, sp, true, 1)], 1.0);
        // a head on B4 in the same vertical: a chord
        let b4 = pos2(g4.x + sp * 0.3, g4.y - sp);
        let t = symbol(&mut a, &[head(b4, sp, true, 6)], t);
        assert_eq!(notes(&a), vec![(0, 3, vec![67, 71])]);
        // ♯ then a head on F4 at beat 2: F♯4
        let f4 = spot(&a, TPQ, -3);
        let mut strokes = sharp(pos2(f4.x - 2.0 * sp, f4.y), sp, 3);
        strokes.push(head(f4, sp, true, 3));
        let t = symbol(&mut a, &strokes, t);
        assert_eq!(notes(&a)[1], (TPQ, 3, vec![66]));
        // a quarter rest on beat 3
        let r = quarter_rest(spot(&a, 2 * TPQ, 0), sp, 8);
        let t = symbol(&mut a, &[r], t);
        assert_eq!(notes(&a)[2], (2 * TPQ, 3, vec![]));
        // "was it an eighth?": one tap on the alternatives
        let h4 = head(spot(&a, 3 * TPQ, -2), sp, true, 9);
        let t = symbol(&mut a, &[h4], t);
        assert_eq!(notes(&a)[3], (3 * TPQ, 3, vec![67]));
        let i = a.pen.alts.as_ref().unwrap().readings.iter().position(|r| r.label() == "n4.0").expect("eighth offered");
        a.pen_pick_alternative(i);
        assert_eq!(notes(&a)[3], (3 * TPQ, 4, vec![67]), "changed to an eighth note");
        // a dot after the eighth on beat 4: a dotted eighth (it still fits)
        drawn(&mut a);
        let e = a.geo.notes.iter().find(|h| h.r.idx == 3 && h.pitch.is_some()).unwrap().rect;
        let _ = symbol(&mut a, &[dot(pos2(e.right() + 0.6 * sp, e.center().y), sp, 1)], t);
        assert_eq!(a.score.events[0][0][3].dots, 1);
    }

    #[test]
    fn a_scribble_deletes_everything_it_covers_and_nothing_elsewhere() {
        let mut a = app();
        let sp = a.geo.sp;
        let mut t = 1.0;
        for (k, pos) in [(0u32, -2), (1, 0), (2, -1)] {
            let h = spot(&a, k * TPQ, pos);
            t = symbol(&mut a, &[head(h, sp, true, k + 1), stem(h, sp, true, k + 1)], t);
        }
        assert_eq!(notes(&a).len(), 3);
        // over empty space: the ink goes, nothing changes
        let empty = spot_m(&a, 1, 2 * TPQ, 0);
        t = symbol(&mut a, &[scribble_dir(empty, sp, 0.4, 3.0, 5, 2)], t);
        assert_eq!(notes(&a).len(), 3);
        assert!(a.pen.feedback.iter().any(|f| f.kind == FeedbackKind::Vanish));
        // a long horizontal scribble across the three notes: all three go, as one undo step
        let hs: Vec<Rect> = a.geo.notes.iter().filter(|h| h.r.measure == 0).map(|h| h.rect).collect();
        let (x0, x1) = (hs.iter().map(|r| r.left()).fold(f32::MAX, f32::min), hs.iter().map(|r| r.right()).fold(f32::MIN, f32::max));
        let y = hs.iter().map(|r| r.center().y).sum::<f32>() / hs.len() as f32;
        let len = (x1 - x0) / sp + 1.0;
        let undo = a.undo.len();
        let _ = symbol(&mut a, &[scribble_dir(pos2((x0 + x1) / 2.0, y), sp, 0.0, len, 6, 5)], t);
        assert!(notes(&a).is_empty(), "left: {:?}", notes(&a));
        assert_eq!(a.undo.len(), undo + 1);
        a.do_undo();
        assert_eq!(notes(&a).len(), 3);
    }

    #[test]
    fn a_quarter_rest_on_empty_space_is_a_rest_not_a_scribble() {
        let mut a = app();
        let sp = a.geo.sp;
        for (i, hand) in hands().into_iter().enumerate() {
            let at = spot(&a, i as u32 * TPQ, 0);
            let _ = symbol(&mut a, &[quarter_rest_t(at, sp, hand)], 1.0 + i as f64 * 3.0);
        }
        assert_eq!(notes(&a).iter().filter(|n| n.2.is_empty() && n.1 == 3).count(), 3, "{:?}", notes(&a));
    }

    #[test]
    fn a_beamed_group_is_written_beamed() {
        let mut a = app();
        let sp = a.geo.sp;
        let at = spot(&a, 0, -2);
        let _ = symbol(&mut a, &beamed_t(at, sp, 4, 2, false, hands()[0]), 1.0);
        let n = notes(&a);
        assert_eq!(n.len(), 4, "{n:?}");
        assert!(n.iter().all(|x| x.1 == 5), "sixteenths: {n:?}");
        assert!(a.score.events[0][0][..3].iter().all(|e| e.beam == BeamMode::Join));
    }

    #[test]
    fn a_correction_teaches_the_same_stroke() {
        let mut a = app();
        let sp = a.geo.sp;
        // a half note in another hand: two arcs for the head, the stem on the left. The rules see a ♭ in it; no
        // head follows, so after the wait their best note guess goes in, with the alternatives under it
        let h = spot(&a, 0, -2);
        let t = symbol(&mut a, &styled_note(h, sp, 2, 7), 1.0);
        a.pen_tick(t + pen::ACCIDENTAL_WAIT + 0.1);
        let wrong = notes(&a);
        assert_ne!(wrong, vec![(0, 2, vec![67])], "the rules alone get it wrong");
        let alts = a.pen.alts.clone().expect("alternatives offered");
        // the writer taps the half note among the alternatives
        let i = alts.readings.iter().position(|r| matches!(r, Reading::Notes { cols, .. } if cols[0].value == 2)).expect("a half note offered");
        a.pen_pick_alternative(i);
        assert_eq!(notes(&a), vec![(0, 2, vec![67])]);
        assert!(a.pen.personal.count() > 0);
        assert_eq!(a.pen.samples.last().and_then(|s| s.corrected.clone()), Some("n2.0".into()));
        // the same handwriting again, on beat 3: read right at once
        let h2 = spot(&a, 2 * TPQ, -2);
        let _ = symbol(&mut a, &styled_note(h2, sp, 2, 8), t + 5.0);
        assert_eq!(notes(&a).get(1), Some(&(2 * TPQ, 2, vec![67])), "{:?}", notes(&a));
    }

    #[test]
    fn teaching_the_handwriting_makes_another_hand_readable() {
        // without models: this hand is misread; after the calibration (each figure twice), it's read right
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, 0, -2);
        let _ = symbol(&mut a, &styled_note(h, sp, 2, 31), 1.0);
        assert_ne!(notes(&a), vec![(0, 2, vec![67])], "misread without models");
        a.score.events[0][0].clear();
        drawn(&mut a);
        a.pen.alts = None;
        a.pen_calib_start();
        let mut t = 10.0;
        let pad = pos2(400.0, 400.0);
        while let Some(c) = a.pen.calib.clone() {
            if c.step >= CALIB.len() {
                break;
            }
            let label = CALIB[c.step].1;
            let seed = 40 + c.step as u32 * 2 + c.rep as u32;
            let strokes: Vec<Vec<Pos2>> = match label {
                "n1.0" => styled_note(pad, CALIB_SP, 1, seed),
                "n2.0" => styled_note(pad, CALIB_SP, 2, seed),
                "n3.0" => styled_note(pad, CALIB_SP, 3, seed),
                "n4.0" => styled_note(pad, CALIB_SP, 4, seed),
                // the other figures: skipped by this writer
                _ => {
                    a.pen.calib.as_mut().unwrap().step += 1;
                    continue;
                }
            };
            for s in strokes {
                a.pen_calib_stroke(s, t);
            }
            t += pen::DEBOUNCE + 0.1;
            a.pen_calib_tick(t);
        }
        assert!(a.pen.personal.count() >= 8);
        a.pen.calib = None;
        let mut t = 100.0;
        for (m, v) in [(0usize, 2u8), (1, 1)] {
            let at = spot_m(&a, m, 0, -2);
            t = symbol(&mut a, &styled_note(at, sp, v, 77 + m as u32), t);
        }
        assert_eq!(notes(&a), vec![(0, 2, vec![67])]);
        assert_eq!(notes_m(&a, 1), vec![(0, 1, vec![67])]);
    }

    #[test]
    fn export_holds_only_points_and_readings() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = head(spot(&a, 0, -2), sp, true, 1);
        let _ = symbol(&mut a, &[h], 1.0);
        let j: serde_json::Value = serde_json::from_str(&a.pen_export_json()).unwrap();
        assert_eq!(j["format"], "fermata-pen-samples");
        let s = &j["samples"][0];
        let keys: Vec<&String> = s.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["corrected", "read", "strokes"]);
        assert_eq!(s["read"], "n3.0");
    }
}

#[cfg(test)]
mod balloon_tests {
    use super::*;
    /// "? Not recognized · See gestures" is drawn while the balloon is up.
    #[test]
    fn the_not_recognized_balloon_is_drawn() {
        let mut a = App::new(true);
        a.pen.balloon = Some((pos2(300.0, 300.0), 1e9));
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut found = false;
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let p = ui.painter().clone();
                a.pen_draw(ui, &p, 0.0);
            });
            out.textures_delta.clear();
            for c in &out.shapes {
                if let egui::Shape::Text(t) = &c.shape {
                    if t.galley.text().contains("Not recognized") {
                        found = true;
                    }
                }
            }
        }
        assert!(found, "balloon text not drawn; balloon state {:?}", a.pen.balloon);
    }
}

#[cfg(test)]
mod toolbar_tests {
    use super::*;
    use crate::app::Tool;

    /// The tablet's toolbar and score, as the app lays them out; the pen's samples are handed in as the browser's
    /// would be, together with the touch egui gets for them.
    fn frame(ctx: &egui::Context, a: &mut App, pen: &[(u8, Pos2)], t: f64) {
        a.pen.injected = pen.iter().map(|(ph, p)| (*ph, p.x, p.y)).collect();
        let mut events = vec![];
        for (ph, p) in pen {
            events.push(egui::Event::PointerMoved(*p));
            match ph {
                0 => events.push(egui::Event::PointerButton { pos: *p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }),
                2 => events.push(egui::Event::PointerButton { pos: *p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }),
                _ => {}
            }
        }
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1180.0, 820.0))), events, time: Some(t), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::Panel::top("score_toolbar").exact_size(58.0).show(ui, |ui| a.score_toolbar(ui));
            egui::CentralPanel::default().show(ui, |ui| a.score_area(ui));
        });
        out.textures_delta.clear();
    }

    /// The Arrow, Eraser and Pencil buttons (the first three square buttons of the toolbar, left to right).
    fn tool_buttons(ctx: &egui::Context) -> Vec<Rect> {
        let mut rs: Vec<Rect> = ctx.viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter().filter(|w| w.sense.senses_click() && w.rect.top() < 58.0 && (40.0..=48.0).contains(&w.rect.width()) && (40.0..=48.0).contains(&w.rect.height())).map(|w| w.rect)).collect());
        rs.sort_by(|a, b| a.left().partial_cmp(&b.left()).unwrap());
        rs.dedup_by(|a, b| (a.left() - b.left()).abs() < 1.0);
        rs.truncate(3);
        rs
    }

    #[test]
    fn a_pen_tap_on_the_toolbar_changes_the_tool_and_ink_stays_on_the_page() {
        crate::kit::set_touch(true);
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut a = App::new(true);
        a.device = crate::touch::Device::Touch;
        a.voice = 1;
        a.tool = Tool::Pencil;
        let mut t = 1.0;
        for _ in 0..3 {
            frame(&ctx, &mut a, &[], t);
            t += 0.05;
        }
        let b = tool_buttons(&ctx);
        assert_eq!(b.len(), 3, "the tool buttons: {b:?}");
        // the pen taps the Arrow: the tool changes, and no ink is left on the toolbar
        let p = b[0].center();
        frame(&ctx, &mut a, &[(0, p)], t);
        frame(&ctx, &mut a, &[(2, p)], t + 0.08);
        frame(&ctx, &mut a, &[], t + 0.2);
        assert_eq!(a.tool, Tool::Arrow, "the pen's tap chose the Arrow");
        assert!(a.pen.cur.is_none() && a.pen.pending.is_empty(), "no ink outside the page");
        // the Pencil again, with the pen: then the pen writes on the staff
        let p = b[2].center();
        frame(&ctx, &mut a, &[(0, p)], t + 0.5);
        frame(&ctx, &mut a, &[(2, p)], t + 0.58);
        frame(&ctx, &mut a, &[], t + 0.7);
        assert_eq!(a.tool, Tool::Pencil);
        let sys = &a.geo.systems[0];
        let on_staff = pos2(sys.measures[0].x0 + (sys.measures[0].x1 - sys.measures[0].x0) * 0.5, sys.staves[0].1);
        let stroke: Vec<(u8, Pos2)> = (0..8).map(|i| (if i == 0 { 0 } else { 1 }, on_staff + vec2(i as f32 * 2.0, i as f32))).collect();
        frame(&ctx, &mut a, &stroke, t + 1.0);
        assert!(a.pen.cur.is_some(), "ink starts on the page");
        frame(&ctx, &mut a, &[(2, on_staff + vec2(16.0, 8.0))], t + 1.1);
        assert_eq!(a.pen.pending.len(), 1, "one stroke waits to be read");
        assert_eq!(a.tool, Tool::Pencil);
        crate::kit::set_touch(false);
    }
}
