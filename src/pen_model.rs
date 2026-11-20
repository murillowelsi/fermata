//! F-40/F-41: a small trained classifier for one handwritten symbol (a note, rest, accidental, dot, clef, time
//! signature, dynamic, articulation, hairpin or arc).
//!
//! F-41: two sets of weights, the same network:
//! - `assets/pen_model.bin` (default): trained on the HOMUS pen-stroke dataset (writers 1–80, the classes Fermata
//!   uses) plus synthetic ink. HOMUS declares no licence; it is used for training with the permission of
//!   Fermata's maintainer, downloaded by `tools/train-pen/fetch-homus.sh` and never redistributed (only these
//!   weights ship). See CREDITS.md.
//! - `assets/pen_model_synthetic.bin` (plan B): synthetic ink only. Build with `--features pen-synthetic-only`
//!   to ship it instead.
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
pub const CLASSES: [&str; 46] = [
    "n1.0", "n2.0", "n3.0", "n4.0", "n5.0", "n6.0", "n7.0", "rblock", "r3.0", "r4.0", "r5.0", "r6.0", "r7.0", "a1", "a-1", "a0", "a2", "a-2", "dot", "other",
    // F-41
    "clefG", "clefF", "clefC", "t4/4", "t3/4", "t2/4", "t2/2", "t3/8", "t6/8", "t9/8", "t12/8", "tC", "tCut", "barline", "dp", "dpp", "dmp", "dmf", "df", "dff", "accent", "tenuto", "fermata", "cresc", "dim", "arc",
];
pub const TIME_FIRST: usize = 23;
pub const TIME_LAST: usize = 32;
pub const BARLINE: usize = 33;
pub const DYN_FIRST: usize = 34;
pub const DYN_LAST: usize = 39;
pub const ACCENT: usize = 40;
pub const TENUTO: usize = 41;
pub const FERMATA: usize = 42;
pub const CRESC: usize = 43;
pub const DIM: usize = 44;
pub const ARC: usize = 45;

/// The time signature a time class stands for (num, den); C is 4/4 and ₵ 2/2.
pub fn time_of(class: usize) -> Option<(u8, u8)> {
    Some(match class {
        23 => (4, 4),
        24 => (3, 4),
        25 => (2, 4),
        26 => (2, 2),
        27 => (3, 8),
        28 => (6, 8),
        29 => (9, 8),
        30 => (12, 8),
        31 => (4, 4),
        32 => (2, 2),
        _ => return None,
    })
}

/// The dynamic a dynamic class stands for.
pub fn dynamic_of(class: usize) -> Option<&'static str> {
    Some(["p", "pp", "mp", "mf", "f", "ff"][class.checked_sub(DYN_FIRST).filter(|k| *k < 6)?])
}
pub const OTHER: usize = 19;
pub const DOT: usize = 18;

const GRID: usize = 12;
const ZONES: usize = 4;
const DIRS: usize = 4;
const N_GLOBAL: usize = 20 + 2 * SCAN + 4 + 13;
pub const N_FEAT: usize = GRID * GRID + ZONES * ZONES * DIRS + N_GLOBAL;
/// The hidden layer of each network of the built-in ensemble.
pub const N_HIDDEN: usize = 160;
pub const N_OUT: usize = CLASSES.len();

/// One network's numbers, in this order: feature means, feature spreads, W1 (hidden × features), b1, W2 (out ×
/// hidden), b2.
pub struct Model {
    pub hidden: usize,
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: Vec<f32>,
}

/// F-42: the built-in recognizer: a few networks trained from different seeds, whose probabilities are averaged,
/// over the symbol as written and a few slightly turned and stretched views of it.
pub struct Ensemble {
    pub models: Vec<Model>,
}

// (plan B: `--features pen-synthetic-only` ships the weights trained on synthetic ink only)
#[cfg(not(feature = "pen-synthetic-only"))]
const MODEL_BYTES: &[u8] = include_bytes!("../assets/pen_model.bin");
#[cfg(feature = "pen-synthetic-only")]
const MODEL_BYTES: &[u8] = include_bytes!("../assets/pen_model_synthetic.bin");

thread_local! {
    static MODEL: Option<Ensemble> = Ensemble::from_bytes(MODEL_BYTES);
}

/// The weight file: "FPM3", the number of networks (u32), then each network: its hidden size (u32); the feature
/// means and spreads and the biases as f32; W1 and W2 a row at a time, as a scale (f32) and 8-bit steps of it (a
/// quarter of the size, and no reading changes).
const MAGIC: &[u8; 4] = b"FPM3";

impl Model {
    /// The hidden layer and the class probabilities for (already standardized) features.
    fn forward(&self, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let nh = self.hidden;
        let mut h = vec![0.0f32; nh];
        for (j, hj) in h.iter_mut().enumerate() {
            let row = &self.w1[j * N_FEAT..(j + 1) * N_FEAT];
            let s: f32 = row.iter().zip(x).map(|(w, v)| w * v).sum::<f32>() + self.b1[j];
            *hj = s.max(0.0);
        }
        let mut o = vec![0.0f32; N_OUT];
        for (k, ok) in o.iter_mut().enumerate() {
            let row = &self.w2[k * nh..(k + 1) * nh];
            *ok = row.iter().zip(&h).map(|(w, v)| w * v).sum::<f32>() + self.b2[k];
        }
        (h, softmax(&o))
    }

