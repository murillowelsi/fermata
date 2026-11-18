//! Built-in software synthesizer and score playback (stands in for MIDI devices / the VST player).
use crate::model::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct PlayNote {
    pub at: f64,
    pub len: f64,
    pub midi: i32,
    pub vel: f32,
    pub program: u8,
    pub gain: f32,
    pub pan: f32,
    pub drum: bool,
    /// MIDI channel 0..15 and output port (0 = A, 1 = B) for an external instrument.
    pub channel: u8,
    pub port: u8,
    /// Metronome click (not part of the score).
    pub click: bool,
    /// A controller change instead of a note (e.g. sustain pedal 64).
    pub cc: Option<(u8, u8)>,
}

/// Where each measure begins in seconds, so the UI can follow playback.
#[derive(Clone, Debug, Default)]
pub struct TimeMap {
    pub measures: Vec<(f64, f64, u32)>, // (start sec, sec per tick, ticks)
    /// Order in which measures are played (repeats/endings expanded).
    pub order: Vec<usize>,
}

impl TimeMap {
    pub fn locate(&self, t: f64) -> Option<(usize, u32)> {
        for (k, &(s, spt, ticks)) in self.measures.iter().enumerate() {
            let end = s + spt * ticks as f64;
            if t >= s && t < end {
                return Some((self.order[k], ((t - s) / spt) as u32));
            }
        }
        None
    }
    pub fn total(&self) -> f64 {
        self.measures.last().map(|&(s, spt, t)| s + spt * t as f64).unwrap_or(0.0)
    }
}

struct Voice {
    freq: f64,
    phase: f64,
    phase2: f64,
    age: f64,
    len: f64,
    vel: f32,
    program: u8,
    gain: f32,
    pan: f32,
    drum: bool,
    noise: u32,
    done: bool,
    /// MIDI key of a live (held) note, released by note_off.
    key: Option<i32>,
}

#[derive(Default)]
struct SynthState {
    notes: Vec<PlayNote>,
    next: usize,
    time: f64,
    playing: bool,
    voices: Vec<Voice>,
    master: f32,
    pending: Vec<PlayNote>,
    /// F-30: the SoundFont synthesizer once loaded (None: the built-in sound plays).
    gm: Option<crate::soundfont::GmSynth>,
    /// Loaded while something was sounding: switched in at the next silence, never in the middle of a note.
    gm_ready: Option<crate::soundfont::GmSynth>,
    /// Seconds since the sound started (runs also when not playing), and the note-offs due on that clock.
    clock: f64,
    offs: Vec<(f64, u8, i32)>,
    /// A MIDI keyboard's notes: (on, key, instrument, velocity).
    live: Vec<(bool, i32, u8, u8)>,
}

/// A handle other threads (the MIDI input) can use to sound notes without waiting for the screen.
#[derive(Clone)]
pub struct SynthHandle(Arc<Mutex<SynthState>>);

impl SynthHandle {
    pub fn note_on(&self, midi: i32, program: u8, vel: u8) {
        if let Ok(mut s) = self.0.lock() {
            if s.gm.is_some() {
                s.live.push((true, midi, program, vel));
                return;
            }
            let mut v = start_voice(&PlayNote { at: 0.0, len: 1.0e9, midi, vel: vel as f32 / 127.0, program, gain: 1.0, pan: 0.0, drum: false, ..Default::default() });
            v.key = Some(midi);
            s.voices.push(v);
        }
    }
    pub fn note_off(&self, midi: i32) {
        if let Ok(mut s) = self.0.lock() {
            if s.gm.is_some() {
                s.live.push((false, midi, 0, 0));
                return;
            }
            for v in s.voices.iter_mut() {
                if v.key == Some(midi) && v.len > v.age {
                    v.len = v.age;
                    v.key = None;
                }
            }
        }
    }
}

pub struct Audio {
    state: Arc<Mutex<SynthState>>,
    _stream: Option<cpal::Stream>,
    pub sample_rate: f64,
    pub error: Option<String>,
}

