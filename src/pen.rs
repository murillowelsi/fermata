//! F-38/F-39: writing with a pen (Apple Pencil, S Pen, USI, or a passive pen in Pen Mode): the recognizer.
//!
//! Strokes that come close together in time are one symbol: after the pen lifts, the ink waits `DEBOUNCE` seconds;
//! a new stroke in that time joins the same symbol and starts the wait again. Then the group is read offline, with
//! Fermata's own code, **in parts**:
//! 1. each stroke is cut (where that reads best) into components: a head (any small loop, closed or nearly, or a
//!    blot the size of a head; filled when the ink is dense or goes round more than once), a stem (a near-vertical
//!    line), a flag or hook (a short curve or tick), a beam (a longer slanting line), a dot, a block (a short, flat
//!    filled bar);
//! 2. the figure is put together from how those components sit: heads on a stem's end (several = a chord), flags on
//!    its other end (on either side), beams joining stem ends, dots after a head, an accidental before it;
//! 3. rests and accidentals are read from the whole group: the quarter rest as a tall zigzag, the hooked rests as a
//!    slanting line with 1 to 4 hooks on its left, the whole and half rests as a block hanging from the fourth line or
//!    sitting on the third, and ♯ ♭ ♮ 𝄪 𝄫 with a point-cloud matcher written from the published description of the
//!    $P recognizer (Vatavu, Anthony and Wobbrock, 2012).
//!
//! Every reading gets a score; the context (the time left in the bar, the value of the neighbouring note, the height
//! on the staff) breaks ties; the best one is written and the others are offered as alternatives. The writer's own
//! models (`Personal`), learnt from corrections and from "Teach Fermata your handwriting", count per component and
//! per figure. Every limit below is a starting point to tune with real strokes (see "Export pen samples…").
use egui::{pos2, vec2, Pos2, Rect};
use serde::{Deserialize, Serialize};

/// Seconds the ink waits after the pen lifts before it is read; a new stroke in that time joins the same symbol.
pub const DEBOUNCE: f64 = 0.7;
/// Ink turning into the note that was written.
pub const RECOGNIZED_FADE: f64 = 0.2;
/// Grey ink and "Not recognized" stay this long.
pub const UNRECOGNIZED_SHOW: f64 = 2.0;
/// The alternatives under a new note stay this long (or until the next note).
pub const ALTERNATIVES_SHOW: f64 = 3.0;
/// A two-finger tap shorter than this, without moving, is Undo.
pub const TWO_FINGER_TAP: f64 = 0.25;
/// …and moving less than this many points.
pub const TWO_FINGER_SLOP: f32 = 12.0;
/// A head within this many staff spaces of an existing note's x joins its chord.
pub const CHORD_DX: f32 = 0.8;
/// An accidental written alone waits this long for the head it belongs to, at most this far to its left (spaces).
pub const ACCIDENTAL_WAIT: f64 = 1.5;
pub const ACCIDENTAL_REACH: f32 = 2.5;
/// A scribble turns back at least this many times (along any direction).
pub const SCRIBBLE_TURNS: usize = 3;
/// The writer's models kept per label (the oldest go first), and the samples kept for "Export pen samples…".
pub const PERSONAL_PER_LABEL: usize = 6;
pub const SAMPLES_KEPT: usize = 100;
/// How many readings the alternatives offer.
pub const ALTERNATIVES_MAX: usize = 6;

/// One stroke: its points (screen points, y down) and when the pen touched and lifted (seconds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub pts: Vec<Pos2>,
    pub t0: f64,
    pub t1: f64,
}

/// Groups strokes (in the order drawn) into symbols (the rule `App::pen_tick` follows live; the tests check it here):
/// a stroke that starts less than `gap` seconds after the previous one lifted belongs to the same symbol.
#[cfg_attr(not(test), allow(dead_code))]
pub fn group_by_time(times: &[(f64, f64)], gap: f64) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![];
    for (i, &(t0, _)) in times.iter().enumerate() {
        let joins = i > 0 && t0 - times[i - 1].1 < gap;
        if joins {
            out.last_mut().unwrap().push(i);
        } else {
            out.push(vec![i]);
        }
    }
    out
}

// ---------------- readings ----------------

/// One chord position of a written group: its heads (screen points), value (1 whole … 7 sixty-fourth), dots, and an
/// accidental drawn before it (for the head `acc_head`).
#[derive(Clone, Debug, PartialEq)]
pub struct Col {
    pub heads: Vec<Pos2>,
    pub value: u8,
    pub dots: u8,
    pub acc: Option<i8>,
    pub acc_head: usize,
}

/// What a group of strokes reads as (positions in screen points).
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    /// Notes: one column, a chord (several heads in a column), or a beamed group (several columns).
    Notes { cols: Vec<Col>, beamed: bool },
    Rest { value: u8, dots: u8, at: Pos2 },
    /// ♯ ♭ ♮ 𝄪 𝄫 alone: it waits for its head.
    Accidental { alter: i8, at: Pos2, right: f32 },
    /// A dot alone (after an existing note: it dots it).
    Dot { at: Pos2 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub reading: Reading,
    pub score: f32,
}

pub fn value_name(value: u8) -> &'static str {
    match value {
        1 => "whole note",
        2 => "half note",
        3 => "quarter note",
        4 => "eighth note",
        5 => "16th note",
        6 => "32nd note",
        7 => "64th note",
        _ => "note",
    }
}

pub fn rest_name(value: u8) -> &'static str {
    match value {
        1 => "whole rest",
        2 => "half rest",
        3 => "quarter rest",
        4 => "eighth rest",
        5 => "16th rest",
        6 => "32nd rest",
        7 => "64th rest",
        _ => "rest",
    }
}

impl Reading {
    /// A short label for models and samples ("n3.1" a dotted quarter, "r4.0" an eighth rest, "a-1" a flat,
    /// "g3x5" three beamed 16ths); None for what isn't one figure.
    pub fn label(&self) -> String {
        match self {
            Reading::Notes { cols, beamed } if cols.len() == 1 => format!("n{}.{}", cols[0].value, cols[0].dots),
            Reading::Notes { cols, beamed: true } => format!("g{}x{}", cols.len(), cols[0].value),
            Reading::Notes { cols, .. } => format!("c{}", cols.len()),
            Reading::Rest { value, dots, .. } => format!("r{value}.{dots}"),
            Reading::Accidental { alter, .. } => format!("a{alter}"),
            Reading::Dot { .. } => "dot".into(),
        }
    }

    /// Where the alternatives go (under this point).
    pub fn anchor(&self) -> Pos2 {
        match self {
            Reading::Notes { cols, .. } => {
                let n = cols.iter().map(|c| c.heads.len()).sum::<usize>().max(1) as f32;
                let s = cols.iter().flat_map(|c| c.heads.iter()).fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2());
                (s / n).to_pos2()
            }
            Reading::Rest { at, .. } | Reading::Accidental { at, .. } | Reading::Dot { at } => *at,
        }
    }

    pub fn describe(&self) -> String {
        let dotted = |d: u8| match d {
            1 => "dotted ",
            2 => "double-dotted ",
            _ => "",
        };
        match self {
            Reading::Notes { cols, beamed: true } => format!("{} beamed {}s", cols.len(), value_name(cols[0].value)),
            Reading::Notes { cols, .. } if cols.len() == 1 && cols[0].heads.len() > 1 => format!("{}-note chord ({}{})", cols[0].heads.len(), dotted(cols[0].dots), value_name(cols[0].value)),
            Reading::Notes { cols, .. } if cols.len() == 1 => format!("{}{}", dotted(cols[0].dots), value_name(cols[0].value)),
            Reading::Notes { cols, .. } => format!("{} notes", cols.len()),
            Reading::Rest { value, dots, .. } => format!("{}{}", dotted(*dots), rest_name(*value)),
            Reading::Accidental { alter, .. } => alter_name(*alter).into(),
            Reading::Dot { .. } => "dot".into(),
        }
    }

    /// This reading with another value (and dots): the alternatives of one figure.
    fn with_value(&self, value: u8, dots: u8) -> Reading {
        match self {
            Reading::Notes { cols, beamed } => Reading::Notes { cols: cols.iter().map(|c| Col { value, dots: if *beamed { c.dots } else { dots }, ..c.clone() }).collect(), beamed: *beamed },
            Reading::Rest { at, .. } => Reading::Rest { value, dots, at: *at },
            r => r.clone(),
        }
    }

    /// The length in ticks of each column (for "does it fit").
    fn ticks(&self) -> u32 {
        match self {
            Reading::Notes { cols, .. } => cols.iter().map(|c| crate::model::dur_ticks(c.value, c.dots, None)).sum(),
            Reading::Rest { value, dots, .. } => crate::model::dur_ticks(*value, *dots, None),
            _ => 0,
        }
    }

    fn value(&self) -> Option<u8> {
        match self {
            Reading::Notes { cols, .. } => cols.first().map(|c| c.value),
            Reading::Rest { value, .. } => Some(*value),
            _ => None,
        }
    }
}

pub fn alter_name(alter: i8) -> &'static str {
    match alter {
        2 => "double sharp",
        1 => "sharp",
        0 => "natural",
        -1 => "flat",
        -2 => "double flat",
        _ => "accidental",
    }
}

/// What around the ink helps decide between close readings.
#[derive(Clone, Copy, Default)]
pub struct Ctx<'a> {
    /// The y (screen points) of the middle line of the staff the ink is on.
    pub staff_mid: Option<f32>,
    /// Ticks left in the bar from where the ink starts.
    pub remaining: Option<u32>,
    /// The value of the note just before, in the same voice.
    pub neighbor: Option<u8>,
    pub personal: Option<&'a Personal>,
}

// ---------------- the writer's own models ----------------

/// One model: a label ("head-filled", "stem", "flag"… for components; a reading's label for whole figures), the
/// shape as a point cloud, its size in staff spaces, and (figures) where the head is in its box (0..1).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Template {
    pub label: String,
    pub cloud: Vec<[f32; 2]>,
    pub size: f32,
    #[serde(default)]
    pub anchor: [f32; 2],
}

/// The writer's models, kept in the browser (never sent anywhere).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Personal {
    pub parts: Vec<Template>,
    pub symbols: Vec<Template>,
}

fn push_template(list: &mut Vec<Template>, t: Template) {
    list.push(t.clone());
    let same: Vec<usize> = list.iter().enumerate().filter(|(_, x)| x.label == t.label).map(|(i, _)| i).collect();
    if same.len() > PERSONAL_PER_LABEL {
        list.remove(same[0]);
    }
}

impl Personal {
    pub fn count(&self) -> usize {
        self.parts.len() + self.symbols.len()
    }

    /// Learns `strokes` (screen points, staff space `sp`) as `reading`: the whole figure, and each component with the
    /// role it has in that figure (the head, the stem, the flags), even when the rules alone don't see that role.
    pub fn learn(&mut self, strokes: &[Vec<Pos2>], sp: f32, reading: &Reading) {
        let s = to_sp(strokes, sp);
        if s.is_empty() || sp <= 0.0 {
            return;
        }
        let all: Vec<Pos2> = s.iter().flatten().copied().collect();
        let r = bbox(&all);
        let size = r.width().max(r.height()).max(0.05);
        let rel = |p: Pos2| [((p.x / sp - r.left()) / r.width().max(1e-3)).clamp(0.0, 1.0), ((p.y / sp - r.top()) / r.height().max(1e-3)).clamp(0.0, 1.0)];
        let anchor = rel(reading.anchor());
        let label = reading.label();
        if matches!(reading, Reading::Notes { cols, .. } if cols.len() > 1) {
            // groups: only their components (below)
        } else {
            push_template(&mut self.symbols, Template { label, cloud: cloud_arr(&s), size, anchor });
        }
        // components: the roles they play in this figure
        if let Reading::Notes { cols, .. } = reading {
            let value = cols.first().map(|c| c.value).unwrap_or(3);
            let pieces: Vec<Vec<Pos2>> = s.iter().flat_map(|st| best_split(st, None)).map(|p| p.pts).collect();
            for (piece, role) in assign_roles(&pieces, value, cols.iter().map(|c| c.heads.len()).sum()) {
                let pr = bbox(&piece);
                push_template(&mut self.parts, Template { label: role.into(), cloud: cloud_arr(&[piece]), size: pr.width().max(pr.height()).max(0.05), anchor: [0.5, 0.5] });
            }
        }
    }
}

