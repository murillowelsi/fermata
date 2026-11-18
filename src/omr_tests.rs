//! F-31 tests. The real Audiveris runs only when it is installed (usual places, or FERMATA_AUDIVERIS); the QA's
//! pictures, reference scores and saved Audiveris outputs are read from qa/F-31 (or FERMATA_OMR_SAMPLES) when there.
use super::*;
use std::time::Duration;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermata-omr-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A stand-in for Audiveris: a shell script that writes `score` where Audiveris would (`<out>/<stem>/<stem>.mxl`),
/// after `delay` seconds.
#[cfg(unix)]
fn fake_audiveris(dir: &Path, score: Option<&Path>, delay: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("Audiveris");
    let copy = match score {
        Some(s) => format!("mkdir -p \"$out/$stem\" && cp \"{}\" \"$out/$stem/$stem.mxl\"", s.display()),
        None => "echo 'WARN no staves found'".into(),
    };
    let body = format!("#!/bin/sh\n[ \"$1 $2 $3 $4\" = \"-batch -transcribe -export -output\" ] || exit 9\nout=\"$5\"; f=\"$7\"; stem=$(basename \"$f\"); stem=${{stem%.*}}\nsleep {delay}\n{copy}\n");
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn samples() -> Option<PathBuf> {
    let d = std::env::var_os("FERMATA_OMR_SAMPLES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../qa/F-31"));
    d.join("gabarito").is_dir().then_some(d)
}

fn picture(dir: &Path) -> PathBuf {
    let p = dir.join("page.png");
    std::fs::write(&p, [0x89, b'P', b'N', b'G']).unwrap();
    p
}

fn run(program: PathBuf, timeout: u64) -> OmrRun {
    OmrRun { program, timeout: Duration::from_secs(timeout), home: Some(std::env::temp_dir().join("fermata-omr-test-home")) }
}

// --- finding it --------------------------------------------------------------------------------------------------

#[test]
fn the_program_is_found_from_the_app_bundle_or_the_preferences() {
    let d = tmp("find");
    let app = d.join("Audiveris.app/Contents/MacOS");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("Audiveris"), b"").unwrap();
    let want = app.join("Audiveris");
    // the bundle, its folder, or the program itself, as saved in the preferences
    for p in [d.join("Audiveris.app"), d.clone(), want.clone()] {
        assert_eq!(program_in(&p), Some(want.clone()), "{}", p.display());
        assert_eq!(find_audiveris(Some(p.to_str().unwrap())), Some(want.clone()));
    }
    assert_eq!(program_in(&d.join("nothing")), None);
    assert!(!default_locations().is_empty());
}

#[test]
fn not_installed_says_so() {
    let m = OmrError::NotInstalled.to_string();
    assert!(m.contains("isn't available"));
}

// --- running it (a stand-in program) -------------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_run_gives_the_score_audiveris_wrote() {
    let d = tmp("run");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/musicxml/estudo-tercinas.musicxml");
    let job = start(run(fake_audiveris(&d, Some(&src), 0), 30), &picture(&d));
    let out = job.wait().expect("score");
    assert_eq!(out.name, "page.mxl");
    let (sc, note) = to_score(&out).unwrap();
    assert!(note.starts_with(CHECK_IT), "{note}");
    assert_eq!(sc.staves.len(), 2);
    assert_eq!(sc.text.title, "Estudo de tercinas");
}

#[cfg(unix)]
#[test]
fn no_music_cancel_and_timeout_end_cleanly() {
    let d = tmp("ends");
    // nothing written
    let job = start(run(fake_audiveris(&d, None, 0), 30), &picture(&d));
    assert!(matches!(job.wait(), Err(OmrError::NoScore(_))));
    // too slow
    let d2 = tmp("slow");
    let t = Instant::now();
    let job = start(run(fake_audiveris(&d2, None, 30), 1), &picture(&d2));
    assert_eq!(job.wait().unwrap_err(), OmrError::TimedOut(1));
    assert!(t.elapsed() < Duration::from_secs(10));
    // cancelled while reading
    let d3 = tmp("cancel");
    let job = start(run(fake_audiveris(&d3, None, 30), 60), &picture(&d3));
    std::thread::sleep(Duration::from_millis(300));
    assert!(job.try_result().is_none(), "still reading");
    job.cancel();
    assert_eq!(job.wait().unwrap_err(), OmrError::Cancelled);
}

#[test]
fn pictures_fermata_cannot_read_are_refused() {
    let d = tmp("types");
    let txt = d.join("notes.txt");
    std::fs::write(&txt, b"x").unwrap();
    let job = start(run(d.join("no-program"), 5), &txt);
    assert!(matches!(job.wait(), Err(OmrError::Unsupported(_))));
    // a missing program is an error, not a crash
    let job = start(run(d.join("no-program"), 5), &picture(&d));
    assert!(matches!(job.wait(), Err(OmrError::Failed(_))));
}

// --- the piano put back together -----------------------------------------------------------------------------------