fn family_wave(program: u8, ph: f64, ph2: f64) -> f64 {
    let s = |x: f64| (x * std::f64::consts::TAU).sin();
    let saw = |x: f64| 2.0 * (x - x.floor()) - 1.0;
    let sq = |x: f64| if (x - x.floor()) < 0.5 { 1.0 } else { -1.0 };
    match program / 8 {
        0 | 1 => s(ph) * 0.7 + s(ph * 2.0) * 0.2 + s(ph * 3.0) * 0.08 + s(ph * 4.0) * 0.04, // pianos, chromatic perc
        2 => s(ph) * 0.5 + s(ph * 2.0) * 0.3 + s(ph * 4.0) * 0.2,                          // organs
        3 => s(ph) * 0.6 + s(ph * 2.0) * 0.25 + s(ph * 3.0) * 0.15,                        // guitars
        4 => s(ph) * 0.85 + s(ph * 2.0) * 0.15,                                            // bass
        5 | 6 => saw(ph) * 0.35 + saw(ph2) * 0.35,                                         // strings, ensemble
        7 => saw(ph) * 0.6,                                                                // brass
        8 => sq(ph) * 0.3 + s(ph) * 0.4,                                                   // reeds
        9 => s(ph) * 0.9 + s(ph * 2.0) * 0.05,                                             // pipes
        10 => saw(ph) * 0.5,
        11 => saw(ph) * 0.3 + saw(ph2) * 0.3,
        _ => s(ph) * 0.6 + sq(ph) * 0.15,
    }
}

fn envelope(program: u8, age: f64, len: f64) -> f64 {
    let fam = program / 8;
    let (a, decay, sustain, rel) = match fam {
        0 | 1 => (0.005, 1.6, 0.0, 0.25),
        3 | 13 => (0.005, 1.2, 0.05, 0.2),
        4 => (0.01, 0.8, 0.3, 0.12),
        2 | 9 => (0.02, 0.1, 0.9, 0.08),
        5 | 6 | 11 => (0.08, 0.3, 0.8, 0.25),
        _ => (0.03, 0.2, 0.75, 0.12),
    };
    let held = if age < len {
        if age < a {
            age / a
        } else {
            let d = age - a;
            if sustain > 0.0 { sustain + (1.0 - sustain) * (-d / decay * 3.0).exp() } else { (-d / decay * 2.2).exp() }
        }
    } else {
        let at_rel = envelope_hold(a, decay, sustain, len);
        at_rel * (1.0 - (age - len) / rel).max(0.0)
    };
    held
}

fn envelope_hold(a: f64, decay: f64, sustain: f64, len: f64) -> f64 {
    if len < a {
        return len / a;
    }
    let d = len - a;
    if sustain > 0.0 { sustain + (1.0 - sustain) * (-d / decay * 3.0).exp() } else { (-d / decay * 2.2).exp() }
}

impl Audio {
    pub fn new() -> Audio {
        let state = Arc::new(Mutex::new(SynthState { master: 0.7, ..Default::default() }));
        let host = cpal::default_host();
        let dev = match host.default_output_device() {
            Some(d) => d,
            None => return Audio { state, _stream: None, sample_rate: 44100.0, error: Some("No audio output device".into()) },
        };
        let cfg = match dev.default_output_config() {
            Ok(c) => c,
            Err(e) => return Audio { state, _stream: None, sample_rate: 44100.0, error: Some(e.to_string()) },
        };
        let sr = cfg.sample_rate() as f64;
        let channels = cfg.channels() as usize;
        let st2 = state.clone();
        let config: cpal::StreamConfig = cfg.config();
        let stream = dev.build_output_stream::<f32, _, _>(
            config,
            move |data: &mut [f32], _| {
                let mut s = match st2.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                render(&mut s, data, channels, sr);
            },
            |e| eprintln!("audio error: {e}"),
            None,
        );
        match stream {
            Ok(stream) => {
                let _ = stream.play();
                attach_soundfont(&state, sr);
                Audio { state, _stream: Some(stream), sample_rate: sr, error: None }
            }
            Err(e) => Audio { state, _stream: None, sample_rate: sr, error: Some(e.to_string()) },
        }
    }

