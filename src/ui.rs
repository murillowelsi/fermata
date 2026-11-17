//! Menus, toolbars, floating palettes, dialogs and side windows.
use crate::app::*;
use crate::commands::{self, Cmd, PALETTES};
use crate::glyphs as g;
use crate::model::*;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

const PAL_BG: Color32 = Color32::from_rgb(214, 214, 214);

/// "Ctrl+Alt+Shift+M", "Cmd+S": words rather than the Mac's modifier symbols, the same on every computer.
fn fmt_shortcut(s: &egui::KeyboardShortcut) -> String {
    let m = s.modifiers;
    let mut t = String::new();
    if m.ctrl {
        t.push_str("Ctrl+");
    }
    if m.mac_cmd || (m.command && !m.ctrl) {
        t.push_str("Cmd+");
    }
    if m.alt {
        t.push_str("Alt+"); // the Option key on a Mac keyboard
    }
    if m.shift {
        t.push_str("Shift+");
    }
    let k = match s.logical_key {
        egui::Key::OpenBracket => "[".to_string(),
        egui::Key::CloseBracket => "]".to_string(),
        egui::Key::Equals => "=".to_string(),
        egui::Key::Minus => "-".to_string(),
        egui::Key::Semicolon => ";".to_string(),
        egui::Key::Quote => "'".to_string(),
        egui::Key::Slash => "/".to_string(),
        egui::Key::Period => ".".to_string(),
        egui::Key::Backspace => "Backspace".to_string(),
        egui::Key::Delete => "Del".to_string(),
        egui::Key::Insert => "Ins".to_string(),
        k => k.name().trim_start_matches("Num").to_string(),
    };
    t + &k
}

fn shortcut_text(_ctx: &egui::Context, c: Cmd) -> String {
    if let Some(s) = commands::shortcut(c) {
        // Windows Encore's Ctrl keys first; on the Mac also the Cmd key that does the same (e.g. "Ctrl+T / Cmd+T")
        match commands::mac_shortcut(c) {
            Some(w) => format!("{} / {}", fmt_shortcut(&s), fmt_shortcut(&w)),
            None => fmt_shortcut(&s),
        }
    } else {
        commands::plain_key_text(c).unwrap_or("").to_string()
    }
}

impl App {
    fn mi(&mut self, ui: &mut Ui, c: Cmd) {
        let txt = shortcut_text(ui.ctx(), c);
        if ui.add(egui::Button::new(commands::label(c)).shortcut_text(txt)).clicked() {
            self.pending_cmd = Some(c);
            ui.close();
        }
    }
    fn mi_check(&mut self, ui: &mut Ui, c: Cmd, on: bool) {
        let txt = shortcut_text(ui.ctx(), c);
        // room for a check mark, drawn as lines (the bundled font has no check symbol)
        let l = format!("      {}", commands::label(c));
        let resp = ui.add(egui::Button::new(l).shortcut_text(txt));
        if on {
            let r = resp.rect;
            let x = r.left() + 8.0;
            let y = r.center().y;
            let col = ui.visuals().text_color();
            ui.painter().line_segment([egui::pos2(x, y), egui::pos2(x + 3.5, y + 3.5)], Stroke::new(1.8, col));
            ui.painter().line_segment([egui::pos2(x + 3.5, y + 3.5), egui::pos2(x + 10.0, y - 4.5)], Stroke::new(1.8, col));
        }
        if resp.clicked() {
            self.pending_cmd = Some(c);
            ui.close();
        }
    }

