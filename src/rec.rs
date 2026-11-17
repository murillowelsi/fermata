//! Playing a MIDI keyboard into the score: step entry at the cursor, and real-time recording with a click
//! (Record / Enter), where each note appears on the staff the moment it's played and grows while it's held.
use crate::app::App;
use crate::midi_in::MidiMsg;
use crate::model::*;
use crate::score_view::NoteRef;
use std::collections::HashMap;
use std::time::Instant;

pub struct RecState {
    /// When beat 1 of the start measure sounds (after the count-in).
    pub t0: Instant,
    pub start_measure: usize,
    /// Quarter notes per minute.
    pub qpm: f64,
    pub upper: usize,
    pub lower: Option<usize>,
    /// Held keys: key -> (staff, absolute start tick).
    pub open: HashMap<u8, (usize, u64)>,
}

#[derive(Clone, Debug)]
pub struct RecSettings {
    /// Smallest note value written (4 = eighth, 5 = 16th, 6 = 32nd).
    pub quantize: u8,
    /// Keys below this go to the lower staff of a grand staff ("Split = C3" in the original's toolbar,
    /// where C3 is middle C, MIDI 60).
    pub split: u8,
    pub count_in: u8,
    pub click: bool,
}

impl Default for RecSettings {
    fn default() -> Self {
        RecSettings { quantize: 5, split: 60, count_in: 1, click: true }
    }
}

impl App {
    fn rec_grid(&self) -> u64 {
        base_ticks(self.rec_settings.quantize.clamp(3, 7)) as u64
    }

    /// Absolute tick (from the start measure) -> measure and tick in it, adding measures when needed.
    fn rec_locate(&mut self, start: usize, abs: u64) -> (usize, u32) {
        let mut m = start;
        let mut left = abs;
        loop {
            while m >= self.score.measures.len() {
                self.score.push_measure();
            }
            let mt = self.score.measures[m].ticks() as u64;
            if left < mt {
                return (m, left as u32);
            }
            left -= mt;
            m += 1;
        }
    }

    fn rec_abs_now(&self, rs: &RecState, at: Instant) -> u64 {
        let secs = if at >= rs.t0 { (at - rs.t0).as_secs_f64() } else { 0.0 };
        (secs * rs.qpm / 60.0 * TPQ as f64) as u64
    }

    fn quant(&self, t: u64) -> u64 {
        let g = self.rec_grid();
        (t + g / 2) / g * g
    }

    /// Give the recorded note starting at `abs` on staff `s` a written length of `len` ticks.
    fn rec_set_len(&mut self, start_measure: usize, s: usize, abs: u64, len: u64) {
        let (m, local) = self.rec_locate(start_measure, abs);
        let mt = self.score.measures[m].ticks();
        let len = (len as u32).min(mt - local).max(1);
        // never longer than the next note on this staff
        let next = self.score.events[s][m].iter().filter(|e| e.voice == 1 && e.start > local).map(|e| e.start).min().unwrap_or(mt);
        let room = (next - local).max(1);
        let len = len.min(room).max(1);
        // the closest written value that fits (a note released a little early keeps its full value)
        let (v, d) = crate::io::nearest_value(len, room);
        if let Some(e) = self.score.events[s][m].iter_mut().find(|e| e.voice == 1 && e.start == local && !e.is_rest()) {
            e.value = v;
            e.dots = d;
            e.tuplet = None;
            e.play_len = None;
        }
    }

    /// Fill the gaps in voice 1 of a measure with rests, each on its own beat grid (largest that fits first).
    /// Measures with no notes are left empty (they show a whole-measure rest).
    pub fn fill_rests(&mut self, s: usize, m: usize) {
        let mt = self.score.measures[m].ticks();
        let evs = &mut self.score.events[s][m];
        if !evs.iter().any(|e| e.voice == 1 && !e.is_rest()) {
            return;
        }
        evs.retain(|e| !(e.voice == 1 && e.is_rest()));
        let mut spans: Vec<(u32, u32)> = evs.iter().filter(|e| e.voice == 1 && !e.grace).map(|e| (e.start, e.end())).collect();
        spans.sort();
        let mut gaps = vec![];
        let mut pos = 0u32;
        for (a, b) in spans {
            if a > pos {
                gaps.push((pos, a));
            }
            pos = pos.max(b);
        }
        if pos < mt {
            gaps.push((pos, mt));
        }
        for (mut a, b) in gaps {
            while a < b {
                let mut pick = None;
                for v in 0..=8u8 {
                    let t = base_ticks(v);
                    if t <= b - a && a % t == 0 {
                        pick = Some(v);
                        break;
                    }
                }
                let v = pick.unwrap_or_else(|| (0..=8u8).find(|&v| base_ticks(v) <= b - a).unwrap_or(8));
                let t = base_ticks(v).max(1);
                evs.push(Event::new(1, a, v, vec![]));
                a += t;
            }
        }
        self.score.sort_measure(s, m);
    }

