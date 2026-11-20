//! F-40: a small trained classifier for one handwritten symbol (the ink of one note, rest, accidental or dot).
//!
//! - **Features** (`features`): the ink scaled into a box, as a 12×12 picture of where the ink is, a 4×4 grid of
//!   stroke directions (4 directions), and a few numbers about its size and shape (size in staff spaces, how
//!   wide, how many strokes, how much it turns, where the ink weighs most). The order and direction of the
//!   strokes don't matter to the picture.
//! - **Model**: one hidden layer of 64 (ReLU), softmax over `CLASSES`. About 16 000 numbers, kept in
//!   `assets/pen_model.bin` (little-endian f32) and read at start; inference is a few thousand multiply-adds.
//! - **Training** (`fermata --train-pen-model <file>`, native only): plain mini-batch SGD with momentum on
//!   synthetic ink drawn here by many random "hands" (heads as loops, spirals, blots or dashes; stems either side,
//!   apart, leaning; hooks of many shapes; rests; accidentals; dots; and scribbles and lines as "other"), with
//!   random slant, shear, scale, jitter and pen sampling. No outside data: the public pen-stroke music sets
//!   (HOMUS) carry no licence.
//! - **Evaluation** (`fermata --eval-pen-model`): accuracy per class on ink the training never saw: other seeds of
//!   the generator, and the hand-made test shapes of `pen::samples` (another family of drawings).
use crate::pen::{bbox, path_len, resample};
use egui::{pos2, vec2, Pos2};

/// What the classifier tells apart. `rblock` is the whole/half rest block (the staff line decides which).
pub const CLASSES: [&str; 20] = ["n1.0", "n2.0", "n3.0", "n4.0", "n5.0", "n6.0", "n7.0", "rblock", "r3.0", "r4.0", "r5.0", "r6.0", "r7.0", "a1", "a-1", "a0", "a2", "a-2", "dot", "other"];
pub const OTHER: usize = 19;
pub const DOT: usize = 18;

const GRID: usize = 12;
const ZONES: usize = 4;
const DIRS: usize = 4;
const N_GLOBAL: usize = 14;
pub const N_FEAT: usize = GRID * GRID + ZONES * ZONES * DIRS + N_GLOBAL;
pub const N_HIDDEN: usize = 96;
pub const N_OUT: usize = CLASSES.len();

/// The model's numbers, in this order: feature means, feature spreads, W1 (hidden × features), b1, W2 (out ×
/// hidden), b2.
pub struct Model {
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: Vec<f32>,
}

const MODEL_BYTES: &[u8] = include_bytes!("../assets/pen_model.bin");

thread_local! {
    static MODEL: Option<Model> = Model::from_bytes(MODEL_BYTES);
}

impl Model {
    fn sizes() -> [usize; 6] {
        [N_FEAT, N_FEAT, N_HIDDEN * N_FEAT, N_HIDDEN, N_OUT * N_HIDDEN, N_OUT]
    }

    pub fn from_bytes(b: &[u8]) -> Option<Model> {
        let total: usize = Self::sizes().iter().sum();
        if b.len() != total * 4 {
            return None;
        }
        let f: Vec<f32> = b.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
        let mut at = 0;
        let mut take = |n: usize| {
            let v = f[at..at + n].to_vec();
            at += n;
            v
        };
        let s = Self::sizes();
        Some(Model { mean: take(s[0]), std: take(s[1]), w1: take(s[2]), b1: take(s[3]), w2: take(s[4]), b2: take(s[5]) })
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn to_bytes(&self) -> Vec<u8> {
        [&self.mean, &self.std, &self.w1, &self.b1, &self.w2, &self.b2].iter().flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes())).collect()
    }

    /// The hidden layer and the class probabilities for (already standardized) features.
    fn forward(&self, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut h = vec![0.0f32; N_HIDDEN];
        for (j, hj) in h.iter_mut().enumerate() {
            let row = &self.w1[j * N_FEAT..(j + 1) * N_FEAT];
            let s: f32 = row.iter().zip(x).map(|(w, v)| w * v).sum::<f32>() + self.b1[j];
            *hj = s.max(0.0);
        }
        let mut o = vec![0.0f32; N_OUT];
        for (k, ok) in o.iter_mut().enumerate() {
            let row = &self.w2[k * N_HIDDEN..(k + 1) * N_HIDDEN];
            *ok = row.iter().zip(&h).map(|(w, v)| w * v).sum::<f32>() + self.b2[k];
        }
        (h, softmax(&o))
    }

    fn standardize(&self, f: &[f32]) -> Vec<f32> {
        f.iter().zip(self.mean.iter().zip(&self.std)).map(|(v, (m, s))| (v - m) / s.max(1e-4)).collect()
    }

    pub fn predict(&self, strokes_sp: &[Vec<Pos2>]) -> Vec<f32> {
        let x = self.standardize(&features(strokes_sp));
        self.forward(&x).1
    }
}

fn softmax(o: &[f32]) -> Vec<f32> {
    let m = o.iter().copied().fold(f32::MIN, f32::max);
    let e: Vec<f32> = o.iter().map(|v| (v - m).exp()).collect();
    let s: f32 = e.iter().sum();
    e.iter().map(|v| v / s).collect()
}

