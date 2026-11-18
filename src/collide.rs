//! Placing what shares a moment on a staff without collisions (F-04): seconds in a chord, two voices on one staff,
//! columns of accidentals, augmentation dots. Pure geometry in staff spaces, so it can be tested without drawing.

/// Steps are diatonic staff positions (one step = half a space), higher is higher.
///
/// The noteheads of a chord: x offset of each (0 = the normal side of the stem). In a second the stem has heads on
/// both sides: with the stem up the upper note of the pair goes to the right, with the stem down the lower note goes
/// to the left. `w` is the notehead width. Clusters alternate.
pub fn chord_offsets(steps: &[i32], up: bool, w: f32) -> Vec<f32> {
    let mut order: Vec<usize> = (0..steps.len()).collect();
    order.sort_by_key(|&k| steps[k]);
    if !up {
        order.reverse();
    }
    let mut off = vec![0.0; steps.len()];
    let mut prev: Option<usize> = None;
    for &k in &order {
        if let Some(pk) = prev {
            if (steps[k] - steps[pk]).abs() == 1 && off[pk] == 0.0 {
                off[k] = if up { w } else { -w };
            }
        }
        prev = Some(k);
    }
    off
}

/// One voice's notes at a moment, for `voice_shift`.
#[derive(Clone, Debug)]
pub struct VoiceAt<'a> {
    pub steps: &'a [i32],
    /// 0 double whole .. 8 = 128th.
    pub value: u8,
    pub dots: u8,
}

/// The notehead shape of a value (heads of the same shape can be shared in a unison).
fn head_kind(value: u8) -> u8 {
    value.min(3)
}

/// Two voices on one staff at the same moment, the upper with stems up, the lower with stems down. Returns how far
/// the upper voice moves right (in noteheads, 0 or 1) so nothing collides:
/// - apart by a third or more and not crossing: no move;
/// - a unison with the same notehead and the same dots: the head is shared, no move;
/// - a second, a crossing, or a unison of different heads: the upper voice moves right by one notehead.
pub fn voice_shift(upper: &VoiceAt, lower: &VoiceAt) -> u8 {
    let (Some(&umin), Some(&lmax)) = (upper.steps.iter().min(), lower.steps.iter().max()) else { return 0 };
    if umin - lmax >= 2 {
        return 0;
    }
    let close: Vec<i32> = upper.steps.iter().flat_map(|u| lower.steps.iter().map(move |l| (u - l).abs())).filter(|&d| d <= 1).collect();
    let only_unisons = !close.is_empty() && close.iter().all(|&d| d == 0);
    let crossing = umin < lmax;
    let shareable = head_kind(upper.value) == head_kind(lower.value) && upper.dots == lower.dots && upper.value >= 2;
    if only_unisons && shareable && !crossing && upper.steps.len() == 1 && lower.steps.len() == 1 {
        return 0;
    }
    if close.is_empty() && !crossing {
        return 0;
    }
    1
}

/// An accidental to place: its line or space (`y`, screen units growing downwards), how far its ink reaches above
/// and below that point, and its width (all in the same units).
#[derive(Clone, Copy, Debug)]
pub struct Acc {
    pub y: f32,
    pub above: f32,
    pub below: f32,
    pub width: f32,
}

/// Columns of accidentals left of the noteheads: returns each accidental's right edge as a distance to the left of
/// `0` (the left edge of the leftmost notehead minus the gap). Highest first, each goes into the nearest column where
/// it doesn't touch an accidental already there (with `pad` clearance); a column is as wide as its widest
/// accidental, columns `gap` apart.
pub fn accidental_columns(accs: &[Acc], gap: f32, pad: f32) -> Vec<f32> {
    let mut order: Vec<usize> = (0..accs.len()).collect();
    order.sort_by(|&a, &b| accs[a].y.partial_cmp(&accs[b].y).unwrap_or(std::cmp::Ordering::Equal));
    let mut columns: Vec<Vec<usize>> = vec![];
    let mut col_of = vec![0usize; accs.len()];
    for &k in &order {
        let a = accs[k];
        let fits = |col: &Vec<usize>| col.iter().all(|&o| {
            let b = accs[o];
            a.y + a.below + pad <= b.y - b.above || b.y + b.below + pad <= a.y - a.above
        });
        let c = match columns.iter().position(fits) {
            Some(c) => c,
            None => {
                columns.push(vec![]);
                columns.len() - 1
            }
        };
        columns[c].push(k);
        col_of[k] = c;
    }
    // right edge of each column, going left
    let mut right = vec![0.0f32; columns.len()];
    let mut x = 0.0;
    for (c, col) in columns.iter().enumerate() {
        right[c] = x;
        let w = col.iter().map(|&k| accs[k].width).fold(0.0, f32::max);
        x -= w + gap;
    }
    (0..accs.len()).map(|k| right[col_of[k]]).collect()
}

