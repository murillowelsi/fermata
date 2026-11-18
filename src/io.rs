//! Files: native score (JSON), Standard MIDI File export/import, MusicXML export.
use crate::model::*;
use std::path::Path;

/// The native score file (JSON).
pub fn score_bytes(score: &Score) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(score).map_err(|e| e.to_string())
}

/// Open a score from a file's name and contents.
pub fn open_bytes(name: &str, bytes: &[u8]) -> Result<Score, String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    if ext == "mid" || ext == "midi" || bytes.starts_with(b"MThd") {
        return import_midi(bytes);
    }
    if ext == "enc" || bytes.starts_with(b"SCOW") || bytes.starts_with(b"SCO5") {
        let mut sc = crate::enc::import(bytes)?;
        repair(&mut sc);
        return Ok(sc);
    }
    let s = String::from_utf8_lossy(bytes);
    let mut sc: Score = serde_json::from_str(&s).map_err(|e| format!("Not a score file: {e}"))?;
    repair(&mut sc);
    Ok(sc)
}

/// What the person reads when a file isn't a score Fermata can open (also a renamed or damaged one): the file's
/// name first, then the reason.
pub fn open_error(name: &str, reason: &str) -> String {
    let name = if name.trim().is_empty() { "This file" } else { name.trim() };
    format!("{name} isn't a score Fermata can open.\n{}", reason.trim())
}

#[allow(dead_code)]
pub fn open(path: &Path) -> Result<Score, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    open_bytes(&path.to_string_lossy(), &bytes)
}

pub fn repair(sc: &mut Score) {
    let n = sc.measures.len();
    sc.events.resize(sc.staves.len(), vec![]);
    for st in sc.events.iter_mut() {
        st.resize(n, vec![]);
        for m in st.iter_mut() {
            for e in m.iter_mut() {
                e.normalize();
            }
        }
    }
}

fn vlq(mut v: u32, out: &mut Vec<u8>) {
    let mut buf = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        buf.push(((v & 0x7f) as u8) | 0x80);
        v >>= 7;
    }
    buf.reverse();
    out.extend(buf);
}

