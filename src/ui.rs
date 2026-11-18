//! Menus, toolbars, floating palettes, dialogs and side windows.
use crate::app::*;
use crate::commands::{self, Cmd};
use crate::kit;
use crate::model::*;
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, Ui};

/// The shortcut text of a command as menus, tips and Keyboard Shortcuts show it (Mac key symbols, Ctrl first).
fn shortcut_text(c: Cmd) -> String {
    theme::shortcut_glyphs(c)
}

impl App {
    fn mi(&mut self, ui: &mut Ui, c: Cmd) {
        if kit::menu_item(ui, &commands::label(c), &shortcut_text(c), false, true).clicked() {
            self.pending_cmd = Some(c);
            ui.close();
        }
    }
    /// A menu item for something not built yet: greyed out, no check mark, its shortcut still shown, and the tip
    /// "Not available yet".
    fn mi_unavailable(&mut self, ui: &mut Ui, c: Cmd) {
        let r = kit::menu_item(ui, &commands::label(c), &shortcut_text(c), false, false);
        tip(r, "Not available yet", "");
    }

    fn mi_check(&mut self, ui: &mut Ui, c: Cmd, on: bool) {
        if kit::menu_item(ui, &commands::label(c), &shortcut_text(c), on, true).clicked() {
            self.pending_cmd = Some(c);
            ui.close();
        }
    }

    /// View ▸ Appearance: applies the look at once and keeps it for the next sessions.
    pub fn set_appearance(&mut self, ctx: &egui::Context, a: crate::prefs::Appearance) {
        self.prefs.appearance = a;
        // a choice made in the menu wins over a --theme given on the command line
        self.theme_forced = false;
        ctx.set_theme(a.preference());
        if !self.screenshot_mode {
            crate::prefs::save(&self.prefs);
        }
    }