/// Which component plays which role in a note of `value` drawn as `pieces` (staff spaces): the stem is the tallest
/// straight piece, the heads the pieces at its ends that aren't straight (or the biggest), flags what's left near the
/// far end, dots the tiny ones.
fn assign_roles(pieces: &[Vec<Pos2>], value: u8, heads: usize) -> Vec<(Vec<Pos2>, &'static str)> {
    let mut out = vec![];
    let mut used = vec![false; pieces.len()];
    let fs: Vec<Feats> = pieces.iter().map(|p| feats(p)).collect();
    for (i, f) in fs.iter().enumerate() {
        if f.w.max(f.h) <= 0.55 && f.len <= 1.8 {
            used[i] = true;
            out.push((pieces[i].clone(), "dot"));
        }
    }
    let mut stem: Option<usize> = None;
    if value >= 2 {
        stem = (0..pieces.len()).filter(|&i| !used[i]).max_by(|&a, &b| (fs[a].chord * fs[a].straight).partial_cmp(&(fs[b].chord * fs[b].straight)).unwrap());
        if let Some(s) = stem {
            if fs[s].chord < 1.2 {
                stem = None;
            } else {
                used[s] = true;
                out.push((pieces[s].clone(), "stem"));
            }
        }
    }
    let head_label = if value >= 3 { "head-filled" } else { "head-open" };
    // heads: the pieces nearest the stem's end (or the biggest ones when there's no stem)
    let mut order: Vec<usize> = (0..pieces.len()).filter(|&i| !used[i]).collect();
    match stem {
        Some(s) => {
            let (a, b) = (pieces[s][0], *pieces[s].last().unwrap());
            let ends = [a, b];
            order.sort_by(|&x, &y| {
                let d = |i: usize| ends.iter().map(|e| e.distance(fs[i].c)).fold(f32::MAX, f32::min);
                d(x).partial_cmp(&d(y)).unwrap()
            });
            // the head end: the end nearest the first head
            let head_end = order.first().map(|&i| if a.distance(fs[i].c) < b.distance(fs[i].c) { a } else { b });
            for &i in order.iter().take(heads.max(1)) {
                used[i] = true;
                out.push((pieces[i].clone(), head_label));
            }
            if value >= 4 {
                let far = if head_end == Some(a) { b } else { a };
                for i in 0..pieces.len() {
                    if !used[i] && fs[i].c.distance(far) <= 3.0 {
                        used[i] = true;
                        out.push((pieces[i].clone(), "flag"));
                    }
                }
            }
        }
        None => {
            order.sort_by(|&x, &y| (fs[y].w * fs[y].h).partial_cmp(&(fs[x].w * fs[x].h)).unwrap());
            for &i in order.iter().take(heads.max(1)) {
                used[i] = true;
                out.push((pieces[i].clone(), head_label));
            }
        }
    }
    out
}

// ---------------- geometry helpers (staff-space units) ----------------

pub(crate) fn bbox(p: &[Pos2]) -> Rect {
    let mut r = Rect::NOTHING;
    for q in p {
        r.extend_with(*q);
    }
    r
}

pub(crate) fn path_len(p: &[Pos2]) -> f32 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub(crate) fn to_sp(strokes: &[Vec<Pos2>], sp: f32) -> Vec<Vec<Pos2>> {
    if sp <= 0.0 {
        return vec![];
    }
    strokes.iter().filter(|s| !s.is_empty()).map(|s| s.iter().map(|p| pos2(p.x / sp, p.y / sp)).collect()).collect()
}

/// `n` points evenly spaced along the path.
pub(crate) fn resample(p: &[Pos2], n: usize) -> Vec<Pos2> {
    if p.len() < 2 || n < 2 {
        return vec![p.first().copied().unwrap_or(Pos2::ZERO); n.max(1)];
    }
    let total = path_len(p);
    if total <= 1e-6 {
        return vec![p[0]; n];
    }
    let step = total / (n - 1) as f32;
    let mut out = vec![p[0]];
    let mut acc = 0.0;
    let mut prev = p[0];
    let mut i = 1;
    while i < p.len() {
        let d = prev.distance(p[i]);
        if acc + d >= step && d > 0.0 {
            let t = (step - acc) / d;
            let q = prev + (p[i] - prev) * t;
            out.push(q);
            prev = q;
            acc = 0.0;
            if out.len() == n - 1 {
                break;
            }
        } else {
            acc += d;
            prev = p[i];
            i += 1;
        }
    }
    while out.len() < n {
        out.push(*p.last().unwrap());
    }
    out
}

/// How many times the position along direction `d` (a unit vector) turns back, ignoring wobbles smaller than `h`.
fn reversals_along(p: &[Pos2], d: egui::Vec2, h: f32) -> usize {
    if p.is_empty() {
        return 0;
    }
    let v = |q: &Pos2| q.to_vec2().dot(d);
    let mut n = 0;
    let mut dir = 0i8;
    let mut anchor = v(&p[0]);
    for q in p.iter().skip(1) {
        let x = v(q);
        let dd = x - anchor;
        if dd.abs() >= h {
            let nd = if dd > 0.0 { 1 } else { -1 };
            if dir != 0 && nd != dir {
                n += 1;
            }
            dir = nd;
            anchor = x;
        } else if (dir > 0 && x > anchor) || (dir < 0 && x < anchor) {
            anchor = x;
        }
    }
    n
}

fn reversals(p: &[Pos2], along_x: bool, h: f32) -> usize {
    reversals_along(p, if along_x { vec2(1.0, 0.0) } else { vec2(0.0, 1.0) }, h)
}

/// The shape of one piece of ink (staff spaces).
#[derive(Clone, Copy, Debug)]
struct Feats {
    w: f32,
    h: f32,
    len: f32,
    /// distance between the ends
    gap: f32,
    /// the ends' distance over the length (1 = a straight line)
    straight: f32,
    chord: f32,
    /// the main direction from vertical, 0..90° (radians)
    from_vertical: f32,
    /// how many times it goes round (net turning / 360°)
    loops: f32,
    /// length over the larger side (a blot is dense)
    density: f32,
    c: Pos2,
    r: Rect,
}

fn feats(p: &[Pos2]) -> Feats {
    let r = bbox(p);
    let len = path_len(p);
    let (a, b) = (p.first().copied().unwrap_or(Pos2::ZERO), p.last().copied().unwrap_or(Pos2::ZERO));
    let gap = a.distance(b);
    // the main axis from the spread of the points
    let n = p.len().max(1) as f32;
    let c = (p.iter().fold(vec2(0.0, 0.0), |s, q| s + q.to_vec2()) / n).to_pos2();
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for q in p {
        let d = *q - c;
        sxx += d.x * d.x;
        syy += d.y * d.y;
        sxy += d.x * d.y;
    }
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy); // from the x axis
    let from_vertical = (std::f32::consts::FRAC_PI_2 - angle.abs()).abs();
    // net turning, on points about 0.1 space apart
    let rs = resample(p, ((len / 0.1) as usize).clamp(8, 240));
    let mut turn = 0.0;
    let mut prev: Option<egui::Vec2> = None;
    for w in rs.windows(2) {
        let v = w[1] - w[0];
        if v.length() < 1e-5 {
            continue;
        }
        if let Some(u) = prev {
            turn += u.x.mul_add(v.y, -(u.y * v.x)).atan2(u.dot(v));
        }
        prev = Some(v);
    }
    let m = r.width().max(r.height()).max(1e-3);
    Feats { w: r.width(), h: r.height(), len, gap, straight: if len > 1e-4 { gap / len } else { 1.0 }, chord: gap, from_vertical, loops: turn.abs() / std::f32::consts::TAU, density: len / m, c: r.center(), r }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

// ---------------- components ----------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Head,
    Stem,
    Flag,
    Beam,
    Dot,
    Block,
}

impl Part {
    fn of_label(l: &str) -> Option<(Part, Option<bool>)> {
        Some(match l {
            "head-filled" => (Part::Head, Some(true)),
            "head-open" => (Part::Head, Some(false)),
            "stem" => (Part::Stem, None),
            "flag" => (Part::Flag, None),
            "beam" => (Part::Beam, None),
            "dot" => (Part::Dot, None),
            "block" => (Part::Block, None),
            _ => return None,
        })
    }
}

/// A piece of ink with what it may be (confidence 0..1) and, as a head, how likely it is filled.
#[derive(Clone, Debug)]
struct Piece {
    pts: Vec<Pos2>,
    f: Feats,
    parts: Vec<(Part, f32)>,
    filled: f32,
}

impl Piece {
    fn conf(&self, k: Part) -> f32 {
        self.parts.iter().filter(|p| p.0 == k).map(|p| p.1).fold(0.0, f32::max)
    }
    fn best(&self) -> f32 {
        self.parts.iter().map(|p| p.1).fold(0.0, f32::max)
    }
}

/// What the rules (and the writer's models) see in one piece.
fn classify(p: &[Pos2], personal: Option<&Personal>, whole: bool) -> Piece {
    let f = feats(p);
    let mut parts = vec![];
    let (m, n) = (f.w.max(f.h), f.w.min(f.h));
    let mut filled = 0.0;
    // (a dot is a stroke of its own: a short piece cut from a longer stroke is not a dot)
    if m <= 0.55 && f.len <= 1.8 && whole {
        parts.push((Part::Dot, 0.95));
    } else {
        // head: a small loop (closed or nearly), or a blot the size of a head
        if (0.45..=3.0).contains(&m) && n >= 0.2 && n / m >= 0.22 && f.straight < 0.8 {
            let closing = if f.len >= 1.8 * m { clamp01(1.0 - f.gap / (0.95 * m)) } else { 0.0 };
            let round = clamp01((f.loops - 0.5) / 0.35).max(closing).max(clamp01((f.density - 2.6) / 1.5));
            if round > 0.25 {
                // (a head is roundish: a very flat blot is more likely a block or a dash)
                let size_ok = if (0.7..=2.4).contains(&m) { 1.0 } else { 0.75 } * if f.w > 2.1 * f.h { 0.6 } else { 1.0 };
                parts.push((Part::Head, (0.4 + 0.55 * round) * size_ok));
                let fill = f.len / (std::f32::consts::PI * (f.w + f.h) / 2.0).max(1e-3);
                filled = clamp01((fill - 1.3) / 0.5).max(clamp01((f.loops - 1.25) / 0.5)).max(clamp01((f.density - 4.2) / 1.5));
            }
        }
        // stem: a near-vertical line
        if (1.5..=9.0).contains(&f.chord) && f.straight >= 0.82 && f.from_vertical <= 0.44 {
            parts.push((Part::Stem, 0.5 + 0.45 * clamp01((f.straight - 0.82) / 0.14) * (1.0 - f.from_vertical / 0.9)));
        }
        // beam: a longer line, closer to horizontal
        if (1.0..=16.0).contains(&f.chord) && f.straight >= 0.85 && f.from_vertical >= 0.87 {
            parts.push((Part::Beam, 0.45 + 0.4 * clamp01((f.straight - 0.85) / 0.12)));
        }
        // flag or hook: a short curve or tick
        if (0.35..=3.6).contains(&m) && f.len <= 6.0 && f.gap >= 0.35 && f.loops < 0.75 {
            parts.push((Part::Flag, 0.45));
        }
        // block: a short flat filled bar
        if (0.45..=2.8).contains(&f.w) && (0.12..=1.1).contains(&f.h) && f.w >= 1.15 * f.h && f.density >= 2.4 {
            parts.push((Part::Block, 0.45 + 0.45 * clamp01((f.w / f.h.max(0.05) - 1.3) / 1.0)));
        }
    }
    // the writer's own components
    if let Some(per) = personal {
        if !per.parts.is_empty() && m > 0.0 {
            let c = cloud(&[p.to_vec()]);
            for t in &per.parts {
                if m < t.size * 0.45 || m > t.size * 2.2 {
                    continue;
                }
                let d = cloud_distance(&c, &from_arr(&t.cloud));
                let conf = clamp01(1.0 - d / 2.0) * 0.98;
                if conf <= 0.3 {
                    continue;
                }
                if let Some((k, fill)) = Part::of_label(&t.label) {
                    match parts.iter_mut().find(|x| x.0 == k) {
                        Some(x) => x.1 = x.1.max(conf),
                        None => parts.push((k, conf)),
                    }
                    if let Some(fl) = fill {
                        if conf > 0.45 {
                            filled = if fl { filled.max(0.92) } else { filled.min(0.08) };
                        }
                    }
                }
            }
        }
    }
    Piece { pts: p.to_vec(), f, parts, filled }
}

