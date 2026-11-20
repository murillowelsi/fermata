//! Menu commands and their keyboard shortcuts (from the Quick Start shortcut charts).
use egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cmd {
    // File
    New, NewDefault, Open, Close, Save, SaveAs, Revert, Print, PageSetup, ImportMidi, ImportXml, ExportMidi, ExportXml, ExportEnc, ExportPng, ExportPdf, ExtractPart, Quit,
    // Edit
    Undo, Redo, Cut, Copy, Paste, Clear, SelectAll, NudgeLeft, NudgeRight, NudgeUp, NudgeDown,
    // Notes
    NoteAttributes, Voice(u8), Enharmonic, AccToSharps, AccToFlats, StemUp, StemDown, StemNormal, AddMark, RemoveMark, Tie, FlipTie,
    SlurAbove, SlurBelow, BeamGroup, BeamOnBeat, SubGroupBeam, FlattenBeam, ChangePitch, ChangeDuration, ChangeVelocity,
    MakeChord, MakeGrace, MakeTab, GuessDurations, RevertToRaw, RaisePitch, LowerPitch, RaiseOctave, LowerOctave, Transpose,
    // Measures
    AddMeasure, DeleteMeasure, Tempo, TimeSig, KeySig, Barline, Endings, Coda, MeasureNumbers, CompressRests, AlignSpacing,
    AlignPlayback, SwingPlayback,
    // Score
    AddPage, RemovePage, AddStaff, DeleteStaff, SplitStaff, TabStaff, ConnectStaves, CenterStaves, CenterSystems,
    SystemsPerPage, MeasuresPerSystem, LessMeasure, MoreMeasure, TextElements, MidiDevice, MidiPlayback,
    // View
    ShowHide, ControlPoints, LinearView, FloatingWindows, ZoomIn, ZoomOut, HideStaff, ShowAllStaves, Redraw,
    // Windows
    Palette(usize), StaffSheet, Keyboard, TempoWin, Toolbar, Tile, Cascade, CloseAllPalettes,
    // Setup
    ClickOnOff, AutoGuessBeam, AutoSpace, QwertyMode, FollowPlayback,
    // added from the original's menu resource
    MidiSetup, ClickSetup, ClavinovaGuide, CheckUpdates, PenGestures, PenMode, TranscribeInk, ClearInk, PenImmediate, SimplifyAccidentals, RemoveAllMarks, OpenAllPalettes, SpacingDefaults, PercussionStaff, UndoAlt, CutAlt, CopyAlt, PasteAlt, AlignSpacingPrev,
    // Transport
    Play, Stop, Record, JumpToMeasure,
    // Help
    Shortcuts, About,
}

pub const PALETTES: [&str; 11] = ["Notes", "Clefs", "Graphics", "Tools", "Dynamics", "Marks 1", "Marks 2", "Symbols", "Guitar", "Expressions", "Colors"];
const PALETTE_KEYS: [Key; 11] = [Key::N, Key::C, Key::L, Key::T, Key::D, Key::M, Key::Period, Key::S, Key::G, Key::E, Key::O];

/// The main modifier: Ctrl, as in Windows Encore, on every platform (also on the Mac, so the keys are the same
/// on both computers: Ctrl+T ties, Ctrl+M beams, Ctrl+Alt+M adds a measure).
pub fn primary() -> Modifiers {
    Modifiers::CTRL
}

/// The second modifier of Windows Encore's Measures and Score commands: Ctrl+Alt.
pub fn secondary() -> Modifiers {
    Modifiers::CTRL | Modifiers::ALT
}

fn sc(m: Modifiers, k: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(m, k))
}

/// Shortcuts from Encore.exe's accelerator table THEACCEL (resource type 9): Windows Encore's keys, used on every
/// platform. Quit stays Cmd+Q on the Mac (Ctrl+Q opens the toolbar in Encore).
pub fn shortcut(c: Cmd) -> Option<KeyboardShortcut> {
    if cfg!(target_os = "macos") && c == Cmd::Quit {
        return sc(Modifiers::MAC_CMD, Key::Q);
    }
    // the browser off the Mac: Chrome and Edge keep Ctrl+N, Ctrl+Shift+N, Ctrl+W, Ctrl+T and Ctrl+Shift+T for
    // themselves, so those commands take Ctrl+Alt+Shift keys there (menus show them); the installed app is unchanged
    #[cfg(target_arch = "wasm32")]
    if !crate::web::on_mac() {
        if let Some(s) = web_alternative(c) {
            return Some(s);
        }
    }
    shortcut_with(c, false, primary(), secondary())
}