    pub fn play(&self, mut notes: Vec<PlayNote>, from: f64, master: f32) {
        notes.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());
        if let Ok(mut s) = self.state.lock() {
            gm_stop(&mut s);
            s.next = notes.iter().position(|n| n.at >= from).unwrap_or(notes.len());
            s.notes = notes;
            s.time = from;
            s.playing = true;
            s.master = master;
            s.voices.clear();
        }
    }
    pub fn stop(&self) {
        if let Ok(mut s) = self.state.lock() {
            gm_stop(&mut s);
            s.playing = false;
            for v in s.voices.iter_mut() {
                if v.len > v.age {
                    v.len = v.age;
                }
            }
        }
    }
    pub fn is_playing(&self) -> bool {
        self.state.lock().map(|s| s.playing).unwrap_or(false)
    }
    pub fn handle(&self) -> SynthHandle {
        SynthHandle(self.state.clone())
    }
    pub fn time(&self) -> f64 {
        self.state.lock().map(|s| s.time).unwrap_or(0.0)
    }
    /// Is the SoundFont playing (rather than the built-in sound)?
    #[allow(dead_code)]
    pub fn using_soundfont(&self) -> bool {
        self.state.lock().map(|s| s.gm.is_some()).unwrap_or(false)
    }
    /// Sound one note right away (auditioning).
    pub fn audition(&self, midi: i32, program: u8, vel: u8) {
        if let Ok(mut s) = self.state.lock() {
            s.pending.push(PlayNote { at: 0.0, len: 0.45, midi, vel: vel as f32 / 127.0, program, gain: 1.0, pan: 0.0, drum: false, ..Default::default() });
        }
    }
}

fn start_voice(n: &PlayNote) -> Voice {
    let freq = 440.0 * 2f64.powf((n.midi as f64 - 69.0) / 12.0);
    Voice { freq, phase: 0.0, phase2: 0.0, age: 0.0, len: n.len.max(0.03), vel: n.vel, program: n.program, gain: n.gain, pan: n.pan, drum: n.drum, noise: 0x12345 ^ n.midi as u32, done: false, key: None }
}