/// The class probabilities of one symbol (strokes in staff spaces), or None when no model is built in.
pub fn classify(strokes_sp: &[Vec<Pos2>]) -> Option<Vec<f32>> {
    if strokes_sp.iter().all(|s| s.len() < 2) {
        return None;
    }
    MODEL.with(|m| m.as_ref().map(|m| m.predict(strokes_sp)))
}

pub fn model_loaded() -> bool {
    MODEL.with(|m| m.is_some())
}

/// The class of a reading of the rules (`pen::recognize`), or None.
pub fn class_of(r: &crate::pen::Reading) -> Option<usize> {
    use crate::pen::Reading;
    Some(match r {
        Reading::Notes { cols, .. } => (cols.first()?.value.clamp(1, 7) - 1) as usize,
        Reading::Rest { value: 1 | 2, .. } => 7,
        Reading::Rest { value, .. } => 8 + (*value as usize).clamp(3, 7) - 3,
        Reading::Accidental { alter, .. } => match alter {
            1 => 13,
            -1 => 14,
            0 => 15,
            2 => 16,
            _ => 17,
        },
        Reading::Dot { .. } => DOT,
    })
}

/// How much the trained model counts against the rules (and the writer's own models, which come through them).
pub const MODEL_WEIGHT: f32 = 0.75;

/// The class probabilities of one symbol from the model and the rules together (strokes in staff spaces).
pub fn ensemble(strokes_sp: &[Vec<Pos2>], ctx: &crate::pen::Ctx) -> Vec<f32> {
    let cands = crate::pen::recognize(strokes_sp, 1.0, ctx);
    let mut pr = vec![0.0f32; N_OUT];
    for c in &cands {
        if let Some(k) = class_of(&c.reading) {
            pr[k] = pr[k].max(c.score);
        }
    }
    let s: f32 = pr.iter().sum();
    if s <= 1e-6 {
        pr[OTHER] = 1.0;
    } else {
        for v in pr.iter_mut() {
            *v /= s;
        }
    }
    // a strong match with one of the writer's own figures wins (their handwriting comes first): the writer's models
    // changed the reading, or raised it to a sure one
    let personal_hit = ctx.personal.is_some_and(|p| p.count() > 0) && {
        let plain = crate::pen::recognize(strokes_sp, 1.0, &crate::pen::Ctx { personal: None, ..*ctx });
        match (cands.first(), plain.first()) {
            (Some(a), Some(b)) => a.score >= 0.8 && (a.reading.label() != b.reading.label() || a.score > b.score + 0.05),
            (Some(a), None) => a.score >= 0.8,
            _ => false,
        }
    };
    match classify(strokes_sp) {
        Some(pm) if !personal_hit => pm.iter().zip(&pr).map(|(a, b)| MODEL_WEIGHT * a + (1.0 - MODEL_WEIGHT) * b).collect(),
        Some(pm) => pm.iter().zip(&pr).map(|(a, b)| 0.25 * a + 0.75 * b).collect(),
        None => pr,
    }
}

// ---------------- features ----------------

/// The features of one symbol (strokes in staff spaces).
pub fn features(strokes: &[Vec<Pos2>]) -> Vec<f32> {
    let strokes: Vec<&Vec<Pos2>> = strokes.iter().filter(|s| !s.is_empty()).collect();
    let mut f = vec![0.0f32; N_FEAT];
    if strokes.is_empty() {
        return f;
    }
    let all: Vec<Pos2> = strokes.iter().flat_map(|s| s.iter().copied()).collect();
    let r = bbox(&all);
    let (w, h) = (r.width(), r.height());
    let scale = w.max(h).max(0.3);
    let c = r.center();
    let norm = |p: Pos2| ((p - c) / scale).to_pos2(); // in -0.5..0.5 along the longer side
    // the picture: ink deposited along each segment
    let mut total_len = 0.0;
    let mut dir = [0.0f32; ZONES * ZONES * DIRS];
    for s in &strokes {
        let pts: Vec<Pos2> = s.iter().map(|p| norm(*p)).collect();
        if pts.len() == 1 {
            put(&mut f, pts[0], 1.0);
        }
        for wseg in pts.windows(2) {
            let (a, b) = (wseg[0], wseg[1]);
            let l = a.distance(b);
            if l < 1e-6 {
                continue;
            }
            total_len += l;
            let n = (l / 0.02).ceil() as usize;
            let ang = (b.y - a.y).atan2(b.x - a.x).rem_euclid(std::f32::consts::PI);
            let d = ((ang / (std::f32::consts::PI / DIRS as f32)).round() as usize) % DIRS;
            for k in 0..=n {
                let p = a + (b - a) * (k as f32 / n as f32);
                put(&mut f, p, 1.0);
                let zx = (((p.x + 0.5) * ZONES as f32) as isize).clamp(0, ZONES as isize - 1) as usize;
                let zy = (((p.y + 0.5) * ZONES as f32) as isize).clamp(0, ZONES as isize - 1) as usize;
                dir[(zy * ZONES + zx) * DIRS + d] += l / (n + 1) as f32;
            }
        }
    }
    for v in f.iter_mut().take(GRID * GRID) {
        *v = v.min(1.0);
    }
    let tl = total_len.max(1e-3);
    for (i, v) in dir.iter().enumerate() {
        f[GRID * GRID + i] = v / tl;
    }
    // the global numbers
    let g = GRID * GRID + ZONES * ZONES * DIRS;
    let n = strokes.len();
    let turning: f32 = strokes.iter().map(|s| net_turn(s).abs()).sum::<f32>() / std::f32::consts::TAU;
    let straight = strokes.iter().map(|s| {
        let l = path_len(s);
        if l > 1e-4 { s[0].distance(*s.last().unwrap()) / l } else { 1.0 }
    }).fold(0.0f32, f32::max);
    let (mut top, mut left) = (0.0f32, 0.0f32);
    for p in &all {
        let q = norm(*p);
        if q.y < 0.0 {
            top += 1.0;
        }
        if q.x < 0.0 {
            left += 1.0;
        }
    }
    let m = all.len().max(1) as f32;
    let raw_len: f32 = strokes.iter().map(|s| path_len(s)).sum();
    let glob = [
        (w.max(0.05)).ln(),
        (h.max(0.05)).ln(),
        ((w.max(0.05)) / (h.max(0.05))).ln(),
        (n == 1) as u8 as f32,
        (n == 2) as u8 as f32,
        (n == 3) as u8 as f32,
        (n >= 4) as u8 as f32,
        (raw_len / scale).min(30.0) / 10.0,
        turning.min(6.0),
        straight,
        top / m,
        left / m,
        (raw_len.max(0.05)).ln(),
        (n as f32).min(8.0) / 8.0,
    ];
    f[g..g + N_GLOBAL].copy_from_slice(&glob);
    f
}

