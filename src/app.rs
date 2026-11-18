//! Application state and editing actions.
use crate::audio::{self, Audio, TimeMap};
use crate::commands::Cmd;
use crate::model::*;
use crate::score_view::{Geometry, NoteRef};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Arrow,
    Pencil,
    Eraser,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Item {
    Note(u8),
    Rest(u8),
    Accidental(i8),
    Courtesy,
    Parens,
    Dot(u8),
    Tuplet,
    Clef(Clef),
    Mark(Mark),
    Dynamic(String),
    Expression(String),
    Color(Rgb),
    Graphic(GraphicKind),
    Lyric,
    Chord,
    ChordGrid,
    TempoTool,
    MidiTool,
    Guitar(String),
}

/// Note Attributes with several notes: the playback length, note value and dots as the dialog opened, and
/// whether they differ among the selected notes. Only fields the person changed are applied to every note.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttrInit {
    pub play_len: u32,
    pub value: u8,
    pub dots: u8,
    /// play length, note value, dots: different among the selected notes
    pub mixed: [bool; 3],
}

#[derive(Clone, Debug)]
pub enum Dialog {
    NewScore { staves: usize, systems: usize, mps: usize, c_score: bool, handwritten: bool, template: usize },
    TimeSig { num: u8, den: u8, from: usize, to: usize },
    KeySig { key: i8, from: usize, to: usize, transpose: bool },
    Tempo { bpm: u32, measure: usize, to_measure: usize, mode: usize, to_bpm: u32 },
    Barline { kind: Barline, measure: usize, times: u8 },
    Endings { text: String, measure: usize },
    Coda { text: String, measure: usize, rehearsal: String },
    MeasureNumbers { mode: MeasureNumbers },
    AddMeasures { count: usize, before: bool },
    MeasuresPerSystem { n: usize },
    SystemsPerPage { n: usize },
    Text(TextElements),
    /// `init`: the length fields as the dialog opened, and which differ among the selected notes ("mixed").
    NoteAttributes { velocity: u8, play_len: u32, value: u8, dots: u8, voice: u8, hidden: bool, stem: Stem, init: AttrInit },
    Velocity { mode: usize, value: i32, to: i32 },
    SpacingDefaults { stem: u16, beam: u16, tie_h: u16, tie_v: u16, ms: [u16; 4] },
    Duration { percent: u32 },
    Transpose { semitones: i32, octave: i32 },
    Swing { swing: u8, note: u8 },
    Tuplet { notes: u8, in_time: u8 },
    MidiPlayback,
    MidiSetup(crate::prefs::Prefs),
    ClickSetup(crate::prefs::Prefs),
    ClavinovaGuide,
    JumpTo { measure: usize },
    PageSetup { w: f32, h: f32, margin: f32, staff_space: f32, staff_gap: f32, system_gap: f32 },
    AddStaff { name: String, clef: Clef, program: u8 },
    SplitStaff { split: i32 },
    ConnectStaves { kind: usize },
    /// Choose Chord: root, chord type, optional bass note, or a custom text that replaces them.
    ChordSymbol { root: String, kind: String, bass: String, custom: String, target: NoteRef },
    TextGraphic { text: String, index: usize, size: f32, bold: bool },
    Lyric { target: NoteRef, line: usize, text: String },
    Expression { text: String, target: NoteRef },
    Shortcuts,
    About,
    Message(String),
    ConfirmClose(Cmd),
}

#[derive(Clone, Debug)]
pub enum Drag {
    /// axis: 0 not decided yet, 1 up/down (pitch), 2 left/right (position); dx0 = each note's nudge at the start
    Note { start: egui::Pos2, refs: Vec<NoteRef>, step0: i32, moved: bool, axis: u8, dx0: Vec<f32> },
    Rubber { start: egui::Pos2 },
    Graphic { index: usize, start: egui::Pos2, dx0: f32, dy0: f32, end: bool },
    NewGraphic { kind: GraphicKind, start: egui::Pos2 },
}

pub struct App {
    pub score: Score,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub undo: Vec<String>,
    pub redo: Vec<String>,
    pub tool: Tool,
    pub item: Item,
    pub cur_value: u8,
    pub cur_dots: u8,
    pub cur_tuplet: bool,
    /// the tuplet the 3:2 button enters (notes, in the time of)
    pub tuplet_ratio: (u8, u8),
    pub voice: u8,
    pub show_voice: Option<u8>,
    pub selected: HashSet<NoteRef>,
    pub sel_graphics: HashSet<usize>,
    pub sel_measures: Option<(usize, usize)>,
    pub cursor: (usize, usize, u32),
    pub zoom: f32,
    pub linear: bool,
    pub show_hidden: bool,
    pub control_points: bool,
    pub palettes: [bool; 11],
    pub floating_hidden: bool,
    pub toolbar_open: bool,
    pub staff_sheet: bool,
    pub keyboard_win: bool,
    pub tempo_win: bool,
    pub dialog: Option<Dialog>,
    pub qwerty: bool,
    pub voice_prefix: bool,
    pub clipboard: Option<(Vec<(usize, usize, Event)>, Option<Vec<MeasureInfo>>)>,
    pub audio: Option<Audio>,
    pub timemap: TimeMap,
    pub status: String,
    pub geo: Geometry,
    pub drag: Option<Drag>,
    pub recording: bool,
    pub last_pitch: Pitch,
    pub palette_layout_gen: u32,
    pub scroll_to: Option<egui::Rect>,
    pub quit_requested: bool,
    pub screenshot_mode: bool,
    pub audition: bool,
    pub export_png_requested: bool,
    pub palette_cascade: bool,
    /// Palettes folded to their header (double-click on the header).
    /// MIDI Playback settings as they were when its dialog opened (Cancel puts them back).
    pub pb_backup: Option<PlaybackSettings>,
    /// `--theme light|dark` was given: it wins over the saved Appearance (and isn't saved).
    pub theme_forced: bool,
    pub pal_collapsed: [bool; 11],
    /// Palettes already put at their first position (until then they follow the window's size).
    pub pal_placed: [bool; 11],
    /// Fixed palette positions for the `--palette-sheet` contact sheet.
    pub palette_sheet_pos: Option<Vec<egui::Pos2>>,
    /// Every palette cell drawn this frame, for the pixel check of `--palette-sheet`.
    pub palette_cell_log: Option<Vec<crate::palettes::CellLog>>,
    pub lyric_line: usize,
    pub pending_cmd: Option<Cmd>,
    pub after_new_wizard: bool,
    pub scroll_to_cursor: bool,
    /// System the view last scrolled to while following playback.
    pub follow_sys: Option<usize>,
    pub midi: Option<crate::midi_in::MidiIn>,
    pub rec: Option<crate::rec::RecState>,
    pub rec_settings: crate::rec::RecSettings,
    pub last_step: Option<(crate::clock::Instant, NoteRef)>,
    pub pending_open: Vec<(u8, usize, u64, bool)>,
    /// The original-format file the score came from, used as the model when saving back to .enc.
    pub enc_model: Option<Vec<u8>>,
    pub midi_out: Option<crate::midi_out::MidiOut>,
    /// Notes written by the current recording take (staff, measure, tick); everything else in the take's path is replaced.
    pub rec_new: std::collections::HashSet<(usize, usize, u32)>,
    /// How far (absolute ticks from the start measure) old notes have been cleared during this take.
    pub rec_cleared: u64,
    /// A warning shown over the score (text, when, the measure to flash red as staff/measure).
    pub warn: Option<(String, crate::clock::Instant, Option<(usize, usize)>)>,
    pub prefs: crate::prefs::Prefs,
}