    pub fn start_recording(&mut self) {
        self.checkpoint();
        let (s, m, _) = self.cursor;
        let qpm = self.score.tempo_at(m).max(20) as f64;
        let mt = self.score.measures[m].ticks() as f64;
        let count_secs = self.rec_settings.count_in as f64 * mt / TPQ as f64 * 60.0 / qpm;
        // the lower staff of a braced pair takes the low notes
        let lower = if self.score.staves[s].brace_with_next && s + 1 < self.score.staves.len() { Some(s + 1) } else { None };
        // click through the built-in synth
        if self.rec_settings.click {
            let mut notes = vec![];
            let mut t = 0.0f64;
            let beat_of = |mi: &MeasureInfo| TPQ * 4 / mi.den.max(1) as u32;
            let first = self.score.measures[m].clone();
            for _ in 0..self.rec_settings.count_in {
                let mut b = 0;
                while b < first.ticks() {
                    notes.push(crate::audio::PlayNote { at: t + b as f64 / TPQ as f64 * 60.0 / qpm, len: 0.03, midi: if b == 0 { 96 } else { 84 }, vel: 0.6, program: 115, gain: 0.6, pan: 0.0, drum: true, channel: 9, click: true, ..Default::default() });
                    b += beat_of(&first);
                }
                t += first.ticks() as f64 / TPQ as f64 * 60.0 / qpm;
            }
            for k in 0..400usize {
                let mi = self.score.measures.get(m + k).cloned().unwrap_or_else(|| first.clone());
                let mut b = 0;
                while b < mi.ticks() {
                    notes.push(crate::audio::PlayNote { at: t + b as f64 / TPQ as f64 * 60.0 / qpm, len: 0.03, midi: if b == 0 { 96 } else { 84 }, vel: 0.5, program: 115, gain: 0.5, pan: 0.0, drum: true, channel: 9, click: true, ..Default::default() });
                    b += beat_of(&mi);
                }
                t += mi.ticks() as f64 / TPQ as f64 * 60.0 / qpm;
            }
            self.apply_click_prefs(&mut notes, false);
            self.play_notes(notes, 0.0, qpm, false);
        }
        let now = Instant::now();
        self.rec_new.clear();
        self.rec_cleared = 0;
        self.rec = Some(RecState {
            t0: now + std::time::Duration::from_secs_f64(count_secs),
            start_measure: m,
            qpm,
            upper: s,
            lower,
            open: HashMap::new(),
        });
        self.recording = true;
        self.selected.clear();
        self.status = format!(
            "Recording from measure {} at {} per minute{} - play your keyboard. Enter or Space stops.",
            m + 1,
            qpm as u32,
            if self.rec_settings.count_in > 0 { " after one bar of count-in" } else { "" }
        );
    }

    pub fn stop_recording(&mut self) {
        if let Some(rs) = self.rec.take() {
            let now = Instant::now();
            if now >= rs.t0 {
                let upto = self.quant(self.rec_abs_now(&rs, now));
                self.rec_clear(&rs, upto.saturating_sub(1));
            }
            let open: Vec<(u8, (usize, u64))> = rs.open.iter().map(|(k, v)| (*k, *v)).collect();
            for (_, (s, start)) in open {
                let end = self.quant(self.rec_abs_now(&rs, now)).max(start + self.rec_grid());
                self.rec_set_len(rs.start_measure, s, start, end - start);
            }
            for s in 0..self.score.staves.len() {
                for m in 0..self.score.measures.len() {
                    self.score.sort_measure(s, m);
                }
            }
            // rests fill the gaps in the measures this take wrote into (e.g. dotted half + quarter rest)
            let last = self.rec_locate(rs.start_measure, self.quant(self.rec_abs_now(&rs, now))).0;
            let staves: Vec<usize> = std::iter::once(rs.upper).chain(rs.lower).collect();
            for s in staves {
                for m in rs.start_measure..=last.min(self.score.measures.len() - 1) {
                    self.fill_rests(s, m);
                }
            }
        }
        self.recording = false;
        self.stop_sound();
        self.status = "Recording stopped.".into();
    }

    /// Notes from the keyboard: recorded live while recording, otherwise entered step by step at the cursor.
    pub fn handle_midi(&mut self, msg: MidiMsg) {
        if msg.pedal.is_some() {
            return;
        }
        if let Some(rs) = self.rec.take() {
            self.handle_midi_rec(&rs, msg);
            self.rec = Some(rs);
            return;
        }
        // step entry: keys pressed together (within 60 ms) make one chord
        if !msg.on {
            return;
        }
        let flats = self.score.measures[self.cursor.1].key < 0;
        let pitch = Pitch::from_midi(msg.note as i32, flats);
        if let Some((t, r)) = self.last_step {
            if msg.at.saturating_duration_since(t).as_millis() < 60 {
                if let Some(e) = self.ev_mut(r) {
                    if !e.pitches.contains(&pitch) {
                        e.pitches.push(pitch);
                        e.sort_pitches();
                    }
                }
                return;
            }
        }
        let (s, m, t) = self.cursor;
        let (v, d) = (self.cur_value, self.cur_dots);
        let aud = self.audition;
        self.audition = false; // the keyboard already sounds through Thru
        let r = self.enter(s, m, t, Some(pitch), v, d, true);
        self.audition = aud;
        self.last_step = Some((msg.at, r));
        self.scroll_to_cursor = true;
        self.status = format!("{} entered (step entry). Press Enter to record in real time.", pitch.name());
    }

