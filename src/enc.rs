//! Reader for the original program's score files (.enc, "SCOW" little-endian Windows / "SCO5" big-endian Mac).
//!
//! Layout worked out from the shipped example files and Encore.exe (see MECHANICS.md):
//! * file header: magic at 0, ticks per quarter (240) at 0x2c, number of tracks at 0x32, number of shown staves
//!   at 0x33 (a split/grand staff shows one track on two staves);
//! * "TITL" chunk: UTF-16 strings, the first is the title, the second the subtitle;
//! * "MEAS" chunk per measure: 62-byte header (+8 tempo in beats per minute, +12 beat length, +14 measure length,
//!   +16 time-signature numerator, +17 denominator), then events `[tick u16][kind u8][size u8][body]` until FF FF;
//! * event kind high nibble 9 = note, 8 = rest, 3 = tie/slur, 6 = text; low nibble = voice − 1;
//! * note/rest body: byte 0 = track (bit 0x40 = lower staff of a split pair), byte 1 low nibble = note value
//!   (1 whole … 7 64th), byte 6–7 = x position, byte 8 = staff position (steps above two steps below the bottom
//!   line), byte 10 = dot flags (0x18 set = dotted), byte 11 = MIDI pitch, bytes 12–13 = played length,
//!   byte 14 = velocity.
use crate::model::*;
use std::collections::{BTreeMap, HashMap};

struct Rd<'a> {
    d: &'a [u8],
    be: bool,
}

impl<'a> Rd<'a> {
    fn u16(&self, o: usize) -> u16 {
        if o + 2 > self.d.len() {
            return 0;
        }
        let b = [self.d[o], self.d[o + 1]];
        if self.be { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) }
    }
    fn u32(&self, o: usize) -> u32 {
        if o + 4 > self.d.len() {
            return 0;
        }
        let b = [self.d[o], self.d[o + 1], self.d[o + 2], self.d[o + 3]];
        if self.be { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }
    }
    fn u8(&self, o: usize) -> u8 {
        *self.d.get(o).unwrap_or(&0)
    }
}

struct RawEv {
    tick: u32,
    kind: u8,
    body: Vec<u8>,
}

struct RawMeas {
    tempo: u16,
    beat: u16,
    num: u8,
    den: u8,
    evs: Vec<RawEv>,
}

fn find_all(d: &[u8], pat: &[u8]) -> Vec<usize> {
    let mut v = vec![];
    let mut i = 0;
    while i + pat.len() <= d.len() {
        if &d[i..i + pat.len()] == pat {
            v.push(i);
        }
        i += 1;
    }
    v
}

fn utf16_strings(d: &[u8], be: bool) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut i = 0;
    while i + 1 < d.len() {
        let c = if be { u16::from_be_bytes([d[i], d[i + 1]]) } else { u16::from_le_bytes([d[i], d[i + 1]]) };
        if (0x20..0x7f).contains(&c) || (0xa0..0x250).contains(&c) {
            cur.push(char::from_u32(c as u32).unwrap_or('?'));
        } else {
            if cur.trim().chars().count() >= 2 {
                out.push(cur.trim().to_string());
            }
            cur.clear();
        }
        i += 2;
    }
    if cur.trim().chars().count() >= 2 {
        out.push(cur.trim().to_string());
    }
    out
}

/// The staff position byte counts diatonic steps from two steps below the bottom line.
fn clef_base(c: Clef) -> i32 {
    match c {
        Clef::Treble | Clef::Percussion => 4 * 7,     // C4 (bottom line E4)
        Clef::Treble8va => 5 * 7,                     // C5
        Clef::Treble8vb => 3 * 7,                     // C3
        Clef::Bass => 2 * 7 + 2,                      // E2 (bottom line G2)
        Clef::Bass8vb => 7 + 2,                       // E1
        Clef::Alto => 3 * 7 + 1,                      // D3 (bottom line F3)
        Clef::Tenor => 2 * 7 + 6,                     // B2 (bottom line D3)
    }
}

fn natural_midi(step: i32) -> i32 {
    Pitch { step, alter: 0 }.midi()
}