impl App {
    pub fn new(screenshot_mode: bool) -> App {
        App {
            score: default_new_score(),
            path: None,
            dirty: false,
            undo: vec![],
            redo: vec![],
            tool: Tool::Pencil,
            item: Item::Note(3),
            cur_value: 3,
            cur_dots: 0,
            cur_tuplet: false,
            tuplet_ratio: (3, 2),
            voice: 1,
            show_voice: None,
            selected: HashSet::new(),
            sel_graphics: HashSet::new(),
            sel_measures: None,
            cursor: (0, 0, 0),
            zoom: 1.35,
            linear: false,
            show_hidden: false,
            control_points: false,
            palettes: [true, false, false, false, false, false, false, false, false, false, false],
            floating_hidden: false,
            toolbar_open: true, // the toolbar under the menus; Windows ▸ Toolbar hides it
            staff_sheet: false,
            keyboard_win: false,
            tempo_win: false,
            dialog: None,
            qwerty: false,
            voice_prefix: false,
            clipboard: None,
            audio: None,
            timemap: TimeMap::default(),
            status: "Ready. Pick a note value and click on a staff, or press Q for QWERTY note entry.".into(),
            geo: Geometry::default(),
            drag: None,
            recording: false,
            last_pitch: Pitch { step: 4 * 7 + 6, alter: 0 },
            palette_layout_gen: 0,
            scroll_to: None,
            quit_requested: false,
            screenshot_mode,
            audition: true,
            export_png_requested: false,
            palette_cascade: false,
            pb_backup: None,
            theme_forced: false,
            pal_collapsed: [false; 11],
            pal_placed: [false; 11],
            palette_sheet_pos: None,
            palette_cell_log: None,
            lyric_line: 0,
            pending_cmd: None,
            after_new_wizard: false,
            scroll_to_cursor: false,
            follow_sys: None,
            midi: None,
            rec: None,
            rec_settings: crate::rec::RecSettings::default(),
            last_step: None,
            pending_open: vec![],
            enc_model: None,
            midi_out: None,
            rec_new: std::collections::HashSet::new(),
            rec_cleared: 0,
            warn: None,
            prefs: crate::prefs::Prefs::default(),
        }
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| self.score.text.title.clone());
        format!("{}{} - Fermata", name, if self.dirty { " *" } else { "" })
    }

    pub fn checkpoint(&mut self) {
        if let Ok(s) = serde_json::to_string(&self.score) {
            self.undo.push(s);
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.dirty = true;
    }

    pub fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            if let Ok(cur) = serde_json::to_string(&self.score) {
                self.redo.push(cur);
            }
            if let Ok(sc) = serde_json::from_str(&s) {
                self.score = sc;
            }
            self.clamp_state();
            self.status = "Undone.".into();
        }
    }
    pub fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            if let Ok(cur) = serde_json::to_string(&self.score) {
                self.undo.push(cur);
            }
            if let Ok(sc) = serde_json::from_str(&s) {
                self.score = sc;
            }
            self.clamp_state();
        }
    }

    pub fn clamp_state(&mut self) {
        let ns = self.score.staves.len();
        let nm = self.score.measures.len();
        self.cursor.0 = self.cursor.0.min(ns.saturating_sub(1));
        self.cursor.1 = self.cursor.1.min(nm.saturating_sub(1));
        let sc = &self.score;
        self.selected.retain(|r| r.staff < ns && r.measure < nm && r.idx < sc.events[r.staff][r.measure].len());
        let ng = sc.graphics.len();
        self.sel_graphics.retain(|g| *g < ng);
        if let Some((a, b)) = self.sel_measures {
            if a >= nm || b >= nm {
                self.sel_measures = None;
            }
        }
    }

    pub fn ev(&self, r: NoteRef) -> Option<&Event> {
        self.score.events.get(r.staff)?.get(r.measure)?.get(r.idx)
    }
    pub fn ev_mut(&mut self, r: NoteRef) -> Option<&mut Event> {
        self.score.events.get_mut(r.staff)?.get_mut(r.measure)?.get_mut(r.idx)
    }

    /// Selected notes, or all notes in selected measures.
    pub fn targets(&self) -> Vec<NoteRef> {
        let mut v: Vec<NoteRef> = self.selected.iter().copied().collect();
        if v.is_empty() {
            if let Some((a, b)) = self.sel_measures {
                for s in 0..self.score.staves.len() {
                    for m in a..=b {
                        for i in 0..self.score.events[s][m].len() {
                            v.push(NoteRef { staff: s, measure: m, idx: i });
                        }
                    }
                }
            }
        }
        v.sort_by_key(|r| (r.staff, r.measure, self.ev(*r).map(|e| e.start).unwrap_or(0), r.idx));
        v
    }

    pub fn cur_tuplet_opt(&self) -> Option<(u8, u8)> {
        if self.cur_tuplet { Some(self.tuplet_ratio) } else { None }
    }

    pub fn grid(&self) -> u32 {
        if self.cur_tuplet {
            // tuplet notes sit on their own grid (a quarter-note triplet every 320 ticks)
            dur_ticks(self.cur_value, 0, Some(self.tuplet_ratio)).max(1)
        } else {
            dur_ticks(self.cur_value, 0, None).min(base_ticks(4)).max(1)
        }
    }

    fn sort_and_remap(&mut self, s: usize, m: usize, keep: Option<usize>) -> Option<usize> {
        let evs = &mut self.score.events[s][m];
        let mut idx: Vec<usize> = (0..evs.len()).collect();
        idx.sort_by_key(|&i| (evs[i].start, evs[i].voice, !evs[i].grace));
        let new: Vec<Event> = idx.iter().map(|&i| evs[i].clone()).collect();
        *evs = new;
        // remap selection in this measure
        let old_sel: Vec<NoteRef> = self.selected.iter().filter(|r| r.staff == s && r.measure == m).copied().collect();
        for r in &old_sel {
            self.selected.remove(r);
        }
        for r in old_sel {
            if let Some(ni) = idx.iter().position(|&i| i == r.idx) {
                self.selected.insert(NoteRef { staff: s, measure: m, idx: ni });
            }
        }
        keep.and_then(|k| idx.iter().position(|&i| i == k))
    }

    /// Enter a note or rest (Pencil tool, QWERTY, keyboard window). Returns the event.
    pub fn enter(&mut self, s: usize, m: usize, tick: u32, pitch: Option<Pitch>, value: u8, dots: u8, advance: bool) -> NoteRef {
        let tuplet = self.cur_tuplet_opt();
        let voice = self.voice;
        // a note added to a chord keeps that chord's length; anything else must fit before the bar line
        let joins_chord = pitch.is_some() && self.score.events[s][m].iter().any(|e| e.voice == voice && e.start == tick && !e.grace && !e.is_rest());
        let len = dur_ticks(value, dots, tuplet);
        let tick = if joins_chord {
            tick
        } else {
            match self.place_tick(s, m, tick, len) {
                Some(t) => t,
                None => {
                    // notes left with empty stretches between them (entered on an off-beat): close those up when
                    // that makes room, so a 4/4 bar always takes its four beats
                    if self.room_after_closing(s, m, len) {
                        self.close_gaps(s, m, voice)
                    } else {
                        tick
                    }
                }
            }
        };
        if !joins_chord && !self.fits(m, tick, dur_ticks(value, dots, tuplet)) {
            self.warn_overflow(s, m, tick + dur_ticks(value, dots, tuplet));
            return NoteRef { staff: s, measure: m, idx: usize::MAX };
        }
        self.checkpoint();
        let evs = &mut self.score.events[s][m];
        let existing = evs.iter().position(|e| e.voice == voice && e.start == tick && !e.grace);
        let idx = match (existing, pitch) {
            (Some(i), Some(p)) if !evs[i].is_rest() => {
                let e = &mut evs[i];
                if !e.pitches.contains(&p) {
                    e.pitches.push(p);
                    e.sort_pitches();
                }
                i
            }
            (Some(i), _) => {
                let mut e = Event::new(voice, tick, value, pitch.into_iter().collect());
                e.dots = dots;
                e.tuplet = tuplet;
                evs[i] = e;
                i
            }
            (None, _) => {
                let mut e = Event::new(voice, tick, value, pitch.into_iter().collect());
                e.dots = dots;
                e.tuplet = tuplet;
                // keep within the bar: overlapping later notes of the voice are removed
                let end = e.end();
                evs.retain(|x| !(x.voice == voice && x.start > tick && x.start < end && !x.grace));
                evs.push(e);
                evs.len() - 1
            }
        };
        let idx = self.sort_and_remap(s, m, Some(idx)).unwrap_or(0);
        let r = NoteRef { staff: s, measure: m, idx };
        if let Some(p) = pitch {
            self.last_pitch = p;
            if self.audition {
                let tr = self.score.staves[s].transpose as i32;
                self.sound_note(s, p.midi() + tr, 90);
            }
        }
        self.selected.clear();
        self.selected.insert(r);
        if advance {
            let e = self.ev(r).cloned().unwrap();
            let mut t = e.end();
            let mut mm = m;
            if t >= self.score.measures[m].ticks() {
                t = 0;
                mm += 1;
                if mm >= self.score.measures.len() {
                    self.score.push_measure();
                }
            }
            self.cursor = (s, mm, t);
        }
        r
    }

    pub fn audio_mut(&mut self) -> &mut Audio {
        if self.audio.is_none() {
            self.audio = Some(Audio::new());
            if let Some(e) = &self.audio.as_ref().unwrap().error {
                self.status = format!("Sound unavailable: {e}");
            }
        }
        self.audio.as_mut().unwrap()
    }

    pub fn delete_refs(&mut self, refs: &[NoteRef]) {
        if refs.is_empty() {
            return;
        }
        self.checkpoint();
        let mut by: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
        for r in refs {
            by.entry((r.staff, r.measure)).or_default().push(r.idx);
        }
        for ((s, m), mut idx) in by {
            idx.sort();
            idx.dedup();
            for i in idx.into_iter().rev() {
                if i < self.score.events[s][m].len() {
                    self.score.events[s][m].remove(i);
                }
            }
        }
        self.selected.clear();
    }

    pub fn delete_pitch(&mut self, r: NoteRef, k: usize) {
        self.checkpoint();
        if let Some(e) = self.ev_mut(r) {
            if e.pitches.len() > 1 && k < e.pitches.len() {
                e.pitches.remove(k);
                e.normalize();
                if k < e.courtesy.len() {
                    e.courtesy.remove(k);
                }
                if k < e.ties.len() {
                    e.ties.remove(k);
                }
                e.normalize();
                return;
            }
        }
        self.score.events[r.staff][r.measure].remove(r.idx);
        self.selected.clear();
    }

    /// Does a note of `len` ticks starting at `tick` fit in measure `m`?
    /// The tuplet for `n` selected notes: n in the time of the largest power of two below n (3:2, 5:4, 6:4, 7:4),
    /// or none when n is already a power of two.
    pub fn auto_tuplet(n: usize) -> Option<(u8, u8)> {
        if n < 3 || n > 15 || n.is_power_of_two() {
            return None;
        }
        let mut p = 1usize;
        while p * 2 < n {
            p *= 2;
        }
        Some((n as u8, p as u8))
    }

    /// Turn the selected notes of each voice and measure into one tuplet group (`ratio`, or worked out from how many
    /// were selected), closing them up from the first one; what follows in the voice moves with them. Selecting a
    /// group that already is that tuplet turns it back into plain notes. Returns false when nothing was changed.
    pub fn make_tuplet(&mut self, ratio: Option<(u8, u8)>) -> bool {
        let t = self.targets();
        let mut groups: std::collections::BTreeMap<(usize, usize, u8), Vec<usize>> = Default::default();
        for r in &t {
            if let Some(e) = self.ev(*r) {
                if !e.grace {
                    groups.entry((r.staff, r.measure, e.voice)).or_default().push(r.idx);
                }
            }
        }
        if groups.is_empty() {
            self.status = "Select the notes to group first.".into();
            return false;
        }
        self.checkpoint();
        let mut changed = vec![];
        let mut msg = String::new();
        for ((s, m, voice), mut idx) in groups {
            let evs = &mut self.score.events[s][m];
            idx.sort_by_key(|&i| evs[i].start);
            idx.dedup_by_key(|i| evs[*i].start);
            let Some(r) = ratio.or_else(|| Self::auto_tuplet(idx.len())) else { continue };
            let undo = idx.iter().all(|&i| evs[i].tuplet == Some(r));
            let first = evs[idx[0]].start;
            let old_end = idx.iter().map(|&i| evs[i].end()).max().unwrap_or(first);
            let mut tick = first;
            for &i in &idx {
                evs[i].tuplet = if undo { None } else { Some(r) };
                evs[i].start = tick;
                tick += evs[i].ticks();
            }
            let new_end = tick;
            // later notes of the voice follow the group
            for (k, e) in evs.iter_mut().enumerate() {
                if e.voice == voice && !idx.contains(&k) && e.start >= old_end {
                    e.start = (e.start as i64 + new_end as i64 - old_end as i64).max(0) as u32;
                }
            }
            msg = if undo { "Tuplet removed.".into() } else { format!("{}:{} tuplet ({} notes in the time of {}).", r.0, r.1, r.0, r.1) };
            changed.push((s, m));
        }
        if changed.is_empty() {
            self.undo.pop();
            return false;
        }
        for &(s, m) in &changed {
            self.score.sort_measure(s, m);
        }
        self.selected.clear();
        self.dirty = true;
        if self.revert_if_overflow(&changed) {
            return false;
        }
        self.status = msg;
        true
    }

    /// The measures a measure command works on: the selected measures, else the measures of the selected notes,
    /// else the whole score.
    pub fn command_measures(&self) -> Vec<usize> {
        if let Some((a, b)) = self.sel_measures {
            return (a..=b.min(self.score.measures.len().saturating_sub(1))).collect();
        }
        let mut v: Vec<usize> = self.selected.iter().map(|r| r.measure).collect();
        v.sort();
        v.dedup();
        if v.is_empty() {
            (0..self.score.measures.len()).collect()
        } else {
            v
        }
    }

    /// Put the notes of every voice back on their beats: each note starts where the one before it in its voice
    /// ends, unless a real gap (at least as long as the note) was left on purpose. Tuplets land on their own grid
    /// (three triplet quarters fill two beats). Returns how many notes moved.
    pub fn align_times(&mut self, measures: &[usize], clear_nudge: bool, clear_playback: bool) -> usize {
        let mut moved = 0;
        for s in 0..self.score.staves.len() {
            for &m in measures {
                let Some(mi) = self.score.measures.get(m) else { continue };
                let mt = mi.ticks();
                let evs = &mut self.score.events[s][m];
                let mut voices: Vec<u8> = evs.iter().map(|e| e.voice).collect();
                voices.sort();
                voices.dedup();
                for v in voices {
                    let mut idx: Vec<usize> = (0..evs.len()).filter(|&i| evs[i].voice == v && !evs[i].grace).collect();
                    idx.sort_by_key(|&i| (evs[i].start, i));
                    let mut t = match idx.first() {
                        Some(&i) => {
                            // a tuplet that starts a voice sits on its own grid (triplet quarters every 320 ticks)
                            let g = evs[i].ticks().max(1);
                            let st = (evs[i].start + g / 2) / g * g;
                            if evs[i].tuplet.is_some() && st != evs[i].start && st + evs[i].ticks() <= mt {
                                evs[i].start = st;
                                moved += 1;
                            }
                            evs[i].start
                        }
                        None => continue,
                    };
                    let mut prev_start = u32::MAX;
                    for &i in &idx {
                        if evs[i].start == prev_start {
                            continue;
                        }
                        let len = evs[i].ticks();
                        let st = evs[i].start;
                        // overlapping the previous note, or an empty stretch before it (the bar shows no rest there,
                        // so it can only be a slip): close it up
                        let new = t;
                        if new != st && new + len <= mt {
                            evs[i].start = new;
                            moved += 1;
                        }
                        prev_start = st;
                        t = evs[i].start + len;
                    }
                }
                for e in evs.iter_mut() {
                    if clear_nudge {
                        e.dx = 0.0;
                    }
                    if clear_playback {
                        e.play_len = None;
                    }
                }
                self.score.sort_measure(s, m);
            }
        }
        moved
    }

    /// Where a new note of `len` ticks clicked at `tick` goes. Notes are written one after another: a click in
    /// the empty space after the notes of the voice adds the next note right where the last one ends (an empty
    /// stretch would be invisible, since the bar shows no rest there). None when the bar has no room for it.
    pub fn place_tick(&self, s: usize, m: usize, tick: u32, len: u32) -> Option<u32> {
        let mt = self.score.measures.get(m)?.ticks();
        let voice = self.voice;
        let evs = self.score.events.get(s)?.get(m)?;
        let prev_end = evs.iter().filter(|e| e.voice == voice && !e.grace && e.start < tick).map(|e| e.end()).max().unwrap_or(0);
        let t = if tick > prev_end { prev_end } else { tick };
        if t + len <= mt {
            Some(t)
        } else {
            None
        }
    }

    /// Whether a note of `len` ticks fits once the empty stretches of the voice are closed up.
    pub fn room_after_closing(&self, s: usize, m: usize, len: u32) -> bool {
        let used: u32 = self.score.events[s][m].iter().filter(|e| e.voice == self.voice && !e.grace).map(|e| e.ticks()).sum();
        used + len <= self.score.measures[m].ticks()
    }

    /// Close the empty stretches between the notes of one voice of a measure, from the start of the bar.
    /// Returns the tick where the voice now ends.
    pub fn close_gaps(&mut self, s: usize, m: usize, voice: u8) -> u32 {
        let evs = &mut self.score.events[s][m];
        let mut idx: Vec<usize> = (0..evs.len()).filter(|&i| evs[i].voice == voice && !evs[i].grace).collect();
        idx.sort_by_key(|&i| (evs[i].start, i));
        let mut t = 0u32;
        let mut last_start = u32::MAX;
        let mut last_new = 0u32;
        for &i in &idx {
            if evs[i].start == last_start {
                evs[i].start = last_new;
                continue;
            }
            last_start = evs[i].start;
            evs[i].start = t;
            last_new = t;
            t += evs[i].ticks();
        }
        self.score.sort_measure(s, m);
        t
    }

    pub fn fits(&self, m: usize, tick: u32, len: u32) -> bool {
        self.score.measures.get(m).map(|mi| tick + len <= mi.ticks()).unwrap_or(false)
    }

    /// Show that something doesn't fit: a red message over the score and the measure flashing red.
    pub fn warn_overflow(&mut self, s: usize, m: usize, end: u32) {
        let mi = &self.score.measures[m];
        let beat = (TPQ * 4 / mi.den.max(1) as u32).max(1) as f32;
        let need = end as f32 / beat;
        let need_txt = if (need - need.round()).abs() < 0.01 { format!("{}", need.round() as i32) } else { format!("{need:.2}").trim_end_matches('0').trim_end_matches('.').to_string() };
        let msg = format!("Doesn't fit: measure {} is {}/{} ({} beats) and this would need {} beats. Nothing was changed.", m + 1, mi.num, mi.den, mi.num, need_txt);
        self.status = msg.clone();
        self.warn = Some((msg, crate::clock::Instant::now(), Some((s, m))));
    }

    /// Time Signature: sets num/den on the measures `from..=to` (1-based). If any of them holds more than the new
    /// signature allows, in any staff or voice, nothing changes and the first such measure is returned (0-based),
    /// with its staff. A larger signature always fits.
    pub fn set_time_signature(&mut self, num: u8, den: u8, from: usize, to: usize) -> Result<(), (usize, usize)> {
        let range = from.saturating_sub(1)..to.min(self.score.measures.len());
        let new_ticks = TPQ * 4 * num as u32 / den.max(1) as u32;
        for m in range.clone() {
            for s in 0..self.score.staves.len() {
                let end = self.score.events.get(s).and_then(|st| st.get(m)).map(|evs| evs.iter().map(|e| e.end()).max().unwrap_or(0)).unwrap_or(0);
                if end > new_ticks {
                    return Err((s, m));
                }
            }
        }
        self.checkpoint();
        for m in range {
            self.score.measures[m].num = num;
            self.score.measures[m].den = den;
        }
        Ok(())
    }

    /// The warning when a new time signature doesn't fit: "Measure 1 has more beats than 3/4. Nothing was changed."
    pub fn warn_time_signature(&mut self, s: usize, m: usize, num: u8, den: u8) {
        let msg = format!("Measure {} has more beats than {num}/{den}. Nothing was changed.", m + 1);
        self.status = msg.clone();
        self.warn = Some((msg, crate::clock::Instant::now(), Some((s, m))));
    }

    /// After an edit, any note running past its bar line undoes the edit and shows the warning.
    pub fn revert_if_overflow(&mut self, measures: &[(usize, usize)]) -> bool {
        for &(s, m) in measures {
            let Some(mi) = self.score.measures.get(m) else { continue };
            let mt = mi.ticks();
            if let Some(end) = self.score.events.get(s).and_then(|x| x.get(m)).and_then(|evs| evs.iter().filter(|e| !e.grace).map(|e| e.end()).max()) {
                if end > mt {
                    if let Some(prev) = self.undo.pop() {
                        if let Ok(sc) = serde_json::from_str(&prev) {
                            self.score = sc;
                        }
                    }
                    self.clamp_state();
                    self.warn_overflow(s, m, end);
                    return true;
                }
            }
        }
        false
    }

    pub fn modify<F: FnMut(&mut Event)>(&mut self, mut f: F) -> usize {
        let t = self.targets();
        if t.is_empty() {
            self.status = "Select notes first (click with the arrow tool, or Shift+click / drag a box).".into();
            return 0;
        }
        self.checkpoint();
        for r in &t {
            if let Some(e) = self.ev_mut(*r) {
                f(e);
                e.normalize();
            }
        }
        let measures: Vec<(usize, usize)> = t.iter().map(|r| (r.staff, r.measure)).collect();
        if self.revert_if_overflow(&measures) {
            return 0;
        }
        t.len()
    }

    pub fn transpose_sel(&mut self, semis: i32) {
        let flats = self.score.measures.get(self.cursor.1).map(|m| m.key < 0).unwrap_or(false);
        let n = self.modify(|e| {
            for p in e.pitches.iter_mut() {
                if semis % 12 == 0 {
                    p.step += semis / 12 * 7;
                } else {
                    *p = Pitch::from_midi(p.midi() + semis, flats);
                }
            }
            e.sort_pitches();
        });
        if n > 0 {
            self.status = format!("Transposed {n} note(s) by {semis} semitone(s).");
            self.audition_selection();
        }
    }

    pub fn audition_selection(&mut self) {
        if !self.audition {
            return;
        }
        if let Some(r) = self.targets().first().copied() {
            if let Some(e) = self.ev(r).cloned() {
                for p in e.pitches.iter().take(4) {
                    self.sound_note(r.staff, p.midi(), e.velocity);
                }
            }
        }
    }

    pub fn select_all(&mut self) {
        self.selected.clear();
        for s in 0..self.score.staves.len() {
            for m in 0..self.score.measures.len() {
                for i in 0..self.score.events[s][m].len() {
                    self.selected.insert(NoteRef { staff: s, measure: m, idx: i });
                }
            }
        }
        self.status = format!("{} item(s) selected.", self.selected.len());
    }

    pub fn copy(&mut self, cut: bool) {
        if let Some((a, b)) = self.sel_measures {
            let mut items = vec![];
            for s in 0..self.score.staves.len() {
                for m in a..=b {
                    for e in &self.score.events[s][m] {
                        items.push((s, m - a, e.clone()));
                    }
                }
            }
            let infos = self.score.measures[a..=b].to_vec();
            self.clipboard = Some((items, Some(infos)));
            if cut {
                self.checkpoint();
                for s in 0..self.score.staves.len() {
                    for m in a..=b {
                        self.score.events[s][m].clear();
                    }
                }
            }
            self.status = format!("{} measure(s) copied.", b - a + 1);
            return;
        }
        let t = self.targets();
        if t.is_empty() {
            self.status = "Nothing selected to copy.".into();
            return;
        }
        let s0 = t.iter().map(|r| r.staff).min().unwrap();
        let m0 = t.iter().map(|r| r.measure).min().unwrap();
        let t0 = t.iter().filter(|r| r.measure == m0).filter_map(|r| self.ev(*r).map(|e| e.start)).min().unwrap_or(0);
        let mut items = vec![];
        for r in &t {
            if let Some(e) = self.ev(*r) {
                let mut e = e.clone();
                if r.measure == m0 {
                    e.start -= t0.min(e.start);
                }
                items.push((r.staff - s0, r.measure - m0, e));
            }
        }
        self.clipboard = Some((items, None));
        if cut {
            self.delete_refs(&t);
        }
        self.status = format!("{} note(s) {}.", t.len(), if cut { "cut" } else { "copied" });
    }

    pub fn paste(&mut self) {
        let Some((items, infos)) = self.clipboard.clone() else {
            self.status = "Clipboard is empty.".into();
            return;
        };
        self.checkpoint();
        let (s0, m0, t0) = self.cursor;
        let start_s = if infos.is_some() { 0 } else { s0 };
        let need = items.iter().map(|x| x.1).max().unwrap_or(0) + m0 + 1;
        while self.score.measures.len() < need {
            self.score.push_measure();
        }
        self.selected.clear();
        for (ds, dm, mut e) in items {
            let s = start_s + ds;
            let m = m0 + dm;
            if s >= self.score.staves.len() {
                continue;
            }
            if dm == 0 && infos.is_none() {
                e.start += t0;
            }
            let mt = self.score.measures[m].ticks();
            if e.start >= mt {
                continue;
            }
            let evs = &mut self.score.events[s][m];
            evs.retain(|x| !(x.voice == e.voice && x.start == e.start));
            evs.push(e);
        }
        for s in 0..self.score.staves.len() {
            for m in m0..need.min(self.score.measures.len()) {
                self.score.sort_measure(s, m);
            }
        }
        let measures: Vec<(usize, usize)> = (0..self.score.staves.len()).flat_map(|s| (m0..need.min(self.score.measures.len())).map(move |m| (s, m))).collect();
        if self.revert_if_overflow(&measures) {
            return;
        }
        self.status = "Pasted at the cursor.".into();
    }

    /// Give click notes the sound, port and channel from Click Settings (bar clicks are the ones built at 96).
    pub fn apply_click_prefs(&self, notes: &mut Vec<audio::PlayNote>, playback: bool) {
        if playback && self.prefs.click_only_rec {
            notes.retain(|n| !n.click);
            return;
        }
        for n in notes.iter_mut().filter(|n| n.click) {
            let bar = n.midi >= 96;
            let c = if bar { &self.prefs.click_bar } else { &self.prefs.click_beat };
            if c.port >= 2 {
                // the computer's own click sound
                n.port = 2;
                n.vel = c.vel as f32 / 127.0;
                n.drum = true;
            } else {
                n.port = c.port;
                n.channel = c.chan.clamp(1, 16) - 1;
                n.midi = c.pitch as i32;
                n.vel = c.vel.max(1) as f32 / 127.0;
                n.len = c.dur_ms.max(10) as f64 / 1000.0;
            }
        }
    }

    /// Click Settings ▸ Test click: one bar of clicks (bar click + three beat clicks) with the given settings.
    pub fn test_click(&mut self, p: &crate::prefs::Prefs) {
        let saved = (self.prefs.click_bar.clone(), self.prefs.click_beat.clone(), self.prefs.single_channel);
        self.prefs.click_bar = p.click_bar.clone();
        self.prefs.click_beat = p.click_beat.clone();
        self.prefs.single_channel = p.single_channel;
        let mut notes: Vec<audio::PlayNote> = (0..4)
            .map(|k| audio::PlayNote { at: k as f64 * 0.5, len: 0.03, midi: if k == 0 { 96 } else { 84 }, vel: 0.6, program: 115, gain: 0.6, drum: true, channel: 9, click: true, ..Default::default() })
            .collect();
        self.apply_click_prefs(&mut notes, false);
        let ext = self.play_notes(notes, 0.0, 120.0, false);
        self.prefs.click_bar = saved.0;
        self.prefs.click_beat = saved.1;
        self.prefs.single_channel = saved.2;
        self.status = if ext { "Test click sent to the piano (one bar).".into() } else { "Test click played on the computer (the port isn't connected).".into() };
    }

    pub fn play_from(&mut self, measure: usize) {
        // Click On/Off is a setting kept between sessions, applied to every score
        self.score.playback.click = self.prefs.click_on;
        let (mut notes, tm, start) = audio::build(&self.score, measure);
        self.apply_click_prefs(&mut notes, true);
        self.timemap = tm;
        let qpm = self.score.tempo_at(measure) as f64;
        let ext = self.play_notes(notes, start, qpm, true);
        self.status = if ext {
            format!("Playing from measure {} on {}. Space stops.", measure + 1, self.midi_out.as_ref().map(|m| m.port_label(0)).unwrap_or_default())
        } else {
            format!("Playing from measure {}. Space stops.", measure + 1)
        };
    }

    /// Ports A/B that reach an external instrument right now.
    pub fn routed(&self) -> [bool; 2] {
        match &self.midi_out {
            Some(m) => [m.connected[0].is_some(), m.connected[1].is_some()],
            None => [false, false],
        }
    }

    /// Send notes to the MIDI output for staves on a connected port, and the rest to the built-in sound.
    /// Returns true when anything went to the external instrument.
    pub fn play_notes(&mut self, notes: Vec<audio::PlayNote>, start: f64, qpm: f64, setup: bool) -> bool {
        self.stop_sound();
        let routed = self.routed();
        let (mut ext, int): (Vec<audio::PlayNote>, Vec<audio::PlayNote>) = notes.into_iter().partition(|n| n.port < 2 && routed[n.port as usize]);
        let single = self.prefs.single_channel;
        if single {
            // the piano only plays channel 1: everything goes there, and its voice is never changed
            for n in ext.iter_mut() {
                n.channel = 0;
                if n.click && n.midi < 48 {
                    // drum-kit click pitches would be low piano notes; use high ticks instead
                    n.midi = if n.midi == self.prefs.click_bar.pitch as i32 { 96 } else { 91 };
                }
            }
        }
        let any_ext = !ext.is_empty();
        if any_ext {
            let mut setup_msgs = if setup && !single { self.channel_setup(routed) } else { vec![] };
            // the click's instrument and full volume on its own channel, every time (recording included)
            let mut seen = std::collections::HashSet::new();
            for n in ext.iter().filter(|n| n.click && !single) {
                if !seen.insert((n.port, n.channel)) {
                    continue;
                }
                let c = if self.prefs.click_bar.port == n.port && self.prefs.click_bar.chan.saturating_sub(1) == n.channel { &self.prefs.click_bar } else { &self.prefs.click_beat };
                let ch = n.channel & 0x0f;
                if ch == 9 {
                    // drum channel: the instrument's own drum kit plays the click; its kit and volume are left alone
                    continue;
                }
                setup_msgs.push((n.port, vec![0xb0 | ch, 0, 0]));
                setup_msgs.push((n.port, vec![0xb0 | ch, 32, 0]));
                setup_msgs.push((n.port, vec![0xc0 | ch, c.program.min(127)]));
                setup_msgs.push((n.port, vec![0xb0 | ch, 7, 127]));
                setup_msgs.push((n.port, vec![0xb0 | ch, 11, 127]));
                setup_msgs.push((n.port, vec![0xb0 | ch, 10, 64]));
            }
            if let Some(m) = self.midi_out.as_mut() {
                m.play(&ext, setup_msgs, start, qpm);
            }
        }
        if !int.is_empty() {
            let vol = self.score.playback.master_volume;
            self.audio_mut().play(int, start, vol);
        }
        any_ext
    }

    /// Bank, instrument, volume and pan for every staff on an external port, sent before the notes.
    fn channel_setup(&self, routed: [bool; 2]) -> Vec<(u8, Vec<u8>)> {
        let mut out = vec![];
        let mut seen = std::collections::HashSet::new();
        for st in &self.score.staves {
            let port = st.port.min(1);
            if !routed[port as usize] || st.drum || st.muted {
                continue;
            }
            let ch = st.channel.min(15);
            if !seen.insert((port, ch)) {
                continue;
            }
            out.push((port, vec![0xb0 | ch, 0, 0]));
            out.push((port, vec![0xb0 | ch, 32, 0]));
            out.push((port, vec![0xc0 | ch, st.program.min(127)]));
            out.push((port, vec![0xb0 | ch, 7, st.volume.min(127)]));
            out.push((port, vec![0xb0 | ch, 10, (64 + st.pan as i32).clamp(0, 127) as u8]));
        }
        out
    }

    /// Stop the built-in sound and the MIDI output, releasing every note.
    pub fn stop_sound(&mut self) {
        if let Some(a) = &self.audio {
            a.stop();
        }
        if let Some(m) = self.midi_out.as_mut() {
            m.stop();
        }
    }
    pub fn stop(&mut self) {
        self.stop_sound();
        self.status = "Stopped.".into();
    }
    pub fn is_playing(&self) -> bool {
        self.audio.as_ref().map(|a| a.is_playing()).unwrap_or(false) || self.midi_out.as_ref().map(|m| m.is_playing()).unwrap_or(false)
    }
    pub fn play_position(&self) -> Option<(usize, u32)> {
        if let Some(m) = self.midi_out.as_ref().filter(|m| m.is_playing()) {
            return self.timemap.locate(m.time());
        }
        let a = self.audio.as_ref()?;
        if !a.is_playing() {
            return None;
        }
        self.timemap.locate(a.time())
    }

    /// Sound one note of a staff (auditioning while editing): on the piano when its port is connected.
    pub fn sound_note(&mut self, staff: usize, midi: i32, vel: u8) {
        let (port, ch, prog) = self.score.staves.get(staff).map(|s| (s.port.min(1), if s.drum { 9 } else { s.channel.min(15) }, s.program)).unwrap_or((0, 0, 0));
        if self.routed()[port as usize] {
            if let Some(m) = &self.midi_out {
                m.audition(port, ch, midi.clamp(0, 127) as u8, vel);
            }
        } else {
            self.audio_mut().audition(midi, prog, vel);
        }
    }

    /// Thru target as the original shows it in the toolbar ("A1").
    pub fn thru_target(&self) -> (u8, u8) {
        if self.prefs.thru_follow_staff {
            self.score.staves.get(self.cursor.0).map(|s| (s.port.min(1), if s.drum { 9 } else { s.channel.min(15) })).unwrap_or((0, 0))
        } else {
            (self.prefs.thru_port.min(1), self.prefs.thru_channel.min(15))
        }
    }

    /// Put saved settings to use (at start and after MIDI Setup).
    pub fn apply_prefs(&mut self) {
        let p = self.prefs.clone();
        self.audition = p.thru_on;
        self.rec_settings.quantize = p.rec_quantize;
        self.rec_settings.split = p.rec_split;
        self.rec_settings.count_in = p.rec_count_in;
        self.rec_settings.click = p.rec_click;
        if let Some(m) = self.midi_out.as_mut() {
            m.transmit_sync = p.transmit_sync;
            if m.chosen != [p.out_port_a.clone(), p.out_port_b.clone()] {
                m.choose(0, p.out_port_a.clone());
                m.choose(1, p.out_port_b.clone());
            }
        }
        if let Some(mi) = self.midi.as_mut() {
            if mi.record_port != p.record_port {
                mi.record_port = p.record_port.clone();
                mi.disconnect_all();
                mi.last_scan = None;
            }
        }
    }

    pub fn set_voice(&mut self, v: u8) {
        let n = self.modify(|e| e.voice = v);
        self.voice = v;
        if n > 0 {
            for s in 0..self.score.staves.len() {
                for m in 0..self.score.measures.len() {
                    self.score.sort_measure(s, m);
                }
            }
            self.selected.clear();
        }
        self.status = format!("Voice {v}.");
    }

    /// Tie Notes (Ctrl+T): each selected note is tied to the next selected note of its staff and voice, pitch by
    /// pitch where they match, across bar lines, systems and pages. A single selected note is tied to the note
    /// that follows it. When every one of those ties is already there, they are removed.
    pub fn tie_selection(&mut self) {
        let t = self.targets();
        if t.is_empty() {
            self.status = "Select the notes to tie first (Shift+click adds notes, also on the next page).".into();
            return;
        }
        // selected notes by staff and voice, in time order
        let mut groups: std::collections::BTreeMap<(usize, u8), Vec<NoteRef>> = Default::default();
        for r in &t {
            if let Some(e) = self.ev(*r) {
                if !e.is_rest() && !e.grace {
                    groups.entry((r.staff, e.voice)).or_default().push(*r);
                }
            }
        }
        let mut pairs: Vec<(NoteRef, NoteRef)> = vec![];
        for ((s, voice), mut refs) in groups {
            refs.sort_by_key(|r| (r.measure, self.ev(*r).map(|e| e.start).unwrap_or(0)));
            refs.dedup();
            if refs.len() == 1 {
                if let Some(n) = self.next_in_voice(refs[0], s, voice) {
                    pairs.push((refs[0], n));
                }
            } else {
                for w in refs.windows(2) {
                    pairs.push((w[0], w[1]));
                }
            }
        }
        // which pitches of each first note can tie (the same sounding pitch in the next note)
        let mut want: Vec<(NoteRef, Vec<usize>)> = vec![];
        for (a, b) in &pairs {
            let (Some(ea), Some(eb)) = (self.ev(*a), self.ev(*b)) else { continue };
            let ks: Vec<usize> = (0..ea.pitches.len()).filter(|&k| eb.pitches.iter().any(|q| q.midi() == ea.pitches[k].midi())).collect();
            if !ks.is_empty() {
                want.push((*a, ks));
            }
        }
        if want.is_empty() {
            self.status = "Nothing to tie: a tie joins the same note (same pitch) in two notes one after another.".into();
            return;
        }
        let all_on = want.iter().all(|(r, ks)| self.ev(*r).map(|e| ks.iter().all(|&k| e.ties.get(k).copied().unwrap_or(false))).unwrap_or(false));
        self.checkpoint();
        let mut n = 0;
        for (r, ks) in &want {
            if let Some(e) = self.ev_mut(*r) {
                e.normalize();
                for &k in ks {
                    e.ties[k] = !all_on;
                    n += 1;
                }
            }
        }
        self.dirty = true;
        self.status = if all_on { format!("{n} tie(s) removed.") } else { format!("{n} tie(s) added.") };
    }

    /// The next note of a staff's voice after `r`, in this measure or a later one.
    fn next_in_voice(&self, r: NoteRef, s: usize, voice: u8) -> Option<NoteRef> {
        let start = self.ev(r)?.start;
        for m in r.measure..self.score.measures.len() {
            let evs = &self.score.events[s][m];
            let best = (0..evs.len())
                .filter(|&i| evs[i].voice == voice && !evs[i].grace && (m > r.measure || evs[i].start > start))
                .min_by_key(|&i| evs[i].start);
            if let Some(i) = best {
                return if evs[i].is_rest() { None } else { Some(NoteRef { staff: s, measure: m, idx: i }) };
            }
        }
        None
    }

    pub fn slur_selection(&mut self, below: bool) {
        let t = self.targets();
        if t.len() < 2 {
            self.status = "Select two or more notes to slur.".into();
            return;
        }
        let a = t[0];
        let b = *t.last().unwrap();
        let ea = self.ev(a).cloned().unwrap();
        let eb = self.ev(b).cloned().unwrap();
        let clef = self.score.staves[a.staff].clef_at(a.measure);
        let mid = clef.middle_step();
        let pos = |e: &Event, below: bool| -> f32 {
            let st = if below { e.pitches.iter().map(|p| p.step).min() } else { e.pitches.iter().map(|p| p.step).max() }.unwrap_or(mid);
            // dy in staff spaces from staff top (top line = mid+4)
            (mid + 4 - st) as f32 * 0.5 + if below { 1.2 } else { -1.2 }
        };
        self.checkpoint();
        self.score.graphics.push(Graphic {
            kind: GraphicKind::Slur,
            staff: a.staff,
            measure: a.measure,
            tick: ea.start,
            end_measure: b.measure,
            end_tick: eb.start,
            dx: 0.0,
            dy: pos(&ea, below),
            end_dy: pos(&eb, below),
            text: String::new(),
            color: BLACK,
            flip: below,
            size: 0.0,
            bold: false,
            italic: false,
        });
        self.status = "Slur added.".into();
    }

    pub fn add_graphic(&mut self, kind: GraphicKind, s: usize, m: usize, tick: u32, dy: f32, em: usize, et: u32, edy: f32, text: String) -> usize {
        self.checkpoint();
        self.score.graphics.push(Graphic {
            kind,
            staff: s,
            measure: m,
            tick,
            end_measure: em,
            end_tick: et,
            dx: 0.0,
            dy,
            end_dy: edy,
            text,
            color: BLACK,
            flip: false,
            size: 0.0,
            bold: false,
            italic: false,
        });
        self.score.graphics.len() - 1
    }

    pub fn make_chord(&mut self) {
        let t = self.targets();
        if t.len() < 2 {
            self.status = "Select notes that start together to make a chord.".into();
            return;
        }
        self.checkpoint();
        let mut by: std::collections::BTreeMap<(usize, usize, u32), Vec<NoteRef>> = Default::default();
        for r in &t {
            if let Some(e) = self.ev(*r) {
                by.entry((r.staff, r.measure, e.start)).or_default().push(*r);
            }
        }
        let mut remove = vec![];
        for (_, refs) in by {
            if refs.len() < 2 {
                continue;
            }
            let first = refs[0];
            let mut add = vec![];
            for r in &refs[1..] {
                if let Some(e) = self.ev(*r) {
                    add.extend(e.pitches.clone());
                }
                remove.push(*r);
            }
            if let Some(e) = self.ev_mut(first) {
                e.pitches.extend(add);
                e.sort_pitches();
            }
        }
        let mut by2: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
        for r in remove {
            by2.entry((r.staff, r.measure)).or_default().push(r.idx);
        }
        for ((s, m), mut v) in by2 {
            v.sort();
            for i in v.into_iter().rev() {
                self.score.events[s][m].remove(i);
            }
        }
        self.selected.clear();
        self.status = "Chord made.".into();
    }

    pub fn guess_durations(&mut self) {
        let n = self.modify(|e| {
            if let Some(pl) = e.play_len {
                let (v, d) = crate::io::guess_value(pl.max(1));
                e.value = v;
                e.dots = d;
            }
        });
        if n > 0 {
            self.status = "Durations guessed from playback lengths.".into();
        }
    }

    pub fn split_staff(&mut self, s: usize, split: i32) {
        self.checkpoint();
        let mut lower = Staff::new("Bass", Clef::Bass, self.score.staves[s].program);
        lower.channel = self.score.staves[s].channel;
        self.score.staves[s].brace_with_next = true;
        self.score.staves[s].barline_to_next = true;
        self.score.add_staff(s + 1, lower);
        for m in 0..self.score.measures.len() {
            let evs = std::mem::take(&mut self.score.events[s][m]);
            let mut up = vec![];
            let mut down = vec![];
            for e in evs {
                if e.is_rest() {
                    up.push(e);
                    continue;
                }
                let (hi, lo): (Vec<(usize, Pitch)>, Vec<(usize, Pitch)>) = e.pitches.iter().copied().enumerate().partition(|(_, p)| p.midi() >= split);
                if !hi.is_empty() {
                    let mut a = e.clone();
                    a.pitches = hi.iter().map(|x| x.1).collect();
                    a.ties = hi.iter().map(|x| e.ties.get(x.0).copied().unwrap_or(false)).collect();
                    a.courtesy = hi.iter().map(|_| false).collect();
                    up.push(a);
                }
                if !lo.is_empty() {
                    let mut b = e.clone();
                    b.pitches = lo.iter().map(|x| x.1).collect();
                    b.ties = lo.iter().map(|x| e.ties.get(x.0).copied().unwrap_or(false)).collect();
                    b.courtesy = lo.iter().map(|_| false).collect();
                    b.lyric.clear();
                    down.push(b);
                }
            }
            self.score.events[s][m] = up;
            self.score.events[s + 1][m] = down;
        }
        self.selected.clear();
        self.status = format!("Staff split at {}.", Pitch::from_midi(split, false).name());
    }

    pub fn tab_staff(&mut self, s: usize) {
        self.checkpoint();
        let mut t = self.score.staves[s].clone();
        t.name = format!("{} Tab", t.name);
        t.abbrev = "Tab".into();
        t.tab = true;
        t.lines = 6;
        t.clefs = vec![(0, Clef::Treble8vb)];
        t.brace_with_next = false;
        t.muted = true;
        self.score.add_staff(s + 1, t);
        self.score.events[s + 1] = self.score.events[s].clone();
        self.status = "Tablature staff added (fret numbers aren't drawn yet).".into();
    }

    pub fn extract_part(&mut self, s: usize) {
        self.checkpoint();
        let staff = self.score.staves[s].clone();
        let ev = self.score.events[s].clone();
        let mut sc = self.score.clone();
        sc.staves = vec![staff.clone()];
        sc.staves[0].brace_with_next = false;
        sc.events = vec![ev];
        sc.graphics.retain(|g| g.staff == s);
        for g in sc.graphics.iter_mut() {
            g.staff = 0;
        }
        sc.text.subtitle = staff.name.clone();
        self.score = sc;
        self.path = None;
        self.selected.clear();
        self.cursor = (0, 0, 0);
        self.status = format!("Part \"{}\" extracted into this window (Undo brings the full score back).", staff.name);
    }

    pub fn flow(&mut self, more: bool) {
        let systems = self.score.systems();
        let m = self.cursor.1;
        let si = systems.iter().position(|(a, b)| m >= *a && m < *b).unwrap_or(0);
        self.checkpoint();
        let mut sizes: Vec<usize> = systems.iter().map(|(a, b)| b - a).collect();
        if more {
            sizes[si] += 1;
        } else if sizes[si] > 1 {
            sizes[si] -= 1;
        }
        for mi in self.score.measures.iter_mut() {
            mi.system_break = false;
        }
        sizes.truncate(si + 1);
        self.score.layout.system_sizes = sizes;
    }

    pub fn file_base_name(&self) -> String {
        if self.score.text.title.trim().is_empty() { "Untitled".into() } else { self.score.text.title.trim().replace(['/', '\\', ':'], "-") }
    }

    /// File > Export Encore File: the score in the original's .enc format. The original's file layout keeps
    /// instrument set-up, fonts and print settings that the rebuild doesn't model, so an existing .enc is the model:
    /// the one this score was opened from, else the last one opened, else one the person picks.
    pub fn export_enc(&mut self) {
        let model = self.enc_model.clone().or_else(|| {
            let ok = |b: &Vec<u8>| crate::enc::export(&self.score, b).is_ok();
            crate::prefs::dir().and_then(|d| std::fs::read(d.join("model.enc")).ok()).filter(ok)
        });
        let model = match model {
            Some(m) => m,
            None => {
                let Some(o) = crate::platform::pick_any("Choose an Encore file (.enc) made in Encore, to use as the model") else {
                    self.status = "Export cancelled: an .enc file made in Encore is needed as the model.".into();
                    return;
                };
                if o.bytes.get(0..4) != Some(b"SCOW") {
                    self.dialog = Some(Dialog::Message(format!("{} isn't an Encore file (.enc). Pick a score saved by Encore; it's only used as a model for the export.", o.name)));
                    return;
                }
                remember_enc_model(&o.bytes);
                o.bytes
            }
        };
        match crate::enc::export(&self.score, &model) {
            Ok(b) => self.export_bytes("enc", "Export Encore file (.enc)", &b),
            Err(e) => self.dialog = Some(Dialog::Message(format!("Couldn't export to .enc: {e}"))),
        }
    }

    /// Save bytes as a new file through the save window.
    pub fn export_bytes(&mut self, ext: &str, desc: &str, bytes: &[u8]) {
        let name = self.file_base_name();
        match crate::platform::save_as(&name, ext, desc, bytes) {
            Ok(Some(p)) => self.status = format!("Saved {}", p.display()),
            Ok(None) => {}
            Err(e) => self.dialog = Some(Dialog::Message(format!("Couldn't save: {e}"))),
        }
    }

    pub fn save(&mut self, as_new: bool) -> bool {
        let bytes = match crate::io::score_bytes(&self.score) {
            Ok(b) => b,
            Err(e) => {
                self.dialog = Some(Dialog::Message(format!("Couldn't save: {e}")));
                return false;
            }
        };
        let res = if as_new || self.path.is_none() {
            let name = self.file_base_name();
            crate::platform::save_as(&name, "fmt", "Fermata score", &bytes)
        } else {
            let p = self.path.clone().unwrap();
            crate::platform::save_to(&p, &bytes).map(|_| Some(p))
        };
        match res {
            Ok(Some(p)) => {
                self.path = Some(p.clone());
                self.dirty = false;
                self.status = format!("Saved {}", p.display());
                true
            }
            Ok(None) => false,
            Err(e) => {
                self.dialog = Some(Dialog::Message(format!("Couldn't save: {e}")));
                false
            }
        }
    }

    pub fn open(&mut self) {
        if let Some(o) = crate::platform::pick_open("Scores, Encore files and MIDI", &crate::platform::OPEN_EXTS) {
            self.load_opened(o);
        }
    }

    /// Files delivered by the platform layer since the last frame (none on the desktop, kept for drag-and-drop).
    pub fn poll_opened(&mut self) {
        for o in crate::platform::take_opened() {
            self.load_opened(o);
        }
    }

    pub fn open_path(&mut self, p: PathBuf) {
        match crate::platform::read_path(&p) {
            Some(o) => self.load_opened(o),
            None => self.dialog = Some(Dialog::Message(format!("Couldn't read {}", p.display()))),
        }
    }

    pub fn load_opened(&mut self, o: crate::platform::Opened) {
        match crate::io::open_bytes(&o.name, &o.bytes) {
            Ok(sc) => {
                let ext = o.name.rsplit('.').next().unwrap_or("").to_lowercase();
                let is_native = !matches!(ext.as_str(), "mid" | "midi" | "enc");
                if ext == "enc" {
                    self.enc_model = Some(o.bytes.clone());
                    remember_enc_model(&o.bytes);
                }
                self.score = sc;
                self.path = if is_native { o.path.clone() } else { None };
                self.dirty = !is_native;
                self.undo.clear();
                self.redo.clear();
                self.selected.clear();
                self.sel_graphics.clear();
                self.sel_measures = None;
                self.cursor = (0, 0, 0);
                self.status = format!("Opened {}", o.name);
            }
            Err(e) => self.dialog = Some(Dialog::Message(e)),
        }
    }

    pub fn new_score(&mut self, sc: Score) {
        self.score = sc;
        self.path = None;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.selected.clear();
        self.sel_graphics.clear();
        self.sel_measures = None;
        self.cursor = (0, 0, 0);
    }
}