/// The ways a stroke can be cut into up to `max_parts` pieces (indices of the cuts).
fn cuts(n: usize, max_parts: usize) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];
    if max_parts >= 2 {
        for a in (4..n.saturating_sub(4)).step_by(2) {
            out.push(vec![a]);
        }
    }
    if max_parts >= 3 {
        for a in (4..n.saturating_sub(8)).step_by(3) {
            for b in ((a + 4)..n.saturating_sub(4)).step_by(3) {
                out.push(vec![a, b]);
            }
        }
    }
    out
}

/// The best ways of cutting one stroke into components (the first is the best): the reading of the weakest piece
/// decides, so a loop isn't cut in two, and head-and-stem in one stroke is.
fn splits(stroke: &[Pos2], personal: Option<&Personal>, keep: usize) -> Vec<Vec<Piece>> {
    let s = resample(stroke, 48);
    let whole = classify(stroke, personal, true);
    let mut opts: Vec<(f32, Vec<Piece>)> = vec![(whole.best(), vec![whole])];
    for c in cuts(s.len(), 3).into_iter().skip(1) {
        let mut pieces = vec![];
        let mut from = 0;
        for &k in &c {
            pieces.push(classify(&s[from..=k], personal, false));
            from = k;
        }
        pieces.push(classify(&s[from..], personal, false));
        let score = pieces.iter().map(|p| p.best()).fold(f32::MAX, f32::min) - 0.06 * c.len() as f32;
        opts.push((score, pieces));
    }
    opts.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let top = opts[0].0;
    opts.into_iter().enumerate().filter(|(i, o)| *i == 0 || (*i < keep && o.0 >= top - 0.08)).map(|(_, o)| o.1).collect()
}

fn best_split(stroke: &[Pos2], personal: Option<&Personal>) -> Vec<Piece> {
    splits(stroke, personal, 1).into_iter().next().unwrap_or_default()
}

// ---------------- notes from components ----------------

struct Column {
    heads: Vec<usize>,
    stem: Option<usize>,
    /// the stem's far end (flags and beams go there) and its head end
    far: Option<Pos2>,
    near: Option<Pos2>,
    up: bool,
    flags: u8,
    dots: u8,
}

/// The distance from `p` to the segment a–b.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The far part of a column's stem (where flags and beams sit): from the far end toward the head, up to 3 spaces.
fn far_part(c: &Column) -> Option<(Pos2, Pos2)> {
    let (f, n) = (c.far?, c.near?);
    let d = n - f;
    let l = d.length().max(1e-3);
    Some((f, f + d / l * l.min(3.2) * 0.85))
}

/// How many hooks a flag piece draws (a double flag in one stroke goes back once).
fn lobes(p: &Piece) -> u8 {
    let rs = resample(&p.pts, 48);
    (1 + reversals(&rs, false, 0.3) / 2).min(4) as u8
}

/// Notes put together from the pieces: Some((columns, beams, used share, piece confidence)).
fn assemble(pieces: &[Piece]) -> Option<(Vec<Column>, u8, Vec<bool>, f32)> {
    let n = pieces.len();
    let mut used = vec![false; n];
    let mut conf_sum = 0.0;
    let mut conf_n = 0.0;
    let is_head = |p: &Piece| {
        let h = p.conf(Part::Head);
        h >= 0.35 && h >= p.conf(Part::Stem).max(p.conf(Part::Beam)) - 0.1
    };
    let heads: Vec<usize> = (0..n).filter(|&i| is_head(&pieces[i])).collect();
    if heads.is_empty() {
        return None;
    }
    let stems: Vec<usize> = (0..n).filter(|&i| !heads.contains(&i) && pieces[i].conf(Part::Stem) >= 0.45).collect();
    // each head goes to the nearest stem that it sits on (either side, up to a space away, at either end)
    let mut cols: Vec<Column> = vec![];
    let mut head_stem: Vec<Option<usize>> = vec![None; n];
    for &h in &heads {
        let hc = pieces[h].f.c;
        let hw = pieces[h].f.w;
        let mut best: Option<(usize, f32)> = None;
        for &s in &stems {
            let sr = pieces[s].f.r;
            let (top, bot) = (sr.top(), sr.bottom());
            if hc.y < top - 1.1 || hc.y > bot + 1.1 {
                continue;
            }
            // the stem's x at the head's height
            let (a, b) = (pieces[s].pts[0], *pieces[s].pts.last().unwrap());
            let (lo, hi) = if a.y < b.y { (a, b) } else { (b, a) };
            let t = ((hc.y - lo.y) / (hi.y - lo.y).max(1e-3)).clamp(0.0, 1.0);
            let sx = lo.x + (hi.x - lo.x) * t;
            let dx = (hc.x - sx).abs();
            if dx > (hw / 2.0 + 1.0).max(1.4) {
                continue;
            }
            // near one end (heads of a chord stack along the stem from that end)
            let along = (hc.y - top) / (bot - top).max(1e-3);
            if (0.3..0.7).contains(&along) && heads.len() == 1 {
                continue;
            }
            if best.map(|x| dx < x.1).unwrap_or(true) {
                best = Some((s, dx));
            }
        }
        head_stem[h] = best.map(|b| b.0);
    }
    for &s in &stems {
        let hs: Vec<usize> = heads.iter().copied().filter(|&h| head_stem[h] == Some(s)).collect();
        if hs.is_empty() {
            continue;
        }
        let sr = pieces[s].f.r;
        let mean_y = hs.iter().map(|&h| pieces[h].f.c.y).sum::<f32>() / hs.len() as f32;
        let up = mean_y > sr.center().y;
        let (a, b) = (pieces[s].pts[0], *pieces[s].pts.last().unwrap());
        let far = if up { if a.y < b.y { a } else { b } } else if a.y > b.y { a } else { b };
        used[s] = true;
        conf_sum += pieces[s].conf(Part::Stem);
        conf_n += 1.0;
        for &h in &hs {
            used[h] = true;
            conf_sum += pieces[h].conf(Part::Head);
            conf_n += 1.0;
        }
        let near = if far == a { b } else { a };
        cols.push(Column { heads: hs, stem: Some(s), far: Some(far), near: Some(near), up, flags: 0, dots: 0 });
    }
    // heads without a stem; heads close together vertically are one chord
    let mut loose: Vec<usize> = heads.iter().copied().filter(|&h| head_stem[h].is_none()).collect();
    loose.sort_by(|&a, &b| pieces[a].f.c.x.partial_cmp(&pieces[b].f.c.x).unwrap());
    for h in loose {
        used[h] = true;
        conf_sum += pieces[h].conf(Part::Head);
        conf_n += 1.0;
        let x = pieces[h].f.c.x;
        if let Some(c) = cols.iter_mut().find(|c| c.stem.is_none() && c.heads.iter().any(|&o| (pieces[o].f.c.x - x).abs() < 0.8)) {
            c.heads.push(h);
        } else {
            cols.push(Column { heads: vec![h], stem: None, far: None, near: None, up: true, flags: 0, dots: 0 });
        }
    }
    // beams: lines joining the far ends of two or more stems
    let mut beams = 0u8;
    let mut beamed = vec![false; cols.len()];
    for i in 0..n {
        if used[i] || pieces[i].conf(Part::Beam).max(pieces[i].conf(Part::Flag)) < 0.3 || pieces[i].f.chord < 1.0 {
            continue;
        }
        let (a, b) = (pieces[i].pts[0], *pieces[i].pts.last().unwrap());
        // a beam crosses (or reaches) the far part of each stem it joins
        let touches: Vec<usize> = (0..cols.len())
            .filter(|&c| {
                far_part(&cols[c]).is_some_and(|(f, g)| {
                    let near_end = seg_dist(f, a, b) <= 1.1;
                    let crosses = (0..=8).any(|k| seg_dist(f + (g - f) * (k as f32 / 8.0), a, b) <= 0.45);
                    near_end || crosses
                })
            })
            .collect();
        if touches.len() >= 2 {
            used[i] = true;
            beams += 1;
            conf_sum += pieces[i].conf(Part::Beam).max(0.5);
            conf_n += 1.0;
            for c in touches {
                beamed[c] = true;
            }
        }
    }
    // flags: short hooks at a stem's far end, on either side
    for i in 0..n {
        if used[i] || pieces[i].conf(Part::Flag).max(pieces[i].conf(Part::Beam)) < 0.3 {
            continue;
        }
        let (a, b) = (pieces[i].pts[0], *pieces[i].pts.last().unwrap());
        let mut best: Option<(usize, f32)> = None;
        for (c, col) in cols.iter().enumerate() {
            let Some((f, g)) = far_part(col) else { continue };
            // one end of the hook on (or near) the far part of the stem, either side
            let d = seg_dist(a, f, g).min(seg_dist(b, f, g));
            if d <= 1.0 && best.map(|x| d < x.1).unwrap_or(true) {
                best = Some((c, d));
            }
        }
        if let Some((c, _)) = best {
            used[i] = true;
            cols[c].flags = (cols[c].flags + lobes(&pieces[i])).min(4);
            conf_sum += 0.6;
            conf_n += 1.0;
        }
    }
    // dots after a head
    let mut dots: Vec<usize> = (0..n).filter(|&i| !used[i] && pieces[i].conf(Part::Dot) > 0.5).collect();
    dots.sort_by(|&a, &b| pieces[a].f.c.x.partial_cmp(&pieces[b].f.c.x).unwrap());
    for i in dots {
        let pc = pieces[i].f.c;
        let mut best: Option<(usize, f32)> = None;
        for (c, col) in cols.iter().enumerate() {
            for &h in &col.heads {
                let hr = pieces[h].f.r;
                let dx = pc.x - hr.right();
                let dy = (pc.y - hr.center().y).abs();
                if (-0.25..=2.6).contains(&dx) && dy <= 1.0 && best.map(|x| dx < x.1).unwrap_or(true) {
                    best = Some((c, dx));
                }
            }
        }
        if let Some((c, _)) = best {
            if cols[c].dots < 2 {
                used[i] = true;
                cols[c].dots += 1;
            }
        }
    }
    if beams > 0 {
        // only the joined columns are the group (a lone column joined to nothing stays out of it)
        let joined: Vec<Column> = cols.into_iter().zip(beamed.iter()).filter(|(_, b)| **b).map(|(c, _)| c).collect();
        cols = joined;
    }
    cols.sort_by(|a, b| {
        let x = |c: &Column| c.heads.iter().map(|&h| pieces[h].f.c.x).sum::<f32>() / c.heads.len().max(1) as f32;
        x(a).partial_cmp(&x(b)).unwrap()
    });
    let conf = if conf_n > 0.0 { conf_sum / conf_n } else { 0.0 };
    Some((cols, beams, used, conf))
}