fn staff_with(name: &str, clef: Clef, measures: &[usize], key: i32, n: usize) -> (Staff, Vec<Vec<Event>>) {
    let st = Staff::new(name, clef, 0);
    let mut evs = vec![vec![]; n];
    for &m in measures {
        evs[m].push(Event::new(1, 0, 1, vec![Pitch::from_midi(key, false)]));
    }
    (st, evs)
}

fn score_of(parts: Vec<(Staff, Vec<Vec<Event>>)>, n: usize) -> Score {
    let staves: Vec<Staff> = parts.iter().map(|p| p.0.clone()).collect();
    let mut sc = Score::new(staves, n);
    sc.events = parts.into_iter().map(|p| p.1).collect();
    sc
}

#[test]
fn a_piano_split_by_system_is_joined_into_one_grand_staff() {
    // like the QA study: "Piano" (2 staves) plays measures 4-6, two single "Voice" staves the rest
    let n = 8;
    let (mut a, ea) = staff_with("Piano", Clef::Treble, &[3, 4, 5], 72, n);
    a.brace_with_next = true;
    let (b, eb) = staff_with("", Clef::Bass, &[3, 4, 5], 48, n);
    let (c, ec) = staff_with("Voice", Clef::Treble, &[0, 1, 2, 6, 7], 74, n);
    let (d, ed) = staff_with("Voice", Clef::Bass, &[0, 1, 2, 6, 7], 50, n);
    let mut sc = score_of(vec![(a, ea), (b, eb), (c, ec), (d, ed)], n);
    assert_eq!(join_split_staves(&mut sc), 2);
    assert_eq!(sc.staves.len(), 2);
    assert!(sc.staves[0].brace_with_next);
    assert_eq!(sc.staves[0].name, "Piano");
    for m in 0..n {
        assert_eq!(sc.events[0][m][0].pitches[0].midi(), if (3..6).contains(&m) { 72 } else { 74 }, "upper m{m}");
        assert_eq!(sc.events[1][m][0].pitches[0].midi(), if (3..6).contains(&m) { 48 } else { 50 }, "lower m{m}");
    }
}

#[test]
fn two_simultaneous_parts_become_a_grand_staff() {
    // like the QA chorale: treble and bass read as two "Voice" parts
    let (a, ea) = staff_with("Voice", Clef::Treble, &[0, 1], 67, 2);
    let (b, eb) = staff_with("Voice", Clef::Bass, &[0, 1], 48, 2);
    let mut sc = score_of(vec![(a, ea), (b, eb)], 2);
    assert_eq!(join_split_staves(&mut sc), 0);
    assert!(sc.staves[0].brace_with_next && sc.staves[0].barline_to_next);
    assert_eq!(sc.staves[0].name, "Piano");
}

#[test]
fn different_instruments_are_not_joined() {
    // a flute in the first half and a violin in the second never play together, but aren't one staff
    let (a, ea) = staff_with("Flute", Clef::Treble, &[0, 1], 76, 4);
    let (b, eb) = staff_with("Violin", Clef::Treble, &[2, 3], 69, 4);
    let mut sc = score_of(vec![(a, ea), (b, eb)], 4);
    assert_eq!(join_split_staves(&mut sc), 0);
    assert_eq!(sc.staves.len(), 2);
    assert!(!sc.staves[0].brace_with_next);
}

// --- the QA's material ---------------------------------------------------------------------------------------------

fn reference(dir: &Path, piece: &str) -> Score {
    let b = std::fs::read(dir.join(format!("gabarito/{piece}-gabarito.musicxml"))).unwrap();
    crate::musicxml::import("ref.musicxml", &b).unwrap().score
}

#[test]
fn audiveris_outputs_saved_by_the_qa_open_as_one_piano() {
    let Some(dir) = samples() else { return };
    let saved = dir.join("saida-audiveris");
    if !saved.is_dir() {
        return;
    }
    for (file, piece, staves) in [("estudo-limpa", "estudo", 2), ("estudo-foto", "estudo", 2), ("coral-limpa", "coral", 2), ("coral-foto", "coral", 2), ("melodia-limpa", "melodia", 1)] {
        let bytes = std::fs::read(saved.join(format!("{file}.mxl"))).unwrap();
        let (sc, note) = to_score(&OmrOutput { name: format!("{file}.mxl"), bytes, seconds: 20.0, log: String::new() }).unwrap();
        assert_eq!(sc.staves.len(), staves, "{file}: {:?}", sc.staves.iter().map(|s| &s.name).collect::<Vec<_>>());
        if staves == 2 {
            assert!(sc.staves[0].brace_with_next, "{file}");
        }
        assert!(note.starts_with(CHECK_IT));
        let (ok, total) = pitch_match(&reference(&dir, piece), &sc);
        eprintln!("{file}: {ok}/{total} pitches ({:.1}%)", 100.0 * ok as f64 / total as f64);
        assert!(ok as f64 >= 0.7 * total as f64, "{file}: {ok}/{total}");
    }
}