/// The original's New Score defaults: 2 staves per system, 5 systems per page, 3 measures per system (Quick Start p. QS-2).
pub fn default_new_score() -> Score {
    build_template(0, 2, 5, 3, false, false)
}

pub const TEMPLATES: [&str; 22] = [
    "Single Staves",
    "Piano",
    "Voice-Piano",
    "Big Band",
    "Big Band (small)",
    "Brass Quintet",
    "Chamber Orchestra",
    "Choir 2 Stave SATB with Keyboard",
    "Choir 2 Stave SATB",
    "Choir SATB with Organ",
    "Concert Band",
    "Guitar + Tab",
    "Jazz Combo, 2 Horns",
    "Jazz Combo, 3 Horns",
    "Jazz Combo, 4 Horns",
    "Jazz Piano Quintet",
    "Jazz Piano Trio",
    "Jazz Quartet",
    "Lead Sheet",
    "Marching Band",
    "Orchestra",
    "String Quartet",
];

fn st(name: &str, clef: Clef, prog: u8, tr: i8) -> Staff {
    let mut s = Staff::new(name, clef, prog);
    s.transpose = tr;
    s
}

fn grand(name: &str, prog: u8) -> Vec<Staff> {
    let mut a = st(name, Clef::Treble, prog, 0);
    a.brace_with_next = true;
    a.barline_to_next = true;
    let mut b = st("", Clef::Bass, prog, 0);
    b.abbrev = String::new();
    vec![a, b]
}