/// How well a column's evidence fits each (value, dots): filled or open head, a stem or not, how many flags.
fn value_likelihood(filled: f32, stem: bool, flags: u8, dots: u8, value: u8, want_dots: u8) -> f32 {
    let pf = filled.clamp(0.04, 0.96);
    let flag_fit = |k: u8| match (flags as i32 - k as i32).abs() {
        0 => 1.0,
        1 => 0.3,
        _ => 0.06,
    };
    let stem_p = if stem { 0.95 } else { 0.05 };
    let v = match value {
        1 => (1.0 - pf) * (1.0 - stem_p) * flag_fit(0),
        2 => (1.0 - pf) * stem_p * flag_fit(0),
        // a filled head without a stem: most often a quarter written fast
        3 => pf * (stem_p + 0.7 * (1.0 - stem_p)) * flag_fit(0),
        4..=7 => pf * (stem_p * flag_fit(value - 3) + (1.0 - stem_p) * if value == 4 { 0.25 } else { 0.06 }),
        _ => 0.0,
    };
    let d = match (dots as i32 - want_dots as i32).abs() {
        0 => 1.0,
        1 => 0.12,
        _ => 0.02,
    };
    v * d
}

/// The note readings of the pieces; true when a head sits on a stem in a way a ♭ doesn't.
fn note_candidates(pieces: &[Piece], out: &mut Vec<Candidate>, back: &dyn Fn(Pos2) -> Pos2) -> bool {
    let Some((cols, beams, used, conf)) = assemble(pieces) else { return false };
    if cols.is_empty() {
        return false;
    }
    // a ♭ looks like a small open head with a stem up on its left whose foot is the bowl's foot: say so
    // (the bowl of a ♭ is open on the left: both its ends lie on the stem; a head is a loop of its own)
    let flat_like = |c: &Column| {
        let (Some(s), Some(&h)) = (c.stem, c.heads.first()) else { return false };
        let (sp_, hp) = (&pieces[s], &pieces[h]);
        let (a, b) = (sp_.pts[0], *sp_.pts.last().unwrap());
        let (e0, e1) = (hp.pts[0], *hp.pts.last().unwrap());
        let on_stem = seg_dist(e0, a, b) <= 0.4 && seg_dist(e1, a, b) <= 0.4;
        c.heads.len() == 1 && c.up && sp_.f.c.x < hp.f.c.x && on_stem && hp.f.w <= 1.1 && hp.filled < 0.5
    };
    let all_flat = cols.iter().all(flat_like);
    let structure = cols.iter().any(|c| c.stem.is_some() && !flat_like(c));
    // whatever was left over makes the reading weaker (it may be an accidental: tried later)
    let left: Vec<usize> = (0..pieces.len()).filter(|&i| !used[i]).collect();
    let mut base = conf * 0.6f32.powi(left.len() as i32) * if all_flat { 0.45 } else { 1.0 };
    // an accidental drawn before the first head
    let mut acc: Option<(i8, usize)> = None;
    if !left.is_empty() {
        let first_x = cols[0].heads.iter().map(|&h| pieces[h].f.r.left()).fold(f32::MAX, f32::min);
        let lp: Vec<Vec<Pos2>> = left.iter().map(|&i| pieces[i].pts.clone()).collect();
        let lr = bbox(&lp.iter().flatten().copied().collect::<Vec<_>>());
        if lr.right() <= first_x + 0.3 && first_x - lr.right() <= ACCIDENTAL_REACH {
            if let Some((alter, _, d)) = read_accidental(&lp, None) {
                // the head it belongs to: the one nearest in height
                let k = (0..cols[0].heads.len()).min_by(|&a, &b| (pieces[cols[0].heads[a]].f.c.y - lr.center().y).abs().partial_cmp(&(pieces[cols[0].heads[b]].f.c.y - lr.center().y).abs()).unwrap()).unwrap_or(0);
                acc = Some((alter, k));
                base = conf * clamp01(1.0 - d / 3.5).max(0.5);
            }
        }
    }
    let make = |value: u8, dots: Vec<u8>| -> Reading {
        let cs = cols
            .iter()
            .enumerate()
            .map(|(i, c)| Col { heads: c.heads.iter().map(|&h| back(pieces[h].f.c)).collect(), value, dots: dots.get(i).copied().unwrap_or(0), acc: if i == 0 { acc.map(|a| a.0) } else { None }, acc_head: if i == 0 { acc.map(|a| a.1).unwrap_or(0) } else { 0 } })
            .collect();
        Reading::Notes { cols: cs, beamed: beams > 0 }
    };
    let filled_of = |c: &Column| c.heads.iter().map(|&h| pieces[h].filled).sum::<f32>() / c.heads.len().max(1) as f32;
    if beams > 0 && cols.len() >= 2 {
        // a beamed group: one beam = eighths, two = sixteenths, three = thirty-seconds
        let pf = cols.iter().map(filled_of).sum::<f32>() / cols.len() as f32;
        let dots: Vec<u8> = cols.iter().map(|c| c.dots).collect();
        for k in 1..=4u8 {
            let fit = match (beams as i32 - k as i32).abs() {
                0 => 1.0,
                1 => 0.3,
                _ => 0.05,
            };
            out.push(Candidate { reading: make(3 + k, dots.clone()), score: base * fit * (0.3 + 0.7 * pf.max(0.5)) });
        }
        return structure;
    }
    if cols.len() > 1 {
        // several notes at once, not beamed: each its own value
        let cs: Vec<Col> = cols
            .iter()
            .map(|c| {
                let pf = filled_of(c);
                let best = (1..=7u8).max_by(|&a, &b| value_likelihood(pf, c.stem.is_some(), c.flags, c.dots, a, c.dots).partial_cmp(&value_likelihood(pf, c.stem.is_some(), c.flags, c.dots, b, c.dots)).unwrap()).unwrap_or(3);
                Col { heads: c.heads.iter().map(|&h| back(pieces[h].f.c)).collect(), value: best, dots: c.dots, acc: None, acc_head: 0 }
            })
            .collect();
        out.push(Candidate { reading: Reading::Notes { cols: cs, beamed: false }, score: base * 0.8 });
        return structure;
    }
    let c = &cols[0];
    let pf = filled_of(c);
    for value in 1..=7u8 {
        for d in 0..=2u8 {
            let l = value_likelihood(pf, c.stem.is_some(), c.flags, c.dots, value, d);
            if l > 0.003 {
                out.push(Candidate { reading: make(value, vec![d]), score: base * l });
            }
        }
    }
    structure
}

// ---------------- rests ----------------

fn rest_candidates(s: &[Vec<Pos2>], pieces: &[Piece], ctx: &Ctx, sp: f32, out: &mut Vec<Candidate>, back: &dyn Fn(Pos2) -> Pos2) {
    let all: Vec<Pos2> = s.iter().flatten().copied().collect();
    let r = bbox(&all);
    // dots after the rest
    let dots = pieces.iter().filter(|p| p.conf(Part::Dot) > 0.5 && p.f.c.x > r.center().x).count().min(2) as u8;
    let body: Vec<&Piece> = pieces.iter().filter(|p| !(p.conf(Part::Dot) > 0.5 && p.f.c.x > r.center().x)).collect();
    if body.is_empty() {
        return;
    }
    let ball: Vec<Pos2> = body.iter().flat_map(|p| p.pts.iter().copied()).collect();
    let br = bbox(&ball);
    // the quarter rest: a tall zigzag, one or two strokes
    if s.len() == 1 && hooked_rest(std::slice::from_ref(&ball)).is_none() {
        let p = resample(&ball, 64);
        let xr = reversals(&p, true, 0.22);
        if (1.4..=4.8).contains(&br.height()) && (0.35..=2.4).contains(&br.width()) && br.height() >= 1.25 * br.width() && (2..=7).contains(&xr) && reversals(&p, false, 0.35) <= 2 {
            let straight_line = body.len() == 1 && body[0].f.straight > 0.85;
            if !straight_line {
                out.push(Candidate { reading: Reading::Rest { value: 3, dots, at: back(br.center()) }, score: 0.75 });
            }
        }
    }
    // the hooked rests: a slanting line, top to the right, with hooks on its left near the top
    // (whole strokes: the line may be cut between pieces; dots after it left out)
    let body_strokes: Vec<Vec<Pos2>> = s.iter().filter(|st| {
        let b = bbox(st);
        !(b.width().max(b.height()) <= 0.55 && b.center().x > r.center().x)
    }).cloned().collect();
    if let Some(v) = hooked_rest(&body_strokes) {
        for k in 4..=7u8 {
            let fit = match (v as i32 - k as i32).abs() {
                0 => 1.0,
                1 => 0.3,
                _ => 0.05,
            };
            out.push(Candidate { reading: Reading::Rest { value: k, dots, at: back(br.center()) }, score: 0.78 * fit });
        }
    }
    // the whole and half rests: a flat filled block (hanging from the 4th line / sitting on the 3rd)
    if body.len() == 1 && body[0].conf(Part::Block) > 0.3 {
        let b = body[0].conf(Part::Block);
        let (whole, half) = match ctx.staff_mid {
            Some(mid) => {
                let c = (br.center().y * sp - mid) / sp;
                if (-1.3..-0.5).contains(&c) {
                    (1.0, 0.35)
                } else if (-0.5..=0.3).contains(&c) {
                    (0.35, 1.0)
                } else {
                    (0.3, 0.4)
                }
            }
            None => (0.4, 0.5),
        };
        out.push(Candidate { reading: Reading::Rest { value: 1, dots, at: back(br.center()) }, score: b * whole });
        out.push(Candidate { reading: Reading::Rest { value: 2, dots, at: back(br.center()) }, score: b * half });
    }
}

/// A hooked rest: the slanting line from the lowest point to the rightmost point above it, and the hooks: groups of
/// ink to the left of that line, in its upper part. Some(value) (4 eighth … 7 sixty-fourth).
fn hooked_rest(strokes: &[Vec<Pos2>]) -> Option<u8> {
    let p: Vec<Pos2> = strokes.iter().flatten().copied().collect();
    let p = &p[..];
    let r = bbox(p);
    if !(1.2..=5.5).contains(&r.height()) || r.width() > 3.0 || r.width() < 0.3 {
        return None;
    }
    let b = *p.iter().max_by(|a, c| a.y.partial_cmp(&c.y).unwrap())?;
    let t = *p.iter().filter(|q| q.y < b.y - 1.0).max_by(|a, c| (a.x - 0.25 * a.y).partial_cmp(&(c.x - 0.25 * c.y)).unwrap())?;
    let line = t - b;
    let l = line.length();
    if !(1.2..=5.5).contains(&l) || t.x <= b.x + 0.12 {
        return None;
    }
    let from_vertical = (line.x.abs() / l).asin();
    if !(0.12..=0.95).contains(&from_vertical) {
        return None;
    }
    let u = line / l;
    // the slanting line is really drawn: one run of ink along most of it
    let mut best_run = 0.0f32;
    for st in strokes {
        let rs = resample(st, ((path_len(st) / 0.08) as usize).clamp(8, 200));
        let mut run: Option<(f32, f32)> = None;
        for q in &rs {
            let d = *q - b;
            let along = d.dot(u);
            let off = (u.x * d.y - u.y * d.x).abs();
            if off <= 0.22 && (-0.2..=l + 0.2).contains(&along) {
                run = Some(run.map_or((along, along), |(lo, hi)| (lo.min(along), hi.max(along))));
                best_run = best_run.max(run.map(|(lo, hi)| hi - lo).unwrap_or(0.0));
            } else {
                run = None;
            }
        }
    }
    if best_run < 0.72 * l {
        return None;
    }
    // left of the line (y down: the left of b→t has a negative cross product)
    let side = |q: Pos2| {
        let d = q - b;
        u.x * d.y - u.y * d.x
    };
    let right_share = p.iter().filter(|q| side(**q) > 0.35).count() as f32 / p.len() as f32;
    if right_share > 0.12 {
        return None;
    }
    // hooks: points well to the left, grouped along the line
    let mut ts: Vec<f32> = p.iter().filter(|q| side(**q) < -0.3).map(|q| ((*q - b).dot(u) / l).clamp(0.0, 1.2)).collect();
    if ts.is_empty() {
        return None;
    }
    ts.sort_by(|a, c| a.partial_cmp(c).unwrap());
    // they sit in the upper part of the line
    if ts.iter().filter(|x| **x < 0.3).count() as f32 > 0.4 * ts.len() as f32 {
        return None;
    }
    let gap = 0.42 / l;
    let mut hooks = 1;
    for w in ts.windows(2) {
        if w[1] - w[0] > gap {
            hooks += 1;
        }
    }
    Some((3 + hooks.min(4)) as u8)
}