/// Where an augmentation dot goes for a note at `step` (relative to the middle line, half spaces): a note on a line
/// has its dot in the space above, or below for the lower of two voices. Returns the dot's step.
pub fn dot_step(rel: i32, lower_voice: bool) -> i32 {
    if rel.rem_euclid(2) == 0 {
        if lower_voice {
            rel - 1
        } else {
            rel + 1
        }
    } else {
        rel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_put_a_head_on_the_other_side() {
        // stem up: the upper note of the second goes right
        assert_eq!(chord_offsets(&[0, 1], true, 1.0), vec![0.0, 1.0]);
        // stem down: the lower note goes left
        assert_eq!(chord_offsets(&[0, 1], false, 1.0), vec![-1.0, 0.0]);
        // a cluster of three alternates; a third doesn't move
        assert_eq!(chord_offsets(&[0, 1, 2], true, 1.0), vec![0.0, 1.0, 0.0]);
        assert_eq!(chord_offsets(&[0, 2, 4], true, 1.0), vec![0.0, 0.0, 0.0]);
        // order of entry doesn't matter
        assert_eq!(chord_offsets(&[1, 0], true, 1.0), vec![1.0, 0.0]);
    }

    #[test]
    fn two_voices() {
        let v = |steps: &'static [i32], value: u8, dots: u8| VoiceAt { steps, value, dots };
        // a third or more apart: nothing moves
        assert_eq!(voice_shift(&v(&[4], 3, 0), &v(&[2], 3, 0)), 0);
        // a second: the upper voice moves right
        assert_eq!(voice_shift(&v(&[3], 3, 0), &v(&[2], 3, 0)), 1);
        // a unison of two quarters shares the head; a half and a quarter can't
        assert_eq!(voice_shift(&v(&[2], 3, 0), &v(&[2], 3, 0)), 0);
        assert_eq!(voice_shift(&v(&[2], 2, 0), &v(&[2], 3, 0)), 1);
        // dotted against undotted: no sharing
        assert_eq!(voice_shift(&v(&[2], 3, 1), &v(&[2], 3, 0)), 1);
        // crossing voices: the upper voice moves
        assert_eq!(voice_shift(&v(&[1], 3, 0), &v(&[4], 3, 0)), 1);
        // whole notes in unison never share (no stem to tell them apart)
        assert_eq!(voice_shift(&v(&[2], 1, 0), &v(&[2], 1, 0)), 1);
    }

    #[test]
    fn accidentals_never_touch() {
        // a chord of four notes with three accidentals a third apart: sharp (1.4 up, 1.4 down), flat (1.75, 0.7)
        let sharp = |y: f32| Acc { y, above: 1.4, below: 1.4, width: 1.0 };
        let flat = |y: f32| Acc { y, above: 1.75, below: 0.7, width: 0.9 };
        let accs = [sharp(0.0), flat(1.0), sharp(2.0)];
        let x = accidental_columns(&accs, 0.15, 0.1);
        for i in 0..accs.len() {
            for j in i + 1..accs.len() {
                let (a, b) = (accs[i], accs[j]);
                let vert = !(a.y + a.below + 0.1 <= b.y - b.above || b.y + b.below + 0.1 <= a.y - a.above);
                let horiz = !(x[i] - a.width >= x[j] || x[j] - b.width >= x[i]);
                assert!(!(vert && horiz), "accidentals {i} and {j} touch: {x:?}");
            }
        }
        // far apart (an octave): both in the first column
        assert_eq!(accidental_columns(&[sharp(0.0), sharp(3.5)], 0.15, 0.1), vec![0.0, 0.0]);
    }

    #[test]
    fn dots_go_in_spaces() {
        assert_eq!(dot_step(0, false), 1, "on the middle line: the space above");
        assert_eq!(dot_step(0, true), -1, "lower voice: the space below");
        assert_eq!(dot_step(1, false), 1, "in a space: stays");
        assert_eq!(dot_step(-2, false), -1);
    }
}
