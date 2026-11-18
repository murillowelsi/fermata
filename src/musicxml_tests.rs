//! F-07 tests: MusicXML import, round trips through the exporter, .mxl, and damaged files.
use super::*;
use crate::io::{musicxml_string, open_bytes, open_bytes_noted};

const TERCINAS: &str = include_str!("../tests/musicxml/estudo-tercinas.musicxml");
const CANTIGA: &str = include_str!("../tests/musicxml/cantiga-letra-cifras.musicxml");
const ESTUDO_FMT: &[u8] = include_bytes!("../tests/corpus/estudo-em-sol.fmt");
const PIANO_FMT: &[u8] = include_bytes!("../tests/corpus/piano.fmt");
const PIANO34_XML: &[u8] = include_bytes!("../tests/corpus/piano-34.musicxml");

/// Every note and rest that matters musically: (staff, measure, start, voice, pitches, value, dots, tuplet, ties, grace).
type NoteKey = (usize, usize, u32, u8, Vec<(i32, i8)>, u8, u8, Option<(u8, u8)>, Vec<bool>, bool);

fn notes(sc: &Score) -> Vec<NoteKey> {
    let mut v = vec![];
    for (s, st) in sc.events.iter().enumerate() {
        for (m, evs) in st.iter().enumerate() {
            for e in evs {
                v.push((s, m, e.start, e.voice, e.pitches.iter().map(|p| (p.step, p.alter)).collect(), e.value, e.dots, e.tuplet, e.ties.clone(), e.grace));
            }
        }
    }
    v.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    v
}

/// Measures and staves as far as MusicXML carries them both ways.
type Frame = (Vec<(u8, u8, i8, Option<u32>)>, Vec<(String, Vec<(usize, Clef)>, i8)>);

fn frame(sc: &Score) -> Frame {
    (
        sc.measures.iter().map(|m| (m.num, m.den, m.key, m.tempo)).collect(),
        sc.staves.iter().map(|s| (s.name.clone(), s.clefs.clone(), s.transpose)).collect(),
    )
}

fn import_str(x: &str) -> Import {
    import("test.musicxml", x.as_bytes()).expect("imports")
}

fn round_trip(sc: &Score) -> Score {
    let x = musicxml_string(sc);
    import("rt.musicxml", x.as_bytes()).unwrap_or_else(|e| panic!("re-import failed: {e}\n{x}")).score
}

fn ev(sc: &Score, s: usize, m: usize) -> Vec<&Event> {
    sc.events[s][m].iter().collect()
}

// --- the XML reader -------------------------------------------------------------------------------------------

#[test]
fn xml_reader_handles_the_usual_shapes() {
    let x = r#"<?xml version="1.0"?><!DOCTYPE a [ <!ENTITY x "y"> ]><!-- c --><a k='1' j="&lt;&amp;&#233;&#x41;"><b>t &gt; u</b><c/><![CDATA[<raw>]]></a>"#;
    let n = parse_xml(x).unwrap();
    assert_eq!(n.name, "a");
    assert_eq!(n.attr("k"), Some("1"));
    assert_eq!(n.attr("j"), Some("<&éA"));
    assert_eq!(n.text_of("b"), Some("t > u"));
    assert!(n.has("c"));
    assert_eq!(n.text, "<raw>");
}

#[test]
fn utf16_files_are_read() {
    let mut b = vec![0xff, 0xfe];
    for u in TERCINAS.encode_utf16() {
        b.extend(u.to_le_bytes());
    }
    let imp = import("utf16.xml", &b).unwrap();
    assert_eq!(imp.score.text.title, "Estudo de tercinas");
}

// --- hand-written file 1: triplets, quintuplets, two voices, piano on two staves ---------------------------------