/// Whether the keys follow the Mac: the Mac app, or the browser on a Mac (same keys, same symbols in menus).
pub fn on_mac() -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::on_mac();
    #[cfg(not(target_arch = "wasm32"))]
    return cfg!(target_os = "macos");
}

/// On the Mac, the Mac Encore chart's Cmd keys still work too (Cmd+S, Cmd+Z, Cmd+T...), alongside the Ctrl ones.
/// The Mac chart's Ctrl keys (Ctrl+M Add Measure, Ctrl+T Time Signature...) are left out: in the Windows layout
/// those keys belong to other commands.
pub fn mac_shortcut(c: Cmd) -> Option<KeyboardShortcut> {
    if !on_mac() {
        return None;
    }
    let m = shortcut_with(c, true, Modifiers::MAC_CMD, Modifiers::CTRL)?;
    if !m.modifiers.mac_cmd || m.modifiers.ctrl || Some(m) == shortcut(c) {
        return None;
    }
    Some(m)
}

/// Every key combination with its command, most specific first (Cmd+Shift+S before Cmd+S).
pub fn all_shortcuts() -> Vec<(KeyboardShortcut, Cmd)> {
    let mut v = vec![];
    for c in all_shortcut_cmds() {
        if let Some(s) = shortcut(c) {
            v.push((s, c));
        }
        if let Some(s) = mac_shortcut(c) {
            v.push((s, c));
        }
    }
    v.sort_by_key(|(s, _)| {
        let m = s.modifiers;
        -((m.alt as i32) + (m.shift as i32) + (m.ctrl as i32) + (m.mac_cmd as i32) + (m.command as i32))
    });
    v
}

fn shortcut_with(c: Cmd, mac: bool, p: Modifiers, s2: Modifiers) -> Option<KeyboardShortcut> {
    use Cmd::*;
    let ps = p | Modifiers::SHIFT;
    let s2s = s2 | Modifiers::SHIFT;
    let alt = Modifiers::ALT;
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    match c {
        New => sc(p, Key::N),
        NewDefault => sc(ps, Key::N),
        Open => sc(p, Key::O),
        Close => sc(p, Key::W),
        Save => sc(p, Key::S),
        SaveAs => sc(ps, Key::S),
        Revert => sc(p, Key::R),
        Print => sc(p, Key::P),
        PageSetup => sc(ps, Key::P),
        Quit => if mac { sc(p, Key::Q) } else { sc(alt, Key::F4) },
        Undo => sc(p, Key::Z),
        UndoAlt => sc(alt, Key::Backspace),
        Redo => sc(ps, Key::Z),
        Cut => sc(p, Key::X),
        CutAlt => sc(Modifiers::SHIFT, Key::Delete),
        Copy => sc(p, Key::C),
        CopyAlt => sc(p, Key::Insert),
        Paste => sc(p, Key::V),
        PasteAlt => sc(Modifiers::SHIFT, Key::Insert),
        SelectAll => sc(p, Key::A),
        NudgeLeft => sc(p, Key::OpenBracket),
        NudgeRight => sc(p, Key::CloseBracket),
        NudgeUp => sc(ps, Key::CloseBracket),
        NudgeDown => sc(ps, Key::OpenBracket),
        NoteAttributes => sc(p, Key::I),
        Voice(n) => sc(p, [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8][(n.clamp(1, 8) - 1) as usize]),
        Enharmonic => sc(p, Key::E),
        SimplifyAccidentals => sc(ps, Key::E),
        StemUp => sc(p, Key::U),
        StemDown => sc(p, Key::D),
        StemNormal => sc(s2s, Key::N),
        AddMark => sc(p, Key::Num9),
        RemoveMark => sc(ps, Key::Num9),
        Tie => sc(p, Key::T),
        FlipTie => sc(ps, Key::T),
        SlurAbove => sc(p, Key::L),
        SlurBelow => sc(ps, Key::L),
        BeamGroup => if mac { sc(ps, Key::B) } else { sc(p, Key::M) },
        BeamOnBeat => sc(p, Key::B),
        FlattenBeam => sc(ps, Key::F),
        ChangePitch => sc(s2, Key::X),
        ChangeDuration => sc(s2, Key::D),
        ChangeVelocity => sc(s2, Key::V),
        MakeChord => sc(s2, Key::C),
        MakeGrace => sc(s2, Key::G),
        MakeTab => sc(s2, Key::Z),
        GuessDurations => sc(p, Key::G),
        RaisePitch => sc(p, Key::Equals),
        LowerPitch => sc(p, Key::Minus),
        RaiseOctave => sc(ps, Key::Equals),
        LowerOctave => sc(ps, Key::Minus),
        AddMeasure => sc(s2, Key::M),
        DeleteMeasure => sc(s2s, Key::M),
        Tempo => sc(s2, Key::Q),
        TimeSig => sc(s2, Key::T),
        KeySig => sc(s2, Key::K),
        Barline => sc(s2, Key::B),
        Endings => sc(s2, Key::E),
        Coda => sc(s2, Key::A),
        MeasureNumbers => sc(s2, Key::N),
        CompressRests => sc(s2, Key::R),
        AlignSpacing => sc(p, Key::J),
        AlignSpacingPrev => sc(ps, Key::J),
        AlignPlayback => sc(s2, Key::J),
        SwingPlayback => sc(s2s, Key::J),
        AddPage => sc(s2, Key::P),
        RemovePage => sc(s2s, Key::P),
        AddStaff => sc(s2, Key::S),
        DeleteStaff => sc(s2s, Key::S),
        SplitStaff => sc(s2, Key::F),
        TabStaff => sc(s2, Key::Num3),
        PercussionStaff => sc(s2, Key::Num1),
        ConnectStaves => sc(alt, Key::OpenBracket),
        CenterStaves => sc(s2s, Key::U),
        CenterSystems => sc(s2, Key::U),
        SystemsPerPage => sc(s2, Key::Y),
        MeasuresPerSystem => sc(s2, Key::W),
        TextElements => sc(s2, Key::H),
        MidiDevice => sc(s2, Key::I),
        MidiPlayback => sc(s2, Key::L),
        ShowHide => if mac { sc(p, Key::Semicolon) } else { sc(p, Key::H) },
        ControlPoints => sc(p, Key::Quote),
        LinearView => sc(p, Key::Y),
        FloatingWindows => if mac { sc(p, Key::K) } else { sc(alt, Key::K) },
        Palette(i) => sc(alt_shift, PALETTE_KEYS[i.min(10)]),
        OpenAllPalettes => sc(alt, Key::O),
        CloseAllPalettes => sc(alt, Key::P),
        StaffSheet => sc(p, Key::Slash),
        Keyboard => if mac { sc(s2s, Key::K) } else { sc(p, Key::K) },
        TempoWin => sc(p, Key::Num0),
        Toolbar => if mac { sc(alt_shift, Key::Q) } else { sc(p, Key::Q) },
        Tile => sc(Modifiers::SHIFT, Key::F4),
        Cascade => sc(Modifiers::SHIFT, Key::F5),
        ClickOnOff => sc(p, Key::F),
        AutoGuessBeam => sc(ps, Key::G),
        AutoSpace => sc(ps, Key::A),
        _ => None,
    }
}