pub fn midi_bytes(score: &Score) -> Vec<u8> {
    let order = crate::audio::play_order(score);
    let mut tracks: Vec<Vec<u8>> = vec![];
    // conductor track
    {
        let mut evs: Vec<(u32, Vec<u8>)> = vec![];
        let mut t = 0u32;
        let mut last_tempo = 0;
        let mut last_ts = (0, 0);
        for &m in &order {
            let mi = &score.measures[m];
            let tempo = score.tempo_at(m);
            if tempo != last_tempo {
                let us = 60_000_000 / tempo.max(1);
                evs.push((t, vec![0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8]));
                last_tempo = tempo;
            }
            if (mi.num, mi.den) != last_ts {
                let dd = (mi.den as f32).log2() as u8;
                evs.push((t, vec![0xff, 0x58, 4, mi.num, dd, 24, 8]));
                evs.push((t, vec![0xff, 0x59, 2, mi.key as u8, 0]));
                last_ts = (mi.num, mi.den);
            }
            t += mi.ticks();
        }
        let title = score.text.title.as_bytes();
        let mut tr = vec![0u8, 0xff, 0x03];
        vlq(title.len() as u32, &mut tr);
        tr.extend(title);
        let mut last = 0;
        for (at, d) in evs {
            let at = at * FILE_TPQ / TPQ;
            vlq(at - last, &mut tr);
            tr.extend(d);
            last = at;
        }
        tr.extend([0, 0xff, 0x2f, 0]);
        tracks.push(tr);
    }
    for (si, staff) in score.staves.iter().enumerate() {
        let ch = if staff.drum { 9 } else { (staff.channel.min(15)) as u8 };
        let mut evs: Vec<(u32, u8, Vec<u8>)> = vec![];
        let mut t = 0u32;
        for &m in &order {
            let mi = &score.measures[m];
            for e in &score.events[si][m] {
                if e.is_rest() || e.grace {
                    continue;
                }
                let len = e.play_len.unwrap_or(e.ticks()).max(1);
                for (i, p) in e.pitches.iter().enumerate() {
                    // a note reached by a tie isn't struck again: the first note of the tie holds through it
                    if crate::audio::tied_into(score, si, m, e, p) {
                        continue;
                    }
                    let len = if e.ties.get(i).copied().unwrap_or(false) { e.ticks() + crate::audio::tie_extension(score, si, m, e, p) } else { len };
                    let k = (p.midi() + staff.transpose as i32).clamp(0, 127) as u8;
                    evs.push((t + e.start, 1, vec![0x90 | ch, k, e.velocity.max(1)]));
                    evs.push((t + e.start + len, 0, vec![0x80 | ch, k, 0]));
                }
            }
            t += mi.ticks();
        }
        evs.sort_by_key(|x| (x.0, x.1));
        let mut tr = vec![];
        let name = staff.name.as_bytes();
        tr.extend([0, 0xff, 0x03]);
        vlq(name.len() as u32, &mut tr);
        tr.extend(name);
        tr.extend([0, 0xc0 | ch, staff.program, 0, 0xb0 | ch, 7, staff.volume.min(127)]);
        tr.extend([0, 0xb0 | ch, 10, (64 + staff.pan as i32).clamp(0, 127) as u8]);
        let mut last = 0;
        for (at, _, d) in evs {
            let at = at * FILE_TPQ / TPQ;
            vlq(at - last, &mut tr);
            tr.extend(d);
            last = at;
        }
        tr.extend([0, 0xff, 0x2f, 0]);
        tracks.push(tr);
    }
    let mut out = b"MThd".to_vec();
    out.extend([0, 0, 0, 6, 0, 1]);
    out.extend((tracks.len() as u16).to_be_bytes());
    out.extend((FILE_TPQ as u16).to_be_bytes());
    for t in tracks {
        out.extend(b"MTrk");
        out.extend((t.len() as u32).to_be_bytes());
        out.extend(t);
    }
    out
}