    pub fn menu_bar(&mut self, ui: &mut Ui) {
        use Cmd::*;
        // the menu titles: blue while their menu is open, a light shade under the mouse
        let k = theme::t(ui);
        {
            let w = &mut ui.visuals_mut().widgets;
            w.open.weak_bg_fill = k.accent;
            w.open.fg_stroke = Stroke::new(1.0, k.accent_fg);
            w.open.bg_stroke = Stroke::NONE;
            w.hovered.weak_bg_fill = k.cell_hover;
            w.hovered.bg_stroke = Stroke::NONE;
            for v in [&mut w.open, &mut w.hovered, &mut w.inactive, &mut w.active] {
                v.corner_radius = egui::CornerRadius::same(5);
            }
        }
        // egui marks the title of the open menu as "selected"
        ui.visuals_mut().selection.bg_fill = k.accent;
        ui.visuals_mut().selection.stroke = Stroke::new(1.0, k.accent_fg);
        egui::MenuBar::new().config(egui::containers::menu::MenuConfig::new().style(kit::menu_style)).ui(ui, |ui| {
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
                // the browser: closing the tab or the installed app's window ends it; there's nothing to quit
                if !cfg!(target_arch = "wasm32") {
                    ui.separator();
                    self.mi(ui, Quit);
                }
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
                ui.separator();
                // light or dark look (F-15): no shortcut; the menu stays open, like "Audition notes while editing"
                let keep_open = egui::containers::menu::MenuConfig::new()
                    .style(kit::menu_style)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                egui::containers::menu::SubMenuButton::new("Appearance").config(keep_open).ui(ui, |ui| {
                    use crate::prefs::Appearance;
                    let cur = self.prefs.appearance;
                    let mut pick = None;
                    if kit::menu_item(ui, "Use System Setting", "", cur == Appearance::System, true).clicked() {
                        pick = Some(Appearance::System);
                    }
                    ui.separator();
                    if kit::menu_item(ui, "Light", "", cur == Appearance::Light, true).clicked() {
                        pick = Some(Appearance::Light);
                    }
                    if kit::menu_item(ui, "Dark", "", cur == Appearance::Dark, true).clicked() {
                        pick = Some(Appearance::Dark);
                    }
                    if let Some(a) = pick {
                        self.set_appearance(ui.ctx(), a);
                    }
                });
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
                let (a, b, e) = (self.prefs.click_on, self.score.playback.follow, self.qwerty);
                self.mi_check(ui, ClickOnOff, a);
                self.mi(ui, ClickSetup);
                self.mi_check(ui, FollowPlayback, b);
                // not built yet: greyed out until they do something
                self.mi_unavailable(ui, AutoGuessBeam);
                self.mi_unavailable(ui, AutoSpace);
                ui.separator();
                self.mi_check(ui, QwertyMode, e);
                ui.separator();
                self.mi(ui, MidiSetup);
                self.mi(ui, MidiPlayback);
                // a toggle that keeps the menu open, as before
                if kit::menu_item(ui, "Audition notes while editing", "", self.audition, true).clicked() {
                    self.audition = !self.audition;
                }
            });
            ui.menu_button("Help", |ui| {
                self.mi(ui, ClavinovaGuide);
                ui.separator();
                self.mi(ui, Shortcuts);
                self.mi(ui, About);
            });
        });
    }

    /// The toolbar under the menus, in the original's order: Voice ▾ | Arrow Eraser Pencil | Record Play Stop Click
    /// Play-from-cursor | Zoom − 100% + | measure. Everything else lives in the menus, shortcuts and palettes;
    /// Thru and the page arrows are in the status bar.
    pub fn score_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let vlabel = match self.show_voice {
                None => "Voice –".to_string(),
                Some(v) => format!("Voice {v}"),
            };
            let vresp = tip(voice_button(ui, &vlabel), "Voice: the voice you enter and the voices shown", "V 1–8 · V A");
            egui::Popup::menu(&vresp).style(kit::menu_style).show(|ui| {
                ui.set_min_width(200.0);
                for v in 1..=8u8 {
                    if kit::menu_item(ui, &format!("Enter in voice {v}"), "", self.voice == v, true).clicked() {
                        self.voice = v;
                        ui.close();
                    }
                }
                ui.separator();
                if kit::menu_item(ui, "Show all voices", "", self.show_voice.is_none(), true).clicked() {
                    self.show_voice = None;
                    ui.close();
                }
                for v in 1..=8u8 {
                    if kit::menu_item(ui, &format!("Show voice {v} only"), "", self.show_voice == Some(v), true).clicked() {
                        self.show_voice = Some(v);
                        ui.close();
                    }
                }
            });
            bar_sep(ui);
            for (t, icon, name, key) in [(Tool::Arrow, Icon::Arrow, "Arrow tool", "A · Esc"), (Tool::Eraser, Icon::Eraser, "Eraser tool", "E"), (Tool::Pencil, Icon::Pencil, "Pencil tool", "P")] {
                if tip(icon_button(ui, icon, self.tool == t), name, key).clicked() {
                    self.tool = t;
                }
            }
            bar_sep(ui);
            if tip(icon_button(ui, Icon::Record, self.rec.is_some()), "Record", &shortcut_text(Cmd::Record)).clicked() {
                self.pending_cmd = Some(Cmd::Record);
            }
            let playing = self.is_playing();
            if tip(icon_button(ui, Icon::Play, playing), "Play", &shortcut_text(Cmd::Play)).clicked() {
                self.pending_cmd = Some(Cmd::Play);
            }
            if tip(icon_button(ui, Icon::Stop, false), "Stop", &shortcut_text(Cmd::Stop)).clicked() {
                self.pending_cmd = Some(Cmd::Stop);
            }
            if tip(icon_button(ui, Icon::Metronome, self.prefs.click_on), "Click On/Off", &shortcut_text(Cmd::ClickOnOff)).clicked() {
                self.pending_cmd = Some(Cmd::ClickOnOff);
            }
            if tip(icon_button(ui, Icon::PlayFrom, false), "Play from the cursor measure", "").clicked() {
                let m = self.cursor.1;
                self.play_from(m);
            }
            bar_sep(ui);
            let zo = tip(icon_button(ui, Icon::ZoomOut, false), "Zoom Out", &shortcut_text(Cmd::ZoomOut));
            if zo.clicked() {
                self.pending_cmd = Some(Cmd::ZoomOut);
            }
            let k = theme::t(ui);
            let (zr, _) = ui.allocate_exact_size(vec2(50.0, 28.0), Sense::hover());
            ui.painter().text(zr.center(), Align2::CENTER_CENTER, format!("{:.0}%", self.zoom / 1.35 * 100.0), theme::font_medium(13.0), k.text);
            // a click zooms in, a right-click zooms out, as the original's zoom button did
            let zi = tip(icon_button(ui, Icon::ZoomIn, false), "Zoom In", &shortcut_text(Cmd::ZoomIn));
            if zi.clicked() {
                self.pending_cmd = Some(Cmd::ZoomIn);
            }
            if zi.secondary_clicked() || zo.secondary_clicked() {
                self.pending_cmd = Some(Cmd::ZoomOut);
            }
            bar_sep(ui);
            let m = self.play_position().map(|x| x.0).unwrap_or(self.cursor.1);
            if tip(measure_box(ui, m + 1), "Jump to Measure", &shortcut_text(Cmd::JumpToMeasure)).clicked() {
                self.pending_cmd = Some(Cmd::JumpToMeasure);
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

    /// Status bar in segments: cursor | time | key | tempo | MIDI and Thru | split | pages | QWERTY, REC | message.
    pub fn status_bar(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
            let f = theme::font(11.5);
            let txt = |s: String| egui::RichText::new(s).font(theme::font(11.5)).color(k.text_2);
            let (s, m, t) = self.cursor;
            let mi = &self.score.measures[m.min(self.score.measures.len() - 1)];
            let beat = t / (TPQ * 4 / mi.den.max(1) as u32) + 1;
            ui.add_space(4.0);
            ui.label(txt(format!("Staff {} · Measure {} · Beat {}", s + 1, m + 1, beat)));
            status_sep(ui);
            ui.label(txt(format!("{}/{}", mi.num, mi.den)));
            status_sep(ui);
            ui.label(txt(key_name(mi.key).to_string()));
            status_sep(ui);
            ui.label(txt(format!("Tempo {}", self.score.tempo_at(m))));
            status_sep(ui);
            // MIDI keyboard, output and Thru
            let (mtxt, dot) = match &self.midi {
                Some(mm) if mm.connected() => {
                    let last = mm.last_note.load(std::sync::atomic::Ordering::Relaxed);
                    let heard = if last == 255 { "no notes yet".to_string() } else { format!("last note {}", Pitch::from_midi(last as i32, false).name()) };
                    (format!("MIDI in: {} ({heard})", mm.names.join(", ")), k.ok)
                }
                _ => (web_midi_note().unwrap_or("No MIDI keyboard").to_string(), k.text_3),
            };
            let (dr, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dr.center(), 3.5, dot);
            if ui.add(egui::Label::new(txt(mtxt)).sense(Sense::click())).on_hover_text("MIDI Setup").clicked() {
                self.pending_cmd = Some(Cmd::MidiSetup);
            }
            if let Some(mo) = self.midi_out.as_ref() {
                match (&mo.chosen[0], &mo.connected[0]) {
                    (Some(_), Some(o)) => {
                        ui.label(egui::RichText::new(format!("· Out A: {o} ({} sent)", mo.sent[0].load(std::sync::atomic::Ordering::Relaxed))).font(f.clone()).color(k.ok));
                    }
                    (Some(c), None) => {
                        ui.label(egui::RichText::new(format!("· Out A: {c} not found")).font(f.clone()).color(k.warn));
                    }
                    _ => {}
                }
            }
            ui.label(txt("·".into()));
            let (tp, tc) = self.thru_target();
            let thru = if self.audition { format!("Thru {}{}", if tp == 0 { "A" } else { "B" }, tc + 1) } else { "Thru Off".to_string() };
            if ui.add(egui::Label::new(txt(thru)).sense(Sense::click())).on_hover_text("MIDI Thru: what you play goes to this port and channel (click to turn on/off)").clicked() {
                self.audition = !self.audition;
                self.prefs.thru_on = self.audition;
                if !self.screenshot_mode {
                    crate::prefs::save(&self.prefs);
                }
            }
            status_sep(ui);
            ui.label(txt(format!("Split = {}", Pitch::from_midi(self.rec_settings.split as i32, false).name())));
            status_sep(ui);
            // pages: ‹ Page 1 of 2 ›
            let pages = self.geo.pages.len().max(1);
            let cur = self.geo.systems.iter().find(|sy| m >= sy.m0 && m < sy.m1).map(|sy| sy.page).unwrap_or(0).min(pages - 1);
            if ui.add(egui::Label::new(txt("‹".into())).sense(Sense::click())).on_hover_text("Previous page").clicked() && cur > 0 {
                self.goto_page(cur - 1);
            }
            ui.label(txt(format!("Page {} of {}", cur + 1, pages)));
            if ui.add(egui::Label::new(txt("›".into())).sense(Sense::click())).on_hover_text("Next page").clicked() && cur + 1 < pages {
                self.goto_page(cur + 1);
            }
            status_sep(ui);
            if self.qwerty {
                status_tag(ui, "QWERTY", k.accent_soft, k.accent);
            }
            if self.rec.is_some() {
                status_tag(ui, "● REC", k.danger, k.danger_fg);
            }
            if self.qwerty || self.rec.is_some() {
                status_sep(ui);
            }
            ui.add(egui::Label::new(egui::RichText::new(&self.status).font(f).color(k.text_3)).truncate());
        });
    }

    // ---------------- palettes ----------------

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

    // ---------------- windows ----------------

    pub fn side_windows(&mut self, ctx: &egui::Context) {
        if self.staff_sheet {
            let mut open = true;
            kit::float_window(ctx, "Staff Sheet", &mut open, pos2(20.0, 300.0), |ui| self.staff_sheet_ui(ui));
            self.staff_sheet = open;
        }
        if self.keyboard_win {
            let mut open = true;
            kit::float_window(ctx, "Keyboard", &mut open, pos2(260.0, 600.0), |ui| self.keyboard_ui(ui));
            self.keyboard_win = open;
        }
        if self.tempo_win {
            let mut open = true;
            kit::float_window(ctx, "Tempo", &mut open, pos2(900.0, 640.0), |ui| {
                let m = self.cursor.1;
                let mut t = self.score.tempo_at(m);
                kit::form(ui, "tempo_win", |ui| {
                    kit::row(ui, "beats per minute", |ui| kit::slider(ui, &mut t, 20..=300, |v| format!("{}", v as i64)));
                    kit::row(ui, "Volume", |ui| kit::slider(ui, &mut self.score.playback.master_volume, 0.0..=1.0, |v| format!("{:.0}%", v * 100.0)));
                });
                if t != self.score.tempo_at(m) {
                    // change the tempo mark governing this measure
                    let idx = (0..=m).rev().find(|&i| self.score.measures[i].tempo.is_some()).unwrap_or(0);
                    self.score.measures[idx].tempo = Some(t);
                    self.dirty = true;
                }
                ui.horizontal(|ui| {
                    let playing = self.is_playing();
                    if tip(icon_button(ui, Icon::Play, playing), "Play", &shortcut_text(Cmd::Play)).clicked() {
                        self.pending_cmd = Some(Cmd::Play);
                    }
                    if tip(icon_button(ui, Icon::Stop, false), "Stop", &shortcut_text(Cmd::Stop)).clicked() {
                        self.pending_cmd = Some(Cmd::Stop);
                    }
                });
            });
            self.tempo_win = open;
        }
    }

    /// Staff Sheet: one row per staff, the cursor staff in light blue; scrolls sideways when the window is narrow.
    fn staff_sheet_ui(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let n = self.score.staves.len();
        let k = theme::t(ui);
        let cursor_row = self.cursor.0 + 1;
        let max_w = (ui.ctx().content_rect().width() - 90.0).max(400.0);
        egui::Frame::new().fill(k.field).stroke(Stroke::new(1.0, k.line)).corner_radius(9.0).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            egui::ScrollArea::both().max_height(420.0).max_width(max_w).auto_shrink([true, true]).show(ui, |ui| {
                let (head, soft, zebra) = (k.panel_2, k.accent_soft, k.cell);
                egui::Grid::new("sheet")
                    .spacing([10.0, 6.0])
                    .min_row_height(30.0)
                    .with_row_color(move |row, _| if row == 0 { Some(head) } else if row == cursor_row { Some(soft) } else if row % 2 == 0 { Some(zebra) } else { None })
                    .show(ui, |ui| {
                        for h in ["#", "Name", "Abbrev.", "Clef", "Instrument", "Port", "Ch", "Vol", "Pan", "Transp.", "Hide", "Mute", "Solo", "Brace", "Bracket", "Bar thru", "Lines", "Drum"] {
                            ui.label(egui::RichText::new(h).font(theme::font_semibold(11.5)).color(k.text_2));
                        }
                        ui.end_row();
                        for i in 0..n {
                            let st = &mut self.score.staves[i];
                            ui.label(format!("{}", i + 1));
                            changed |= ui.add_sized([130.0, 24.0], egui::TextEdit::singleline(&mut st.name)).changed();
                            changed |= ui.add_sized([64.0, 24.0], egui::TextEdit::singleline(&mut st.abbrev)).changed();
                            let mut clef = st.clef_at(0);
                            egui::ComboBox::from_id_salt(("clef", i)).selected_text(clef.name()).width(110.0).show_ui(ui, |ui| {
                                kit::menu_look(ui);
                                for c in Clef::ALL {
                                    changed |= ui.selectable_value(&mut clef, c, c.name()).changed();
                                }
                            });
                            if clef != st.clef_at(0) {
                                st.set_clef(0, clef);
                            }
                            egui::ComboBox::from_id_salt(("prog", i)).selected_text(format!("{} {}", st.program as usize + 1, GM_NAMES[st.program as usize])).width(200.0).show_ui(ui, |ui| {
                                kit::menu_look(ui);
                                for (kk, nm) in GM_NAMES.iter().enumerate() {
                                    changed |= ui.selectable_value(&mut st.program, kk as u8, format!("{} {}", kk + 1, nm)).changed();
                                }
                            });
                            egui::ComboBox::from_id_salt(("port", i)).selected_text(if st.port == 0 { "A" } else { "B" }).width(48.0).show_ui(ui, |ui| {
                                kit::menu_look(ui);
                                changed |= ui.selectable_value(&mut st.port, 0, "A").changed();
                                changed |= ui.selectable_value(&mut st.port, 1, "B").changed();
                            });
                            let mut ch = st.channel + 1;
                            changed |= kit::num_with(ui, &mut ch, 1..=16, 60.0, |d| d).changed();
                            st.channel = ch - 1;
                            changed |= kit::num_with(ui, &mut st.volume, 0..=127, 66.0, |d| d).changed();
                            changed |= kit::num_with(ui, &mut st.pan, -64..=63, 66.0, |d| d).changed();
                            changed |= kit::num_with(ui, &mut st.transpose, -36..=36, 66.0, |d| d).changed();
                            changed |= kit::checkbox(ui, &mut st.hidden, "").changed();
                            changed |= kit::checkbox(ui, &mut st.muted, "").changed();
                            changed |= kit::checkbox(ui, &mut st.solo, "").changed();
                            changed |= kit::checkbox(ui, &mut st.brace_with_next, "").changed();
                            changed |= kit::checkbox(ui, &mut st.bracket_with_next, "").changed();
                            changed |= kit::checkbox(ui, &mut st.barline_to_next, "").changed();
                            changed |= kit::num_with(ui, &mut st.lines, 1..=6, 56.0, |d| d).changed();
                            changed |= kit::checkbox(ui, &mut st.drum, "").changed();
                            ui.end_row();
                        }
                    });
            });
        });
        if self.score.staves.iter().all(|s| s.hidden) {
            self.score.staves[0].hidden = false;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if kit::btn_inline(ui, "Add Staff...").clicked() {
                self.pending_cmd = Some(Cmd::AddStaff);
            }
            if button_danger_inline(ui, "Delete Cursor Staff").clicked() {
                self.pending_cmd = Some(Cmd::DeleteStaff);
            }
            if kit::btn_inline(ui, "Move Cursor Staff Up").clicked() && self.cursor.0 > 0 {
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

    /// The pitches of the note at the cursor (lit on the Keyboard window).
    fn cursor_pitches(&self) -> Vec<i32> {
        let (s, m, t) = self.cursor;
        self.score.events.get(s).and_then(|st| st.get(m)).map(|evs| evs.iter().filter(|e| e.start == t && !e.is_rest()).flat_map(|e| e.pitches.iter().map(|p| p.midi())).collect()).unwrap_or_default()
    }

    fn keyboard_ui(&mut self, ui: &mut Ui) {
        let low = 36; // C2
        let octaves = 5;
        let ww = 18.0;
        let k = theme::t(ui);
        let lit = self.cursor_pitches();
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
            p.rect_filled(*kr, 0.0, if lit.contains(m) { k.accent_soft } else { theme::PIANO_WHITE });
            p.rect_stroke(*kr, 0.0, Stroke::new(1.0, theme::PIANO_LINE), egui::StrokeKind::Inside);
            if m % 12 == 0 {
                p.text(kr.center_bottom() - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("C{}", m / 12 - 1), theme::font(9.0), theme::PIANO_LINE);
            }
        }
        for (m, kr) in &blacks {
            p.rect_filled(*kr, 1.0, if lit.contains(m) { k.accent } else { theme::PIANO_BLACK });
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
        kit::help(ui, "Click a key to enter that note at the red cursor with the current note value.");
    }

    // ---------------- dialogs ----------------

    pub fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else {
            self.pb_backup = None;
            return;
        };
        // MIDI Playback changes are heard at once; Cancel puts back what was there when the dialog opened
        if matches!(d, Dialog::MidiPlayback) && self.pb_backup.is_none() {
            self.pb_backup = Some(self.score.playback.clone());
        }
        let mut keep = true;
        let mut ok = false;
        let mut cancel = false;
        let title = dialog_title(&d);
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let alert = matches!(d, Dialog::ConfirmClose(_) | Dialog::Message(_) | Dialog::About);
        let wid = egui::Id::unique(("dialog_w", title));
        egui::Window::new(title)
            .id(egui::Id::unique("dialog"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(kit::window_frame(ctx))
            .show(ctx, |ui| {
                if alert {
                    self.alert_body(ui, &d, &mut ok, &mut keep, &mut cancel);
                    return;
                }
                let width = ui.memory(|m| m.data.get_temp::<f32>(wid)).unwrap_or(320.0);
                if kit::title_bar(ui, title, width, 32.0, false) {
                    cancel = true;
                }
                let body = egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 6, bottom: 18 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                    ui.set_max_width(980.0);
                    self.dialog_body(ui, &mut d);
                    ui.add_space(8.0);
                    // footer: the main button on the right, Cancel to its left
                    kit::footer(ui, |ui| match &d {
                        Dialog::Shortcuts | Dialog::ClavinovaGuide => {
                            if kit::btn_primary(ui, "OK").clicked() || enter {
                                keep = false;
                            }
                        }
                        Dialog::NewScore { .. } => {
                            if kit::btn_primary(ui, "Create").clicked() || enter {
                                ok = true;
                            }
                            if kit::btn(ui, "Create, then open Text Elements").clicked() {
                                ok = true;
                                self.after_new_wizard = true;
                            }
                            if kit::btn(ui, "Cancel").clicked() {
                                cancel = true;
                            }
                        }
                        _ => {
                            if kit::btn_primary(ui, "OK").clicked() || enter {
                                ok = true;
                            }
                            if kit::btn(ui, "Cancel").clicked() {
                                cancel = true;
                            }
                        }
                    });
                });
                ui.memory_mut(|m| m.data.insert_temp(wid, body.response.rect.width()));
            });
        if esc {
            cancel = true;
        }
        if cancel {
            keep = false;
            ok = false;
            if let Some(b) = self.pb_backup.take() {
                self.score.playback = b;
            }
        }
        if ok {
            keep = false;
            if matches!(d, Dialog::MidiPlayback) {
                // one undo step for everything changed in the dialog
                if let Some(b) = self.pb_backup.take() {
                    if b != self.score.playback {
                        let now = std::mem::replace(&mut self.score.playback, b);
                        self.checkpoint();
                        self.score.playback = now;
                    }
                }
            }
            self.apply_dialog(d.clone());
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        } else if self.dialog.is_none() {
            self.pb_backup = None;
        }
    }

    /// Alerts (Unsaved Changes, messages, About): icon, sentence in bold, buttons in the macOS order.
    fn alert_body(&mut self, ui: &mut Ui, d: &Dialog, ok: &mut bool, keep: &mut bool, cancel: &mut bool) {
        let k = theme::t(ui);
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 22, bottom: 20 }).show(ui, |ui| {
            ui.set_width(380.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                kit::alert_icon(ui, matches!(d, Dialog::Message(_)), if matches!(d, Dialog::About) { 64.0 } else { 44.0 });
                let (head, sub) = match d {
                    Dialog::ConfirmClose(_) => ("Save changes to this score first?".to_string(), self.file_base_name()),
                    Dialog::Message(m) => (m.clone(), String::new()),
                    _ => (
                        "Fermata".to_string(),
                        format!(
                            "A free music notation editor, rebuilt with the same workflow as a classic desktop notation program.\nVersion {} - score layout, palettes, shortcuts, note entry, playback, MIDI and MusicXML.",
                            env!("CARGO_PKG_VERSION")
                        ),
                    ),
                };
                ui.add(egui::Label::new(egui::RichText::new(head).font(theme::font_semibold(14.0)).color(k.text)).wrap());
                if !sub.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(sub).font(theme::font(12.0)).color(k.text_3)).wrap());
                }
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                match d {
                    Dialog::ConfirmClose(_) => {
                        // macOS order: Don't Save on the left; Cancel and Save on the right (Esc = Cancel, Enter = Save)
                        if kit::btn(ui, "Don't Save").clicked() {
                            self.dirty = false;
                            *ok = true;
                        }
                        kit::footer(ui, |ui| {
                            if kit::btn_primary(ui, "Save").clicked() || enter {
                                *ok = true;
                            }
                            if kit::btn(ui, "Cancel").clicked() {
                                *cancel = true;
                            }
                        });
                    }
                    _ => {
                        kit::footer(ui, |ui| {
                            if kit::btn_primary(ui, "OK").clicked() || enter {
                                *keep = false;
                            }
                        });
                    }
                }
            });
        });
    }

    fn dialog_body(&mut self, ui: &mut Ui, d: &mut Dialog) {
        let nm = self.score.measures.len();
        // measure numbers are kept from 0 but shown and typed from 1
        let pitch_fmt = |v: f64| Pitch::from_midi(v as i32, false).name();
        let int_fmt = |v: f64| format!("{}", v as i64);
        match d {
            Dialog::NewScore { staves, systems, mps, c_score, handwritten, template } => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 24.0;
                    ui.vertical(|ui| {
                        kit::form(ui, "newscore", |ui| {
                            kit::row(ui, "Staves per System", |ui| kit::num(ui, staves, 1..=32));
                            kit::row(ui, "Systems per Page", |ui| kit::num(ui, systems, 1..=12));
                            kit::row(ui, "Measures per System", |ui| kit::num(ui, mps, 1..=16));
                            kit::row(ui, "", |ui| kit::checkbox(ui, c_score, "Make a \"C\" Score"));
                            kit::row(ui, "", |ui| {
                                // no handwritten music font yet: greyed out, and never on
                                *handwritten = false;
                                ui.add_enabled_ui(false, |ui| kit::checkbox(ui, handwritten, "Use Handwritten Style")).response.on_disabled_hover_text("Not available yet")
                            });
                        });
                    });
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Starting Template").font(theme::font_semibold(11.5)).color(theme::t(ui).text_2));
                        let k = theme::t(ui);
                        egui::Frame::new().fill(k.field).stroke(Stroke::new(1.0, k.line)).corner_radius(9.0).inner_margin(4.0).show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for (k, t) in TEMPLATES.iter().enumerate() {
                                    if kit::list_row(ui, *template == k, t, 240.0).clicked() {
                                        *template = k;
                                    }
                                }
                            });
                        });
                    });
                });
            }
            Dialog::TimeSig { num, den, from, to } => {
                kit::form(ui, "timesig", |ui| {
                    kit::row(ui, "Time Signature", |ui| {
                        kit::num(ui, num, 1..=32);
                        ui.label("/");
                        egui::ComboBox::from_id_salt("den").selected_text(den.to_string()).width(60.0).show_ui(ui, |ui| {
                            kit::menu_look(ui);
                            for v in [1u8, 2, 4, 8, 16, 32] {
                                ui.selectable_value(den, v, v.to_string());
                            }
                        });
                    });
                    kit::row(ui, "", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (a, b) in [(2, 4), (3, 4), (4, 4), (6, 8), (9, 8), (12, 8), (2, 2), (5, 4)] {
                            if kit::chip(ui, *num == a && *den == b, &format!("{a}/{b}")).clicked() {
                                *num = a;
                                *den = b;
                            }
                        }
                    });
                    kit::row(ui, "From measure", |ui| kit::num(ui, from, 1..=nm));
                    kit::row(ui, "Through measure", |ui| kit::num(ui, to, 1..=nm));
                });
            }
            Dialog::KeySig { key, from, to, transpose } => {
                kit::form(ui, "keysig", |ui| {
                    kit::row(ui, "Key", |ui| {
                        egui::ComboBox::from_id_salt("key").selected_text(key_name(*key)).width(200.0).show_ui(ui, |ui| {
                            kit::menu_look(ui);
                            for k in -7..=7i8 {
                                ui.selectable_value(key, k, key_name(k));
                            }
                        });
                    });
                    kit::row(ui, "From measure", |ui| kit::num(ui, from, 1..=nm));
                    kit::row(ui, "Through measure", |ui| kit::num(ui, to, 1..=nm));
                    kit::row(ui, "", |ui| kit::checkbox(ui, transpose, "Transpose the notes to the new key"));
                });
            }
            Dialog::Tempo { bpm, measure, to_measure, mode, to_bpm } => {
                kit::form(ui, "tempo", |ui| {
                    kit::row(ui, "From measure", |ui| {
                        kit::num_with(ui, measure, 0..=nm - 1, 72.0, kit::from1);
                        ui.label("to");
                        kit::num_with(ui, to_measure, 0..=nm - 1, 72.0, kit::from1);
                    });
                    kit::label(ui, "Mode");
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        kit::radio_value(ui, mode, 0, "Set all Tempos to");
                        kit::radio_value(ui, mode, 1, "Change to (% of current values)");
                        kit::radio_value(ui, mode, 2, "Change smoothly from ... to");
                        kit::radio_value(ui, mode, 3, "Add (to all values)");
                    });
                    ui.end_row();
                    kit::row(ui, "Tempo", |ui| {
                        kit::num(ui, bpm, 1..=400);
                        if *mode == 2 {
                            ui.label("to");
                            kit::num(ui, to_bpm, 1..=400);
                        }
                        kit::help(ui, "bpm");
                    });
                });
            }
            Dialog::Barline { kind, times, .. } => {
                kit::form(ui, "barline", |ui| {
                    kit::label(ui, "Type");
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        for b in crate::model::Barline::ALL {
                            kit::radio_value(ui, kind, b, b.name());
                        }
                    });
                    ui.end_row();
                    kit::row(ui, "Play repeat times", |ui| kit::num(ui, times, 2..=8));
                });
            }
            Dialog::Endings { text, .. } => {
                kit::text(ui, "Ending text (e.g. 1.  2.  1, 2.) - leave empty to remove:");
                ui.add(egui::TextEdit::singleline(text).desired_width(320.0));
            }
            Dialog::Coda { text, rehearsal, .. } => {
                kit::form(ui, "coda", |ui| {
                    kit::label(ui, "Phrase");
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for line in [["D.C.", "D.C. al Fine", "D.C. al Coda", "D.S."], ["D.S. al Fine", "D.S. al Coda", "Fine", "To Coda"]] {
                            ui.horizontal(|ui| {
                                for t in line {
                                    if kit::chip(ui, text.as_str() == t, t).clicked() {
                                        *text = t.into();
                                    }
                                }
                            });
                        }
                    });
                    ui.end_row();
                    kit::row(ui, "", |ui| ui.add(egui::TextEdit::singleline(text).desired_width(260.0)));
                    kit::row(ui, "Rehearsal letter:", |ui| ui.add(egui::TextEdit::singleline(rehearsal).desired_width(80.0)));
                });
            }
            Dialog::MeasureNumbers { mode } => {
                kit::radio_value(ui, mode, MeasureNumbers::None, "None");
                kit::radio_value(ui, mode, MeasureNumbers::EverySystem, "At the start of each system");
                kit::radio_value(ui, mode, MeasureNumbers::EveryMeasure, "Every measure");
                kit::radio_value(ui, mode, MeasureNumbers::Every5, "Every 5 measures");
            }
            Dialog::AddMeasures { count, before } => {
                kit::form(ui, "addm", |ui| {
                    kit::row(ui, "Add", |ui| kit::num(ui, count, 1..=500));
                    kit::label(ui, "");
                    ui.vertical(|ui| {
                        kit::radio_value(ui, before, true, "Before the cursor measure");
                        kit::radio_value(ui, before, false, "After the cursor measure");
                    });
                    ui.end_row();
                });
            }
            Dialog::MeasuresPerSystem { n } => {
                kit::form(ui, "mps", |ui| kit::row(ui, "Measures per system", |ui| kit::num(ui, n, 1..=32)));
            }
            Dialog::SystemsPerPage { n } => {
                kit::form(ui, "spp", |ui| kit::row(ui, "Systems per page", |ui| kit::num(ui, n, 1..=16)));
            }
            Dialog::Text(t) => {
                kit::form(ui, "txt", |ui| {
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
                        kit::row(ui, l, |ui| ui.add(egui::TextEdit::singleline(v).desired_width(300.0)));
                    }
                    kit::row(ui, "", |ui| kit::checkbox(ui, &mut t.show_page_numbers, "Page numbers"));
                });
            }
            Dialog::NoteAttributes { velocity, play_len, value, dots, voice, hidden, stem, init } => {
                // a length field that differs among the selected notes says so until it's changed
                let mixed = |ui: &mut Ui, on: bool| {
                    if on {
                        kit::help(ui, "mixed (left as it is)");
                    }
                };
                let (m_len, m_val, m_dots) = (init.mixed[0] && *play_len == init.play_len, init.mixed[1] && *value == init.value, init.mixed[2] && *dots == init.dots);
                kit::form(ui, "attr", |ui| {
                    kit::row(ui, "Velocity", |ui| kit::slider(ui, velocity, 1..=127, int_fmt));
                    kit::row(ui, "Playback length (ticks, 480 = quarter)", |ui| {
                        kit::num_with(ui, play_len, 1..=TPQ * 16, 96.0, |d| d);
                        mixed(ui, m_len);
                    });
                    kit::row(ui, "Note value (0 = double whole ... 8 = 128th)", |ui| {
                        kit::slider(ui, value, 0..=8, int_fmt);
                        mixed(ui, m_val);
                    });
                    kit::row(ui, "Dots", |ui| {
                        kit::slider(ui, dots, 0..=2, int_fmt);
                        mixed(ui, m_dots);
                    });
                    kit::row(ui, "Voice", |ui| kit::slider(ui, voice, 1..=8, int_fmt));
                    kit::row(ui, "", |ui| {
                        kit::radio_value(ui, stem, Stem::Auto, "Stem normal");
                        kit::radio_value(ui, stem, Stem::Up, "Up");
                        kit::radio_value(ui, stem, Stem::Down, "Down");
                    });
                    kit::row(ui, "", |ui| kit::checkbox(ui, hidden, "Hidden (plays but doesn't print)"));
                });
            }
            Dialog::Velocity { mode, value, to } => {
                kit::form(ui, "vel", |ui| {
                    kit::label(ui, "Mode");
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        kit::radio_value(ui, mode, 0, "Set all values to");
                        kit::radio_value(ui, mode, 2, "Change to (% of current values)");
                        kit::radio_value(ui, mode, 3, "Change smoothly from ... to");
                        kit::radio_value(ui, mode, 1, "Add (to all values)");
                    });
                    ui.end_row();
                    kit::row(ui, "Value", |ui| {
                        kit::num(ui, value, -127..=400);
                        if *mode == 3 {
                            ui.label("to");
                            kit::num(ui, to, 1..=127);
                        }
                    });
                });
            }
            Dialog::SpacingDefaults { stem, beam, tie_h, tie_v, ms } => {
                kit::help(ui, "NOTE: \"half space\" means one half the height of a staff space");
                kit::form(ui, "spacing", |ui| {
                    kit::row(ui, "Beam stem length (half spaces)", |ui| kit::num(ui, stem, 1..=16));
                    kit::row(ui, "Beam thickness (% of half space)", |ui| kit::num(ui, beam, 25..=400));
                    kit::row(ui, "Horizontal tie offset (quarter spaces)", |ui| kit::num(ui, tie_h, 0..=8));
                    kit::row(ui, "Vertical tie offset (quarter spaces)", |ui| kit::num(ui, tie_v, 0..=8));
                    kit::row(ui, "Measure spacing (half spaces)", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for v in ms.iter_mut() {
                            kit::num_with(ui, v, 0..=8, 52.0, |d| d);
                        }
                    });
                });
            }
            Dialog::Duration { percent } => {
                kit::form(ui, "dur", |ui| kit::row(ui, "% of written length", |ui| kit::slider(ui, percent, 10..=200, int_fmt)));
            }
            Dialog::Transpose { semitones, octave } => {
                kit::form(ui, "transp", |ui| {
                    kit::row(ui, "Semitones", |ui| kit::slider(ui, semitones, -11..=11, int_fmt));
                    kit::row(ui, "Octaves", |ui| kit::slider(ui, octave, -3..=3, int_fmt));
                });
                kit::help(ui, "Applies to the selected notes, or the selected measures.");
            }
            Dialog::Swing { swing, note } => {
                kit::form(ui, "swing", |ui| {
                    kit::row(ui, "% (50.0 to 75.0)", |ui| kit::slider(ui, swing, 50..=75, int_fmt));
                    kit::row(ui, "Note Type to Swing:", |ui| {
                        for (v, n) in [(1u8, "Whole"), (2, "Half"), (3, "Quarter"), (4, "Eighth"), (5, "16th"), (6, "32nd")] {
                            kit::radio_value(ui, note, v, n);
                        }
                    });
                });
            }
            Dialog::Tuplet { notes, in_time } => {
                kit::text(ui, "Notes entered with the tuplet tool play this many notes in the time of the other number.");
                ui.horizontal(|ui| {
                    kit::num(ui, notes, 2..=15);
                    ui.label("notes in the time of");
                    kit::num(ui, in_time, 1..=15);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (a, b) in [(3u8, 2u8), (2, 3), (5, 4), (6, 4), (7, 4), (7, 8), (9, 8)] {
                        if kit::chip(ui, *notes == a && *in_time == b, &format!("{a}:{b}")).clicked() {
                            *notes = a;
                            *in_time = b;
                        }
                    }
                });
            }
            Dialog::ClickSetup(p) => {
                // the original's Click Settings: a row for the bar click and one for the beat click
                let k = theme::t(ui);
                egui::Frame::new().fill(k.field).stroke(Stroke::new(1.0, k.line)).corner_radius(9.0).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                    egui::Grid::new("clickgrid").num_columns(7).spacing([10.0, 6.0]).show(ui, |ui| {
                        for h in ["", "Port", "Chan", "Pitch", "Vel", "Dur (msec)", "Sound"] {
                            ui.label(egui::RichText::new(h).font(theme::font_semibold(11.5)).color(k.text_2));
                        }
                        ui.end_row();
                        for (label, c) in [("Bar click", &mut p.click_bar), ("Beat click", &mut p.click_beat)] {
                            ui.label(label);
                            egui::ComboBox::from_id_salt(("clickport", label)).width(90.0).selected_text(match c.port { 0 => "A", 1 => "B", _ => "Computer" }).show_ui(ui, |ui| {
                                kit::menu_look(ui);
                                ui.selectable_value(&mut c.port, 0, "A");
                                ui.selectable_value(&mut c.port, 1, "B");
                                ui.selectable_value(&mut c.port, 2, "Computer");
                            });
                            let on_port = c.port < 2;
                            ui.add_enabled_ui(on_port, |ui| kit::num_with(ui, &mut c.chan, 1..=16, 60.0, |d| d));
                            ui.add_enabled_ui(on_port, |ui| kit::num_with(ui, &mut c.pitch, 0..=127, 96.0, |d| d.custom_formatter(|v, _| format!("{} ({})", v as i32, Pitch::from_midi(v as i32, false).name()))));
                            kit::num_with(ui, &mut c.vel, 1..=127, 64.0, |d| d);
                            ui.add_enabled_ui(on_port, |ui| kit::num_with(ui, &mut c.dur_ms, 10..=1000, 72.0, |d| d));
                            ui.add_enabled_ui(on_port && c.chan != 10, |ui| {
                                egui::ComboBox::from_id_salt(("clickprog", label)).width(150.0).selected_text(GM_NAMES[c.program.min(127) as usize]).show_ui(ui, |ui| {
                                    kit::menu_look(ui);
                                    for (k, n) in GM_NAMES.iter().enumerate() {
                                        ui.selectable_value(&mut c.program, k as u8, *n);
                                    }
                                });
                            });
                            ui.end_row();
                        }
                    });
                });
                kit::checkbox(ui, &mut p.click_only_rec, "Only Use Click When Recording");
                kit::checkbox(ui, &mut p.click_on, "Click on (toolbar metronome button)");
                ui.horizontal(|ui| {
                    if kit::btn_inline(ui, "Test click").on_hover_text("Plays one bar of clicks with these settings").clicked() {
                        let draft = p.clone();
                        self.test_click(&draft);
                    }
                    if kit::btn_inline(ui, "Defaults: channel 1, piano voice").clicked() {
                        p.click_bar = crate::prefs::bar_click_ch1();
                        p.click_beat = crate::prefs::beat_click_ch1();
                    }
                    if kit::btn_inline(ui, "Defaults: channel 10, drum metronome").clicked() {
                        p.click_bar = crate::prefs::bar_click();
                        p.click_beat = crate::prefs::beat_click();
                    }
                });
                ui.set_max_width(640.0);
                kit::help(ui, "Port A = the piano, together with the music (when the piano isn't connected the click plays on the computer). Computer = always on the Mac. Channel 10 uses the piano's own drum kit (34 = metronome bell, 33 = metronome click on Yamaha). Other channels use the Sound chosen here - pick a channel the piano's MIDI Receive setting plays. Use Test click to try.");
            }
            Dialog::ClavinovaGuide => {
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    let (out_ok, in_ok) = (
                        self.midi_out.as_ref().map(|m| m.connected[0].clone()).flatten(),
                        self.midi.as_ref().map(|m| m.names.join(", ")).filter(|s| !s.is_empty()),
                    );
                    ui.label(egui::RichText::new("Situação agora").font(theme::font_semibold(13.0)));
                    kit::text(ui, &format!("* Teclado (entrada): {}", in_ok.unwrap_or_else(|| "nenhum encontrado".into())));
                    kit::text(ui, &format!("* Som da partitura (saída, Port A): {}", out_ok.unwrap_or_else(|| "som do computador".into())));
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
                        ui.label(egui::RichText::new(n).font(theme::font_semibold(13.0)));
                        kit::text(ui, t);
                        ui.add_space(4.0);
                    }
                    if kit::btn_inline(ui, "Abrir MIDI Setup").clicked() {
                        self.pending_cmd = Some(Cmd::MidiSetup);
                    }
                });
            }
            Dialog::MidiSetup(p) => {
                // Same content as the original's MIDI Setup: MIDI Out, MIDI In, Sync Source | MIDI Thru, Recording, check
                let outs: Vec<String> = self.midi_out.as_ref().map(|m| m.available.clone()).unwrap_or_default();
                let ins: Vec<String> = self.midi.as_ref().map(|m| m.available.clone()).unwrap_or_default();
                let none_out = "None (built-in sound)";
                if let Some(note) = web_midi_note() {
                    kit::status_line(ui, theme::t(ui).warn, note);
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    ui.vertical(|ui| {
                        ui.set_width(400.0);
                        kit::group(ui, "MIDI Out", |ui| {
                            kit::form(ui, "midi_out", |ui| {
                                for (k, label) in ["Port A", "Port B"].iter().enumerate() {
                                    kit::row(ui, label, |ui| {
                                        let cur = if k == 0 { &mut p.out_port_a } else { &mut p.out_port_b };
                                        egui::ComboBox::from_id_salt(("outp", k)).width(190.0).selected_text(cur.clone().unwrap_or_else(|| none_out.into())).show_ui(ui, |ui| {
                                            kit::menu_look(ui);
                                            ui.selectable_value(cur, None, none_out);
                                            for n in &outs {
                                                ui.selectable_value(cur, Some(n.clone()), n);
                                            }
                                        });
                                        kit::checkbox(ui, &mut p.transmit_sync[k], "Transmit Sync");
                                    });
                                }
                            });
                            kit::checkbox(ui, &mut p.single_channel, "Send everything on channel 1, without changing the piano's voice")
                                .on_hover_text("For pianos that only play channel 1, like a Clavinova in MIDI Accord mode (RECEIVE: CH1 = KEYBOARD, other channels OFF). The click plays as short high notes in the piano's own sound.");
                        });
                        kit::group(ui, "MIDI In", |ui| {
                            kit::form(ui, "midi_in", |ui| {
                                kit::row(ui, "Record Port", |ui| {
                                    egui::ComboBox::from_id_salt("recp").width(190.0).selected_text(p.record_port.clone().unwrap_or_else(|| "All MIDI inputs".into())).show_ui(ui, |ui| {
                                        kit::menu_look(ui);
                                        ui.selectable_value(&mut p.record_port, None, "All MIDI inputs");
                                        for n in &ins {
                                            ui.selectable_value(&mut p.record_port, Some(n.clone()), n);
                                        }
                                    });
                                });
                                kit::row(ui, "Receive Sync Port", |ui| {
                                    // following another device's clock isn't built yet (see Sync Source ▸ External)
                                    ui.add_enabled_ui(false, |ui| {
                                    egui::ComboBox::from_id_salt("syncp").width(190.0).selected_text(p.receive_sync_port.clone().unwrap_or_else(|| "None".into())).show_ui(ui, |ui| {
                                        kit::menu_look(ui);
                                        ui.selectable_value(&mut p.receive_sync_port, None, "None");
                                        for n in &ins {
                                            ui.selectable_value(&mut p.receive_sync_port, Some(n.clone()), n);
                                        }
                                    });
                                    })
                                    .response
                                    .on_disabled_hover_text("Not available yet");
                                });
                            });
                        });
                        kit::group(ui, "Sync Source", |ui| {
                            kit::radio_value(ui, &mut p.sync_external, false, "Internal");
                            ui.add_enabled_ui(false, |ui| kit::radio(ui, p.sync_external, "External (following another device's clock isn't available yet)"));
                        });
                    });
                    ui.vertical(|ui| {
                        ui.set_width(470.0);
                        kit::group(ui, "MIDI Thru", |ui| {
                            kit::checkbox(ui, &mut p.thru_on, "Thru on (what you play on the keyboard sounds through the output)");
                            ui.horizontal(|ui| {
                                kit::radio_value(ui, &mut p.thru_follow_staff, true, "Follow Current Staff");
                                kit::radio_value(ui, &mut p.thru_follow_staff, false, "Always send out:");
                            });
                            ui.add_enabled_ui(!p.thru_follow_staff, |ui| {
                                for (port, name) in [(0u8, "A"), (1u8, "B")] {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        ui.label(egui::RichText::new(name).color(theme::t(ui).text_2));
                                        ui.add_space(6.0);
                                        for ch in 0..16u8 {
                                            let sel = p.thru_port == port && p.thru_channel == ch;
                                            if kit::chip_sized(ui, sel, &format!("{}", ch + 1), 24.0, 22.0).clicked() {
                                                p.thru_port = port;
                                                p.thru_channel = ch;
                                            }
                                        }
                                    });
                                }
                            });
                        });
                        kit::group(ui, "Recording", |ui| {
                            kit::form(ui, "midi_rec", |ui| {
                                kit::row(ui, "Smallest note written", |ui| {
                                    egui::ComboBox::from_id_salt("recq").width(120.0).selected_text(match p.rec_quantize { 4 => "Eighth", 5 => "16th", 6 => "32nd", _ => "Quarter" }).show_ui(ui, |ui| {
                                        kit::menu_look(ui);
                                        for (v, n) in [(3u8, "Quarter"), (4, "Eighth"), (5, "16th"), (6, "32nd")] {
                                            ui.selectable_value(&mut p.rec_quantize, v, n);
                                        }
                                    });
                                });
                                kit::row(ui, "Split point (grand staff)", |ui| kit::slider(ui, &mut p.rec_split, 36..=84, pitch_fmt));
                                kit::row(ui, "Count-in bars", |ui| kit::slider(ui, &mut p.rec_count_in, 0..=2, int_fmt));
                                kit::row(ui, "", |ui| kit::checkbox(ui, &mut p.rec_click, "Click while recording"));
                            });
                        });
                        kit::group(ui, "Check the connection", |ui| {
                            let k = theme::t(ui);
                            if let Some(m) = self.midi_out.as_ref() {
                                for kk in 0..2usize {
                                    let name = if kk == 0 { "A" } else { "B" };
                                    let sent = m.sent[kk].load(std::sync::atomic::Ordering::Relaxed);
                                    match (&m.chosen[kk], &m.connected[kk]) {
                                        (None, _) => kit::status_line(ui, k.text_3, &format!("Port {name}: built-in sound")),
                                        (Some(c), None) => kit::status_line(ui, k.warn, &format!("Port {name}: \"{c}\" not found. Is the piano on and the USB cable in its USB TO HOST port?")),
                                        (Some(_), Some(n)) => kit::status_line(ui, k.ok, &format!("Port {name}: connected to \"{n}\" - {sent} messages sent")),
                                    }
                                }
                            }
                            if let Some(e) = self.midi_out.as_ref().and_then(|m| m.error.clone()).or_else(|| self.midi.as_ref().and_then(|m| m.error.clone())) {
                                ui.colored_label(k.warn, e);
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                                if kit::btn_inline(ui, "Test Port A").on_hover_text("Plays C-E-G-C on channel 1 straight to the piano (press OK first if you just changed Port A)").clicked() {
                                    let ok = self.midi_out.as_ref().map(|m| m.test(0)).unwrap_or(false);
                                    self.status = if ok { "Test notes sent to Port A. The piano should play four notes and its Rx Monitor should blink.".into() } else { "Port A isn't connected yet: choose the piano in Port A and press OK, then test again.".into() };
                                }
                                if kit::btn_inline(ui, "Copy diagnostics").on_hover_text("Copies what the program sees, to paste when asking for help").clicked() {
                                    let mut t = self.midi_out.as_ref().map(|m| m.diagnostics()).unwrap_or_default();
                                    if let Some(mi) = self.midi.as_ref() {
                                        t.push_str(&format!("MIDI inputs found: {:?}\nInputs open: {:?}\nNotes received: {}\nRecording: {}\n", mi.available, mi.names, mi.received.load(std::sync::atomic::Ordering::Relaxed), self.rec.is_some()));
                                    }
                                    t.push_str(&format!("Cursor staff port/channel: {:?}\n", self.score.staves.get(self.cursor.0).map(|s| (s.port, s.channel + 1))));
                                    ui.ctx().copy_text(t);
                                    self.status = "Diagnostics copied - paste them in the chat.".into();
                                }
                                if kit::btn_inline(ui, "Look for devices again").clicked() {
                                    if let Some(m) = self.midi.as_mut() {
                                        m.disconnect_all();
                                        m.last_scan = None;
                                    }
                                    if let Some(m) = self.midi_out.as_mut() {
                                        m.last_scan = None;
                                    }
                                }
                                if kit::btn_inline(ui, "All Notes Off").on_hover_text("Silence anything still sounding on the MIDI outputs").clicked() {
                                    if let Some(m) = self.midi_out.as_mut() {
                                        m.all_notes_off();
                                    }
                                }
                            });
                            kit::help(ui, "If the piano sounds twice when you play it, turn Thru off here.");
                        });
                    });
                });
            }
            Dialog::MidiPlayback => {
                let pb = &mut self.score.playback;
                kit::form(ui, "midipb", |ui| {
                    kit::row(ui, "Default tempo", |ui| kit::slider(ui, &mut pb.default_tempo, 20..=300, int_fmt));
                    kit::row(ui, "Swing", |ui| kit::slider(ui, &mut pb.swing, 50..=75, int_fmt));
                    kit::row(ui, "", |ui| kit::checkbox(ui, &mut pb.click, "Click (metronome)"));
                    kit::row(ui, "", |ui| kit::checkbox(ui, &mut pb.follow, "Follow playback"));
                    kit::row(ui, "Volume", |ui| kit::slider(ui, &mut pb.master_volume, 0.0..=1.0, |v| format!("{:.0}%", v * 100.0)));
                    kit::row(ui, "", |ui| kit::checkbox(ui, &mut pb.humanize, "Humanize (random velocity within each dynamic's range)"));
                });
                kit::section(ui, "dynr_section", "Velocity Ranges", |ui| {
                    egui::Grid::new("dynr").spacing([8.0, 6.0]).show(ui, |ui| {
                        for (i, d) in DYNAMICS.iter().enumerate() {
                            if let Some(r) = pb.dyn_ranges.get_mut(i) {
                                ui.label(*d);
                                kit::num_with(ui, &mut r.0, 1..=127, 64.0, |d| d);
                                ui.label("to");
                                kit::num_with(ui, &mut r.1, 1..=127, 64.0, |d| d);
                                ui.end_row();
                            }
                        }
                    });
                });
                kit::help(ui, "Instruments, channels, volume and pan per staff are in Windows > Staff Sheet.");
            }
            Dialog::JumpTo { measure } => {
                kit::form(ui, "jump", |ui| kit::row(ui, "Measure", |ui| kit::num(ui, measure, 1..=nm)));
            }
            Dialog::PageSetup { w, h, margin, staff_space, staff_gap, system_gap } => {
                kit::form(ui, "page", |ui| {
                    kit::row(ui, "Paper", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let (pw, ph) = (w.min(*h), w.max(*h));
                        if kit::chip(ui, pw == 612.0 && ph == 792.0, "Letter").clicked() {
                            *w = 612.0;
                            *h = 792.0;
                        }
                        if kit::chip(ui, pw == 595.0 && ph == 842.0, "A4").clicked() {
                            *w = 595.0;
                            *h = 842.0;
                        }
                        if kit::chip(ui, *w > *h, "Landscape").clicked() {
                            std::mem::swap(w, h);
                        }
                    });
                    kit::row(ui, "Width (pt)", |ui| kit::num_with(ui, w, 200.0..=2000.0, 88.0, |d| d));
                    kit::row(ui, "Height (pt)", |ui| kit::num_with(ui, h, 200.0..=2000.0, 88.0, |d| d));
                    kit::row(ui, "Margin (pt)", |ui| kit::num_with(ui, margin, 10.0..=150.0, 88.0, |d| d));
                    kit::row(ui, "Staff size", |ui| kit::slider(ui, staff_space, 3.0..=12.0, |v| format!("{v:.1}")));
                    kit::row(ui, "Space between staves", |ui| kit::slider(ui, staff_gap, 4.0..=20.0, |v| format!("{v:.1}")));
                    kit::row(ui, "Space between systems", |ui| kit::slider(ui, system_gap, 4.0..=24.0, |v| format!("{v:.1}")));
                });
            }
            Dialog::AddStaff { name, clef, program } => {
                kit::form(ui, "addstaff", |ui| {
                    kit::row(ui, "Name", |ui| ui.add(egui::TextEdit::singleline(name).desired_width(220.0)));
                    kit::row(ui, "Clef", |ui| {
                        egui::ComboBox::from_id_salt("asclef").selected_text(clef.name()).width(140.0).show_ui(ui, |ui| {
                            kit::menu_look(ui);
                            for c in Clef::ALL {
                                ui.selectable_value(clef, c, c.name());
                            }
                        });
                    });
                    kit::row(ui, "Instrument", |ui| {
                        egui::ComboBox::from_id_salt("asprog").selected_text(GM_NAMES[*program as usize]).width(220.0).show_ui(ui, |ui| {
                            kit::menu_look(ui);
                            for (k, n) in GM_NAMES.iter().enumerate() {
                                ui.selectable_value(program, k as u8, *n);
                            }
                        });
                    });
                });
                kit::help(ui, "The staff is added below the cursor staff.");
            }
            Dialog::SplitStaff { split } => {
                kit::form(ui, "split", |ui| kit::row(ui, "Split point", |ui| kit::slider(ui, split, 36..=84, pitch_fmt)));
                kit::help(ui, "Notes at or above the split stay; lower notes move to a new bass staff.");
            }
            Dialog::ConnectStaves { kind } => {
                kit::text(ui, "Connect the cursor staff with the staff below it:");
                kit::radio_value(ui, kind, 0, "Brace (piano style)");
                kit::radio_value(ui, kind, 1, "Bracket");
                kit::radio_value(ui, kind, 2, "Barlines only");
                kit::radio_value(ui, kind, 3, "Disconnect");
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
                let k = theme::t(ui);
                ui.spacing_mut().item_spacing.y = 6.0;
                let cell = vec2(78.0, 20.0);
                ui.label(egui::RichText::new("Name").font(theme::font_semibold(11.5)).color(k.text_2));
                for (family, rows) in CHORD_GRID.iter() {
                    egui::Grid::new(("chordgrid", *family)).spacing([3.0, 2.0]).min_col_width(cell.x).show(ui, |ui| {
                        for (ri, row) in rows.iter().enumerate() {
                            ui.label(egui::RichText::new(if ri == 0 { *family } else { "" }).font(theme::font_medium(12.5)));
                            for kk in row.iter() {
                                match kk {
                                    Some(kk) => {
                                        let label = if kk.is_empty() { "Major" } else { kk };
                                        let on = custom.is_empty() && kind.as_str() == *kk;
                                        if kit::chip_sized(ui, on, label, cell.x, cell.y).clicked() {
                                            *kind = kk.to_string();
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
                    ui.add_space(2.0);
                }
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(egui::TextEdit::singleline(custom).desired_width(200.0).hint_text("any text"));
                });
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    for (title, val, none) in [("Root", &mut *root, false), ("Bass", &mut *bass, true)] {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(title).font(theme::font_semibold(11.5)).color(k.text_2));
                            for acc in ["#", "", "b"] {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    for l in ["C", "D", "E", "F", "G", "A", "B"] {
                                        let name = format!("{l}{acc}");
                                        let on = *val == name;
                                        if kit::chip_sized(ui, on, &name, 32.0, 20.0).clicked() {
                                            // clicking the chosen bass again removes it
                                            *val = if none && on { String::new() } else { name };
                                        }
                                    }
                                });
                            }
                            if none && kit::chip(ui, val.is_empty(), "No bass note").clicked() {
                                val.clear();
                            }
                        });
                        ui.add_space(24.0);
                    }
                });
                ui.add_space(4.0);
                kit::help(ui, "Keys: a-g root, A-G bass, 3/4 sharp/flat root, #/$ sharp/flat bass, M major, m minor, + aug, o dim, 7, & maj7, 9, 1 = 11, 5, 6");
                let preview = chord_text(root, kind, bass, custom);
                egui::Frame::new().fill(k.accent_soft).corner_radius(7.0).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                    ui.label(egui::RichText::new(format!("Chord: {}", if preview.is_empty() { "(none)".into() } else { preview })).font(theme::font_semibold(16.0)).color(k.text));
                });
            }
            Dialog::TextGraphic { text, size, bold, .. } => {
                ui.add(egui::TextEdit::singleline(text).desired_width(300.0)).request_focus();
                kit::form(ui, "textg", |ui| {
                    kit::row(ui, "Size", |ui| kit::slider(ui, size, 0.8..=5.0, |v| format!("{v:.1}")));
                    kit::row(ui, "", |ui| kit::checkbox(ui, bold, "Bold"));
                });
            }
            Dialog::Lyric { text, line, target } => {
                kit::help(ui, "Space = next note, - = hyphen and next note, Shift+- = melisma (_), Shift+Space = to the tied note,");
                kit::help(ui, "Tab / Shift+Tab = next / previous syllable, Ctrl+Space = space inside the lyric, Ctrl+- = hyphen inside a word.");
                kit::form(ui, "lyricline", |ui| kit::row(ui, "Verse line", |ui| kit::slider(ui, line, 0..=7, |v| format!("{}", v as usize + 1))));
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
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(300.0));
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
                ui.add(egui::TextEdit::singleline(text).desired_width(300.0)).request_focus();
            }
            Dialog::Shortcuts => {
                let k = theme::t(ui);
                let key_glyphs = |s: &str| s.replace("Shift+", "⇧").replace("Ctrl+", "⌃").replace("Alt+", "⌥").replace("Left", "←").replace("Right", "→");
                egui::Frame::new().fill(k.field).stroke(Stroke::new(1.0, k.line)).corner_radius(9.0).inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                        egui::Grid::new("sc").striped(true).min_col_width(140.0).spacing([24.0, 6.0]).show(ui, |ui| {
                            let mut cmds = commands::all_shortcut_cmds();
                            cmds.sort_by_key(|c| commands::label(*c));
                            for c in cmds {
                                ui.label(commands::label(c));
                                ui.label(egui::RichText::new(shortcut_text(c)).color(k.text_2));
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
                                ui.label(egui::RichText::new(key_glyphs(b)).color(k.text_2));
                                ui.end_row();
                            }
                        });
                    });
                });
            }
            // drawn as alerts (see alert_body)
            Dialog::About | Dialog::Message(_) | Dialog::ConfirmClose(_) => {}
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
                // refused as a whole when a measure in the range holds more than the new signature allows
                if let Err((s, m)) = self.set_time_signature(num, den, from, to) {
                    self.warn_time_signature(s, m, num, den);
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
                    // "Set all Tempos to": every measure of the range gets the new tempo, also those with their own mark
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
            Dialog::NoteAttributes { velocity, play_len, value, dots, voice, hidden, stem, init } => {
                // the length fields go to every selected note when they were changed; untouched (or mixed and
                // untouched) fields leave each note as it is. A length that runs past a bar line is refused whole.
                let (set_len, set_val, set_dots) = (play_len != init.play_len, value != init.value, dots != init.dots);
                self.modify(|e| {
                    e.velocity = velocity;
                    e.hidden = hidden;
                    e.stem = stem;
                    e.voice = voice;
                    if set_val {
                        e.value = value;
                    }
                    if set_dots {
                        e.dots = dots;
                    }
                    if set_len {
                        e.play_len = Some(play_len);
                    }
                    if e.play_len == Some(dur_ticks(e.value, e.dots, e.tuplet)) {
                        e.play_len = None;
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


// ---------------- toolbar and status bar pieces ----------------

/// A hover tip: the name, then each key in its own small key cap ("Play  [Space]"). Keys are separated by " · ".
pub fn tip(resp: egui::Response, name: &str, keys: &str) -> egui::Response {
    let name = name.to_string();
    let keys = keys.to_string();
    resp.on_hover_ui(move |ui| {
        let k = theme::t(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            ui.label(egui::RichText::new(&name).font(theme::font_medium(12.0)).color(k.text));
            for key in keys.split(" · ").filter(|s| !s.is_empty()) {
                let galley = ui.painter().layout_no_wrap(key.to_string(), theme::font_medium(11.0), k.text);
                let (r, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 4.0), Sense::hover());
                ui.painter().rect_filled(r, 5.0, k.cell_hover);
                ui.painter().galley(r.center() - galley.size() / 2.0, galley, k.text);
            }
        });
    })
}

/// A red-text button inside a window's content (Delete Cursor Staff).
fn button_danger_inline(ui: &mut Ui, text: &str) -> egui::Response {
    kit::btn_danger(ui, text)
}

/// In the browser, why there may be no MIDI: no Web MIDI (Safari) or the site isn't allowed to use it.
#[cfg(target_arch = "wasm32")]
fn web_midi_note() -> Option<&'static str> {
    match crate::web::midi_status().as_str() {
        "unsupported" => Some("No MIDI in this browser: use Chrome or Edge to connect the piano"),
        "denied" => Some("MIDI is blocked for this page: allow MIDI devices in the browser's site settings"),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn web_midi_note() -> Option<&'static str> {
    None
}

/// The thin divider between toolbar groups.
fn bar_sep(ui: &mut Ui) {
    let k = theme::t(ui);
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, k.line));
    ui.add_space(6.0);
}

/// The thin divider between status bar segments.
fn status_sep(ui: &mut Ui) {
    let k = theme::t(ui);
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 14.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, k.line));
    ui.add_space(4.0);
}