    pub fn menu_bar(&mut self, ui: &mut Ui) {
        use Cmd::*;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for c in [New, NewDefault, Open, Close] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [Save, SaveAs, Revert] {
                    self.mi(ui, c);
                }
                ui.separator();
                ui.menu_button("Export", |ui| {
                    for c in [ExportEnc, ExportXml, ExportMidi, ExportPng] {
                        self.mi(ui, c);
                    }
                });
                ui.menu_button("Import", |ui| self.mi(ui, ImportMidi));
                self.mi(ui, ExtractPart);
                ui.separator();
                for c in [Print, PageSetup] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, Quit);
            });
            ui.menu_button("Edit", |ui| {
                for c in [Undo, Redo] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [Cut, Copy, Paste, Clear, SelectAll] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [NudgeLeft, NudgeRight, NudgeUp, NudgeDown] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, JumpToMeasure);
            });
            ui.menu_button("Notes", |ui| {
                ui.menu_button("Attributes", |ui| self.mi(ui, NoteAttributes));
                ui.menu_button("Voice", |ui| {
                    for v in 1..=8 {
                        self.mi(ui, Voice(v));
                    }
                });
                ui.menu_button("Accidentals", |ui| {
                    for c in [Enharmonic, SimplifyAccidentals, AccToSharps, AccToFlats] {
                        self.mi(ui, c);
                    }
                });
                ui.menu_button("Stems", |ui| {
                    for c in [StemUp, StemDown, StemNormal] {
                        self.mi(ui, c);
                    }
                });
                ui.menu_button("Marks", |ui| {
                    for c in [AddMark, RemoveMark, RemoveAllMarks] {
                        self.mi(ui, c);
                    }
                });
                ui.separator();
                for c in [Tie, FlipTie, SlurAbove, SlurBelow] {
                    self.mi(ui, c);
                }
                ui.menu_button("Beams", |ui| {
                    for c in [BeamGroup, BeamOnBeat, SubGroupBeam, FlattenBeam] {
                        self.mi(ui, c);
                    }
                });
                ui.separator();
                for c in [ChangePitch, ChangeDuration, ChangeVelocity] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [MakeChord, MakeGrace, MakeTab] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [RevertToRaw, GuessDurations] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [RaisePitch, LowerPitch, RaiseOctave, LowerOctave] {
                    self.mi(ui, c);
                }
            });
            ui.menu_button("Measures", |ui| {
                for c in [AddMeasure, DeleteMeasure] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [Tempo, TimeSig, KeySig] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [Barline, Endings, Coda] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [MeasureNumbers, CompressRests] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [AlignPlayback, SwingPlayback] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, AlignSpacing);
            });
            ui.menu_button("Score", |ui| {
                for c in [AddPage, RemovePage] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [AddStaff, DeleteStaff, SplitStaff, TabStaff, PercussionStaff] {
                    self.mi(ui, c);
                }
                ui.separator();
                for c in [ConnectStaves, CenterStaves, CenterSystems] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, SystemsPerPage);
                ui.separator();
                for c in [MeasuresPerSystem, LessMeasure, MoreMeasure] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, TextElements);
                ui.separator();
                for c in [MidiDevice, MidiPlayback] {
                    self.mi(ui, c);
                }
            });
            ui.menu_button("View", |ui| {
                let (a, b, c2, d) = (self.show_hidden, self.control_points, self.linear, self.floating_hidden);
                self.mi_check(ui, ShowHide, a);
                self.mi_check(ui, ControlPoints, b);
                ui.separator();
                self.mi(ui, SpacingDefaults);
                ui.separator();
                for c in [ShowAllStaves, HideStaff] {
                    self.mi(ui, c);
                }
                ui.separator();
                self.mi(ui, Redraw);
                ui.separator();
                self.mi_check(ui, LinearView, c2);
                self.mi_check(ui, FloatingWindows, !d);
                ui.separator();
                for c in [ZoomIn, ZoomOut] {
                    self.mi(ui, c);
                }
            });
            ui.menu_button("Windows", |ui| {
                ui.menu_button("Palette", |ui| {
                    for i in 0..11 {
                        let on = self.palettes[i];
                        self.mi_check(ui, Palette(i), on);
                    }
                });
                self.mi(ui, OpenAllPalettes);
                self.mi(ui, CloseAllPalettes);
                ui.separator();
                let (a, b, c2, d) = (self.staff_sheet, self.keyboard_win, self.tempo_win, self.toolbar_open);
                self.mi_check(ui, StaffSheet, a);
                self.mi_check(ui, Keyboard, b);
                self.mi_check(ui, TempoWin, c2);
                self.mi_check(ui, Toolbar, d);
                ui.separator();
                self.mi(ui, Tile);
                self.mi(ui, Cascade);
            });
            ui.menu_button("Setup", |ui| {
                let (a, b, c2, d, e) = (self.prefs.click_on, self.score.playback.follow, self.auto_beam, self.auto_space, self.qwerty);
                self.mi_check(ui, ClickOnOff, a);
                self.mi(ui, ClickSetup);
                self.mi_check(ui, FollowPlayback, b);
                self.mi_check(ui, AutoGuessBeam, c2);
                self.mi_check(ui, AutoSpace, d);
                ui.separator();
                self.mi_check(ui, QwertyMode, e);
                ui.separator();
                self.mi(ui, MidiSetup);
                self.mi(ui, MidiPlayback);
                ui.checkbox(&mut self.audition, "Audition notes while editing");
            });
            ui.menu_button("Help", |ui| {
                self.mi(ui, ClavinovaGuide);
                ui.separator();
                self.mi(ui, Shortcuts);
                self.mi(ui, About);
            });
        });
    }

    /// Score Window Toolbar: voice, tools, transport, zoom, thru, measure, pages.
    /// Score Window Toolbar, in the original's order (Quick Start QS-4 and its screen):
    /// Voice ▾ | Arrow Eraser Pencil | Record Play Stop Click | Thru A1 | Zoom Play-from | M1 | ◀ page ▶
    pub fn score_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let vlabel = match self.show_voice {
                None => "Voice -".to_string(),
                Some(v) => format!("Voice {v} -"),
            };
            // a light box with dark text, like the original's "Voice -" button, opening the voice menu
            let vresp = field_button(ui, &vlabel, 58.0).on_hover_text("Voice: which voice you enter, and which voices are shown (V then 1-8 / V then A)");
            egui::Popup::menu(&vresp).show(|ui| {
                for v in 1..=8u8 {
                    if ui.selectable_label(self.voice == v, format!("Enter in voice {v}")).clicked() {
                        self.voice = v;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.selectable_label(self.show_voice.is_none(), "Show all voices").clicked() {
                    self.show_voice = None;
                    ui.close();
                }
                for v in 1..=8u8 {
                    if ui.selectable_label(self.show_voice == Some(v), format!("Show voice {v} only")).clicked() {
                        self.show_voice = Some(v);
                        ui.close();
                    }
                }
            });
            strip_sep(ui);
            for (t, kind, tip) in [(Tool::Arrow, 0, "Arrow tool (A / Esc)"), (Tool::Eraser, 1, "Eraser tool (E)"), (Tool::Pencil, 2, "Pencil tool (P)")] {
                if icon_button(ui, kind, self.tool == t).on_hover_text(tip).clicked() {
                    self.tool = t;
                }
            }
            strip_sep(ui);
            if icon_button(ui, 3, self.rec.is_some()).on_hover_text("Record (Enter)").clicked() {
                self.pending_cmd = Some(Cmd::Record);
            }
            let playing = self.is_playing();
            if icon_button(ui, 4, playing).on_hover_text("Play (Space)").clicked() {
                self.pending_cmd = Some(Cmd::Play);
            }
            if icon_button(ui, 5, false).on_hover_text("Stop (Space)").clicked() {
                self.pending_cmd = Some(Cmd::Stop);
            }
            if icon_button(ui, 8, self.prefs.click_on).on_hover_text("Click On/Off").clicked() {
                self.pending_cmd = Some(Cmd::ClickOnOff);
            }
            strip_sep(ui);
            let (tp, tc) = self.thru_target();
            let thru = if self.audition { format!("Thru {}{}", if tp == 0 { "A" } else { "B" }, tc + 1) } else { "Thru Off".to_string() };
            if field_button(ui, &thru, 62.0).on_hover_text("MIDI Thru: what you play goes to this port and channel (click to turn on/off)").clicked() {
                self.audition = !self.audition;
                self.prefs.thru_on = self.audition;
                if !self.screenshot_mode {
                    crate::prefs::save(&self.prefs);
                }
            }
            strip_sep(ui);
            let zr = icon_button(ui, 6, false).on_hover_text("Zoom: click to zoom in, right-click to zoom out (Z / Shift+Z)");
            if zr.clicked() {
                self.pending_cmd = Some(Cmd::ZoomIn);
            }
            if zr.secondary_clicked() {
                self.pending_cmd = Some(Cmd::ZoomOut);
            }
            if icon_button(ui, 9, false).on_hover_text("Play from the cursor measure").clicked() {
                let m = self.cursor.1;
                self.play_from(m);
            }
            strip_sep(ui);
            let m = self.play_position().map(|x| x.0).unwrap_or(self.cursor.1);
            if field_button(ui, &format!("M{}", m + 1), 62.0).on_hover_text("Jump to measure (M)").clicked() {
                self.pending_cmd = Some(Cmd::JumpToMeasure);
            }
            strip_sep(ui);
            // page navigation: ◀ [page] ▶
            let pages = self.geo.pages.len().max(1);
            let cur = self.geo.systems.iter().find(|s| m >= s.m0 && m < s.m1).map(|s| s.page).unwrap_or(0).min(pages - 1);
            if icon_button(ui, 10, false).on_hover_text("Previous page").clicked() && cur > 0 {
                self.goto_page(cur - 1);
            }
            page_icon(ui, cur + 1).on_hover_text(format!("Page {} of {}", cur + 1, pages));
            if icon_button(ui, 11, false).on_hover_text("Next page").clicked() && cur + 1 < pages {
                self.goto_page(cur + 1);
            }
        });
    }

    fn goto_page(&mut self, p: usize) {
        if let Some(r) = self.geo.pages.get(p) {
            self.scroll_to = Some(*r);
        }
        if let Some(sys) = self.geo.systems.iter().find(|s| s.page == p) {
            self.cursor = (self.cursor.0, sys.m0, 0);
        }
    }

    /// The floating customizable toolbar (Windows ▸ Toolbar), a floating window as in the original.
    pub fn big_toolbar(&mut self, ui: &mut Ui) {
        use Cmd::*;
        let items: &[(&str, Cmd)] = &[
            ("New", New), ("Open", Open), ("Save", Save), ("Print", Print), ("Undo", Undo), ("Cut", Cut), ("Copy", Copy), ("Paste", Paste), ("Clear", Clear), ("All", SelectAll),
            ("<", NudgeLeft), (">", NudgeRight), ("^", NudgeUp), ("v", NudgeDown), ("Tempo", Tempo), ("4/4", TimeSig), ("Key", KeySig), ("+Bar", AddMeasure), ("-Bar", DeleteMeasure), ("||", Barline),
            ("1.", Endings), ("D.S.", Coda), ("12", MeasureNumbers), ("Rests", CompressRests), ("Space", AlignSpacing), ("Info", NoteAttributes), ("Enh", Enharmonic), ("#", AccToSharps), ("b", AccToFlats), ("Up", StemUp),
            ("Down", StemDown), ("Tie", Tie), ("Slur", SlurAbove), ("Beam", BeamGroup), ("Beat", BeamOnBeat), ("Flat", FlattenBeam), ("Pitch", ChangePitch), ("Dur", ChangeDuration), ("Vel", ChangeVelocity), ("Chord", MakeChord),
            ("Grace", MakeGrace), ("TAB", MakeTab), ("Guess", GuessDurations), ("+Stf", AddStaff), ("-Stf", DeleteStaff), ("Split", SplitStaff), ("{", ConnectStaves), ("Text", TextElements), ("Play", Play), ("Stop", Stop),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            for (icon, c) in items {
                let tip = format!("{}  {}", commands::label(*c), shortcut_text(ui.ctx(), *c));
                if ui.add_sized([38.0, 22.0], egui::Button::new(egui::RichText::new(*icon).size(11.5))).on_hover_text(tip).clicked() {
                    self.pending_cmd = Some(*c);
                }
            }
            ui.separator();
            let lin = if self.linear { "Page View" } else { "Linear View" };
            if ui.add_sized([80.0, 24.0], egui::Button::new(lin)).on_hover_text("Linear View  Cmd+Y").clicked() {
                self.pending_cmd = Some(LinearView);
            }
            let click = if self.score.playback.click { "Click On" } else { "Click Off" };
            if ui.add_sized([70.0, 24.0], egui::Button::new(click)).clicked() {
                self.pending_cmd = Some(ClickOnOff);
            }
        });
    }

    pub fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (s, m, t) = self.cursor;
            let mi = &self.score.measures[m.min(self.score.measures.len() - 1)];
            let beat = t / (TPQ * 4 / mi.den.max(1) as u32) + 1;
            ui.label(format!("Staff {}  Measure {}  Beat {}  |  {}/{}  {}  |  Tempo {}", s + 1, m + 1, beat, mi.num, mi.den, key_name(mi.key), self.score.tempo_at(m)));
            ui.separator();
            ui.label(format!("{:.0}%", self.zoom / 1.35 * 100.0));
            ui.separator();
            let (txt, col) = match &self.midi {
                Some(mm) if mm.connected() => {
                    let last = mm.last_note.load(std::sync::atomic::Ordering::Relaxed);
                    let heard = if last == 255 { "no notes yet".to_string() } else { format!("last note {}", Pitch::from_midi(last as i32, false).name()) };
                    (format!("MIDI in: {} ({heard})", mm.names.join(", ")), Color32::from_rgb(20, 130, 40))
                }
                _ => ("No MIDI keyboard".to_string(), Color32::from_gray(120)),
            };
            if ui.add(egui::Label::new(egui::RichText::new(txt).color(col)).sense(Sense::click())).on_hover_text("MIDI Setup").clicked() {
                self.pending_cmd = Some(Cmd::MidiSetup);
            }
            if let Some(mo) = self.midi_out.as_ref() {
                match (&mo.chosen[0], &mo.connected[0]) {
                    (Some(_), Some(o)) => {
                        ui.label(egui::RichText::new(format!("Out A: {o} ({} sent)", mo.sent[0].load(std::sync::atomic::Ordering::Relaxed))).color(Color32::from_rgb(20, 130, 40)));
                    }
                    (Some(c), None) => {
                        ui.label(egui::RichText::new(format!("Out A: {c} not found")).color(Color32::from_rgb(190, 60, 0)));
                    }
                    _ => {}
                }
            }
            ui.label(format!("Split = {}", Pitch::from_midi(self.rec_settings.split as i32, false).name()));
            if self.qwerty {
                ui.label(egui::RichText::new("QWERTY").strong().color(Color32::from_rgb(30, 90, 230)));
            }
            if self.rec.is_some() {
                ui.label(egui::RichText::new("* REC").strong().color(Color32::from_rgb(210, 30, 30)));
            }
            ui.separator();
            ui.label(&self.status);
        });
    }

    // ---------------- palettes ----------------

    pub fn palettes_ui(&mut self, ctx: &egui::Context) {
        if self.floating_hidden {
            return;
        }
        let gen = self.palette_layout_gen;
        let default_pos = [(6.0, 128.0), (96.0, 128.0), (96.0, 420.0), (96.0, 300.0), (1210.0, 128.0), (1300.0, 330.0), (1300.0, 128.0), (1210.0, 470.0), (1120.0, 470.0), (1120.0, 128.0), (6.0, 560.0)];
        for i in 0..11 {
            if !self.palettes[i] {
                continue;
            }
            let mut open = true;
            let pos = if self.palette_cascade && gen > 0 { pos2(60.0 + i as f32 * 28.0, 120.0 + i as f32 * 24.0) } else { pos2(default_pos[i].0, default_pos[i].1) };
            let mut w = egui::Window::new(PALETTES[i])
                .id(egui::Id::new(("palette", i, gen)))
                .open(&mut open)
                .resizable(false)
                .collapsible(true)
                .title_bar(true)
                .min_width(10.0)
                .default_width(if i == 9 { 84.0 } else if i == 4 { 58.0 } else { 70.0 })
                .max_width(if i == 9 { 84.0 } else if i == 4 { 58.0 } else { 74.0 })
                .default_pos(pos)
                .frame(egui::Frame::window(&ctx.global_style()).fill(PAL_BG).inner_margin(3.0));
            if gen > 0 {
                w = w.current_pos(pos);
            }
            w.show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                self.palette_contents(ui, i);
            });
            if !open {
                self.palettes[i] = false;
            }
        }
    }

    fn pal_button(&mut self, ui: &mut Ui, item: Item, tip: &str, w: f32, draw: impl FnOnce(&egui::Painter, Rect, Color32)) {
        let _ = self.pal_button_r(ui, item, tip, w, draw);
    }

    fn pal_button_r(&mut self, ui: &mut Ui, item: Item, tip: &str, w: f32, draw: impl FnOnce(&egui::Painter, Rect, Color32)) -> egui::Response {
        // the tuplet button works with the note value beside it, so it shows as pressed while it's on
        let selected = match item {
            Item::Tuplet => self.cur_tuplet,
            Item::Dot(d) => self.cur_dots == d,
            _ => self.item == item,
        };
        let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
        let bg = if selected { Color32::from_rgb(70, 70, 70) } else if resp.hovered() { Color32::from_gray(240) } else { Color32::from_gray(228) };
        let fg = if selected { Color32::WHITE } else { Color32::BLACK };
        let p = ui.painter_at(r);
        p.rect_filled(r, 2.0, bg);
        p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
        draw(&p, r, fg);
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            self.pick_item(item);
        }
        resp
    }

    pub fn pick_item(&mut self, item: Item) {
        match &item {
            Item::Note(v) => {
                self.cur_value = *v;
                self.tool = Tool::Pencil;
            }
            Item::Rest(v) => {
                self.cur_value = *v;
                self.tool = Tool::Pencil;
            }
            Item::Color(c) => {
                let c = *c;
                if !self.selected.is_empty() {
                    self.modify(|e| e.color = c);
                }
                if !self.sel_graphics.is_empty() {
                    for &i in &self.sel_graphics.clone() {
                        if let Some(gr) = self.score.graphics.get_mut(i) {
                            gr.color = c;
                        }
                    }
                }
            }
            Item::Dot(d) => {
                // the dot goes together with the chosen note or rest value (both stay pressed): the next notes
                // entered are dotted. With the arrow tool, it dots the selected notes instead.
                self.cur_dots = if self.cur_dots == *d { 0 } else { *d };
                let d = self.cur_dots;
                if self.tool == Tool::Arrow && !self.selected.is_empty() {
                    let ms: Vec<(usize, usize)> = self.selected.iter().map(|r| (r.staff, r.measure)).collect();
                    self.modify(|e| e.dots = d);
                    self.revert_if_overflow(&ms);
                    return;
                }
                let names = ["double whole", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th", "128th"];
                let kind = if matches!(self.item, Item::Rest(_)) { "rest" } else { "note" };
                self.status = match d {
                    0 => format!("Dot off: {} {}.", names[self.cur_value.min(8) as usize], kind),
                    1 => format!("Dotted {} {} selected.", names[self.cur_value.min(8) as usize], kind),
                    _ => format!("Double-dotted {} {} selected.", names[self.cur_value.min(8) as usize], kind),
                };
                if !matches!(self.item, Item::Note(_) | Item::Rest(_)) {
                    self.item = Item::Note(self.cur_value);
                }
                self.tool = Tool::Pencil;
                return;
            }
            Item::Tuplet => {
                // a modifier like the dots: the chosen note or rest stays the tool, its notes become tuplets
                self.cur_tuplet = !self.cur_tuplet;
                let (a, n) = self.tuplet_ratio;
                if self.cur_tuplet && self.targets().len() >= 2 {
                    self.make_tuplet(Some((a, n)));
                }
                self.status = if self.cur_tuplet { format!("Tuplet {a}:{n} on: the next notes are {a} in the time of {n}.") } else { "Tuplet off.".into() };
                if !matches!(self.item, Item::Note(_) | Item::Rest(_)) {
                    self.item = Item::Note(self.cur_value);
                }
                self.tool = Tool::Pencil;
                return;
            }
            _ => {}
        }
        self.status = format!("Tool: {}", item_name(&item));
        self.item = item;
        if self.tool == Tool::Eraser {
            self.tool = Tool::Pencil;
        }
    }

    fn text_btn(&mut self, ui: &mut Ui, item: Item, label: &str, w: f32) {
        let l = label.to_string();
        self.pal_button(ui, item, label, w, move |p, r, c| {
            p.text(r.center(), Align2::CENTER_CENTER, &l, FontId::proportional(13.0), c);
        });
    }

    fn palette_contents(&mut self, ui: &mut Ui, i: usize) {
        match i {
            0 => {
                // Notes
                egui::Grid::new("notes_pal").spacing([1.0, 1.0]).show(ui, |ui| {
                    for v in 0..=8u8 {
                        let names = ["Double whole", "Whole", "Half", "Quarter", "Eighth", "16th", "32nd", "64th", "128th"];
                        self.pal_button(ui, Item::Rest(v), &format!("{} rest ({})", names[v as usize], v), 30.0, move |p, r, c| g::rest(p, r.center() + vec2(0.0, if v == 1 { 4.0 } else if v == 2 { 2.0 } else { -2.0 }), 5.0, v, c));
                        self.pal_button(ui, Item::Note(v), &format!("{} note ({})", names[v as usize], v), 30.0, move |p, r, c| {
                            let ctr = r.center() + vec2(-3.0, 6.0);
                            g::notehead(p, ctr, 5.0, v, c);
                            if v >= 2 {
                                p.line_segment([ctr + vec2(3.0, 0.0), ctr + vec2(3.0, -17.0)], Stroke::new(1.0, c));
                            }
                            if v >= 4 {
                                g::flag(p, ctr + vec2(3.0, -17.0), 4.0, (v - 3).min(3), true, c);
                            }
                        });
                        ui.end_row();
                    }
                    self.pal_button(ui, Item::Accidental(2), "Double sharp (Shift+S)", 30.0, |p, r, c| g::double_sharp(p, r.center(), 5.0, c));
                    self.pal_button(ui, Item::Accidental(1), "Sharp (S)", 30.0, |p, r, c| g::sharp(p, r.center(), 6.0, c));
                    ui.end_row();
                    self.pal_button(ui, Item::Accidental(-2), "Double flat (Shift+F)", 30.0, |p, r, c| g::accidental(p, r.center() + vec2(0.0, 3.0), 5.0, -2, c));
                    self.pal_button(ui, Item::Accidental(-1), "Flat (F)", 30.0, |p, r, c| g::flat(p, r.center() + vec2(0.0, 3.0), 6.0, c));
                    ui.end_row();
                    self.text_btn(ui, Item::Courtesy, "( )", 30.0);
                    self.pal_button(ui, Item::Accidental(0), "Natural (N)", 30.0, |p, r, c| g::natural(p, r.center(), 6.0, c));
                    ui.end_row();
                    self.text_btn(ui, Item::Dot(2), "··", 30.0);
                    self.text_btn(ui, Item::Dot(1), "·", 30.0);
                    ui.end_row();
                });
                let (a, n) = self.tuplet_ratio;
                let lbl = format!("{a}:{n}");
                let l2 = lbl.clone();
                let resp = self.pal_button_r(ui, Item::Tuplet, "Tuplet. Double-click tool to change value", 61.0, move |p, r, c| {
                    p.text(r.center(), Align2::CENTER_CENTER, &l2, FontId::proportional(13.0), c);
                });
                if resp.double_clicked() || resp.secondary_clicked() {
                    self.cur_tuplet = true;
                    self.dialog = Some(Dialog::Tuplet { notes: a, in_time: n });
                }
            }
            1 => {
                egui::Grid::new("clefs_pal").spacing([1.0, 1.0]).show(ui, |ui| {
                    for (k, c) in Clef::ALL.iter().enumerate() {
                        let cl = *c;
                        self.pal_button(ui, Item::Clef(cl), &format!("{} clef", cl.name()), 36.0, move |p, r, col| {
                            crate::score_view::draw_clef(p, r.center() + vec2(-2.0, 0.0), 3.4, cl, col)
                        });
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            }
            2 => {
                egui::Grid::new("gfx_pal").spacing([1.0, 1.0]).show(ui, |ui| {
                    self.text_btn(ui, Item::Graphic(GraphicKind::Line), "L", 32.0);
                    self.text_btn(ui, Item::Graphic(GraphicKind::Text), "T", 32.0);
                    ui.end_row();
                    self.text_btn(ui, Item::Chord, "C", 32.0);
                    self.text_btn(ui, Item::ChordGrid, "Grid", 32.0);
                    ui.end_row();
                    self.text_btn(ui, Item::Graphic(GraphicKind::Rect), "Box", 32.0);
                    self.text_btn(ui, Item::Graphic(GraphicKind::Bracket), "[ ]", 32.0);
                    ui.end_row();
                    self.text_btn(ui, Item::Graphic(GraphicKind::RoundRect), "Rnd", 32.0);
                    self.text_btn(ui, Item::Graphic(GraphicKind::Ellipse), "O", 32.0);
                    ui.end_row();
                });
            }
            3 => {
                egui::Grid::new("tools_pal").spacing([1.0, 1.0]).show(ui, |ui| {
                    self.pal_button(ui, Item::Graphic(GraphicKind::Slur), "Slur (drag from note to note)", 32.0, |p, r, c| g::tie_or_slur(p, r.center() + vec2(-10.0, 4.0), r.center() + vec2(10.0, 4.0), 4.0, true, c));
                    self.pal_button(ui, Item::Graphic(GraphicKind::Crescendo), "Crescendo hairpin (Shift flips)", 32.0, |p, r, c| g::hairpin(p, r.center() + vec2(-10.0, 0.0), r.center() + vec2(10.0, 0.0), 6.0, true, c));
                    ui.end_row();
                    self.text_btn(ui, Item::TempoTool, "Tmp", 32.0);
                    self.pal_button(ui, Item::Graphic(GraphicKind::Decrescendo), "Decrescendo hairpin", 32.0, |p, r, c| g::hairpin(p, r.center() + vec2(-10.0, 0.0), r.center() + vec2(10.0, 0.0), 6.0, false, c));
                    ui.end_row();
                    self.text_btn(ui, Item::Graphic(GraphicKind::TrillLine), "tr~", 32.0);
                    self.text_btn(ui, Item::Mark(Mark::Arpeggio), "Arp", 32.0);
                    ui.end_row();
                    self.text_btn(ui, Item::Graphic(GraphicKind::Ottava), "8va", 32.0);
                    self.text_btn(ui, Item::Graphic(GraphicKind::OttavaBassa), "8vb", 32.0);
                    ui.end_row();
                    self.text_btn(ui, Item::Lyric, "Lyric", 32.0);
                    self.text_btn(ui, Item::Chord, "Chd", 32.0);
                    ui.end_row();
                });
                self.text_btn(ui, Item::MidiTool, "MIDI", 65.0);
            }
            4 => {
                let order = ["pppppp", "ppppp", "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fffff", "ffffff", "sf", "sfp", "sfpp", "fp", "rf", "rfz", "sfz", "sffz", "fz"];
                for d in order {
                    let s = d.to_string();
                    self.pal_button(ui, Item::Dynamic(s.clone()), &format!("Dynamic {d}"), 46.0, move |p, r, c| crate::score_view::dynamic_text(p, r.center(), 6.0, &s, c));
                }
            }
            5 | 6 | 7 => {
                let marks: Vec<Mark> = match i {
                    5 => vec![Mark::Trill, Mark::Turn, Mark::Mordent, Mark::InvMordent, Mark::Plus, Mark::Harmonic, Mark::UpBow, Mark::DownBow],
                    6 => vec![Mark::Tenuto, Mark::Staccato, Mark::Accent, Mark::Marcato, Mark::Staccatissimo],
                    _ => vec![Mark::Fermata, Mark::FermataBelow, Mark::Tremolo1, Mark::Tremolo2, Mark::Tremolo3, Mark::Finger(0), Mark::Finger(1), Mark::Finger(2), Mark::Finger(3), Mark::Finger(4), Mark::Finger(5), Mark::Segno, Mark::Coda, Mark::Breath, Mark::Caesura, Mark::PedalDown, Mark::PedalUp],
                };
                egui::Grid::new(("marks", i)).spacing([1.0, 1.0]).show(ui, |ui| {
                    for (k, mk) in marks.into_iter().enumerate() {
                        self.pal_button(ui, Item::Mark(mk), &mk.label(), 32.0, move |p, r, c| g::mark(p, r.center(), 5.0, mk, true, c));
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            }
            8 => {
                egui::Grid::new("guitar").spacing([1.0, 1.0]).show(ui, |ui| {
                    for (k, t) in ["p", "i", "m", "a", "c", "P", "H", "T", "(1)", "(2)", "(3)", "(4)", "(5)", "(6)", "(7)", "(8)", "B", "1/2", "Rel.", "vib."].iter().enumerate() {
                        self.text_btn(ui, Item::Guitar(t.to_string()), t, 28.0);
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            }
            9 => {
                for t in [
                    "cresc.", "decresc.", "poco a poco", "rit.", "legato", "staccato", "dolce", "ritard.", "accel.", "dim.", "leggiero", "Tempo I", "Largo", "Adagio",
                    "Andante", "Allegro", "Presto", "a tempo", "attacca", "espress.", "sotto voce", "perdendosi", "Tranquillo",
                ] {
                    self.text_btn(ui, Item::Expression(t.to_string()), t, 78.0);
                }
            }
            _ => {
                let cols = [
                    Rgb(128, 0, 0), Rgb(255, 0, 0), Rgb(0, 0, 128), Rgb(0, 0, 255), Rgb(0, 128, 128), Rgb(0, 255, 255), Rgb(0, 128, 0), Rgb(0, 255, 0),
                    Rgb(128, 128, 0), Rgb(100, 255, 200), Rgb(128, 0, 128), Rgb(255, 0, 255), Rgb(128, 128, 128), Rgb(200, 200, 200), Rgb(255, 255, 255), Rgb(0, 0, 0),
                ];
                egui::Grid::new("colors").spacing([1.0, 1.0]).show(ui, |ui| {
                    for (k, c) in cols.into_iter().enumerate() {
                        self.pal_button(ui, Item::Color(c), "Color (applies to selection)", 26.0, move |p, r, _| {
                            p.rect_filled(r.shrink(5.0), 0.0, Color32::from_rgb(c.0, c.1, c.2));
                            p.rect_stroke(r.shrink(5.0), 0.0, Stroke::new(1.0, Color32::DARK_GRAY), egui::StrokeKind::Inside);
                        });
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            }
        }
    }

    // ---------------- windows ----------------

    pub fn side_windows(&mut self, ctx: &egui::Context) {
        if self.staff_sheet {
            let mut open = true;
            egui::Window::new("Staff Sheet").open(&mut open).default_pos(pos2(220.0, 160.0)).default_width(900.0).show(ctx, |ui| self.staff_sheet_ui(ui));
            self.staff_sheet = open;
        }
        if self.keyboard_win {
            let mut open = true;
            egui::Window::new("Keyboard").open(&mut open).default_pos(pos2(260.0, 640.0)).resizable(false).show(ctx, |ui| self.keyboard_ui(ui));
            self.keyboard_win = open;
        }
        if self.tempo_win {
            let mut open = true;
            egui::Window::new("Tempo").open(&mut open).default_pos(pos2(900.0, 640.0)).resizable(false).show(ctx, |ui| {
                let m = self.cursor.1;
                let mut t = self.score.tempo_at(m);
                ui.add(egui::Slider::new(&mut t, 20..=300).text("beats per minute"));
                if t != self.score.tempo_at(m) {
                    // change the tempo mark governing this measure
                    let idx = (0..=m).rev().find(|&i| self.score.measures[i].tempo.is_some()).unwrap_or(0);
                    self.score.measures[idx].tempo = Some(t);
                    self.dirty = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Play Play").clicked() {
                        self.pending_cmd = Some(Cmd::Play);
                    }
                    if ui.button("Stop Stop").clicked() {
                        self.pending_cmd = Some(Cmd::Stop);
                    }
                    ui.add(egui::Slider::new(&mut self.score.playback.master_volume, 0.0..=1.0).text("Volume"));
                });
            });
            self.tempo_win = open;
        }
    }

    fn staff_sheet_ui(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let n = self.score.staves.len();
        egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
            egui::Grid::new("sheet").striped(true).show(ui, |ui| {
                for h in ["#", "Name", "Abbrev.", "Clef", "Instrument", "Port", "Ch", "Vol", "Pan", "Transp.", "Hide", "Mute", "Solo", "Brace", "Bracket", "Bar thru", "Lines", "Drum"] {
                    ui.strong(h);
                }
                ui.end_row();
                for i in 0..n {
                    let st = &mut self.score.staves[i];
                    ui.label(format!("{}", i + 1));
                    changed |= ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(110.0)).changed();
                    changed |= ui.add(egui::TextEdit::singleline(&mut st.abbrev).desired_width(50.0)).changed();
                    let mut clef = st.clef_at(0);
                    egui::ComboBox::from_id_salt(("clef", i)).selected_text(clef.name()).width(90.0).show_ui(ui, |ui| {
                        for c in Clef::ALL {
                            changed |= ui.selectable_value(&mut clef, c, c.name()).changed();
                        }
                    });
                    if clef != st.clef_at(0) {
                        st.set_clef(0, clef);
                    }
                    egui::ComboBox::from_id_salt(("prog", i)).selected_text(GM_NAMES[st.program as usize]).width(160.0).show_ui(ui, |ui| {
                        for (k, nm) in GM_NAMES.iter().enumerate() {
                            changed |= ui.selectable_value(&mut st.program, k as u8, format!("{} {}", k + 1, nm)).changed();
                        }
                    });
                    egui::ComboBox::from_id_salt(("port", i)).selected_text(if st.port == 0 { "A" } else { "B" }).width(36.0).show_ui(ui, |ui| {
                        changed |= ui.selectable_value(&mut st.port, 0, "A").changed();
                        changed |= ui.selectable_value(&mut st.port, 1, "B").changed();
                    });
                    let mut ch = st.channel + 1;
                    changed |= ui.add(egui::DragValue::new(&mut ch).range(1..=16)).changed();
                    st.channel = ch - 1;
                    changed |= ui.add(egui::DragValue::new(&mut st.volume).range(0..=127)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut st.pan).range(-64..=63)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut st.transpose).range(-36..=36)).changed();
                    changed |= ui.checkbox(&mut st.hidden, "").changed();
                    changed |= ui.checkbox(&mut st.muted, "").changed();
                    changed |= ui.checkbox(&mut st.solo, "").changed();
                    changed |= ui.checkbox(&mut st.brace_with_next, "").changed();
                    changed |= ui.checkbox(&mut st.bracket_with_next, "").changed();
                    changed |= ui.checkbox(&mut st.barline_to_next, "").changed();
                    changed |= ui.add(egui::DragValue::new(&mut st.lines).range(1..=6)).changed();
                    changed |= ui.checkbox(&mut st.drum, "").changed();
                    ui.end_row();
                }
            });
        });
        if self.score.staves.iter().all(|s| s.hidden) {
            self.score.staves[0].hidden = false;
        }
        ui.horizontal(|ui| {
            if ui.button("Add Staff...").clicked() {
                self.pending_cmd = Some(Cmd::AddStaff);
            }
            if ui.button("Delete Cursor Staff").clicked() {
                self.pending_cmd = Some(Cmd::DeleteStaff);
            }
            if ui.button("Move Cursor Staff Up").clicked() && self.cursor.0 > 0 {
                let s = self.cursor.0;
                self.checkpoint();
                self.score.staves.swap(s, s - 1);
                self.score.events.swap(s, s - 1);
                self.cursor.0 -= 1;
            }
        });
        if changed {
            self.dirty = true;
        }
    }

    fn keyboard_ui(&mut self, ui: &mut Ui) {
        let low = 36; // C2
        let octaves = 5;
        let ww = 18.0;
        let (r, resp) = ui.allocate_exact_size(vec2(ww * 7.0 * octaves as f32 + ww, 80.0), Sense::click());
        let p = ui.painter_at(r);
        let mut whites = vec![];
        let mut blacks = vec![];
        let mut x = r.left();
        for m in low..=(low + octaves * 12) {
            let pc = m % 12;
            if [1, 3, 6, 8, 10].contains(&pc) {
                blacks.push((m, Rect::from_min_size(pos2(x - ww * 0.3, r.top()), vec2(ww * 0.6, 50.0))));
            } else {
                whites.push((m, Rect::from_min_size(pos2(x, r.top()), vec2(ww, 80.0))));
                x += ww;
            }
        }
        for (m, kr) in &whites {
            p.rect_filled(*kr, 0.0, Color32::WHITE);
            p.rect_stroke(*kr, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
            if m % 12 == 0 {
                p.text(kr.center_bottom() - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("C{}", m / 12 - 1), FontId::proportional(9.0), Color32::GRAY);
            }
        }
        for (_, kr) in &blacks {
            p.rect_filled(*kr, 1.0, Color32::BLACK);
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let hit = blacks.iter().find(|(_, kr)| kr.contains(pos)).or_else(|| whites.iter().find(|(_, kr)| kr.contains(pos)));
                if let Some((m, _)) = hit {
                    let flats = self.score.measures[self.cursor.1].key < 0;
                    let pitch = Pitch::from_midi(*m, flats);
                    if self.recording || ui.input(|i| !i.modifiers.alt) {
                        let (s, mm, t) = self.cursor;
                        let v = self.cur_value;
                        let d = self.cur_dots;
                        self.enter(s, mm, t, Some(pitch), v, d, true);
                    } else {
                        let s = self.cursor.0;
                        self.sound_note(s, *m, 90);
                    }
                    self.status = format!("{} entered at the cursor (Alt+click only listens).", pitch.name());
                }
            }
        }
        ui.label("Click a key to enter that note at the red cursor with the current note value.");
    }

    // ---------------- dialogs ----------------

    pub fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let mut keep = true;
        let mut ok = false;
        let title = dialog_title(&d);
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                self.dialog_body(ui, &mut d);
                ui.separator();
                ui.horizontal(|ui| match &d {
                    Dialog::Message(_) | Dialog::About | Dialog::Shortcuts | Dialog::ClavinovaGuide => {
                        if ui.button("OK").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::ConfirmClose(_) => {
                        if ui.button("Save").clicked() {
                            ok = true;
                        }
                        if ui.button("Don't Save").clicked() {
                            self.dirty = false;
                            ok = true;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::NewScore { .. } => {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui.button("Create, then open Text Elements").clicked() {
                            ok = true;
                            self.after_new_wizard = true;
                        }
                        if ui.button("Create").clicked() {
                            ok = true;
                        }
                    }
                    _ => {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ok = true;
                        }
                    }
                });
            });
        if ok {
            keep = false;
            self.apply_dialog(d.clone());
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dialog = None;
        }
    }

    fn dialog_body(&mut self, ui: &mut Ui, d: &mut Dialog) {
        let nm = self.score.measures.len();
        match d {
            Dialog::NewScore { staves, systems, mps, c_score, handwritten, template } => {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.add(egui::DragValue::new(staves).range(1..=32).prefix("Staves per System: "));
                        ui.add(egui::DragValue::new(systems).range(1..=12).prefix("Systems per Page: "));
                        ui.add(egui::DragValue::new(mps).range(1..=16).prefix("Measures per System: "));
                        ui.checkbox(c_score, "Make a \"C\" Score");
                        ui.checkbox(handwritten, "Use Handwritten Style");
                    });
                    ui.vertical(|ui| {
                        ui.strong("Starting Template");
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for (k, t) in TEMPLATES.iter().enumerate() {
                                ui.selectable_value(template, k, *t);
                            }
                        });
                    });
                });
            }
            Dialog::TimeSig { num, den, from, to } => {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(num).range(1..=32));
                    ui.label("/");
                    egui::ComboBox::from_id_salt("den").selected_text(den.to_string()).show_ui(ui, |ui| {
                        for v in [1u8, 2, 4, 8, 16, 32] {
                            ui.selectable_value(den, v, v.to_string());
                        }
                    });
                });
                ui.horizontal(|ui| {
                    for (a, b) in [(2, 4), (3, 4), (4, 4), (6, 8), (9, 8), (12, 8), (2, 2), (5, 4)] {
                        if ui.button(format!("{a}/{b}")).clicked() {
                            *num = a;
                            *den = b;
                        }
                    }
                });
                ui.add(egui::DragValue::new(from).range(1..=nm).prefix("From measure "));
                ui.add(egui::DragValue::new(to).range(1..=nm).prefix("Through measure "));
            }
            Dialog::KeySig { key, from, to, transpose } => {
                egui::ComboBox::from_id_salt("key").selected_text(key_name(*key)).width(200.0).show_ui(ui, |ui| {
                    for k in -7..=7i8 {
                        ui.selectable_value(key, k, key_name(k));
                    }
                });
                ui.add(egui::DragValue::new(from).range(1..=nm).prefix("From measure "));
                ui.add(egui::DragValue::new(to).range(1..=nm).prefix("Through measure "));
                ui.checkbox(transpose, "Transpose the notes to the new key");
            }
            Dialog::Tempo { bpm, measure, to_measure, mode, to_bpm } => {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(measure).range(0..=nm - 1).custom_formatter(|v, _| format!("From measure {}", v as usize + 1)));
                    ui.add(egui::DragValue::new(to_measure).range(0..=nm - 1).custom_formatter(|v, _| format!("to {}", v as usize + 1)));
                });
                ui.radio_value(mode, 0, "Set all Tempos to");
                ui.radio_value(mode, 1, "Change to (% of current values)");
                ui.radio_value(mode, 2, "Change smoothly from ... to");
                ui.radio_value(mode, 3, "Add (to all values)");
                ui.add(egui::DragValue::new(bpm).range(1..=400));
                if *mode == 2 {
                    ui.add(egui::DragValue::new(to_bpm).range(1..=400).prefix("to "));
                }
            }
            Dialog::Barline { kind, times, .. } => {
                for b in crate::model::Barline::ALL {
                    ui.radio_value(kind, b, b.name());
                }
                ui.add(egui::DragValue::new(times).range(2..=8).prefix("Play repeat times: "));
            }
            Dialog::Endings { text, .. } => {
                ui.label("Ending text (e.g. 1.  2.  1, 2.) - leave empty to remove:");
                ui.text_edit_singleline(text);
            }
            Dialog::Coda { text, rehearsal, .. } => {
                ui.horizontal_wrapped(|ui| {
                    for t in ["D.C.", "D.C. al Fine", "D.C. al Coda", "D.S.", "D.S. al Fine", "D.S. al Coda", "Fine", "To Coda"] {
                        if ui.button(t).clicked() {
                            *text = t.into();
                        }
                    }
                });
                ui.text_edit_singleline(text);
                ui.label("Rehearsal letter:");
                ui.text_edit_singleline(rehearsal);
            }
            Dialog::MeasureNumbers { mode } => {
                ui.radio_value(mode, MeasureNumbers::None, "None");
                ui.radio_value(mode, MeasureNumbers::EverySystem, "At the start of each system");
                ui.radio_value(mode, MeasureNumbers::EveryMeasure, "Every measure");
                ui.radio_value(mode, MeasureNumbers::Every5, "Every 5 measures");
            }
            Dialog::AddMeasures { count, before } => {
                ui.add(egui::DragValue::new(count).range(1..=500).prefix("Add "));
                ui.radio_value(before, true, "Before the cursor measure");
                ui.radio_value(before, false, "After the cursor measure");
            }
            Dialog::MeasuresPerSystem { n } => {
                ui.add(egui::DragValue::new(n).range(1..=32).prefix("Measures per system: "));
            }
            Dialog::SystemsPerPage { n } => {
                ui.add(egui::DragValue::new(n).range(1..=16).prefix("Systems per page: "));
            }
            Dialog::Text(t) => {
                egui::Grid::new("txt").show(ui, |ui| {
                    for (l, v) in [
                        ("Title", &mut t.title),
                        ("Subtitle", &mut t.subtitle),
                        ("Composer", &mut t.composer),
                        ("Lyricist / Arranger", &mut t.lyricist),
                        ("Instructions", &mut t.instructions),
                        ("Copyright", &mut t.copyright),
                        ("Header", &mut t.header),
                        ("Footer", &mut t.footer),
                    ] {
                        ui.label(l);
                        ui.add(egui::TextEdit::singleline(v).desired_width(300.0));
                        ui.end_row();
                    }
                });
                ui.checkbox(&mut t.show_page_numbers, "Page numbers");
            }
            Dialog::NoteAttributes { velocity, play_len, value, dots, voice, hidden, stem } => {
                ui.add(egui::Slider::new(velocity, 1..=127).text("Velocity"));
                ui.add(egui::DragValue::new(play_len).range(1..=TPQ * 16).prefix("Playback length (ticks, 480 = quarter): "));
                ui.add(egui::Slider::new(value, 0..=8).text("Note value (0 = double whole ... 8 = 128th)"));
                ui.add(egui::Slider::new(dots, 0..=2).text("Dots"));
                ui.add(egui::Slider::new(voice, 1..=8).text("Voice"));
                ui.horizontal(|ui| {
                    ui.radio_value(stem, Stem::Auto, "Stem normal");
                    ui.radio_value(stem, Stem::Up, "Up");
                    ui.radio_value(stem, Stem::Down, "Down");
                });
                ui.checkbox(hidden, "Hidden (plays but doesn't print)");
            }
            Dialog::Velocity { mode, value, to } => {
                ui.radio_value(mode, 0, "Set all values to");
                ui.radio_value(mode, 2, "Change to (% of current values)");
                ui.radio_value(mode, 3, "Change smoothly from ... to");
                ui.radio_value(mode, 1, "Add (to all values)");
                ui.add(egui::DragValue::new(value).range(-127..=400));
                if *mode == 3 {
                    ui.add(egui::DragValue::new(to).range(1..=127).prefix("to "));
                }
            }
            Dialog::SpacingDefaults { stem, beam, tie_h, tie_v, ms } => {
                ui.label("NOTE: \"half space\" means one half the height of a staff space");
                ui.add(egui::DragValue::new(stem).range(1..=16).prefix("Beam stem length (half spaces): "));
                ui.add(egui::DragValue::new(beam).range(25..=400).prefix("Beam thickness (% of half space): "));
                ui.add(egui::DragValue::new(tie_h).range(0..=8).prefix("Horizontal tie offset (quarter spaces): "));
                ui.add(egui::DragValue::new(tie_v).range(0..=8).prefix("Vertical tie offset (quarter spaces): "));
                ui.horizontal(|ui| {
                    ui.label("Measure spacing (half spaces):");
                    for v in ms.iter_mut() {
                        ui.add(egui::DragValue::new(v).range(0..=8));
                    }
                });
            }
            Dialog::Duration { percent } => {
                ui.add(egui::Slider::new(percent, 10..=200).text("% of written length"));
            }
            Dialog::Transpose { semitones, octave } => {
                ui.add(egui::Slider::new(semitones, -11..=11).text("Semitones"));
                ui.add(egui::Slider::new(octave, -3..=3).text("Octaves"));
                ui.label("Applies to the selected notes, or the selected measures.");
            }
            Dialog::Swing { swing, note } => {
                ui.add(egui::Slider::new(swing, 50..=75).text("% (50.0 to 75.0)"));
                ui.label("Note Type to Swing:");
                ui.horizontal(|ui| {
                    for (v, n) in [(1u8, "Whole"), (2, "Half"), (3, "Quarter"), (4, "Eighth"), (5, "16th"), (6, "32nd")] {
                        ui.radio_value(note, v, n);
                    }
                });
            }
            Dialog::Tuplet { notes, in_time } => {
                ui.label("Notes entered with the tuplet tool play this many notes in the time of the other number.");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(notes).range(2..=15));
                    ui.label("notes in the time of");
                    ui.add(egui::DragValue::new(in_time).range(1..=15));
                });
                ui.horizontal_wrapped(|ui| {
                    for (a, b) in [(3u8, 2u8), (2, 3), (5, 4), (6, 4), (7, 4), (7, 8), (9, 8)] {
                        if ui.selectable_label(*notes == a && *in_time == b, format!("{a}:{b}")).clicked() {
                            *notes = a;
                            *in_time = b;
                        }
                    }
                });
            }
            Dialog::ClickSetup(p) => {
                // the original's Click Settings: a row for the bar click and one for the beat click
                egui::Grid::new("clickgrid").num_columns(6).spacing([8.0, 6.0]).show(ui, |ui| {
                    for h in ["", "Port", "Chan", "Pitch", "Vel", "Dur (msec)", "Sound"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (label, c) in [("Bar click", &mut p.click_bar), ("Beat click", &mut p.click_beat)] {
                        ui.label(label);
                        egui::ComboBox::from_id_salt(("clickport", label)).width(90.0).selected_text(match c.port { 0 => "A", 1 => "B", _ => "Computer" }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut c.port, 0, "A");
                            ui.selectable_value(&mut c.port, 1, "B");
                            ui.selectable_value(&mut c.port, 2, "Computer");
                        });
                        let on_port = c.port < 2;
                        ui.add_enabled(on_port, egui::DragValue::new(&mut c.chan).range(1..=16));
                        ui.add_enabled(on_port, egui::DragValue::new(&mut c.pitch).range(0..=127).custom_formatter(|v, _| format!("{} ({})", v as i32, Pitch::from_midi(v as i32, false).name())));
                        ui.add(egui::DragValue::new(&mut c.vel).range(1..=127));
                        ui.add_enabled(on_port, egui::DragValue::new(&mut c.dur_ms).range(10..=1000));
                        ui.add_enabled_ui(on_port && c.chan != 10, |ui| {
                            egui::ComboBox::from_id_salt(("clickprog", label)).width(150.0).selected_text(GM_NAMES[c.program.min(127) as usize]).show_ui(ui, |ui| {
                                for (k, n) in GM_NAMES.iter().enumerate() {
                                    ui.selectable_value(&mut c.program, k as u8, *n);
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
                ui.checkbox(&mut p.click_only_rec, "Only Use Click When Recording");
                ui.checkbox(&mut p.click_on, "Click on (toolbar metronome button)");
                ui.horizontal(|ui| {
                    if ui.button("Test click").on_hover_text("Plays one bar of clicks with these settings").clicked() {
                        let draft = p.clone();
                        self.test_click(&draft);
                    }
                    if ui.button("Defaults: channel 1, piano voice").clicked() {
                        p.click_bar = crate::prefs::bar_click_ch1();
                        p.click_beat = crate::prefs::beat_click_ch1();
                    }
                    if ui.button("Defaults: channel 10, drum metronome").clicked() {
                        p.click_bar = crate::prefs::bar_click();
                        p.click_beat = crate::prefs::beat_click();
                    }
                });
                ui.add_space(4.0);
                ui.label("Port A = the piano, together with the music (when the piano isn't connected the click plays on the computer). Computer = always on the Mac. Channel 10 uses the piano's own drum kit (34 = metronome bell, 33 = metronome click on Yamaha). Other channels use the Sound chosen here - pick a channel the piano's MIDI Receive setting plays. Use Test click to try.");
            }
            Dialog::ClavinovaGuide => {
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    let (out_ok, in_ok) = (
                        self.midi_out.as_ref().map(|m| m.connected[0].clone()).flatten(),
                        self.midi.as_ref().map(|m| m.names.join(", ")).filter(|s| !s.is_empty()),
                    );
                    ui.label(egui::RichText::new("Situação agora").strong());
                    ui.label(format!("* Teclado (entrada): {}", in_ok.unwrap_or_else(|| "nenhum encontrado".into())));
                    ui.label(format!("* Som da partitura (saída, Port A): {}", out_ok.unwrap_or_else(|| "som do computador".into())));
                    ui.separator();
                    for (n, t) in [
                        ("1. Ligar o cabo", "Ligue o cabo USB da entrada USB TO HOST do Clavinova (atrás do piano) a uma porta USB do Mac. Ligue o piano."),
                        ("2. Abrir o programa", "Abra o Fermata. Em poucos segundos aparece na barra de cima \"MIDI: Clavinova Port1\" (em verde). Se não aparecer, desligue e ligue o piano, ou use Setup > MIDI Setup > \"Look for devices again\"."),
                        ("3. Escolher onde o som sai", "Abra Setup > MIDI Setup. Em MIDI Out, no campo Port A, escolha \"Clavinova Port1\". Deixe Port B em \"None (built-in sound)\". Transmit Sync pode ficar desmarcado."),
                        ("4. Escolher de onde vêm as notas", "Em MIDI In > Record Port, escolha \"Clavinova Port1\" (ou deixe \"All MIDI inputs\"). Receive Sync Port: \"None\"."),
                        ("5. MIDI Thru", "Marque \"Follow Current Staff\". Se, ao tocar no piano, cada nota soar em dobro, desmarque \"Thru on\" (ou clique no botão \"Thru A1\" da barra para ele mostrar \"Thru Off\"): o piano já toca o próprio som."),
                        ("6. Confirmar", "Clique em OK. O programa guarda essa escolha e usa o Clavinova sozinho nas próximas vezes que for aberto."),
                        ("7. Canais das pautas", "Em Windows > Staff Sheet, cada pauta tem Port (A) e Ch (canal 1 a 16). Deixe Port = A em todas. Pautas com canais diferentes podem tocar instrumentos diferentes no piano (a coluna Instrument)."),
                        ("8. Testar", "Escreva ou abra uma partitura e aperte a barra de espaço. O som deve sair no Clavinova, e o Rx Monitor do piano deve piscar. Espaço de novo para parar."),
                        ("No piano", "Não é preciso mudar nada: com o template MIDI Song ativo, os canais 1-16 da MIDI/USB 1 já recebem para a parte SONG."),
                        ("Se ficar alguma nota presa", "Setup > MIDI Setup > \"All Notes Off\" silencia tudo."),
                        ("Gravar tocando", "Com a partitura aberta, aperte Enter: conta um compasso e grava o que você tocar. Enter ou espaço para parar."),
                    ] {
                        ui.label(egui::RichText::new(n).strong());
                        ui.label(t);
                        ui.add_space(4.0);
                    }
                    if ui.button("Abrir MIDI Setup").clicked() {
                        self.pending_cmd = Some(Cmd::MidiSetup);
                    }
                });
            }
            Dialog::MidiSetup(p) => {
                // Same layout as the original's MIDI Setup: MIDI Out, MIDI In, MIDI Thru, Sync Source.
                let outs: Vec<String> = self.midi_out.as_ref().map(|m| m.available.clone()).unwrap_or_default();
                let ins: Vec<String> = self.midi.as_ref().map(|m| m.available.clone()).unwrap_or_default();
                let none_out = "None (built-in sound)";
                ui.strong("MIDI Out");
                egui::Grid::new("midi_out").num_columns(3).show(ui, |ui| {
                    for (k, label) in ["Port A:", "Port B:"].iter().enumerate() {
                        ui.label(*label);
                        let cur = if k == 0 { &mut p.out_port_a } else { &mut p.out_port_b };
                        egui::ComboBox::from_id_salt(("outp", k)).width(260.0).selected_text(cur.clone().unwrap_or_else(|| none_out.into())).show_ui(ui, |ui| {
                            ui.selectable_value(cur, None, none_out);
                            for n in &outs {
                                ui.selectable_value(cur, Some(n.clone()), n);
                            }
                        });
                        ui.checkbox(&mut p.transmit_sync[k], "Transmit Sync");
                        ui.end_row();
                    }
                });
                ui.checkbox(&mut p.single_channel, "Send everything on channel 1, without changing the piano's voice")
                    .on_hover_text("For pianos that only play channel 1, like a Clavinova in MIDI Accord mode (RECEIVE: CH1 = KEYBOARD, other channels OFF). The click plays as short high notes in the piano's own sound.");
                ui.add_space(4.0);
                ui.strong("MIDI In");
                egui::Grid::new("midi_in").num_columns(2).show(ui, |ui| {
                    ui.label("Record Port:");
                    egui::ComboBox::from_id_salt("recp").width(260.0).selected_text(p.record_port.clone().unwrap_or_else(|| "All MIDI inputs".into())).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.record_port, None, "All MIDI inputs");
                        for n in &ins {
                            ui.selectable_value(&mut p.record_port, Some(n.clone()), n);
                        }
                    });
                    ui.end_row();
                    ui.label("Receive Sync Port:");
                    egui::ComboBox::from_id_salt("syncp").width(260.0).selected_text(p.receive_sync_port.clone().unwrap_or_else(|| "None".into())).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.receive_sync_port, None, "None");
                        for n in &ins {
                            ui.selectable_value(&mut p.receive_sync_port, Some(n.clone()), n);
                        }
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.strong("MIDI Thru");
                ui.checkbox(&mut p.thru_on, "Thru on (what you play on the keyboard sounds through the output)");
                ui.radio_value(&mut p.thru_follow_staff, true, "Follow Current Staff");
                ui.radio_value(&mut p.thru_follow_staff, false, "Always send out:");
                ui.add_enabled_ui(!p.thru_follow_staff, |ui| {
                    for (port, name) in [(0u8, "A"), (1u8, "B")] {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            for ch in 0..16u8 {
                                let sel = p.thru_port == port && p.thru_channel == ch;
                                if ui.selectable_label(sel, format!("{}", ch + 1)).clicked() {
                                    p.thru_port = port;
                                    p.thru_channel = ch;
                                }
                            }
                        });
                    }
                });
                ui.add_space(4.0);
                ui.strong("Sync Source");
                ui.radio_value(&mut p.sync_external, false, "Internal");
                ui.add_enabled(false, egui::RadioButton::new(p.sync_external, "External (following another device's clock isn't available yet)"));
                ui.separator();
                ui.collapsing("Recording", |ui| {
                    egui::ComboBox::from_label("Smallest note written").selected_text(match p.rec_quantize { 4 => "Eighth", 5 => "16th", 6 => "32nd", _ => "Quarter" }).show_ui(ui, |ui| {
                        for (v, n) in [(3u8, "Quarter"), (4, "Eighth"), (5, "16th"), (6, "32nd")] {
                            ui.selectable_value(&mut p.rec_quantize, v, n);
                        }
                    });
                    ui.add(egui::Slider::new(&mut p.rec_split, 36..=84).text("Split point (grand staff)").custom_formatter(|v, _| Pitch::from_midi(v as i32, false).name()));
                    ui.add(egui::Slider::new(&mut p.rec_count_in, 0..=2).text("Count-in bars"));
                    ui.checkbox(&mut p.rec_click, "Click while recording");
                });
                ui.horizontal(|ui| {
                    if ui.button("Look for devices again").clicked() {
                        if let Some(m) = self.midi.as_mut() {
                            m.disconnect_all();
                            m.last_scan = None;
                        }
                        if let Some(m) = self.midi_out.as_mut() {
                            m.last_scan = None;
                        }
                    }
                    if ui.button("All Notes Off").on_hover_text("Silence anything still sounding on the MIDI outputs").clicked() {
                        if let Some(m) = self.midi_out.as_mut() {
                            m.all_notes_off();
                        }
                    }
                });
                if let Some(e) = self.midi_out.as_ref().and_then(|m| m.error.clone()).or_else(|| self.midi.as_ref().and_then(|m| m.error.clone())) {
                    ui.colored_label(Color32::from_rgb(180, 60, 0), e);
                }
                ui.separator();
                ui.strong("Check the connection");
                if let Some(m) = self.midi_out.as_ref() {
                    for k in 0..2usize {
                        let name = if k == 0 { "A" } else { "B" };
                        let sent = m.sent[k].load(std::sync::atomic::Ordering::Relaxed);
                        match (&m.chosen[k], &m.connected[k]) {
                            (None, _) => {
                                ui.label(format!("Port {name}: built-in sound"));
                            }
                            (Some(c), None) => {
                                ui.colored_label(Color32::from_rgb(190, 60, 0), format!("Port {name}: \"{c}\" not found. Is the piano on and the USB cable in its USB TO HOST port?"));
                            }
                            (Some(_), Some(n)) => {
                                ui.colored_label(Color32::from_rgb(20, 130, 40), format!("Port {name}: connected to \"{n}\" - {sent} messages sent"));
                            }
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Test Port A").on_hover_text("Plays C-E-G-C on channel 1 straight to the piano (press OK first if you just changed Port A)").clicked() {
                        let ok = self.midi_out.as_ref().map(|m| m.test(0)).unwrap_or(false);
                        self.status = if ok { "Test notes sent to Port A. The piano should play four notes and its Rx Monitor should blink.".into() } else { "Port A isn't connected yet: choose the piano in Port A and press OK, then test again.".into() };
                    }
                    if ui.button("Copy diagnostics").on_hover_text("Copies what the program sees, to paste when asking for help").clicked() {
                        let mut t = self.midi_out.as_ref().map(|m| m.diagnostics()).unwrap_or_default();
                        if let Some(mi) = self.midi.as_ref() {
                            t.push_str(&format!("MIDI inputs found: {:?}\nInputs open: {:?}\nNotes received: {}\nRecording: {}\n", mi.available, mi.names, mi.received.load(std::sync::atomic::Ordering::Relaxed), self.rec.is_some()));
                        }
                        t.push_str(&format!("Cursor staff port/channel: {:?}\n", self.score.staves.get(self.cursor.0).map(|s| (s.port, s.channel + 1))));
                        ui.ctx().copy_text(t);
                        self.status = "Diagnostics copied - paste them in the chat.".into();
                    }
                });
                ui.label("If the piano sounds twice when you play it, turn Thru off here.");
            }
            Dialog::MidiPlayback => {
                let pb = &mut self.score.playback;
                ui.add(egui::Slider::new(&mut pb.default_tempo, 20..=300).text("Default tempo"));
                ui.add(egui::Slider::new(&mut pb.swing, 50..=75).text("Swing"));
                ui.checkbox(&mut pb.click, "Click (metronome)");
                ui.checkbox(&mut pb.follow, "Follow playback");
                ui.add(egui::Slider::new(&mut pb.master_volume, 0.0..=1.0).text("Volume"));
                ui.checkbox(&mut pb.humanize, "Humanize (random velocity within each dynamic's range)");
                ui.collapsing("Velocity Ranges", |ui| {
                    egui::Grid::new("dynr").show(ui, |ui| {
                        for (i, d) in DYNAMICS.iter().enumerate() {
                            if let Some(r) = pb.dyn_ranges.get_mut(i) {
                                ui.label(*d);
                                ui.add(egui::DragValue::new(&mut r.0).range(1..=127));
                                ui.label("to");
                                ui.add(egui::DragValue::new(&mut r.1).range(1..=127));
                                ui.end_row();
                            }
                        }
                    });
                });
                ui.label("Instruments, channels, volume and pan per staff are in Windows > Staff Sheet.");
            }
            Dialog::JumpTo { measure } => {
                ui.add(egui::DragValue::new(measure).range(1..=nm).prefix("Measure "));
            }
            Dialog::PageSetup { w, h, margin, staff_space, staff_gap, system_gap } => {
                ui.horizontal(|ui| {
                    if ui.button("Letter").clicked() {
                        *w = 612.0;
                        *h = 792.0;
                    }
                    if ui.button("A4").clicked() {
                        *w = 595.0;
                        *h = 842.0;
                    }
                    if ui.button("Landscape").clicked() {
                        std::mem::swap(w, h);
                    }
                });
                ui.add(egui::DragValue::new(w).range(200.0..=2000.0).prefix("Width (pt): "));
                ui.add(egui::DragValue::new(h).range(200.0..=2000.0).prefix("Height (pt): "));
                ui.add(egui::DragValue::new(margin).range(10.0..=150.0).prefix("Margin (pt): "));
                ui.add(egui::Slider::new(staff_space, 3.0..=12.0).text("Staff size"));
                ui.add(egui::Slider::new(staff_gap, 4.0..=20.0).text("Space between staves"));
                ui.add(egui::Slider::new(system_gap, 4.0..=24.0).text("Space between systems"));
            }
            Dialog::AddStaff { name, clef, program } => {
                ui.text_edit_singleline(name);
                egui::ComboBox::from_id_salt("asclef").selected_text(clef.name()).show_ui(ui, |ui| {
                    for c in Clef::ALL {
                        ui.selectable_value(clef, c, c.name());
                    }
                });
                egui::ComboBox::from_id_salt("asprog").selected_text(GM_NAMES[*program as usize]).show_ui(ui, |ui| {
                    for (k, n) in GM_NAMES.iter().enumerate() {
                        ui.selectable_value(program, k as u8, *n);
                    }
                });
                ui.label("The staff is added below the cursor staff.");
            }
            Dialog::SplitStaff { split } => {
                ui.add(egui::Slider::new(split, 36..=84).text("Split point").custom_formatter(|v, _| Pitch::from_midi(v as i32, false).name()));
                ui.label("Notes at or above the split stay; lower notes move to a new bass staff.");
            }
            Dialog::ConnectStaves { kind } => {
                ui.label("Connect the cursor staff with the staff below it:");
                ui.radio_value(kind, 0, "Brace (piano style)");
                ui.radio_value(kind, 1, "Bracket");
                ui.radio_value(kind, 2, "Barlines only");
                ui.radio_value(kind, 3, "Disconnect");
            }
            Dialog::ChordSymbol { root, kind, bass, custom, .. } => {
                // the original's Choose Chord window: chord types by family, then the root and an optional bass
                // keys, as in the original (when the Custom box isn't being typed in): a-g root, A-G bass, 3/4 sharp/flat
                // root, #/$ sharp/flat bass, M major, m minor, + or = aug, o dim, 7, & maj7, 9, 1 = 11, 5, 6
                if ui.memory(|m| m.focused().is_none()) {
                    let typed: Vec<String> = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect());
                    for t in typed {
                        for ch in t.chars() {
                            let minor = kind.starts_with('m') && !kind.starts_with("maj");
                            let letter = |s: &str| s.chars().next().map(|c| c.to_string()).unwrap_or_default();
                            match ch {
                                'a'..='g' => *root = ch.to_ascii_uppercase().to_string(),
                                'A'..='G' => *bass = ch.to_string(),
                                '3' => *root = format!("{}#", letter(root)),
                                '4' => *root = format!("{}b", letter(root)),
                                '#' if !bass.is_empty() => *bass = format!("{}#", letter(bass)),
                                '$' if !bass.is_empty() => *bass = format!("{}b", letter(bass)),
                                'M' => *kind = String::new(),
                                'm' => *kind = "m".into(),
                                '+' | '=' => *kind = "+".into(),
                                'o' => *kind = "dim".into(),
                                '&' => *kind = "maj7".into(),
                                '7' => *kind = if minor { "m7".into() } else if kind == "dim" { "dim7".into() } else if kind == "+" { "+7".into() } else { "7".into() },
                                '9' => *kind = if minor { "m9".into() } else { "9".into() },
                                '1' => *kind = if minor { "m11".into() } else { "11".into() },
                                '5' => *kind = "5".into(),
                                '6' => *kind = if minor { "m6".into() } else { "6".into() },
                                _ => continue,
                            }
                            custom.clear();
                        }
                    }
                }
                let cell = vec2(78.0, 20.0);
                ui.label(egui::RichText::new("Name").strong());
                for (family, rows) in CHORD_GRID.iter() {
                    egui::Grid::new(("chordgrid", *family)).spacing([3.0, 3.0]).min_col_width(cell.x).show(ui, |ui| {
                        for (ri, row) in rows.iter().enumerate() {
                            ui.label(if ri == 0 { *family } else { "" });
                            for k in row.iter() {
                                match k {
                                    Some(k) => {
                                        let label = if k.is_empty() { "Major" } else { k };
                                        let on = custom.is_empty() && kind.as_str() == *k;
                                        if ui.add_sized(cell, egui::Button::selectable(on, label)).clicked() {
                                            *kind = k.to_string();
                                            custom.clear();
                                        }
                                    }
                                    None => {
                                        ui.allocate_exact_size(cell, Sense::hover());
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(egui::TextEdit::singleline(custom).desired_width(200.0).hint_text("any text"));
                });
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    for (title, val, none) in [("Root", &mut *root, false), ("Bass", &mut *bass, true)] {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(title).strong());
                            for acc in ["#", "", "b"] {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    for l in ["C", "D", "E", "F", "G", "A", "B"] {
                                        let name = format!("{l}{acc}");
                                        let on = *val == name;
                                        if ui.add_sized(vec2(30.0, 20.0), egui::Button::selectable(on, &name)).clicked() {
                                            // clicking the chosen bass again removes it
                                            *val = if none && on { String::new() } else { name };
                                        }
                                    }
                                });
                            }
                            if none {
                                if ui.add(egui::Button::selectable(val.is_empty(), "No bass note")).clicked() {
                                    val.clear();
                                }
                            }
                        });
                        ui.add_space(24.0);
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Keys: a-g root, A-G bass, 3/4 sharp/flat root, #/$ sharp/flat bass, M major, m minor, + aug, o dim, 7, & maj7, 9, 1 = 11, 5, 6").small().weak());
                let preview = chord_text(root, kind, bass, custom);
                ui.label(egui::RichText::new(format!("Chord: {}", if preview.is_empty() { "(none)".into() } else { preview })).size(16.0).strong());
            }
            Dialog::TextGraphic { text, size, bold, .. } => {
                ui.text_edit_singleline(text).request_focus();
                ui.add(egui::Slider::new(size, 0.8..=5.0).text("Size"));
                ui.checkbox(bold, "Bold");
            }
            Dialog::Lyric { text, line, target } => {
                ui.label("Space = next note, - = hyphen and next note, Shift+- = melisma (_), Shift+Space = to the tied note,");
                ui.label("Tab / Shift+Tab = next / previous syllable, Ctrl+Space = space inside the lyric, Ctrl+- = hyphen inside a word.");
                ui.add(egui::Slider::new(line, 0..=7).text("Verse line").custom_formatter(|v, _| format!("{}", v as usize + 1)));
                // the original's lyric keys, taken before the text box sees them
                #[derive(PartialEq)]
                enum Act {
                    Next,
                    Prev,
                }
                let mut act = None;
                let mut add = String::new();
                ui.input_mut(|i| {
                    let mut keep = vec![];
                    let evs = std::mem::take(&mut i.events);
                    let mut skip_text: Option<String> = None;
                    for ev in evs {
                        match &ev {
                            egui::Event::Key { key, pressed: true, modifiers, .. } if act.is_none() => {
                                let ctrl = modifiers.ctrl || modifiers.command;
                                match key {
                                    egui::Key::Space if ctrl => {
                                        add.push(' ');
                                        skip_text = Some(" ".into());
                                    }
                                    egui::Key::Space => {
                                        act = Some(Act::Next);
                                        skip_text = Some(" ".into());
                                    }
                                    egui::Key::Minus if ctrl => {
                                        add.push('-');
                                        skip_text = Some("-".into());
                                    }
                                    egui::Key::Minus if modifiers.shift => {
                                        add.push('_');
                                        act = Some(Act::Next);
                                        skip_text = Some("_".into());
                                    }
                                    egui::Key::Minus => {
                                        add.push('-');
                                        act = Some(Act::Next);
                                        skip_text = Some("-".into());
                                    }
                                    egui::Key::Tab => act = Some(if modifiers.shift { Act::Prev } else { Act::Next }),
                                    _ => keep.push(ev),
                                }
                            }
                            egui::Event::Text(t) if skip_text.as_deref() == Some(t.as_str()) => {
                                skip_text = None;
                            }
                            _ => keep.push(ev),
                        }
                    }
                    i.events = keep;
                });
                text.push_str(&add);
                let r = ui.text_edit_singleline(text);
                r.request_focus();
                match act {
                    Some(Act::Next) => {
                        self.set_lyric(*target, *line, text.clone(), true);
                        return;
                    }
                    Some(Act::Prev) => {
                        self.set_lyric(*target, *line, text.clone(), false);
                        if let Some(prev) = self.prev_note(*target) {
                            let cur = self.ev(prev).and_then(|e| e.lyric.get(*line).cloned()).unwrap_or_default();
                            self.selected.clear();
                            self.selected.insert(prev);
                            self.dialog = Some(Dialog::Lyric { target: prev, line: *line, text: cur });
                        }
                        return;
                    }
                    None => {}
                }
            }
            Dialog::Expression { text, .. } => {
                ui.text_edit_singleline(text).request_focus();
            }
            Dialog::Shortcuts => {
                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    egui::Grid::new("sc").striped(true).show(ui, |ui| {
                        let mut cmds = commands::all_shortcut_cmds();
                        cmds.sort_by_key(|c| commands::label(*c));
                        for c in cmds {
                            ui.label(commands::label(c));
                            ui.monospace(shortcut_text(ui.ctx(), c));
                            ui.end_row();
                        }
                        for (a, b) in [
                            ("Note values double whole ... 128th", "0 - 8"),
                            ("Rest / Sharp / Flat / Natural", "R / S / F / N"),
                            ("Double sharp / double flat / courtesy", "Shift+S / Shift+F / Shift+N"),
                            ("Dot / double dot / tuplet", "D / Shift+D / T"),
                            ("Arrow / Pencil toggle, Eraser", "A or P (Esc = Arrow), E"),
                            ("Play / Stop", "Space"),
                            ("Record (step entry)", "Enter"),
                            ("Jump to measure", "M"),
                            ("Zoom in / out", "Z / Shift+Z"),
                            ("QWERTY note entry on/off", "Q"),
                            ("Show voice n / all voices", "V then 1-8 / V then A"),
                            ("Move cursor by measure / staff", "Arrow keys"),
                            ("First / last measure", "Home / End"),
                            ("Select note left/right of cursor", "Ctrl+Left / Ctrl+Right"),
                            ("One less / one more measure on system", "[ / ]"),
                            ("Delete note left of cursor or selection", "Delete"),
                            ("Select note / region corner", "Shift+click"),
                            ("Audition pitch", "Right-click on staff"),
                        ] {
                            ui.label(a);
                            ui.monospace(b);
                            ui.end_row();
                        }
                    });
                });
            }
            Dialog::About => {
                ui.heading("Fermata");
                ui.label("A free music notation editor, rebuilt with the same workflow as a classic desktop notation program.");
                ui.label("Version 0.1 - score layout, palettes, shortcuts, note entry, playback, MIDI and MusicXML.");
            }
            Dialog::Message(m) => {
                ui.label(m.as_str());
            }
            Dialog::ConfirmClose(_) => {
                ui.label("Save changes to this score first?");
            }
        }
    }

    fn apply_dialog(&mut self, d: Dialog) {
        match d {
            Dialog::NewScore { staves, systems, mps, c_score, handwritten, template } => {
                self.new_score(build_template(template, staves, systems, mps, c_score, handwritten));
                if self.after_new_wizard {
                    self.after_new_wizard = false;
                    self.dialog = Some(Dialog::Text(self.score.text.clone()));
                }
            }
            Dialog::TimeSig { num, den, from, to } => {
                self.checkpoint();
                for m in from.saturating_sub(1)..to.min(self.score.measures.len()) {
                    self.score.measures[m].num = num;
                    self.score.measures[m].den = den;
                }
            }
            Dialog::KeySig { key, from, to, transpose } => {
                self.checkpoint();
                for m in from.saturating_sub(1)..to.min(self.score.measures.len()) {
                    let old = self.score.measures[m].key;
                    self.score.measures[m].key = key;
                    if transpose && old != key {
                        let semis = ((key as i32 - old as i32) * 7).rem_euclid(12);
                        let semis = if semis > 6 { semis - 12 } else { semis };
                        for s in 0..self.score.staves.len() {
                            for e in self.score.events[s][m].iter_mut() {
                                for p in e.pitches.iter_mut() {
                                    *p = Pitch::from_midi(p.midi() + semis, key < 0);
                                }
                            }
                        }
                    }
                }
            }
            Dialog::Tempo { bpm, measure, to_measure, mode, to_bpm } => {
                self.checkpoint();
                let (a, b) = (measure.min(to_measure), measure.max(to_measure));
                let span = (b - a).max(1) as f32;
                for m in a..=b {
                    let cur = self.score.tempo_at(m) as i64;
                    let v = match mode {
                        0 => bpm as i64,
                        1 => cur * bpm as i64 / 100,
                        2 => (bpm as f32 + (to_bpm as f32 - bpm as f32) * (m - a) as f32 / span).round() as i64,
                        _ => cur + bpm as i64,
                    };
                    if mode == 0 && m > a {
                        continue;
                    }
                    self.score.measures[m].tempo = Some(v.clamp(1, 400) as u32);
                }
            }
            Dialog::Barline { kind, measure, times } => {
                self.checkpoint();
                let (a, b) = self.sel_measures.unwrap_or((measure, measure));
                for m in a..=b {
                    self.score.measures[m].barline = kind;
                    self.score.measures[m].repeat_times = times;
                }
            }
            Dialog::Endings { text, measure } => {
                self.checkpoint();
                self.score.measures[measure].ending = if text.trim().is_empty() { None } else { Some(text) };
            }
            Dialog::Coda { text, measure, rehearsal } => {
                self.checkpoint();
                self.score.measures[measure].coda_phrase = if text.trim().is_empty() { None } else { Some(text) };
                self.score.measures[measure].rehearsal = if rehearsal.trim().is_empty() { None } else { Some(rehearsal) };
            }
            Dialog::MeasureNumbers { mode } => {
                self.checkpoint();
                self.score.layout.show_measure_numbers = mode;
            }
            Dialog::AddMeasures { count, before } => {
                self.checkpoint();
                let at = if before { self.cursor.1 } else { self.cursor.1 + 1 };
                for _ in 0..count {
                    self.score.insert_measure(at);
                }
                self.status = format!("{count} measure(s) added.");
            }
            Dialog::MeasuresPerSystem { n } => {
                self.checkpoint();
                self.score.layout.measures_per_system = n;
                self.score.layout.system_sizes.clear();
            }
            Dialog::SystemsPerPage { n } => {
                self.checkpoint();
                self.score.layout.systems_per_page = n;
            }
            Dialog::Text(t) => {
                self.checkpoint();
                self.score.text = t;
            }
            Dialog::NoteAttributes { velocity, play_len, value, dots, voice, hidden, stem } => {
                let one = self.targets().len() == 1;
                self.modify(|e| {
                    e.velocity = velocity;
                    e.hidden = hidden;
                    e.stem = stem;
                    e.voice = voice;
                    if one {
                        e.value = value;
                        e.dots = dots;
                        e.play_len = if play_len == dur_ticks(value, dots, e.tuplet) { None } else { Some(play_len) };
                    }
                });
            }
            Dialog::Velocity { mode, value, to } => {
                let t = self.targets();
                let n = t.len().max(2) as f32 - 1.0;
                if !t.is_empty() {
                    self.checkpoint();
                }
                for (k, r) in t.iter().enumerate() {
                    if let Some(e) = self.ev_mut(*r) {
                        let v = match mode {
                            0 => value,
                            1 => e.velocity as i32 + value,
                            2 => e.velocity as i32 * value / 100,
                            _ => (value as f32 + (to - value) as f32 * k as f32 / n).round() as i32,
                        };
                        e.velocity = v.clamp(1, 127) as u8;
                    }
                }
            }
            Dialog::SpacingDefaults { stem, beam, tie_h, tie_v, ms } => {
                self.checkpoint();
                let l = &mut self.score.layout;
                l.stem_len_hs = stem;
                l.beam_thick_pct = beam;
                l.tie_h_qs = tie_h;
                l.tie_v_qs = tie_v;
                l.measure_space_hs = ms;
            }
            Dialog::Duration { percent } => {
                self.modify(|e| e.play_len = Some((e.ticks() * percent / 100).max(1)));
            }
            Dialog::Transpose { semitones, octave } => self.transpose_sel(semitones + octave * 12),
            Dialog::Tuplet { notes, in_time } => {
                self.tuplet_ratio = (notes.max(2), in_time.max(1));
                self.cur_tuplet = true;
                self.status = format!("Tuplet {}:{} on.", notes, in_time);
            }
            Dialog::Swing { swing, note } => {
                self.score.playback.swing = swing;
                self.score.playback.swing_note = note;
            }
            Dialog::JumpTo { measure } => {
                self.cursor = (self.cursor.0, measure.saturating_sub(1).min(self.score.measures.len() - 1), 0);
                self.scroll_to_cursor = true;
            }
            Dialog::PageSetup { w, h, margin, staff_space, staff_gap, system_gap } => {
                self.checkpoint();
                let l = &mut self.score.layout;
                l.page_w = w;
                l.page_h = h;
                l.margin = margin;
                l.staff_space = staff_space;
                l.staff_gap = staff_gap;
                l.system_gap = system_gap;
            }
            Dialog::AddStaff { name, clef, program } => {
                self.checkpoint();
                let mut s = Staff::new(&name, clef, program);
                s.channel = (self.score.staves.len() % 15) as u8;
                let at = self.cursor.0 + 1;
                self.score.add_staff(at, s);
            }
            Dialog::SplitStaff { split } => self.split_staff(self.cursor.0, split),
            Dialog::ConnectStaves { kind } => {
                let s = self.cursor.0;
                if s + 1 < self.score.staves.len() {
                    self.checkpoint();
                    let st = &mut self.score.staves[s];
                    st.brace_with_next = kind == 0;
                    st.bracket_with_next = kind == 1;
                    st.barline_to_next = kind <= 2;
                }
            }
            Dialog::ChordSymbol { root, kind, bass, custom, target } => {
                self.checkpoint();
                let text = chord_text(&root, &kind, &bass, &custom);
                if let Some(e) = self.ev_mut(target) {
                    e.chord_symbol = if text.trim().is_empty() { None } else { Some(text.clone()) };
                }
                self.status = if text.is_empty() { "Chord symbol removed.".into() } else { format!("Chord {text} placed above the staff.") };
            }
            Dialog::TextGraphic { text, index, size, bold } => {
                if let Some(gr) = self.score.graphics.get_mut(index) {
                    gr.text = text;
                    gr.size = size;
                    gr.bold = bold;
                }
                if self.score.graphics.get(index).map(|g| g.text.is_empty()).unwrap_or(false) {
                    self.score.graphics.remove(index);
                }
            }
            Dialog::Lyric { target, line, text } => {
                self.set_lyric(target, line, text, true);
            }
            Dialog::Expression { text, target } => {
                self.checkpoint();
                if let Some(e) = self.ev_mut(target) {
                    e.expression = if text.is_empty() { None } else { Some(text) };
                }
            }
            Dialog::ConfirmClose(c) => {
                if self.dirty && !self.save(false) {
                    return;
                }
                self.dirty = false;
                match c {
                    Cmd::Quit => self.quit_requested = true,
                    _ => self.new_score(default_new_score()),
                }
            }
            Dialog::ClickSetup(p) => {
                self.prefs = p;
                crate::prefs::save(&self.prefs);
                self.status = "Click settings saved.".into();
            }
            Dialog::MidiSetup(p) => {
                self.prefs = p;
                crate::prefs::save(&self.prefs);
                self.apply_prefs();
                self.status = format!("MIDI Out A: {}.", self.midi_out.as_ref().map(|m| m.port_label(0)).unwrap_or_else(|| "built-in sound".into()));
            }
            Dialog::ClavinovaGuide | Dialog::MidiPlayback | Dialog::Shortcuts | Dialog::About | Dialog::Message(_) => {}
        }
    }

    /// Store a lyric syllable and open the next note for the following one.
    pub fn set_lyric(&mut self, target: crate::score_view::NoteRef, line: usize, text: String, advance: bool) {
        self.checkpoint();
        if let Some(e) = self.ev_mut(target) {
            if e.lyric.len() <= line {
                e.lyric.resize(line + 1, String::new());
            }
            e.lyric[line] = text;
        }
        self.lyric_line = line;
        if advance {
            if let Some(next) = self.next_note(target) {
                let cur = self.ev(next).and_then(|e| e.lyric.get(line).cloned()).unwrap_or_default();
                self.selected.clear();
                self.selected.insert(next);
                self.dialog = Some(Dialog::Lyric { target: next, line, text: cur });
            }
        }
    }

    /// The note of the same voice before `r` (for Shift+Tab in lyric entry).
    pub fn prev_note(&self, r: crate::score_view::NoteRef) -> Option<crate::score_view::NoteRef> {
        let e = self.ev(r)?;
        let (v, start) = (e.voice, e.start);
        let evs = &self.score.events[r.staff][r.measure];
        if let Some((i, _)) = evs.iter().enumerate().filter(|(_, x)| x.voice == v && x.start < start && !x.is_rest()).max_by_key(|(_, x)| x.start) {
            return Some(crate::score_view::NoteRef { staff: r.staff, measure: r.measure, idx: i });
        }
        for m in (0..r.measure).rev() {
            let evs = &self.score.events[r.staff][m];
            if let Some((i, _)) = evs.iter().enumerate().filter(|(_, x)| x.voice == v && !x.is_rest()).max_by_key(|(_, x)| x.start) {
                return Some(crate::score_view::NoteRef { staff: r.staff, measure: m, idx: i });
            }
        }
        None
    }

    pub fn next_note(&self, r: crate::score_view::NoteRef) -> Option<crate::score_view::NoteRef> {
        let e = self.ev(r)?;
        let v = e.voice;
        let start = e.start;
        let evs = &self.score.events[r.staff][r.measure];
        if let Some((i, _)) = evs.iter().enumerate().filter(|(_, x)| x.voice == v && x.start > start && !x.is_rest()).min_by_key(|(_, x)| x.start) {
            return Some(crate::score_view::NoteRef { staff: r.staff, measure: r.measure, idx: i });
        }
        for m in r.measure + 1..self.score.measures.len() {
            let evs = &self.score.events[r.staff][m];
            if let Some((i, _)) = evs.iter().enumerate().filter(|(_, x)| x.voice == v && !x.is_rest()).min_by_key(|(_, x)| x.start) {
                return Some(crate::score_view::NoteRef { staff: r.staff, measure: m, idx: i });
            }
        }
        None
    }
}