// ---------------- point clouds ($P, written from the paper) ----------------

const CLOUD_N: usize = 32;

/// A cloud of `CLOUD_N` points from all strokes, scaled to a unit size and centred on its centroid.
fn cloud(strokes: &[Vec<Pos2>]) -> Vec<Pos2> {
    let strokes: Vec<&Vec<Pos2>> = strokes.iter().filter(|s| !s.is_empty()).collect();
    if strokes.is_empty() {
        return vec![Pos2::ZERO; CLOUD_N];
    }
    let total: f32 = strokes.iter().map(|s| path_len(s)).sum();
    let mut pts = vec![];
    for s in &strokes {
        let share = if total > 0.0 { ((path_len(s) / total) * CLOUD_N as f32).round().max(2.0) as usize } else { CLOUD_N / strokes.len().max(1) };
        pts.extend(resample(s, share));
    }
    let pts = if pts.len() == CLOUD_N {
        pts
    } else {
        (0..CLOUD_N).map(|i| pts[i * pts.len() / CLOUD_N]).collect()
    };
    let r = bbox(&pts);
    let scale = r.width().max(r.height()).max(1e-6);
    let c = pts.iter().fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2()) / CLOUD_N as f32;
    pts.iter().map(|p| ((p.to_vec2() - c) / scale).to_pos2()).collect()
}

fn cloud_arr(strokes: &[Vec<Pos2>]) -> Vec<[f32; 2]> {
    cloud(strokes).iter().map(|p| [(p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0]).collect()
}

fn from_arr(a: &[[f32; 2]]) -> Vec<Pos2> {
    a.iter().map(|p| pos2(p[0], p[1])).collect()
}

/// The greedy cloud distance of $P: each point of one cloud matched to the nearest unmatched point of the other,
/// earlier matches weighing more, from several starting points; the smaller of both directions.
fn cloud_distance(a: &[Pos2], b: &[Pos2]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return f32::MAX;
    }
    let step = ((n as f32).sqrt() as usize).max(1);
    let mut best = f32::MAX;
    let mut i = 0;
    while i < n {
        best = best.min(greedy(&a[..n], &b[..n], i)).min(greedy(&b[..n], &a[..n], i));
        i += step;
    }
    best
}

fn greedy(a: &[Pos2], b: &[Pos2], start: usize) -> f32 {
    let n = a.len();
    let mut used = vec![false; n];
    let mut sum = 0.0;
    let mut i = start;
    for k in 0..n {
        let mut min = f32::MAX;
        let mut idx = 0;
        for (j, q) in b.iter().enumerate() {
            if !used[j] {
                let d = a[i].distance(*q);
                if d < min {
                    min = d;
                    idx = j;
                }
            }
        }
        used[idx] = true;
        let weight = 1.0 - (k as f32) / n as f32;
        sum += weight * min;
        i = (i + 1) % n;
    }
    sum
}

/// An accidental's template: its alteration, its strokes (unit box), the range of its size (staff spaces).
type AccTemplate = (i8, Vec<Vec<Pos2>>, (f32, f32));

/// Templates for ♯ ♭ ♮ 𝄪 𝄫 (unit box, y down), drawn for Fermata, with the size range (staff spaces) of each.
fn templates() -> Vec<AccTemplate> {
    let l = |pts: &[(f32, f32)]| -> Vec<Pos2> { pts.iter().map(|&(x, y)| pos2(x, y)).collect() };
    let tall = (1.4, 4.8);
    vec![
        // ♯: two verticals and two rising bars
        (1, vec![l(&[(0.38, 0.0), (0.35, 1.0)]), l(&[(0.68, 0.02), (0.65, 0.98)]), l(&[(0.1, 0.42), (0.92, 0.32)]), l(&[(0.1, 0.72), (0.92, 0.62)])], tall),
        // ♭: a long vertical, then the bowl at its foot, to the right
        (-1, vec![l(&[(0.25, 0.0), (0.25, 0.55), (0.25, 1.0), (0.55, 0.86), (0.72, 0.68), (0.62, 0.55), (0.42, 0.56), (0.25, 0.7)])], tall),
        // ♮: two L shapes
        (0, vec![l(&[(0.3, 0.0), (0.3, 0.72), (0.72, 0.6)]), l(&[(0.3, 0.38), (0.72, 0.26), (0.72, 1.0)])], tall),
        // ♮ in one stroke (down, across, down)
        (0, vec![l(&[(0.3, 0.0), (0.3, 0.72), (0.72, 0.6), (0.72, 1.0)]), l(&[(0.3, 0.38), (0.72, 0.26)])], tall),
        // 𝄪: a small x
        (2, vec![l(&[(0.0, 0.0), (1.0, 1.0)]), l(&[(1.0, 0.0), (0.0, 1.0)])], (0.45, 1.6)),
        // 𝄫: two flats side by side
        (
            -2,
            vec![
                l(&[(0.1, 0.0), (0.1, 1.0), (0.3, 0.86), (0.42, 0.68), (0.36, 0.56), (0.22, 0.57), (0.1, 0.7)]),
                l(&[(0.55, 0.0), (0.55, 1.0), (0.75, 0.86), (0.88, 0.68), (0.8, 0.56), (0.66, 0.57), (0.55, 0.7)]),
            ],
            tall,
        ),
    ]
}

/// ♯ ♭ ♮ 𝄪 𝄫 by the point cloud, when the size fits: Some((alteration, score 0..1, distance)).
fn read_accidental(strokes: &[Vec<Pos2>], personal: Option<&Personal>) -> Option<(i8, f32, f32)> {
    let all: Vec<Pos2> = strokes.iter().flatten().copied().collect();
    let r = bbox(&all);
    if strokes.len() > 5 || r.width() > 3.4 || r.height() > 5.0 {
        return None;
    }
    let c = cloud(strokes);
    let mut scores: Vec<(i8, f32)> = templates()
        .into_iter()
        .filter(|(alter, _, (lo, hi))| {
            let m = if *alter == 2 { r.width().max(r.height()) } else { r.height() };
            (*lo..=*hi).contains(&m) && (*alter == 2 || (0.35..=3.4).contains(&r.width()))
        })
        .map(|(alter, t, _)| (alter, cloud_distance(&c, &cloud(&t))))
        .collect();
    if let Some(per) = personal {
        let m = r.width().max(r.height());
        for t in per.symbols.iter().filter(|t| t.label.starts_with('a') && m >= t.size * 0.55 && m <= t.size * 1.8) {
            if let Ok(alter) = t.label[1..].parse::<i8>() {
                scores.push((alter, cloud_distance(&c, &from_arr(&t.cloud)) * 0.85));
            }
        }
    }
    scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let &(alter, best) = scores.first()?;
    let second = scores.iter().find(|s| s.0 != alter).map(|s| s.1).unwrap_or(f32::MAX);
    if best > 2.7 {
        return None;
    }
    let clear = best * 1.2 < second;
    Some((alter, if clear { 0.82 - 0.1 * best } else { 0.45 }, best))
}

// ---------------- scribbles ----------------

/// How many times one stroke turns back along its two main axes (from the spread of its points, any orientation):
/// (the larger count, the smaller).
fn turns_back(st: &[Pos2]) -> (usize, usize) {
    let rs = resample(st, 96);
    let n = rs.len().max(1) as f32;
    let c = rs.iter().fold(vec2(0.0, 0.0), |s, q| s + q.to_vec2()) / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for q in &rs {
        let d = q.to_vec2() - c;
        sxx += d.x * d.x;
        syy += d.y * d.y;
        sxy += d.x * d.y;
    }
    let a = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let count = |d: egui::Vec2| {
        let (lo, hi) = rs.iter().fold((f32::MAX, f32::MIN), |(l, h), q| {
            let v = q.to_vec2().dot(d);
            (l.min(v), h.max(v))
        });
        reversals_along(&rs, d, (0.18 * (hi - lo)).max(0.15))
    };
    let (x, y) = (count(vec2(a.cos(), a.sin())), count(vec2(-a.sin(), a.cos())));
    (x.max(y), x.min(y))
}

/// A stroke that goes back and forth (along one direction), not round: a spiral head turns back as often across as
/// along; a scribble mostly one way. Its turns, or 0.
fn zigzag_turns(st: &[Pos2]) -> usize {
    let (along, across) = turns_back(st);
    if along >= 2 && across * 2 <= along {
        along
    } else {
        0
    }
}

/// How many times the ink goes back and forth (the strokes that zigzag rather than go round).
#[cfg_attr(not(test), allow(dead_code))]
pub fn scribble_turns(strokes: &[Vec<Pos2>], sp: f32) -> usize {
    to_sp(strokes, sp).iter().map(|st| zigzag_turns(st)).sum()
}

/// A scribble: the box (screen points) of the strokes that go back and forth, when they turn back
/// `SCRIBBLE_TURNS` times or more.
pub fn scribble_rect(strokes: &[Vec<Pos2>], sp: f32) -> Option<Rect> {
    let s = to_sp(strokes, sp);
    let zig: Vec<usize> = (0..s.len()).filter(|&i| zigzag_turns(&s[i]) > 0).collect();
    let n: usize = zig.iter().map(|&i| zigzag_turns(&s[i])).sum();
    if n < SCRIBBLE_TURNS {
        return None;
    }
    let mut r = Rect::NOTHING;
    for &i in &zig {
        for p in &strokes[i] {
            r.extend_with(*p);
        }
    }
    Some(r)
}

// ---------------- reading a group ----------------

