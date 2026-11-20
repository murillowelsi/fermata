//! F-09 tests: D.C., D.S., Fine, To Coda and Coda in the play order, the old segno/coda note marks, MusicXML.
use super::*;

/// `n` measures of 4/4, each a whole note C5 (so playback has something to time).
fn piece(n: usize) -> Score {
    let mut sc = Score::new(vec![Staff::new("Piano", Clef::Treble, 0)], n);
    for m in 0..n {
        sc.events[0][m] = vec![Event::new(1, 0, 1, vec![Pitch::from_midi(72, false)])];
    }
    sc
}

fn phrase(sc: &mut Score, m: usize, text: &str) {
    sc.measures[m].coda_phrase = Some(text.into());
}

#[test]
fn the_phrases_are_read_in_any_spelling() {
    use Until::*;
    assert_eq!(Jump::of("D.C."), Some(Jump::Back { segno: false, until: End }));
    assert_eq!(Jump::of("Da Capo al Fine"), Some(Jump::Back { segno: false, until: Fine }));
    assert_eq!(Jump::of("d.c. al coda"), Some(Jump::Back { segno: false, until: Coda }));
    assert_eq!(Jump::of("D.S."), Some(Jump::Back { segno: true, until: End }));
    assert_eq!(Jump::of("Dal Segno al Fine"), Some(Jump::Back { segno: true, until: Fine }));
    assert_eq!(Jump::of("D. S. al Coda"), Some(Jump::Back { segno: true, until: Coda }));
    assert_eq!(Jump::of("Fine"), Some(Jump::Fine));
    assert_eq!(Jump::of("To Coda"), Some(Jump::ToCoda));
    assert_eq!(Jump::of("dolce"), None);
    for p in Jump::PHRASES {
        assert!(Jump::of(p).is_some(), "{p}");
    }
}

#[test]
fn da_capo_goes_back_to_the_start() {
    let mut sc = piece(4);
    phrase(&mut sc, 3, "D.C.");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 0, 1, 2, 3]);
}

#[test]
fn da_capo_al_fine_stops_at_fine() {
    let mut sc = piece(4);
    phrase(&mut sc, 1, "Fine");
    phrase(&mut sc, 3, "D.C. al Fine");
    // the first time through, Fine is passed by
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 0, 1]);
}

#[test]
fn da_capo_al_coda_goes_from_to_coda_to_the_coda() {
    let mut sc = piece(6);
    phrase(&mut sc, 1, "To Coda");
    phrase(&mut sc, 3, "D.C. al Coda");
    sc.measures[4].coda = true;
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 0, 1, 4, 5]);
}

#[test]
fn dal_segno_goes_back_to_the_segno() {
    let mut sc = piece(4);
    sc.measures[1].segno = true;
    phrase(&mut sc, 3, "D.S.");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 1, 2, 3]);
}

#[test]
fn dal_segno_al_fine_stops_at_fine() {
    let mut sc = piece(4);
    sc.measures[1].segno = true;
    phrase(&mut sc, 2, "Fine");
    phrase(&mut sc, 3, "D.S. al Fine");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 1, 2]);
}

#[test]
fn dal_segno_al_coda_goes_from_to_coda_to_the_coda() {
    let mut sc = piece(6);
    sc.measures[1].segno = true;
    phrase(&mut sc, 2, "To Coda");
    phrase(&mut sc, 3, "D.S. al Coda");
    sc.measures[4].coda = true;
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 1, 2, 4, 5]);
}

/// | 1 |: 2 | 3 (1.) :| 4 (2.) Fine | 5 D.C. al Fine |: the repeat and the first ending the first time; after the
/// jump the repeat isn't taken again and only the last ending plays.
#[test]
fn after_a_jump_repeats_are_not_taken_and_the_last_ending_plays() {
    let mut sc = piece(5);
    sc.measures[0].barline = Barline::RepeatStart;
    sc.measures[2].ending = Some("1.".into());
    sc.measures[2].barline = Barline::RepeatEnd;
    sc.measures[3].ending = Some("2.".into());
    phrase(&mut sc, 3, "Fine");
    phrase(&mut sc, 4, "D.C. al Fine");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 1, 3, 4, 0, 1, 3]);
}

/// A repeat in the measure of the D.S. is played out first; then the jump.
#[test]
fn a_repeat_ending_on_the_jump_measure_is_played_first() {
    let mut sc = piece(4);
    sc.measures[1].segno = true;
    sc.measures[0].barline = Barline::RepeatStart;
    sc.measures[3].barline = Barline::RepeatEnd;
    phrase(&mut sc, 3, "D.S.");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 1, 2, 3, 1, 2, 3]);
}

#[test]
fn jumps_never_loop() {
    // a D.S. with no segno anywhere: nothing to go back to
    let mut sc = piece(3);
    phrase(&mut sc, 2, "D.S.");
    assert_eq!(play_order(&sc), vec![0, 1, 2]);
    // two D.C.: only the first is taken
    let mut sc = piece(4);
    phrase(&mut sc, 1, "D.C.");
    phrase(&mut sc, 3, "D.C.");
    assert_eq!(play_order(&sc), vec![0, 1, 0, 1, 2, 3]);
    // To Coda with no coda: it plays on
    let mut sc = piece(4);
    phrase(&mut sc, 1, "To Coda");
    phrase(&mut sc, 3, "D.C. al Coda");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 3, 0, 1, 2, 3]);
    // a D.S. that goes back to itself (segno on the D.S. measure): once
    let mut sc = piece(3);
    sc.measures[2].segno = true;
    phrase(&mut sc, 2, "D.S.");
    assert_eq!(play_order(&sc), vec![0, 1, 2, 2]);
}