fn dialog_title(d: &Dialog) -> &'static str {
    match d {
        Dialog::NewScore { .. } => "New Score",
        Dialog::TimeSig { .. } => "Time Signature",
        Dialog::KeySig { .. } => "Key Signature",
        Dialog::Tempo { .. } => "Change Tempo",
        Dialog::Barline { .. } => "Barline Type",
        Dialog::Endings { .. } => "Endings",
        Dialog::Coda { .. } => "Coda Phrases",
        Dialog::MeasureNumbers { .. } => "Measure Numbers",
        Dialog::AddMeasures { .. } => "Add Measures",
        Dialog::MeasuresPerSystem { .. } => "Measures per System",
        Dialog::SystemsPerPage { .. } => "Systems per Page",
        Dialog::Text(_) => "Text Elements",
        Dialog::NoteAttributes { .. } => "Note Attributes",
        Dialog::Velocity { .. } => "Change Velocity",
        Dialog::Duration { .. } => "Change Duration",
        Dialog::Transpose { .. } => "Change Pitch / Transpose",
        Dialog::Swing { .. } => "Swing Playback",
        Dialog::Tuplet { .. } => "Tuplet",
        Dialog::MidiPlayback => "MIDI Playback",
        Dialog::MidiSetup(_) => "MIDI Setup",
        Dialog::ClickSetup(_) => "Click Settings",
        Dialog::ClavinovaGuide => "Como ligar o Clavinova",
        Dialog::JumpTo { .. } => "Jump to Measure",
        Dialog::PageSetup { .. } => "Page Setup",
        Dialog::AddStaff { .. } => "Add Staff",
        Dialog::SplitStaff { .. } => "Split Staff",
        Dialog::SpacingDefaults { .. } => "Spacing Defaults",
        Dialog::ConnectStaves { .. } => "Connect Staves",
        Dialog::ChordSymbol { .. } => "Choose Chord",
        Dialog::TextGraphic { .. } => "Text",
        Dialog::Lyric { .. } => "Lyric Entry",
        Dialog::Expression { .. } => "Expression",
        Dialog::Shortcuts => "Keyboard Shortcuts",
        Dialog::About => "About Fermata",
        Dialog::Message(_) => "Fermata",
        Dialog::ConfirmClose(_) => "Unsaved Changes",
    }
}