fn put(f: &mut [f32], p: Pos2, v: f32) {
    let x = (((p.x + 0.5) * GRID as f32) as isize).clamp(0, GRID as isize - 1) as usize;
    let y = (((p.y + 0.5) * GRID as f32) as isize).clamp(0, GRID as isize - 1) as usize;
    f[y * GRID + x] += v * 0.35;
}

fn net_turn(p: &[Pos2]) -> f32 {
    let l = path_len(p);
    if l < 1e-4 {
        return 0.0;
    }
    let rs = resample(p, ((l / 0.1) as usize).clamp(8, 200));
    let mut t = 0.0;
    let mut prev: Option<egui::Vec2> = None;
    for w in rs.windows(2) {
        let v = w[1] - w[0];
        if v.length() < 1e-5 {
            continue;
        }
        if let Some(u) = prev {
            t += u.x.mul_add(v.y, -(u.y * v.x)).atan2(u.dot(v));
        }
        prev = Some(v);
    }
    t
}

// ---------------- synthetic ink, training and evaluation (native) ----------------

#[cfg(not(target_arch = "wasm32"))]
pub mod train {
    use super::*;

    /// A small deterministic random source (xorshift).
    pub struct Rng(pub u64);
    impl Rng {
        pub fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        pub fn f(&mut self) -> f32 {
            (self.next() >> 40) as f32 / (1u64 << 24) as f32
        }
        pub fn r(&mut self, a: f32, b: f32) -> f32 {
            a + (b - a) * self.f()
        }
        pub fn chance(&mut self, p: f32) -> bool {
            self.f() < p
        }
        pub fn pick(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    type S = Vec<(f32, f32)>;

    fn line(a: (f32, f32), b: (f32, f32)) -> S {
        vec![a, b]
    }

    /// An ellipse arc: centre, radii, from t0 to t1 (turns), rotated by `tilt`.
    fn arc(c: (f32, f32), ra: f32, rb: f32, t0: f32, t1: f32, tilt: f32, steps: usize) -> S {
        (0..=steps)
            .map(|i| {
                let t = (t0 + (t1 - t0) * i as f32 / steps as f32) * std::f32::consts::TAU;
                let (x, y) = (ra * t.cos(), rb * t.sin());
                (c.0 + x * tilt.cos() - y * tilt.sin(), c.1 + x * tilt.sin() + y * tilt.cos())
            })
            .collect()
    }

    /// A filled head, centred on `c`, about `size` spaces wide.
    fn head_filled(g: &mut Rng, c: (f32, f32), size: f32, tilt: f32) -> Vec<S> {
        let (a, b) = (size / 2.0, size * g.r(0.3, 0.42));
        match g.pick(10) {
            0..=3 => {
                // a spiral, getting smaller
                let turns = g.r(1.6, 3.6);
                let n = (turns * 18.0) as usize;
                let mut s = vec![];
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let k = 1.0 - g.r(0.45, 0.75) * t;
                    let ang = t * turns * std::f32::consts::TAU;
                    let (x, y) = (a * k * ang.cos(), b * k * ang.sin());
                    s.push((c.0 + x * tilt.cos() - y * tilt.sin(), c.1 + x * tilt.sin() + y * tilt.cos()));
                }
                vec![s]
            }
            4 | 5 => {
                // a blot: hatched back and forth
                let k = 3 + g.pick(4);
                let mut s = vec![];
                for i in 0..=k {
                    let y = -b + 2.0 * b * i as f32 / k as f32;
                    let half = a * (1.0 - (y / b).powi(2)).max(0.1).sqrt();
                    let (x0, x1) = if i % 2 == 0 { (-half, half) } else { (half, -half) };
                    for (x, yy) in [(x0, y), (x1, y)] {
                        s.push((c.0 + x * tilt.cos() - yy * tilt.sin(), c.1 + x * tilt.sin() + yy * tilt.cos()));
                    }
                }
                vec![s]
            }
            6 | 7 => {
                // a loop gone over twice
                vec![arc(c, a, b, g.r(0.0, 1.0), g.r(2.0, 2.4), tilt, 40)]
            }
            8 => {
                // a quick dash
                let l = size * g.r(0.6, 0.9);
                vec![line((c.0 - l / 2.0 * tilt.cos(), c.1 - l / 2.0 * tilt.sin() + 0.1), (c.0 + l / 2.0 * tilt.cos(), c.1 + l / 2.0 * tilt.sin() - 0.1))]
            }
            _ => {
                // a small circle scribbled in
                let mut s = arc(c, a * 0.9, b * 0.9, 0.0, 1.0, tilt, 16);
                s.extend(arc(c, a * 0.5, b * 0.5, 0.0, 1.5, tilt, 16));
                vec![s]
            }
        }
    }