pub fn build_template(t: usize, staves: usize, systems: usize, mps: usize, c_score: bool, handwritten: bool) -> Score {
    let mut v: Vec<Staff> = match t {
        0 => {
            if staves == 2 {
                grand("Piano", 0)
            } else {
                (0..staves.max(1)).map(|i| st(&format!("Staff {}", i + 1), if i % 2 == 1 && staves > 1 { Clef::Bass } else { Clef::Treble }, 0, 0)).collect()
            }
        }
        1 => grand("Piano", 0),
        2 => {
            let mut v = vec![st("Voice", Clef::Treble, 52, 0)];
            v.extend(grand("Piano", 0));
            v
        }
        3 | 4 => {
            let mut v = vec![
                st("Alto Sax 1", Clef::Treble, 65, -9),
                st("Alto Sax 2", Clef::Treble, 65, -9),
                st("Tenor Sax 1", Clef::Treble, 66, -14),
                st("Tenor Sax 2", Clef::Treble, 66, -14),
                st("Bari Sax", Clef::Treble, 67, -21),
                st("Trumpet 1", Clef::Treble, 56, -2),
                st("Trumpet 2", Clef::Treble, 56, -2),
                st("Trumpet 3", Clef::Treble, 56, -2),
                st("Trombone 1", Clef::Bass, 57, 0),
                st("Trombone 2", Clef::Bass, 57, 0),
                st("Guitar", Clef::Treble8vb, 26, 0),
            ];
            if t == 4 {
                v.retain(|s| !s.name.ends_with('2') && !s.name.ends_with('3'));
            }
            v.extend(grand("Piano", 0));
            v.push(st("Bass", Clef::Bass, 32, 0));
            let mut d = st("Drums", Clef::Percussion, 0, 0);
            d.drum = true;
            d.channel = 9;
            v.push(d);
            v
        }
        5 => vec![
            st("Trumpet 1", Clef::Treble, 56, -2),
            st("Trumpet 2", Clef::Treble, 56, -2),
            st("Horn", Clef::Treble, 60, -7),
            st("Trombone", Clef::Bass, 57, 0),
            st("Tuba", Clef::Bass, 58, 0),
        ],
        6 => vec![
            st("Flute", Clef::Treble, 73, 0),
            st("Oboe", Clef::Treble, 68, 0),
            st("Clarinet", Clef::Treble, 71, -2),
            st("Bassoon", Clef::Bass, 70, 0),
            st("Horn", Clef::Treble, 60, -7),
            st("Violin I", Clef::Treble, 40, 0),
            st("Violin II", Clef::Treble, 40, 0),
            st("Viola", Clef::Alto, 41, 0),
            st("Cello", Clef::Bass, 42, 0),
        ],
        7 | 8 | 9 => {
            let mut s = st("Soprano Alto", Clef::Treble, 52, 0);
            s.bracket_with_next = true;
            s.barline_to_next = true;
            let mut v = vec![s, st("Tenor Bass", Clef::Bass, 52, 0)];
            if t == 7 {
                v.extend(grand("Keyboard", 0));
            }
            if t == 9 {
                v = vec![st("Soprano", Clef::Treble, 52, 0), st("Alto", Clef::Treble, 52, 0), st("Tenor", Clef::Treble8vb, 52, 0), st("Bass", Clef::Bass, 52, 0)];
                v.extend(grand("Organ", 19));
            }
            v
        }
        10 | 19 => vec![
            st("Flute", Clef::Treble, 73, 0),
            st("Clarinet 1", Clef::Treble, 71, -2),
            st("Clarinet 2", Clef::Treble, 71, -2),
            st("Alto Sax", Clef::Treble, 65, -9),
            st("Tenor Sax", Clef::Treble, 66, -14),
            st("Trumpet 1", Clef::Treble, 56, -2),
            st("Trumpet 2", Clef::Treble, 56, -2),
            st("Horn", Clef::Treble, 60, -7),
            st("Trombone", Clef::Bass, 57, 0),
            st("Tuba", Clef::Bass, 58, 0),
            {
                let mut d = st("Percussion", Clef::Percussion, 0, 0);
                d.drum = true;
                d.channel = 9;
                d
            },
        ],
        11 => {
            let g = st("Guitar", Clef::Treble8vb, 25, 0);
            let mut t2 = g.clone();
            t2.name = "Guitar Tab".into();
            t2.abbrev = "Tab".into();
            t2.tab = true;
            t2.lines = 6;
            t2.muted = true;
            vec![g, t2]
        }
        12 | 13 | 14 => {
            let horns = [st("Trumpet", Clef::Treble, 56, -2), st("Tenor Sax", Clef::Treble, 66, -14), st("Alto Sax", Clef::Treble, 65, -9), st("Trombone", Clef::Bass, 57, 0)];
            let n = t - 10;
            let mut v: Vec<Staff> = horns[..n].to_vec();
            v.extend(grand("Piano", 0));
            v.push(st("Bass", Clef::Bass, 32, 0));
            v
        }
        15 => {
            let mut v = vec![st("Trumpet", Clef::Treble, 56, -2), st("Tenor Sax", Clef::Treble, 66, -14)];
            v.extend(grand("Piano", 0));
            v.push(st("Bass", Clef::Bass, 32, 0));
            v
        }
        16 => {
            let mut v = grand("Piano", 0);
            v.push(st("Bass", Clef::Bass, 32, 0));
            let mut d = st("Drums", Clef::Percussion, 0, 0);
            d.drum = true;
            d.channel = 9;
            v.push(d);
            v
        }
        17 => {
            let mut v = vec![st("Tenor Sax", Clef::Treble, 66, -14)];
            v.extend(grand("Piano", 0));
            v.push(st("Bass", Clef::Bass, 32, 0));
            v
        }
        18 => vec![st("Lead", Clef::Treble, 0, 0)],
        20 => vec![
            st("Flute", Clef::Treble, 73, 0),
            st("Oboe", Clef::Treble, 68, 0),
            st("Clarinet", Clef::Treble, 71, -2),
            st("Bassoon", Clef::Bass, 70, 0),
            st("Horn", Clef::Treble, 60, -7),
            st("Trumpet", Clef::Treble, 56, -2),
            st("Trombone", Clef::Bass, 57, 0),
            st("Timpani", Clef::Bass, 47, 0),
            st("Violin I", Clef::Treble, 40, 0),
            st("Violin II", Clef::Treble, 40, 0),
            st("Viola", Clef::Alto, 41, 0),
            st("Cello", Clef::Bass, 42, 0),
            st("Contrabass", Clef::Bass, 43, -12),
        ],
        _ => vec![st("Violin I", Clef::Treble, 40, 0), st("Violin II", Clef::Treble, 40, 0), st("Viola", Clef::Alto, 41, 0), st("Cello", Clef::Bass, 42, 0)],
    };
    for (i, s) in v.iter_mut().enumerate() {
        if !s.drum {
            s.channel = (i % 15) as u8 + if i >= 9 { 1 } else { 0 };
        }
        if c_score {
            s.transpose = s.transpose;
        }
    }
    let total = systems.max(1) * mps.max(1);
    let mut sc = Score::new(v, total);
    sc.layout.measures_per_system = mps.max(1);
    sc.layout.systems_per_page = if sc.staves.len() > 6 { 1 } else { systems.max(1) };
    if sc.staves.len() > 3 {
        sc.layout.staff_space = 5.0;
    }
    sc.layout.handwritten = handwritten;
    sc.layout.c_score = c_score;
    sc.measures[total - 1].barline = Barline::Final;
    sc
}