pub fn import(bytes: &[u8]) -> Result<Score, String> {
    let be = match bytes.get(0..4) {
        Some(b"SCOW") => false,
        Some(b"SCO5") => true,
        _ => return Err("This isn't a score from the original program (no SCOW/SCO5 header).".into()),
    };
    let r = Rd { d: bytes, be };
    let tpq_file = r.u16(0x2c).max(1) as u32;
    let tracks = r.u8(0x32).max(1) as usize;

    // measures
    let mut meas: Vec<RawMeas> = vec![];
    for o in find_all(bytes, b"MEAS") {
        if o + 0x40 > bytes.len() {
            continue;
        }
        let num = r.u8(o + 16);
        let den = r.u8(o + 17);
        let mticks = r.u16(o + 14) as u32;
        if num == 0 || num > 32 || ![1u8, 2, 4, 8, 16, 32].contains(&den) {
            continue;
        }
        if mticks != tpq_file * 4 * num as u32 / den as u32 {
            continue;
        }
        let mut p = o + 0x3e;
        let mut evs = vec![];
        let mut ok = false;
        while p + 4 <= bytes.len() {
            if bytes[p] == 0xff && bytes[p + 1] == 0xff {
                ok = true;
                break;
            }
            let tick = r.u16(p) as u32;
            let kind = bytes[p + 2];
            let size = bytes[p + 3] as usize;
            if size < 4 || p + size > bytes.len() {
                break;
            }
            evs.push(RawEv { tick, kind, body: bytes[p + 4..p + size].to_vec() });
            p += size;
        }
        if ok {
            meas.push(RawMeas { tempo: r.u16(o + 8), beat: r.u16(o + 12), num, den, evs });
        }
    }
    if meas.is_empty() {
        return Err("No measures could be read from this file.".into());
    }

    // which tracks are split (grand staff): a lower-half flag on any note or rest
    let mut split = vec![false; tracks.max(64)];
    let mut max_track = 0usize;
    for m in &meas {
        for e in &m.evs {
            if matches!(e.kind >> 4, 8 | 9) && !e.body.is_empty() {
                let t = (e.body[0] & 0x3f) as usize;
                max_track = max_track.max(t);
                if e.body[0] & 0x40 != 0 && t < split.len() {
                    split[t] = true;
                }
            }
        }
    }
    let tracks = tracks.max(max_track + 1);
    let mut staff_of: HashMap<(usize, bool), usize> = HashMap::new();
    let mut staves = vec![];
    for t in 0..tracks {
        staff_of.insert((t, false), staves.len());
        let mut st = Staff::new(&format!("Staff {}", staves.len() + 1), Clef::Treble, 0);
        st.abbrev = format!("St. {}", staves.len() + 1);
        st.channel = (t % 16) as u8;
        if split[t] {
            st.brace_with_next = true;
            st.barline_to_next = true;
            st.name = if tracks == 1 { "Piano".into() } else { format!("Piano {}", t + 1) };
            st.abbrev = "Pno.".into();
            staves.push(st);
            staff_of.insert((t, true), staves.len());
            let mut lo = Staff::new("", Clef::Bass, 0);
            lo.abbrev = String::new();
            lo.channel = (t % 16) as u8;
            staves.push(lo);
        } else {
            staves.push(st);
        }
    }

    // clef per staff: the clef whose staff positions best match the stored MIDI pitches
    let ns = staves.len();
    let mut samples: Vec<Vec<(i32, i32)>> = vec![vec![]; ns];
    for m in &meas {
        for e in &m.evs {
            if e.kind >> 4 == 9 && e.body.len() >= 12 {
                let s = staff_of[&(((e.body[0] & 0x3f) as usize).min(tracks - 1), e.body[0] & 0x40 != 0)];
                samples[s].push((e.body[8] as i8 as i32, e.body[11] as i32));
            }
        }
    }
    for (s, smp) in samples.iter().enumerate() {
        if smp.is_empty() {
            continue;
        }
        let mut best = (i64::MAX, Clef::Treble);
        for c in [Clef::Treble, Clef::Bass, Clef::Alto, Clef::Tenor, Clef::Treble8vb, Clef::Treble8va, Clef::Bass8vb] {
            let err: i64 = smp.iter().map(|&(pos, midi)| ((midi - natural_midi(clef_base(c) + pos)).abs().min(12) as i64).pow(2)).sum();
            if err < best.0 {
                best = (err, c);
            }
        }
        staves[s].clefs = vec![(0, best.1)];
    }

    // key signature: the one that explains the most stored alterations
    let mut alters: Vec<(usize, i8)> = vec![];
    for m in &meas {
        for e in &m.evs {
            if e.kind >> 4 == 9 && e.body.len() >= 12 {
                let s = staff_of[&(((e.body[0] & 0x3f) as usize).min(tracks - 1), e.body[0] & 0x40 != 0)];
                let step = clef_base(staves[s].clef_at(0)) + e.body[8] as i8 as i32;
                let alter = (e.body[11] as i32 - natural_midi(step)).clamp(-2, 2) as i8;
                alters.push((step.rem_euclid(7) as usize, alter));
            }
        }
    }
    let mut key = 0i8;
    let mut best = -1i64;
    for k in -7..=7i8 {
        let score: i64 = alters.iter().map(|&(l, a)| if key_alter(k, l) == a { 1 } else { 0 }).sum::<i64>() * 4 - (k.unsigned_abs() as i64);
        if score > best {
            best = score;
            key = k;
        }
    }

    let mut sc = Score::new(staves, meas.len());
    let conv = |t: u32| t * TPQ / tpq_file;
    for (i, m) in meas.iter().enumerate() {
        let mi = &mut sc.measures[i];
        mi.num = m.num;
        mi.den = m.den;
        mi.key = key;
        mi.tempo = None;
        if m.tempo > 0 {
            // tempo counts the measure's beat; convert to quarter notes per minute
            let q = (m.tempo as u32 * m.beat.max(1) as u32 / tpq_file).max(1);
            let prev = if i == 0 { 0 } else { sc.tempo_at(i - 1) };
            if i == 0 || q != prev {
                sc.measures[i].tempo = Some(q);
            }
            // a short pickup bar counts a smaller beat; it shares the next bar's tempo
            if i == 1 && meas[0].beat != m.beat && meas[0].tempo == m.tempo {
                sc.measures[0].tempo = Some(q);
                sc.measures[1].tempo = None;
            }
        }
    }
    if sc.measures[0].tempo.is_none() {
        sc.measures[0].tempo = Some(120);
    }
    let last = sc.measures.len() - 1;
    sc.measures[last].barline = Barline::Final;

    // notes, rests, ties, text
    let mut ties: Vec<(usize, usize, u32, u8, i32)> = vec![]; // staff, measure, tick, voice, pos
    let mut texts: Vec<(usize, usize, u32, String)> = vec![];
    let mut chords: Vec<(usize, usize, u32, String)> = vec![];
    for (mi, m) in meas.iter().enumerate() {
        let mut by_start: BTreeMap<(usize, u8, u32), Event> = BTreeMap::new();
        for e in &m.evs {
            let hi = e.kind >> 4;
            let voice = (e.kind & 0x0f) + 1;
            if e.body.is_empty() {
                continue;
            }
            let track = ((e.body[0] & 0x3f) as usize).min(tracks - 1);
            let s = staff_of[&(track, e.body[0] & 0x40 != 0)];
            match hi {
                9 | 8 if e.body.len() >= 12 => {
                    let code = (e.body[1] & 0x0f) as i32;
                    let value = if code == 0 { 0 } else { code.clamp(1, 8) as u8 };
                    let dots = if e.body[10] & 0x18 == 0x18 { 1 } else { 0 };
                    let tick = conv(e.tick);
                    // byte 9 holds a tuplet as two digits: 0x32 = 3 in the time of 2, 0x54 = 5 in the time of 4
                    let tb = e.body[9];
                    let tuplet = if tb >= 0x21 && (tb >> 4) >= 2 && (tb & 0x0f) >= 1 && (tb >> 4) != (tb & 0x0f) { Some((tb >> 4, tb & 0x0f)) } else { None };
                    let ent = by_start.entry((s, voice, tick)).or_insert_with(|| {
                        let mut ev = Event::new(voice, tick, value, vec![]);
                        ev.dots = dots;
                        ev.tuplet = tuplet;
                        ev
                    });
                    if hi == 9 {
                        let step = clef_base(sc.staves[s].clef_at(0)) + e.body[8] as i8 as i32;
                        let midi = e.body[11] as i32;
                        let alter = (midi - natural_midi(step)).clamp(-2, 2) as i8;
                        let p = Pitch { step, alter };
                        if !ent.pitches.contains(&p) {
                            ent.pitches.push(p);
                        }
                        if e.body.len() >= 15 && e.body[14] > 0 {
                            ent.velocity = e.body[14].min(127);
                        }
                        if e.body.len() >= 14 {
                            let played = u16::from_le_bytes([e.body[12], e.body[13]]) as u32;
                            let played = if be { u16::from_be_bytes([e.body[12], e.body[13]]) as u32 } else { played };
                            if played > 0 {
                                ent.play_len = Some(conv(played));
                            }
                        }
                    }
                }
                3 if e.body.len() >= 12 => {
                    // tie when it starts and ends on the same staff position
                    if e.body[10] == e.body[11] {
                        ties.push((s, mi, conv(e.tick), voice, e.body[10] as i8 as i32));
                    }
                }
                // chord symbol: [1] type (index into the original's chord list), [2] bit 0x02 = has a bass note,
                // [8] root, [9] bass (letter 0..6 = C..B, 0x20 = flat, 0x10 = sharp)
                7 if e.body.len() >= 10 => {
                    let mut name = format!("{}{}", root_name(e.body[8]), CHORD_TYPES.get(e.body[1] as usize).copied().unwrap_or(""));
                    if e.body[2] & 0x02 != 0 && e.body[9] != e.body[8] {
                        name.push('/');
                        name.push_str(&root_name(e.body[9]));
                    }
                    chords.push((s, mi, conv(e.tick), name));
                }
                6 if e.body.len() > 16 => {
                    let t = utf16_strings(&e.body[12..], be).into_iter().next();
                    if let Some(t) = t {
                        texts.push((s, mi, conv(e.tick), t));
                    }
                }
                _ => {}
            }
        }
        for ((s, _, _), mut ev) in by_start {
            ev.sort_pitches();
            ev.normalize();
            // keep playback length only when it differs from the usual 80 % of the written length
            if let Some(pl) = ev.play_len {
                let written = ev.ticks();
                if (pl as i64 - (written as i64 * 8 / 10)).abs() <= (written as i64 / 20).max(2) {
                    ev.play_len = None;
                }
            }
            sc.events[s][mi].push(ev);
        }
    }
    // rests: the original's dot flags differ on rests; a rest is dotted only when the next event in its voice
    // starts one and a half of its value later
    for s in 0..sc.staves.len() {
        for m in 0..sc.measures.len() {
            let mt = sc.measures[m].ticks();
            let starts: Vec<(u8, u32)> = sc.events[s][m].iter().map(|e| (e.voice, e.start)).collect();
            for e in sc.events[s][m].iter_mut().filter(|e| e.is_rest() && e.tuplet.is_none()) {
                let next = starts.iter().filter(|(v, st)| *v == e.voice && *st > e.start).map(|x| x.1).min().unwrap_or(mt);
                let base = base_ticks(e.value);
                e.dots = if next - e.start == base + base / 2 { 1 } else { 0 };
            }
        }
    }
    for (s, m, tick, voice, pos) in ties {
        let base = clef_base(sc.staves[s].clef_at(0));
        if let Some(e) = sc.events[s][m].iter_mut().find(|e| e.start == tick && (e.voice == voice || true) && !e.is_rest()) {
            for (k, p) in e.pitches.iter().enumerate() {
                if p.step == base + pos {
                    e.ties[k] = true;
                }
            }
        }
    }
    // chord symbols go on the note at (or just before) their position, otherwise as text above the staff
    for (s, m, tick, name) in chords {
        let target = sc.events[s][m].iter_mut().filter(|e| e.start <= tick + TPQ / 8).min_by_key(|e| (tick as i64 - e.start as i64).abs());
        match target {
            Some(e) if e.chord_symbol.is_none() && (e.start as i64 - tick as i64).abs() <= (TPQ / 4) as i64 => e.chord_symbol = Some(name),
            _ => sc.graphics.push(Graphic {
                kind: GraphicKind::Text,
                staff: s,
                measure: m,
                tick,
                end_measure: m,
                end_tick: tick,
                dx: 0.0,
                dy: -3.0,
                end_dy: -3.0,
                text: name,
                color: BLACK,
                flip: false,
                size: 1.8,
                bold: false,
                italic: false,
            }),
        }
    }
    for (s, m, tick, t) in texts {
        if let Some(e) = sc.events[s][m].iter_mut().find(|e| e.start == tick && !e.is_rest()) {
            if e.lyric.is_empty() {
                e.lyric.push(t);
                continue;
            }
        }
        sc.graphics.push(Graphic {
            kind: GraphicKind::Text,
            staff: s,
            measure: m,
            tick,
            end_measure: m,
            end_tick: tick,
            dx: 0.0,
            dy: -3.0,
            end_dy: -3.0,
            text: t,
            color: BLACK,
            flip: false,
            size: 1.5,
            bold: false,
            italic: false,
        });
    }

    // Text elements: the TITL chunk holds fixed slots of 0x420 bytes, the text of slot k starting at
    // TITL + 0x28 + k*0x420 (UTF-16, zero-ended). Slot 0 title, 1-2 subtitles, 3 instructions, 6 composer,
    // 7-9 further credit lines, 10 the header ("#P" = page number).
    if let Some(&t) = find_all(bytes, b"TITL").first() {
        let len = r.u32(t + 4) as usize;
        let end = (t + 8 + len).min(bytes.len());
        let slot = |k: usize| -> String {
            let start = t + 0x28 + k * 0x420;
            if start + 2 > end {
                return String::new();
            }
            let mut out = String::new();
            let mut i = start;
            while i + 1 < end && i < start + 0x400 {
                let c = if be { u16::from_be_bytes([bytes[i], bytes[i + 1]]) } else { u16::from_le_bytes([bytes[i], bytes[i + 1]]) };
                if c == 0 {
                    break;
                }
                out.push(char::from_u32(c as u32).unwrap_or('?'));
                i += 2;
            }
            out.trim().to_string()
        };
        let title = slot(0);
        if !title.is_empty() {
            sc.text.title = title.trim_end_matches(|c: char| c == '-' || c.is_whitespace()).to_string();
        }
        let subs: Vec<String> = [slot(1), slot(2)].into_iter().filter(|x| !x.is_empty()).collect();
        sc.text.subtitle = subs.join("\n");
        sc.text.instructions = slot(3);
        let credits: Vec<String> = [slot(6), slot(8), slot(9)].into_iter().filter(|x| !x.is_empty()).collect();
        sc.text.composer = credits.join("\n");
        sc.text.lyricist = slot(7);
        let header = slot(10);
        sc.text.show_page_numbers = header.contains("#P");
        sc.text.header = header;
    }
    // a sensible page layout for what was read
    sc.layout.measures_per_system = 4;
    sc.layout.systems_per_page = if sc.staves.len() > 4 { 2 } else { 5 };
    if sc.staves.len() > 3 {
        sc.layout.staff_space = 5.0;
    }
    if sc.staves.len() > 8 {
        sc.layout.staff_space = 4.0;
        sc.layout.systems_per_page = 1;
    }
    Ok(sc)
}