    /// An open head.
    fn head_open(g: &mut Rng, c: (f32, f32), size: f32, tilt: f32) -> Vec<S> {
        let (a, b) = (size / 2.0, size * g.r(0.3, 0.45));
        match g.pick(10) {
            0..=5 => {
                let start = g.r(0.0, 1.0);
                vec![arc(c, a, b, start, start + g.r(0.82, 1.0), tilt, 24)]
            }
            6 | 7 => {
                let start = g.r(0.0, 1.0);
                vec![arc(c, a, b, start, start + g.r(1.05, 1.3), tilt, 28)]
            }
            _ => {
                // two arcs
                vec![arc(c, a, b, 0.2, 0.8, tilt, 10), arc(c, a, b, 0.7, 1.3, tilt, 10)]
            }
        }
    }

    /// One note (value 1..7), maybe a chord, maybe with the stem down; strokes in spaces around (0, 0).
    pub fn note(g: &mut Rng, value: u8) -> Vec<S> {
        let size = g.r(0.9, 1.6);
        let tilt = g.r(-0.65, 0.15);
        let heads = if g.chance(0.12) { 2 + g.pick(2) } else { 1 };
        let up = g.chance(0.7);
        let mut out: Vec<S> = vec![];
        let mut head_strokes: Vec<S> = vec![];
        for k in 0..heads {
            let c = (g.r(-0.15, 0.15) + if k % 2 == 1 && g.chance(0.3) { size * 0.9 } else { 0.0 }, -(k as f32) * g.r(0.9, 1.1));
            let hs = if value >= 3 { head_filled(g, c, size, tilt) } else { head_open(g, c, size, tilt) };
            head_strokes.extend(hs);
        }
        if value == 1 {
            return head_strokes;
        }
        // the stem: right side up, or left side down (or the other way in a quick hand)
        let right = if up { !g.chance(0.15) } else { g.chance(0.15) };
        let sx = if right { size * g.r(0.42, 0.55) } else { -size * g.r(0.42, 0.55) };
        let gap = if g.chance(0.6) { g.r(0.0, 0.25) } else { g.r(0.25, 1.0) };
        let len = g.r(2.6, 4.2) + (heads as f32 - 1.0);
        let lean = g.r(-0.22, 0.22);
        let (y0, y1) = if up { (-gap, -gap - len) } else { (gap - (heads as f32 - 1.0), gap - (heads as f32 - 1.0) + len) };
        let top_x = sx + len * lean.sin();
        let stem: S = vec![(sx, y0), (sx + (top_x - sx) * 0.5 + g.r(-0.06, 0.06), (y0 + y1) / 2.0), (top_x, y1)];
        let far = (top_x, y1);
        let mut flags: Vec<S> = vec![];
        let k = value.saturating_sub(3) as usize;
        let fdir = if up { 1.0 } else { -1.0 };
        let side = if g.chance(0.85) { 1.0 } else { -1.0 };
        let style = g.pick(3);
        let gapf = g.r(0.55, 1.0);
        for i in 0..k {
            let o = (far.0, far.1 + fdir * i as f32 * gapf);
            let w = g.r(0.5, 1.0) * side;
            let hgt = g.r(0.8, 1.5) * fdir;
            let f: S = match style {
                0 => (0..=6).map(|j| {
                    let t = j as f32 / 6.0;
                    (o.0 + w * (t * 1.7).sin(), o.1 + hgt * t)
                }).collect(),
                1 => vec![o, (o.0 + w, o.1 + hgt * 0.7)],
                _ => vec![o, (o.0 + w * 0.8, o.1 + hgt * 0.4), (o.0 + w * 0.6, o.1 + hgt)],
            };
            flags.push(f);
        }
        // how the hand puts the strokes together
        if g.chance(0.3) && heads == 1 {
            // head and stem in one stroke
            let mut s = head_strokes.pop().unwrap();
            s.push(stem[0]);
            s.extend(stem.iter().skip(1));
            if k > 0 && g.chance(0.5) {
                for f in &flags {
                    s.extend(f.iter().copied());
                }
                flags.clear();
            }
            head_strokes.push(s);
            out.extend(head_strokes);
        } else {
            out.extend(head_strokes);
            let mut st = stem;
            if k > 0 && g.chance(0.35) {
                for f in &flags {
                    st.extend(f.iter().copied());
                }
                flags.clear();
            }
            out.push(st);
        }
        out.extend(flags);
        if g.chance(0.3) {
            out.reverse();
        }
        out
    }