/// Reads a group of strokes (screen points, y down) at staff space `sp`: the readings, best first (at most one per
/// label). Empty when the ink is clearly not music: too big, a lone straight line, or nothing the rules see.
pub fn recognize(strokes: &[Vec<Pos2>], sp: f32, ctx: &Ctx) -> Vec<Candidate> {
    let strokes: Vec<Vec<Pos2>> = strokes.iter().filter(|s| s.len() >= 2).cloned().collect();
    if strokes.is_empty() || sp <= 0.0 {
        return vec![];
    }
    let s = to_sp(&strokes, sp);
    let back = |p: Pos2| pos2(p.x * sp, p.y * sp);
    let all: Vec<Pos2> = s.iter().flatten().copied().collect();
    let r = bbox(&all);
    // too big for one figure (a beamed group may be wide, not tall)
    if r.height() > 9.5 || r.width() > 18.0 {
        return vec![];
    }
    // off the staff: not music
    if let Some(mid) = ctx.staff_mid {
        if (r.center().y * sp - mid).abs() > 6.5 * sp {
            return vec![];
        }
    }
    let mut out: Vec<Candidate> = vec![];
    let personal = ctx.personal;
    // the ways of cutting the strokes into components (a few combinations at most)
    let per_stroke: Vec<Vec<Vec<Piece>>> = s.iter().map(|st| splits(st, personal, if s.len() <= 4 { 2 } else { 1 })).collect();
    let mut combos: Vec<Vec<Piece>> = vec![vec![]];
    for opts in &per_stroke {
        let mut next = vec![];
        for c in &combos {
            for o in opts {
                if next.len() >= 24 {
                    break;
                }
                let mut v = c.clone();
                v.extend(o.iter().cloned());
                next.push(v);
            }
        }
        combos = next;
    }
    let mut structure = false;
    for pieces in &combos {
        structure |= note_candidates(pieces, &mut out, &back);
    }
    // a drawn slanting line with hooks on its left: a rest more than a note
    if hooked_rest(&s).is_some() {
        for c in &mut out {
            c.score *= 0.6;
        }
    }
    for pieces in &combos {
        rest_candidates(&s, pieces, ctx, sp, &mut out, &back);
    }
    // a dot alone
    if s.len() == 1 && r.width().max(r.height()) <= 0.55 && path_len(&s[0]) <= 1.8 {
        out.push(Candidate { reading: Reading::Dot { at: back(r.center()) }, score: 0.9 });
    }
    // an accidental alone
    if let Some((alter, score, _)) = read_accidental(&s, personal) {
        // a head on a stem drawn there makes a note more likely than an accidental
        out.push(Candidate { reading: Reading::Accidental { alter, at: back(r.center()), right: r.right() * sp }, score: score * if structure { 0.6 } else { 1.0 } });
    }
    // the writer's own figures
    if let Some(per) = personal {
        let m = r.width().max(r.height());
        let c = cloud(&s);
        for t in per.symbols.iter().filter(|t| m >= t.size * 0.55 && m <= t.size * 1.8) {
            let d = cloud_distance(&c, &from_arr(&t.cloud));
            let conf = clamp01(1.0 - d / 2.0);
            if conf <= 0.35 {
                continue;
            }
            let at = back(pos2(r.left() + t.anchor[0] * r.width(), r.top() + t.anchor[1] * r.height()));
            if let Some(reading) = reading_of_label(&t.label, at, r.right() * sp) {
                out.push(Candidate { reading, score: 0.55 + 0.45 * conf });
            }
        }
    }
    // a lone straight line is not a figure (a stem, a beam or a slip): nothing on a guess
    let lone_line = s.len() == 1 && {
        let f = feats(&s[0]);
        f.straight > 0.9 && f.chord >= 1.2
    };
    let has_note = out.iter().any(|c| matches!(c.reading, Reading::Notes { .. }));
    if !has_note && !lone_line && r.width().max(r.height()) <= 6.0 && r.width().max(r.height()) >= 0.4 {
        // the best guess: a quarter note at the roundest piece (the alternatives offer the others)
        let on_staff = ctx.staff_mid.is_none_or(|mid| (r.center().y * sp - mid).abs() <= 6.0 * sp);
        if on_staff {
            let pieces: Vec<Piece> = s.iter().flat_map(|st| best_split(st, personal)).collect();
            let head = pieces.iter().max_by(|a, b| (a.f.loops + a.f.density * 0.2).partial_cmp(&(b.f.loops + b.f.density * 0.2)).unwrap()).map(|p| p.f.c).unwrap_or(r.center());
            let col = |v: u8| Reading::Notes { cols: vec![Col { heads: vec![back(head)], value: v, dots: 0, acc: None, acc_head: 0 }], beamed: false };
            for (v, sc) in [(3u8, 0.2f32), (2, 0.15), (4, 0.12), (1, 0.1)] {
                out.push(Candidate { reading: col(v), score: sc });
            }
            out.push(Candidate { reading: Reading::Rest { value: 3, dots: 0, at: back(r.center()) }, score: 0.08 });
        }
    }
    // the context breaks ties: what fits in the bar, the neighbour's value
    for c in &mut out {
        if let Some(rem) = ctx.remaining {
            let t = c.reading.ticks();
            if t > 0 && t > rem {
                c.score *= 0.3;
            }
        }
        if let (Some(nb), Some(v)) = (ctx.neighbor, c.reading.value()) {
            if nb == v && matches!(c.reading, Reading::Notes { .. }) {
                c.score *= 1.15;
            }
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.reading.label()));
    // the alternatives of the best figure: its other values first
    if let Some(first) = out.first().cloned() {
        if matches!(first.reading, Reading::Notes { ref cols, beamed: false } if cols.len() == 1) || matches!(first.reading, Reading::Rest { .. }) {
            let have: std::collections::HashSet<String> = out.iter().map(|c| c.reading.label()).collect();
            let mut extra = vec![];
            for v in [3u8, 2, 1, 4, 5] {
                let r = first.reading.with_value(v, 0);
                if !have.contains(&r.label()) {
                    extra.push(Candidate { reading: r, score: 0.001 });
                }
            }
            out.extend(extra);
        }
    }
    out
}

/// The figure `label` stands for, placed on `strokes` (teaching): the head where its piece is, whatever it looks like.
pub fn reading_for(label: &str, strokes: &[Vec<Pos2>], sp: f32) -> Option<Reading> {
    let s = to_sp(strokes, sp);
    let all: Vec<Pos2> = s.iter().flatten().copied().collect();
    if all.is_empty() {
        return None;
    }
    let r = bbox(&all);
    let mut at = r.center();
    if let Some(rest) = label.strip_prefix('n') {
        let v: u8 = rest.split('.').next()?.parse().ok()?;
        let pieces: Vec<Vec<Pos2>> = s.iter().flat_map(|st| best_split(st, None)).map(|p| p.pts).collect();
        let heads: Vec<Vec<Pos2>> = assign_roles(&pieces, v, 1).into_iter().filter(|(_, role)| role.starts_with("head")).map(|(p, _)| p).collect();
        if !heads.is_empty() {
            at = bbox(&heads.concat()).center();
        }
    }
    reading_of_label(label, pos2(at.x * sp, at.y * sp), r.right() * sp)
}

/// A reading back from a model's label, placed at `at`.
fn reading_of_label(label: &str, at: Pos2, right: f32) -> Option<Reading> {
    let (kind, rest) = label.split_at(1);
    match kind {
        "n" | "r" => {
            let (v, d) = rest.split_once('.')?;
            let (v, d) = (v.parse::<u8>().ok()?, d.parse::<u8>().ok()?);
            Some(if kind == "n" { Reading::Notes { cols: vec![Col { heads: vec![at], value: v, dots: d, acc: None, acc_head: 0 }], beamed: false } } else { Reading::Rest { value: v, dots: d, at } })
        }
        "a" => Some(Reading::Accidental { alter: rest.parse().ok()?, at, right }),
        _ => None,
    }
}

/// Test strokes: shapes as a hand draws them, recorded as point lists in staff spaces around (0, 0) and placed with
/// `at`. `Hand` makes them crooked: slanted, not quite closed, stems apart from the head, short hooks. They feed the
/// tests and the screenshots.
#[cfg_attr(not(test), allow(dead_code))]
pub mod samples {
    use super::*;

    /// A small deterministic wobble (no randomness: the same every run).
    fn wobble(i: usize, seed: u32, amp: f32) -> f32 {
        let x = (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed.wrapping_mul(40_503));
        ((x >> 8) % 1000) as f32 / 1000.0 * 2.0 * amp - amp
    }

    fn place(p: Vec<(f32, f32)>, at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place_amp(p, at, sp, seed, 0.04)
    }

    fn place_amp(p: Vec<(f32, f32)>, at: Pos2, sp: f32, seed: u32, amp: f32) -> Vec<Pos2> {
        p.into_iter().enumerate().map(|(i, (x, y))| pos2(at.x + (x + wobble(i, seed, amp)) * sp, at.y + (y + wobble(i + 97, seed, amp)) * sp)).collect()
    }

    /// An oval head drawn once (open) or three times round, getting smaller (filled), slanted like a notehead.
    pub fn head(at: Pos2, sp: f32, filled: bool, seed: u32) -> Vec<Pos2> {
        let turns = if filled { 3.0 } else { 1.06 };
        let n = (turns * 28.0) as usize;
        let pts = (0..=n)
            .map(|i| {
                let t = i as f32 / 28.0 * std::f32::consts::TAU;
                let shrink = if filled { 1.0 - 0.22 * (i as f32 / n as f32) } else { 1.0 };
                let (a, b) = (0.62 * shrink, 0.42 * shrink);
                let (x, y) = (a * t.cos(), b * t.sin());
                (x * 0.94 - y * 0.34, x * 0.34 + y * 0.94)
            })
            .collect();
        place(pts, at, sp, seed)
    }

    /// A stem from the head's right side up (or left side down), 3.4 spaces.
    pub fn stem(head: Pos2, sp: f32, up: bool, seed: u32) -> Vec<Pos2> {
        let (x, y0, y1) = if up { (0.58, -0.05, -3.4) } else { (-0.58, 0.05, 3.4) };
        let pts = (0..=12).map(|i| (x + 0.03 * i as f32 / 12.0, y0 + (y1 - y0) * i as f32 / 12.0)).collect();
        place(pts, head, sp, seed)
    }