pub fn item_name(i: &Item) -> String {
    match i {
        Item::Note(v) => format!("note value {v}"),
        Item::Rest(v) => format!("rest value {v}"),
        Item::Accidental(a) => format!("accidental {a:+}"),
        Item::Courtesy => "courtesy accidental".into(),
        Item::Parens => "parentheses".into(),
        Item::Dot(d) => format!("{d} dot(s)"),
        Item::Tuplet => "triplet".into(),
        Item::Clef(c) => format!("{} clef - click a measure", c.name()),
        Item::Mark(m) => format!("{} - click a note", m.label()),
        Item::Dynamic(d) => format!("dynamic {d} - click a note"),
        Item::Expression(e) => format!("expression \"{e}\" - click a note"),
        Item::Color(_) => "color".into(),
        Item::Graphic(k) => format!("{k:?} - drag on the score"),
        Item::Lyric => "lyrics - click a note".into(),
        Item::Chord => "chord symbol - click a note".into(),
        Item::ChordGrid => "chord grid - click a note".into(),
        Item::TempoTool => "tempo - click a measure".into(),
        Item::MidiTool => "MIDI attributes - click a note".into(),
        Item::Guitar(t) => format!("guitar mark {t} - click a note"),
    }
}


/// Small painted icons for the score window toolbar (the bundled font has no such symbols).
fn strip_sep(ui: &mut Ui) {
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(2.0, 22.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, Color32::from_rgb(52, 104, 170)));
    ui.painter().line_segment([r.center_top() + vec2(1.0, 0.0), r.center_bottom() + vec2(1.0, 0.0)], Stroke::new(1.0, Color32::from_rgb(150, 196, 240)));
    ui.add_space(4.0);
}