    pub fn rest(g: &mut Rng, class: usize) -> Vec<S> {
        match class {
            7 => {
                let (w, h) = (g.r(0.8, 1.5), g.r(0.25, 0.6));
                if g.chance(0.25) {
                    vec![vec![(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0), (-w / 2.0, -h / 2.0 + 0.05)]]
                } else {
                    let k = 3 + g.pick(4);
                    let vertical = g.chance(0.3);
                    let mut s = vec![];
                    for i in 0..=k {
                        let t = -0.5 + i as f32 / k as f32;
                        let (a, b) = if i % 2 == 0 { (-0.5, 0.5) } else { (0.5, -0.5) };
                        if vertical {
                            s.push((t * w, a * h));
                            s.push((t * w, b * h));
                        } else {
                            s.push((a * w, t * h));
                            s.push((b * w, t * h));
                        }
                    }
                    vec![s]
                }
            }
            8 => {
                let j = |g: &mut Rng| g.r(-0.12, 0.12);
                vec![vec![(-0.2 + j(g), -1.5), (0.4 + j(g), -0.85), (-0.3 + j(g), -0.2), (0.4 + j(g), 0.5), (j(g), 0.8), (-0.3 + j(g), 1.0), (0.15 + j(g), 1.5)]]
            }
            _ => {
                let hooks = class - 8;
                let len = g.r(2.0, 3.2) + 0.3 * hooks as f32;
                let lean = g.r(0.25, 0.6);
                let bottom = (-len / 2.0 * lean.sin(), len / 2.0);
                let top = (len / 2.0 * lean.sin(), -len / 2.0);
                let mut out = vec![];
                let one = g.chance(0.5) || hooks == 1;
                let mut first: S = vec![];
                for k in 0..hooks {
                    let t = k as f32 * g.r(0.22, 0.3);
                    let on = (top.0 + (bottom.0 - top.0) * t, top.1 + (bottom.1 - top.1) * t);
                    let blob = (on.0 - g.r(0.55, 0.85), on.1 + g.r(-0.05, 0.25));
                    let mut hk = arc(blob, g.r(0.12, 0.25), g.r(0.1, 0.2), 0.0, g.r(0.8, 1.6), 0.0, 8);
                    hk.push((on.0 - 0.2, on.1 + 0.1));
                    hk.push(on);
                    if k == 0 || !one {
                        if k == 0 {
                            first = hk;
                        } else {
                            out.push(hk);
                        }
                    } else {
                        out.push(hk);
                    }
                }
                first.push(top);
                first.push(bottom);
                out.insert(0, first);
                out
            }
        }
    }

    pub fn accidental(g: &mut Rng, class: usize) -> Vec<S> {
        let h = g.r(2.0, 3.0);
        match class {
            13 => {
                let gap = g.r(0.45, 0.8);
                let w = g.r(1.0, 1.6);
                let sl = g.r(0.1, 0.35);
                let mut v = vec![
                    line((-gap / 2.0, -h / 2.0), (-gap / 2.0 - 0.05, h / 2.0)),
                    line((gap / 2.0, -h / 2.0 - 0.1), (gap / 2.0 - 0.05, h / 2.0 - 0.1)),
                    line((-w / 2.0, -0.35 + sl), (w / 2.0, -0.35 - sl)),
                    line((-w / 2.0, 0.35 + sl), (w / 2.0, 0.35 - sl)),
                ];
                if g.chance(0.3) {
                    // the bars as one zigzag
                    let b = vec![v[2][0], v[2][1], v[3][0], v[3][1]];
                    v.truncate(2);
                    v.push(b);
                }
                v
            }
            14 => {
                let bw = g.r(0.5, 0.9);
                let mut s = vec![(-0.3, -h * 0.6), (-0.3, h * 0.4)];
                s.extend([(-0.3 + bw * 0.6, h * 0.3), (-0.3 + bw, h * 0.12), (-0.3 + bw * 0.7, -0.05), (-0.3 + bw * 0.2, 0.0), (-0.3, h * 0.12)]);
                if g.chance(0.3) {
                    vec![s[..2].to_vec(), s[1..].to_vec()]
                } else {
                    vec![s]
                }
            }
            15 => {
                let w = g.r(0.55, 0.85);
                let a = vec![(-w / 2.0, -h / 2.0), (-w / 2.0, h * 0.25), (w / 2.0, h * 0.15)];
                let b = vec![(-w / 2.0, -h * 0.15), (w / 2.0, -h * 0.25), (w / 2.0, h / 2.0)];
                if g.chance(0.3) {
                    vec![a.iter().chain(b.iter()).copied().collect()]
                } else {
                    vec![a, b]
                }
            }
            16 => {
                let s = g.r(0.6, 1.1);
                if g.chance(0.25) {
                    vec![vec![(-s / 2.0, -s / 2.0), (s / 2.0, s / 2.0), (s / 2.0, -s / 2.0), (-s / 2.0, s / 2.0)]]
                } else {
                    vec![line((-s / 2.0, -s / 2.0), (s / 2.0, s / 2.0)), line((s / 2.0, -s / 2.0), (-s / 2.0, s / 2.0))]
                }
            }
            _ => {
                let mut v = vec![];
                for dx in [-0.45f32, 0.45] {
                    let bw = g.r(0.4, 0.7);
                    v.push(vec![(dx - 0.25, -h * 0.6), (dx - 0.25, h * 0.4), (dx - 0.25 + bw * 0.7, h * 0.25), (dx - 0.25 + bw, h * 0.1), (dx - 0.25 + bw * 0.5, -0.05), (dx - 0.25, h * 0.12)]);
                }
                v
            }
        }
    }

