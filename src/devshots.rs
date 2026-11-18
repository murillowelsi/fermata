//! Development helpers for screenshots (`--dialog NAME`, `--clicks`, `--hover`): open any of the 36 dialogs or
//! the floating windows, and move or click the mouse at given frames, so menus, submenus and tips can be pictured.
// the browser build has no command-line flags for these
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
use crate::app::{App, Dialog};
use crate::commands::Cmd;
use crate::model::*;
use crate::score_view::NoteRef;

/// Every dialog by its name in the code (the `Dialog` variants), for `--dialog`.
pub const DIALOGS: [&str; 36] = [
    "NewScore", "TimeSig", "KeySig", "Tempo", "Barline", "Endings", "Coda", "MeasureNumbers", "AddMeasures", "MeasuresPerSystem", "SystemsPerPage", "Text",
    "NoteAttributes", "Velocity", "SpacingDefaults", "Duration", "Transpose", "Swing", "Tuplet", "MidiPlayback", "MidiSetup", "ClickSetup", "ClavinovaGuide",
    "JumpTo", "PageSetup", "AddStaff", "SplitStaff", "ConnectStaves", "ChordSymbol", "TextGraphic", "Lyric", "Expression", "Shortcuts", "About", "Message",
    "ConfirmClose",
];

/// Opens a dialog the way its menu command does when it can; otherwise with typical values.
/// Also accepts the floating windows "StaffSheet", "Keyboard" and "TempoWin". Returns false for an unknown name.
pub fn open(app: &mut App, name: &str) -> bool {
    use Cmd::*;
    match name {
        "StaffSheet" => {
            app.staff_sheet = true;
            return true;
        }
        "Keyboard" => {
            app.keyboard_win = true;
            return true;
        }
        "TempoWin" => {
            app.tempo_win = true;
            return true;
        }
        _ => {}
    }
    let cmd = match name {
        "NewScore" => Some(New),
        "TimeSig" => Some(TimeSig),
        "KeySig" => Some(KeySig),
        "Tempo" => Some(Tempo),
        "Barline" => Some(Barline),
        "Endings" => Some(Endings),
        "Coda" => Some(Coda),
        "MeasureNumbers" => Some(MeasureNumbers),
        "AddMeasures" => Some(AddMeasure),
        "MeasuresPerSystem" => Some(MeasuresPerSystem),
        "SystemsPerPage" => Some(SystemsPerPage),
        "Text" => Some(TextElements),
        "Velocity" => Some(ChangeVelocity),
        "SpacingDefaults" => Some(SpacingDefaults),
        "Duration" => Some(ChangeDuration),
        "Swing" => Some(SwingPlayback),
        "MidiPlayback" => Some(MidiPlayback),
        "MidiSetup" => Some(MidiSetup),
        "ClickSetup" => Some(ClickSetup),
        "ClavinovaGuide" => Some(ClavinovaGuide),
        "JumpTo" => Some(JumpToMeasure),
        "PageSetup" => Some(PageSetup),
        "AddStaff" => Some(AddStaff),
        "SplitStaff" => Some(SplitStaff),
        "ConnectStaves" => Some(ConnectStaves),
        "Shortcuts" => Some(Shortcuts),
        "About" => Some(About),
        _ => None,
    };
    if let Some(c) = cmd {
        app.exec(c);
    }
    if app.dialog.is_none() {
        let r = NoteRef { staff: 0, measure: 0, idx: 0 };
        app.dialog = match name {
            "NoteAttributes" => Some(Dialog::NoteAttributes { velocity: 80, play_len: TPQ, value: 3, dots: 0, voice: 1, hidden: false, stem: Stem::Auto, init: crate::app::AttrInit { play_len: TPQ, value: 3, ..Default::default() } }),
            "Transpose" => Some(Dialog::Transpose { semitones: 2, octave: 0 }),
            "Tuplet" => Some(Dialog::Tuplet { notes: 3, in_time: 2 }),
            "ChordSymbol" => Some(Dialog::ChordSymbol { root: "G".into(), kind: "7".into(), bass: String::new(), custom: String::new(), target: r }),
            "TextGraphic" => Some(Dialog::TextGraphic { text: "dolce".into(), index: 0, size: 1.6, bold: false }),
            "Lyric" => Some(Dialog::Lyric { target: r, line: 0, text: "Glo".into() }),
            "Expression" => Some(Dialog::Expression { text: "espress.".into(), target: r }),
            "Message" => Some(Dialog::Message("This file couldn't be opened: it isn't a Fermata or Encore-compatible score.".into())),
            "ConfirmClose" => Some(Dialog::ConfirmClose(crate::app::Then::Close)),
            _ => None,
        };
    }
    app.dialog.is_some() || DIALOGS.contains(&name)
}