/// Read a Standard MIDI File into a new score, with quantized durations (Guess Durations).
pub fn import_midi(b: &[u8]) -> Result<Score, String> {
    if b.len() < 14 || &b[0..4] != b"MThd" {
        return Err("Not a MIDI file".into());
    }
    let div = u16::from_be_bytes([b[12], b[13]]) as u32;
    if div & 0x8000 != 0 || div == 0 {
        return Err("SMPTE-timed MIDI files are not supported".into());
    }
    let mut pos = 14;
    let mut tracks: Vec<(String, u8, Vec<(u32, u32, u8, u8)>)> = vec![];
    let mut tempo = 120;
    let mut ts = (4u8, 4u8);
    let mut key = 0i8;
    while pos + 8 <= b.len() {
        let len = u32::from_be_bytes([b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]]) as usize;
        let body = &b[pos + 8..(pos + 8 + len).min(b.len())];
        pos += 8 + len;
        let mut i = 0;
        let mut t = 0u32;
        let mut status = 0u8;
        let mut on: std::collections::HashMap<(u8, u8), (u32, u8)> = Default::default();
        let mut notes = vec![];
        let mut name = String::new();
        let mut program = 0;
        let rd = |i: &mut usize| -> u32 {
            let mut v = 0u32;
            while *i < body.len() {
                let c = body[*i];
                *i += 1;
                v = (v << 7) | (c & 0x7f) as u32;
                if c & 0x80 == 0 {
                    break;
                }
            }
            v
        };
        while i < body.len() {
            t += rd(&mut i);
            if i >= body.len() {
                break;
            }
            let mut c = body[i];
            if c & 0x80 != 0 {
                i += 1;
                if c < 0xf0 {
                    status = c;
                }
            } else {
                c = status;
            }
            match c {
                0xff => {
                    let ty = body[i];
                    i += 1;
                    let l = rd(&mut i) as usize;
                    let d = &body[i..(i + l).min(body.len())];
                    match ty {
                        3 => name = String::from_utf8_lossy(d).into(),
                        0x51 if d.len() == 3 => {
                            let us = ((d[0] as u32) << 16) | ((d[1] as u32) << 8) | d[2] as u32;
                            if t == 0 {
                                tempo = 60_000_000 / us.max(1);
                            }
                        }
                        0x58 if d.len() >= 2 && t == 0 => ts = (d[0], 1 << d[1]),
                        0x59 if !d.is_empty() && t == 0 => key = d[0] as i8,
                        _ => {}
                    }
                    i += l;
                }
                0xf0 | 0xf7 => {
                    let l = rd(&mut i) as usize;
                    i += l;
                }
                _ => {
                    let kind = c & 0xf0;
                    let chn = c & 0x0f;
                    let n = if kind == 0xc0 || kind == 0xd0 { 1 } else { 2 };
                    if i + n > body.len() {
                        break;
                    }
                    let d1 = body[i];
                    let d2 = if n == 2 { body[i + 1] } else { 0 };
                    i += n;
                    match kind {
                        0x90 if d2 > 0 => {
                            on.insert((chn, d1), (t, d2));
                        }
                        0x80 | 0x90 => {
                            if let Some((st, v)) = on.remove(&(chn, d1)) {
                                notes.push((st, t.saturating_sub(st).max(1), d1, v));
                            }
                        }
                        0xc0 => program = d1,
                        _ => {}
                    }
                }
            }
        }
        if !notes.is_empty() {
            tracks.push((name, program, notes));
        }
    }
    if tracks.is_empty() {
        return Err("The MIDI file has no notes".into());
    }
    let scale = |t: u32| (t as u64 * TPQ as u64 / div as u64) as u32;
    let mt = TPQ * 4 * ts.0 as u32 / ts.1.max(1) as u32;
    let end = tracks.iter().flat_map(|t| t.2.iter().map(|n| scale(n.0 + n.1))).max().unwrap_or(0);
    let nmeas = (end / mt + 1) as usize;
    let mut staves = vec![];
    for (k, (name, prog, notes)) in tracks.iter().enumerate() {
        let avg = notes.iter().map(|n| n.2 as u32).sum::<u32>() / notes.len() as u32;
        let clef = if avg < 57 { Clef::Bass } else { Clef::Treble };
        let mut st = Staff::new(if name.is_empty() { GM_NAMES[*prog as usize] } else { name }, clef, *prog);
        st.channel = k as u8 % 16;
        staves.push(st);
    }
    let mut sc = Score::new(staves, nmeas);
    for m in sc.measures.iter_mut() {
        m.num = ts.0;
        m.den = ts.1;
        m.key = key.clamp(-7, 7);
    }
    sc.measures[0].tempo = Some(tempo);
    sc.text.title = "Imported MIDI".into();
    let q = TPQ / 4; // quantize to 16ths
    for (si, (_, _, notes)) in tracks.iter().enumerate() {
        for &(st, len, key_n, vel) in notes {
            let st = ((scale(st) + q / 2) / q) * q;
            let m = (st / mt) as usize;
            if m >= nmeas {
                continue;
            }
            let start = st % mt;
            let len = scale(len).max(q).min(mt - start);
            let (value, dots) = guess_value(len);
            let p = Pitch::from_midi(key_n as i32, key < 0);
            let evs = &mut sc.events[si][m];
            if let Some(e) = evs.iter_mut().find(|e| e.start == start && e.voice == 1) {
                if !e.pitches.contains(&p) {
                    e.pitches.push(p);
                    e.sort_pitches();
                }
            } else {
                let mut e = Event::new(1, start, value, vec![p]);
                e.dots = dots;
                e.velocity = vel;
                evs.push(e);
            }
        }
    }
    for s in 0..sc.staves.len() {
        for m in 0..nmeas {
            sc.sort_measure(s, m);
        }
    }
    Ok(sc)
}