    fn standardize(&self, f: &[f32]) -> Vec<f32> {
        f.iter().zip(self.mean.iter().zip(&self.std)).map(|(v, (m, s))| (v - m) / s.max(1e-4)).collect()
    }
}

/// The views of a symbol the ensemble looks at: as written, and turned and stretched a little (radians, x, y).
const VIEWS: [(f32, f32, f32); 5] = [(0.0, 1.0, 1.0), (0.08, 1.05, 0.95), (-0.08, 0.95, 1.05), (0.04, 1.1, 1.0), (-0.04, 1.0, 1.1)];

pub fn view(strokes: &[Vec<Pos2>], v: (f32, f32, f32)) -> Vec<Vec<Pos2>> {
    let (rot, sx, sy) = v;
    let (c, s) = (rot.cos(), rot.sin());
    strokes.iter().map(|st| st.iter().map(|p| {
        let (x, y) = (p.x * sx, p.y * sy);
        pos2(x * c - y * s, x * s + y * c)
    }).collect()).collect()
}

impl Ensemble {
    pub fn from_bytes(b: &[u8]) -> Option<Ensemble> {
        if b.len() < 8 || &b[..4] != MAGIC {
            return None;
        }
        struct Reader<'a>(&'a [u8], usize);
        impl Reader<'_> {
            fn take(&mut self, n: usize) -> Option<&[u8]> {
                let v = self.0.get(self.1..self.1 + n)?;
                self.1 += n;
                Some(v)
            }
            fn u32(&mut self) -> Option<usize> {
                Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
            }
            fn f32s(&mut self, n: usize) -> Option<Vec<f32>> {
                Some(self.take(4 * n)?.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
            }
            fn rows(&mut self, count: usize, len: usize) -> Option<Vec<f32>> {
                let mut out = Vec::with_capacity(count * len);
                for _ in 0..count {
                    let scale = f32::from_le_bytes(self.take(4)?.try_into().ok()?);
                    out.extend(self.take(len)?.iter().map(|&q| q as i8 as f32 * scale));
                }
                Some(out)
            }
        }
        let mut r = Reader(b, 4);
        let n = r.u32()?;
        let mut models = vec![];
        for _ in 0..n {
            let h = r.u32()?;
            let (mean, std, b1, b2) = (r.f32s(N_FEAT)?, r.f32s(N_FEAT)?, r.f32s(h)?, r.f32s(N_OUT)?);
            let (w1, w2) = (r.rows(h, N_FEAT)?, r.rows(N_OUT, h)?);
            models.push(Model { hidden: h, mean, std, w1, b1, w2, b2 });
        }
        (r.1 == b.len() && !models.is_empty()).then_some(Ensemble { models })
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((self.models.len() as u32).to_le_bytes());
        for m in &self.models {
            out.extend((m.hidden as u32).to_le_bytes());
            for v in [&m.mean, &m.std, &m.b1, &m.b2] {
                out.extend(v.iter().flat_map(|x| x.to_le_bytes()));
            }
            for (w, len) in [(&m.w1, N_FEAT), (&m.w2, m.hidden)] {
                for row in w.chunks(len) {
                    let scale = row.iter().fold(0.0f32, |a, v| a.max(v.abs())).max(1e-8) / 127.0;
                    out.extend(scale.to_le_bytes());
                    out.extend(row.iter().map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8 as u8));
                }
            }
        }
        out
    }

    /// The class probabilities, averaged over the networks and the views.
    pub fn predict(&self, strokes_sp: &[Vec<Pos2>]) -> Vec<f32> {
        let mut acc = [0.0f32; N_OUT];
        for v in VIEWS {
            let x = features(&view(strokes_sp, v));
            for m in &self.models {
                for (a, b) in acc.iter_mut().zip(m.forward(&m.standardize(&x)).1) {
                    *a += b;
                }
            }
        }
        let n = (VIEWS.len() * self.models.len()) as f32;
        acc.iter().map(|v| v / n).collect()
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

#[cfg_attr(not(test), allow(dead_code))]
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
        Reading::Symbol { class, .. } if *class < N_OUT => *class,
        Reading::Symbol { .. } => return None,
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
    ensemble_with(pr, personal_hit, classify(strokes_sp))
}