/// One scripted mouse step: at `frame`, move to `pos` and (when `click`) press and release there.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub frame: u32,
    pub pos: egui::Pos2,
    pub click: bool,
}

/// Parses "x,y;x,y" (points in the window) into steps 20 frames apart, starting at frame 20.
pub fn parse_steps(s: &str, click: bool, first_frame: u32) -> Vec<Step> {
    s.split(';')
        .filter_map(|p| {
            let mut it = p.split(',').map(|v| v.trim().parse::<f32>());
            match (it.next(), it.next()) {
                (Some(Ok(x)), Some(Ok(y))) => Some(egui::pos2(x, y)),
                _ => None,
            }
        })
        .enumerate()
        .map(|(i, pos)| Step { frame: first_frame + 20 * i as u32, pos, click })
        .collect()
}

/// Adds the pointer events of the steps due at this frame to egui's input.
pub fn inject(steps: &[Step], frame: u32, raw: &mut egui::RawInput) {
    for s in steps {
        if s.frame == frame {
            raw.events.push(egui::Event::PointerMoved(s.pos));
            if s.click {
                raw.events.push(egui::Event::PointerButton { pos: s.pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
            }
        }
        if s.click && s.frame + 2 == frame {
            raw.events.push(egui::Event::PointerButton { pos: s.pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of the app's dialogs, with the given key pressed.
    fn frame(ctx: &egui::Context, app: &mut App, key: Option<egui::Key>) {
        let mut input = egui::RawInput::default();
        if let Some(k) = key {
            input.events.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
        }
        let _ = ctx.run_ui(input, |ui| app.dialogs(ui.ctx()));
    }

    #[test]
    fn midi_playback_cancel_restores_and_ok_is_one_undo_step() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut app = App::new(true);
        let before = app.score.playback.clone();
        // Cancel (Esc): the changes heard while the dialog was open are undone
        assert!(open(&mut app, "MidiPlayback"));
        frame(&ctx, &mut app, None);
        app.score.playback.swing = 70;
        app.score.playback.master_volume = 0.2;
        frame(&ctx, &mut app, Some(egui::Key::Escape));
        assert!(app.dialog.is_none());
        assert_eq!(app.score.playback, before);
        // OK (Enter): the changes stay, and a single undo brings back the old settings
        let undo_len = app.undo.len();
        assert!(open(&mut app, "MidiPlayback"));
        frame(&ctx, &mut app, None);
        app.score.playback.swing = 66;
        app.score.playback.click = !before.click;
        frame(&ctx, &mut app, Some(egui::Key::Enter));
        assert!(app.dialog.is_none());
        assert_eq!(app.score.playback.swing, 66);
        assert_eq!(app.undo.len(), undo_len + 1);
        app.do_undo();
        assert_eq!(app.score.playback, before);
    }

    #[test]
    fn every_dialog_opens_by_name() {
        let mut app = App::new(true);
        for n in DIALOGS {
            app.dialog = None;
            assert!(open(&mut app, n), "{n}");
            assert!(app.dialog.is_some(), "{n} didn't open");
        }
    }
}