/// Largest written value (with up to one dot) that fits in `len` ticks.
pub fn guess_value(len: u32) -> (u8, u8) {
    for v in 0..=8u8 {
        let b = base_ticks(v);
        if b + b / 2 <= len {
            return (v, 1);
        }
        if b <= len {
            return (v, 0);
        }
    }
    (8, 0)
}

/// The written value (with up to one dot) closest to a played length, never longer than `max` ticks.
/// A long note released a little early still becomes the full value (3.75 beats -> whole note), and one held
/// for 3 beats becomes a dotted half. Ties go to the longer value.
pub fn nearest_value(len: u32, max: u32) -> (u8, u8) {
    let mut best: Option<(u8, u8, u32)> = None;
    for v in 0..=8u8 {
        for d in 0..=1u8 {
            if v == 8 && d == 1 {
                continue;
            }
            let t = dur_ticks(v, d, None);
            if t > max.max(base_ticks(8)) {
                continue;
            }
            let diff = (t as i64 - len as i64).unsigned_abs() as u32;
            let better = match best {
                None => true,
                Some((_, _, bt)) => {
                    let bdiff = (bt as i64 - len as i64).unsigned_abs() as u32;
                    diff < bdiff || (diff == bdiff && t > bt)
                }
            };
            if better {
                best = Some((v, d, t));
            }
        }
    }
    best.map(|b| (b.0, b.1)).unwrap_or((8, 0))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn musicxml_string(score: &Score) -> String {
    let div = FILE_TPQ;
    let cv = |t: u32| t * FILE_TPQ / TPQ;
    let types = ["breve", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th", "128th"];
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 3.1 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n<score-partwise version=\"3.1\">\n");
    x.push_str(&format!("<work><work-title>{}</work-title></work>\n", xml_escape(&score.text.title)));
    x.push_str(&format!("<identification><creator type=\"composer\">{}</creator></identification>\n<part-list>\n", xml_escape(&score.text.composer)));
    for (i, s) in score.staves.iter().enumerate() {
        x.push_str(&format!(
            "<score-part id=\"P{}\"><part-name>{}</part-name><midi-instrument id=\"P{}-I1\"><midi-channel>{}</midi-channel><midi-program>{}</midi-program></midi-instrument></score-part>\n",
            i + 1,
            xml_escape(&s.name),
            i + 1,
            s.channel + 1,
            s.program as u32 + 1
        ));
    }
    x.push_str("</part-list>\n");
    for (si, staff) in score.staves.iter().enumerate() {
        x.push_str(&format!("<part id=\"P{}\">\n", si + 1));
        for (m, mi) in score.measures.iter().enumerate() {
            x.push_str(&format!("<measure number=\"{}\">\n", m + 1));
            let prev = if m > 0 { Some(&score.measures[m - 1]) } else { None };
            let clef_change = staff.clefs.iter().find(|c| c.0 == m).map(|c| c.1);
            if m == 0 || prev.map(|p| p.key != mi.key || p.num != mi.num || p.den != mi.den).unwrap_or(false) || clef_change.is_some() {
                x.push_str(&format!("<attributes><divisions>{div}</divisions><key><fifths>{}</fifths></key><time><beats>{}</beats><beat-type>{}</beat-type></time>", mi.key, mi.num, mi.den));
                if let Some(c) = clef_change.or(if m == 0 { Some(staff.clef_at(0)) } else { None }) {
                    let (sign, line, oct) = match c {
                        Clef::Treble => ("G", 2, 0),
                        Clef::Treble8va => ("G", 2, 1),
                        Clef::Treble8vb => ("G", 2, -1),
                        Clef::Bass => ("F", 4, 0),
                        Clef::Bass8vb => ("F", 4, -1),
                        Clef::Alto => ("C", 3, 0),
                        Clef::Tenor => ("C", 4, 0),
                        Clef::Percussion => ("percussion", 3, 0),
                    };
                    x.push_str(&format!("<clef><sign>{sign}</sign><line>{line}</line>"));
                    if oct != 0 {
                        x.push_str(&format!("<clef-octave-change>{oct}</clef-octave-change>"));
                    }
                    x.push_str("</clef>");
                }
                if staff.transpose != 0 {
                    x.push_str(&format!("<transpose><chromatic>{}</chromatic></transpose>", staff.transpose));
                }
                x.push_str("</attributes>\n");
            }
            if let Some(t) = mi.tempo {
                x.push_str(&format!("<direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{t}</per-minute></metronome></direction-type><sound tempo=\"{t}\"/></direction>\n"));
            }
            let evs = &score.events[si][m];
            let mut voices: Vec<u8> = evs.iter().map(|e| e.voice).collect();
            voices.sort();
            voices.dedup();
            if voices.is_empty() {
                x.push_str(&format!("<note><rest measure=\"yes\"/><duration>{}</duration><voice>1</voice></note>\n", cv(mi.ticks())));
            }
            for (vi, v) in voices.iter().enumerate() {
                if vi > 0 {
                    x.push_str(&format!("<backup><duration>{}</duration></backup>\n", cv(mi.ticks())));
                }
                let mut t = 0u32;
                for e in evs.iter().filter(|e| e.voice == *v) {
                    if e.start > t && !e.grace {
                        x.push_str(&format!("<forward><duration>{}</duration></forward>\n", cv(e.start - t)));
                        t = e.start;
                    }
                    if let Some(d) = &e.dynamic {
                        x.push_str(&format!("<direction placement=\"below\"><direction-type><dynamics><{}/></dynamics></direction-type></direction>\n", xml_escape(d)));
                    }
                    let items: Vec<Option<&Pitch>> = if e.is_rest() { vec![None] } else { e.pitches.iter().map(Some).collect() };
                    for (k, p) in items.iter().enumerate() {
                        x.push_str("<note>");
                        if e.grace {
                            x.push_str("<grace/>");
                        }
                        if k > 0 {
                            x.push_str("<chord/>");
                        }
                        match p {
                            Some(p) => {
                                x.push_str(&format!("<pitch><step>{}</step>", LETTERS[p.letter()]));
                                if p.alter != 0 {
                                    x.push_str(&format!("<alter>{}</alter>", p.alter));
                                }
                                x.push_str(&format!("<octave>{}</octave></pitch>", p.step.div_euclid(7)));
                            }
                            None => x.push_str("<rest/>"),
                        }
                        if !e.grace {
                            x.push_str(&format!("<duration>{}</duration>", cv(e.ticks())));
                        }
                        // a tie ends on this note (stop) and/or starts from it (start), in that order
                        let tie_stop = !e.grace && p.map(|p| crate::audio::tied_into(score, si, m, e, p)).unwrap_or(false);
                        let tie_start = e.ties.get(k).copied().unwrap_or(false);
                        if tie_stop {
                            x.push_str("<tie type=\"stop\"/>");
                        }
                        if tie_start {
                            x.push_str("<tie type=\"start\"/>");
                        }
                        x.push_str(&format!("<voice>{v}</voice><type>{}</type>", types[e.value.min(8) as usize]));
                        for _ in 0..e.dots {
                            x.push_str("<dot/>");
                        }
                        if let Some((a, n)) = e.tuplet {
                            x.push_str(&format!("<time-modification><actual-notes>{a}</actual-notes><normal-notes>{n}</normal-notes></time-modification>"));
                        }
                        match e.stem {
                            Stem::Up => x.push_str("<stem>up</stem>"),
                            Stem::Down => x.push_str("<stem>down</stem>"),
                            _ => {}
                        }
                        let mut notations = String::new();
                        if tie_stop {
                            notations.push_str("<tied type=\"stop\"/>");
                        }
                        if tie_start {
                            notations.push_str("<tied type=\"start\"/>");
                        }
                        if k == 0 {
                            let mut arts = String::new();
                            for mk in &e.marks {
                                match mk {
                                    Mark::Staccato => arts.push_str("<staccato/>"),
                                    Mark::Staccatissimo => arts.push_str("<staccatissimo/>"),
                                    Mark::Tenuto => arts.push_str("<tenuto/>"),
                                    Mark::Accent => arts.push_str("<accent/>"),
                                    Mark::Marcato => arts.push_str("<strong-accent/>"),
                                    Mark::Fermata | Mark::FermataBelow => notations.push_str("<fermata/>"),
                                    Mark::Trill => notations.push_str("<ornaments><trill-mark/></ornaments>"),
                                    _ => {}
                                }
                            }
                            if !arts.is_empty() {
                                notations.push_str(&format!("<articulations>{arts}</articulations>"));
                            }
                        }
                        if !notations.is_empty() {
                            x.push_str(&format!("<notations>{notations}</notations>"));
                        }
                        if k == 0 {
                            for (li, l) in e.lyric.iter().enumerate() {
                                if !l.is_empty() {
                                    x.push_str(&format!("<lyric number=\"{}\"><text>{}</text></lyric>", li + 1, xml_escape(l.trim_end_matches('-'))));
                                }
                            }
                        }
                        x.push_str("</note>\n");
                    }
                    if !e.grace {
                        t = e.end();
                    }
                }
            }
            let bar = match mi.barline {
                Barline::Double => Some(("light-light", None)),
                Barline::Final => Some(("light-heavy", None)),
                Barline::RepeatEnd | Barline::RepeatBoth => Some(("light-heavy", Some("backward"))),
                Barline::Dashed => Some(("dashed", None)),
                Barline::Invisible => Some(("none", None)),
                _ => None,
            };
            if let Some((style, rep)) = bar {
                x.push_str(&format!("<barline location=\"right\"><bar-style>{style}</bar-style>"));
                if let Some(r) = rep {
                    x.push_str(&format!("<repeat direction=\"{r}\"/>"));
                }
                x.push_str("</barline>\n");
            }
            x.push_str("</measure>\n");
        }
        x.push_str("</part>\n");
    }
    x.push_str("</score-partwise>\n");
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    /// The notes of a Standard MIDI File written by `midi_bytes`: (key, start, length) in quarter notes.
    fn notes_of(bytes: &[u8]) -> Vec<(u8, f32, f32)> {
        let div = u16::from_be_bytes([bytes[12], bytes[13]]) as f32;
        let mut out = vec![];
        let mut i = 14;
        while i + 8 <= bytes.len() {
            let len = u32::from_be_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
            let tr = &bytes[i + 8..i + 8 + len];
            i += 8 + len;
            let (mut p, mut t) = (0usize, 0u32);
            let mut open: std::collections::HashMap<u8, Vec<u32>> = Default::default();
            while p < tr.len() {
                let mut d = 0u32;
                loop {
                    let b = tr[p];
                    p += 1;
                    d = (d << 7) | (b & 0x7f) as u32;
                    if b & 0x80 == 0 {
                        break;
                    }
                }
                t += d;
                let st = tr[p];
                match st & 0xf0 {
                    0x90 if tr[p + 2] > 0 => {
                        open.entry(tr[p + 1]).or_default().push(t);
                        p += 3;
                    }
                    0x80 | 0x90 => {
                        if let Some(s) = open.get_mut(&tr[p + 1]).and_then(|v| if v.is_empty() { None } else { Some(v.remove(0)) }) {
                            out.push((tr[p + 1], s as f32 / div, (t - s) as f32 / div));
                        }
                        p += 3;
                    }
                    0xb0 => p += 3,
                    0xc0 => p += 2,
                    0xf0 if st == 0xff => {
                        let l = tr[p + 2] as usize;
                        p += 3 + l;
                    }
                    _ => break,
                }
            }
        }
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)));
        out
    }

    fn app() -> App {
        let mut a = App::new(true);
        a.score.measures.iter_mut().for_each(|m| m.tempo = None);
        a
    }

    /// One entry for `score_with`: (measure, tick, midi keys, value, tied keys).
    type NoteSpec<'a> = (usize, u32, &'a [i32], u8, &'a [i32]);

    /// Enters notes and ties the given pitches.
    fn score_with(notes: &[NoteSpec]) -> App {
        let mut a = app();
        for &(m, t, keys, v, _) in notes {
            for &k in keys {
                a.enter(0, m, t, Some(Pitch::from_midi(k, false)), v, 0, false);
            }
        }
        for &(m, t, _, _, tied) in notes {
            let e = a.score.events[0][m].iter_mut().find(|e| e.start == t).unwrap();
            for (i, p) in e.pitches.clone().iter().enumerate() {
                if tied.contains(&p.midi()) {
                    e.ties[i] = true;
                }
            }
        }
        a
    }

    #[test]
    fn midi_whole_tied_across_the_bar_line_is_one_note_of_8_beats() {
        // B-01: G4 whole in measure 1 tied to G4 whole in measure 2
        let a = score_with(&[(0, 0, &[67], 1, &[67]), (1, 0, &[67], 1, &[])]);
        let n: Vec<_> = notes_of(&midi_bytes(&a.score)).into_iter().filter(|n| n.0 == 67).collect();
        assert_eq!(n.len(), 1, "{n:?}");
        assert_eq!(n[0].2, 8.0);
    }

    #[test]
    fn midi_chain_of_three_tied_notes_is_one_note() {
        let a = score_with(&[(0, 0, &[64], 2, &[64]), (0, 2 * TPQ, &[64], 2, &[64]), (1, 0, &[64], 3, &[])]);
        let n: Vec<_> = notes_of(&midi_bytes(&a.score)).into_iter().filter(|n| n.0 == 64).collect();
        assert_eq!(n.len(), 1, "{n:?}");
        assert_eq!(n[0].2, 5.0);
    }

    #[test]
    fn midi_tie_in_a_chord_only_holds_the_tied_notes() {
        // C-E-G half notes; only G is tied into the next C-E-G
        let a = score_with(&[(0, 0, &[60, 64, 67], 2, &[67]), (0, 2 * TPQ, &[60, 64, 67], 2, &[])]);
        let n = notes_of(&midi_bytes(&a.score));
        let count = |k: u8| n.iter().filter(|x| x.0 == k).count();
        assert_eq!((count(60), count(64), count(67)), (2, 2, 1), "{n:?}");
        assert_eq!(n.iter().find(|x| x.0 == 67).unwrap().2, 4.0);
    }

    /// The tie elements of every G4/E4/C4... note in the MusicXML, in order: ("start", "stop" or "stop+start").
    fn tie_marks(xml: &str, step: &str, octave: i32) -> Vec<String> {
        xml.split("<note>")
            .skip(1)
            .filter(|n| n.contains(&format!("<step>{step}</step>")) && n.contains(&format!("<octave>{octave}</octave>")))
            .map(|n| {
                let n = &n[..n.find("</note>").unwrap()];
                let tie: Vec<&str> = ["stop", "start"].into_iter().filter(|t| n.contains(&format!("<tie type=\"{t}\"/>"))).collect();
                let tied: Vec<&str> = ["stop", "start"].into_iter().filter(|t| n.contains(&format!("<tied type=\"{t}\"/>"))).collect();
                assert_eq!(tie, tied, "<tie> and <tied> must agree");
                if n.contains("<tie type=\"start\"/>") && n.contains("<tie type=\"stop\"/>") {
                    assert!(n.find("<tie type=\"stop\"/>") < n.find("<tie type=\"start\"/>"), "stop comes before start");
                }
                tie.join("+")
            })
            .collect()
    }

    #[test]
    fn musicxml_ties_have_start_and_stop() {
        // B-02, the same three cases as the MIDI tests
        let a = score_with(&[(0, 0, &[67], 1, &[67]), (1, 0, &[67], 1, &[])]);
        assert_eq!(tie_marks(&musicxml_string(&a.score), "G", 4), vec!["start", "stop"]);
        let a = score_with(&[(0, 0, &[64], 2, &[64]), (0, 2 * TPQ, &[64], 2, &[64]), (1, 0, &[64], 3, &[])]);
        assert_eq!(tie_marks(&musicxml_string(&a.score), "E", 4), vec!["start", "stop+start", "stop"]);
        let a = score_with(&[(0, 0, &[60, 64, 67], 2, &[67]), (0, 2 * TPQ, &[60, 64, 67], 2, &[])]);
        let x = musicxml_string(&a.score);
        assert_eq!(tie_marks(&x, "G", 4), vec!["start", "stop"]);
        assert_eq!(tie_marks(&x, "C", 4), vec!["", ""]);
        assert_eq!(tie_marks(&x, "E", 4), vec!["", ""]);
    }
}