/// A small coloured label in the status bar (QWERTY, REC).
fn status_tag(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::font_semibold(10.5), fg);
    let (r, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(r, 5.0, bg);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, fg);
}

/// A framed box on the toolbar (field colour, thin ring, rounded), hovered a shade darker.
fn bar_box(ui: &mut Ui, w: f32) -> (Rect, egui::Response) {
    let k = theme::t(ui);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let fill = if resp.is_pointer_button_down_on() { theme::blend(k.field, k.cell_press) } else if resp.hovered() { theme::blend(k.field, k.cell) } else { k.field };
    ui.painter().rect(r, 7.0, fill, Stroke::new(1.0, k.line_2), egui::StrokeKind::Inside);
    (r, resp)
}

/// The Voice ▾ box that opens the voice menu.
fn voice_button(ui: &mut Ui, label: &str) -> egui::Response {
    let k = theme::t(ui);
    let (r, resp) = bar_box(ui, 104.0);
    let p = ui.painter();
    // a small eighth note before the label
    let n = pos2(r.left() + 13.0, r.center().y + 3.5);
    p.add(egui::Shape::convex_polygon(ellipse(n, 3.0, 2.2, -0.4), k.text_3, Stroke::NONE));
    p.line_segment([n + vec2(2.6, -0.6), n + vec2(2.6, -9.0)], Stroke::new(1.2, k.text_3));
    p.line_segment([n + vec2(2.6, -9.0), n + vec2(5.6, -5.5)], Stroke::new(1.2, k.text_3));
    p.text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, label, theme::font_medium(13.0), k.text);
    // chevron
    let c = pos2(r.right() - 13.0, r.center().y);
    p.line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], Stroke::new(1.3, k.text_2));
    p.line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], Stroke::new(1.3, k.text_2));
    resp
}