/// The original's chord types in its own order (string table 10010..10073 of Encore.exe; 0 = major, no text).
const CHORD_TYPES: [&str; 64] = [
    "", "m", "+", "dim", "dim7", "5", "6", "6/9", "(add2)", "(add9)", "(omit3)", "(omit5)", "maj7", "maj7(b5)", "maj7(6/9)", "maj7(+5)",
    "maj7(#11)", "maj9", "maj9(b5)", "maj9(+5)", "maj9(#11)", "maj13", "maj13(b5)", "maj13(#11)", "7", "7(b5)", "7(b9)", "7(#9)", "7(#11)",
    "7(b5,b9)", "7(b5,#9)", "7(b9,#9)", "9", "9(b5)", "9(#11)", "11", "13", "13(b5)", "13(b9)", "13(#9)", "13(#11)", "+7", "+7(b9)", "+7(#9)",
    "+9", "sus2", "sus2, sus4", "sus4", "7sus4", "9sus4", "13sus4", "m(add2)", "m(add9)", "m6", "m6/9", "m7", "m(maj7)", "m7(b5)", "m7(add4)",
    "m7(add11)", "m9", "m9(maj7)", "m11", "m13",
];

fn root_name(code: u8) -> String {
    let letter = ['C', 'D', 'E', 'F', 'G', 'A', 'B'][(code & 0x07).min(6) as usize];
    let acc = if code & 0x20 != 0 { "b" } else if code & 0x10 != 0 { "#" } else { "" };
    format!("{letter}{acc}")
}