#[test]
fn tercinas_parts_staves_and_measures() {
    let imp = import_str(TERCINAS);
    let sc = &imp.score;
    assert_eq!(sc.text.title, "Estudo de tercinas");
    assert_eq!(sc.text.composer, "Fermata Dev 2");
    assert_eq!(sc.staves.len(), 2, "piano: one part, two staves");
    assert_eq!(sc.staves[0].name, "Piano");
    assert_eq!(sc.staves[0].abbrev, "Pno.");
    assert!(sc.staves[0].brace_with_next);
    assert_eq!(sc.staves[0].volume, 102);
    assert_eq!(sc.staves[1].clef_at(0), Clef::Bass);
    assert_eq!(sc.staves[1].clef_at(2), Clef::Treble, "clef change in measure 3");
    assert_eq!(sc.measures.len(), 5);
    let m: Vec<_> = sc.measures.iter().map(|m| (m.num, m.den, m.key)).collect();
    assert_eq!(m, vec![(4, 4, 1), (4, 4, 1), (6, 8, 0), (6, 8, 0), (6, 8, 0)]);
    assert_eq!(sc.measures[0].tempo, Some(96));
    assert_eq!(sc.measures[2].tempo, Some(90), "dotted quarter = 60 is quarter = 90");
    assert_eq!(sc.measures[1].barline, Barline::RepeatStart, "forward repeat at measure 3 = bar line after measure 2");
    assert_eq!(sc.measures[3].barline, Barline::RepeatEnd);
    assert_eq!(sc.measures[4].barline, Barline::Final);
    assert_eq!(sc.measures[3].ending.as_deref(), Some("1."));
    assert_eq!(sc.measures[4].ending.as_deref(), Some("2."));
    // playback: 1 2 3 4 3 5
    assert_eq!(crate::audio::play_order(sc), vec![0, 1, 2, 3, 2, 4]);
    assert!(imp.skipped.is_empty(), "{:?}", imp.skipped);
}

#[test]
fn tercinas_notes() {
    let sc = import_str(TERCINAS).score;
    let q = TPQ;
    // measure 1, upper staff, voice 1: triplet eighths G A B, staccato C, D tied
    let v1: Vec<_> = ev(&sc, 0, 0).into_iter().filter(|e| e.voice == 1).collect();
    assert_eq!(v1.iter().map(|e| e.start).collect::<Vec<_>>(), vec![0, q / 3, 2 * q / 3, q, 2 * q]);
    assert!(v1[..3].iter().all(|e| e.tuplet == Some((3, 2)) && e.value == 4));
    assert_eq!(v1[0].pitches[0].name(), "G4");
    assert_eq!(v1[0].stem, Stem::Up);
    assert_eq!(v1[3].marks, vec![Mark::Staccato]);
    assert_eq!(v1[4].ties, vec![true]);
    assert_eq!(v1[0].dynamic.as_deref(), Some("p"));
    // voice 2 under it: D4 half, then D4+F#4 chord
    let v2: Vec<_> = ev(&sc, 0, 0).into_iter().filter(|e| e.voice == 2).collect();
    assert_eq!(v2.len(), 2);
    assert_eq!(v2[1].start, 2 * q);
    assert_eq!(v2[1].pitches.iter().map(|p| p.name()).collect::<Vec<_>>(), vec!["D4", "F#4"]);
    assert_eq!(v2[0].stem, Stem::Down);
    // lower staff: voice 5 in the file becomes voice 1; whole-note chord
    let low = ev(&sc, 1, 0);
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].voice, 1);
    assert_eq!(low[0].pitches.len(), 2);
    // measure 2: quintuplet sixteenths after the tied half, fermata, grace before D3
    let m2: Vec<_> = ev(&sc, 0, 1);
    assert_eq!(m2.iter().filter(|e| e.tuplet == Some((5, 4))).count(), 5);
    assert_eq!(m2[1].start, 2 * q);
    assert_eq!(m2[2].start, 2 * q + q / 5);
    assert!(m2.last().unwrap().marks.contains(&Mark::Fermata));
    assert!(m2.iter().all(|e| e.play_len.is_none()), "written values match the file's durations");
    let low2 = ev(&sc, 1, 1);
    assert_eq!(low2.iter().map(|e| (e.start, e.grace, e.dots)).collect::<Vec<_>>(), vec![(0, false, 1), (3 * q, true, 0), (3 * q, false, 0)]);
    // graphics: crescendo m1 -> m2, slur over the quintuplet
    let kinds: Vec<_> = sc.graphics.iter().map(|g| (g.kind, g.measure, g.end_measure)).collect();
    assert!(kinds.contains(&(GraphicKind::Crescendo, 0, 1)), "{kinds:?}");
    assert!(kinds.contains(&(GraphicKind::Slur, 1, 1)), "{kinds:?}");
    // measure 3: accent; measure 4: tenuto; measure 5: ff and a dotted quarter rest
    assert_eq!(ev(&sc, 0, 2)[0].marks, vec![Mark::Accent]);
    assert_eq!(ev(&sc, 0, 3)[0].marks, vec![Mark::Tenuto]);
    assert_eq!(ev(&sc, 0, 4)[0].dynamic.as_deref(), Some("ff"));
    assert!(ev(&sc, 0, 4)[1].is_rest());
    // every measure is exactly full in each voice
    for (s, st) in sc.events.iter().enumerate() {
        for (m, evs) in st.iter().enumerate() {
            for v in 1..=2u8 {
                let total: u32 = evs.iter().filter(|e| e.voice == v).map(|e| e.ticks()).sum();
                assert!(total == 0 || total == sc.measures[m].ticks(), "staff {s} measure {m} voice {v}: {total}");
            }
        }
    }
}