/// Keep a copy of the last .enc opened, so a new score can be exported in that format too.
fn remember_enc_model(bytes: &[u8]) {
    if bytes.get(0..4) != Some(b"SCOW") {
        return;
    }
    if let Some(d) = crate::prefs::dir() {
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join("model.enc"), bytes);
    }
}

/// Split a chord symbol into (root, type, bass) as the Choose Chord window shows it; None when it isn't one
/// (it is then shown as custom text).
pub fn split_chord(text: &str) -> Option<(String, String, String)> {
    fn root_len(t: &str) -> Option<usize> {
        let mut c = t.chars();
        let l = c.next()?;
        if !"CDEFGAB".contains(l) {
            return None;
        }
        Some(match c.next() {
            Some('#') | Some('b') => 2,
            _ => 1,
        })
    }
    let t = text.trim();
    let n = root_len(t)?;
    let (root, rest) = t.split_at(n);
    let (kind, bass) = match rest.rfind('/') {
        Some(i) if root_len(&rest[i + 1..]) == Some(rest.len() - i - 1) => (&rest[..i], &rest[i + 1..]),
        _ => (rest, ""),
    };
    if !crate::ui::CHORD_GRID.iter().flat_map(|(_, rows)| rows.iter().flat_map(|r| r.iter())).any(|k| k.map(|k| k == kind).unwrap_or(false)) {
        return None;
    }
    Some((root.to_string(), kind.to_string(), bass.to_string()))
}