/// The browser off the Mac: the keys used instead of the ones the browser keeps (all free in the Windows keys).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn web_alternative(c: Cmd) -> Option<KeyboardShortcut> {
    let cas = Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT;
    match c {
        Cmd::New => sc(cas, Key::E),
        Cmd::NewDefault => sc(cas, Key::D),
        Cmd::Close => sc(cas, Key::W),
        Cmd::Tie => sc(cas, Key::T),
        Cmd::FlipTie => sc(cas, Key::Y),
        _ => None,
    }
}

/// Plain-key shortcuts shown in menus (handled by the score window itself).
pub fn plain_key_text(c: Cmd) -> Option<&'static str> {
    use Cmd::*;
    match c {
        Play => Some("Space"),
        Stop => Some("Space"),
        Record => Some("Enter"),
        JumpToMeasure => Some("M"),
        ZoomIn => Some("Z"),
        ZoomOut => Some("Shift+Z"),
        Clear => Some("Delete"),
        LessMeasure => Some("["),
        MoreMeasure => Some("]"),
        QwertyMode => Some("Q"),
        _ => None,
    }
}

pub fn all_shortcut_cmds() -> Vec<Cmd> {
    use Cmd::*;
    let mut v = vec![
        New, NewDefault, Open, Close, Save, SaveAs, Revert, Print, PageSetup, Quit, Undo, Redo, Cut, Copy, Paste, SelectAll, NudgeLeft,
        NudgeRight, NudgeUp, NudgeDown, NoteAttributes, Enharmonic, StemUp, StemDown, AddMark, RemoveMark, Tie, FlipTie, SlurAbove,
        SlurBelow, BeamGroup, BeamOnBeat, FlattenBeam, ChangePitch, ChangeDuration, ChangeVelocity, MakeChord, MakeGrace, MakeTab,
        GuessDurations, RaisePitch, LowerPitch, RaiseOctave, LowerOctave, AddMeasure, DeleteMeasure, Tempo, TimeSig, KeySig, Barline,
        Endings, Coda, MeasureNumbers, CompressRests, AlignSpacing, AlignPlayback, SwingPlayback, AddPage, RemovePage, AddStaff,
        DeleteStaff, SplitStaff, TabStaff, ConnectStaves, CenterStaves, CenterSystems, SystemsPerPage, MeasuresPerSystem,
        TextElements, MidiDevice, MidiPlayback, ShowHide, ControlPoints, LinearView, FloatingWindows, StaffSheet, Keyboard, TempoWin,
        Toolbar, Tile, Cascade, ClickOnOff, AutoGuessBeam, AutoSpace, SimplifyAccidentals, StemNormal, OpenAllPalettes, CloseAllPalettes,
        PercussionStaff, UndoAlt, CutAlt, CopyAlt, PasteAlt, AlignSpacingPrev,
    ];
    for i in 1..=8 {
        v.push(Voice(i));
    }
    for i in 0..11 {
        v.push(Palette(i));
    }
    // most specific first, so Cmd+Shift+S wins over Cmd+S and Ctrl+Alt+M over Ctrl+M
    v.sort_by_key(|c| {
        let m = shortcut(*c).map(|s| s.modifiers).unwrap_or_default();
        -((m.alt as i32) + (m.shift as i32) + (m.ctrl as i32) + (m.mac_cmd as i32) + (m.command as i32))
    });
    v
}

