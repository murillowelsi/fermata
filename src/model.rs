//! Score data model. Values marked "estimated" in MECHANICS.md were not found in the original's code.
use serde::{Deserialize, Serialize};

/// Internal ticks per quarter note. The original stores 240 per quarter (score header field 0x2c, written as
/// MusicXML <divisions> by sub_463850 @0x463ba5); 480 is used inside so 128th notes stay whole numbers and every
/// exported file is converted to the original's 240.
pub const TPQ: u32 = 480;
/// The original's resolution, used for MIDI and MusicXML files.
pub const FILE_TPQ: u32 = 240;

/// The original's dynamic marks in its own order (index 0..22), from the table at 0x4eb8a0 read by sub_491ef0.
pub const DYNAMICS: [&str; 23] = [
    "p", "pp", "ppp", "pppp", "ppppp", "pppppp", "f", "ff", "fff", "ffff", "fffff", "ffffff", "mp", "mf", "sf", "sfp", "sfpp", "fp", "rf", "rfz", "sfz", "sffz", "fz",
];

/// Velocity range per dynamic (same order as DYNAMICS). Estimated: the defaults aren't a constant table in the
/// program. The rule is the original's: the middle of the range plays, or a random value in it with Humanize.
pub fn default_dynamic_ranges() -> Vec<(u8, u8)> {
    vec![
        (42, 56), (30, 42), (20, 30), (14, 20), (9, 14), (5, 9), (88, 104), (104, 116), (116, 124), (120, 126), (123, 127), (125, 127),
        (56, 72), (72, 88), (104, 120), (100, 116), (96, 112), (88, 104), (96, 112), (100, 116), (104, 120), (112, 127), (100, 116),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clef {
    Treble,
    Bass,
    Alto,
    Tenor,
    Treble8va,
    Treble8vb,
    Bass8vb,
    Percussion,
}

impl Clef {
    pub const ALL: [Clef; 8] = [
        Clef::Treble,
        Clef::Bass,
        Clef::Alto,
        Clef::Tenor,
        Clef::Treble8va,
        Clef::Treble8vb,
        Clef::Bass8vb,
        Clef::Percussion,
    ];
    /// Diatonic step (octave*7+letter) of the middle staff line.
    pub fn middle_step(self) -> i32 {
        match self {
            Clef::Treble => 4 * 7 + 6,          // B4
            Clef::Treble8va => 5 * 7 + 6,       // B5
            Clef::Treble8vb => 3 * 7 + 6,       // B3
            Clef::Bass => 3 * 7 + 1,            // D3
            Clef::Bass8vb => 2 * 7 + 1,         // D2
            Clef::Alto => 4 * 7,                // C4
            Clef::Tenor => 3 * 7 + 5,           // A3
            Clef::Percussion => 4 * 7 + 6,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Clef::Treble => "Treble",
            Clef::Bass => "Bass",
            Clef::Alto => "Alto",
            Clef::Tenor => "Tenor",
            Clef::Treble8va => "Treble 8va",
            Clef::Treble8vb => "Treble 8vb",
            Clef::Bass8vb => "Bass 8vb",
            Clef::Percussion => "Percussion",
        }
    }
}

/// A spelled pitch: diatonic step (octave*7 + letter index C=0..B=6) and alteration in semitones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pitch {
    pub step: i32,
    pub alter: i8,
}

const LETTER_SEMIS: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
pub const LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];

