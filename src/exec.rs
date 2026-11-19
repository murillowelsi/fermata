//! Running a menu command.
use crate::app::*;
use crate::commands::Cmd;
use crate::model::*;

impl App {
    pub fn exec(&mut self, c: Cmd) {
        use Cmd::*;
        let (cs, cm, _ct) = self.cursor;
        let mrange = self.sel_measures.unwrap_or((cm, cm));
        match c {
            New => self.dialog = Some(Dialog::NewScore { staves: 2, systems: 5, mps: 3, c_score: false, handwritten: false, template: 0 }),
            // everything that replaces the score asks first when it has unsaved changes (App::replace_score)
            NewDefault => self.replace_score(Then::NewDefault),
            Open => self.open(),
            Close => self.replace_score(Then::Close),
            Save => {
                self.save(false);
            }
            SaveAs => {
                self.save(true);
            }
            Revert => match self.path.clone() {
                Some(p) => self.replace_score(Then::Revert(p)),
                None => self.status = "This score hasn't been saved yet.".into(),
            },
            Print => self.print(),
            ExportPng => self.export_png_requested = true,
            ExportPdf => self.export_pdf(),
            PageSetup => {
                let l = &self.score.layout;
                let margins = l.margins().map(crate::pdf::pt_to_mm);
                let same = margins.iter().all(|m| (m - margins[0]).abs() < 0.05);
                self.dialog = Some(Dialog::PageSetup { w: l.page_w, h: l.page_h, margins, same, scale: l.print_scale, staff_space: l.staff_space, staff_gap: l.staff_gap, system_gap: l.system_gap, page: 0 });
            }
            ImportMidi => self.replace_score(Then::ImportMidi),
            ImportXml => self.replace_score(Then::ImportXml),
            ExportMidi => {
                let b = crate::io::midi_bytes(&self.score);
                self.export_bytes("mid", "MIDI file", &b);
            }
            ExportEnc => self.export_enc(),
            ExportXml => {
                let b = crate::io::musicxml_string(&self.score).into_bytes();
                self.export_bytes("musicxml", "MusicXML", &b);
            }
            ExtractPart => self.extract_part(cs),
            Quit => self.replace_score(Then::Quit),
            Undo | UndoAlt => self.do_undo(),
            CutAlt => self.copy(true),
            CopyAlt => self.copy(false),
            PasteAlt => self.paste(),
            SimplifyAccidentals => {
                let flats = self.score.measures[cm].key < 0;
                self.modify(|e| {
                    for p in e.pitches.iter_mut() {
                        if p.alter.abs() == 2 || (p.alter != 0 && (p.letter() == 2 || p.letter() == 6 || p.letter() == 0 || p.letter() == 3) && ((p.alter > 0 && (p.letter() == 2 || p.letter() == 6)) || (p.alter < 0 && (p.letter() == 0 || p.letter() == 3)))) {
                            *p = Pitch::from_midi(p.midi(), flats);
                        }
                    }
                    e.sort_pitches();
                });
            }
            RemoveAllMarks => {
                self.modify(|e| e.marks.clear());
            }
            OpenAllPalettes => {
                self.palettes = [true; 11];
                self.floating_hidden = false;
            }
            SpacingDefaults => {
                let l = &self.score.layout;
                self.dialog = Some(Dialog::SpacingDefaults { stem: l.stem_len_hs, beam: l.beam_thick_pct, tie_h: l.tie_h_qs, tie_v: l.tie_v_qs, ms: l.measure_space_hs });
            }
            AlignSpacingPrev => self.exec(AlignSpacing),
            Redo => self.do_redo(),
            Cut => self.copy(true),
            Copy => self.copy(false),
            Paste => self.paste(),
            Clear => {
                if !self.sel_graphics.is_empty() {
                    self.checkpoint();
                    let mut v: Vec<usize> = self.sel_graphics.drain().collect();
                    v.sort();
                    for i in v.into_iter().rev() {
                        self.score.graphics.remove(i);
                    }
                } else {
                    let t = self.targets();
                    self.delete_refs(&t);
                }
            }
            SelectAll => self.select_all(),
            NudgeLeft | NudgeRight | NudgeUp | NudgeDown => {
                let (dx, dy) = match c {
                    NudgeLeft => (-0.25, 0.0),
                    NudgeRight => (0.25, 0.0),
                    NudgeUp => (0.0, -0.25),
                    _ => (0.0, 0.25),
                };
                if !self.sel_graphics.is_empty() {
                    self.checkpoint();
                    for &i in &self.sel_graphics {
                        if let Some(g) = self.score.graphics.get_mut(i) {
                            g.dx += dx;
                            g.dy += dy;
                            g.end_dy += dy;
                        }
                    }
                } else if dy != 0.0 {
                    self.transpose_diatonic(if dy < 0.0 { 1 } else { -1 });
                } else {
                    let grid = base_ticks(5) as i64;
                    let d = if dx < 0.0 { -grid } else { grid };
                    let n = self.modify(|e| e.start = (e.start as i64 + d).max(0) as u32);
                    if n > 0 {
                        for s in 0..self.score.staves.len() {
                            for m in 0..self.score.measures.len() {
                                self.score.sort_measure(s, m);
                            }
                        }
                        self.selected.clear();
                    }
                }
            }
            NoteAttributes => {
                if let Some(r) = self.targets().first().copied() {
                    let e = self.ev(r).cloned().unwrap();
                    let notes: Vec<Event> = self.targets().iter().filter_map(|r| self.ev(*r).cloned()).collect();
                    let len = |x: &Event| x.play_len.unwrap_or(x.ticks());
                    let init = crate::app::AttrInit {
                        play_len: len(&e),
                        value: e.value,
                        dots: e.dots,
                        mixed: [notes.iter().any(|x| len(x) != len(&e)), notes.iter().any(|x| x.value != e.value), notes.iter().any(|x| x.dots != e.dots)],
                    };
                    self.dialog = Some(Dialog::NoteAttributes { velocity: e.velocity, play_len: len(&e), value: e.value, dots: e.dots, voice: e.voice, hidden: e.hidden, stem: e.stem, init });
                } else {
                    self.status = "Select a note first.".into();
                }
            }
            Voice(n) => self.set_voice(n),
            Enharmonic => {
                self.modify(|e| {
                    for p in e.pitches.iter_mut() {
                        *p = p.enharmonic();
                    }
                    e.sort_pitches();
                });
            }
            AccToSharps | AccToFlats => {
                let flats = c == AccToFlats;
                self.modify(|e| {
                    for p in e.pitches.iter_mut() {
                        if p.alter != 0 {
                            *p = Pitch::from_midi(p.midi(), flats);
                        }
                    }
                    e.sort_pitches();
                });
            }
            StemUp => {
                self.modify(|e| e.stem = Stem::Up);
            }
            StemDown => {
                self.modify(|e| e.stem = Stem::Down);
            }
            StemNormal => {
                self.modify(|e| e.stem = Stem::Auto);
            }
            AddMark => {
                let mk = match &self.item {
                    Item::Mark(m) => *m,
                    _ => Mark::Accent,
                };
                self.modify(|e| {
                    if !e.marks.contains(&mk) {
                        e.marks.push(mk)
                    }
                });
            }
            RemoveMark => {
                self.modify(|e| {
                    e.marks.clear();
                    e.dynamic = None;
                    e.expression = None;
                });
            }
            Tie => self.tie_selection(),
            FlipTie => {
                self.modify(|e| e.tie_flip = !e.tie_flip);
            }
            SlurAbove => self.slur_selection(false),
            SlurBelow => self.slur_selection(true),
            BeamGroup => {
                // the original groups an odd number of notes (3, 5, 6, 7...) into a tuplet with its number above,
                // of any value; a power-of-two number of short notes is beamed together (6/8 eighths stay beamed)
                let t = self.targets();
                let compound = t.first().map(|r| {
                    let mi = &self.score.measures[r.measure];
                    mi.den >= 8 && mi.num % 3 == 0
                }).unwrap_or(false);
                let mut per: std::collections::HashMap<(usize, usize, u8), std::collections::HashSet<u32>> = Default::default();
                let mut all_short = true;
                for r in &t {
                    if let Some(e) = self.ev(*r) {
                        per.entry((r.staff, r.measure, e.voice)).or_default().insert(e.start);
                        all_short &= e.value >= 4;
                    }
                }
                let n = per.values().map(|v| v.len()).max().unwrap_or(0);
                let already = t.iter().all(|r| self.ev(*r).map(|e| e.tuplet.is_some()).unwrap_or(false));
                if !already && crate::app::App::auto_tuplet(n).is_some() && !(compound && all_short && n % 3 == 0) {
                    self.make_tuplet(None);
                    return;
                }
                let t = self.targets();
                let last = t.last().copied();
                self.modify(|e| e.beam = BeamMode::Join);
                if let Some(l) = last {
                    if let Some(e) = self.ev_mut(l) {
                        e.beam = BeamMode::Auto;
                    }
                }
                if let Some(f) = t.first().copied() {
                    if let Some(e) = self.ev_mut(f) {
                        if t.len() > 1 {
                            e.beam = BeamMode::Join;
                        }
                    }
                }
                self.status = "Beamed as one group.".into();
            }
            BeamOnBeat => {
                self.modify(|e| e.beam = BeamMode::Auto);
            }
            SubGroupBeam => {
                if let Some(f) = self.targets().first().copied() {
                    self.checkpoint();
                    if let Some(e) = self.ev_mut(f) {
                        e.beam = BeamMode::Break;
                    }
                }
            }
            FlattenBeam => {
                self.modify(|e| e.flat_beam = !e.flat_beam);
            }
            ChangePitch | Transpose => self.dialog = Some(Dialog::Transpose { semitones: 0, octave: 0 }),
            ChangeDuration => self.dialog = Some(Dialog::Duration { percent: 100 }),
            ChangeVelocity => self.dialog = Some(Dialog::Velocity { mode: 0, value: 80, to: 80 }),
            MakeChord => self.make_chord(),
            MakeGrace => {
                self.modify(|e| e.grace = !e.grace);
            }
            MakeTab => self.tab_staff(cs),
            GuessDurations => self.guess_durations(),
            RevertToRaw => {
                self.modify(|e| e.play_len = None);
                self.status = "Playback set back to the written notes.".into();
            }
            AlignPlayback => {
                // playback follows the notation exactly: every note starts on its written beat and lasts its value
                self.checkpoint();
                let ms = self.command_measures();
                let n = self.align_times(&ms, false, true);
                self.selected.clear();
                self.status = format!("Playback aligned to the written beats in {} measure(s); {n} note(s) moved onto the beat.", ms.len());
            }
            RaisePitch => self.transpose_sel(1),
            LowerPitch => self.transpose_sel(-1),
            RaiseOctave => self.transpose_sel(12),
            LowerOctave => self.transpose_sel(-12),
            AddMeasure => self.dialog = Some(Dialog::AddMeasures { count: 1, before: false }),
            DeleteMeasure => {
                self.checkpoint();
                let (a, b) = mrange;
                for _ in a..=b {
                    self.score.delete_measure(a);
                }
                self.sel_measures = None;
                self.clamp_state();
                self.status = "Measure(s) deleted.".into();
            }
            Tempo => self.dialog = Some(Dialog::Tempo { bpm: self.score.tempo_at(cm), measure: cm, to_measure: mrange.1, mode: 0, to_bpm: self.score.tempo_at(cm) }),
            TimeSig => {
                let mi = &self.score.measures[cm];
                self.dialog = Some(Dialog::TimeSig { num: mi.num, den: mi.den, from: mrange.0 + 1, to: self.score.measures.len() });
            }
            KeySig => self.dialog = Some(Dialog::KeySig { key: self.score.measures[cm].key, from: mrange.0 + 1, to: self.score.measures.len(), transpose: false }),
            Barline => self.dialog = Some(Dialog::Barline { kind: self.score.measures[cm].barline, measure: cm, times: self.score.measures[cm].repeat_times }),
            Endings => self.dialog = Some(Dialog::Endings { text: self.score.measures[cm].ending.clone().unwrap_or_else(|| "1.".into()), measure: cm }),
            Coda => self.dialog = Some(Dialog::Coda {
                text: self.score.measures[cm].coda_phrase.clone().unwrap_or_default(),
                measure: cm,
                rehearsal: self.score.measures[cm].rehearsal.clone().unwrap_or_default(),
            }),
            MeasureNumbers => self.dialog = Some(Dialog::MeasureNumbers { mode: self.score.layout.show_measure_numbers }),
            CompressRests => {
                self.checkpoint();
                self.score.layout.compress_rests = !self.score.layout.compress_rests;
                self.status = format!("Compress rests {}.", if self.score.layout.compress_rests { "on (2 or more empty measures in a row are drawn as one multi-measure rest)" } else { "off" });
            }
            AlignSpacing => {
                // notes go back to their beats, sideways nudges are cleared, and notes sounding together line up
                // across all staves (the layout spaces every staff of a measure on one shared set of positions)
                self.checkpoint();
                let ms = self.command_measures();
                let n = self.align_times(&ms, true, false);
                self.score.layout.system_sizes.clear();
                self.selected.clear();
                self.status = format!("Spacing aligned in {} measure(s); {n} note(s) moved onto the beat.", ms.len());
            }
            SwingPlayback => self.dialog = Some(Dialog::Swing { swing: self.score.playback.swing, note: self.score.playback.swing_note }),
            AddPage => {
                self.checkpoint();
                let n = self.score.layout.measures_per_system * self.score.layout.systems_per_page;
                for _ in 0..n {
                    self.score.push_measure();
                }
                self.status = format!("Added a page ({n} measures).");
            }
            RemovePage => {
                self.checkpoint();
                let n = self.score.layout.measures_per_system * self.score.layout.systems_per_page;
                for _ in 0..n {
                    let last = self.score.measures.len() - 1;
                    self.score.delete_measure(last);
                }
                self.clamp_state();
            }
            AddStaff => self.dialog = Some(Dialog::AddStaff { name: format!("Staff {}", self.score.staves.len() + 1), clef: Clef::Treble, program: 0 }),
            DeleteStaff => {
                if self.score.staves.len() > 1 {
                    self.checkpoint();
                    self.score.delete_staff(cs);
                    self.clamp_state();
                } else {
                    self.status = "A score needs at least one staff.".into();
                }
            }
            SplitStaff => self.dialog = Some(Dialog::SplitStaff { split: 60 }),
            TabStaff => self.tab_staff(cs),
            PercussionStaff => {
                self.checkpoint();
                let st = &mut self.score.staves[cs];
                st.drum = !st.drum;
                if st.drum {
                    st.channel = 9;
                    st.set_clef(0, Clef::Percussion);
                }
                self.status = format!("Staff {} is {} a drum staff (General MIDI drum map, channel 10).", cs + 1, if self.score.staves[cs].drum { "now" } else { "no longer" });
            }
            ConnectStaves => self.dialog = Some(Dialog::ConnectStaves { kind: 0 }),
            CenterStaves | CenterSystems => {
                self.checkpoint();
                self.score.layout.system_sizes.clear();
                self.status = "Staves and systems are spread evenly on each page.".into();
            }
            SystemsPerPage => self.dialog = Some(Dialog::SystemsPerPage { n: self.score.layout.systems_per_page }),
            MeasuresPerSystem => self.dialog = Some(Dialog::MeasuresPerSystem { n: self.score.layout.measures_per_system }),
            LessMeasure => self.flow(false),
            MoreMeasure => self.flow(true),
            TextElements => self.dialog = Some(Dialog::Text(self.score.text.clone())),
            MidiDevice => self.staff_sheet = true,
            MidiPlayback => self.dialog = Some(Dialog::MidiPlayback),
            ShowHide => self.show_hidden = !self.show_hidden,
            ControlPoints => self.control_points = !self.control_points,
            LinearView => self.linear = !self.linear,
            FloatingWindows => self.floating_hidden = !self.floating_hidden,
            ZoomIn => self.zoom = (self.zoom * 1.25).min(5.0),
            ZoomOut => self.zoom = (self.zoom / 1.25).max(0.3),
            HideStaff => {
                if self.score.staves.iter().filter(|s| !s.hidden).count() > 1 {
                    self.checkpoint();
                    self.score.staves[cs].hidden = true;
                }
            }
            ShowAllStaves => {
                self.checkpoint();
                for s in self.score.staves.iter_mut() {
                    s.hidden = false;
                }
            }
            Redraw => {}
            // the tablet's rail shows one palette at a time: the same key opens or closes it there
            Palette(i) if self.device == crate::touch::Device::Touch => {
                self.rail_sel = if self.rail_sel == Some(i) { None } else { Some(i) };
            }
            Palette(i) => {
                self.palettes[i] = !self.palettes[i];
                self.floating_hidden = false;
            }
            StaffSheet => self.staff_sheet = !self.staff_sheet,
            Keyboard => self.keyboard_win = !self.keyboard_win,
            TempoWin => self.tempo_win = !self.tempo_win,
            Toolbar => self.toolbar_open = !self.toolbar_open,
            Tile | Cascade => {
                self.palette_layout_gen += 1;
                self.palette_cascade = c == Cascade;
            }
            CloseAllPalettes => self.palettes = [false; 11],
            ClickOnOff => {
                self.prefs.click_on = !self.prefs.click_on;
                self.score.playback.click = self.prefs.click_on;
                if !self.screenshot_mode {
                    crate::prefs::save(&self.prefs);
                }
                self.status = format!("Click {}.", if self.prefs.click_on { "on" } else { "off" });
            }
            // not built yet: the shortcuts stay, and say so instead of toggling a mark that does nothing
            AutoGuessBeam | AutoSpace => self.status = format!("{} isn't available yet.", crate::commands::label(c)),
            QwertyMode => {
                self.qwerty = !self.qwerty;
                self.status = if self.qwerty {
                    "QWERTY entry on: A W S E D F T G Y H U J K play C to C; 1-8 note values, comma = rest, period = dot, / = triplet, Shift+/ = tie. Q turns it off.".into()
                } else {
                    "QWERTY entry off.".into()
                };
            }
            FollowPlayback => self.score.playback.follow = !self.score.playback.follow,
            Play => {
                if self.rec.is_some() {
                    self.stop_recording();
                } else if self.is_playing() {
                    self.stop()
                } else {
                    self.play_from(cm)
                }
            }
            Stop => {
                if self.rec.is_some() {
                    self.stop_recording();
                } else {
                    self.stop();
                }
            }
            Record => {
                if self.rec.is_some() {
                    self.stop_recording();
                } else {
                    if self.is_playing() {
                        self.stop();
                    }
                    self.start_recording();
                    if !self.midi.as_ref().map(|m| m.connected()).unwrap_or(false) {
                        self.status = "Recording, but no MIDI keyboard is connected. Plug it in (USB or a MIDI interface); it's found automatically. Setup > MIDI Setup shows the devices.".into();
                    }
                }
            }
            MidiSetup => self.dialog = Some(Dialog::MidiSetup(self.prefs.clone())),
            // the browser: the step-by-step guide is the site's page for the computer in use (its text speaks of
            // the desktop app)
            #[cfg(target_arch = "wasm32")]
            ClavinovaGuide => crate::web::open_page(if crate::web::on_mac() { "../mac/" } else { "../windows/" }),
            #[cfg(not(target_arch = "wasm32"))]
            ClavinovaGuide => self.dialog = Some(Dialog::ClavinovaGuide),
            #[cfg(not(target_arch = "wasm32"))]
            CheckUpdates => self.start_update_check(true),
            #[cfg(target_arch = "wasm32")]
            CheckUpdates => {}
            ClickSetup => self.dialog = Some(Dialog::ClickSetup(self.prefs.clone())),
            JumpToMeasure => self.dialog = Some(Dialog::JumpTo { measure: self.score.measure_label(cm) }),
            Shortcuts => self.dialog = Some(Dialog::Shortcuts),
            About => self.dialog = Some(Dialog::About),
        }
    }

    pub fn transpose_diatonic(&mut self, steps: i32) {
        let key = self.score.measures.get(self.cursor.1).map(|m| m.key).unwrap_or(0);
        let n = self.modify(|e| {
            for p in e.pitches.iter_mut() {
                p.step += steps;
                p.alter = key_alter(key, p.letter());
            }
            e.sort_pitches();
        });
        if n > 0 {
            self.audition_selection();
        }
    }

}