#[cfg(test)]
mod open_any_file_tests {
    //! F-26: the browser's file picker has no type filter (iOS greys out extensions it doesn't know), so a file is
    //! recognised by its content, and anything else gets a clear message instead of breaking the app.
    use super::*;

    fn fmt_bytes() -> Vec<u8> {
        score_bytes(&crate::app::default_new_score()).unwrap()
    }

    #[test]
    fn scores_are_recognised_by_content_whatever_the_name() {
        // a Fermata score and a MIDI file under names iOS might hand over (no extension, another extension)
        for name in ["score.fmt", "score", "score.json", "score.txt"] {
            assert!(open_bytes(name, &fmt_bytes()).is_ok(), "{name}");
        }
        // (a MIDI file needs a note to import)
        let mut a = crate::app::App::new(true);
        a.enter(0, 0, 0, Some(Pitch::from_midi(60, false)), 3, 0, false);
        let midi = midi_bytes(&a.score);
        for name in ["song.mid", "song", "song.bin"] {
            assert!(open_bytes(name, &midi).is_ok(), "{name}");
        }
    }

    #[test]
    fn other_files_are_refused_without_breaking() {
        let junk: Vec<u8> = (0..4000u32).map(|i| (i * 7 + 3) as u8).collect();
        let png = [0x89u8, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
        // a broken Encore file: the right header, then nothing useful
        let mut broken_enc = b"SCOW".to_vec();
        broken_enc.extend_from_slice(&[0xff; 40]);
        let cases: [(&str, &[u8]); 8] = [
            ("photo.png", &png),
            ("notes.txt", b"hello"),
            ("data.json", b"{\"a\": 1}"),
            ("empty.fmt", b""),
            ("junk.enc", &junk),
            ("junk.mid", &junk),
            ("broken.enc", &broken_enc),
            ("short.enc", b"SCOW"),
        ];
        for (name, bytes) in cases {
            // must return (Ok for a damaged-but-readable Encore file, Err otherwise), never panic
            let r = std::panic::catch_unwind(|| open_bytes(name, bytes));
            assert!(r.is_ok(), "{name} made the app panic");
        }
        for (name, bytes) in [("photo.png", &png[..]), ("notes.txt", b"hello"), ("junk.mid", &junk[..]), ("short.enc", b"SCO")] {
            assert!(open_bytes(name, bytes).is_err(), "{name} was taken for a score");
        }
    }

    #[test]
    fn the_message_names_the_file() {
        let m = open_error("photo.png", "Not a score file: expected value");
        assert!(m.starts_with("photo.png isn't a score Fermata can open."));
        assert!(open_error("", "x").starts_with("This file isn't a score Fermata can open."));
    }
}
