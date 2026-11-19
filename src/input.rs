//! The score window: drawing, mouse tools and plain-key shortcuts.
use crate::app::*;
use crate::commands::{self, Cmd};
use crate::model::*;
use crate::score_view::{self as sv, DrawState, NoteRef};
use egui::{pos2, vec2, Color32, Key, Rect, Sense, Stroke, Ui};
use std::collections::HashSet;

impl App {
    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.dialog.is_some() {
            return;
        }
        for (s, c) in commands::all_shortcuts() {
            if ctx.input_mut(|i| i.consume_shortcut(&s)) {
                self.exec(c);
                return;
            }
        }
        // plain keys
        let events: Vec<(Key, egui::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        for (key, m) in events {
            if m.command || m.ctrl || m.mac_cmd {
                // Cmd+arrows: select notes left/right of cursor
                if matches!(key, Key::ArrowLeft | Key::ArrowRight) {
                    self.select_neighbour(key == Key::ArrowRight, m.shift, m.alt);
                }
                continue;
            }
            self.plain_key(key, m.shift, m.alt);
        }
    }

    fn plain_key(&mut self, key: Key, shift: bool, _alt: bool) {
        if self.voice_prefix {
            self.voice_prefix = false;
            let n = match key {
                Key::Num1 => Some(1),
                Key::Num2 => Some(2),
                Key::Num3 => Some(3),
                Key::Num4 => Some(4),
                Key::Num5 => Some(5),
                Key::Num6 => Some(6),
                Key::Num7 => Some(7),
                Key::Num8 => Some(8),
                _ => None,
            };
            if let Some(n) = n {
                self.show_voice = Some(n);
                self.voice = n;
                self.status = format!("Showing voice {n}.");
            } else if matches!(key, Key::A | Key::Minus) {
                self.show_voice = None;
                self.status = "Showing all voices.".into();
            }
            return;
        }
        let num = |k: Key| -> Option<u8> {
            Some(match k {
                Key::Num0 => 0,
                Key::Num1 => 1,
                Key::Num2 => 2,
                Key::Num3 => 3,
                Key::Num4 => 4,
                Key::Num5 => 5,
                Key::Num6 => 6,
                Key::Num7 => 7,
                Key::Num8 => 8,
                _ => return None,
            })
        };
        if let Some(v) = num(key) {
            if shift {
                // change note value without changing playback
                self.modify(|e| {
                    let pl = e.play_len.unwrap_or(e.ticks());
                    e.value = v;
                    e.play_len = Some(pl);
                });
            } else {
                let rest = matches!(self.item, Item::Rest(_));
                self.pick_item(if rest { Item::Rest(v) } else { Item::Note(v) });
                if !self.selected.is_empty() && !self.qwerty {
                    self.modify(|e| e.value = v);
                }
            }
            return;
        }
        if self.qwerty {
            let semis = match key {
                Key::A => Some(0),
                Key::W => Some(1),
                Key::S => Some(2),
                Key::E => Some(3),
                Key::D => Some(4),
                Key::F => Some(5),
                Key::T => Some(6),
                Key::G => Some(7),
                Key::Y => Some(8),
                Key::H => Some(9),
                Key::U => Some(10),
                Key::J => Some(11),
                Key::K => Some(12),
                _ => None,
            };
            if let Some(sm) = semis {
                // nearest octave to the last entered note
                let last = self.last_pitch.midi();
                let mut best = 60 + sm;
                for o in -4..=4 {
                    let c = 60 + o * 12 + sm;
                    if (c - last).abs() < (best - last).abs() {
                        best = c;
                    }
                }
                let flats = self.score.measures[self.cursor.1].key < 0;
                let (s, m, t) = self.cursor;
                let (v, d) = (self.cur_value, self.cur_dots);
                self.enter(s, m, t, Some(Pitch::from_midi(best, flats)), v, d, true);
                return;
            }
            match key {
                Key::Comma => {
                    let (s, m, t) = self.cursor;
                    let (v, d) = (self.cur_value, self.cur_dots);
                    self.enter(s, m, t, None, v, d, true);
                    return;
                }
                Key::Period => {
                    self.cur_dots = if shift { 2 } else if self.cur_dots == 1 { 0 } else { 1 };
                    self.status = format!("{} dot(s) for the next note.", self.cur_dots);
                    return;
                }
                Key::Slash => {
                    if shift {
                        self.tie_selection();
                    } else {
                        self.cur_tuplet = !self.cur_tuplet;
                    }
                    return;
                }
                Key::Z | Key::X => {
                    self.last_pitch.step += if key == Key::X { 7 } else { -7 };
                    self.status = format!("Octave {}.", self.last_pitch.step.div_euclid(7));
                    return;
                }
                _ => {}
            }
        }
        match key {
            Key::Q => self.exec(Cmd::QwertyMode),
            Key::A | Key::P => {
                self.tool = if self.tool == Tool::Arrow { Tool::Pencil } else { Tool::Arrow };
            }
            Key::Escape => {
                self.tool = Tool::Arrow;
                self.selected.clear();
                self.sel_graphics.clear();
                self.sel_measures = None;
            }
            Key::E => self.tool = Tool::Eraser,
            Key::Space => self.exec(Cmd::Play),
            Key::Enter => self.exec(Cmd::Record),
            Key::M => self.exec(Cmd::JumpToMeasure),
            Key::Z => self.exec(if shift { Cmd::ZoomOut } else { Cmd::ZoomIn }),
            Key::V => self.voice_prefix = true,
            Key::R => {
                let v = self.cur_value;
                self.pick_item(Item::Rest(v));
            }
            Key::S | Key::F | Key::N => {
                let alter: i8 = match (key, shift) {
                    (Key::S, false) => 1,
                    (Key::S, true) => 2,
                    (Key::F, false) => -1,
                    (Key::F, true) => -2,
                    _ => 0,
                };
                if key == Key::N && shift {
                    self.modify(|e| {
                        for c in e.courtesy.iter_mut() {
                            *c = !*c;
                        }
                    });
                } else if !self.selected.is_empty() {
                    self.modify(|e| {
                        for p in e.pitches.iter_mut() {
                            p.alter = alter;
                        }
                    });
                    self.audition_selection();
                } else {
                    self.pick_item(Item::Accidental(alter));
                }
            }
            Key::D => self.pick_item(Item::Dot(if shift { 2 } else { 1 })),
            Key::T => self.pick_item(Item::Tuplet),
            Key::Delete | Key::Backspace => {
                if self.selected.is_empty() && self.sel_graphics.is_empty() && self.sel_measures.is_none() {
                    // delete note left of cursor
                    let (s, m, t) = self.cursor;
                    let voice = self.voice;
                    if let Some((i, e)) = self.score.events[s][m].iter().enumerate().filter(|(_, e)| e.start < t && e.voice == voice).max_by_key(|(_, e)| e.start) {
                        let st = e.start;
                        let r = NoteRef { staff: s, measure: m, idx: i };
                        self.delete_refs(&[r]);
                        self.cursor.2 = st;
                    }
                } else {
                    self.exec(Cmd::Clear);
                }
            }
            Key::ArrowLeft => {
                self.cursor.1 = self.cursor.1.saturating_sub(1);
                self.cursor.2 = 0;
                self.scroll_to_cursor = true;
            }
            Key::ArrowRight => {
                self.cursor.1 = (self.cursor.1 + 1).min(self.score.measures.len() - 1);
                self.cursor.2 = 0;
                self.scroll_to_cursor = true;
            }
            Key::ArrowUp => self.cursor.0 = self.cursor.0.saturating_sub(1),
            Key::ArrowDown => self.cursor.0 = (self.cursor.0 + 1).min(self.score.staves.len() - 1),
            Key::Home => {
                self.cursor = (self.cursor.0, 0, 0);
                self.scroll_to_cursor = true;
            }
            Key::End => {
                self.cursor = (self.cursor.0, self.score.measures.len() - 1, 0);
                self.scroll_to_cursor = true;
            }
            Key::OpenBracket => self.exec(Cmd::LessMeasure),
            Key::CloseBracket => self.exec(Cmd::MoreMeasure),
            _ => {}
        }
    }

