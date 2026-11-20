//! F-20: quantizing played or imported notes without losing triplets. For each beat the onsets are measured against
//! the binary grid (16ths) and against triplet grids (quarter-note triplets over two beats, eighth-note triplets in
//! a beat, sixteenth-note triplets in half a beat); a triplet grid is taken only when it fits clearly better and the
//! notes make the group (or nearly), with room for a human performance. Everything else stays on the binary grid,
//! exactly as before.
use crate::model::{Event, Pitch, TPQ};

/// A stretch of time written as one triplet group: `start` (absolute ticks), the triplet unit (`unit` ticks, written
/// as `value` under 3:2), and the slots (0..3) that notes start on.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub start: u32,
    pub unit: u32,
    pub value: u8,
    pub slots: Vec<u32>,
}

impl Group {
    pub fn end(&self) -> u32 {
        self.start + 3 * self.unit
    }
    /// The onset at `t` belongs to this group (it may come a little early).
    pub fn takes(&self, t: u32) -> bool {
        let early = EARLY.min(self.unit / 3);
        t + early >= self.start && t + early < self.end()
    }
    /// The slot (0..3) of an onset this group takes.
    pub fn slot(&self, t: u32) -> u32 {
        ((t as i64 - self.start as i64 + self.unit as i64 / 2).div_euclid(self.unit as i64)).clamp(0, 2) as u32
    }
}

/// The notes and rests of a triplet group, in voice 1, starting at `g.start % measure_len`: each note (`members`:
/// slot, MIDI key, velocity; chord notes share a slot) lasts to the next one in the group, the last to the group's
/// end, and a rest stands where the group starts late.
pub fn group_events(g: &Group, members: &[(u32, u8, u8)], flats: bool, measure_len: u32) -> Vec<Event> {
    let mut slots: Vec<u32> = members.iter().map(|x| x.0).collect();
    slots.sort_unstable();
    slots.dedup();
    let mut out = vec![];
    let ev = |slot: u32, until: u32, pitches: Vec<Pitch>, vel: u8| {
        let units = until - slot;
        let mut e = Event::new(1, (g.start + slot * g.unit) % measure_len, if units >= 2 { g.value - 1 } else { g.value }, pitches);
        e.tuplet = Some((3, 2));
        e.velocity = vel;
        if !e.pitches.is_empty() {
            e.sort_pitches();
        }
        e
    };
    let Some(&first) = slots.first() else { return out };
    if first > 0 {
        out.push(ev(0, first, vec![], 80));
    }
    for (i, &sl) in slots.iter().enumerate() {
        let until = slots.get(i + 1).copied().unwrap_or(3);
        let mut ps: Vec<Pitch> = members.iter().filter(|x| x.0 == sl).map(|x| Pitch::from_midi(x.1 as i32, flats)).collect();
        ps.sort_by_key(|p| p.midi());
        ps.dedup();
        let vel = members.iter().filter(|x| x.0 == sl).map(|x| x.2).max().unwrap_or(80);
        out.push(ev(sl, until, ps, vel));
    }
    out
}

/// An onset this far before a beat (ticks) still counts as on it (played a little early).
const EARLY: u32 = 40;
/// What a 16th off the eighth-note grid costs the binary reading, in ticks of error.
const SYNCOPATION: i64 = 15;

/// The triplet groups among `onsets` (absolute ticks, any order; chord notes may repeat), for measures of
/// `measure_len` ticks starting at 0, up to `end`.
pub fn triplet_groups(onsets: &[u32], measure_len: u32, end: u32) -> Vec<Group> {
    let mut on: Vec<u32> = onsets.to_vec();
    on.sort_unstable();
    // a chord's notes, or a spread chord, are one onset
    let mut ons: Vec<u32> = vec![];
    for t in on {
        if ons.last().is_none_or(|&l| t > l + 30) {
            ons.push(t);
        }
    }
    let beat = TPQ;
    let beats_per_measure = measure_len / beat;
    let mut groups = vec![];
    if beats_per_measure == 0 {
        return groups;
    }
    let measures = end / measure_len + 1;
    for m in 0..measures {
        let m0 = m * measure_len;
        let mut k = 0;
        while k < beats_per_measure {
            let b0 = m0 + k * beat;
            // quarter-note triplets: two beats
            if k + 1 < beats_per_measure {
                if let Some(g) = fit(&ons, b0, 2 * beat / 3, 3, true) {
                    groups.push(g);
                    k += 2;
                    continue;
                }
            }
            // eighth-note triplets: one beat
            if let Some(g) = fit(&ons, b0, beat / 3, 4, false) {
                groups.push(g);
            } else {
                // sixteenth-note triplets: each half of the beat
                for h in 0..2 {
                    if let Some(g) = fit(&ons, b0 + h * beat / 2, beat / 6, 5, true) {
                        groups.push(g);
                    }
                }
            }
            k += 1;
        }
    }
    groups
}