/// The measure indicator: "M" and the cursor (or playing) measure; a click opens Jump to Measure.
fn measure_box(ui: &mut Ui, m: usize) -> egui::Response {
    let k = theme::t(ui);
    let (r, resp) = bar_box(ui, 80.0);
    let p = ui.painter();
    let g1 = p.layout_no_wrap("M".into(), theme::font_medium(12.0), k.text_3);
    let g2 = p.layout_no_wrap(m.to_string(), theme::font_semibold(14.0), k.text);
    let w = g1.size().x + 5.0 + g2.size().x;
    let x = r.center().x - w / 2.0;
    p.galley(pos2(x, r.center().y - g1.size().y / 2.0), g1.clone(), k.text_3);
    p.galley(pos2(x + g1.size().x + 5.0, r.center().y - g2.size().y / 2.0), g2, k.text);
    resp
}

fn ellipse(c: egui::Pos2, rx: f32, ry: f32, rot: f32) -> Vec<egui::Pos2> {
    let (s, co) = rot.sin_cos();
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * std::f32::consts::TAU;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            c + vec2(x * co - y * s, x * s + y * co)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Arrow,
    Eraser,
    Pencil,
    Record,
    Play,
    Stop,
    Metronome,
    PlayFrom,
    ZoomOut,
    ZoomIn,
}