    fn select_neighbour(&mut self, right: bool, add: bool, measure: bool) {
        let (s, m, t) = self.cursor;
        if !add {
            self.selected.clear();
        }
        if measure {
            let mm = if right { (m + 1).min(self.score.measures.len() - 1) } else { m.saturating_sub(1) };
            for i in 0..self.score.events[s][mm].len() {
                self.selected.insert(NoteRef { staff: s, measure: mm, idx: i });
            }
            self.cursor = (s, mm, 0);
            return;
        }
        let evs = &self.score.events[s][m];
        let pick = if right {
            evs.iter().enumerate().filter(|(_, e)| e.start >= t).min_by_key(|(_, e)| e.start)
        } else {
            evs.iter().enumerate().filter(|(_, e)| e.start < t).max_by_key(|(_, e)| e.start)
        };
        if let Some((i, e)) = pick {
            let st = e.start;
            let end = e.end();
            self.selected.insert(NoteRef { staff: s, measure: m, idx: i });
            self.cursor.2 = if right { end.min(self.score.measures[m].ticks() - 1) } else { st };
            self.audition_selection();
        }
    }

    /// The scrolling page area with the score.
    pub fn score_area(&mut self, ui: &mut Ui) {
        let play = self.play_position();
        let reader = self.device == crate::touch::Device::Reader;
        let fingers = self.device != crate::touch::Device::Desktop;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            // the phone's reader: the page fits the width until it is pinched (F-25)
            if reader && !self.reader_user_zoom {
                self.zoom = ((ui.clip_rect().width() - 16.0) / self.score.layout.page_w).clamp(0.3, 5.0);
            }
            let any_left = !fingers && !self.floating_hidden && self.palettes.iter().enumerate().any(|(i, o)| *o && [0usize, 1, 2, 3, 10].contains(&i));
            // room for the palettes on the left; with space to spare the page sits in the middle of the window
            // (on a tablet the palettes have their own column; the phone shows only the page)
            let left = if any_left { if self.palettes[1] || self.palettes[2] || self.palettes[3] { 196.0 } else { 104.0 } } else if reader { 8.0 } else { 24.0 };
            let page_w = self.score.layout.page_w * self.zoom;
            let centred = if self.linear { left } else { ((ui.clip_rect().width() - page_w) / 2.0).max(left) };
            let origin = ui.min_rect().min + vec2(centred, if reader { 8.0 } else { 16.0 });
            let geo = sv::layout(&self.score, origin, self.zoom, self.linear);
            // the scrollable area reaches the right edge of the page (plus a margin), wherever the page sits
            let size = vec2(centred + geo.size.x - geo.sp * 4.0 + 24.0, geo.size.y + 32.0);
            // the reader only taps (to choose where playback starts): one finger scrolls the page
            let (rect, resp) = ui.allocate_exact_size(size, if reader { Sense::click() } else { Sense::click_and_drag() });
            // two fingers: scroll and zoom (the same zoom as Z and Shift+Z), keeping the point between the fingers
            // in place; they never write
            if fingers {
                if let Some(mt) = ui.input(|i| i.multi_touch()) {
                    let old = self.zoom;
                    let new = (old * mt.zoom_delta).clamp(0.3, 5.0);
                    if (new - old).abs() > 1e-4 {
                        self.zoom = new;
                        self.reader_user_zoom |= reader;
                        let rel = mt.center_pos - ui.min_rect().min;
                        ui.scroll_with_delta(-rel * (new / old - 1.0));
                    }
                    // in the reader the page already follows the first finger
                    if !reader {
                        ui.scroll_with_delta(mt.translation_delta);
                    }
                    ui.ctx().request_repaint();
                }
            }
            let painter = ui.painter_at(rect.expand(2000.0).intersect(ui.clip_rect()));
            // follow playback / cursor scrolling: scroll once when the playing system changes, and only if that
            // system isn't already in view (asking every frame made the last page bounce up and down)
            let following = matches!((play, self.score.playback.follow || reader), (Some(_), true));
            if !following && play.is_none() {
                self.follow_sys = None;
            }
            let target = if following { play.map(|p| p.0) } else if self.scroll_to_cursor { Some(self.cursor.1) } else { None };
            if let Some(tm) = target {
                if let Some((si, sys)) = geo.systems.iter().enumerate().find(|(_, s)| tm >= s.m0 && tm < s.m1) {
                    let changed = !following || self.follow_sys != Some(si);
                    if changed {
                        self.follow_sys = if following { Some(si) } else { None };
                        if let Some(mg) = sys.measures.iter().find(|x| x.covers(tm)) {
                            let r = Rect::from_min_max(pos2(mg.x0, sys.top(geo.sp) - geo.sp * 4.0), pos2(mg.x1, sys.bottom(geo.sp) + geo.sp * 4.0));
                            let clip = ui.clip_rect();
                            let visible = r.top() >= clip.top() && r.bottom() <= clip.bottom() && r.left() >= clip.left() && r.right() <= clip.right();
                            if !visible {
                                // top-align so a scroll at the end of the score lands once and stays put
                                ui.scroll_to_rect(r, Some(egui::Align::TOP));
                            }
                        }
                    }
                }
                self.scroll_to_cursor = false;
            }
            if let Some(r) = self.scroll_to.take() {
                ui.scroll_to_rect(r, Some(egui::Align::TOP));
            }

            let pointer = resp.hover_pos();
            // F-37: over the score, the Eraser and the Pencil are the pointer (a mouse; a finger has no hover)
            let tool_pointer = pointer.filter(|_| !reader && !crate::kit::touch() && self.drag.is_none() && matches!(self.tool, Tool::Eraser | Tool::Pencil));
            let ghost = if !reader && self.tool == Tool::Pencil && matches!(self.item, Item::Note(_) | Item::Rest(_)) && self.drag.is_none() {
                pointer.and_then(|p| sv::locate(&self.score, &geo, p, self.grid())).map(|mut l| {
                    let rest = matches!(self.item, Item::Rest(_));
                    let tuplet = self.cur_tuplet_opt();
                    let joins = !rest && self.score.events[l.staff][l.measure].iter().any(|e| e.voice == self.voice && e.start == l.tick && !e.is_rest() && !e.grace);
                    let len = dur_ticks(self.cur_value, self.cur_dots, tuplet);
                    // show the note where it will really go: on the beat pointed at, after a note it points inside, or
                    // moved back before the bar line (F-36)
                    let placed = if joins { Some(l.tick) } else { self.pencil_tick(l.staff, l.measure, l.tick, len) };
                    if !joins {
                        if let Some(t) = placed {
                            if t != l.tick {
                                l.tick = t;
                                let mt = self.score.measures[l.measure].ticks();
                                if let Some(mg) = geo.systems.iter().flat_map(|s| s.measures.iter()).find(|mg| mg.covers(l.measure)) {
                                    l.x = sv::x_for_tick(mg, t, mt);
                                }
                            }
                        }
                    }
                    let fits = placed.is_some();
                    (l, self.cur_value, rest, fits)
                })
            } else {
                None
            };
            // F-36: the insertion cursor blinks like a text cursor (0.5 s on, 0.5 s off) and is back on with every click
            // or key; it stays on while playing or recording, in screenshots and when the window isn't in front
            let now = ui.input(|i| i.time);
            if ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::PointerButton { .. } | egui::Event::Text(_)))) {
                self.cursor_since = now;
            }
            let steady = self.screenshot_mode || play.is_some() || self.rec.is_some() || !ui.input(|i| i.focused);
            let phase = (now - self.cursor_since).rem_euclid(1.0);
            let cursor_on = steady || phase < 0.5;
            if !steady && !reader {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(0.5 - phase.rem_euclid(0.5) + 0.01));
            }
            let mut hits = vec![];
            let mut ghits = vec![];
            {
                let st = DrawState {
                    selected: &self.selected,
                    sel_graphics: &self.sel_graphics,
                    // the reader has no insertion cursor
                    cursor: if reader || !cursor_on { None } else { Some(self.cursor) },
                    play_tick: play,
                    show_voice: self.show_voice,
                    show_control_points: self.control_points,
                    show_hidden: self.show_hidden,
                    sel_measures: self.sel_measures,
                    hover_ghost: ghost,
                    flash: self.warn.as_ref().and_then(|(_, t, at)| {
                        let age = t.elapsed().as_secs_f32();
                        at.filter(|_| age < 2.5).map(|(s, m)| (s, m, (1.0 - age / 2.5).max(0.0)))
                    }),
                    erase: tool_pointer.filter(|_| self.tool == Tool::Eraser).and_then(|p| self.eraser_target(p)),
                };
                sv::draw(&painter, &self.score, &geo, &st, &mut hits, &mut ghits);
            }
            if let Some(p) = tool_pointer {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                let top = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("tool-pointer")));
                crate::ui::paint_tool_pointer(&top, p, if self.tool == Tool::Eraser { crate::ui::Icon::Eraser } else { crate::ui::Icon::Pencil });
            }
            if let Some(Drag::Rubber { start }) = &self.drag {
                if let Some(p) = pointer.or(ui.ctx().pointer_latest_pos()) {
                    let r = Rect::from_two_pos(*start, p);
                    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(60, 120, 255, 30));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, sv::SEL), egui::StrokeKind::Inside);
                }
            }
            if let Some(Drag::NewGraphic { start, .. }) = &self.drag {
                if let Some(p) = pointer.or(ui.ctx().pointer_latest_pos()) {
                    painter.line_segment([*start, p], Stroke::new(1.0, sv::SEL));
                }
            }
            let mut geo = geo;
            geo.notes = hits;
            geo.graphics = ghits;
            self.geo = geo;
            if reader {
                // a tap chooses the measure playback starts from (nothing is edited)
                if resp.clicked() {
                    if let Some(l) = resp.interact_pointer_pos().and_then(|p| sv::locate(&self.score, &self.geo, p, self.grid())) {
                        self.cursor = (l.staff, l.measure, 0);
                        self.reader_pos = l.measure;
                    }
                }
            } else if self.device == crate::touch::Device::Touch {
                self.touch_mouse(ui, &resp, &painter);
            } else {
                self.mouse(ui, &resp);
            }
        });
    }

    /// The tablet: the tool follows one finger; a second finger turns the press into scrolling and zooming, and
    /// when it lands within 120 ms it undoes what the first finger started, without an undo step (F-25).
    fn touch_mouse(&mut self, ui: &mut Ui, resp: &egui::Response, painter: &egui::Painter) {
        // the most fingers on the screen at any moment of this frame (several touch events can arrive in one frame)
        let (mut fingers, pressed, now) = (self.touch_ids.len(), ui.input(|i| i.pointer.primary_pressed()), ui.input(|i| i.time));
        // (a release frame still counts the fingers it had, so the gesture resets only on the next frame)
        ui.input(|i| {
            for e in &i.events {
                if let egui::Event::Touch { id, phase, .. } = e {
                    match phase {
                        egui::TouchPhase::Start => {
                            self.touch_ids.insert(id.0);
                        }
                        egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                            self.touch_ids.remove(&id.0);
                        }
                        egui::TouchPhase::Move => {}
                    }
                    fingers = fingers.max(self.touch_ids.len());
                }
            }
        });
        if pressed && !self.gesture.multi {
            self.gesture_undo = Some((self.score.clone(), self.undo.len()));
        }
        let (act, blocked) = self.gesture.step(now, pressed, fingers);
        match act {
            crate::touch::FingerAct::Cancel => {
                if let Some((sc, n)) = self.gesture_undo.take() {
                    self.score = sc;
                    self.undo.truncate(n);
                }
                self.drag = None;
            }
            crate::touch::FingerAct::Stop => self.drag = None,
            crate::touch::FingerAct::Normal => {}
        }
        if blocked {
            return;
        }
        // with the pencil, the finger may slide to the right line or space before lifting: the note goes where
        // it lifts (a slide is a drag, not a tap)
        let slide_note = self.tool == Tool::Pencil && matches!(self.item, Item::Note(_) | Item::Rest(_));
        if slide_note && resp.drag_stopped() && self.drag.is_none() {
            if let Some(p) = resp.interact_pointer_pos().or(ui.ctx().pointer_latest_pos()) {
                let mods = ui.input(|i| i.modifiers);
                self.click(p, mods);
            }
        }
        // the long press (a right click) shows a ring filling up under the finger
        let ring = ui.input(|i| {
            let p = &i.pointer;
            match (p.press_start_time(), p.press_origin(), p.latest_pos()) {
                (Some(t0), Some(o), Some(at)) if p.primary_down() && o.distance(at) < 6.0 => Some((at, ((i.time - t0) / crate::touch::LONG_PRESS) as f32)),
                _ => None,
            }
        });
        if let Some((at, f)) = ring {
            if resp.rect.contains(at) && f > 0.2 && f < 1.0 {
                let k = crate::theme::t(ui);
                let r = 26.0;
                painter.circle_stroke(at, r, Stroke::new(3.0, crate::theme::alpha(k.accent, 0.25)));
                let n = 32;
                let pts: Vec<egui::Pos2> = (0..=n).map(|j| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * f * j as f32 / n as f32;
                    at + vec2(a.cos(), a.sin()) * r
                }).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(3.0, k.accent)));
                ui.ctx().request_repaint();
            }
        }
        self.mouse(ui, resp);
    }

    fn hit_note(&self, p: egui::Pos2) -> Option<sv::NoteHit> {
        self.geo.notes.iter().rev().find(|h| h.rect.expand(2.0).contains(p)).cloned()
    }
    fn hit_graphic(&self, p: egui::Pos2) -> Option<usize> {
        self.geo.graphics.iter().rev().find(|(_, r)| r.expand(3.0).contains(p)).map(|x| x.0)
    }

    /// F-37: what a click of the Eraser at `p` removes: a symbol first, else the note or rest (one note of a chord
    /// when the pointer is on that notehead). The click and the red highlight both ask this.
    pub fn eraser_target(&self, p: egui::Pos2) -> Option<sv::EraseHover> {
        if let Some(gi) = self.hit_graphic(p) {
            return Some(sv::EraseHover::Graphic(gi));
        }
        let h = self.hit_note(p)?;
        let chord = self.ev(h.r).map(|e| e.pitches.len() > 1).unwrap_or(false);
        Some(match h.pitch {
            Some(k) if chord => sv::EraseHover::Pitch(h.r, k),
            k => sv::EraseHover::Event(h.r, k),
        })
    }

    fn mouse(&mut self, ui: &mut Ui, resp: &egui::Response) {
        let mods = ui.input(|i| i.modifiers);
        let grid = self.grid();
        // right click: audition pitch under pointer
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some(l) = sv::locate(&self.score, &self.geo, p, grid) {
                    let key = self.score.measures[l.measure].key;
                    let pitch = Pitch { step: l.step, alter: key_alter(key, l.step.rem_euclid(7) as usize) };
                    self.sound_note(l.staff, pitch.midi(), 90);
                    self.status = format!("{} (MIDI {})", pitch.name(), pitch.midi());
                }
            }
        }
        if resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some(gi) = self.hit_graphic(p) {
                    let gr = &self.score.graphics[gi];
                    if gr.kind == GraphicKind::Text {
                        self.dialog = Some(Dialog::TextGraphic { text: gr.text.clone(), index: gi, size: if gr.size > 0.0 { gr.size } else { 1.6 }, bold: gr.bold });
                    } else {
                        self.checkpoint();
                        self.score.graphics[gi].flip = !self.score.graphics[gi].flip;
                    }
                    return;
                }
                if let Some(h) = self.hit_note(p) {
                    if self.tool == Tool::Arrow {
                        self.selected.clear();
                        self.selected.insert(h.r);
                        self.exec(Cmd::NoteAttributes);
                        return;
                    }
                }
            }
        }
        if resp.drag_started() {
            let Some(p) = resp.interact_pointer_pos() else { return };
            match (&self.tool, self.item.clone()) {
                (Tool::Pencil, Item::Graphic(k)) if k != GraphicKind::Text => {
                    self.drag = Some(Drag::NewGraphic { kind: k, start: p });
                }
                (Tool::Arrow, _) => {
                    if let Some(gi) = self.hit_graphic(p) {
                        let (dx0, dy0) = (self.score.graphics[gi].dx, self.score.graphics[gi].dy);
                        self.checkpoint();
                        self.sel_graphics.clear();
                        self.sel_graphics.insert(gi);
                        self.drag = Some(Drag::Graphic { index: gi, start: p, dx0, dy0, end: false });
                    } else if let Some(h) = self.hit_note(p) {
                        if !self.selected.contains(&h.r) {
                            if !mods.shift {
                                self.selected.clear();
                            }
                            self.selected.insert(h.r);
                        }
                        let step0 = sv::locate(&self.score, &self.geo, p, grid).map(|l| l.pos).unwrap_or(0);
                        let refs: Vec<NoteRef> = self.selected.iter().copied().collect();
                        let dx0 = refs.iter().map(|r| self.ev(*r).map(|e| e.dx).unwrap_or(0.0)).collect();
                        self.drag = Some(Drag::Note { start: p, refs, step0, moved: false, axis: 0, dx0 });
                    } else {
                        if !mods.shift {
                            self.selected.clear();
                            self.sel_graphics.clear();
                        }
                        self.drag = Some(Drag::Rubber { start: p });
                    }
                }
                _ => {}
            }
        }
        if resp.dragged() {
            if let (Some(p), Some(d)) = (resp.interact_pointer_pos(), self.drag.clone()) {
                match d {
                    Drag::Graphic { index, start, dx0, dy0, .. } => {
                        let sp = self.geo.sp.max(1.0);
                        if let Some(gr) = self.score.graphics.get_mut(index) {
                            let ddy = (p.y - start.y) / sp;
                            gr.end_dy += dy0 + ddy - gr.dy;
                            gr.dx = dx0 + (p.x - start.x) / sp;
                            gr.dy = dy0 + ddy;
                        }
                    }
                    Drag::Note { start, refs, step0, moved, axis, dx0 } => {
                        // the first few pixels decide: up/down changes the pitch, left/right moves the note sideways
                        let d = p - start;
                        let axis = if axis == 0 && d.length() > 4.0 { if d.x.abs() > d.y.abs() { 2 } else { 1 } } else { axis };
                        if axis == 2 {
                            if !moved {
                                self.checkpoint();
                            }
                            let sp = self.geo.sp.max(1.0);
                            for (k, r) in refs.iter().enumerate() {
                                let base = dx0.get(k).copied().unwrap_or(0.0);
                                if let Some(e) = self.ev_mut(*r) {
                                    e.dx = (base + d.x / sp).clamp(-12.0, 12.0);
                                }
                            }
                            self.dirty = true;
                            self.status = "Note moved sideways (Align Spacing puts it back in place).".into();
                            self.drag = Some(Drag::Note { start, refs, step0, moved: true, axis, dx0 });
                        } else {
                            let now = sv::locate(&self.score, &self.geo, p, grid).map(|l| l.pos).unwrap_or(step0);
                            let delta = now - step0;
                            if axis == 1 && delta != 0 {
                                if !moved {
                                    self.checkpoint();
                                }
                                let key = self.score.measures[self.cursor.1].key;
                                for r in &refs {
                                    if let Some(e) = self.ev_mut(*r) {
                                        for pp in e.pitches.iter_mut() {
                                            pp.step += delta;
                                            pp.alter = key_alter(key, pp.letter());
                                        }
                                    }
                                }
                                self.drag = Some(Drag::Note { start, refs, step0: now, moved: true, axis, dx0 });
                                self.audition_selection();
                            } else {
                                self.drag = Some(Drag::Note { start, refs, step0, moved, axis, dx0 });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.drag_stopped() {
            let end = resp.interact_pointer_pos().or(ui.ctx().pointer_latest_pos());
            match (self.drag.take(), end) {
                (Some(Drag::Rubber { start }), Some(e)) => {
                    let r = Rect::from_two_pos(start, e);
                    for h in &self.geo.notes {
                        if r.intersects(h.rect) {
                            self.selected.insert(h.r);
                        }
                    }
                    for (gi, gr) in &self.geo.graphics {
                        if r.contains_rect(*gr) {
                            self.sel_graphics.insert(*gi);
                        }
                    }
                    // a box with no notes across measures selects those measures
                    if self.selected.is_empty() {
                        let a = sv::locate(&self.score, &self.geo, start, grid);
                        let b = sv::locate(&self.score, &self.geo, e, grid);
                        if let (Some(a), Some(b)) = (a, b) {
                            self.sel_measures = Some((a.measure.min(b.measure), a.measure.max(b.measure)));
                        }
                    } else {
                        self.sel_measures = None;
                    }
                    self.status = format!("{} note(s) selected.", self.selected.len());
                }
                (Some(Drag::NewGraphic { kind, start }), Some(e)) => {
                    let a = sv::locate(&self.score, &self.geo, start, grid);
                    let b = sv::locate(&self.score, &self.geo, e, grid);
                    if let (Some(a), Some(b)) = (a, b) {
                        let sp = self.geo.sp.max(1.0);
                        let sys_top = |l: &sv::Locate| -> f32 {
                            self.geo.systems.iter().find_map(|s| if l.measure >= s.m0 && l.measure < s.m1 { s.staff_y(a.staff) } else { None }).unwrap_or(0.0) - 2.0 * sp
                        };
                        let dy = (start.y - sys_top(&a)) / sp;
                        let edy = (e.y - sys_top(&a)) / sp;
                        let edy = if b.measure == a.measure || matches!(kind, GraphicKind::Crescendo | GraphicKind::Decrescendo | GraphicKind::Ottava | GraphicKind::OttavaBassa | GraphicKind::TrillLine) { if matches!(kind, GraphicKind::Line | GraphicKind::Slur | GraphicKind::Rect | GraphicKind::RoundRect | GraphicKind::Ellipse) { edy } else { dy } } else { edy };
                        let (am, at, bm, bt) = if (b.measure, b.tick) >= (a.measure, a.tick) { (a.measure, a.tick, b.measure, b.tick) } else { (b.measure, b.tick, a.measure, a.tick) };
                        let gi = self.add_graphic(kind, a.staff, am, at, dy, bm, bt.max(if bm == am { at + 1 } else { 0 }), edy, String::new());
                        if mods.shift {
                            self.score.graphics[gi].flip = true;
                        }
                        self.status = format!("{kind:?} added.");
                    }
                }
                _ => {}
            }
        }
        if resp.clicked() {
            let Some(p) = resp.interact_pointer_pos() else { return };
            self.click(p, mods);
        }
    }

    fn click(&mut self, p: egui::Pos2, mods: egui::Modifiers) {
        let grid = self.grid();
        let loc = sv::locate(&self.score, &self.geo, p, grid);
        let hit = self.hit_note(p);
        match self.tool {
            Tool::Arrow => {
                if let Some(gi) = self.hit_graphic(p) {
                    if !mods.shift {
                        self.sel_graphics.clear();
                        self.selected.clear();
                    }
                    self.sel_graphics.insert(gi);
                    return;
                }
                if let Some(h) = hit {
                    if mods.shift {
                        if !self.selected.remove(&h.r) {
                            self.selected.insert(h.r);
                        }
                    } else {
                        self.selected.clear();
                        self.sel_graphics.clear();
                        self.selected.insert(h.r);
                    }
                    self.sel_measures = None;
                    if let Some(e) = self.ev(h.r).cloned() {
                        self.cursor = (h.r.staff, h.r.measure, e.start);
                        let name: Vec<String> = e.pitches.iter().map(|p| p.name()).collect();
                        self.status = if name.is_empty() { "Rest selected.".into() } else { format!("{} - velocity {}, voice {}", name.join(" "), e.velocity, e.voice) };
                    }
                    self.audition_selection();
                } else if let Some(l) = loc {
                    if mods.shift {
                        let (a, b) = (self.cursor.1.min(l.measure), self.cursor.1.max(l.measure));
                        self.sel_measures = Some((a, b));
                    } else {
                        self.selected.clear();
                        self.sel_graphics.clear();
                        self.sel_measures = None;
                        self.cursor = (l.staff, l.measure, l.tick);
                    }
                }
            }
            Tool::Eraser => match self.eraser_target(p) {
                Some(sv::EraseHover::Graphic(gi)) => {
                    self.checkpoint();
                    self.score.graphics.remove(gi);
                    self.sel_graphics.clear();
                }
                Some(sv::EraseHover::Pitch(r, k)) | Some(sv::EraseHover::Event(r, Some(k))) => self.delete_pitch(r, k),
                Some(sv::EraseHover::Event(r, None)) => self.delete_refs(&[r]),
                None => {}
            },
            Tool::Pencil => {
                let item = self.item.clone();
                match item {
                    Item::Note(v) | Item::Rest(v) => {
                        let Some(l) = loc else { return };
                        let rest = matches!(item, Item::Rest(_));
                        let pitch = if rest {
                            None
                        } else {
                            let key = self.score.measures[l.measure].key;
                            let mut alter = key_alter(key, l.step.rem_euclid(7) as usize);
                            // an accidental of the same line earlier in the bar carries through
                            if let Some(prev) = self.score.events[l.staff][l.measure].iter().filter(|e| e.start < l.tick).flat_map(|e| e.pitches.iter()).filter(|q| q.step == l.step).last() {
                                alter = prev.alter;
                            }
                            Some(Pitch { step: l.step, alter })
                        };
                        let d = self.cur_dots;
                        let r = self.enter_clicked(l.staff, l.measure, l.tick, pitch, v, d);
                        if mods.alt && !rest {
                            if let Some(e) = self.ev_mut(r) {
                                let n = e.courtesy.len();
                                if n > 0 {
                                    e.courtesy[n - 1] = true;
                                }
                            }
                        }
                    }
                    Item::Accidental(a) => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(h.r) {
                                let k = h.pitch.unwrap_or(0);
                                if let Some(pp) = e.pitches.get_mut(k) {
                                    pp.alter = a;
                                }
                            }
                            self.audition_selection();
                        }
                    }
                    Item::Courtesy | Item::Parens => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(h.r) {
                                let k = h.pitch.unwrap_or(0);
                                if let Some(c) = e.courtesy.get_mut(k) {
                                    *c = !*c;
                                }
                            }
                        }
                    }
                    Item::Dot(d) => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(h.r) {
                                e.dots = if e.dots == d { 0 } else { d };
                            }
                            self.revert_if_overflow(&[(h.r.staff, h.r.measure)]);
                        }
                    }
                    Item::Tuplet => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            let (s, m) = (h.r.staff, h.r.measure);
                            let e0 = self.ev(h.r).cloned().unwrap();
                            let mut t = e0.start;
                            let evs = &mut self.score.events[s][m];
                            let mut idxs: Vec<usize> = (0..evs.len()).filter(|&i| evs[i].voice == e0.voice && evs[i].start >= e0.start).collect();
                            idxs.sort_by_key(|&i| evs[i].start);
                            for &i in idxs.iter().take(3) {
                                evs[i].tuplet = Some((3, 2));
                                evs[i].start = t;
                                t += evs[i].ticks();
                            }
                        }
                    }
                    Item::Clef(c) => {
                        if let Some(l) = loc {
                            self.checkpoint();
                            self.score.staves[l.staff].set_clef(l.measure, c);
                        }
                    }
                    Item::Mark(mk) => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(h.r) {
                                if let Some(i) = e.marks.iter().position(|x| *x == mk) {
                                    e.marks.remove(i);
                                } else {
                                    e.marks.push(mk);
                                }
                            }
                        }
                    }
                    Item::Dynamic(d) => {
                        if let Some(r) = hit.map(|h| h.r).or_else(|| loc.and_then(|l| self.nearest_event(l.staff, l.measure, l.tick))) {
                            self.checkpoint();
                            let vel = dyn_velocity_in(&self.score.playback, &d);
                            if let Some(e) = self.ev_mut(r) {
                                e.dynamic = if e.dynamic.as_deref() == Some(d.as_str()) { None } else { Some(d.clone()) };
                            }
                            // following notes of the staff take the new loudness
                            self.apply_dynamic_velocity(r, vel);
                        }
                    }
                    Item::Expression(t) | Item::Guitar(t) => {
                        if let Some(r) = hit.map(|h| h.r).or_else(|| loc.and_then(|l| self.nearest_event(l.staff, l.measure, l.tick))) {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(r) {
                                e.expression = if e.expression.as_deref() == Some(t.as_str()) { None } else { Some(t.clone()) };
                            }
                        }
                    }
                    Item::Color(c) => {
                        if let Some(h) = hit {
                            self.checkpoint();
                            if let Some(e) = self.ev_mut(h.r) {
                                e.color = c;
                            }
                        }
                    }
                    Item::Graphic(GraphicKind::Text) => {
                        if let Some(l) = loc {
                            let sp = self.geo.sp.max(1.0);
                            let top = self.geo.systems.iter().find_map(|s| if l.measure >= s.m0 && l.measure < s.m1 { s.staff_y(l.staff) } else { None }).unwrap_or(0.0) - 2.0 * sp;
                            let gi = self.add_graphic(GraphicKind::Text, l.staff, l.measure, l.tick, (p.y - top) / sp, l.measure, l.tick, (p.y - top) / sp, String::new());
                            self.dialog = Some(Dialog::TextGraphic { text: String::new(), index: gi, size: 1.6, bold: false });
                        }
                    }
                    Item::Graphic(_) => {
                        self.status = "Drag on the score to draw this.".into();
                    }
                    Item::Lyric => {
                        if let Some(h) = hit {
                            let line = self.lyric_line;
                            let text = self.ev(h.r).and_then(|e| e.lyric.get(line).cloned()).unwrap_or_default();
                            self.dialog = Some(Dialog::Lyric { target: h.r, line, text });
                        }
                    }
                    Item::Chord | Item::ChordGrid => {
                        if let Some(r) = hit.map(|h| h.r).or_else(|| loc.and_then(|l| self.nearest_event(l.staff, l.measure, l.tick))) {
                            let text = self.ev(r).and_then(|e| e.chord_symbol.clone()).unwrap_or_default();
                            self.dialog = Some(match crate::app::split_chord(&text) {
                                Some((root, kind, bass)) => Dialog::ChordSymbol { root, kind, bass, custom: String::new(), target: r },
                                None => Dialog::ChordSymbol { root: "C".into(), kind: String::new(), bass: String::new(), custom: text, target: r },
                            });
                        }
                    }
                    Item::TempoTool => {
                        if let Some(l) = loc {
                            self.dialog = Some(Dialog::Tempo { bpm: self.score.tempo_at(l.measure), measure: l.measure, to_measure: l.measure, mode: 0, to_bpm: self.score.tempo_at(l.measure) });
                        }
                    }
                    Item::MidiTool => {
                        if let Some(h) = hit {
                            self.selected.clear();
                            self.selected.insert(h.r);
                            self.exec(Cmd::NoteAttributes);
                        }
                    }
                }
            }
        }
    }

    pub fn nearest_event(&self, s: usize, m: usize, t: u32) -> Option<NoteRef> {
        self.score.events[s][m]
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| (e.start as i64 - t as i64).abs())
            .map(|(i, _)| NoteRef { staff: s, measure: m, idx: i })
    }

    fn apply_dynamic_velocity(&mut self, r: NoteRef, vel: u8) {
        let start = self.ev(r).map(|e| e.start).unwrap_or(0);
        'outer: for m in r.measure..self.score.measures.len() {
            for e in self.score.events[r.staff][m].iter_mut() {
                if m == r.measure && e.start < start {
                    continue;
                }
                if (m > r.measure || e.start > start) && e.dynamic.is_some() {
                    break 'outer;
                }
                e.velocity = vel;
            }
        }
    }

    pub fn _unused(&self) -> HashSet<u8> {
        HashSet::new()
    }
}