// --- hand-written file 2: lyrics in two verses, chord symbols, a transposing part, a pickup ----------------------

#[test]
fn cantiga_lyrics_chords_and_transposition() {
    let imp = import_str(CANTIGA);
    let sc = &imp.score;
    assert_eq!(sc.text.title, "Cantiga da manhã");
    assert_eq!(sc.text.lyricist, "Fermata Dev 2");
    assert_eq!(sc.staves.len(), 2);
    assert_eq!(sc.staves[1].name, "Trompete em Sib", "♭ becomes b: the interface font has no ♭");
    assert_eq!(sc.staves[1].transpose, -2);
    assert_eq!(sc.staves[1].program, 56);
    assert_eq!(sc.staves[1].channel, 1);
    assert!(sc.staves[0].bracket_with_next && sc.staves[0].barline_to_next);
    // concert key from the voice, not the trumpet's written D major
    assert!(sc.measures.iter().all(|m| m.key == 0 && (m.num, m.den) == (3, 4)));
    assert_eq!(sc.measures[0].tempo, Some(84));
    assert_eq!(sc.measures[0].rehearsal.as_deref(), Some("A"));
    // the pickup's quarter goes to the last beat of measure 1
    let pick = ev(sc, 0, 0);
    assert_eq!(pick.len(), 1);
    assert_eq!(pick[0].start, 2 * TPQ);
    assert_eq!(pick[0].expression.as_deref(), Some("dolce"));
    // lyrics, two verses, hyphen after a syllable that continues, _ for the extender
    let syl = |l: usize| -> Vec<String> { sc.events[0].iter().flatten().map(|e| e.lyric.get(l).cloned().unwrap_or_default()).collect() };
    assert_eq!(syl(0), vec!["Can-", "ta", "o", "ga-", "lo,", "nas-", "ce", "o", "di-", "a"]);
    assert_eq!(syl(1), vec!["So-", "pra", "o", "ven-", "to", "so-", "bre", "o", "mar_", ""]);
    // chord symbols on their notes
    let chords: Vec<_> = sc.events[0].iter().flatten().filter_map(|e| e.chord_symbol.clone()).collect();
    assert_eq!(chords, vec!["C", "Am7", "Fmaj7", "Dm7", "G7", "C/E"]);
    // trumpet: written pitches kept (sounding = written - 2)
    let tp: Vec<String> = sc.events[1].iter().flatten().filter(|e| !e.is_rest()).map(|e| e.pitches[0].name()).collect();
    assert_eq!(tp, vec!["E5", "F#5", "E5", "D5", "E5"]);
    // what was left out is reported
    let s = imp.summary("cantiga.musicxml");
    for k in ["different key signatures per part", "pickup measure", "figured bass", "line or bracket"] {
        assert!(s.contains(k), "{k} missing in: {s}");
    }
    assert!(s.starts_with("Opened cantiga.musicxml (MusicXML). Not imported: "), "{s}");
}

// --- round trips -----------------------------------------------------------------------------------------------