/// A toolbar icon button (30×28). `on` marks the current tool or a running state (playing, recording, click on).
fn icon_button(ui: &mut Ui, icon: Icon, on: bool) -> egui::Response {
    let k = theme::t(ui);
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    let p = ui.painter_at(r.expand(1.0));
    if on {
        p.rect(r, 7.0, k.accent_soft, Stroke::new(1.0, theme::alpha(k.accent, 0.35)), egui::StrokeKind::Inside);
    } else if resp.is_pointer_button_down_on() {
        p.rect_filled(r, 7.0, k.cell_press);
    } else if resp.hovered() {
        p.rect_filled(r, 7.0, k.cell_hover);
    }
    let col = if on { k.accent } else { k.text };
    paint_icon(&p, r.center(), icon, col, k);
    resp
}

/// The toolbar icons, drawn in code (no icon font or image files). `c` is the centre of an 18-point box.
pub fn paint_icon(p: &egui::Painter, c: egui::Pos2, icon: Icon, col: Color32, k: &theme::Tokens) {
    let st = Stroke::new(1.5, col);
    // a point in a box rotated 45° up to the right, for the eraser and the pencil
    let rot = |x: f32, y: f32| {
        let (s, co) = (-std::f32::consts::FRAC_PI_4).sin_cos();
        c + vec2(x * co - y * s, x * s + y * co)
    };
    match icon {
        Icon::Arrow => {
            let o = c + vec2(-1.0, 0.0);
            p.add(egui::Shape::convex_polygon(vec![o + vec2(-4.0, -7.5), o + vec2(-4.0, 5.0), o + vec2(5.0, 1.8)], col, Stroke::NONE));
            p.add(egui::Shape::convex_polygon(vec![o + vec2(-1.2, 2.2), o + vec2(1.2, 1.4), o + vec2(3.6, 6.6), o + vec2(1.4, 7.5)], col, Stroke::NONE));
        }
        Icon::Eraser => {
            let body = vec![rot(-7.0, -3.2), rot(5.5, -3.2), rot(5.5, 3.2), rot(-7.0, 3.2)];
            p.add(egui::Shape::closed_line(body, st));
            p.line_segment([rot(-2.0, -3.2), rot(-2.0, 3.2)], st);
            p.line_segment([c + vec2(-0.5, 7.0), c + vec2(7.0, 7.0)], st);
        }
        Icon::Pencil => {
            let body = vec![rot(-3.5, -2.4), rot(7.5, -2.4), rot(7.5, 2.4), rot(-3.5, 2.4), rot(-8.0, 0.0)];
            p.add(egui::Shape::closed_line(body, st));
            p.line_segment([rot(4.5, -2.4), rot(4.5, 2.4)], st);
        }
        Icon::Record => {
            p.circle_filled(c, 5.5, k.danger);
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-4.0, 6.0)], col, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(10.0, 10.0)), 1.5, col);
        }
        Icon::Metronome => {
            let body = vec![c + vec2(-5.5, 7.0), c + vec2(-2.2, -7.0), c + vec2(2.2, -7.0), c + vec2(5.5, 7.0)];
            p.add(egui::Shape::closed_line(body, st));
            p.line_segment([c + vec2(-4.2, 3.0), c + vec2(4.2, 3.0)], st);
            p.line_segment([c + vec2(0.0, 3.0), c + vec2(4.5, -5.0)], st);
        }
        Icon::PlayFrom => {
            p.rect_filled(Rect::from_min_max(c + vec2(-6.0, -6.0), c + vec2(-4.0, 6.0)), 0.5, col);
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-1.5, -6.0), c + vec2(6.5, 0.0), c + vec2(-1.5, 6.0)], col, Stroke::NONE));
        }
        Icon::ZoomOut | Icon::ZoomIn => {
            let m = c + vec2(-1.5, -1.5);
            p.circle_stroke(m, 5.5, st);
            p.line_segment([m + vec2(4.0, 4.0), m + vec2(8.0, 8.0)], Stroke::new(2.0, col));
            p.line_segment([m + vec2(-2.8, 0.0), m + vec2(2.8, 0.0)], st);
            if icon == Icon::ZoomIn {
                p.line_segment([m + vec2(0.0, -2.8), m + vec2(0.0, 2.8)], st);
            }
        }
    }
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

