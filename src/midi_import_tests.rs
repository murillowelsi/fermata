//! F-20 tests: importing MIDI keeps triplets, and leaves MIDI without triplets exactly as before. The files are made
//! here (a small Standard MIDI File writer), plus the scores of the test corpus exported as MIDI
//! (tests/corpus/midi); the expected imports of the binary ones were made with Fermata 0.3.22.
use super::*;

/// A Standard MIDI File, format 1, 480 ticks a quarter: one track per list of notes (start, length, key, velocity),
/// 4/4 at `bpm`.
pub fn smf(bpm: u32, tracks: &[Vec<(u32, u32, u8, u8)>]) -> Vec<u8> {
    fn var(mut v: u32, out: &mut Vec<u8>) {
        let mut b = vec![(v & 0x7f) as u8];
        v >>= 7;
        while v > 0 {
            b.push((v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        b.reverse();
        out.extend(b);
    }
    let mut out = b"MThd".to_vec();
    out.extend([0, 0, 0, 6, 0, 1]);
    out.extend((tracks.len() as u16 + 1).to_be_bytes());
    out.extend(480u16.to_be_bytes());
    let mut chunk = |body: Vec<u8>| {
        out.extend(b"MTrk");
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
    };
    let us = 60_000_000 / bpm;
    let mut meta = vec![0, 0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8, 0, 0xff, 0x58, 4, 4, 2, 24, 8];
    meta.extend([0, 0xff, 0x2f, 0]);
    chunk(meta);
    for (k, notes) in tracks.iter().enumerate() {
        let mut ev: Vec<(u32, u8, u8, u8)> = vec![];
        for &(t, len, key, vel) in notes {
            ev.push((t, 0x90 | k as u8, key, vel));
            ev.push((t + len, 0x80 | k as u8, key, 0));
        }
        // note-offs before note-ons at the same moment
        ev.sort_by_key(|e| (e.0, e.1 & 0xf0 == 0x90));
        let mut body = vec![];
        let mut now = 0;
        for (t, st, key, vel) in ev {
            var(t - now, &mut body);
            now = t;
            body.extend([st, key, vel]);
        }
        body.extend([0, 0xff, 0x2f, 0]);
        chunk(body);
    }
    out
}

/// A deterministic wobble of up to ±`amp` ticks.
fn wobble(i: u32, amp: i32) -> i32 {
    let x = (i.wrapping_mul(2654435761) >> 8) % 1000;
    (x as i32 * 2 * amp) / 1000 - amp
}

fn at(t: u32, i: u32, amp: i32) -> u32 {
    (t as i32 + wobble(i, amp)).max(0) as u32
}

/// The MIDI files without triplets: straight, dotted, syncopated, chords, two hands, light swing, and played (±20
/// ms at 100 bpm, ±16 ticks).
pub fn binary_cases() -> Vec<(&'static str, Vec<u8>)> {
    let q = 480u32;
    let scale = [60u8, 62, 64, 65, 67, 69, 71, 72];
    let eighths: Vec<_> = (0..16).map(|i| (i * q / 2, q / 2 - 20, scale[i as usize % 8], 80)).collect();
    let sixteenths: Vec<_> = (0..32).map(|i| (i * q / 4, q / 4 - 10, scale[i as usize % 8], 80)).collect();
    let dotted: Vec<_> = (0..8).flat_map(|b| [(b * q, 3 * q / 4 - 10, 67, 80), (b * q + 3 * q / 4, q / 4 - 10, 69, 80)]).collect();
    let synco: Vec<_> = (0..4).flat_map(|b| [(b * 2 * q, q / 2, 60, 80), (b * 2 * q + q / 2, q, 64, 80), (b * 2 * q + 3 * q / 2, q / 2, 67, 80)]).collect();
    let chords: Vec<_> = (0..8).flat_map(|b| [(b * q, q - 30, 60, 70), (b * q, q - 30, 64, 70), (b * q, q - 30, 67, 70)]).collect();
    let bass: Vec<_> = (0..4).map(|b| (b * 2 * q, 2 * q - 40, 48, 70)).collect();
    let swing: Vec<_> = (0..16).map(|i| (i / 2 * q + if i % 2 == 1 { 278 } else { 0 }, q / 3, scale[i as usize % 8], 80)).collect();
    let played: Vec<_> = (0..32).map(|i| (at(i * q / 4, i, 16), q / 4 - 20, scale[i as usize % 8], 70 + (i % 20) as u8)).collect();
    let played8: Vec<_> = (0..16).map(|i| (at(i * q / 2, i + 7, 16), q / 2 - 40, scale[(i * 3) as usize % 8], 80)).collect();
    vec![
        ("eighths", smf(100, &[eighths])),
        ("sixteenths", smf(100, &[sixteenths])),
        ("dotted", smf(100, &[dotted])),
        ("syncopated", smf(100, &[synco])),
        ("chords-two-hands", smf(100, &[chords, bass])),
        ("light-swing", smf(100, &[swing])),
        ("played-sixteenths", smf(100, &[played])),
        ("played-eighths", smf(100, &[played8])),
    ]
}

/// The corpus scores exported as MIDI (tests/corpus/midi) that have no triplets (the triplet study, and "piano" and
/// "piano-34" with quarter-note triplets in their second measure, have).
pub fn corpus_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("estudo-em-sol", include_bytes!("../tests/corpus/midi/estudo-em-sol.mid")),
        ("cantiga-letra-cifras", include_bytes!("../tests/corpus/midi/cantiga-letra-cifras.mid")),
    ]
}

fn json(sc: &Score) -> String {
    serde_json::to_string_pretty(sc).unwrap()
}

/// Writes the expected imports (run with Fermata 0.3.22's import: `F20_GOLDEN=<dir> cargo test -- --ignored`).
#[test]
#[ignore]
fn write_goldens() {
    let dir = std::env::var("F20_GOLDEN").expect("F20_GOLDEN");
    for (name, b) in binary_cases() {
        std::fs::write(format!("{dir}/{name}.json"), json(&import_midi(&b).unwrap())).unwrap();
    }
    for (name, b) in corpus_cases() {
        std::fs::write(format!("{dir}/{name}.json"), json(&import_midi(b).unwrap())).unwrap();
    }
}

/// MIDI without triplets imports exactly as in 0.3.22 (the same score, byte for byte).
#[test]
fn midi_without_triplets_imports_exactly_as_before() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/midi/golden");
    let mut cases: Vec<(String, Vec<u8>)> = binary_cases().into_iter().map(|(n, b)| (n.to_string(), b)).collect();
    cases.extend(corpus_cases().into_iter().map(|(n, b)| (n.to_string(), b.to_vec())));
    for (name, b) in cases {
        let want = std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap();
        assert!(json(&import_midi(&b).unwrap()) == want, "{name}: the import changed");
    }
}