/// A score with the things the exporter writes: triplets, dots, two voices, chords, ties across the bar, grace notes,
/// rests, clef/key/time changes, tempo, a transposing staff, dynamics, marks, lyrics.
fn rich_score() -> Score {
    let mut sc = Score::new(vec![Staff::new("Flauta", Clef::Treble, 73), Staff::new("Clarinete", Clef::Treble, 71), Staff::new("Fagote", Clef::Bass, 70)], 4);
    sc.staves[1].transpose = -2;
    sc.staves[2].set_clef(2, Clef::Tenor);
    sc.measures[0].tempo = Some(100);
    sc.measures[0].key = -3;
    sc.measures[1].key = -3;
    sc.measures[2].key = 2;
    sc.measures[3].key = 2;
    sc.measures[2].num = 3;
    sc.measures[3].num = 3;
    sc.measures[2].tempo = Some(72);
    let p = |m: i32, f: bool| Pitch::from_midi(m, f);
    let q = TPQ;
    let mut put = |s: usize, m: usize, e: Event| sc.events[s][m].push(e);
    // flute: triplet quarters, then a dotted quarter + eighth, tie into measure 2
    for (i, k) in [72, 74, 75].into_iter().enumerate() {
        let mut e = Event::new(1, i as u32 * 2 * q / 3, 3, vec![p(k, true)]);
        e.tuplet = Some((3, 2));
        put(0, 0, e);
    }
    let mut e = Event::new(1, 2 * q, 3, vec![p(77, true)]);
    e.dots = 1;
    e.marks = vec![Mark::Accent];
    e.dynamic = Some("mf".into());
    put(0, 0, e);
    let mut e = Event::new(1, 3 * q + q / 2, 4, vec![p(79, true)]);
    e.ties = vec![true];
    put(0, 0, e);
    let mut e = Event::new(1, 0, 2, vec![p(79, true)]);
    e.lyric = vec!["la".into(), "lu".into()];
    put(0, 1, e);
    let mut g = Event::new(1, 2 * q, 5, vec![p(80, true)]);
    g.grace = true;
    put(0, 1, g);
    put(0, 1, Event::new(1, 2 * q, 3, vec![p(79, true)]));
    put(0, 1, Event::new(1, 3 * q, 3, vec![]));
    // flute voice 2 in measure 2
    let mut e = Event::new(2, 0, 1, vec![p(67, true), p(70, true)]);
    e.stem = Stem::Down;
    put(0, 1, e);
    // clarinet: chord with one tied pitch, quintuplet eighths, double-dotted quarter
    let mut e = Event::new(1, 0, 2, vec![p(62, false), p(65, false), p(69, false)]);
    e.ties = vec![false, false, true];
    put(1, 0, e);
    put(1, 0, Event::new(1, 2 * q, 2, vec![p(62, false), p(65, false), p(69, false)]));
    for i in 0..5u32 {
        let mut e = Event::new(1, i * 2 * q / 5, 4, vec![p(64 + i as i32, false)]);
        e.tuplet = Some((5, 4));
        put(1, 2, e);
    }
    put(1, 2, Event::new(1, 2 * q, 3, vec![p(76, false)]));
    let mut e = Event::new(1, 0, 3, vec![p(76, false)]);
    e.dots = 2;
    put(1, 3, e);
    put(1, 3, Event::new(1, q + 3 * q / 4, 5, vec![p(74, false)]));
    put(1, 3, Event::new(1, 2 * q, 3, vec![]));
    // bassoon: sixteenths, a half rest, then a 3/4 bar in tenor clef
    for i in 0..4u32 {
        put(2, 0, Event::new(1, i * q / 4, 5, vec![p(43 + i as i32, true)]));
    }
    put(2, 0, Event::new(1, q, 3, vec![p(48, true)]));
    put(2, 0, Event::new(1, 2 * q, 2, vec![]));
    let mut e = Event::new(1, 0, 2, vec![p(55, false)]);
    e.dots = 1;
    e.marks = vec![Mark::Fermata];
    put(2, 2, e);
    for s in 0..3 {
        for m in 0..4 {
            sc.sort_measure(s, m);
            for e in sc.events[s][m].iter_mut() {
                e.normalize();
            }
        }
    }
    sc
}