#[cfg(test)]
mod qa2_tests {
    use super::*;
    use crate::score_view::NoteRef;

    /// Quarter notes at the given ticks of measure 1, staff 1, all selected.
    fn selected_quarters(ticks: &[u32]) -> App {
        let mut a = App::new(true);
        for &t in ticks {
            a.enter(0, 0, t, Some(Pitch::from_midi(60, false)), 3, 0, false);
        }
        for i in 0..a.score.events[0][0].len() {
            a.selected.insert(NoteRef { staff: 0, measure: 0, idx: i });
        }
        a
    }

    fn open_attributes(a: &mut App) -> Dialog {
        a.exec(Cmd::NoteAttributes);
        a.dialog.take().expect("Note Attributes opens")
    }

    #[test]
    fn note_attributes_value_and_dots_apply_to_every_selected_note() {
        let mut a = selected_quarters(&[0, 2 * TPQ]);
        let Dialog::NoteAttributes { velocity, play_len, voice, hidden, stem, init, .. } = open_attributes(&mut a) else { panic!() };
        a.apply_dialog(Dialog::NoteAttributes { velocity, play_len, value: 4, dots: 1, voice, hidden, stem, init });
        let got: Vec<(u8, u8)> = a.score.events[0][0].iter().map(|e| (e.value, e.dots)).collect();
        assert_eq!(got, vec![(4, 1), (4, 1)]);
    }