/// A sunken text box on the strip, like the original's "Thru A1" and "M1" fields.
fn field_button(ui: &mut Ui, text: &str, w: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), Sense::click());
    let p = ui.painter_at(r);
    let bg = if resp.hovered() { Color32::from_rgb(214, 232, 252) } else { Color32::from_rgb(196, 220, 246) };
    p.rect_filled(r, 1.0, bg);
    p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, Color32::from_rgb(60, 100, 150)));
    p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, Color32::from_rgb(60, 100, 150)));
    p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, Color32::WHITE));
    p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, Color32::WHITE));
    p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(11.5), Color32::from_gray(20));
    resp
}

/// The small page symbol with the page number between the page arrows.
fn page_icon(ui: &mut Ui, n: usize) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 24.0), Sense::hover());
    let p = ui.painter_at(r);
    let page = Rect::from_center_size(r.center(), vec2(15.0, 19.0));
    p.add(egui::Shape::convex_polygon(
        vec![page.left_top(), page.right_top() - vec2(5.0, 0.0), page.right_top() + vec2(0.0, 5.0), page.right_bottom(), page.left_bottom()],
        Color32::WHITE,
        Stroke::new(1.0, Color32::from_gray(60)),
    ));
    p.text(page.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, n.to_string(), FontId::proportional(10.0), Color32::BLACK);
    resp
}

