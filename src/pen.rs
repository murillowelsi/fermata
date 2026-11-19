//! F-38: writing with a pen (Apple Pencil, S Pen, USI, or a passive pen in Pen Mode) — the recognizer, phase 1.
//!
//! Strokes that come close together in time are one symbol (a head, then its stem, then a flag): after the pen
//! lifts, the ink waits `DEBOUNCE` seconds; a new stroke in that time joins the same symbol and starts the wait again.
//! Then the group is read, offline, with Fermata's own code:
//! - **notes** by shape rules: an open or filled head (a closed curve, filled when drawn over itself), a stem (a
//!   near-vertical line about an octave long touching the head) and a flag (a short curve from the stem's end). A
//!   single stroke may draw head and stem together: it is split where that reads best;
//! - **♯ ♭ ♮** by a point-cloud matcher written from the published description of the $P recognizer (Vatavu, Anthony
//!   and Wobbrock, 2012): the strokes are resampled into a cloud of points, normalized, and compared with templates
//!   drawn here, whatever the order and direction of the strokes;
//! - the **quarter rest** (a zigzag going down) and the **scribble** (a zigzag over a note: delete) by rules.
//!
//! Anything else is "not recognized": nothing is ever inserted on a guess. Every limit below is a starting point to
//! calibrate with a real pen.
use egui::{pos2, vec2, Pos2, Rect};

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
/// A scribble deletes a note when it covers at least this share of the notehead's width.
pub const SCRIBBLE_COVER: f32 = 0.6;

/// One stroke: its points (screen points, y down) and when the pen touched and lifted (seconds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub pts: Vec<Pos2>,
    pub t0: f64,
    pub t1: f64,
}

/// Groups strokes (in the order drawn) into symbols (the rule `App::pen_tick` follows live; the tests check it here): a stroke that starts less than `gap` seconds after the previous
/// one lifted belongs to the same symbol.
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

/// What a group of strokes reads as.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A note: 1 whole, 2 half, 3 quarter, 4 eighth; the head's centre; an accidental written with it.
    Note { value: u8, head: Pos2, acc: Option<i8> },
    /// A quarter rest, at the centre of the zigzag.
    Rest { at: Pos2 },
    /// ♯ ♭ ♮ alone (it waits for its head), its centre and right edge.
    Accidental { alter: i8, at: Pos2, right: f32 },
    /// A scribble: deletes the note it covers (decided against the score).
    Scribble { rect: Rect },
    Unknown,
}

impl Shape {
    pub fn value_name(value: u8) -> &'static str {
        match value {
            1 => "whole note",
            2 => "half note",
            3 => "quarter note",
            4 => "eighth note",
            _ => "note",
        }
    }
}

// ---------------- geometry helpers (staff-space units) ----------------

fn bbox(p: &[Pos2]) -> Rect {
    let mut r = Rect::NOTHING;
    for q in p {
        r.extend_with(*q);
    }
    r
}