/// Older files put Segno and Coda on a note: they count as the measure's.
#[test]
fn the_old_segno_and_coda_note_marks_count_as_the_measures() {
    let mut sc = piece(6);
    sc.events[0][1][0].marks.push(Mark::Segno);
    sc.events[0][4][0].marks.push(Mark::Coda);
    phrase(&mut sc, 2, "To Coda");
    phrase(&mut sc, 3, "D.S. al Coda");
    // through a file written before F-09: no measure-level segno or coda in it
    let json = serde_json::to_string(&sc).unwrap();
    assert!(!json.contains("\"segno\"") && !json.contains("\"coda\""), "nothing new written when unset");
    let back: Score = serde_json::from_str(&json).unwrap();
    assert!(back.has_segno(1) && back.has_coda(4));
    assert_eq!(play_order(&back), vec![0, 1, 2, 3, 1, 2, 4, 5]);
}

#[test]
fn the_fmt_file_keeps_the_signs_and_phrases() {
    let mut sc = piece(6);
    sc.measures[1].segno = true;
    sc.measures[4].coda = true;
    phrase(&mut sc, 2, "To Coda");
    phrase(&mut sc, 3, "D.S. al Coda");
    let back: Score = serde_json::from_str(&serde_json::to_string(&sc).unwrap()).unwrap();
    assert_eq!(back.measures, sc.measures);
    assert_eq!(play_order(&back), play_order(&sc));
}

/// Playing from the cursor in the coda starts where the coda is played: after the jump.
#[test]
fn playing_from_the_cursor_in_the_coda_starts_after_the_jump() {
    let mut sc = piece(6);
    sc.measures[1].segno = true;
    phrase(&mut sc, 2, "To Coda");
    phrase(&mut sc, 3, "D.S. al Coda");
    sc.measures[4].coda = true;
    let (_, tm, start) = build_at(&sc, 5, 0);
    assert_eq!(tm.order, vec![0, 1, 2, 3, 1, 2, 4, 5]);
    // measure 6 is the 8th measure played
    assert!((start - tm.measures[7].0).abs() < 1e-9, "starts at {start}, measure 6 at {}", tm.measures[7].0);
    // and from measure 2 (before the jump), the first time it is played
    let (_, tm, start) = build_at(&sc, 1, 0);
    assert!((start - tm.measures[1].0).abs() < 1e-9);
}

#[test]
fn musicxml_round_trip_keeps_the_jumps() {
    let mut sc = piece(6);
    sc.measures[1].segno = true;
    sc.measures[4].coda = true;
    phrase(&mut sc, 2, "To Coda");
    phrase(&mut sc, 3, "D.S. al Coda");
    let xml = crate::io::musicxml_string(&sc);
    assert!(xml.contains("<segno/>") && xml.contains("<coda/>"));
    assert!(xml.contains("<sound dalsegno=\"segno\"/>") && xml.contains("<sound tocoda=\"coda\"/>"));
    let back = crate::musicxml::import("j.musicxml", xml.as_bytes()).unwrap().score;
    let marks = |s: &Score| s.measures.iter().map(|m| (m.segno, m.coda, m.coda_phrase.clone())).collect::<Vec<_>>();
    assert_eq!(marks(&back), marks(&sc));
    assert_eq!(play_order(&back), vec![0, 1, 2, 3, 1, 2, 4, 5]);
}

/// Other programs: a jump given only by `<sound>`, and a "To Coda" that carries the coda sign as well.
#[test]
fn musicxml_from_other_programs() {
    let m = |n: usize, body: &str| format!("<measure number=\"{n}\">{body}<note><rest measure=\"yes\"/><duration>4</duration></note></measure>");
    let xml = format!(
        "<?xml version=\"1.0\"?><score-partwise version=\"3.1\"><part-list><score-part id=\"P1\"><part-name>P</part-name></score-part></part-list><part id=\"P1\">{}{}{}{}{}</part></score-partwise>",
        m(1, "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>"),
        m(2, "<direction><direction-type><words>To Coda</words></direction-type><direction-type><coda/></direction-type><sound tocoda=\"coda\"/></direction>"),
        m(3, "<direction><sound dacapo=\"yes\"/></direction>"),
        m(4, "<direction><direction-type><coda/></direction-type></direction>"),
        m(5, ""),
    );
    let sc = crate::musicxml::import("o.musicxml", xml.as_bytes()).unwrap().score;
    assert!(!sc.measures[1].coda, "the To Coda's sign is not the coda");
    assert_eq!(sc.measures[1].coda_phrase.as_deref(), Some("To Coda"));
    assert_eq!(sc.measures[2].coda_phrase.as_deref(), Some("D.C."));
    assert!(sc.measures[3].coda);
}