    pub fn dot(g: &mut Rng) -> Vec<S> {
        let s = g.r(0.04, 0.3);
        if g.chance(0.5) {
            vec![vec![(0.0, 0.0), (s, s * 0.3)]]
        } else {
            vec![arc((0.0, 0.0), s / 2.0, s / 2.0, 0.0, 1.0, 0.0, 8)]
        }
    }

    pub fn other(g: &mut Rng) -> Vec<S> {
        match g.pick(7) {
            0 => {
                let l = g.r(3.0, 9.0);
                let a = g.r(-0.4, 0.4);
                vec![line((-l / 2.0 * a.cos(), -l / 2.0 * a.sin()), (l / 2.0 * a.cos(), l / 2.0 * a.sin()))]
            }
            1 => vec![arc((0.0, 0.0), g.r(2.5, 5.0), g.r(2.0, 4.0), 0.0, 1.0, 0.0, 30)],
            2 => {
                // a scribble
                let (l, t) = (g.r(1.5, 6.0), 4 + g.pick(6));
                let a = g.r(0.0, std::f32::consts::PI);
                (0..1).map(|_| (0..=t).map(|i| {
                    let x = if i % 2 == 0 { -l / 2.0 } else { l / 2.0 };
                    let y = (i as f32 / t as f32 - 0.5) * g.r(0.5, 1.6);
                    (x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos())
                }).collect()).collect()
            }
            3 => {
                // an S or a C
                vec![arc((0.0, -0.6), 0.7, 0.6, 0.25, 1.0, 0.0, 12).into_iter().chain(arc((0.0, 0.6), 0.7, 0.6, 0.75, 0.0, 0.0, 12)).collect()]
            }
            4 => {
                // a stem or a line alone
                let l = g.r(2.0, 4.0);
                let a = g.r(-0.25, 0.25);
                vec![line((0.0, 0.0), (l * a.sin(), -l * a.cos()))]
            }
            5 => {
                // an "m" curl
                vec![(0..=24).map(|i| {
                    let t = i as f32 / 24.0;
                    (t * 2.0 - 1.0, -((t * 9.0).sin()).abs() * 0.6)
                }).collect()]
            }
            _ => {
                // a big cross
                let s = g.r(2.0, 5.0);
                vec![line((-s / 2.0, -s / 2.0), (s / 2.0, s / 2.0)), line((s / 2.0, -s / 2.0), (-s / 2.0, s / 2.0))]
            }
        }
    }

    /// One random sample of a class, as a hand draws it: slant, shear, scale, jitter, pen sampling.
    pub fn sample(g: &mut Rng, class: usize) -> Vec<Vec<Pos2>> {
        let raw = match class {
            0..=6 => note(g, class as u8 + 1),
            7..=12 => rest(g, class),
            13..=17 => accidental(g, class),
            DOT => dot(g),
            _ => other(g),
        };
        let rot = g.r(-0.18, 0.18);
        let shear = g.r(-0.25, 0.25);
        let (sx, sy) = (g.r(0.85, 1.2), g.r(0.85, 1.2));
        let jitter = g.r(0.01, 0.07);
        let spacing = g.r(0.05, 0.22);
        raw.into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| {
                let pts: Vec<Pos2> = s.iter().map(|&(x, y)| {
                    let (x, y) = (x * sx + y * shear, y * sy);
                    pos2(x * rot.cos() - y * rot.sin(), x * rot.sin() + y * rot.cos())
                }).collect();
                // pen sampling along the path, with a little shake
                let l = path_len(&pts);
                let n = ((l / spacing) as usize).clamp(2, 400);
                let mut rs = if pts.len() >= 2 { resample(&pts, n) } else { pts.clone() };
                for p in rs.iter_mut() {
                    *p += vec2(g.r(-jitter, jitter), g.r(-jitter, jitter));
                }
                rs
            })
            .collect()
    }