/// The real thing: Audiveris on the QA's six pictures, timed, with the pitch score against the reference. Runs only
/// when Audiveris is installed (usual places or FERMATA_AUDIVERIS) and the pictures are there; ~25 s a picture.
#[test]
fn audiveris_reads_the_qa_pictures() {
    let (Some(program), Some(dir)) = (find_audiveris(None), samples()) else {
        eprintln!("Audiveris or the QA pictures not found: skipped");
        return;
    };
    let mut lines = vec![];
    let (mut clean, mut photo) = ((0, 0), (0, 0));
    for piece in ["estudo", "coral", "melodia"] {
        for (kind, ext) in [("limpa", "png"), ("foto", "jpg")] {
            let pic = dir.join(format!("imagens/{piece}-{kind}.{ext}"));
            let job = start(OmrRun { program: program.clone(), timeout: Duration::from_secs(600), home: Some(std::env::temp_dir().join("fermata-omr-test-home")) }, &pic);
            let out = match job.wait() {
                Ok(o) => o,
                Err(e) => {
                    let l = format!("{piece}-{kind}: not read ({e})");
                    eprintln!("{l}");
                    lines.push(l);
                    continue;
                }
            };
            let (sc, _) = to_score(&out).unwrap();
            let (ok, total) = pitch_match(&reference(&dir, piece), &sc);
            let l = format!("{piece}-{kind}: {ok}/{total} pitches ({:.1}%), {} staves, {:.1} s", 100.0 * ok as f64 / total as f64, sc.staves.len(), out.seconds);
            eprintln!("{l}");
            lines.push(l);
            let acc = if kind == "limpa" { &mut clean } else { &mut photo };
            acc.0 += ok;
            acc.1 += total;
        }
    }
    let pc = |x: (usize, usize)| 100.0 * x.0 as f64 / x.1.max(1) as f64;
    lines.push(format!("clean {:.1}%, photo {:.1}%", pc(clean), pc(photo)));
    let report = lines.join("\n");
    eprintln!("{report}");
    if let Ok(p) = std::env::var("FERMATA_OMR_REPORT") {
        let _ = std::fs::write(p, &report);
    }
    assert!(!lines.iter().any(|l| l.contains("not read")), "{report}");
    assert!(pc(clean) >= 80.0 && pc(photo) >= 70.0, "{report}");
}

// --- the review after reading (F-31 UI) ----------------------------------------------------------------------------

#[test]
fn measures_whose_count_does_not_add_up_are_found_and_fixed_ones_are_not() {
    let mut a = crate::app::App::new(true);
    let sc = &mut a.score;
    // measure 1: a whole note (full); measure 2: a half note only (short); measure 3: three halves (too long)
    sc.events[0][0] = vec![Event::new(1, 0, 1, vec![Pitch::from_midi(60, false)])];
    sc.events[0][1] = vec![Event::new(1, 0, 2, vec![Pitch::from_midi(62, false)])];
    sc.events[0][2] = (0..3).map(|i| Event::new(1, i * 2 * TPQ, 2, vec![Pitch::from_midi(64, false)])).collect();
    use crate::app::CheckProblem::*;
    // (the default score has two staves: the lower one is empty in these measures, so they would also be "missing";
    // the count comes first)
    assert_eq!(a.count_problems(), vec![(0, Missing), (1, Count), (2, Count)]);
    // fixing measure 2 removes its count mark; filling the lower staff of measure 1 removes its "missing"
    a.score.events[0][1].push(Event::new(1, 2 * TPQ, 2, vec![Pitch::from_midi(64, false)]));
    a.score.events[1][0] = vec![Event::new(1, 0, 1, vec![Pitch::from_midi(48, false)])];
    assert_eq!(a.count_problems(), vec![(1, Missing), (2, Count)]);
    a.score.events[1][1] = vec![Event::new(1, 0, 1, vec![Pitch::from_midi(48, false)])];
    assert_eq!(a.count_problems(), vec![(2, Count)]);
}

#[test]
fn the_photo_strip_goes_at_the_first_edit_and_with_a_new_score() {
    let mut a = crate::app::App::new(true);
    a.replace_score(crate::app::Then::Omr(Box::new(Score::default_score()), "Imported from x.jpg · 16 measures".into()));
    assert!(a.photo_banner && a.photo_check && a.dirty);
    assert_eq!(a.status, "Imported from x.jpg · 16 measures");
    a.enter(0, 0, 0, Some(Pitch::from_midi(60, false)), 3, 0, false);
    assert!(!a.photo_banner, "an edit ends the strip");
    assert!(a.photo_check, "the marks stay while the score is open");
    a.dirty = false;
    a.replace_score(crate::app::Then::NewDefault);
    assert!(!a.photo_check);
}

#[test]
fn the_new_menu_key_is_ctrl_shift_i_without_conflicts() {
    use crate::commands::{shortcut, Cmd};
    let s = shortcut(Cmd::ImportPhoto).unwrap();
    assert!(s.modifiers.ctrl && s.modifiers.shift && !s.modifiers.alt);
    assert_eq!(s.logical_key, egui::Key::I);
    let all = crate::commands::all_shortcuts();
    let same: Vec<_> = all.iter().filter(|(k, c)| *k == s && *c != Cmd::ImportPhoto).collect();
    assert!(same.is_empty(), "{same:?}");
}