/// The notes of voice 1 in a measure: (start, value, dots, triplet, midi keys; a rest has none).
fn bar(sc: &Score, s: usize, m: usize) -> Vec<(u32, u8, u8, bool, Vec<i32>)> {
    sc.events[s][m].iter().filter(|e| e.voice == 1).map(|e| (e.start, e.value, e.dots, e.tuplet == Some((3, 2)), e.pitches.iter().map(|p| p.midi()).collect())).collect()
}

/// Whether the triplet groups of a measure are whole (their notes and rests add up to their beats).
fn groups_whole(sc: &Score, s: usize, m: usize) -> bool {
    let tri: u32 = sc.events[s][m].iter().filter(|e| e.voice == 1 && e.tuplet.is_some()).map(|e| e.ticks()).sum();
    tri.is_multiple_of(TPQ / 2)
}

const Q: u32 = 480;

#[test]
fn eighth_note_triplets_come_back_as_triplets() {
    // beat 1: three triplet eighths; beat 2: two eighths; beat 3: triplet eighths; beat 4: a quarter
    let n = |t: u32, len: u32, k: u8| (t, len, k, 80u8);
    let notes = vec![n(0, 150, 60), n(160, 150, 62), n(320, 150, 64), n(Q, 240, 65), n(Q + 240, 240, 67), n(2 * Q, 150, 69), n(2 * Q + 160, 150, 71), n(2 * Q + 320, 150, 72), n(3 * Q, 480, 72)];
    let sc = import_midi(&smf(120, &[notes])).unwrap();
    assert_eq!(
        bar(&sc, 0, 0),
        vec![
            (0, 4, 0, true, vec![60]),
            (160, 4, 0, true, vec![62]),
            (320, 4, 0, true, vec![64]),
            (Q, 4, 0, false, vec![65]),
            (Q + 240, 4, 0, false, vec![67]),
            (2 * Q, 4, 0, true, vec![69]),
            (2 * Q + 160, 4, 0, true, vec![71]),
            (2 * Q + 320, 4, 0, true, vec![72]),
            (3 * Q, 3, 0, false, vec![72]),
        ]
    );
    // the measure is full: 4 beats
    assert_eq!(sc.events[0][0].iter().map(|e| e.ticks()).sum::<u32>(), 4 * Q);
}