    #[test]
    fn note_attributes_mixed_fields_left_untouched_change_nothing() {
        let mut a = selected_quarters(&[0, TPQ]);
        a.score.events[0][0][1].value = 4; // a quarter and an eighth: the note value is mixed
        let Dialog::NoteAttributes { play_len, value, dots, voice, hidden, stem, init, .. } = open_attributes(&mut a) else { panic!() };
        assert!(init.mixed[1]);
        a.apply_dialog(Dialog::NoteAttributes { velocity: 33, play_len, value, dots, voice, hidden, stem, init });
        let got: Vec<(u8, u8)> = a.score.events[0][0].iter().map(|e| (e.value, e.velocity)).collect();
        assert_eq!(got, vec![(3, 33), (4, 33)], "values stay as they were; the velocity applies to both");
    }

    #[test]
    fn note_attributes_length_that_doesnt_fit_is_refused_whole() {
        let mut a = selected_quarters(&[0, TPQ]);
        a.score.events[0][0][1].start = 3 * TPQ; // the second quarter on beat 4
        let before = a.score.clone();
        let Dialog::NoteAttributes { velocity, play_len, dots, voice, hidden, stem, init, .. } = open_attributes(&mut a) else { panic!() };
        // half notes: the one on beat 4 would run past the bar line
        a.apply_dialog(Dialog::NoteAttributes { velocity, play_len, value: 2, dots, voice, hidden, stem, init });
        assert_eq!(a.score, before);
        assert!(a.status.starts_with("Doesn't fit"), "{}", a.status);
    }