impl Pitch {
    pub fn midi(self) -> i32 {
        let oct = self.step.div_euclid(7);
        let l = self.step.rem_euclid(7) as usize;
        (oct + 1) * 12 + LETTER_SEMIS[l] + self.alter as i32
    }
    pub fn letter(self) -> usize {
        self.step.rem_euclid(7) as usize
    }
    /// Spell a MIDI number, preferring sharps or flats.
    pub fn from_midi(m: i32, prefer_flats: bool) -> Pitch {
        let oct = m.div_euclid(12) - 1;
        let pc = m.rem_euclid(12);
        let (l, a) = if prefer_flats {
            [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)][pc as usize]
        } else {
            [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)][pc as usize]
        };
        Pitch { step: oct * 7 + l, alter: a }
    }
    pub fn name(self) -> String {
        let acc = match self.alter {
            2 => "##",
            1 => "#",
            -1 => "b",
            -2 => "bb",
            _ => "",
        };
        format!("{}{}{}", LETTERS[self.letter()], acc, self.step.div_euclid(7))
    }
    /// The other common spelling of the same sound (Enharmonic command).
    pub fn enharmonic(self) -> Pitch {
        let m = self.midi();
        let alt = if self.alter > 0 {
            Pitch::from_midi(m, true)
        } else if self.alter < 0 {
            Pitch::from_midi(m, false)
        } else {
            // natural: try spelling with a double accidental-free neighbour
            let up = Pitch { step: self.step + 1, alter: 0 };
            let diff = m - up.midi();
            if (-2..=0).contains(&diff) { Pitch { step: self.step + 1, alter: diff as i8 } } else {
                let dn = Pitch { step: self.step - 1, alter: 0 };
                let d2 = m - dn.midi();
                Pitch { step: self.step - 1, alter: d2.clamp(-2, 2) as i8 }
            }
        };
        if alt == self { self } else { alt }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stem {
    Auto,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeamMode {
    Auto,
    /// Beam to the following note even across beats (Beam Group).
    Join,
    /// Break the beam before this note (Beam On Beat / sub group).
    Break,
    /// No beam (flags).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mark {
    Staccato,
    Staccatissimo,
    Tenuto,
    Accent,
    Marcato,
    Fermata,
    FermataBelow,
    Trill,
    Turn,
    Mordent,
    InvMordent,
    UpBow,
    DownBow,
    Harmonic,
    Plus,
    Breath,
    Caesura,
    Tremolo1,
    Tremolo2,
    Tremolo3,
    Finger(u8),
    Segno,
    Coda,
    PedalDown,
    PedalUp,
    Arpeggio,
}

impl Mark {
    pub fn label(self) -> String {
        match self {
            Mark::Staccato => "Staccato".into(),
            Mark::Staccatissimo => "Staccatissimo".into(),
            Mark::Tenuto => "Tenuto".into(),
            Mark::Accent => "Accent".into(),
            Mark::Marcato => "Marcato".into(),
            Mark::Fermata => "Fermata".into(),
            Mark::FermataBelow => "Fermata (below)".into(),
            Mark::Trill => "Trill".into(),
            Mark::Turn => "Turn".into(),
            Mark::Mordent => "Mordent".into(),
            Mark::InvMordent => "Inverted mordent".into(),
            Mark::UpBow => "Up bow".into(),
            Mark::DownBow => "Down bow".into(),
            Mark::Harmonic => "Harmonic".into(),
            Mark::Plus => "Stopped (+)".into(),
            Mark::Breath => "Breath mark".into(),
            Mark::Caesura => "Caesura".into(),
            Mark::Tremolo1 => "Tremolo (1 slash)".into(),
            Mark::Tremolo2 => "Tremolo (2 slashes)".into(),
            Mark::Tremolo3 => "Tremolo (3 slashes)".into(),
            Mark::Finger(n) => format!("Fingering {n}"),
            Mark::Segno => "Segno".into(),
            Mark::Coda => "Coda".into(),
            Mark::PedalDown => "Pedal down".into(),
            Mark::PedalUp => "Pedal up".into(),
            Mark::Arpeggio => "Arpeggio".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);
pub const BLACK: Rgb = Rgb(0, 0, 0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub voice: u8,
    pub start: u32,
    /// 0 = double whole .. 8 = 128th.
    pub value: u8,
    pub dots: u8,
    /// (actual, normal) e.g. (3,2) for triplets.
    pub tuplet: Option<(u8, u8)>,
    /// Empty = rest.
    pub pitches: Vec<Pitch>,
    /// Pitches whose accidental is forced visible (courtesy / cautionary).
    #[serde(default)]
    pub courtesy: Vec<bool>,
    /// Tie from each pitch to the next event.
    #[serde(default)]
    pub ties: Vec<bool>,
    #[serde(default)]
    pub tie_flip: bool,
    pub stem: Stem,
    pub beam: BeamMode,
    #[serde(default)]
    pub flat_beam: bool,
    pub marks: Vec<Mark>,
    pub velocity: u8,
    /// Playback length in ticks, separate from the written value (Change Duration / Revert to raw).
    #[serde(default)]
    pub play_len: Option<u32>,
    #[serde(default)]
    pub grace: bool,
    pub color: Rgb,
    #[serde(default)]
    pub lyric: Vec<String>,
    #[serde(default)]
    pub chord_symbol: Option<String>,
    #[serde(default)]
    pub dynamic: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    /// Sideways nudge set by dragging the note left or right, in staff spaces (Align Spacing clears it).
    #[serde(default)]
    pub dx: f32,
}

pub fn base_ticks(value: u8) -> u32 {
    (TPQ * 8) >> value.min(8)
}

impl Event {
    pub fn new(voice: u8, start: u32, value: u8, pitches: Vec<Pitch>) -> Self {
        let n = pitches.len();
        Event {
            voice,
            start,
            value,
            dots: 0,
            tuplet: None,
            pitches,
            courtesy: vec![false; n],
            ties: vec![false; n],
            tie_flip: false,
            stem: Stem::Auto,
            beam: BeamMode::Auto,
            flat_beam: false,
            marks: vec![],
            velocity: 80,
            play_len: None,
            grace: false,
            color: BLACK,
            lyric: vec![],
            chord_symbol: None,
            dynamic: None,
            expression: None,
            hidden: false,
            dx: 0.0,
        }
    }
    pub fn is_rest(&self) -> bool {
        self.pitches.is_empty()
    }
    pub fn ticks(&self) -> u32 {
        if self.grace {
            return 0;
        }
        dur_ticks(self.value, self.dots, self.tuplet)
    }
    pub fn end(&self) -> u32 {
        self.start + self.ticks()
    }
    pub fn normalize(&mut self) {
        let n = self.pitches.len();
        self.courtesy.resize(n, false);
        self.ties.resize(n, false);
    }
    pub fn sort_pitches(&mut self) {
        self.normalize();
        let mut v: Vec<(Pitch, bool, bool)> = self
            .pitches
            .iter()
            .zip(self.courtesy.iter())
            .zip(self.ties.iter())
            .map(|((p, c), t)| (*p, *c, *t))
            .collect();
        v.sort_by_key(|x| (x.0.step, x.0.alter));
        v.dedup_by_key(|x| x.0);
        self.pitches = v.iter().map(|x| x.0).collect();
        self.courtesy = v.iter().map(|x| x.1).collect();
        self.ties = v.iter().map(|x| x.2).collect();
    }
}

pub fn dur_ticks(value: u8, dots: u8, tuplet: Option<(u8, u8)>) -> u32 {
    let b = base_ticks(value);
    let mut t = b;
    let mut add = b;
    for _ in 0..dots {
        add /= 2;
        t += add;
    }
    if let Some((a, n)) = tuplet {
        t = t * n as u32 / a.max(1) as u32;
    }
    t
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Barline {
    Single,
    Double,
    Final,
    RepeatStart,
    RepeatEnd,
    RepeatBoth,
    Dashed,
    Invisible,
}

impl Barline {
    pub const ALL: [Barline; 8] = [
        Barline::Single,
        Barline::Double,
        Barline::Final,
        Barline::RepeatStart,
        Barline::RepeatEnd,
        Barline::RepeatBoth,
        Barline::Dashed,
        Barline::Invisible,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Barline::Single => "Single",
            Barline::Double => "Double",
            Barline::Final => "Final",
            Barline::RepeatStart => "Begin repeat",
            Barline::RepeatEnd => "End repeat",
            Barline::RepeatBoth => "End and begin repeat",
            Barline::Dashed => "Dashed",
            Barline::Invisible => "Invisible",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasureInfo {
    pub num: u8,
    pub den: u8,
    /// -7..7 (flats negative).
    pub key: i8,
    pub tempo: Option<u32>,
    pub barline: Barline,
    /// Ending number(s) starting here, e.g. "1." / "2.".
    pub ending: Option<String>,
    /// Coda phrase text (D.C., D.S. al Coda, Fine, To Coda...).
    pub coda_phrase: Option<String>,
    pub repeat_times: u8,
    /// Forced system break after this measure.
    #[serde(default)]
    pub system_break: bool,
    /// Compress multi-measure rests starting here.
    #[serde(default)]
    pub rehearsal: Option<String>,
    /// A pickup (anacrusis, F-05): the measure shows its time signature (num/den) but lasts only these ticks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup: Option<u32>,
    /// The tempo mark counts dotted quarters (compound meter); `tempo` itself stays in quarter notes per minute.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tempo_dotted: bool,
}

impl MeasureInfo {
    /// Length in ticks: the time signature's, or the written length of a pickup.
    pub fn ticks(&self) -> u32 {
        let full = self.full_ticks();
        self.pickup.map(|p| p.clamp(1, full)).unwrap_or(full)
    }
    /// Length of a whole measure of this time signature.
    pub fn full_ticks(&self) -> u32 {
        TPQ * 4 * self.num as u32 / self.den.max(1) as u32
    }
    /// The number the tempo mark shows: quarters, or dotted quarters when the tempo was given that way.
    pub fn tempo_shown(&self) -> Option<u32> {
        self.tempo.map(|t| if self.tempo_dotted { ((t as f32) * 2.0 / 3.0).round() as u32 } else { t })
    }
}

/// The smallest time signature that holds `ticks` exactly (how a pickup is written where formats have no pickup).
pub fn short_signature(ticks: u32) -> (u8, u8) {
    for den in [1u32, 2, 4, 8, 16, 32, 64] {
        let unit = TPQ * 4 / den;
        if unit > 0 && ticks.is_multiple_of(unit) && ticks / unit <= 32 {
            return ((ticks / unit).max(1) as u8, den as u8);
        }
    }
    (1, 64)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Staff {
    pub name: String,
    pub abbrev: String,
    /// One clef per measure index start (sorted).
    pub clefs: Vec<(usize, Clef)>,
    pub program: u8,
    pub channel: u8,
    pub volume: u8,
    pub pan: i8,
    /// Written transposition in semitones (sounding = written + transpose).
    pub transpose: i8,
    pub hidden: bool,
    pub muted: bool,
    pub solo: bool,
    /// Connected to the next staff with a brace (true) / bracket in group.
    pub brace_with_next: bool,
    pub bracket_with_next: bool,
    pub barline_to_next: bool,
    pub tab: bool,
    pub drum: bool,
    pub lines: u8,
    /// MIDI output port: 0 = A, 1 = B (the original addresses staves as A1..A16, B1..B16).
    #[serde(default)]
    pub port: u8,
}

impl Staff {
    pub fn new(name: &str, clef: Clef, program: u8) -> Self {
        Staff {
            name: name.into(),
            abbrev: name.chars().take(3).collect::<String>() + ".",
            clefs: vec![(0, clef)],
            program,
            channel: 0,
            volume: 100,
            pan: 0,
            transpose: 0,
            hidden: false,
            muted: false,
            solo: false,
            brace_with_next: false,
            bracket_with_next: false,
            barline_to_next: false,
            tab: false,
            drum: false,
            lines: 5,
            port: 0,
        }
    }
    pub fn clef_at(&self, m: usize) -> Clef {
        let mut c = self.clefs.first().map(|x| x.1).unwrap_or(Clef::Treble);
        for (i, cl) in &self.clefs {
            if *i <= m {
                c = *cl;
            }
        }
        c
    }
    pub fn set_clef(&mut self, m: usize, c: Clef) {
        self.clefs.retain(|x| x.0 != m);
        self.clefs.push((m, c));
        self.clefs.sort_by_key(|x| x.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum GraphicKind {
    Text,
    Line,
    Rect,
    RoundRect,
    Ellipse,
    Slur,
    Crescendo,
    Decrescendo,
    Ottava,
    OttavaBassa,
    TrillLine,
    Bracket,
}

/// A free graphic attached to a staff/measure with an offset in staff spaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    pub kind: GraphicKind,
    pub staff: usize,
    pub measure: usize,
    pub tick: u32,
    /// End anchor (for spanners).
    pub end_measure: usize,
    pub end_tick: u32,
    /// Offset from anchor in staff spaces (x right, y down).
    pub dx: f32,
    pub dy: f32,
    pub end_dy: f32,
    pub text: String,
    pub color: Rgb,
    pub flip: bool,
    #[serde(default)]
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextElements {
    pub title: String,
    pub subtitle: String,
    pub composer: String,
    pub lyricist: String,
    pub copyright: String,
    pub instructions: String,
    pub header: String,
    pub footer: String,
    pub show_page_numbers: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub measures_per_system: usize,
    pub systems_per_page: usize,
    pub staff_space: f32,
    pub page_w: f32,
    pub page_h: f32,
    pub margin: f32,
    pub show_measure_numbers: MeasureNumbers,
    pub handwritten: bool,
    pub c_score: bool,
    pub compress_rests: bool,
    pub staff_gap: f32,
    pub system_gap: f32,
    /// Per-system override of measures (computed flow; empty = use measures_per_system).
    #[serde(default)]
    pub system_sizes: Vec<usize>,
    /// Spacing Defaults (sub_41ca60, initial values at 0x4f7080): beam stem length in half spaces.
    #[serde(default = "d_stem")]
    pub stem_len_hs: u16,
    /// Beam thickness, % of a half space (0x4f7082 = 100; allowed 25..400).
    #[serde(default = "d_beam")]
    pub beam_thick_pct: u16,
    /// Horizontal tie offset in quarter spaces (0x4f7084 = 0).
    #[serde(default)]
    pub tie_h_qs: u16,
    /// Vertical tie offset in quarter spaces (0x4f7086 = 3).
    #[serde(default = "d_tiev")]
    pub tie_v_qs: u16,
    /// Measure spacing in half spaces (0x4f708c..0x4f7092 = 2 each).
    #[serde(default = "d_ms")]
    pub measure_space_hs: [u16; 4],
}

fn d_stem() -> u16 {
    7
}
fn d_beam() -> u16 {
    100
}
fn d_tiev() -> u16 {
    3
}
fn d_ms() -> [u16; 4] {
    [2, 2, 2, 2]
}
fn d_swing_note() -> u8 {
    4
}
fn d_ranges() -> Vec<(u8, u8)> {
    default_dynamic_ranges()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeasureNumbers {
    None,
    EverySystem,
    EveryMeasure,
    Every5,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSettings {
    pub swing: u8,
    pub click: bool,
    pub count_in: u8,
    pub follow: bool,
    pub default_tempo: u32,
    pub master_volume: f32,
    /// Swing Playback dialog (resource 2010): note type to swing, 1 = whole .. 6 = 32nd (estimated default: eighth).
    #[serde(default = "d_swing_note")]
    pub swing_note: u8,
    /// Dynamics velocity ranges and the Humanize switch.
    #[serde(default = "d_ranges")]
    pub dyn_ranges: Vec<(u8, u8)>,
    #[serde(default)]
    pub humanize: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub format: String,
    pub text: TextElements,
    pub staves: Vec<Staff>,
    pub measures: Vec<MeasureInfo>,
    /// events[staff][measure]
    pub events: Vec<Vec<Vec<Event>>>,
    pub graphics: Vec<Graphic>,
    pub layout: Layout,
    pub playback: PlaybackSettings,
}

impl Score {
    pub fn new(staves: Vec<Staff>, measures: usize) -> Score {
        let n = staves.len();
        let mut s = Score {
            format: "fermata-score/1".into(),
            text: TextElements {
                title: "Untitled".into(),
                subtitle: String::new(),
                composer: String::new(),
                lyricist: String::new(),
                copyright: String::new(),
                instructions: String::new(),
                header: String::new(),
                footer: String::new(),
                show_page_numbers: true,
            },
            staves,
            measures: vec![],
            events: vec![vec![]; n],
            graphics: vec![],
            layout: Layout {
                measures_per_system: 4,
                systems_per_page: 5,
                staff_space: 7.0,
                page_w: 612.0,
                page_h: 792.0,
                margin: 48.0,
                show_measure_numbers: MeasureNumbers::EverySystem,
                handwritten: false,
                c_score: false,
                compress_rests: false,
                staff_gap: 9.0,
                system_gap: 10.0,
                system_sizes: vec![],
                stem_len_hs: 7,
                beam_thick_pct: 100,
                tie_h_qs: 0,
                tie_v_qs: 3,
                measure_space_hs: [2, 2, 2, 2],
            },
            playback: PlaybackSettings {
                swing: 50,
                click: false,
                count_in: 0,
                follow: true,
                default_tempo: 120,
                master_volume: 0.7,
                swing_note: 4,
                dyn_ranges: default_dynamic_ranges(),
                humanize: false,
            },
        };
        for _ in 0..measures {
            s.push_measure();
        }
        s
    }
    pub fn default_score() -> Score {
        Score::new(vec![Staff::new("Piano", Clef::Treble, 0)], 16)
    }
    pub fn push_measure(&mut self) {
        let mi = self.measures.last().cloned().map(|mut m| {
            m.tempo = None;
            m.barline = Barline::Single;
            m.ending = None;
            m.coda_phrase = None;
            m.system_break = false;
            m.rehearsal = None;
            m.pickup = None;
            m.tempo_dotted = false;
            m
        });
        self.measures.push(mi.unwrap_or(MeasureInfo {
            num: 4,
            den: 4,
            key: 0,
            // no tempo mark: the first measure plays at MIDI Playback ▸ Default tempo until a mark is entered
            tempo: None,
            barline: Barline::Single,
            ending: None,
            coda_phrase: None,
            repeat_times: 2,
            system_break: false,
            rehearsal: None,
            pickup: None,
            tempo_dotted: false,
        }));
        for st in self.events.iter_mut() {
            st.push(vec![]);
        }
    }
    pub fn insert_measure(&mut self, at: usize) {
        let base = self.measures.get(at.saturating_sub(1)).cloned().or_else(|| self.measures.first().cloned());
        let mut m = base.unwrap_or(MeasureInfo {
            num: 4,
            den: 4,
            key: 0,
            tempo: None,
            barline: Barline::Single,
            ending: None,
            coda_phrase: None,
            repeat_times: 2,
            system_break: false,
            rehearsal: None,
            pickup: None,
            tempo_dotted: false,
        });
        m.tempo = if at == 0 { m.tempo } else { None };
        m.tempo_dotted = at == 0 && m.tempo_dotted;
        m.pickup = None;
        m.barline = Barline::Single;
        m.ending = None;
        m.coda_phrase = None;
        let at = at.min(self.measures.len());
        self.measures.insert(at, m);
        for st in self.events.iter_mut() {
            st.insert(at, vec![]);
        }
        for g in self.graphics.iter_mut() {
            if g.measure >= at {
                g.measure += 1;
            }
            if g.end_measure >= at {
                g.end_measure += 1;
            }
        }
        for s in self.staves.iter_mut() {
            for c in s.clefs.iter_mut() {
                if c.0 >= at && c.0 > 0 {
                    c.0 += 1;
                }
            }
        }
    }
    pub fn delete_measure(&mut self, at: usize) {
        if self.measures.len() <= 1 || at >= self.measures.len() {
            return;
        }
        self.measures.remove(at);
        for st in self.events.iter_mut() {
            st.remove(at);
        }
        self.graphics.retain(|g| g.measure != at);
        for g in self.graphics.iter_mut() {
            if g.measure > at {
                g.measure -= 1;
            }
            if g.end_measure > at {
                g.end_measure -= 1;
            }
        }
        for s in self.staves.iter_mut() {
            s.clefs.retain(|c| c.0 != at || c.0 == 0);
            for c in s.clefs.iter_mut() {
                if c.0 > at {
                    c.0 -= 1;
                }
            }
        }
    }
    pub fn add_staff(&mut self, at: usize, staff: Staff) {
        let at = at.min(self.staves.len());
        self.staves.insert(at, staff);
        self.events.insert(at, vec![vec![]; self.measures.len()]);
        for g in self.graphics.iter_mut() {
            if g.staff >= at {
                g.staff += 1;
            }
        }
    }
    pub fn delete_staff(&mut self, at: usize) {
        if self.staves.len() <= 1 || at >= self.staves.len() {
            return;
        }
        self.staves.remove(at);
        self.events.remove(at);
        self.graphics.retain(|g| g.staff != at);
        for g in self.graphics.iter_mut() {
            if g.staff > at {
                g.staff -= 1;
            }
        }
    }
    /// The number a measure has on the page and in messages: a pickup at the start is measure 0 (F-05).
    pub fn measure_label(&self, m: usize) -> usize {
        if self.measures.first().map(|x| x.pickup.is_some()).unwrap_or(false) { m } else { m + 1 }
    }
    /// The measure index for a number the user typed (the inverse of `measure_label`).
    pub fn measure_from_label(&self, n: usize) -> usize {
        let i = if self.measures.first().map(|x| x.pickup.is_some()).unwrap_or(false) { n } else { n.saturating_sub(1) };
        i.min(self.measures.len().saturating_sub(1))
    }
    pub fn tempo_at(&self, m: usize) -> u32 {
        let mut t = self.playback.default_tempo;
        for (i, mi) in self.measures.iter().enumerate() {
            if i > m {
                break;
            }
            if let Some(x) = mi.tempo {
                t = x;
            }
        }
        t
    }
    pub fn sort_measure(&mut self, s: usize, m: usize) {
        if let Some(v) = self.events.get_mut(s).and_then(|x| x.get_mut(m)) {
            v.sort_by_key(|e| (e.start, e.voice, !e.grace));
        }
    }
    /// Is the system boundary after measure `m`?
    /// Measures ▸ Compressed Rests: the runs `[a, b)` of 2 or more measures that are empty in every visible staff
    /// and are drawn as one multi-measure rest. A run is broken by a change of time signature, key or tempo, a
    /// double or repeat barline, an ending, a rehearsal mark, a coda or segno phrase, and a system break.
    pub fn compressed_runs(&self) -> Vec<(usize, usize)> {
        if !self.layout.compress_rests {
            return vec![];
        }
        let visible: Vec<usize> = (0..self.staves.len()).filter(|&s| !self.staves[s].hidden).collect();
        let empty = |m: usize| visible.iter().all(|&s| self.events.get(s).and_then(|st| st.get(m)).map(|v| v.is_empty()).unwrap_or(true));
        // can measure m+1 continue a run that measure m is in?
        let joins = |m: usize| {
            let (a, b) = (&self.measures[m], &self.measures[m + 1]);
            (a.num, a.den, a.key) == (b.num, b.den, b.key)
                && b.tempo.is_none()
                && matches!(a.barline, Barline::Single | Barline::Dashed | Barline::Invisible)
                && !a.system_break
                && b.ending.is_none()
                && a.ending.is_none()
                && b.rehearsal.is_none()
                && b.coda_phrase.is_none()
                && a.coda_phrase.is_none()
        };
        let n = self.measures.len();
        let mut out = vec![];
        let mut i = 0;
        while i < n {
            if !empty(i) {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < n && empty(j) && joins(j - 1) {
                j += 1;
            }
            if j - i >= 2 {
                out.push((i, j));
            }
            i = j;
        }
        out
    }

    pub fn systems(&self) -> Vec<(usize, usize)> {
        let n = self.measures.len();
        // a compressed run counts as one measure of the system
        let runs = self.compressed_runs();
        let unit_end = |i: usize| runs.iter().find(|r| r.0 == i).map(|r| r.1).unwrap_or(i + 1);
        let mut out = vec![];
        let mut i = 0;
        let mut si = 0;
        while i < n {
            let want = self.layout.system_sizes.get(si).copied().unwrap_or(self.layout.measures_per_system).max(1);
            let mut j = i;
            let mut units = 0;
            while j < n && units < want {
                j = unit_end(j);
                units += 1;
                if self.measures[j - 1].system_break {
                    break;
                }
            }
            out.push((i, j));
            i = j;
            si += 1;
        }
        if out.is_empty() {
            out.push((0, 0));
        }
        out
    }
}

pub fn key_name(k: i8) -> &'static str {
    match k {
        -7 => "Cb major / Ab minor",
        -6 => "Gb major / Eb minor",
        -5 => "Db major / Bb minor",
        -4 => "Ab major / F minor",
        -3 => "Eb major / C minor",
        -2 => "Bb major / G minor",
        -1 => "F major / D minor",
        0 => "C major / A minor",
        1 => "G major / E minor",
        2 => "D major / B minor",
        3 => "A major / F# minor",
        4 => "E major / C# minor",
        5 => "B major / G# minor",
        6 => "F# major / D# minor",
        7 => "C# major / A# minor",
        _ => "?",
    }
}

/// Alteration a key signature gives a letter (0=C..6=B).
pub fn key_alter(key: i8, letter: usize) -> i8 {
    const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B
    const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
    if key > 0 {
        if SHARP_ORDER[..key as usize].contains(&letter) {
            return 1;
        }
    } else if key < 0 && FLAT_ORDER[..(-key) as usize].contains(&letter) {
        return -1;
    }
    0
}

pub const GM_NAMES: [&str; 128] = [
    "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano", "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
    "Celesta", "Glockenspiel", "Music Box", "Vibraphone", "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
    "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ", "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
    "Nylon Guitar", "Steel Guitar", "Jazz Guitar", "Clean Guitar", "Muted Guitar", "Overdriven Guitar", "Distortion Guitar", "Guitar Harmonics",
    "Acoustic Bass", "Finger Bass", "Pick Bass", "Fretless Bass", "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
    "Violin", "Viola", "Cello", "Contrabass", "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
    "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2", "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
    "Trumpet", "Trombone", "Tuba", "Muted Trumpet", "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
    "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax", "Oboe", "English Horn", "Bassoon", "Clarinet",
    "Piccolo", "Flute", "Recorder", "Pan Flute", "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
    "Square Lead", "Sawtooth Lead", "Calliope Lead", "Chiff Lead", "Charang Lead", "Voice Lead", "Fifths Lead", "Bass + Lead",
    "New Age Pad", "Warm Pad", "Polysynth Pad", "Choir Pad", "Bowed Pad", "Metallic Pad", "Halo Pad", "Sweep Pad",
    "Rain", "Soundtrack", "Crystal", "Atmosphere", "Brightness", "Goblins", "Echoes", "Sci-fi",
    "Sitar", "Banjo", "Shamisen", "Koto", "Kalimba", "Bagpipe", "Fiddle", "Shanai",
    "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock", "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet", "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];