/// The rules' probabilities `pr` and the model's `pm` together (`personal_hit`: the writer's own figures decided).
pub fn ensemble_with(pr: Vec<f32>, personal_hit: bool, pm: Option<Vec<f32>>) -> Vec<f32> {
    match pm {
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
    let (mut xr, mut yr) = (0usize, 0usize);
    for st in &strokes {
        let rs = resample(st, 48);
        let hy = (0.12 * scale).max(0.08);
        xr += turns_along(&rs, true, hy);
        yr += turns_along(&rs, false, hy);
    }
    let xrev = (xr as f32 / n as f32).min(10.0) / 4.0;
    let yrev = (yr as f32 / n as f32).min(10.0) / 4.0;
    let short = strokes.iter().filter(|s| path_len(s) < 0.25 * scale).count().min(6) as f32 / 3.0;
    let li = (0..n).max_by(|&a, &b| path_len(strokes[a]).partial_cmp(&path_len(strokes[b])).unwrap()).unwrap_or(0);
    let longest = path_len(strokes[li]);
    let (a, b) = (strokes[li][0], *strokes[li].last().unwrap());
    let chord = a.distance(b).max(1e-3);
    let sag = strokes[li].iter().map(|p| {
        let ab = b - a;
        ((*p - a).x * ab.y - (*p - a).y * ab.x).abs() / chord
    }).fold(0.0f32, f32::max) / chord.max(0.3);
    let lr = bbox(strokes[li]);
    let (lw, lh) = (lr.width().max(0.05), lr.height().max(0.05));
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
        // F-41: how often the ink turns back across and along, how many short strokes, how curved the longest one
        xrev,
        yrev,
        short,
        longest / raw_len.max(1e-3),
        sag,
        (lw / lh).ln().clamp(-3.0, 3.0),
    ];
    f[g..g + glob.len()].copy_from_slice(&glob);
    // F-42: how many times the ink crosses lines across the symbol (stacked flags and hooks are counted, where the
    // picture blurs them together), and what kinds of strokes it has: compact loops (heads), straight lines (stems),
    // open curves (flags, hooks), and the open curves that start at the stem
    let mut at = g + glob.len();
    for vertical in [true, false] {
        for k in 0..SCAN {
            let t = (k as f32 + 0.5) / SCAN as f32;
            let v = if vertical { r.left() + t * w } else { r.top() + t * h };
            let mut n = 0usize;
            for s in &strokes {
                for p in s.windows(2) {
                    let (a, b) = if vertical { (p[0].x, p[1].x) } else { (p[0].y, p[1].y) };
                    if (a - v) * (b - v) < 0.0 {
                        n += 1;
                    }
                }
            }
            f[at] = (n as f32).min(8.0) / 4.0;
            at += 1;
        }
    }
    let kinds: Vec<u8> = strokes.iter().map(|s| stroke_kind(s, scale)).collect();
    let stem = (0..n).filter(|&i| kinds[i] == 1).max_by(|&a, &b| path_len(strokes[a]).partial_cmp(&path_len(strokes[b])).unwrap());
    let from_stem = stem.map(|si| {
        let (sa, sb) = (strokes[si][0], *strokes[si].last().unwrap());
        let d = |p: Pos2| {
            let ab = sb - sa;
            let t = ((p - sa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            p.distance(sa + ab * t)
        };
        (0..n).filter(|&i| kinds[i] == 2 && (d(strokes[i][0]) <= 0.12 * scale || d(*strokes[i].last().unwrap()) <= 0.12 * scale)).count()
    });
    for (k, v) in [
        kinds.iter().filter(|&&k| k == 0).count() as f32,
        kinds.iter().filter(|&&k| k == 1).count() as f32,
        kinds.iter().filter(|&&k| k == 2).count() as f32,
        from_stem.map(|c| c as f32).unwrap_or(-1.0),
    ]
    .into_iter()
    .enumerate()
    {
        f[at + k] = v.min(6.0) / 3.0;
    }
    at += 4;
    // (two counts: one leaves out the scribbled heads, so a note counts its flags; one keeps every blob, so a rest
    // counts its hooks, each a blob on a curve)
    let (count, side) = excursions(&strokes, 3.5 * std::f32::consts::PI);
    f[at + count.min(5)] = 1.0;
    f[at + 6] = side;
    let (all_blobs, _) = excursions(&strokes, f32::MAX);
    f[at + 7 + all_blobs.min(5)] = 1.0;
    f
}

/// F-42: a stroke as a straight line (1: a stem), a loop (0: a filled or open head) or an open curve (2: a flag, a
/// hook), for a symbol `scale` long.
fn stroke_kind(s: &[Pos2], scale: f32) -> u8 {
    let l = path_len(s);
    let sr = bbox(s);
    let ends = s[0].distance(*s.last().unwrap());
    if l > 1e-4 && ends / l >= 0.9 && l >= 0.35 * scale {
        1
    } else if net_turn(s).abs() >= 4.0 || (l > 1e-4 && ends <= 0.3 * l && sr.width().max(sr.height()) <= 0.6 * scale) {
        0
    } else {
        2
    }
}

/// F-42: the flags of a note, or the hooks of a rest: from the stem (the longest straight run of ink), how many times
/// the ink goes out to the side and comes back. A flag drawn on its own is one, a zigzag of three flags in one stroke
/// is three, a stem that runs on into its first flag counts it too; the loops (a filled head) don't count. Also on
/// which side they go (+1 right, −1 left).
pub fn excursions(strokes: &[&Vec<Pos2>], head_turn: f32) -> (usize, f32) {
    let (straight, hi_k, lo_k) = (0.95, 0.12, 0.05);
    // the stem: the longest nearly straight run of any stroke (stroke, from, to in its 32 resampled points)
    let mut best: Option<(f32, usize, usize, usize)> = None;
    let rss: Vec<Vec<Pos2>> = strokes.iter().map(|s| resample(s, 32)).collect();
    for (si, rs) in rss.iter().enumerate() {
        let mut acc = vec![0.0f32; rs.len()];
        for i in 1..rs.len() {
            acc[i] = acc[i - 1] + rs[i].distance(rs[i - 1]);
        }
        for i in 0..rs.len() {
            for j in (i + 3)..rs.len() {
                let (l, c) = (acc[j] - acc[i], rs[i].distance(rs[j]));
                if l > 1e-4 && c / l >= straight && best.is_none_or(|b| c > b.0) {
                    best = Some((c, si, i, j));
                }
            }
        }
    }
    let Some((len, si, i, j)) = best else { return (0, 0.0) };
    if len < 1e-3 {
        return (0, 0.0);
    }
    let (a, b) = (rss[si][i], rss[si][j]);
    let u = (b - a) / len;
    // the pieces that may hold flags: the stem's stroke before and after the stem (a scribbled head drawn in the same
    // stroke is left out), and the other strokes but the loops
    let turning = |p: &[Pos2]| if p.len() >= 3 { net_turn(p).abs() } else { 0.0 };
    let mut pieces: Vec<Vec<Pos2>> = vec![];
    for (n, s) in strokes.iter().enumerate() {
        if n == si {
            for part in [&rss[si][..=i], &rss[si][j..]] {
                if part.len() >= 2 && turning(part) < head_turn {
                    pieces.push(resample(part, 24));
                }
            }
        } else if turning(s) < head_turn {
            pieces.push(resample(s, 48));
        }
    }
    let (hi, lo) = (hi_k * len, lo_k * len);
    let (mut count, mut side) = (0usize, 0.0f32);
    for piece in &pieces {
        let mut out = false;
        for p in piece {
            let q = *p - a;
            let along = q.dot(u);
            if along < -0.15 * len || along > 1.15 * len {
                continue;
            }
            let d = q.x * u.y - q.y * u.x;
            if !out && d.abs() > hi {
                out = true;
                count += 1;
                side += d.signum();
            } else if out && d.abs() < lo {
                out = false;
            }
        }
    }
    // (with the stem drawn downward the sides swap: say them as seen on the page)
    let flip = if u.y < 0.0 { 1.0 } else { -1.0 };
    (count, if count > 0 { flip * side / count as f32 } else { 0.0 })
}

/// F-42: the scan lines across and along a symbol.
const SCAN: usize = 9;

/// How many times a path turns back along x (or y), ignoring wobbles smaller than `h`.
fn turns_along(p: &[Pos2], along_x: bool, h: f32) -> usize {
    let v = |q: &Pos2| if along_x { q.x } else { q.y };
    let (mut n, mut dir, mut anchor) = (0, 0i8, p.first().map(v).unwrap_or(0.0));
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
                // a slanting line
                let l = g.r(2.0, 4.0);
                let a = g.r(0.5, 1.1) * if g.chance(0.5) { 1.0 } else { -1.0 };
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
    fn poly(g: &mut Rng, pts: &[(f32, f32)], j: f32) -> S {
        pts.iter().map(|&(x, y)| (x + g.r(-j, j), y + g.r(-j, j))).collect()
    }

    fn shift(s: S, dx: f32, dy: f32) -> S {
        s.into_iter().map(|(x, y)| (x + dx, y + dy)).collect()
    }

    /// A digit about 1.8 spaces tall, centred on (cx, cy).
    fn digit(g: &mut Rng, d: u8, cx: f32, cy: f32) -> Vec<S> {
        let k = g.r(0.85, 1.1);
        let shapes: Vec<Vec<(f32, f32)>> = match d {
            1 => vec![vec![(-0.3, -0.5), (0.1, -0.9), (0.0, 0.9)]],
            2 => vec![vec![(-0.4, -0.5), (-0.1, -0.9), (0.3, -0.85), (0.4, -0.4), (-0.45, 0.9), (0.45, 0.9)]],
            3 => vec![vec![(-0.4, -0.8), (0.3, -0.85), (0.1, -0.1), (0.4, 0.3), (0.2, 0.85), (-0.4, 0.8)]],
            4 => if g.chance(0.5) { vec![vec![(0.2, 0.9), (0.2, -0.9), (-0.45, 0.3), (0.45, 0.3)]] } else { vec![vec![(-0.1, -0.9), (-0.45, 0.3), (0.45, 0.3)], vec![(0.2, -0.4), (0.2, 0.9)]] },
            6 => vec![vec![(0.3, -0.9), (-0.3, -0.2), (-0.35, 0.5), (0.0, 0.9), (0.35, 0.5), (0.2, 0.1), (-0.3, 0.25)]],
            8 => vec![vec![(0.0, 0.0), (0.35, -0.45), (0.0, -0.9), (-0.35, -0.45), (0.0, 0.0), (0.4, 0.45), (0.0, 0.9), (-0.4, 0.45), (0.0, 0.0)]],
            _ => vec![vec![(0.35, -0.3), (0.0, 0.0), (-0.35, -0.4), (0.0, -0.9), (0.35, -0.5), (0.3, 0.9)]],
        };
        shapes.into_iter().map(|sh| {
            let sh: Vec<(f32, f32)> = sh.iter().map(|&(x, y)| (cx + x * k, cy + y * k)).collect();
            poly(g, &sh, 0.06)
        }).collect()
    }

    pub fn clef(g: &mut Rng, class: usize) -> Vec<S> {
        match class {
            20 => {
                let pts = [(0.2, 0.3), (0.6, 0.0), (0.5, -0.5), (0.0, -0.6), (-0.6, -0.1), (-0.4, 0.7), (0.4, 1.0), (1.0, 0.3), (0.7, -1.0), (0.1, -2.2), (-0.1, -3.3), (0.3, -3.8), (0.5, -3.0), (-0.2, -1.6), (0.0, 0.8), (0.2, 2.4), (-0.2, 2.8), (-0.6, 2.4)];
                vec![poly(g, &pts, 0.12)]
            }
            21 => {
                let mut v = vec![poly(g, &[(-0.5, -0.4), (-0.6, -0.7), (-0.3, -0.95), (0.3, -1.0), (0.9, -0.5), (0.8, 0.4), (0.2, 1.2), (-0.6, 1.8)], 0.08)];
                if g.chance(0.7) {
                    for y in [-0.6f32, 0.4] {
                        let c = (1.3 + g.r(-0.1, 0.1), y + g.r(-0.1, 0.1));
                        v.push(vec![c, (c.0 + 0.08, c.1 + 0.05)]);
                    }
                }
                v
            }
            _ => vec![
                poly(g, &[(-0.6, -2.0), (-0.6, 2.0)], 0.05),
                poly(g, &[(-0.25, -2.0), (-0.25, 2.0)], 0.05),
                poly(g, &[(-0.25, 0.0), (0.2, -0.6), (0.6, -1.2), (0.5, -1.8), (0.1, -1.9), (-0.1, -1.6)], 0.07),
                poly(g, &[(-0.25, 0.0), (0.2, 0.6), (0.6, 1.2), (0.5, 1.8), (0.1, 1.9), (-0.1, 1.6)], 0.07),
            ],
        }
    }

    pub fn time_sig(g: &mut Rng, class: usize) -> Vec<S> {
        match class {
            31 | 32 => {
                let mut v = vec![arc((0.0, 0.0), 0.8, 1.0, 0.12, 0.88, 0.0, 18)];
                if class == 32 {
                    v.push(poly(g, &[(0.05, -1.6), (0.05, 1.6)], 0.05));
                }
                v
            }
            _ => {
                let (num, den) = time_of(class).unwrap();
                let mut v = vec![];
                for (n, cy) in [(num, -1.0f32), (den, 1.0)] {
                    if n >= 10 {
                        v.extend(digit(g, 1, -0.5, cy));
                        v.extend(digit(g, n - 10, 0.45, cy));
                    } else {
                        v.extend(digit(g, n, 0.0, cy));
                    }
                }
                v
            }
        }
    }

    fn letter(g: &mut Rng, c: char, dx: f32) -> Vec<S> {
        let v: Vec<S> = match c {
            'p' => {
                if g.chance(0.6) {
                    vec![poly(g, &[(0.0, -0.4), (0.0, 1.2), (0.0, -0.3), (0.4, -0.5), (0.6, -0.2), (0.4, 0.2), (0.0, 0.15)], 0.05)]
                } else {
                    vec![poly(g, &[(0.0, -0.4), (0.0, 1.2)], 0.04), poly(g, &[(0.0, -0.3), (0.4, -0.5), (0.6, -0.2), (0.4, 0.2), (0.0, 0.15)], 0.05)]
                }
            }
            'm' => vec![poly(g, &[(-0.6, 0.4), (-0.6, -0.4), (-0.4, -0.5), (-0.2, -0.3), (-0.2, 0.4), (-0.2, -0.3), (0.0, -0.5), (0.2, -0.3), (0.2, 0.4), (0.2, -0.3), (0.4, -0.5), (0.6, -0.3), (0.6, 0.4)], 0.05)],
            _ => {
                let mut v = vec![poly(g, &[(0.5, -1.2), (0.2, -1.4), (0.0, -1.0), (-0.1, 0.0), (-0.2, 1.0), (-0.5, 1.2)], 0.05)];
                let bar = poly(g, &[(-0.35, -0.3), (0.35, -0.35)], 0.04);
                if g.chance(0.7) {
                    v.push(bar);
                } else {
                    v[0].extend(bar);
                }
                v
            }
        };
        v.into_iter().map(|s| shift(s, dx, 0.0)).collect()
    }

    pub fn dynamic(g: &mut Rng, class: usize) -> Vec<S> {
        let word = dynamic_of(class).unwrap();
        let gap = g.r(0.9, 1.3);
        let n = word.chars().count() as f32;
        let mut v = vec![];
        for (i, c) in word.chars().enumerate() {
            let dx = (i as f32 - (n - 1.0) / 2.0) * gap;
            v.extend(letter(g, c, dx));
        }
        // italic
        let sl = g.r(0.1, 0.35);
        v.into_iter().map(|s| s.into_iter().map(|(x, y)| (x - y * sl, y)).collect()).collect()
    }

    pub fn mark(g: &mut Rng, class: usize) -> Vec<S> {
        match class {
            ACCENT => {
                let (w, h) = (g.r(0.8, 1.5), g.r(0.45, 0.9));
                if g.chance(0.7) {
                    vec![poly(g, &[(-w / 2.0, -h / 2.0), (w / 2.0, 0.0), (-w / 2.0, h / 2.0)], 0.04)]
                } else {
                    vec![poly(g, &[(-w / 2.0, -h / 2.0), (w / 2.0, 0.0)], 0.03), poly(g, &[(w / 2.0, 0.0), (-w / 2.0, h / 2.0)], 0.03)]
                }
            }
            TENUTO => {
                let w = g.r(0.7, 1.3);
                let mid = g.r(-0.06, 0.06);
                vec![poly(g, &[(-w / 2.0, 0.0), (0.0, mid), (w / 2.0, 0.0)], 0.02)]
            }
            _ => {
                // fermata: an arch and a dot under it
                let (w, h) = (g.r(1.6, 2.4), g.r(0.7, 1.2));
                let mut v = vec![arc((0.0, 0.3), w / 2.0, h, 0.5, 1.0, 0.0, 16)];
                if g.chance(0.9) {
                    let c = (g.r(-0.1, 0.1), g.r(0.0, 0.2));
                    v.push(vec![c, (c.0 + 0.08, c.1 + 0.04)]);
                }
                v
            }
        }
    }

    pub fn hairpin(g: &mut Rng, cresc: bool) -> Vec<S> {
        let (l, o) = (g.r(2.5, 16.0), g.r(0.8, 1.8));
        let (tip, open) = if cresc { (-l / 2.0, l / 2.0) } else { (l / 2.0, -l / 2.0) };
        if g.chance(0.6) {
            vec![poly(g, &[(open, -o / 2.0), (tip, 0.0), (open, o / 2.0)], 0.05)]
        } else {
            vec![poly(g, &[(tip, 0.0), (open, -o / 2.0)], 0.04), poly(g, &[(tip, 0.0), (open, o / 2.0)], 0.04)]
        }
    }

    pub fn arc_symbol(g: &mut Rng) -> Vec<S> {
        let l = g.r(1.5, 14.0);
        let s = g.r(0.3, 1.4).min(l * 0.3) * if g.chance(0.5) { 1.0 } else { -1.0 };
        let skew = g.r(-0.2, 0.2);
        // (F-42: a slur between two pitches climbs or falls: up to about a third of its length)
        let rise = if g.chance(0.6) { g.r(-0.35, 0.35) * l } else { 0.0 };
        vec![(0..=20).map(|i| {
            let t = i as f32 / 20.0 * 2.0 - 1.0;
            (t * l / 2.0, s * (1.0 - t * t) + skew * t * s + rise * t / 2.0)
        }).collect()]
    }

    pub fn sample(g: &mut Rng, class: usize) -> Vec<Vec<Pos2>> {
        let raw = match class {
            0..=6 => note(g, class as u8 + 1),
            7..=12 => rest(g, class),
            13..=17 => accidental(g, class),
            DOT => dot(g),
            20..=22 => clef(g, class),
            TIME_FIRST..=TIME_LAST => time_sig(g, class),
            BARLINE => {
                let (t, x, b) = (-2.0 - g.r(0.0, 0.3), g.r(-0.15, 0.15), 2.0 + g.r(0.0, 0.3));
                vec![poly(g, &[(0.0, t), (x, b)], 0.03)]
            }
            DYN_FIRST..=DYN_LAST => dynamic(g, class),
            ACCENT | TENUTO | FERMATA => mark(g, class),
            CRESC | DIM => hairpin(g, class == CRESC),
            ARC => arc_symbol(g),
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

    /// The learning rate and the passes over the data.
    pub const LR: f32 = 0.05;
    pub const EPOCHS: usize = 40;

    /// Trains one network (`nh` hidden): SGD with momentum and a softmax cross-entropy.
    pub fn fit_lr(train: &[(Vec<f32>, usize)], epochs: usize, seed: u64, lr0: f32, nh: usize) -> Model {
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
        let r2 = (2.0 / nh as f32).sqrt();
        let mut m = Model {
            hidden: nh,
            mean,
            std,
            w1: (0..nh * N_FEAT).map(|_| g.r(-r1, r1)).collect(),
            b1: vec![0.0; nh],
            w2: (0..N_OUT * nh).map(|_| g.r(-r2, r2)).collect(),
            b2: vec![0.0; N_OUT],
        };
        let xs: Vec<Vec<f32>> = train.iter().map(|(x, _)| m.standardize(x)).collect();
        let mut order: Vec<usize> = (0..xs.len()).collect();
        let (mut vw1, mut vb1, mut vw2, mut vb2) = (vec![0.0f32; m.w1.len()], vec![0.0f32; nh], vec![0.0f32; m.w2.len()], vec![0.0f32; N_OUT]);
        let batch = 32;
        for ep in 0..epochs {
            let lr = lr0 * (1.0 - ep as f32 / epochs as f32).max(0.05);
            for i in (1..order.len()).rev() {
                let j = g.pick(i + 1);
                order.swap(i, j);
            }
            for chunk in order.chunks(batch) {
                let (mut gw1, mut gb1, mut gw2, mut gb2) = (vec![0.0f32; m.w1.len()], vec![0.0f32; nh], vec![0.0f32; m.w2.len()], vec![0.0f32; N_OUT]);
                for &i in chunk {
                    let x = &xs[i];
                    let y = train[i].1;
                    let (h, p) = m.forward(x);
                    let mut d2 = p.clone();
                    d2[y] -= 1.0;
                    let mut dh = vec![0.0f32; nh];
                    for k in 0..N_OUT {
                        gb2[k] += d2[k];
                        for j in 0..nh {
                            gw2[k * nh + j] += d2[k] * h[j];
                            dh[j] += d2[k] * m.w2[k * nh + j];
                        }
                    }
                    for j in 0..nh {
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

    /// The class of a HOMUS label (the classes Fermata uses), or None.
    pub fn homus_class(label: &str) -> Option<usize> {
        Some(match label.trim() {
            "Whole-Note" => 0,
            "Half-Note" => 1,
            "Quarter-Note" => 2,
            "Eighth-Note" => 3,
            "Sixteenth-Note" => 4,
            "Thirty-Two-Note" => 5,
            "Sixty-Four-Note" => 6,
            "Whole-Half-Rest" => 7,
            "Quarter-Rest" => 8,
            "Eighth-Rest" => 9,
            "Sixteenth-Rest" => 10,
            "Thirty-Two-Rest" => 11,
            "Sixty-Four-Rest" => 12,
            "Sharp" => 13,
            "Flat" => 14,
            "Natural" => 15,
            "Double-Sharp" => 16,
            "Dot" => DOT,
            "G-Clef" => 20,
            "F-Clef" => 21,
            "C-Clef" => 22,
            "4-4-Time" => 23,
            "3-4-Time" => 24,
            "2-4-Time" => 25,
            "2-2-Time" => 26,
            "3-8-Time" => 27,
            "6-8-Time" => 28,
            "9-8-Time" => 29,
            "12-8-Time" => 30,
            "Common-Time" => 31,
            "Cut-Time" => 32,
            "Barline" => BARLINE,
            _ => return None,
        })
    }

    /// The HOMUS samples under `dir` (…/HOMUS/<writer>/<writer>-<n>.txt): (writer, class, strokes in staff
    /// spaces; HOMUS draws on a staff 14 pixels apart), in a fixed order.
    pub fn load_homus(dir: &std::path::Path) -> Vec<(u32, usize, Vec<Vec<Pos2>>)> {
        let mut out = vec![];
        let mut writers: Vec<(u32, std::path::PathBuf)> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok().map(|w| (w, e.path()))).collect();
        writers.sort();
        for (w, path) in writers {
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                let mut lines = text.lines();
                let Some(class) = lines.next().and_then(homus_class) else { continue };
                let strokes: Vec<Vec<Pos2>> = lines
                    .map(|l| l.split(';').filter_map(|pt| {
                        let (x, y) = pt.trim().split_once(',')?;
                        Some(pos2(x.trim().parse::<f32>().ok()? / 14.0, y.trim().parse::<f32>().ok()? / 14.0))
                    }).collect::<Vec<Pos2>>())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !strokes.is_empty() {
                    out.push((w, class, strokes));
                }
            }
        }
        out
    }

    /// HOMUS writers kept out of training: the test of handwriting the model never saw.
    pub const HOMUS_TEST_WRITERS: std::ops::RangeInclusive<u32> = 81..=100;

    /// A sample moved a little (slant, scale), as another take of the same hand.
    fn jiggle(g: &mut Rng, s: &[Vec<Pos2>]) -> Vec<Vec<Pos2>> {
        let rot = g.r(-0.12, 0.12);
        let (sx, sy) = (g.r(0.9, 1.12), g.r(0.9, 1.12));
        let moved: Vec<Vec<Pos2>> = s.iter().map(|st| st.iter().map(|p| {
            let (x, y) = (p.x * sx, p.y * sy);
            pos2(x * rot.cos() - y * rot.sin() + g.r(-0.02, 0.02), x * rot.sin() + y * rot.cos() + g.r(-0.02, 0.02))
        }).collect()).collect();
        let aug = HOMUS_AUG;
        // F-42: each stroke a little off on its own, and the whole bent gently (another hand)
        let all: Vec<Pos2> = moved.iter().flatten().copied().collect();
        let r = bbox(&all);
        let size = r.width().max(r.height()).max(0.3);
        // (a random phase; 6.28 is what the shipped weights were trained with: kept so training reproduces them)
        #[allow(clippy::approx_constant)]
        let (fx, fy, px, py) = (g.r(0.5, 1.5), g.r(0.5, 1.5), g.r(0.0, 6.28), g.r(0.0, 6.28));
        let amp = 0.03 * aug * size;
        moved.into_iter().map(|st| {
            let c = bbox(&st).center();
            let (dx, dy, a) = (g.r(-0.04, 0.04) * aug * size, g.r(-0.04, 0.04) * aug * size, g.r(-0.1, 0.1) * aug);
            st.into_iter().map(|p| {
                let q = p - c;
                let q = vec2(q.x * a.cos() - q.y * a.sin(), q.x * a.sin() + q.y * a.cos());
                let p = c + q + vec2(dx, dy);
                let (u, v) = ((p.y - r.top()) / size, (p.x - r.left()) / size);
                pos2(p.x + amp * (std::f32::consts::TAU * fx * u + px).sin(), p.y + amp * (std::f32::consts::TAU * fy * v + py).sin())
            }).collect()
        }).collect()
    }

    const SYNTH_MODEL: &[u8] = include_bytes!("../assets/pen_model_synthetic.bin");
    const HOMUS_MODEL: &[u8] = include_bytes!("../assets/pen_model.bin");

    /// Accuracy per class of both models, the rules and the app's combination on the test sets, as a table; with
    /// HOMUS, also on its held-out writers.
    pub fn eval_report(homus: Option<&std::path::Path>) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let ctx = crate::pen::Ctx::default();
        let synth = Ensemble::from_bytes(SYNTH_MODEL);
        let hom = Ensemble::from_bytes(HOMUS_MODEL);
        let nets = |e: &Option<Ensemble>| e.as_ref().map(|e| e.models.iter().map(|m| m.hidden.to_string()).collect::<Vec<_>>().join("+")).unwrap_or_else(|| "none".into());
        let _ = writeln!(out, "Fermata pen model: {} features, {} classes, {} views; built-in: {}; synthetic-only networks (hidden): {}; HOMUS+synthetic networks (hidden): {}", N_FEAT, N_OUT, VIEWS.len(), if cfg!(feature = "pen-synthetic-only") { "synthetic only" } else { "HOMUS+synthetic" }, nets(&synth), nets(&hom));
        let mut sets = test_sets();
        if let Some(dir) = homus {
            let test: Vec<(Vec<Vec<Pos2>>, usize)> = load_homus(dir).into_iter().filter(|(w, _, _)| HOMUS_TEST_WRITERS.contains(w)).map(|(_, c, s)| (s, c)).collect();
            sets.insert(0, ("HOMUS, writers 81-100 (never trained on)", test));
        }
        let arg = |p: &[f32]| (0..N_OUT).max_by(|&a, &b| p[a].partial_cmp(&p[b]).unwrap()).unwrap();
        for (name, set) in sets {
            let mut t = vec![[0usize; 5]; N_OUT]; // synthetic, HOMUS+synthetic, rules, the app (built-in + rules), total
            // F-42: what the flagged notes and the rests are taken for (the app), rows = true class
            let mut conf = vec![vec![0usize; N_OUT]; N_OUT];
            for (strokes, y) in &set {
                let ps = synth.as_ref().map(|m| m.predict(strokes)).unwrap_or_else(|| vec![0.0; N_OUT]);
                let ph = hom.as_ref().map(|m| m.predict(strokes)).unwrap_or_else(|| vec![0.0; N_OUT]);
                let rule = crate::pen::recognize(strokes, 1.0, &ctx).first().and_then(|c| class_of(&c.reading)).unwrap_or(OTHER);
                let both = ensemble(strokes, &ctx);
                t[*y][0] += (arg(&ps) == *y) as usize;
                t[*y][1] += (arg(&ph) == *y) as usize;
                t[*y][2] += (rule == *y) as usize;
                t[*y][3] += (arg(&both) == *y) as usize;
                t[*y][4] += 1;
                conf[*y][arg(&both)] += 1;
            }
            if name.starts_with("HOMUS") {
                let _ = writeln!(out, "\n== {name}: flagged notes and rests, what the app read (rows: written)");
                let _ = write!(out, "{:<8}", "");
                for c in &CLASSES[2..=12] {
                    let _ = write!(out, "{c:>7}");
                }
                let _ = writeln!(out, "{:>7}", "else");
                for (y, row) in conf.iter().enumerate().take(13).skip(2) {
                    let _ = write!(out, "{:<8}", CLASSES[y]);
                    for v in &row[2..=12] {
                        let _ = write!(out, "{v:>7}");
                    }
                    let rest: usize = (0..N_OUT).filter(|k| !(2..=12).contains(k)).map(|k| row[k]).sum();
                    let _ = writeln!(out, "{:>7}", rest);
                }
            }
            let _ = writeln!(out, "\n== {name} ({} samples)", set.len());
            let _ = writeln!(out, "{:<8} {:>6} {:>10} {:>10} {:>8} {:>8}", "class", "n", "synthetic", "HOMUS+syn", "rules", "app");
            let mut sum = [0usize; 5];
            for (k, row) in t.iter().enumerate() {
                if row[4] == 0 {
                    continue;
                }
                let pc = |v: usize| format!("{:.0}%", 100.0 * v as f32 / row[4] as f32);
                let _ = writeln!(out, "{:<8} {:>6} {:>10} {:>10} {:>8} {:>8}", CLASSES[k], row[4], pc(row[0]), pc(row[1]), pc(row[2]), pc(row[3]));
                for i in 0..5 {
                    sum[i] += row[i];
                }
            }
            let pc = |v: usize| format!("{:.1}%", 100.0 * v as f32 / sum[4].max(1) as f32);
            let _ = writeln!(out, "{:<8} {:>6} {:>10} {:>10} {:>8} {:>8}", "all", sum[4], pc(sum[0]), pc(sum[1]), pc(sum[2]), pc(sum[3]));
        }
        out
    }

    /// The networks in each weight file.
    pub const NETWORKS: usize = 4;
    /// Each HOMUS sample is trained on as written and this many times moved a little (`jiggle`, with each stroke
    /// a little off on its own and the whole bent this much: F-42).
    pub const HOMUS_REPS: usize = 9;
    pub const HOMUS_AUG: f32 = 2.0;

    /// The training data: synthetic ink (seeds 1…; the tests use seeds from 1 000 000), plus, with `homus`, the
    /// HOMUS writers 1–80 (each sample as written and moved a little).
    pub fn training_data(homus: Option<&std::path::Path>, synthetic_per_class: usize, reps: usize, held_out: &std::ops::RangeInclusive<u32>) -> Vec<(Vec<f32>, usize)> {
        let mut data = dataset(1, synthetic_per_class);
        if let Some(dir) = homus {
            let mut g = Rng(99);
            for (w, c, s) in load_homus(dir) {
                if held_out.contains(&w) {
                    continue;
                }
                data.push((features(&s), c));
                for _ in 0..reps {
                    data.push((features(&jiggle(&mut g, &s)), c));
                }
            }
        }
        data
    }

    /// Trains `hidden.len()` networks at once (one thread each), from seeds 7, 1007, 2007…
    pub fn fit_ensemble(data: &[(Vec<f32>, usize)], hidden: &[usize], epochs: usize, lr: f32) -> Ensemble {
        let models = std::thread::scope(|sc| {
            let hs: Vec<_> = hidden.iter().enumerate().map(|(i, &nh)| sc.spawn(move || fit_lr(data, epochs, 7 + 1000 * i as u64, lr, nh))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        Ensemble { models }
    }

    /// The training runs behind the two weight files (`pen_model_synthetic.bin` without HOMUS, `pen_model.bin` with).
    pub fn train_default(homus: Option<&std::path::Path>) -> Ensemble {
        let data = training_data(homus, 1200, HOMUS_REPS, &HOMUS_TEST_WRITERS);
        fit_ensemble(&data, &[N_HIDDEN; NETWORKS], EPOCHS, LR)
    }
}