    #[test]
    fn set_all_tempos_covers_measures_with_their_own_mark() {
        let mut a = App::new(true);
        a.score.measures[2].tempo = Some(90);
        a.apply_dialog(Dialog::Tempo { bpm: 100, measure: 0, to_measure: 3, mode: 0, to_bpm: 100 });
        assert_eq!((0..4).map(|m| a.score.tempo_at(m)).collect::<Vec<_>>(), vec![100, 100, 100, 100]);
    }

    #[test]
    fn default_tempo_drives_a_new_score_until_a_mark_is_entered() {
        let mut a = App::new(true);
        assert_eq!(a.score.measures[0].tempo, None, "a new score has no fixed ♩=120");
        a.score.playback.default_tempo = 60;
        assert_eq!(a.score.tempo_at(0), 60);
        a.score.measures[0].tempo = Some(132);
        assert_eq!(a.score.tempo_at(0), 132);
        // a template from New Score too
        let t = crate::app::build_template(0, 2, 5, 3, false, false);
        assert_eq!(t.measures[0].tempo, None);
    }

    #[test]
    fn unavailable_setup_items_only_say_so() {
        let mut a = App::new(true);
        let before = a.score.clone();
        a.exec(Cmd::AutoGuessBeam);
        assert_eq!(a.status, "Auto Guess/Beam isn't available yet.");
        a.exec(Cmd::AutoSpace);
        assert!(a.status.ends_with("isn't available yet."), "{}", a.status);
        assert_eq!(a.score, before);
    }

    #[test]
    fn tablature_message_is_honest() {
        let mut a = App::new(true);
        a.exec(Cmd::TabStaff);
        assert_eq!(a.status, "Tablature staff added (fret numbers aren't drawn yet).");
    }

    #[test]
    fn older_scores_keep_their_first_measure_tempo() {
        let mut sc = Score::default_score();
        sc.measures[0].tempo = Some(120);
        sc.playback.default_tempo = 80;
        let back: Score = serde_json::from_str(&serde_json::to_string(&sc).unwrap()).unwrap();
        assert_eq!(back.measures[0].tempo, Some(120));
        assert_eq!(back.tempo_at(0), 120);
    }
}