// ---------------------------------------------------------------------------------------------------------------
// Writing: a score is saved in the original's format by taking an existing .enc as the model (the file the score
// was opened from, or one the person picks) and replacing its pages, lines, measures and text slots. Everything the
// rebuild doesn't model (instrument set-up, fonts, colours, print settings) is carried over from the model file.
// File layout (see MECHANICS.md): header (0xC2 bytes; 0x2E line count, 0x30 page count, 0x32 tracks, 0x34 measure
// count) | one 0x86E-byte block per track | PAGE chunks | LINE chunks | MEAS chunks | the rest (PREC, TITL, ...).

const FILE_TPQ_ENC: u32 = 240;

struct Chunk {
    off: usize,
    end: usize,
}

struct Model<'a> {
    d: &'a [u8],
    tracks_end: usize,
    pages: Vec<Chunk>,
    lines: Vec<Chunk>,
    meas: Vec<Chunk>,
    tail: usize,
}

fn rd32(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

fn parse_model(d: &[u8]) -> Result<Model<'_>, String> {
    if d.get(0..4) != Some(b"SCOW") {
        return Err("The model file must be a Windows score file of the original (it starts with SCOW).".into());
    }
    let first = find_all(&d[0xc2.min(d.len())..], b"PAGE").first().map(|x| x + 0xc2).ok_or("The model file has no pages.")?;
    let mut p = first;
    let (mut pages, mut lines, mut meas) = (vec![], vec![], vec![]);
    while p + 8 <= d.len() {
        let tag = &d[p..p + 4];
        let n = rd32(d, p + 4);
        let end = match tag {
            b"PAGE" | b"LINE" => p + 8 + n,
            b"MEAS" => p + 0x3e + n,
            _ => break,
        };
        if end > d.len() {
            return Err("The model file is cut short.".into());
        }
        let c = Chunk { off: p, end };
        match tag {
            b"PAGE" => pages.push(c),
            b"LINE" => lines.push(c),
            _ => meas.push(c),
        }
        p = end;
    }
    if pages.is_empty() || lines.is_empty() || meas.is_empty() {
        return Err("The model file has no pages, lines or measures that could be read.".into());
    }
    Ok(Model { d, tracks_end: first, pages, lines, meas, tail: p })
}