    /// A flag from the top of an up-stem, curving down to the right.
    pub fn flag(head: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts = (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0;
                (0.62 + 0.9 * (t * 1.6).sin().max(0.0), -3.35 + 1.9 * t)
            })
            .collect();
        place(pts, head, sp, seed)
    }

    /// Head and stem in one stroke: round the head, then straight up.
    pub fn quarter_one_stroke(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let mut h = head(at, sp, true, seed);
        let s = stem(at, sp, true, seed + 1);
        h.extend(s);
        h
    }

    pub fn quarter_rest(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts = vec![(-0.2, -1.5), (0.35, -0.9), (-0.3, -0.25), (0.35, 0.45), (0.05, 0.8), (-0.25, 1.05), (0.15, 1.5)];
        place(densify(&pts, 6), at, sp, seed)
    }

    pub fn sharp(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        vec![
            place(densify(&[(-0.3, -1.3), (-0.34, 1.2)], 10), at, sp, seed),
            place(densify(&[(0.3, -1.25), (0.26, 1.25)], 10), at, sp, seed + 1),
            place(densify(&[(-0.7, -0.35), (0.7, -0.65)], 8), at, sp, seed + 2),
            place(densify(&[(-0.7, 0.45), (0.7, 0.15)], 8), at, sp, seed + 3),
        ]
    }

    pub fn flat(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        vec![place(densify(&[(-0.4, -1.6), (-0.4, 0.2), (-0.4, 0.95), (0.15, 0.7), (0.42, 0.35), (0.22, 0.1), (-0.1, 0.12), (-0.4, 0.38)], 5), at, sp, seed)]
    }

    pub fn natural(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        vec![
            place(densify(&[(-0.35, -1.3), (-0.35, 0.55), (0.35, 0.35)], 7), at, sp, seed),
            place(densify(&[(-0.35, -0.35), (0.35, -0.55), (0.35, 1.3)], 7), at, sp, seed + 1),
        ]
    }

    pub fn double_sharp(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        vec![place(densify(&[(-0.45, -0.45), (0.45, 0.45)], 8), at, sp, seed), place(densify(&[(0.45, -0.45), (-0.45, 0.45)], 8), at, sp, seed + 1)]
    }

    pub fn double_flat(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        let mut a = flat(pos2(at.x - 0.45 * sp, at.y), sp, seed);
        a.extend(flat(pos2(at.x + 0.45 * sp, at.y), sp, seed + 5));
        a
    }

    /// A scribble: zigzag up and down, moving right, about two spaces wide.
    pub fn scribble(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts: Vec<(f32, f32)> = (0..=8).map(|i| (-1.0 + i as f32 * 0.25, if i % 2 == 0 { -0.7 } else { 0.7 })).collect();
        place(densify(&pts, 5), at, sp, seed)
    }

    /// A scribble back and forth along `angle` (radians from the x axis), `len` spaces long, `turns` reversals,
    /// drifting slowly across.
    pub fn scribble_dir(at: Pos2, sp: f32, angle: f32, len: f32, turns: usize, seed: u32) -> Vec<Pos2> {
        let (d, n) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
        let pts: Vec<(f32, f32)> = (0..=turns + 1)
            .map(|i| {
                let a = if i % 2 == 0 { -len / 2.0 } else { len / 2.0 };
                let drift = (i as f32 / (turns + 1) as f32 - 0.5) * 0.8;
                let p = d * a + n * drift;
                (p.x, p.y)
            })
            .collect();
        place(densify(&pts, 6), at, sp, seed)
    }

    /// Shapes that aren't in the vocabulary.
    pub fn loose_curl(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts = vec![(-1.0, 0.4), (-0.7, -0.4), (-0.4, 0.3), (-0.1, -0.4), (0.25, 0.35), (0.6, -0.1), (0.4, 0.4), (0.9, 0.2)];
        place(densify(&pts, 6), at, sp, seed)
    }

    pub fn dot(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place(vec![(0.0, 0.0), (0.06, 0.04), (0.08, 0.0)], at, sp, seed)
    }

    pub fn long_line(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place(densify(&[(-3.0, 0.2), (3.0, -0.2)], 20), at, sp, seed)
    }

    /// A huge loop, much bigger than any figure.
    pub fn big_loop(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts = (0..=40).map(|i| {
            let t = i as f32 / 40.0 * std::f32::consts::TAU;
            (6.0 * t.cos(), 5.0 * t.sin())
        });
        place(pts.collect(), at, sp, seed)
    }

    // -------- crooked, as a real hand writes ("tortos") --------

    /// How a hand bends a figure: slant (radians), stem gap and tilt, wobble.
    #[derive(Clone, Copy)]
    pub struct Hand {
        pub seed: u32,
        /// the whole figure leans by this (radians)
        pub slant: f32,
        /// the stem starts this far from the head (spaces)
        pub stem_gap: f32,
        /// the stem leans by this (radians)
        pub stem_tilt: f32,
        /// how much of the loop is left open (0..1 of a turn)
        pub open: f32,
        pub wobble: f32,
    }

    pub fn hands() -> Vec<Hand> {
        vec![
            Hand { seed: 11, slant: 0.12, stem_gap: 0.5, stem_tilt: 0.12, open: 0.12, wobble: 0.07 },
            Hand { seed: 29, slant: -0.1, stem_gap: 0.8, stem_tilt: -0.15, open: 0.2, wobble: 0.08 },
            Hand { seed: 47, slant: 0.2, stem_gap: 0.2, stem_tilt: 0.2, open: 0.05, wobble: 0.06 },
        ]
    }

    fn lean(p: Vec<(f32, f32)>, a: f32) -> Vec<(f32, f32)> {
        let (c, s) = (a.cos(), a.sin());
        p.into_iter().map(|(x, y)| (x * c - y * s, x * s + y * c)).collect()
    }

    /// A crooked head: an open one left a little open; a filled one as a quick spiral or a blot.
    pub fn head_t(at: Pos2, sp: f32, filled: bool, h: Hand) -> Vec<Pos2> {
        let pts: Vec<(f32, f32)> = if filled {
            let n = 60;
            (0..=n)
                .map(|i| {
                    let t = i as f32 / 22.0 * std::f32::consts::TAU;
                    let k = 1.0 - 0.6 * (i as f32 / n as f32);
                    (0.55 * k * t.cos(), 0.38 * k * t.sin())
                })
                .collect()
        } else {
            let n = 30;
            let span = 1.0 - h.open;
            (0..=n)
                .map(|i| {
                    let t = 0.3 + i as f32 / n as f32 * span * std::f32::consts::TAU;
                    (0.62 * t.cos(), 0.4 * t.sin())
                })
                .collect()
        };
        place_amp(lean(lean(pts, -0.35), h.slant), at, sp, h.seed, h.wobble)
    }

    /// A crooked stem: apart from the head by `stem_gap`, leaning by `stem_tilt`; `left` puts it on the head's left.
    pub fn stem_t(head: Pos2, sp: f32, up: bool, left: bool, h: Hand) -> Vec<Pos2> {
        let x0 = if left { -0.62 } else { 0.62 };
        let dir = if up { -1.0 } else { 1.0 };
        let len = 3.2;
        let pts = (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0;
                (x0 + t * len * h.stem_tilt.sin(), dir * (h.stem_gap + t * len))
            })
            .collect();
        place_amp(pts, head, sp, h.seed + 3, h.wobble * 0.5)
    }

    /// The top (far end) of a crooked up-stem.
    pub fn stem_top(head: Pos2, sp: f32, left: bool, h: Hand) -> Pos2 {
        let s = stem_t(head, sp, true, left, h);
        *s.last().unwrap()
    }

    /// A short hook from the stem's far end (`k`-th flag a space lower), to the right.
    pub fn hook_t(top: Pos2, sp: f32, k: usize, h: Hand) -> Vec<Pos2> {
        let pts = (0..=6)
            .map(|i| {
                let t = i as f32 / 6.0;
                (0.75 * (t * 1.7).sin(), k as f32 * 0.85 + 1.1 * t)
            })
            .collect();
        place_amp(pts, top, sp, h.seed + 7 + k as u32, h.wobble * 0.5)
    }

    /// A crooked note: head, stem up on the right (`left` for the left), `flags` hooks; dots after the head.
    pub fn note_t(at: Pos2, sp: f32, value: u8, dots: u8, left: bool, h: Hand) -> Vec<Vec<Pos2>> {
        let filled = value >= 3;
        let mut v = vec![head_t(at, sp, filled, h)];
        if value >= 2 {
            v.push(stem_t(at, sp, true, left, h));
            let top = stem_top(at, sp, left, h);
            for k in 0..value.saturating_sub(3) as usize {
                v.push(hook_t(top, sp, k, h));
            }
        }
        for d in 0..dots {
            v.push(dot(pos2(at.x + (1.2 + 0.6 * d as f32) * sp, at.y - 0.1 * sp), sp, h.seed + d as u32));
        }
        v
    }

    /// A crooked quarter in two strokes: the head, then the stem drawn from below a little apart.
    pub fn quarter_two_strokes_t(at: Pos2, sp: f32, h: Hand) -> Vec<Vec<Pos2>> {
        vec![head_t(at, sp, true, h), stem_t(at, sp, true, false, h)]
    }

    /// A beamed group: `n` heads with stems up and `beams` beams across their tops (drawn last, or first).
    pub fn beamed_t(at: Pos2, sp: f32, n: usize, beams: usize, beam_first: bool, h: Hand) -> Vec<Vec<Pos2>> {
        let mut heads = vec![];
        let mut tops = vec![];
        for i in 0..n {
            let a = pos2(at.x + i as f32 * 2.4 * sp, at.y - (i % 2) as f32 * 0.5 * sp);
            heads.push(head_t(a, sp, true, Hand { seed: h.seed + i as u32 * 13, ..h }));
            let hh = Hand { seed: h.seed + i as u32 * 13, stem_gap: 0.15, stem_tilt: 0.03, ..h };
            heads.push(stem_t(a, sp, true, false, hh));
            tops.push(stem_top(a, sp, false, hh));
        }
        let mut bs = vec![];
        for b in 0..beams {
            let (a, z) = (tops[0], *tops.last().unwrap());
            let off = b as f32 * 0.75 * sp;
            bs.push(place_amp(densify(&[((a.x - at.x) / sp, (a.y - at.y) / sp + off / sp), ((z.x - at.x) / sp, (z.y - at.y) / sp + off / sp)], 12), at, sp, h.seed + 50 + b as u32, 0.03));
        }
        if beam_first {
            bs.extend(heads);
            bs
        } else {
            heads.extend(bs);
            heads
        }
    }

    /// A crooked hooked rest: a slanting line with `hooks` hooks on its left near the top (an eighth in one stroke;
    /// more hooks as separate strokes).
    pub fn hooked_rest_t(at: Pos2, sp: f32, hooks: usize, h: Hand) -> Vec<Vec<Pos2>> {
        let top = (0.5, -1.0 - 0.3 * hooks as f32);
        let bottom = (-0.25, 1.3);
        let blob = |cx: f32, cy: f32| -> Vec<(f32, f32)> { (0..=10).map(|i| {
            let t = i as f32 / 10.0 * std::f32::consts::TAU * 1.2;
            (cx + 0.18 * t.cos(), cy + 0.15 * t.sin())
        }).collect() };
        let mut first: Vec<(f32, f32)> = blob(-0.35, top.1 + 0.15);
        first.extend(densify(&[(-0.35, top.1 + 0.3), (0.1, top.1 + 0.35), top], 3));
        first.extend(densify(&[top, bottom], 10));
        let mut v = vec![place_amp(first, at, sp, h.seed, h.wobble * 0.4)];
        for k in 1..hooks {
            let (cx, cy) = (top.0 + (bottom.0 - top.0) * 0.28 * k as f32 - 0.85, top.1 + (bottom.1 - top.1) * 0.28 * k as f32 + 0.1);
            let mut hk = blob(cx, cy);
            hk.extend(densify(&[(cx, cy + 0.15), (cx + 0.5, cy + 0.2), (cx + 0.82, cy - 0.05)], 3));
            v.push(place_amp(hk, at, sp, h.seed + k as u32 * 3, h.wobble * 0.4));
        }
        v
    }

    /// A crooked quarter rest.
    pub fn quarter_rest_t(at: Pos2, sp: f32, h: Hand) -> Vec<Pos2> {
        let pts = vec![(-0.25, -1.5), (0.4, -0.85), (-0.3, -0.2), (0.4, 0.5), (0.0, 0.8), (-0.3, 1.0), (0.2, 1.55)];
        place_amp(lean(densify(&pts, 6), h.slant * 0.5), at, sp, h.seed, h.wobble * 0.6)
    }

    /// A whole or half rest: a short bar filled back and forth, centred on `at`.
    pub fn block_rest(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let mut pts = vec![];
        for i in 0..5 {
            let y = -0.2 + i as f32 * 0.1;
            if i % 2 == 0 {
                pts.push((-0.55, y));
                pts.push((0.55, y));
            } else {
                pts.push((0.55, y));
                pts.push((-0.55, y));
            }
        }
        place(densify(&pts, 4), at, sp, seed)
    }

    // -------- a different hand: dashes for filled heads, two arcs for open ones, stems on the left --------

    /// A filled head drawn as a quick short dash.
    pub fn dash_head(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place_amp(densify(&[(-0.5, 0.18), (0.5, -0.18)], 6), at, sp, seed, 0.03)
    }

    /// An open head drawn as two arcs, left "(" and right ")".
    pub fn arcs_head(at: Pos2, sp: f32, seed: u32) -> Vec<Vec<Pos2>> {
        let arc = |sgn: f32| -> Vec<(f32, f32)> { (0..=8).map(|i| {
            let t = -1.2 + 2.4 * i as f32 / 8.0;
            (sgn * (0.15 + 0.45 * t.cos()), 0.42 * t.sin())
        }).collect() };
        vec![place_amp(arc(-1.0), at, sp, seed, 0.03), place_amp(arc(1.0), at, sp, seed + 1, 0.03)]
    }

    /// A note in that hand: value 1 (arcs), 2 (arcs + left stem), 3 (dash + left stem), 4 (dash + left stem + tick).
    pub fn styled_note(at: Pos2, sp: f32, value: u8, seed: u32) -> Vec<Vec<Pos2>> {
        let h = Hand { seed, slant: 0.0, stem_gap: 0.3, stem_tilt: 0.05, open: 0.0, wobble: 0.04 };
        let mut v = if value >= 3 { vec![dash_head(at, sp, seed)] } else { arcs_head(at, sp, seed) };
        if value >= 2 {
            v.push(stem_t(at, sp, true, true, h));
        }
        if value >= 4 {
            let top = stem_top(at, sp, true, h);
            v.push(place_amp(densify(&[(0.0, 0.0), (0.7, 0.9)], 5), top, sp, seed + 9, 0.02));
        }
        v
    }

    /// Points in between, so a polyline has the density of a pen sample stream.
    pub fn densify(p: &[(f32, f32)], per: usize) -> Vec<(f32, f32)> {
        let mut out = vec![];
        for w in p.windows(2) {
            for k in 0..per {
                let t = k as f32 / per as f32;
                out.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
            }
        }
        out.push(*p.last().unwrap());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::samples::*;
    use super::*;

    const SP: f32 = 7.0;

    fn at() -> Pos2 {
        pos2(300.0, 200.0)
    }

    fn ctx() -> Ctx<'static> {
        Ctx { staff_mid: Some(200.0), ..Default::default() }
    }

    fn read(strokes: &[Vec<Pos2>]) -> Vec<Candidate> {
        recognize(strokes, SP, &ctx())
    }

    fn best(strokes: &[Vec<Pos2>]) -> Option<Reading> {
        read(strokes).into_iter().next().map(|c| c.reading)
    }

    fn note1(r: &Option<Reading>) -> Option<(u8, u8, usize)> {
        match r {
            Some(Reading::Notes { cols, beamed: false }) if cols.len() == 1 => Some((cols[0].value, cols[0].dots, cols[0].heads.len())),
            _ => None,
        }
    }

    #[test]
    fn the_basic_vocabulary_is_read() {
        for seed in [1u32, 7, 23] {
            let h = at();
            assert_eq!(note1(&best(&[head(h, SP, true, seed)])).map(|n| n.0), Some(3), "filled head");
            assert_eq!(note1(&best(&[head(h, SP, true, seed), stem(h, SP, true, seed)])).map(|n| n.0), Some(3), "quarter");
            assert_eq!(note1(&best(&[head(h, SP, false, seed), stem(h, SP, true, seed)])).map(|n| n.0), Some(2), "half");
            assert_eq!(note1(&best(&[head(h, SP, false, seed), stem(h, SP, false, seed)])).map(|n| n.0), Some(2), "half, stem down");
            assert_eq!(note1(&best(&[head(h, SP, false, seed)])).map(|n| n.0), Some(1), "whole");
            assert_eq!(note1(&best(&[head(h, SP, true, seed), stem(h, SP, true, seed), flag(h, SP, seed)])).map(|n| n.0), Some(4), "eighth");
            assert_eq!(note1(&best(&[quarter_one_stroke(h, SP, seed)])).map(|n| n.0), Some(3), "head and stem in one stroke");
            assert!(matches!(best(&[quarter_rest(h, SP, seed)]), Some(Reading::Rest { value: 3, .. })), "quarter rest");
            assert!(matches!(best(&sharp(h, SP, seed)), Some(Reading::Accidental { alter: 1, .. })), "sharp");
            let fl = best(&flat(h, SP, seed));
            assert!(matches!(fl, Some(Reading::Accidental { alter: -1, .. })), "flat (seed {seed}) read as {fl:?}");
            assert!(matches!(best(&natural(h, SP, seed)), Some(Reading::Accidental { alter: 0, .. })), "natural");
        }
    }

    /// Every figure, crooked, in three hands: the best reading is right, and the right one is always in the top 3.
    #[test]
    fn every_figure_is_read_when_crooked() {
        let h0 = at();
        let mut fails = vec![];
        for hand in hands() {
            let mut check = |name: String, strokes: Vec<Vec<Pos2>>, want: &dyn Fn(&Reading) -> bool| {
                let c = read(&strokes);
                let first = c.first().map(|x| want(&x.reading)).unwrap_or(false);
                let top3 = c.iter().take(3).any(|x| want(&x.reading));
                if !first || !top3 {
                    fails.push(format!("{name} (hand {}): best {:?}, top3 ok {top3}", hand.seed, c.first().map(|x| x.reading.label())));
                }
            };
            for value in 1..=7u8 {
                for left in [false, true] {
                    check(format!("note {value} left={left}"), note_t(h0, SP, value, 0, left, hand), &|r| matches!(r, Reading::Notes { cols, beamed: false } if cols.len() == 1 && cols[0].value == value && cols[0].dots == 0));
                }
            }
            for dots in 1..=2u8 {
                check(format!("dotted quarter x{dots}"), note_t(h0, SP, 3, dots, false, hand), &|r| matches!(r, Reading::Notes { cols, .. } if cols[0].value == 3 && cols[0].dots == dots));
                check(format!("dotted half x{dots}"), note_t(h0, SP, 2, dots, false, hand), &|r| matches!(r, Reading::Notes { cols, .. } if cols[0].value == 2 && cols[0].dots == dots));
            }
            check("quarter in two strokes".into(), quarter_two_strokes_t(h0, SP, hand), &|r| matches!(r, Reading::Notes { cols, .. } if cols[0].value == 3));
            check("quarter rest".into(), vec![quarter_rest_t(h0, SP, hand)], &|r| matches!(r, Reading::Rest { value: 3, .. }));
            for hooks in 1..=4usize {
                check(format!("rest with {hooks} hooks"), hooked_rest_t(h0, SP, hooks, hand), &|r| matches!(r, Reading::Rest { value, .. } if *value as usize == 3 + hooks));
            }
            for (alter, s) in [(1i8, sharp(h0, SP, hand.seed)), (-1, flat(h0, SP, hand.seed)), (0, natural(h0, SP, hand.seed)), (2, double_sharp(h0, SP, hand.seed)), (-2, double_flat(h0, SP, hand.seed))] {
                check(format!("accidental {alter}"), s, &|r| matches!(r, Reading::Accidental { alter: a, .. } if *a == alter));
            }
        }
        assert!(fails.is_empty(), "{} misread:\n{}", fails.len(), fails.join("\n"));
    }

    #[test]
    fn whole_and_half_rests_by_their_line() {
        // the middle line is at y 200 (SP 7): the 4th line at 193; the whole rest hangs under it, the half sits on 200
        let whole = best(&[block_rest(pos2(300.0, 200.0 - 0.75 * SP), SP, 3)]);
        let half = best(&[block_rest(pos2(300.0, 200.0 - 0.25 * SP), SP, 3)]);
        assert!(matches!(whole, Some(Reading::Rest { value: 1, .. })), "whole rest read as {whole:?}");
        assert!(matches!(half, Some(Reading::Rest { value: 2, .. })), "half rest read as {half:?}");
    }

    #[test]
    fn beamed_groups_of_two_to_four() {
        for hand in hands() {
            for n in 2..=4usize {
                for beams in 1..=3usize {
                    for first in [false, true] {
                        let r = best(&beamed_t(at(), SP, n, beams, first, hand));
                        let ok = matches!(&r, Some(Reading::Notes { cols, beamed: true }) if cols.len() == n && cols.iter().all(|c| c.value as usize == 3 + beams));
                        assert!(ok, "{n} notes, {beams} beams, beam first {first}, hand {}: {:?}", hand.seed, r.map(|x| x.label()));
                    }
                }
            }
        }
    }

    #[test]
    fn a_chord_on_one_stem() {
        let h = at();
        let low = head_t(h, SP, true, hands()[0]);
        let mid = head_t(pos2(h.x + 0.1 * SP, h.y - SP), SP, true, hands()[1]);
        let hi = head_t(pos2(h.x, h.y - 2.0 * SP), SP, true, hands()[2]);
        let st: Vec<Pos2> = (0..=12).map(|i| pos2(h.x + 0.62 * SP, h.y - (0.1 + 5.2 * i as f32 / 12.0) * SP)).collect();
        let r = best(&[low, mid, hi, st]);
        assert_eq!(note1(&r), Some((3, 0, 3)), "{r:?}");
    }

    #[test]
    fn the_head_centre_gives_the_pitch_position() {
        let h = pos2(410.0, 233.5);
        for hand in hands() {
            match best(&note_t(h, SP, 3, 0, false, hand)) {
                Some(Reading::Notes { cols, .. }) => assert!(cols[0].heads[0].distance(h) < SP * 0.45, "head read at {:?}, drawn at {h:?}", cols[0].heads[0]),
                r => panic!("not read as a note: {r:?}"),
            }
        }
    }

    #[test]
    fn an_accidental_drawn_with_its_note_goes_with_it() {
        let h = at();
        let mut strokes = sharp(pos2(h.x - 2.0 * SP, h.y), SP, 4);
        strokes.push(head(h, SP, true, 4));
        strokes.push(stem(h, SP, true, 4));
        match best(&strokes) {
            Some(Reading::Notes { cols, .. }) => assert_eq!((cols[0].value, cols[0].acc), (3, Some(1))),
            r => panic!("read {r:?}"),
        }
    }

    #[test]
    fn only_what_is_clearly_not_music_is_refused() {
        for seed in [2u32, 9] {
            let h = at();
            assert!(read(&[long_line(h, SP, seed)]).is_empty(), "long line");
            assert!(read(&[stem(h, SP, true, seed)]).is_empty(), "a stem alone");
            assert!(read(&[big_loop(h, SP, seed)]).is_empty(), "too big");
            // off the staff: nothing on a guess
            let far = Ctx { staff_mid: Some(200.0 + 40.0 * SP), ..Default::default() };
            assert!(recognize(&[loose_curl(h, SP, seed)], SP, &far).is_empty(), "off the staff");
            // a note-sized curl on the staff: the best guess (a quarter), with the others offered
            let c = read(&[loose_curl(h, SP, seed)]);
            assert!(!c.is_empty() && c.len() >= 3, "a guess and alternatives: {c:?}");
        }
    }

    #[test]
    fn scribbles_turn_back_and_rests_dont_count_as_scribbles_alone() {
        let h = at();
        for (a, len) in [(0.0f32, 2.0f32), (std::f32::consts::FRAC_PI_2, 2.5), (0.7, 6.0), (2.3, 1.2)] {
            assert!(scribble_turns(&[scribble_dir(h, SP, a, len, 4, 3)], SP) >= SCRIBBLE_TURNS, "scribble at {a} rad, {len} spaces");
        }
        // a quarter rest turns back too: what tells them apart is what lies under the ink (App::pen_apply)
        assert!(scribble_turns(&[quarter_rest(h, SP, 1)], SP) >= 2);
        // a straight stroke or a single loop doesn't
        assert!(scribble_turns(&[long_line(h, SP, 1)], SP) < SCRIBBLE_TURNS);
    }

    #[test]
    fn the_context_breaks_ties() {
        // a filled head alone: a quarter by default; with only an eighth left in the bar, the eighth comes first
        let h = at();
        let c = recognize(&[head_t(h, SP, true, hands()[0])], SP, &Ctx { staff_mid: Some(200.0), remaining: Some(crate::model::TPQ / 2), neighbor: Some(4), personal: None });
        let top3: Vec<String> = c.iter().take(3).map(|x| x.reading.label()).collect();
        assert!(top3.contains(&"n4.0".to_string()), "{top3:?}");
    }

    /// Another hand: dashes for filled heads, two arcs for open heads, stems on the left. The rules alone misread
    /// it; after "Teach Fermata your handwriting" (two drawings of each), it's read right.
    #[test]
    fn a_different_hand_is_learnt() {
        let h = at();
        let right = |r: &Option<Reading>, v: u8| matches!(r, Some(Reading::Notes { cols, .. }) if cols.len() == 1 && cols[0].value == v && cols[0].heads[0].distance(h) < 0.5 * SP);
        let values = [1u8, 2, 3, 4];
        let before: Vec<bool> = values.iter().map(|&v| right(&best(&styled_note(h, SP, v, 101)), v)).collect();
        assert!(before.iter().all(|ok| !ok), "the rules alone should misread this hand: {before:?}");
        let mut per = Personal::default();
        for &v in &values {
            for seed in [5u32, 17] {
                let reading = Reading::Notes { cols: vec![Col { heads: vec![h], value: v, dots: 0, acc: None, acc_head: 0 }], beamed: false };
                per.learn(&styled_note(h, SP, v, seed), SP, &reading);
            }
        }
        let c = Ctx { staff_mid: Some(200.0), personal: Some(&per), ..Default::default() };
        for &v in &values {
            let r = recognize(&styled_note(h, SP, v, 101), SP, &c).into_iter().next().map(|x| x.reading);
            assert!(right(&r, v), "value {v} after learning: {r:?}");
        }
        // and the usual hand still reads
        let r = recognize(&note_t(h, SP, 3, 0, false, hands()[0]), SP, &c).into_iter().next().map(|x| x.reading);
        assert_eq!(note1(&r).map(|n| n.0), Some(3));
    }


    #[test]
    fn strokes_close_in_time_are_one_symbol() {
        assert_eq!(group_by_time(&[(0.0, 0.3), (0.8, 1.0)], DEBOUNCE), vec![vec![0, 1]]);
        assert_eq!(group_by_time(&[(0.0, 0.3), (1.3, 1.5)], DEBOUNCE), vec![vec![0], vec![1]]);
        assert_eq!(group_by_time(&[(0.0, 0.2), (0.7, 0.9), (1.4, 1.6)], DEBOUNCE), vec![vec![0, 1, 2]]);
    }
}