/// Velocity for a dynamic mark: the middle of its range, or a random value in it with Humanize (the rule in the
/// original's Dynamics help text). Unknown marks keep mf.
pub fn dyn_velocity_in(pb: &PlaybackSettings, d: &str) -> u8 {
    let i = DYNAMICS.iter().position(|x| *x == d).unwrap_or(13);
    let (lo, hi) = pb.dyn_ranges.get(i).copied().unwrap_or((72, 88));
    if pb.humanize && hi > lo {
        let r = crate::clock::subsec_nanos();
        lo + (r % (hi - lo + 1) as u32) as u8
    } else {
        ((lo as u16 + hi as u16) / 2) as u8
    }
}

/// F-37: what the Eraser shows in red is what its click removes.
#[cfg(test)]
mod eraser_pointer_tests {
    use super::*;

    /// A bar with a note, a three-note chord and a rest, laid out and drawn as the score area does (so the hit areas
    /// the Eraser uses are filled).
    fn drawn() -> App {
        let mut a = App::new(true);
        a.voice = 1;
        a.enter_clicked(0, 0, 0, Some(Pitch::from_midi(60, false)), 3, 0);
        for n in [60, 64, 67] {
            a.enter_clicked(0, 0, TPQ, Some(Pitch::from_midi(n, false)), 3, 0);
        }
        a.enter_clicked(0, 0, 2 * TPQ, None, 3, 0);
        a.selected.clear();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut geo = sv::layout(&a.score, egui::Pos2::ZERO, 1.0, false);
        let mut hits = vec![];
        let mut ghits = vec![];
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
            let (sel, selg) = (HashSet::new(), HashSet::new());
            let st = DrawState { selected: &sel, sel_graphics: &selg, cursor: None, play_tick: None, show_voice: None, show_control_points: false, show_hidden: false, sel_measures: None, hover_ghost: None, flash: None, erase: None };
            sv::draw(&p, &a.score, &geo, &st, &mut hits, &mut ghits);
        });
        out.textures_delta.clear();
        geo.notes = hits;
        geo.graphics = ghits;
        a.geo = geo;
        a.tool = Tool::Eraser;
        a
    }

    #[test]
    fn the_red_target_is_what_the_click_erases() {
        let a = drawn();
        let mut points: Vec<egui::Pos2> = a.geo.notes.iter().map(|h| h.rect.center()).collect();
        points.push(egui::pos2(-500.0, -500.0)); // nothing there
        let mut kinds = HashSet::new();
        for p in points {
            let target = a.eraser_target(p);
            let mut b = drawn();
            let before = b.score.events[0][0].clone();
            b.click(p, egui::Modifiers::default());
            let after = &b.score.events[0][0];
            match target {
                None => assert_eq!(&before, after, "no target: nothing erased"),
                Some(sv::EraseHover::Pitch(r, k)) => {
                    kinds.insert("pitch");
                    let gone = before[r.idx].pitches[k];
                    assert_eq!(after.len(), before.len(), "one note of the chord, not the chord");
                    assert!(!after[r.idx].pitches.contains(&gone));
                    assert_eq!(after[r.idx].pitches.len(), before[r.idx].pitches.len() - 1);
                }
                Some(sv::EraseHover::Event(r, _)) => {
                    kinds.insert(if before[r.idx].is_rest() { "rest" } else { "note" });
                    assert_eq!(after.len(), before.len() - 1);
                    assert!(!after.iter().any(|e| e.start == before[r.idx].start && e.voice == before[r.idx].voice), "the event under the pointer is gone");
                }
                Some(sv::EraseHover::Graphic(_)) => kinds.insert("graphic").then_some(()).unwrap_or(()),
            }
        }
        assert!(kinds.contains("note") && kinds.contains("pitch") && kinds.contains("rest"), "{kinds:?}");
    }
}