/// Small painted icons for the score window toolbar (the bundled font has no such symbols).
fn icon_button(ui: &mut Ui, kind: u8, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let p = ui.painter_at(r);
    if on {
        p.rect_filled(r, 2.0, Color32::from_rgb(214, 232, 252));
        p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(40, 80, 140)), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 70));
        p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(220, 236, 255)), egui::StrokeKind::Inside);
    }
    let c = r.center();
    let k = Color32::from_gray(15);
    match kind {
        0 => {
            let pts = vec![c + vec2(-4.0, -7.0), c + vec2(-4.0, 6.0), c + vec2(-1.0, 3.0), c + vec2(2.0, 8.0), c + vec2(4.0, 7.0), c + vec2(1.0, 2.0), c + vec2(5.0, 2.0)];
            p.add(egui::Shape::convex_polygon(pts.clone(), Color32::BLACK, Stroke::NONE));
            p.add(egui::Shape::closed_line(pts, Stroke::new(1.0, Color32::WHITE)));
        }
        1 => {
            let pts = vec![c + vec2(-7.0, 3.0), c + vec2(0.0, -5.0), c + vec2(7.0, 0.0), c + vec2(1.0, 7.0), c + vec2(-3.0, 7.0)];
            p.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::new(1.2, k)));
        }
        2 => {
            p.line_segment([c + vec2(-6.0, 7.0), c + vec2(5.0, -6.0)], Stroke::new(4.0, Color32::from_rgb(232, 196, 60)));
            p.line_segment([c + vec2(-6.0, 7.0), c + vec2(-4.0, 5.0)], Stroke::new(2.0, k));
        }
        3 => {
            p.circle_filled(c, 5.0, Color32::from_rgb(220, 20, 20));
        }
        4 => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-4.0, 6.0)], k, Stroke::NONE));
        }
        5 => {
            p.rect_filled(Rect::from_center_size(c, vec2(10.0, 10.0)), 0.0, k);
        }
        8 => {
            // click (metronome)
            p.circle_filled(c, 7.5, Color32::WHITE);
            p.circle_stroke(c, 7.5, Stroke::new(1.2, Color32::from_rgb(200, 30, 30)));
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, 5.0), c + vec2(4.0, 5.0), c + vec2(1.5, -5.0), c + vec2(-1.5, -5.0)], Color32::from_rgb(90, 90, 90), Stroke::NONE));
            p.line_segment([c + vec2(0.0, 4.0), c + vec2(4.0, -5.0)], Stroke::new(1.3, Color32::from_rgb(200, 30, 30)));
        }
        9 => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-3.0, -5.0), c + vec2(5.0, 0.0), c + vec2(-3.0, 5.0)], k, Stroke::NONE));
        }
        10 => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(3.0, -5.0), c + vec2(3.0, 5.0), c + vec2(-4.0, 0.0)], k, Stroke::NONE));
        }
        11 => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)], k, Stroke::NONE));
        }
        _ => {
            p.circle_stroke(c + vec2(-2.0, -2.0), 5.0, Stroke::new(1.6, k));
            p.circle_filled(c + vec2(-2.0, -2.0), 3.6, Color32::from_rgba_unmultiplied(255, 255, 255, 120));
            p.line_segment([c + vec2(2.0, 2.0), c + vec2(7.0, 7.0)], Stroke::new(2.5, k));
        }
    }
    resp
}