/// F-30: once the bank is read (soundfont::preload), make its synthesizer for this output's sample rate.
fn attach_soundfont(state: &Arc<Mutex<SynthState>>, sr: f64) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::soundfont::preload();
        let st = state.clone();
        std::thread::spawn(move || {
            for _ in 0..2400 {
                match crate::soundfont::bank() {
                    None => std::thread::sleep(std::time::Duration::from_millis(25)),
                    Some(Err(_)) => return,
                    Some(Ok(sf)) => {
                        match crate::soundfont::GmSynth::new(&sf, sr) {
                            Ok(g) => {
                                if let Ok(mut s) = st.lock() {
                                    s.gm_ready = Some(g);
                                }
                            }
                            Err(e) => eprintln!("SoundFont: {e}"),
                        }
                        return;
                    }
                }
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (state, sr);
}

/// Stop everything the SoundFont is sounding (release tails ring out).
fn gm_stop(s: &mut SynthState) {
    if let Some(g) = s.gm.as_mut() {
        g.all_off();
    }
    s.offs.clear();
}

/// One score note (or pedal, or click) on the SoundFont, with its note-off on the clock.
fn gm_start(g: &mut crate::soundfont::GmSynth, offs: &mut Vec<(f64, u8, i32)>, n: &PlayNote, off_at: f64) {
    use crate::soundfont::click_key;
    if let Some((cc, val)) = n.cc {
        if !n.drum {
            g.controller(n.channel, cc, val);
        }
        return;
    }
    let (ch, key, vel) = if n.click {
        // the computer's click: wood blocks on the drum kit; a click set to a channel and key plays that
        if n.port >= 2 { (9u8, click_key(n.midi >= 96), n.vel * n.gain) } else { (n.channel & 15, n.midi, n.vel * n.gain) }
    } else if n.drum {
        g.set_channel(9, 0, (n.gain * 127.0) as u8, n.pan);
        (9, n.midi, n.vel)
    } else {
        let ch = if n.channel & 15 == 9 { 0 } else { n.channel & 15 };
        g.set_channel(ch, n.program, (n.gain * 127.0) as u8, n.pan);
        (ch, n.midi, n.vel)
    };
    g.note_on(ch, key, vel);
    offs.push((off_at, ch, key));
}

/// F-30: the SoundFont's level against the built-in sound (RMS of estudo-em-sol: -38.5 dBFS against -33.5).
const GM_GAIN: f32 = 1.8;

/// F-30: the SoundFont path of `render`, in blocks of 64 frames (1.3 ms): notes start at the block where their time
/// falls, as the built-in sound's per-sample start is not audible at this size.
fn render_gm(s: &mut SynthState, data: &mut [f32], channels: usize, sr: f64) {
    use crate::soundfont::LIVE_CHANNEL;
    let Some(mut g) = s.gm.take() else { return };
    let dt = 1.0 / sr;
    let channels = channels.max(1);
    for n in std::mem::take(&mut s.pending) {
        g.set_channel(LIVE_CHANNEL, n.program, 127, 0.0);
        g.note_on(LIVE_CHANNEL, n.midi, n.vel);
        s.offs.push((s.clock + n.len, LIVE_CHANNEL, n.midi));
    }
    for (on, key, prog, vel) in std::mem::take(&mut s.live) {
        if on {
            g.set_channel(LIVE_CHANNEL, prog, 127, 0.0);
            g.note_on(LIVE_CHANNEL, key, vel as f32 / 127.0);
        } else {
            g.note_off(LIVE_CHANNEL, key);
        }
    }
    let master = s.master;
    let frames = data.len() / channels;
    let mut done = 0;
    while done < frames {
        let n = (frames - done).min(64);
        if s.playing {
            while s.next < s.notes.len() && s.notes[s.next].at <= s.time {
                let note = &s.notes[s.next];
                let off_at = s.clock + (note.at - s.time).max(0.0) + note.len.max(0.03);
                gm_start(&mut g, &mut s.offs, note, off_at);
                s.next += 1;
            }
        }
        let now = s.clock;
        s.offs.retain(|&(t, ch, k)| {
            if t <= now {
                g.note_off(ch, k);
                false
            } else {
                true
            }
        });
        let (l, r) = g.render(n);
        for i in 0..n {
            // as loud as the built-in sound (measured on the study: +5 dB), with a soft limit for full scores
            let (a, b) = ((l[i] * master * GM_GAIN).tanh(), (r[i] * master * GM_GAIN).tanh());
            let f = &mut data[(done + i) * channels..(done + i + 1) * channels];
            if channels >= 2 {
                f[0] = a;
                f[1] = b;
                for x in f.iter_mut().skip(2) {
                    *x = 0.0;
                }
            } else {
                f[0] = (a + b) * 0.5;
            }
        }
        s.clock += n as f64 * dt;
        if s.playing {
            s.time += n as f64 * dt;
            if s.next >= s.notes.len() && s.offs.is_empty() {
                s.playing = false;
            }
        }
        done += n;
    }
    s.gm = Some(g);
}

fn render(s: &mut SynthState, data: &mut [f32], channels: usize, sr: f64) {
    // F-30: the SoundFont takes over at the first silence after it is ready
    if s.gm.is_none() && s.gm_ready.is_some() && !s.playing && s.voices.is_empty() {
        s.gm = s.gm_ready.take();
    }
    if s.gm.is_some() {
        render_gm(s, data, channels, sr);
        return;
    }
    s.clock += (data.len() / channels.max(1)) as f64 / sr;
    let dt = 1.0 / sr;
    let pend: Vec<PlayNote> = s.pending.drain(..).collect();
    for n in pend {
        let v = start_voice(&n);
        s.voices.push(v);
    }
    let master = s.master;
    for frame in data.chunks_mut(channels.max(1)) {
        if s.playing {
            while s.next < s.notes.len() && s.notes[s.next].at <= s.time {
                if s.notes[s.next].cc.is_none() {
                    let v = start_voice(&s.notes[s.next]);
                    s.voices.push(v);
                }
                s.next += 1;
            }
            s.time += dt;
            if s.next >= s.notes.len() && s.voices.is_empty() {
                s.playing = false;
            }
        }
        let (mut l, mut r) = (0.0f64, 0.0f64);
        for v in s.voices.iter_mut() {
            let out = if v.drum {
                v.noise ^= v.noise << 13;
                v.noise ^= v.noise >> 17;
                v.noise ^= v.noise << 5;
                let nz = (v.noise as f64 / u32::MAX as f64) * 2.0 - 1.0;
                let env = (-v.age * 18.0).exp();
                if v.age > 0.5 {
                    v.done = true;
                }
                nz * env * 0.5 + (v.phase * std::f64::consts::TAU).sin() * env * 0.4
            } else {
                let env = envelope(v.program, v.age, v.len);
                if v.age > v.len + 0.4 {
                    v.done = true;
                }
                family_wave(v.program, v.phase, v.phase2) * env
            };
            let a = out * v.vel as f64 * v.gain as f64 * 0.18;
            let pan = (v.pan as f64 + 1.0) / 2.0;
            l += a * (1.0 - pan).sqrt();
            r += a * pan.sqrt();
            v.phase = (v.phase + v.freq * dt).fract();
            v.phase2 = (v.phase2 + v.freq * 1.003 * dt).fract();
            v.age += dt;
        }
        s.voices.retain(|v| !v.done);
        let l = (l * master as f64).tanh() as f32;
        let r = (r * master as f64).tanh() as f32;
        if channels >= 2 {
            frame[0] = l;
            frame[1] = r;
            for x in frame.iter_mut().skip(2) {
                *x = 0.0;
            }
        } else if let Some(x) = frame.first_mut() {
            *x = (l + r) * 0.5;
        }
    }
}

/// Expand repeats and endings into a play order of measures.
pub fn play_order(score: &Score) -> Vec<usize> {
    let n = score.measures.len();
    let mut order = vec![];
    let mut i = 0;
    let mut repeat_start = 0;
    let mut pass = 1u8;
    let mut guard = 0;
    while i < n && guard < 10000 {
        guard += 1;
        let mi = &score.measures[i];
        if i > 0 && matches!(score.measures[i - 1].barline, Barline::RepeatStart | Barline::RepeatBoth) && pass == 1 {
            repeat_start = i;
        }
        if i == 0 && matches!(mi.barline, Barline::RepeatStart) {
            repeat_start = 1;
        }
        // skip an ending not meant for this pass
        if let Some(e) = &mi.ending {
            let nums: Vec<u8> = e.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
            if !nums.is_empty() && !nums.contains(&pass) {
                // skip to the measure after the end of this ending (next ending or repeat end)
                let mut j = i + 1;
                while j < n && score.measures[j].ending.is_none() && !matches!(score.measures[j - 1].barline, Barline::RepeatEnd | Barline::RepeatBoth) {
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        order.push(i);
        if matches!(mi.barline, Barline::RepeatEnd | Barline::RepeatBoth) && pass < mi.repeat_times.max(2) {
            pass += 1;
            i = repeat_start;
            continue;
        }
        if matches!(mi.barline, Barline::RepeatEnd | Barline::RepeatBoth) {
            pass = 1;
            repeat_start = i + 1;
        }
        i += 1;
    }
    order
}

/// Turn the score into timed notes. `swing` 50 = straight, up to 75 (Swing Playback).
pub fn build(score: &Score, from_measure: usize) -> (Vec<PlayNote>, TimeMap, f64) {
    let order = play_order(score);
    let any_solo = score.staves.iter().any(|s| s.solo);
    let mut notes = vec![];
    let mut tm = TimeMap { measures: vec![], order: order.clone() };
    let mut t = 0.0f64;
    let mut start_at = 0.0;
    let swing = score.playback.swing.clamp(50, 80) as f64 / 100.0;
    let mut started = false;
    for &m in &order {
        if m == from_measure && !started {
            start_at = t;
            started = true;
        }
        let mi = &score.measures[m];
        let tempo = score.tempo_at(m).max(10) as f64;
        let spt = 60.0 / tempo / TPQ as f64;
        let ticks = mi.ticks();
        tm.measures.push((t, spt, ticks));
        let swing_time = |tick: u32| -> f64 {
            if (swing - 0.5).abs() < 0.001 {
                return tick as f64 * spt;
            }
            // Swing Playback: pairs of the chosen note type (1 = whole .. 6 = 32nd); each pair spans two of them.
            let pair = (base_ticks(score.playback.swing_note.clamp(1, 6)) * 2) as f64;
            let k = (tick as f64 / pair).floor();
            let f = (tick as f64 - k * pair) / pair;
            let f2 = if f <= 0.5 { f / 0.5 * swing } else { swing + (f - 0.5) / 0.5 * (1.0 - swing) };
            (k + f2) * pair * spt
        };
        for (si, staff) in score.staves.iter().enumerate() {
            if staff.muted || (any_solo && !staff.solo) {
                continue;
            }
            let gain = staff.volume as f32 / 127.0;
            for e in &score.events[si][m] {
                // sustain pedal marks become controller 64
                for (mk, val) in [(Mark::PedalDown, 127u8), (Mark::PedalUp, 0u8)] {
                    if e.marks.contains(&mk) {
                        notes.push(PlayNote { at: t + swing_time(e.start), channel: staff.channel.min(15), port: staff.port, cc: Some((64, val)), ..Default::default() });
                    }
                }
                if e.is_rest() {
                    continue;
                }
                // skip notes reached by a tie
                for (k, p) in e.pitches.iter().enumerate() {
                    if tied_into(score, si, m, e, p) {
                        continue;
                    }
                    let mut len_ticks = e.play_len.unwrap_or(e.ticks()) as f64;
                    if e.marks.contains(&Mark::Staccato) {
                        len_ticks *= 0.5;
                    } else if e.marks.contains(&Mark::Staccatissimo) {
                        len_ticks *= 0.3;
                    } else if !e.marks.contains(&Mark::Tenuto) {
                        len_ticks *= 0.92;
                    }
                    // extend through ties
                    if e.ties.get(k).copied().unwrap_or(false) {
                        len_ticks = e.ticks() as f64 + tie_extension(score, si, m, e, p) as f64;
                    }
                    let mut vel = e.velocity as f32 / 127.0;
                    if e.marks.contains(&Mark::Accent) || e.marks.contains(&Mark::Marcato) {
                        vel = (vel * 1.3).min(1.0);
                    }
                    let start = if e.grace { e.start.saturating_sub(TPQ / 8) } else { e.start };
                    let len = if e.grace { TPQ as f64 / 8.0 } else { len_ticks };
                    let mut at = t + swing_time(start);
                    if e.marks.contains(&Mark::Arpeggio) {
                        at += k as f64 * 0.03;
                    }
                    let midi = p.midi() + staff.transpose as i32 + clef_octave(staff.clef_at(m));
                    let fermata = e.marks.contains(&Mark::Fermata) || e.marks.contains(&Mark::FermataBelow);
                    notes.push(PlayNote {
                        at,
                        len: len * spt * if fermata { 2.0 } else { 1.0 },
                        midi,
                        vel,
                        program: staff.program,
                        gain,
                        pan: staff.pan as f32 / 64.0,
                        drum: staff.drum || staff.channel == 9,
                        channel: if staff.drum { 9 } else { staff.channel.min(15) },
                        port: staff.port,
                        click: false,
                        cc: None,
                    });
                }
            }
        }
        if score.playback.click {
            let beat = TPQ * 4 / mi.den.max(1) as u32;
            let mut b = 0;
            while b < ticks {
                notes.push(PlayNote { at: t + b as f64 * spt, len: 0.03, midi: if b == 0 { 96 } else { 84 }, vel: 0.5, program: 115, gain: 0.6, pan: 0.0, drum: true, channel: 9, click: true, ..Default::default() });
                b += beat;
            }
        }
        t += spt * ticks as f64;
    }
    if !started {
        start_at = 0.0;
    }
    (notes, tm, start_at)
}

pub fn clef_octave(c: Clef) -> i32 {
    // Octave clefs are written at the clef's pitch; the 8va/8vb sign changes sounding octave.
    match c {
        Clef::Treble8va | Clef::Treble8vb | Clef::Bass8vb => 0,
        _ => 0,
    }
}

/// Is this pitch of `e` reached by a tie from the previous note of its voice (also across the bar line)?
pub(crate) fn tied_into(score: &Score, s: usize, m: usize, e: &Event, p: &Pitch) -> bool {
    let prev = score.events[s][m].iter().filter(|x| x.voice == e.voice && x.start < e.start && !x.grace).max_by_key(|x| x.start);
    let prev = match prev {
        Some(x) => Some(x),
        None if m > 0 && e.start == 0 => score.events[s][m - 1].iter().filter(|x| x.voice == e.voice && !x.grace).max_by_key(|x| x.start),
        None => None,
    };
    if let Some(pe) = prev {
        return pe.pitches.iter().zip(pe.ties.iter()).any(|(q, t)| *t && q.midi() == p.midi());
    }
    false
}

/// The written length of the notes this pitch of `e` is tied on to (a chain of ties, across bar lines).
pub(crate) fn tie_extension(score: &Score, s: usize, m: usize, e: &Event, p: &Pitch) -> u32 {
    let mut total = 0;
    let mut cur_m = m;
    let mut cur_start = e.start;
    for _ in 0..64 {
        let next = score.events[s][cur_m].iter().filter(|x| x.voice == e.voice && x.start > cur_start && !x.grace).min_by_key(|x| x.start);
        let next = match next {
            Some(n) => Some(n),
            None if cur_m + 1 < score.measures.len() => {
                cur_m += 1;
                score.events[s][cur_m].iter().filter(|x| x.voice == e.voice && !x.grace).min_by_key(|x| x.start)
            }
            None => None,
        };
        match next {
            Some(n) => {
                if let Some(k) = n.pitches.iter().position(|q| q.midi() == p.midi()) {
                    total += n.ticks();
                    cur_start = n.start;
                    if !n.ties.get(k).copied().unwrap_or(false) {
                        break;
                    }
                } else {
                    break;
                }
            }
            None => break,
        }
    }
    total
}

/// Render notes without a sound device (development: `--render-wav`, and tests): interleaved stereo at `sr`, with
/// the SoundFont when `gm` is given, otherwise the built-in sound. Stops `tail` seconds after the last note ends.
pub fn render_offline(notes: Vec<PlayNote>, sr: f64, master: f32, gm: Option<crate::soundfont::GmSynth>, tail: f64) -> Vec<f32> {
    let mut notes = notes;
    notes.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap());
    let mut s = SynthState { master, gm, notes, playing: true, ..Default::default() };
    let mut out = vec![];
    let mut buf = vec![0f32; 512 * 2];
    let mut after = 0.0;
    while after < tail {
        render(&mut s, &mut buf, 2, sr);
        out.extend_from_slice(&buf);
        if !s.playing {
            after += 512.0 / sr;
        }
        if out.len() as f64 > sr * 2.0 * 1200.0 {
            break;
        }
    }
    out
}