    /// A data set: `per` samples of every class from random seeds starting at `seed`.
    pub fn dataset(seed: u64, per: usize) -> Vec<(Vec<f32>, usize)> {
        let mut out = vec![];
        let mut g = Rng(seed.max(1));
        for i in 0..per {
            for c in 0..N_OUT {
                let mut gi = Rng(g.next() ^ ((i as u64) << 20) ^ c as u64 | 1);
                out.push((features(&sample(&mut gi, c)), c));
            }
        }
        out
    }

    /// Trains a model: SGD with momentum and a softmax cross-entropy.
    pub fn fit(train: &[(Vec<f32>, usize)], epochs: usize, seed: u64) -> Model {
        let n = train.len().max(1) as f32;
        let mut mean = vec![0.0f32; N_FEAT];
        let mut std = vec![0.0f32; N_FEAT];
        for (x, _) in train {
            for (m, v) in mean.iter_mut().zip(x) {
                *m += v / n;
            }
        }
        for (x, _) in train {
            for ((s, v), m) in std.iter_mut().zip(x).zip(&mean) {
                *s += (v - m).powi(2) / n;
            }
        }
        for s in std.iter_mut() {
            *s = s.sqrt().max(0.02);
        }
        let mut g = Rng(seed | 1);
        let r1 = (2.0 / N_FEAT as f32).sqrt();
        let r2 = (2.0 / N_HIDDEN as f32).sqrt();
        let mut m = Model {
            mean,
            std,
            w1: (0..N_HIDDEN * N_FEAT).map(|_| g.r(-r1, r1)).collect(),
            b1: vec![0.0; N_HIDDEN],
            w2: (0..N_OUT * N_HIDDEN).map(|_| g.r(-r2, r2)).collect(),
            b2: vec![0.0; N_OUT],
        };
        let xs: Vec<Vec<f32>> = train.iter().map(|(x, _)| m.standardize(x)).collect();
        let mut order: Vec<usize> = (0..xs.len()).collect();
        let (mut vw1, mut vb1, mut vw2, mut vb2) = (vec![0.0f32; m.w1.len()], vec![0.0f32; N_HIDDEN], vec![0.0f32; m.w2.len()], vec![0.0f32; N_OUT]);
        let batch = 32;
        for ep in 0..epochs {
            let lr = 0.05 * (1.0 - ep as f32 / epochs as f32).max(0.05);
            for i in (1..order.len()).rev() {
                let j = g.pick(i + 1);
                order.swap(i, j);
            }
            for chunk in order.chunks(batch) {
                let (mut gw1, mut gb1, mut gw2, mut gb2) = (vec![0.0f32; m.w1.len()], vec![0.0f32; N_HIDDEN], vec![0.0f32; m.w2.len()], vec![0.0f32; N_OUT]);
                for &i in chunk {
                    let x = &xs[i];
                    let y = train[i].1;
                    let (h, p) = m.forward(x);
                    let mut d2 = p.clone();
                    d2[y] -= 1.0;
                    let mut dh = vec![0.0f32; N_HIDDEN];
                    for k in 0..N_OUT {
                        gb2[k] += d2[k];
                        for j in 0..N_HIDDEN {
                            gw2[k * N_HIDDEN + j] += d2[k] * h[j];
                            dh[j] += d2[k] * m.w2[k * N_HIDDEN + j];
                        }
                    }
                    for j in 0..N_HIDDEN {
                        if h[j] <= 0.0 {
                            continue;
                        }
                        gb1[j] += dh[j];
                        let row = &mut gw1[j * N_FEAT..(j + 1) * N_FEAT];
                        for (gv, xv) in row.iter_mut().zip(x) {
                            *gv += dh[j] * xv;
                        }
                    }
                }
                let s = 1.0 / chunk.len() as f32;
                let step = |w: &mut [f32], v: &mut [f32], gr: &[f32], decay: f32| {
                    for ((wi, vi), gi) in w.iter_mut().zip(v.iter_mut()).zip(gr) {
                        *vi = 0.9 * *vi - lr * (gi * s + decay * *wi);
                        *wi += *vi;
                    }
                };
                step(&mut m.w1, &mut vw1, &gw1, 1e-4);
                step(&mut m.b1, &mut vb1, &gb1, 0.0);
                step(&mut m.w2, &mut vw2, &gw2, 1e-4);
                step(&mut m.b2, &mut vb2, &gb2, 0.0);
            }
        }
        m
    }