fn put16(b: &mut [u8], o: usize, v: u32) {
    let v = v.min(0xffff) as u16;
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Chord name → (root code, type index, bass code).
fn chord_code(name: &str) -> Option<(u8, u8, Option<u8>)> {
    fn root(s: &str) -> Option<(u8, usize)> {
        let mut ch = s.chars();
        let l = ch.next()?.to_ascii_uppercase();
        let letter = "CDEFGAB".find(l)? as u8;
        let mut code = letter;
        let mut used = 1;
        match ch.next() {
            Some('b') => {
                code |= 0x20;
                used += 1;
            }
            Some('#') => {
                code |= 0x10;
                used += 1;
            }
            _ => {}
        }
        Some((code, used))
    }
    let (main, bass) = match name.rfind('/') {
        // "6/9" is a chord type, not a bass note
        Some(i) if root(&name[i + 1..]).map(|(_, u)| u == name.len() - i - 1).unwrap_or(false) => (&name[..i], Some(root(&name[i + 1..])?.0)),
        _ => (name, None),
    };
    let (r, used) = root(main)?;
    let kind = main[used..].trim();
    let ty = CHORD_TYPES.iter().position(|t| *t == kind).or_else(|| {
        // closest known type: the longest one the name starts with
        CHORD_TYPES.iter().enumerate().filter(|(_, t)| !t.is_empty() && kind.starts_with(*t)).max_by_key(|(_, t)| t.len()).map(|(i, _)| i)
    });
    Some((r, ty.unwrap_or(0) as u8, bass))
}

fn utf16_slot(b: &mut [u8], at: usize, text: &str) {
    let end = (at + 0x400).min(b.len());
    if at >= end {
        return;
    }
    for x in &mut b[at..end] {
        *x = 0;
    }
    let mut i = at;
    for c in text.encode_utf16() {
        if i + 4 > end {
            break;
        }
        b[i..i + 2].copy_from_slice(&c.to_le_bytes());
        i += 2;
    }
}

/// Save `score` in the original's format, using `model` (an .enc file) for everything the rebuild doesn't keep.
pub fn export(score: &Score, model: &[u8]) -> Result<Vec<u8>, String> {
    let md = parse_model(model)?;
    let d = md.d;
    let model_tracks = d[0x32].max(1) as usize;

    // tracks: a braced pair of staves is one track shown on two staves (the lower one flagged 0x40)
    let mut track_of: Vec<(usize, bool)> = vec![(0, false); score.staves.len()];
    let mut tracks = 0usize;
    let mut s = 0;
    while s < score.staves.len() {
        track_of[s] = (tracks, false);
        if score.staves[s].brace_with_next && s + 1 < score.staves.len() {
            track_of[s + 1] = (tracks, true);
            s += 1;
        }
        tracks += 1;
        s += 1;
    }
    if tracks > model_tracks {
        return Err(format!(
            "The model file has {model_tracks} instrument(s) but this score has {tracks}. Pick a model .enc with at least {tracks} instrument(s) (a piano grand staff counts as one)."
        ));
    }

    // page layout as the rebuild shows it
    let geo = crate::score_view::layout(score, egui::Pos2::ZERO, 1.0, false);
    let systems: Vec<&crate::score_view::SysGeo> = geo.systems.iter().filter(|s| !s.measures.is_empty()).collect();
    if systems.is_empty() {
        return Err("The score has no measures to save.".into());
    }

    // the model's line shapes: per slot on its first page, and its line width in its own units
    let line_body = |c: &Chunk| d[c.off + 8..c.end].to_vec();
    let model_lines: Vec<Vec<u8>> = md.lines.iter().map(line_body).collect();
    let first_page_lines: Vec<&Vec<u8>> = model_lines.iter().filter(|b| b.len() >= 26 && u16::from_le_bytes([b[0], b[1]]) == 0).collect();
    let slots: Vec<&Vec<u8>> = if first_page_lines.is_empty() { model_lines.iter().collect() } else { first_page_lines };
    let staff_entries = (slots[0].len().saturating_sub(26)) / 30;
    let meas_head = |c: &Chunk| d[c.off..c.off + 0x3e].to_vec();
    let mh: Vec<Vec<u8>> = md.meas.iter().map(meas_head).collect();
    // width of a full line: the end of the model's first line
    let m0n = u16::from_le_bytes([slots[0][12], slots[0][13]]).max(1) as usize;
    let line_end = {
        let h = &mh[(m0n - 1).min(mh.len() - 1)];
        (rd32(h, 28) + rd32(h, 24)).max(300) as f32
    };
    let first_x = rd32(&mh[0], 28) as f32;
    let first_lead = rd32(&mh[0], 24).saturating_sub(rd32(&mh[0], 40)) as f32;

    // ---- pages and lines
    let n_pages = systems.iter().map(|s| s.page).max().unwrap_or(0) + 1;
    let mut pages_out = vec![];
    let mut lines_out = vec![];
    // where each measure sits: (x start, width, content width) in the model's units
    let mut mpos: Vec<(u32, u32, u32)> = vec![(first_x as u32, 132, 120); score.measures.len()];
    let mut note_x: HashMap<(usize, u32), u32> = HashMap::new();
    let mut line_idx_on_page = 0usize;
    let mut prev_page = usize::MAX;
    let mut staff_running = 0usize;
    let page_tpl = d[md.pages[0].off + 8..md.pages[0].end].to_vec();
    for pg in 0..n_pages {
        let on: Vec<usize> = (0..systems.len()).filter(|&i| systems[i].page == pg).collect();
        let first_line = lines_out.len();
        let mut b = page_tpl.clone();
        if b.len() >= 14 {
            put16(&mut b, 0, first_line as u32);
            put16(&mut b, 2, on.len() as u32);
            put16(&mut b, 12, (on.len() * staff_entries) as u32);
        }
        pages_out.push(b);
        for &si in &on {
            let sys = systems[si];
            if pg != prev_page {
                line_idx_on_page = 0;
                staff_running = 0;
                prev_page = pg;
            }
            let k = line_idx_on_page.min(slots.len() - 1);
            let mut b = slots[k].clone();
            // lines past the model's page continue its spacing
            if line_idx_on_page >= slots.len() && slots.len() >= 2 {
                let a = &slots[slots.len() - 2];
                let z = &slots[slots.len() - 1];
                let step = rd32(z, 4) as i64 - rd32(a, 4) as i64;
                let extra = (line_idx_on_page - slots.len() + 1) as i64 * step;
                put32(&mut b, 4, (rd32(z, 4) as i64 + extra).max(0) as u32);
                for e in 0..staff_entries {
                    for f in [14usize, 18, 20] {
                        let o = 26 + e * 30 + f;
                        let v = u16::from_le_bytes([b[o], b[o + 1]]) as i64 + extra;
                        put16(&mut b, o, v.max(0) as u32);
                    }
                }
            }
            let m_first = sys.measures[0].m;
            put16(&mut b, 0, pg as u32);
            put16(&mut b, 10, m_first as u32);
            put16(&mut b, 12, sys.measures.len() as u32);
            put16(&mut b, 16, if line_idx_on_page == 0 { 1 } else { 0 });
            for e in 0..staff_entries {
                b[26 + e * 30 + 3] = (staff_running + e) as u8;
            }
            staff_running += staff_entries;
            lines_out.push(b);
            line_idx_on_page += 1;

            // measures across the line, in the model's units
            let (l, r) = (sys.left, sys.right.max(sys.left + 1.0));
            let scale = (line_end - first_x) / (r - l);
            for mg in &sys.measures {
                let x0 = first_x + (mg.x0 - l) * scale;
                let x1 = first_x + (mg.x1 - l) * scale;
                let cx = first_x + (mg.content_x - l) * scale;
                let lead = if mg.m == 0 { first_lead.max(cx - x0) } else { cx - x0 };
                mpos[mg.m] = (x0.round() as u32, (x1 - x0).round().max(10.0) as u32, (x1 - x0 - lead).round().max(10.0) as u32);
                for &(tick, x) in &mg.slots {
                    note_x.insert((mg.m, tick), ((x - mg.x0) * scale).round().max(0.0) as u32);
                }
            }
        }
    }

    // ---- measures
    let mut meas_out: Vec<u8> = vec![];
    for (mi, info) in score.measures.iter().enumerate() {
        let mut h = if mi < mh.len() { mh[mi].clone() } else { mh[if mh.len() > 1 { 1 } else { 0 }].clone() };
        let mt_file = FILE_TPQ_ENC * 4 * info.num as u32 / info.den.max(1) as u32;
        let beat = if info.den >= 8 && info.num % 3 == 0 && info.num > 3 { FILE_TPQ_ENC * 4 * 3 / info.den as u32 } else { FILE_TPQ_ENC * 4 / info.den.max(1) as u32 };
        let q = score.tempo_at(mi).max(1);
        put16(&mut h, 8, (q * FILE_TPQ_ENC / beat.max(1)).max(1));
        put16(&mut h, 12, beat);
        put16(&mut h, 14, mt_file);
        h[16] = info.num;
        h[17] = info.den;
        let (x0, w, content) = mpos[mi];
        put32(&mut h, 24, w);
        put32(&mut h, 28, x0);
        put32(&mut h, 40, content);

        let mut ev: Vec<(u32, Vec<u8>)> = vec![];
        let conv = |t: u32| t * FILE_TPQ_ENC / TPQ;
        for s in 0..score.staves.len() {
            let (track, lower) = track_of[s];
            let tbyte = (track as u8 & 0x3f) | if lower { 0x40 } else { 0 };
            let st = &score.staves[s];
            let base = clef_base(st.clef_at(mi));
            for e in &score.events[s][mi] {
                if e.grace {
                    continue;
                }
                let tick = conv(e.start);
                let x = note_x.get(&(mi, e.start)).copied().unwrap_or(20 + tick * content / mt_file.max(1));
                let vbyte = (e.voice.clamp(1, 16) - 1) & 0x0f;
                let tup = e.tuplet.map(|(a, n)| (a.min(15) << 4) | n.min(15)).unwrap_or(0);
                let len = conv(e.ticks());
                if e.is_rest() {
                    let mut b = vec![0u8; 18];
                    put16(&mut b, 0, tick);
                    b[2] = 0x80 | vbyte;
                    b[3] = 18;
                    b[4] = tbyte;
                    b[5] = e.value.min(8);
                    put16(&mut b, 10, x);
                    b[12] = 6;
                    b[13] = tup;
                    b[14] = 0x18;
                    put16(&mut b, 16, len);
                    ev.push((tick, b));
                    continue;
                }
                let played = conv(e.play_len.unwrap_or(e.ticks() * 8 / 10)).max(1);
                for (k, p) in e.pitches.iter().enumerate() {
                    let pos = (p.step - base).clamp(-120, 120) as i8;
                    let mut b = vec![0u8; 28];
                    put16(&mut b, 0, tick);
                    b[2] = 0x90 | vbyte;
                    b[3] = 28;
                    b[4] = tbyte;
                    b[5] = e.value.min(8);
                    put16(&mut b, 10, x);
                    b[12] = pos as u8;
                    b[13] = tup;
                    b[14] = if e.dots > 0 { 0x18 } else { 0 };
                    b[15] = p.midi().clamp(0, 127) as u8;
                    put16(&mut b, 16, played);
                    b[18] = e.velocity.clamp(1, 127);
                    b[19] = 0x40;
                    // stem down from the middle line up
                    b[20] = if pos >= 6 { 0x80 } else { 0 };
                    ev.push((tick, b));
                    if e.ties.get(k).copied().unwrap_or(false) {
                        let mut t = vec![0u8; 18];
                        put16(&mut t, 0, tick);
                        t[2] = 0x30 | vbyte;
                        t[3] = 18;
                        t[4] = tbyte;
                        t[5] = if pos >= 6 { 0x02 } else { 0xfe };
                        t[6] = 0x80;
                        put16(&mut t, 10, x);
                        t[12] = 5;
                        t[14] = pos as u8;
                        t[15] = pos as u8;
                        ev.push((tick, t));
                    }
                }
                if let Some(name) = &e.chord_symbol {
                    if let Some((root, ty, bass)) = chord_code(name) {
                        let mut c = vec![0u8; 16];
                        put16(&mut c, 0, tick);
                        c[2] = 0x70;
                        c[3] = 16;
                        c[4] = tbyte;
                        c[5] = ty;
                        c[6] = if bass.is_some() { 0x02 } else { 0 };
                        put16(&mut c, 10, x);
                        c[12] = root;
                        c[13] = bass.unwrap_or(root);
                        put16(&mut c, 14, 0x43);
                        ev.push((tick, c));
                    }
                }
            }
        }
        ev.sort_by_key(|x| x.0);
        let mut body: Vec<u8> = ev.into_iter().flat_map(|x| x.1).collect();
        body.extend_from_slice(&[0xff, 0xff]);
        put32(&mut h, 4, body.len() as u32);
        meas_out.extend_from_slice(&h);
        meas_out.extend_from_slice(&body);
    }

    // ---- put the file together
    let mut out = d[..md.tracks_end].to_vec();
    put16(&mut out, 0x2e, lines_out.len() as u32);
    put16(&mut out, 0x30, pages_out.len() as u32);
    put16(&mut out, 0x34, score.measures.len() as u32);
    for b in &pages_out {
        out.extend_from_slice(b"PAGE");
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    for b in &lines_out {
        out.extend_from_slice(b"LINE");
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    out.extend_from_slice(&meas_out);
    let tail_at = out.len();
    out.extend_from_slice(&d[md.tail..]);

    // text slots (title, subtitles, instructions, credits, header)
    if let Some(t) = find_all(&out[tail_at..], b"TITL").first().map(|x| x + tail_at) {
        let t = t + 0x28;
        let t0 = &score.text;
        let subs: Vec<&str> = t0.subtitle.lines().collect();
        let comp: Vec<&str> = t0.composer.lines().collect();
        let header = if t0.show_page_numbers && !t0.header.contains("#P") { format!("{}#P", t0.header) } else { t0.header.clone() };
        let slots: [(usize, String); 9] = [
            (0, t0.title.clone()),
            (1, subs.first().copied().unwrap_or("").to_string()),
            (2, subs.get(1).copied().unwrap_or("").to_string()),
            (3, t0.instructions.clone()),
            (6, comp.first().copied().unwrap_or("").to_string()),
            (7, t0.lyricist.clone()),
            (8, comp.get(1).copied().unwrap_or("").to_string()),
            (9, comp.get(2).copied().unwrap_or("").to_string()),
            (10, header),
        ];
        for (k, txt) in slots {
            utf16_slot(&mut out, t + k * 0x420, &txt);
        }
    }
    Ok(out)
}