#[test]
fn round_trip_keeps_the_notes_of_a_rich_score() {
    let sc = rich_score();
    let back = round_trip(&sc);
    assert_eq!(notes(&back), notes(&sc));
    assert_eq!(frame(&back), frame(&sc));
    // details the model keeps on both sides
    let fl: Vec<_> = back.events[0][0].iter().map(|e| (e.marks.clone(), e.dynamic.clone())).collect();
    assert!(fl.contains(&(vec![Mark::Accent], Some("mf".into()))));
    assert_eq!(back.events[0][1].iter().find(|e| !e.lyric.is_empty()).unwrap().lyric, vec!["la", "lu"]);
    assert!(back.events[2][2][0].marks.contains(&Mark::Fermata));
    assert_eq!(back.events[0][1].iter().find(|e| e.voice == 2).unwrap().stem, Stem::Down);
    // and once more: the second export is the same text as the first
    assert_eq!(musicxml_string(&back), musicxml_string(&sc));
}

#[test]
fn round_trip_keeps_the_notes_of_the_corpus() {
    for (name, bytes) in [("estudo-em-sol.fmt", ESTUDO_FMT), ("piano.fmt", PIANO_FMT)] {
        let sc = open_bytes(name, bytes).unwrap();
        let back = round_trip(&sc);
        assert!(!notes(&sc).is_empty(), "{name}");
        assert_eq!(notes(&back), notes(&sc), "{name}");
        assert_eq!(frame(&back).0, frame(&sc).0, "{name}");
        assert_eq!(musicxml_string(&back), musicxml_string(&sc), "{name}");
    }
}

#[test]
fn hand_written_files_survive_export_and_reimport() {
    for x in [TERCINAS, CANTIGA] {
        let a = import_str(x).score;
        let b = round_trip(&a);
        assert_eq!(notes(&b), notes(&a));
        assert_eq!(frame(&b), frame(&a));
    }
}

#[test]
fn a_file_from_the_qa_opens_like_its_score() {
    // piano-34.musicxml was exported by Fermata 0.3.3 during QA-1
    let (sc, note) = open_bytes_noted("piano-34.musicxml", PIANO34_XML).unwrap();
    assert!(note.unwrap().starts_with("Opened piano-34.musicxml (MusicXML)"));
    assert_eq!(sc.staves.len(), 2);
    assert!(sc.events.iter().flatten().flatten().any(|e| !e.is_rest()));
    assert_eq!(musicxml_string(&round_trip(&sc)), musicxml_string(&sc));
}

// --- .mxl -------------------------------------------------------------------------------------------------------