fn path_len(p: &[Pos2]) -> f32 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// `n` points evenly spaced along the path.
fn resample(p: &[Pos2], n: usize) -> Vec<Pos2> {
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

/// How many times the direction along x (or y) turns back, ignoring wobbles smaller than `h`.
fn reversals(p: &[Pos2], along_x: bool, h: f32) -> usize {
    let v = |q: &Pos2| if along_x { q.x } else { q.y };
    let mut n = 0;
    let mut dir = 0i8;
    let mut anchor = v(&p[0]);
    for q in p.iter().skip(1) {
        let d = v(q) - anchor;
        if d.abs() >= h {
            let nd = if d > 0.0 { 1 } else { -1 };
            if dir != 0 && nd != dir {
                n += 1;
            }
            dir = nd;
            anchor = v(q);
        } else if (dir > 0 && v(q) > anchor) || (dir < 0 && v(q) < anchor) {
            anchor = v(q);
        }
    }
    n
}

/// Head: a small closed curve. Some(true) filled (drawn over itself), Some(false) open.
fn head_kind(p: &[Pos2]) -> Option<bool> {
    let r = bbox(p);
    let (w, h) = (r.width(), r.height());
    if !(0.5..=2.8).contains(&w) || !(0.35..=2.2).contains(&h) || w * h < 0.25 {
        return None;
    }
    // a head goes round (it turns back on both axes); a zigzag turns back on one only
    if is_zigzag(p) {
        return None;
    }
    let len = path_len(p);
    let perimeter = std::f32::consts::PI * (w + h) / 2.0;
    let gap = p[0].distance(*p.last().unwrap());
    let ratio = len / perimeter;
    if ratio < 0.7 {
        return None;
    }
    if ratio >= 1.75 {
        return Some(true);
    }
    // one loop: it must close
    if gap <= 0.45 * w.max(h) {
        Some(false)
    } else {
        None
    }
}

/// Back and forth along one axis (4 or more turns) with the other axis turning at most once.
fn is_zigzag(p: &[Pos2]) -> bool {
    let p = resample(p, 64);
    let (xr, yr) = (reversals(&p, true, 0.25), reversals(&p, false, 0.25));
    xr.max(yr) >= 4 && xr.min(yr) <= 1
}

/// Stem: a near-vertical line about an octave long.
fn is_stem(p: &[Pos2]) -> bool {
    let r = bbox(p);
    let (w, h) = (r.width(), r.height());
    (2.2..=5.5).contains(&h) && w <= (0.3 * h).min(0.9) && path_len(p) <= 1.35 * h
}

/// Flag: a short open curve.
fn is_flag(p: &[Pos2]) -> bool {
    let r = bbox(p);
    let (w, h) = (r.width(), r.height());
    let len = path_len(p);
    (0.35..=2.2).contains(&w) && (0.5..=3.2).contains(&h) && len <= 5.0 && p[0].distance(*p.last().unwrap()) >= 0.5
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Head(bool),
    Stem,
    Flag,
}

fn classify(p: &[Pos2]) -> Vec<Part> {
    let mut v = vec![];
    if let Some(f) = head_kind(p) {
        v.push(Part::Head(f));
    }
    if is_stem(p) {
        v.push(Part::Stem);
    }
    if is_flag(p) {
        v.push(Part::Flag);
    }
    v
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

/// Reads strokes (staff-space units) as a note: one head, at most one stem touching it, and a flag only on a filled
/// head with a stem. Returns the value and the head's centre.
fn read_note(strokes: &[Vec<Pos2>]) -> Option<(u8, Pos2, f32)> {
    let rs: Vec<Vec<Pos2>> = strokes.iter().map(|s| resample(s, 48)).collect();
    let max_parts = match rs.len() {
        1 => 3,
        2 => 2,
        _ => 1,
    };
    // the pieces each stroke can be cut into
    let options: Vec<Vec<Vec<Vec<Pos2>>>> = rs
        .iter()
        .map(|s| {
            cuts(s.len(), max_parts)
                .into_iter()
                .map(|c| {
                    let mut pieces = vec![];
                    let mut from = 0;
                    for &k in &c {
                        pieces.push(s[from..=k].to_vec());
                        from = k;
                    }
                    pieces.push(s[from..].to_vec());
                    pieces
                })
                .collect()
        })
        .collect();
    let mut best: Option<(u8, Pos2, f32, f32)> = None; // value, head, head width, score (lower is better)
    let mut idx = vec![0usize; options.len()];
    loop {
        let pieces: Vec<&Vec<Pos2>> = idx.iter().enumerate().flat_map(|(i, &k)| options[i][k].iter()).collect();
        if pieces.len() <= 3 {
            if let Some(found) = compose(&pieces) {
                if best.map(|b| found.3 < b.3).unwrap_or(true) {
                    best = Some(found);
                }
            }
        }
        // next combination
        let mut i = 0;
        loop {
            if i == idx.len() {
                return best.map(|b| (b.0, b.1, b.2));
            }
            idx[i] += 1;
            if idx[i] < options[i].len() {
                break;
            }
            idx[i] = 0;
            i += 1;
        }
    }
}

/// One way of seeing the pieces as head / stem / flag: Some((value, head centre, head width, cut count)).
fn compose(pieces: &[&Vec<Pos2>]) -> Option<(u8, Pos2, f32, f32)> {
    // every piece must be something, and each kind at most once
    let kinds: Vec<Vec<Part>> = pieces.iter().map(|p| classify(p)).collect();
    if kinds.iter().any(|k| k.is_empty()) {
        return None;
    }
    let n = pieces.len();
    // try every assignment of a kind to each piece
    let mut choice = vec![0usize; n];
    let mut found: Option<(u8, Pos2, f32, f32)> = None;
    loop {
        let parts: Vec<Part> = (0..n).map(|i| kinds[i][choice[i]]).collect();
        let heads: Vec<usize> = (0..n).filter(|&i| matches!(parts[i], Part::Head(_))).collect();
        let stems: Vec<usize> = (0..n).filter(|&i| parts[i] == Part::Stem).collect();
        let flags: Vec<usize> = (0..n).filter(|&i| parts[i] == Part::Flag).collect();
        if heads.len() == 1 && stems.len() <= 1 && flags.len() <= 1 {
            let h = heads[0];
            let filled = matches!(parts[h], Part::Head(true));
            let hr = bbox(pieces[h]);
            let mut ok = true;
            let mut stem_r = None;
            if let Some(&s) = stems.first() {
                let sr = bbox(pieces[s]);
                // the stem touches the head: up from its right side or down from its left side
                // (a ♭'s line stands on the left of its bowl, going up: neither)
                let up = (sr.bottom() - hr.center().y).abs() <= 1.0 && (sr.center().x - hr.right()).abs() <= 0.9 && sr.center().x > hr.center().x;
                let down = (sr.top() - hr.center().y).abs() <= 1.0 && (sr.center().x - hr.left()).abs() <= 0.9 && sr.center().x < hr.center().x;
                ok &= up || down;
                stem_r = Some((sr, up));
            }
            let value = match (filled, stems.len(), flags.len()) {
                (true, _, 0) => 3,
                (true, 1, 1) => 4,
                (false, 1, 0) => 2,
                (false, 0, 0) => 1,
                _ => {
                    ok = false;
                    0
                }
            };
            if let (Some(&f), Some((sr, up))) = (flags.first(), stem_r) {
                // the flag leaves the stem's far end, to the right
                let fr = bbox(pieces[f]);
                let end = if up { pos2(sr.center().x, sr.top()) } else { pos2(sr.center().x, sr.bottom()) };
                let near = pieces[f].first().map(|p| p.distance(end)).unwrap_or(9.0).min(pieces[f].last().map(|p| p.distance(end)).unwrap_or(9.0));
                ok &= near <= 1.0 && fr.center().x >= sr.center().x - 0.2;
            }
            if ok {
                let score = n as f32;
                if found.map(|x| score < x.3).unwrap_or(true) {
                    found = Some((value, hr.center(), hr.width(), score));
                }
            }
        }
        let mut i = 0;
        loop {
            if i == n {
                return found;
            }
            choice[i] += 1;
            if choice[i] < kinds[i].len() {
                break;
            }
            choice[i] = 0;
            i += 1;
        }
    }
}

// ---------------- point clouds ($P, written from the paper) ----------------

const CLOUD_N: usize = 32;

/// A cloud of `CLOUD_N` points from all strokes, scaled to a unit size and centred on its centroid.
fn cloud(strokes: &[Vec<Pos2>]) -> Vec<Pos2> {
    let total: f32 = strokes.iter().map(|s| path_len(s)).sum();
    let mut pts = vec![];
    for s in strokes {
        let share = if total > 0.0 { ((path_len(s) / total) * CLOUD_N as f32).round().max(2.0) as usize } else { CLOUD_N / strokes.len().max(1) };
        pts.extend(resample(s, share));
    }
    // exactly CLOUD_N points
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

/// The greedy cloud distance of $P: each point of one cloud matched to the nearest unmatched point of the other,
/// earlier matches weighing more, from several starting points; the smaller of both directions.
fn cloud_distance(a: &[Pos2], b: &[Pos2]) -> f32 {
    let n = a.len();
    let step = ((n as f32).sqrt() as usize).max(1);
    let mut best = f32::MAX;
    let mut i = 0;
    while i < n {
        best = best.min(greedy(a, b, i)).min(greedy(b, a, i));
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

/// Templates for ♯ ♭ ♮ (unit box, y down), drawn for Fermata.
fn templates() -> Vec<(i8, Vec<Vec<Pos2>>)> {
    let l = |pts: &[(f32, f32)]| -> Vec<Pos2> { pts.iter().map(|&(x, y)| pos2(x, y)).collect() };
    vec![
        // ♯: two verticals and two rising bars
        (1, vec![l(&[(0.38, 0.0), (0.35, 1.0)]), l(&[(0.68, 0.02), (0.65, 0.98)]), l(&[(0.1, 0.42), (0.92, 0.32)]), l(&[(0.1, 0.72), (0.92, 0.62)])]),
        // ♭: a long vertical, then the bowl at its foot, to the right
        (-1, vec![l(&[(0.25, 0.0), (0.25, 0.55), (0.25, 1.0), (0.55, 0.86), (0.72, 0.68), (0.62, 0.55), (0.42, 0.56), (0.25, 0.7)])]),
        // ♮: two L shapes
        (0, vec![l(&[(0.3, 0.0), (0.3, 0.72), (0.72, 0.6)]), l(&[(0.3, 0.38), (0.72, 0.26), (0.72, 1.0)])]),
        // ♮ in one stroke (down, across, down)
        (0, vec![l(&[(0.3, 0.0), (0.3, 0.72), (0.72, 0.6), (0.72, 1.0)]), l(&[(0.3, 0.38), (0.72, 0.26)])]),
    ]
}

/// ♯ ♭ ♮ by the point cloud, when the size fits and the match is clear.
fn read_accidental(strokes: &[Vec<Pos2>]) -> Option<i8> {
    let all: Vec<Pos2> = strokes.iter().flatten().copied().collect();
    let r = bbox(&all);
    if !(1.4..=4.8).contains(&r.height()) || !(0.4..=2.4).contains(&r.width()) || strokes.len() > 4 {
        return None;
    }
    let c = cloud(strokes);
    let mut scores: Vec<(i8, f32)> = templates().into_iter().map(|(alter, t)| (alter, cloud_distance(&c, &cloud(&t)))).collect();
    scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let (alter, best) = scores[0];
    let second = scores.iter().find(|s| s.0 != alter).map(|s| s.1).unwrap_or(f32::MAX);
    // a clear match: close to its template, and clearly closer than to another symbol
    (best < 2.6 && best * 1.25 < second).then_some(alter)
}

/// A quarter rest: one zigzag going down, taller than wide.
fn read_rest(strokes: &[Vec<Pos2>]) -> Option<Pos2> {
    if strokes.len() != 1 {
        return None;
    }
    let p = resample(&strokes[0], 64);
    let r = bbox(&p);
    let ok = (1.6..=4.2).contains(&r.height()) && (0.4..=2.0).contains(&r.width()) && r.height() >= 1.4 * r.width() && (2..=6).contains(&reversals(&p, true, 0.25)) && reversals(&p, false, 0.35) <= 1;
    ok.then(|| r.center())
}

/// A scribble: a zigzag over a small area, at least about as wide as tall (a quarter rest is a tall, narrow one).
fn read_scribble(strokes: &[Vec<Pos2>]) -> Option<Rect> {
    let all: Vec<Pos2> = strokes.iter().flatten().copied().collect();
    let r = bbox(&all);
    let zig = strokes.iter().any(|s| is_zigzag(s));
    (zig && r.width() >= 0.8 && r.width() >= 0.9 * r.height() && r.width() <= 6.0 && r.height() <= 4.0 && path_len(&all) >= 3.0 * r.width().max(r.height())).then_some(r)
}

/// Reads a group of strokes (screen points, y down) at staff space `sp`.
pub fn recognize(strokes: &[Vec<Pos2>], sp: f32) -> Shape {
    let strokes: Vec<Vec<Pos2>> = strokes.iter().filter(|s| s.len() >= 2).cloned().collect();
    if strokes.is_empty() || sp <= 0.0 {
        return Shape::Unknown;
    }
    let to_sp = |s: &Vec<Pos2>| -> Vec<Pos2> { s.iter().map(|p| pos2(p.x / sp, p.y / sp)).collect() };
    let s: Vec<Vec<Pos2>> = strokes.iter().map(to_sp).collect();
    let back = |p: Pos2| pos2(p.x * sp, p.y * sp);
    if let Some(r) = read_scribble(&s) {
        return Shape::Scribble { rect: Rect::from_min_max(back(r.min), back(r.max)) };
    }
    if let Some((value, head, _)) = read_note(&s) {
        return Shape::Note { value, head: back(head), acc: None };
    }
    if let Some(at) = read_rest(&s) {
        return Shape::Rest { at: back(at) };
    }
    if let Some(alter) = read_accidental(&s) {
        let r = bbox(&s.iter().flatten().copied().collect::<Vec<_>>());
        return Shape::Accidental { alter, at: back(r.center()), right: r.right() * sp };
    }
    // an accidental and its note in one group: the strokes on the left are the accidental
    if s.len() >= 2 {
        let mut order: Vec<usize> = (0..s.len()).collect();
        order.sort_by(|&a, &b| bbox(&s[a]).center().x.partial_cmp(&bbox(&s[b]).center().x).unwrap());
        for k in 1..order.len() {
            let left: Vec<Vec<Pos2>> = order[..k].iter().map(|&i| s[i].clone()).collect();
            let right: Vec<Vec<Pos2>> = order[k..].iter().map(|&i| s[i].clone()).collect();
            if let (Some(alter), Some((value, head, hw))) = (read_accidental(&left), read_note(&right)) {
                let lr = bbox(&left.iter().flatten().copied().collect::<Vec<_>>());
                if lr.right() <= head.x - hw * 0.2 && head.x - hw / 2.0 - lr.right() <= ACCIDENTAL_REACH {
                    return Shape::Note { value, head: back(head), acc: Some(alter) };
                }
            }
        }
    }
    Shape::Unknown
}

/// Test strokes: shapes as a hand draws them (wobbly, not quite closed, slightly slanted), recorded as point lists in
/// staff spaces around (0, 0) and placed with `at`. They feed the tests and the screenshots.
#[cfg_attr(not(test), allow(dead_code))]
pub mod samples {
    use super::*;

    /// A small deterministic wobble (no randomness: the same every run).
    fn wobble(i: usize, seed: u32, amp: f32) -> f32 {
        let x = (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed.wrapping_mul(40_503));
        ((x >> 8) % 1000) as f32 / 1000.0 * 2.0 * amp - amp
    }

    fn place(p: Vec<(f32, f32)>, at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        p.into_iter().enumerate().map(|(i, (x, y))| pos2(at.x + (x + wobble(i, seed, 0.04)) * sp, at.y + (y + wobble(i + 97, seed, 0.04)) * sp)).collect()
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
                // slant up to the right, about 20°
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

    /// A scribble: zigzag up and down, moving right, about two spaces wide.
    pub fn scribble(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        let pts: Vec<(f32, f32)> = (0..=8).map(|i| (-1.0 + i as f32 * 0.25, if i % 2 == 0 { -0.7 } else { 0.7 })).collect();
        place(densify(&pts, 5), at, sp, seed)
    }

    /// Shapes that aren't in the vocabulary.
    pub fn loose_curl(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        // an "m" with a tail, about the size of a head
        let pts = vec![(-1.0, 0.4), (-0.7, -0.4), (-0.4, 0.3), (-0.1, -0.4), (0.25, 0.35), (0.6, -0.1), (0.4, 0.4), (0.9, 0.2)];
        place(densify(&pts, 6), at, sp, seed)
    }

    pub fn dot(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place(vec![(0.0, 0.0), (0.06, 0.04), (0.08, 0.0)], at, sp, seed)
    }

    pub fn long_line(at: Pos2, sp: f32, seed: u32) -> Vec<Pos2> {
        place(densify(&[(-3.0, 0.2), (3.0, -0.2)], 20), at, sp, seed)
    }

    /// Points in between, so a polyline has the density of a pen sample stream.
    fn densify(p: &[(f32, f32)], per: usize) -> Vec<(f32, f32)> {
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

    fn value_of(s: &Shape) -> Option<u8> {
        if let Shape::Note { value, .. } = s {
            Some(*value)
        } else {
            None
        }
    }

    #[test]
    fn the_vocabulary_is_read() {
        for seed in [1u32, 7, 23] {
            let h = at();
            // filled head = quarter (with or without stem), open + stem = half, open alone = whole, + flag = eighth
            assert_eq!(value_of(&recognize(&[head(h, SP, true, seed)], SP)), Some(3), "filled head");
            assert_eq!(value_of(&recognize(&[head(h, SP, true, seed), stem(h, SP, true, seed)], SP)), Some(3), "quarter");
            assert_eq!(value_of(&recognize(&[head(h, SP, false, seed), stem(h, SP, true, seed)], SP)), Some(2), "half");
            assert_eq!(value_of(&recognize(&[head(h, SP, false, seed), stem(h, SP, false, seed)], SP)), Some(2), "half, stem down");
            assert_eq!(value_of(&recognize(&[head(h, SP, false, seed)], SP)), Some(1), "whole");
            assert_eq!(value_of(&recognize(&[head(h, SP, true, seed), stem(h, SP, true, seed), flag(h, SP, seed)], SP)), Some(4), "eighth");
            assert_eq!(value_of(&recognize(&[quarter_one_stroke(h, SP, seed)], SP)), Some(3), "head and stem in one stroke");
            assert!(matches!(recognize(&[quarter_rest(h, SP, seed)], SP), Shape::Rest { .. }), "quarter rest");
            assert!(matches!(recognize(&sharp(h, SP, seed), SP), Shape::Accidental { alter: 1, .. }), "sharp");
            let fl = recognize(&flat(h, SP, seed), SP);
            assert!(matches!(fl, Shape::Accidental { alter: -1, .. }), "flat (seed {seed}) read as {fl:?}");
            assert!(matches!(recognize(&natural(h, SP, seed), SP), Shape::Accidental { alter: 0, .. }), "natural");
            let sc = recognize(&[scribble(h, SP, seed)], SP);
            assert!(matches!(sc, Shape::Scribble { .. }), "scribble read as {sc:?}");
        }
    }

    #[test]
    fn the_head_centre_gives_the_pitch_position() {
        let h = pos2(410.0, 233.5);
        if let Shape::Note { head, .. } = recognize(&[head(h, SP, true, 3), stem(h, SP, true, 3)], SP) {
            assert!(head.distance(h) < SP * 0.3, "head read at {head:?}, drawn at {h:?}");
        } else {
            panic!("not read as a note");
        }
    }

    #[test]
    fn an_accidental_drawn_with_its_note_goes_with_it() {
        let h = at();
        let mut strokes = sharp(pos2(h.x - 2.0 * SP, h.y), SP, 4);
        strokes.push(head(h, SP, true, 4));
        strokes.push(stem(h, SP, true, 4));
        assert_eq!(recognize(&strokes, SP), match recognize(&strokes, SP) {
            Shape::Note { value: 3, acc: Some(1), head } => Shape::Note { value: 3, acc: Some(1), head },
            other => panic!("read {other:?}"),
        });
    }

    #[test]
    fn what_is_not_in_the_vocabulary_is_not_recognized() {
        for seed in [2u32, 9] {
            let h = at();
            for (name, s) in [("curl", vec![loose_curl(h, SP, seed)]), ("dot", vec![dot(h, SP, seed)]), ("long line", vec![long_line(h, SP, seed)])] {
                assert_eq!(recognize(&s, SP), Shape::Unknown, "{name}");
            }
            // an open head with a flag isn't a note
            assert_eq!(value_of(&recognize(&[head(h, SP, false, seed), stem(h, SP, true, seed), flag(h, SP, seed)], SP)), None);
            // a stem alone
            assert_eq!(recognize(&[stem(h, SP, true, seed)], SP), Shape::Unknown);
        }
    }

    #[test]
    fn strokes_close_in_time_are_one_symbol() {
        // head at 0.0–0.3 s, stem 0.5 s after: one symbol; with a second's pause: two
        assert_eq!(group_by_time(&[(0.0, 0.3), (0.8, 1.0)], DEBOUNCE), vec![vec![0, 1]]);
        assert_eq!(group_by_time(&[(0.0, 0.3), (1.3, 1.5)], DEBOUNCE), vec![vec![0], vec![1]]);
        // each new stroke starts the wait again: three strokes 0.5 s apart stay one symbol
        assert_eq!(group_by_time(&[(0.0, 0.2), (0.7, 0.9), (1.4, 1.6)], DEBOUNCE), vec![vec![0, 1, 2]]);
    }
}