#[cfg(test)]
mod time_signature_tests {
    use super::*;

    fn quarters(a: &mut App, m: usize, n: u32) {
        for k in 0..n {
            a.enter(0, m, k * TPQ, Some(Pitch::from_midi(60, false)), 3, 0, false);
        }
    }

    #[test]
    fn full_4_4_to_3_4_is_refused_and_nothing_changes() {
        let mut a = App::new(true);
        quarters(&mut a, 0, 4);
        let (before, undo) = (a.score.clone(), a.undo.len());
        assert_eq!(a.set_time_signature(3, 4, 1, 1), Err((0, 0)));
        assert_eq!(a.score, before);
        assert_eq!(a.undo.len(), undo, "a refused change leaves no empty undo step");
    }

    #[test]
    fn three_quarters_in_4_4_fit_3_4() {
        let mut a = App::new(true);
        quarters(&mut a, 0, 3);
        assert_eq!(a.set_time_signature(3, 4, 1, 1), Ok(()));
        assert_eq!((a.score.measures[0].num, a.score.measures[0].den), (3, 4));
    }

    #[test]
    fn empty_measure_changes() {
        let mut a = App::new(true);
        assert_eq!(a.set_time_signature(3, 4, 1, 1), Ok(()));
        assert_eq!(a.score.measures[0].num, 3);
    }

    #[test]
    fn full_3_4_to_4_4_is_accepted() {
        let mut a = App::new(true);
        a.score.measures[0].num = 3;
        quarters(&mut a, 0, 3);
        assert_eq!(a.set_time_signature(4, 4, 1, 1), Ok(()));
        assert_eq!(a.score.measures[0].num, 4);
    }

    #[test]
    fn one_measure_that_doesnt_fit_refuses_the_whole_range() {
        let mut a = App::new(true);
        // measures 1-2 empty, measure 3 full in the second staff
        let s = 1.min(a.score.staves.len() - 1);
        for k in 0..4 {
            a.enter(s, 2, k * TPQ, Some(Pitch::from_midi(48, false)), 3, 0, false);
        }
        let before = a.score.clone();
        assert_eq!(a.set_time_signature(3, 4, 1, 4), Err((s, 2)));
        assert_eq!(a.score, before);
        a.warn_time_signature(s, 2, 3, 4);
        assert_eq!(a.status, "Measure 3 has more beats than 3/4. Nothing was changed.");
    }
}