/// A small ZIP writer for the tests (stored or deflated entries).
fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = vec![];
    let mut dir = vec![];
    for (name, data, deflate) in files {
        let body = if *deflate { miniz_oxide::deflate::compress_to_vec(data, 6) } else { data.to_vec() };
        let method: u16 = if *deflate { 8 } else { 0 };
        let off = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes()); // version
            h.extend(0u16.to_le_bytes()); // flags
            h.extend(method.to_le_bytes());
            h.extend([0u8; 4]); // time, date
            h.extend(0u32.to_le_bytes()); // crc (not checked by the reader)
            h.extend((body.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            if central {
                h.extend([0u8; 6]); // comment len, disk, internal attrs
                h.extend(0u32.to_le_bytes()); // external attrs
                h.extend(off.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(head(0x04034b50, false));
        out.extend(&body);
        dir.extend(head(0x02014b50, true));
    }
    let cd = out.len() as u32;
    out.extend(&dir);
    out.extend(0x06054b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(cd.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[test]
fn mxl_files_open() {
    let container = br#"<?xml version="1.0" encoding="UTF-8"?><container><rootfiles><rootfile full-path="score/cantiga.musicxml" media-type="application/vnd.recordare.musicxml+xml"/></rootfiles></container>"#;
    for deflate in [true, false] {
        let z = zip(&[("mimetype", b"application/vnd.recordare.musicxml", false), ("META-INF/container.xml", container, deflate), ("score/cantiga.musicxml", CANTIGA.as_bytes(), deflate)]);
        let (sc, note) = open_bytes_noted("cantiga.mxl", &z).unwrap();
        assert_eq!(sc.text.title, "Cantiga da manhã");
        assert_eq!(notes(&sc), notes(&import_str(CANTIGA).score));
        assert!(note.unwrap().contains("cantiga.mxl"));
    }
    // no container.xml: the first score file inside
    let z = zip(&[("estudo.xml", TERCINAS.as_bytes(), true)]);
    assert_eq!(open_bytes("estudo.mxl", &z).unwrap().text.title, "Estudo de tercinas");
}

#[test]
fn timewise_files_open() {
    let x = r#"<score-timewise><part-list><score-part id="P1"><part-name>Flute</part-name></score-part></part-list>
        <measure number="1"><part id="P1"><attributes><divisions>1</divisions><time><beats>2</beats><beat-type>4</beat-type></time></attributes>
        <note><pitch><step>A</step><octave>4</octave></pitch><duration>2</duration><type>half</type></note></part></measure>
        <measure number="2"><part id="P1"><note><pitch><step>B</step><octave>4</octave></pitch><duration>2</duration><type>half</type></note></part></measure>
        </score-timewise>"#;
    let sc = import_str(x).score;
    assert_eq!(sc.staves[0].name, "Flute");
    assert_eq!(sc.events[0][1][0].pitches[0].name(), "B4");
    assert_eq!((sc.measures[0].num, sc.measures[0].den), (2, 4));
}

// --- damaged files never break the app ---------------------------------------------------------------------------

#[test]
fn damaged_files_are_refused_without_breaking() {
    let good = TERCINAS.as_bytes();
    let mut cases: Vec<(String, Vec<u8>)> = vec![
        ("empty.musicxml".into(), vec![]),
        ("text.musicxml".into(), b"hello".to_vec()),
        ("other.xml".into(), b"<html><body/></html>".to_vec()),
        ("unclosed.musicxml".into(), b"<score-partwise><part-list>".to_vec()),
        ("mismatch.musicxml".into(), b"<score-partwise><part></measure></score-partwise>".to_vec()),
        ("noparts.musicxml".into(), b"<score-partwise><part-list/></score-partwise>".to_vec()),
        ("deep.musicxml".into(), format!("<score-partwise>{}", "<a>".repeat(100_000)).into_bytes()),
        ("bad.mxl".into(), b"PK\x03\x04garbage".to_vec()),
        ("bomb.mxl".into(), zip(&[("x.xml", &vec![b' '; 70 << 20], true)])),
        ("junk.mxl".into(), (0..5000u32).map(|i| (i * 31 + 7) as u8).collect()),
    ];
    // every prefix of a real file, and the file with bytes flipped
    for cut in (0..good.len()).step_by(97) {
        cases.push((format!("cut{cut}.musicxml"), good[..cut].to_vec()));
    }
    for i in (0..good.len()).step_by(53) {
        let mut b = good.to_vec();
        b[i] = b"<>/=\"x&;"[i % 8];
        cases.push((format!("flip{i}.musicxml"), b));
    }
    // a valid file with values out of range
    let silly = TERCINAS.replace("<duration>20</duration>", "<duration>-5</duration>").replace("<divisions>60</divisions>", "<divisions>0</divisions>").replace("<octave>5</octave>", "<octave>99</octave>").replace("<voice>2</voice>", "<voice>9999</voice>").replace("<staff>2</staff>", "<staff>40</staff>").replace("<beats>6</beats>", "<beats>0</beats>");
    cases.push(("silly.musicxml".into(), silly.into_bytes()));
    for (name, bytes) in &cases {
        let r = std::panic::catch_unwind(|| open_bytes_noted(name, bytes));
        assert!(r.is_ok(), "{name} made the app panic");
    }
    for name in ["empty.musicxml", "text.musicxml", "other.xml", "unclosed.musicxml", "mismatch.musicxml", "noparts.musicxml", "deep.musicxml", "bad.mxl", "bomb.mxl", "junk.mxl"] {
        let (_, b) = cases.iter().find(|c| c.0 == name).unwrap();
        assert!(open_bytes(name, b).is_err(), "{name} was taken for a score");
    }
}

#[test]
fn other_formats_still_open_as_before() {
    assert!(open_bytes("estudo.fmt", ESTUDO_FMT).is_ok());
    assert!(!is_musicxml("estudo.fmt", ESTUDO_FMT));
    assert!(!is_musicxml("song.mid", b"MThd\0\0\0\x06"));
    assert!(is_musicxml("whatever", TERCINAS.as_bytes()), "recognised by content (the web picker has no filter)");
}