/// The onsets of a window `[w0, w0 + 3 unit)` written as a triplet group of `unit`, when that fits clearly better
/// than the 16ths the binary import writes (a 16th off the eighth-note grid, a syncopation, costs a little more).
/// `full`: the group must have its notes on both upbeat slots (1 and 2); otherwise two of the three slots, one an
/// upbeat, will do.
fn fit(ons: &[u32], w0: u32, unit: u32, value: u8, full: bool) -> Option<Group> {
    let len = 3 * unit;
    let early = EARLY.min(unit / 3);
    let inside: Vec<i64> = ons.iter().filter(|&&t| t + early >= w0 && t + early < w0 + len).map(|&t| t as i64 - w0 as i64).collect();
    if inside.len() < 2 {
        return None;
    }
    let bin = (TPQ / 4) as i64;
    let (u, mut slots, mut d_tri, mut d_bin) = (unit as i64, vec![], 0i64, 0i64);
    let mut worst = 0i64;
    for &r in &inside {
        let s = (r + u / 2).div_euclid(u);
        if !(0..3).contains(&s) {
            return None;
        }
        let dt = (r - s * u).abs();
        let snapped = (r + w0 as i64 + bin / 2).div_euclid(bin) * bin;
        let db = (r + w0 as i64 - snapped).abs() + if snapped.rem_euclid(2 * bin) == bin { SYNCOPATION } else { 0 };
        worst = worst.max(dt);
        d_tri += dt;
        d_bin += db;
        if slots.contains(&(s as u32)) {
            return None;
        }
        slots.push(s as u32);
    }
    let up = |s: u32| slots.contains(&s);
    let complete = up(1) && up(2);
    if full && !complete {
        return None;
    }
    if !(up(1) || up(2)) {
        return None;
    }
    // the tolerance for a played performance: about a third of the unit for a whole group, less for two notes
    let tol = if complete { (u * 3 / 10).min(50) } else { (u / 5).min(30) };
    if worst > tol {
        return None;
    }
    // clearly better than the binary grid
    if d_tri + u / 8 >= d_bin {
        return None;
    }
    slots.sort_unstable();
    Some(Group { start: w0, unit, value, slots })
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: u32 = TPQ;
    const BAR: u32 = 4 * TPQ;

    #[test]
    fn straight_eighths_and_sixteenths_stay_binary() {
        let on: Vec<u32> = (0..16).map(|i| i * Q / 4).collect();
        assert!(triplet_groups(&on, BAR, BAR).is_empty());
        let on: Vec<u32> = (0..8).map(|i| i * Q / 2).collect();
        assert!(triplet_groups(&on, BAR, BAR).is_empty());
        // dotted eighth + sixteenth
        let on = [0, 3 * Q / 4, Q, Q + 3 * Q / 4];
        assert!(triplet_groups(&on, BAR, BAR).is_empty());
    }

    #[test]
    fn eighth_triplets_are_found_even_played() {
        let on = [0, 160, 320, Q, Q + Q / 2];
        assert_eq!(triplet_groups(&on, BAR, BAR), vec![Group { start: 0, unit: 160, value: 4, slots: vec![0, 1, 2] }]);
        // played: 20 ticks off either way
        let on = [12, 178, 302];
        assert_eq!(triplet_groups(&on, BAR, BAR).len(), 1);
        // a quarter-triplet + eighth-triplet (long-short)
        let on = [0, 320];
        assert_eq!(triplet_groups(&on, BAR, BAR)[0].slots, vec![0, 2]);
    }

    #[test]
    fn light_swing_stays_straight() {
        // eighths swung to 58%: not a triplet
        let on: Vec<u32> = (0..4).flat_map(|b| [b * Q, b * Q + 278]).collect();
        assert!(triplet_groups(&on, BAR, BAR).is_empty());
    }
}