pub fn label(c: Cmd) -> String {
    use Cmd::*;
    match c {
        New => "New Score...".into(),
        NewDefault => "New Default Score".into(),
        Open => "Open...".into(),
        Close => "Close".into(),
        Save => "Save".into(),
        SaveAs => "Save As...".into(),
        Revert => "Revert to Saved".into(),
        Print => "Print...".into(),
        PageSetup => "Page Setup...".into(),
        ImportMidi => "Import MIDI File...".into(),
        ImportXml => "Import MusicXML File...".into(),
        ExportMidi => "Export MIDI File...".into(),
        ExportXml => "Export MusicXML...".into(),
        ExportEnc => "Export Encore File (.enc)...".into(),
        ExportPng => "Export Page as PNG...".into(),
        ExportPdf => "Export PDF...".into(),
        ExtractPart => "Extract Part".into(),
        Quit => "Quit".into(),
        Undo => "Undo".into(),
        Redo => "Redo".into(),
        Cut => "Cut".into(),
        Copy => "Copy".into(),
        Paste => "Paste".into(),
        Clear => "Clear".into(),
        SelectAll => "Select All".into(),
        NudgeLeft => "Nudge Left".into(),
        NudgeRight => "Nudge Right".into(),
        NudgeUp => "Nudge Up".into(),
        NudgeDown => "Nudge Down".into(),
        NoteAttributes => "Note Attributes...".into(),
        Voice(n) => format!("Set Note to Voice {n}"),
        Enharmonic => "Enharmonic".into(),
        AccToSharps => "Accidentals to Sharps".into(),
        AccToFlats => "Accidentals to Flats".into(),
        StemUp => "Stems Up".into(),
        StemDown => "Stems Down".into(),
        StemNormal => "Normalize Stems".into(),
        AddMark => "Add Mark...".into(),
        RemoveMark => "Remove Mark...".into(),
        Tie => "Tie Notes".into(),
        FlipTie => "Flip Tie Direction".into(),
        SlurAbove => "Slur Notes (above)".into(),
        SlurBelow => "Slur Notes (below)".into(),
        BeamGroup => "Beam Group".into(),
        BeamOnBeat => "Beam On Beat".into(),
        SubGroupBeam => "Sub Group Beam".into(),
        FlattenBeam => "Flatten Beam".into(),
        ChangePitch => "Change Pitch...".into(),
        ChangeDuration => "Change Duration...".into(),
        ChangeVelocity => "Change Velocity...".into(),
        MakeChord => "Make Chord".into(),
        MakeGrace => "Make Grace/Cue".into(),
        MakeTab => "Make Tablature".into(),
        GuessDurations => "Guess Durations".into(),
        RevertToRaw => "Revert to Raw".into(),
        RaisePitch => "Raise Pitch".into(),
        LowerPitch => "Lower Pitch".into(),
        RaiseOctave => "Raise Pitch by Octave".into(),
        LowerOctave => "Lower Pitch by Octave".into(),
        Transpose => "Transpose...".into(),
        AddMeasure => "Add Measure...".into(),
        DeleteMeasure => "Delete Measure".into(),
        Tempo => "Tempo...".into(),
        TimeSig => "Time Signature...".into(),
        KeySig => "Key Signature...".into(),
        Barline => "Barline Type...".into(),
        Endings => "Endings...".into(),
        Coda => "Coda Phrases...".into(),
        MeasureNumbers => "Measure Numbers...".into(),
        CompressRests => "Compressed Rests".into(),
        AlignSpacing => "Align Spacing".into(),
        AlignPlayback => "Align Playback".into(),
        SwingPlayback => "Swing Playback...".into(),
        AddPage => "Add Page".into(),
        RemovePage => "Remove Page".into(),
        AddStaff => "Add Staff...".into(),
        DeleteStaff => "Delete Staff".into(),
        SplitStaff => "Split Staff...".into(),
        TabStaff => "Tab Staff".into(),

        ConnectStaves => "Connect Staves...".into(),
        CenterStaves => "Center Staves".into(),
        CenterSystems => "Center Systems".into(),
        SystemsPerPage => "Systems per Page...".into(),
        MeasuresPerSystem => "Measures per System...".into(),
        LessMeasure => "One Less Measure on System".into(),
        MoreMeasure => "One More Measure on System".into(),
        TextElements => "Text Elements...".into(),
        MidiDevice => "MIDI Device / Instrument...".into(),
        MidiPlayback => "MIDI Playback...".into(),
        ShowHide => "Show/Hide Hidden Items".into(),
        ControlPoints => "Show/Hide Control Points".into(),
        LinearView => "Linear View".into(),
        FloatingWindows => "Show/Hide Floating Windows".into(),
        ZoomIn => "Zoom In".into(),
        ZoomOut => "Zoom Out".into(),
        HideStaff => "Hide Staff".into(),
        ShowAllStaves => "Show All Staves".into(),
        Redraw => "Redraw Screen".into(),
        Palette(i) => PALETTES[i.min(10)].into(),
        StaffSheet => "Staff Sheet".into(),
        Keyboard => "Keyboard".into(),
        TempoWin => "Tempo".into(),
        Toolbar => "Toolbar".into(),
        Tile => "Tile Palettes".into(),
        Cascade => "Cascade Palettes".into(),
        CloseAllPalettes => "Close All Palettes".into(),
        ClickOnOff => "Click On/Off".into(),
        AutoGuessBeam => "Auto Guess/Beam".into(),
        AutoSpace => "Auto Space".into(),
        QwertyMode => "QWERTY Note Entry".into(),
        FollowPlayback => "Follow Playback".into(),
        Play => "Play".into(),
        Stop => "Stop".into(),
        Record => "Record".into(),
        JumpToMeasure => "Jump to Measure...".into(),
        Shortcuts => "Keyboard Shortcuts".into(),
        About => "About Fermata".into(),
        SimplifyAccidentals => "Simplify Accidentals".into(),
        MidiSetup => "MIDI Setup...".into(),
        ClickSetup => "Click Setup...".into(),
        ClavinovaGuide => "Como ligar o Clavinova (passo a passo)".into(),
        CheckUpdates => "Check for Updates...".into(),
        PenGestures => "Pencil Gestures".into(),
        PenMode => "Pen Mode".into(),
        TranscribeInk => "Transcribe Ink".into(),
        ClearInk => "Clear Ink".into(),
        PenImmediate => "Recognize each symbol right away".into(),
        RemoveAllMarks => "Remove All Marks".into(),
        OpenAllPalettes => "Open All Palettes".into(),
        SpacingDefaults => "Spacing Defaults...".into(),
        PercussionStaff => "Percussion Staff...".into(),
        UndoAlt => "Undo".into(),
        CutAlt => "Cut".into(),
        CopyAlt => "Copy".into(),
        PasteAlt => "Paste".into(),
        AlignSpacingPrev => "Align Spacing (previous settings)".into(),
    }
}

#[cfg(test)]
mod web_alternative_tests {
    use super::*;

    /// The browser's replacements for the keys Chrome and Edge keep must be free in the Windows keys.
    #[test]
    fn web_alternatives_are_free_in_the_windows_keys() {
        let taken: Vec<KeyboardShortcut> = all_shortcut_cmds().into_iter().filter_map(|c| shortcut_with(c, false, primary(), secondary())).collect();
        for c in [Cmd::New, Cmd::NewDefault, Cmd::Close, Cmd::Tie, Cmd::FlipTie] {
            let alt = web_alternative(c).unwrap();
            assert!(!taken.contains(&alt), "{c:?}: {alt:?} is already used");
        }
        assert!(web_alternative(Cmd::Save).is_none());
    }
}