/// The Choose Chord window's chord types (the original's names), by family and row; None leaves a cell empty.
pub const CHORD_GRID: [(&str, &[[Option<&str>; 7]]); 4] = [
    (
        "Maj",
        &[
            [Some(""), Some("7"), Some("maj7"), Some("9"), Some("maj9"), Some("13"), Some("maj13")],
            [Some("5"), Some("7(b5)"), Some("maj7(b5)"), Some("9(b5)"), Some("maj9(b5)"), Some("13(b5)"), Some("maj13(b5)")],
            [Some("6"), Some("7(b9)"), None, None, None, Some("13(b9)"), None],
            [Some("6/9"), Some("7(#9)"), Some("maj7(6/9)"), None, None, Some("13(#9)"), None],
            [Some("(add2)"), Some("7(#11)"), Some("maj7(#11)"), Some("9(#11)"), Some("maj9(#11)"), Some("13(#11)"), Some("maj13(#11)")],
            [Some("(add9)"), Some("7(b5,b9)"), None, None, None, None, None],
            [Some("(omit3)"), Some("7(b5,#9)"), None, None, None, Some("11"), None],
            [Some("(omit5)"), Some("7(b9,#9)"), None, None, None, None, Some("sus2")],
            [Some("sus4"), Some("7sus4"), None, Some("9sus4"), None, Some("13sus4"), Some("sus2, sus4")],
        ],
    ),
    (
        "Min",
        &[
            [Some("m"), Some("m7"), Some("m(maj7)"), Some("m9"), None, Some("m13"), None],
            [Some("m(add2)"), Some("m7(add4)"), None, Some("m9(maj7)"), Some("m11"), None, None],
            [Some("m(add9)"), Some("m7(add11)"), Some("m6"), Some("m6/9"), None, None, None],
        ],
    ),
    (
        "Aug",
        &[
            [Some("+"), Some("+7"), Some("maj7(+5)"), Some("+9"), Some("maj9(+5)"), None, None],
            [None, Some("+7(b9)"), None, None, None, None, None],
            [None, Some("+7(#9)"), None, None, None, None, None],
        ],
    ),
    ("Dim", &[[Some("dim"), Some("m7(b5)"), None, None, None, None, None], [None, Some("dim7"), None, None, None, None, None]]),
];

/// The chord symbol text from the Choose Chord window's choices (custom text wins).
pub fn chord_text(root: &str, kind: &str, bass: &str, custom: &str) -> String {
    if !custom.trim().is_empty() {
        return custom.trim().to_string();
    }
    if root.is_empty() {
        return String::new();
    }
    let mut t = format!("{root}{kind}");
    if !bass.is_empty() && bass != root {
        t.push('/');
        t.push_str(bass);
    }
    t
}