    /// A test set: its name and its samples (strokes in spaces, class).
    pub type TestSet = (&'static str, Vec<(Vec<Vec<Pos2>>, usize)>);

    /// The test sets the training never sees.
    pub fn test_sets() -> Vec<TestSet> {
        // 1. the generator, other seeds (1 000 000 and up)
        let mut a = vec![];
        let mut g = Rng(1_000_003);
        for i in 0..120usize {
            for c in 0..N_OUT {
                let mut gi = Rng(g.next() ^ ((i as u64) << 20) ^ c as u64 | 1);
                a.push((sample(&mut gi, c), c));
            }
        }
        // 2. another family: the hand-made test shapes of pen::samples (F-38/F-39), never used in training
        use crate::pen::samples as sm;
        let sp = 10.0;
        let at = pos2(0.0, 0.0);
        let to_sp = |v: Vec<Vec<Pos2>>| -> Vec<Vec<Pos2>> { v.into_iter().map(|s| s.into_iter().map(|p| pos2(p.x / sp, p.y / sp)).collect()).collect() };
        let mut b = vec![];
        for hand in sm::hands() {
            for value in 1..=7u8 {
                for left in [false, true] {
                    b.push((to_sp(sm::note_t(at, sp, value, 0, left, hand)), value as usize - 1));
                }
            }
            b.push((to_sp(sm::quarter_two_strokes_t(at, sp, hand)), 2));
            b.push((to_sp(vec![sm::quarter_rest_t(at, sp, hand)]), 8));
            for hooks in 1..=4usize {
                b.push((to_sp(sm::hooked_rest_t(at, sp, hooks, hand)), 8 + hooks));
            }
            b.push((to_sp(sm::sharp(at, sp, hand.seed)), 13));
            b.push((to_sp(sm::flat(at, sp, hand.seed)), 14));
            b.push((to_sp(sm::natural(at, sp, hand.seed)), 15));
            b.push((to_sp(sm::double_sharp(at, sp, hand.seed)), 16));
            b.push((to_sp(sm::double_flat(at, sp, hand.seed)), 17));
            b.push((to_sp(vec![sm::block_rest(at, sp, hand.seed)]), 7));
            b.push((to_sp(vec![sm::dot(at, sp, hand.seed)]), DOT));
            b.push((to_sp(vec![sm::long_line(at, sp, hand.seed)]), OTHER));
            b.push((to_sp(vec![sm::big_loop(at, sp, hand.seed)]), OTHER));
            b.push((to_sp(vec![sm::scribble_dir(at, sp, 0.3, 4.0, 6, hand.seed)]), OTHER));
        }
        for seed in [3u32, 9, 21] {
            b.push((to_sp(vec![sm::head(at, sp, true, seed), sm::stem(at, sp, true, seed)]), 2));
            b.push((to_sp(vec![sm::head(at, sp, false, seed), sm::stem(at, sp, true, seed)]), 1));
            b.push((to_sp(vec![sm::head(at, sp, false, seed)]), 0));
            b.push((to_sp(vec![sm::head(at, sp, true, seed), sm::stem(at, sp, true, seed), sm::flag(at, sp, seed)]), 3));
            b.push((to_sp(vec![sm::quarter_one_stroke(at, sp, seed)]), 2));
        }
        vec![("generator, held-out seeds", a), ("another family (pen::samples)", b)]
    }

    /// Accuracy per class of the model, the rules and both together on the test sets, as a table.
    pub fn eval_report() -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let ctx = crate::pen::Ctx::default();
        let _ = writeln!(out, "Fermata pen model: {} features, {} hidden, {} classes; model loaded: {}", N_FEAT, N_HIDDEN, N_OUT, model_loaded());
        for (name, set) in test_sets() {
            let mut t = vec![[0usize; 4]; N_OUT]; // model, rules, both, total
            for (strokes, y) in &set {
                let pm = classify(strokes).unwrap_or_else(|| vec![0.0; N_OUT]);
                let cands = crate::pen::recognize(strokes, 1.0, &ctx);
                let rule = cands.first().and_then(|c| class_of(&c.reading)).unwrap_or(OTHER);
                let both = ensemble(strokes, &ctx);
                let arg = |p: &[f32]| (0..N_OUT).max_by(|&a, &b| p[a].partial_cmp(&p[b]).unwrap()).unwrap();
                t[*y][0] += (arg(&pm) == *y) as usize;
                t[*y][1] += (rule == *y) as usize;
                t[*y][2] += (arg(&both) == *y) as usize;
                t[*y][3] += 1;
            }
            let _ = writeln!(out, "\n== {name} ({} samples)", set.len());
            let _ = writeln!(out, "{:<8} {:>6} {:>8} {:>8} {:>8}", "class", "n", "model", "rules", "both");
            let mut sum = [0usize; 4];
            for (k, row) in t.iter().enumerate() {
                if row[3] == 0 {
                    continue;
                }
                let pc = |v: usize| format!("{:.0}%", 100.0 * v as f32 / row[3] as f32);
                let _ = writeln!(out, "{:<8} {:>6} {:>8} {:>8} {:>8}", CLASSES[k], row[3], pc(row[0]), pc(row[1]), pc(row[2]));
                for i in 0..4 {
                    sum[i] += row[i];
                }
            }
            let pc = |v: usize| format!("{:.1}%", 100.0 * v as f32 / sum[3].max(1) as f32);
            let _ = writeln!(out, "{:<8} {:>6} {:>8} {:>8} {:>8}", "all", sum[3], pc(sum[0]), pc(sum[1]), pc(sum[2]));
        }
        out
    }

    /// The training run behind `assets/pen_model.bin` (seeds 1… for training; the tests use seeds from 1 000 000).
    pub fn train_default() -> Model {
        let data = dataset(1, 1500);
        fit(&data, 30, 7)
    }
}