    /// Recording overwrites: on the staves being recorded, every note that isn't from this take is removed from
    /// the take's start up to `upto` (absolute ticks, inclusive), as the recording passes over it.
    fn rec_clear(&mut self, rs: &RecState, upto: u64) {
        if upto < self.rec_cleared {
            return;
        }
        let from = self.rec_cleared;
        let staves: Vec<usize> = std::iter::once(rs.upper).chain(rs.lower).collect();
        let mut m = rs.start_measure;
        let mut m_abs = 0u64;
        while m < self.score.measures.len() && m_abs <= upto {
            let mt = self.score.measures[m].ticks() as u64;
            if m_abs + mt > from {
                for &s in &staves {
                    let new = &self.rec_new;
                    self.score.events[s][m].retain(|e| {
                        let a = m_abs + e.start as u64;
                        !(a >= from && a <= upto && !new.contains(&(s, m, e.start)))
                    });
                }
            }
            m_abs += mt;
            m += 1;
        }
        self.selected.clear();
        self.rec_cleared = upto + 1;
    }

    fn handle_midi_rec(&mut self, rs: &RecState, msg: MidiMsg) {
        let grid = self.rec_grid();
        let abs = self.quant(self.rec_abs_now(rs, msg.at));
        let staff = match rs.lower {
            Some(lo) if msg.note < self.rec_settings.split => lo,
            _ => rs.upper,
        };
        if msg.on {
            let (m, local) = self.rec_locate(rs.start_measure, abs);
            // old notes up to here give way to the new take (never added to)
            self.rec_clear(rs, abs);
            let flats = self.score.measures[m].key < 0;
            let pitch = Pitch::from_midi(msg.note as i32, flats);
            // a key pressed with others (same grid point) joins their chord
            if let Some(e) = self.score.events[staff][m].iter_mut().find(|e| e.voice == 1 && e.start == local && !e.is_rest()) {
                if !e.pitches.contains(&pitch) {
                    e.pitches.push(pitch);
                    e.sort_pitches();
                }
            } else {
                // the previous note on this staff ends where this one starts
                if let Some(prev) = self.score.events[staff][m].iter().filter(|e| e.voice == 1 && e.start < local).map(|e| e.start).max() {
                    let prev_abs = abs - (local - prev) as u64;
                    let cur_end = self.score.events[staff][m].iter().find(|e| e.voice == 1 && e.start == prev).map(|e| e.end()).unwrap_or(0);
                    if cur_end > local {
                        self.rec_set_len(rs.start_measure, staff, prev_abs, (local - prev) as u64);
                    }
                }
                let (v, d) = crate::io::guess_value(grid as u32);
                let mut e = Event::new(1, local, v, vec![pitch]);
                e.dots = d;
                e.velocity = msg.vel.max(1);
                self.score.events[staff][m].push(e);
                self.rec_new.insert((staff, m, local));
                self.score.sort_measure(staff, m);
            }
            if let Some(rsm) = self.rec.as_mut() {
                rsm.open.insert(msg.note, (staff, abs));
            }
            // keep the open-note table on the state we'll put back
            self.pending_open.push((msg.note, staff, abs, true));
            self.cursor = (rs.upper, m, local);
            self.scroll_to_cursor = true;
            self.dirty = true;
        } else {
            self.pending_open.push((msg.note, staff, abs, false));
        }
    }

    /// Called every frame while recording: grows held notes and applies key releases.
    pub fn rec_tick(&mut self) {
        let Some(mut rs) = self.rec.take() else { return };
        for (note, staff, abs, on) in std::mem::take(&mut self.pending_open) {
            if on {
                rs.open.insert(note, (staff, abs));
            } else if let Some((s, start)) = rs.open.remove(&note) {
                let len = abs.saturating_sub(start).max(self.rec_grid());
                self.rec_set_len(rs.start_measure, s, start, len);
            }
        }
        let now = self.quant(self.rec_abs_now(&rs, Instant::now()));
        if Instant::now() >= rs.t0 {
            self.rec_clear(&rs, now);
        }
        let held: Vec<(usize, u64)> = rs.open.values().copied().collect();
        for (s, start) in held {
            let len = now.saturating_sub(start).max(self.rec_grid());
            self.rec_set_len(rs.start_measure, s, start, len);
        }
        // show where the recording is
        let (m, local) = self.rec_locate(rs.start_measure, now);
        if (m, local) != (self.cursor.1, self.cursor.2) && Instant::now() >= rs.t0 {
            self.cursor = (rs.upper, m, local);
            self.scroll_to_cursor = true;
        }
        self.rec = Some(rs);
    }

    pub fn _rec_ref(&self) -> Option<NoteRef> {
        None
    }
}