#[test]
fn quarter_and_sixteenth_triplets_and_rests_inside_the_group() {
    let n = |t: u32, len: u32, k: u8| (t, len, k, 80u8);
    let notes = vec![
        // beats 1-2: three quarter-note triplets
        n(0, 300, 60),
        n(320, 300, 62),
        n(640, 300, 64),
        // beat 3: a sixteenth-note triplet, then two sixteenths
        n(2 * Q, 70, 65),
        n(2 * Q + 80, 70, 67),
        n(2 * Q + 160, 70, 69),
        n(2 * Q + 240, 110, 71),
        n(2 * Q + 360, 110, 72),
        // beat 4: a triplet with its first note missing (a rest), then two eighths of it
        n(3 * Q + 160, 150, 67),
        n(3 * Q + 320, 150, 64),
    ];
    let sc = import_midi(&smf(120, &[notes])).unwrap();
    let b = bar(&sc, 0, 0);
    assert_eq!(&b[..3], &[(0, 3, 0, true, vec![60]), (320, 3, 0, true, vec![62]), (640, 3, 0, true, vec![64])]);
    assert_eq!(&b[3..6], &[(2 * Q, 5, 0, true, vec![65]), (2 * Q + 80, 5, 0, true, vec![67]), (2 * Q + 160, 5, 0, true, vec![69])]);
    assert_eq!(&b[6..8], &[(2 * Q + 240, 5, 0, false, vec![71]), (2 * Q + 360, 5, 0, false, vec![72])]);
    assert_eq!(&b[8..], &[(3 * Q, 4, 0, true, vec![]), (3 * Q + 160, 4, 0, true, vec![67]), (3 * Q + 320, 4, 0, true, vec![64])]);
    assert!(groups_whole(&sc, 0, 0));
    assert_eq!(sc.events[0][0].iter().map(|e| e.ticks()).sum::<u32>(), 4 * Q, "the measure closes");
}

/// Played: every note up to 20 ms early or late (±19 ticks at 120 bpm), chords, a quarter-triplet + eighth-triplet.
#[test]
fn played_triplets_with_twenty_ms_of_wobble() {
    let mut notes = vec![];
    let mut i = 0;
    for bar in 0..4u32 {
        for beat in 0..4u32 {
            let b0 = bar * 4 * Q + beat * Q;
            let slots: &[u32] = if beat == 3 { &[0, 2] } else { &[0, 1, 2] };
            for &s in slots {
                i += 1;
                let t = at(b0 + s * 160, i, 19);
                notes.push((t, 140, 60 + (i % 12) as u8, 80));
                if beat == 0 && s == 0 {
                    notes.push((t + 3, 140, 48, 70)); // a chord, its notes a little apart
                }
            }
        }
    }
    let sc = import_midi(&smf(120, &[notes])).unwrap();
    for m in 0..4 {
        let b = bar(&sc, 0, m);
        assert!(b.iter().all(|e| e.3), "bar {}: every note a triplet: {b:?}", m + 1);
        assert_eq!(b.iter().map(|e| e.0).collect::<Vec<_>>(), vec![0, 160, 320, Q, Q + 160, Q + 320, 2 * Q, 2 * Q + 160, 2 * Q + 320, 3 * Q, 3 * Q + 320]);
        // the long-short beat: quarter triplet + eighth triplet
        assert_eq!((b[9].1, b[10].1), (3, 4));
        assert_eq!(b[0].4.len(), 2, "the chord stays one");
        assert_eq!(sc.events[0][m].iter().map(|e| e.ticks()).sum::<u32>(), 4 * Q);
    }
}

/// Straight eighths played loosely stay straight; eighths played swung to 2:1 are triplets in the file.
#[test]
fn swing_stays_straight_unless_the_file_is_in_triplets() {
    let q = Q;
    let straight: Vec<_> = (0..8).map(|i| (at(i * q / 2, i, 19), 200, 67, 80)).collect();
    let sc = import_midi(&smf(120, &[straight])).unwrap();
    assert!(sc.events[0][0].iter().all(|e| e.tuplet.is_none()));
    let swung: Vec<_> = (0..8).map(|i| (i / 2 * q + if i % 2 == 1 { 320 } else { 0 }, 140, 67, 80)).collect();
    let sc = import_midi(&smf(120, &[swung])).unwrap();
    assert!(sc.events[0][0].iter().all(|e| e.tuplet.is_some()));
}

/// The triplet study of the corpus (MusicXML → MIDI): its first measure's triplets come back.
#[test]
fn the_triplet_study_comes_back_with_its_triplets() {
    let sc = import_midi(include_bytes!("../tests/corpus/midi/estudo-tercinas.mid")).unwrap();
    let b = bar(&sc, 0, 0);
    assert_eq!(&b[..3].iter().map(|e| (e.0, e.1, e.3)).collect::<Vec<_>>(), &[(0, 4, true), (160, 4, true), (320, 4, true)], "{b:?}");
}
