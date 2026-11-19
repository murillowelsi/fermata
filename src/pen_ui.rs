//! F-38: the pen on the score (web on tablets): ink, the guide while writing, reading the strokes after the pause
//! (`pen::DEBOUNCE`), writing the note as the Pencil's click does (F-36), the short transition, "Not recognized", the
//! alternatives under a new note, and the gesture sheet. The recognizer itself is `pen.rs`.
use crate::app::App;
use crate::model::*;
use crate::pen::{self, Shape, Stroke};
use crate::score_view::{self as sv, NoteRef};
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke as EStroke, Ui};

/// Ink: the same blue in both themes (the page is always white).
pub const INK: Color32 = Color32::from_rgb(0x1F, 0x6F, 0xEA);
pub const INK_WIDTH: f32 = 2.4;

#[derive(Clone, Debug, PartialEq)]
pub enum FeedbackKind {
    /// Read and written: the ink shrinks into the note at `head` with a short glow.
    Recognized { head: Pos2 },
    /// Not read: grey ink and the balloon.
    Unrecognized,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub strokes: Vec<Vec<Pos2>>,
    pub kind: FeedbackKind,
    pub start: f64,
}

/// The figures offered under a new note for 3 s.
#[derive(Clone, Debug, PartialEq)]
pub struct Alternatives {
    pub staff: usize,
    pub measure: usize,
    pub tick: u32,
    pub pitch: Option<Pitch>,
    /// 1 whole, 2 half, 3 quarter, 4 eighth, 0 quarter rest
    pub value: u8,
    pub at: Pos2,
    pub until: f64,
}

/// An accidental written alone: (alteration, its right edge, staff middle y, when, its ink).
pub type LoneAccidental = (i8, f32, f32, f64, Vec<Vec<Pos2>>);
/// Two-finger tap: (when the second finger came, where each finger started, how far they moved).
pub type TwoTap = (f64, Vec<(u64, Pos2)>, f32);

#[derive(Clone, Debug, Default)]
pub struct PenState {
    /// The stroke being drawn.
    pub cur: Option<Stroke>,
    /// Strokes waiting for the pause to end (one symbol).
    pub pending: Vec<Stroke>,
    pub last_up: f64,
    pub feedback: Vec<Feedback>,
    pub alts: Option<Alternatives>,
    /// Where the alternatives were drawn last frame (a pen or finger there picks one, it doesn't write).
    pub alts_rect: Option<Rect>,
    /// An accidental written alone: (alteration, its right edge, staff middle y, when, its ink).
    pub acc: Option<LoneAccidental>,
    /// "Not recognized" balloon: where, until when.
    pub balloon: Option<(Pos2, f64)>,
    pub sheet: bool,
    /// A pen was used on this device (from then on, the finger scrolls).
    pub pen_seen: bool,
    /// Two-finger tap: when the second finger came, where the fingers started, how far they moved.
    pub two_tap: Option<TwoTap>,
    pub max_fingers: usize,
}

/// Value → the grid the Pencil uses for it (as `App::grid` does for the palette's value).
fn grid_for(value: u8) -> u32 {
    dur_ticks(value, 0, None).min(base_ticks(4)).max(1)
}

impl App {
    /// A pen sample: phase 0 down, 1 move, 2 up; position in screen points; time in seconds.
    pub fn pen_feed(&mut self, phase: u8, p: Pos2, t: f64) {
        match phase {
            0 => {
                // a stroke on the alternatives picks one; it never writes
                if self.pen.alts_rect.is_some_and(|r| r.contains(p)) && self.pen.alts.is_some() {
                    return;
                }
                // the next note: the alternatives of the last one go
                if self.pen.pending.is_empty() {
                    self.pen.alts = None;
                }
                self.pen.balloon = None;
                self.pen.cur = Some(Stroke { pts: vec![p], t0: t, t1: t });
            }
            1 => {
                if let Some(s) = self.pen.cur.as_mut() {
                    if s.pts.last().map(|q| q.distance(p) >= 0.6).unwrap_or(true) {
                        s.pts.push(p);
                    }
                    s.t1 = t;
                }
            }
            _ => {
                if let Some(mut s) = self.pen.cur.take() {
                    s.pts.push(p);
                    s.t1 = t;
                    self.pen.pending.push(s);
                    self.pen.last_up = t;
                }
            }
        }
    }

    /// Drops the stroke being drawn (a second finger came: it was a scroll, not writing).
    pub fn pen_cancel_stroke(&mut self) {
        self.pen.cur = None;
    }

    /// Reads the waiting strokes once the pen has stayed up `pen::DEBOUNCE` seconds; drops a lone accidental that
    /// waited too long for its head; ends the timed feedback.
    pub fn pen_tick(&mut self, now: f64) {
        if self.pen.cur.is_none() && !self.pen.pending.is_empty() && now - self.pen.last_up >= pen::DEBOUNCE {
            let strokes: Vec<Vec<Pos2>> = std::mem::take(&mut self.pen.pending).into_iter().map(|s| s.pts).collect();
            self.pen_apply(strokes, now);
        }
        if let Some((_, _, _, t, _)) = &self.pen.acc {
            if now - t > pen::ACCIDENTAL_WAIT && self.pen.pending.is_empty() && self.pen.cur.is_none() {
                let (_, _, _, _, ink) = self.pen.acc.take().unwrap();
                self.pen_unrecognized(ink, now);
            }
        }
        self.pen.feedback.retain(|f| match f.kind {
            FeedbackKind::Recognized { .. } => now - f.start < pen::RECOGNIZED_FADE + 0.15,
            FeedbackKind::Unrecognized => now - f.start < pen::UNRECOGNIZED_SHOW,
        });
        if self.pen.balloon.is_some_and(|(_, until)| now > until) {
            self.pen.balloon = None;
        }
        if self.pen.alts.as_ref().is_some_and(|a| now > a.until) {
            self.pen.alts = None;
        }
    }

    fn pen_unrecognized(&mut self, strokes: Vec<Vec<Pos2>>, now: f64) {
        let all: Vec<Pos2> = strokes.iter().flatten().copied().collect();
        let mut r = Rect::NOTHING;
        for p in &all {
            r.extend_with(*p);
        }
        self.pen.balloon = Some((pos2(r.center().x, r.bottom() + 14.0), now + pen::UNRECOGNIZED_SHOW));
        self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Unrecognized, start: now });
        self.status = "Not recognized. Nothing was added.".into();
    }

    /// The staff whose middle line is nearest to `y` in the system around `p`, with that middle line.
    fn pen_staff_at(&self, p: Pos2) -> Option<(usize, f32)> {
        let sp = self.geo.sp;
        let sys = self.geo.systems.iter().find(|s| p.x >= s.left - sp * 2.0 && p.x <= s.right + sp && p.y >= s.top(sp) - sp * 8.0 && p.y <= s.bottom(sp) + sp * 8.0)?;
        sys.staves.iter().min_by(|a, b| (a.1 - p.y).abs().partial_cmp(&(b.1 - p.y).abs()).unwrap()).copied()
    }

    /// Reads one symbol and acts on it.
    pub fn pen_apply(&mut self, strokes: Vec<Vec<Pos2>>, now: f64) {
        let sp = self.geo.sp;
        match pen::recognize(&strokes, sp) {
            Shape::Note { value, head, acc } => {
                // an accidental written just before, a little to the left, on the same staff
                let mut acc = acc;
                let mut ink = strokes.clone();
                if acc.is_none() {
                    if let (Some((alter, right, mid, t, acc_ink)), Some((_, smid))) = (self.pen.acc.clone(), self.pen_staff_at(head)) {
                        if now - t <= pen::ACCIDENTAL_WAIT + pen::DEBOUNCE && (mid - smid).abs() < 1.0 && right <= head.x && head.x - right <= pen::ACCIDENTAL_REACH * sp + sp {
                            acc = Some(alter);
                            ink.extend(acc_ink);
                            self.pen.acc = None;
                        }
                    }
                }
                match self.pen_write(Some(value), head, acc) {
                    Some(a) => {
                        self.pen.feedback.push(Feedback { strokes: ink, kind: FeedbackKind::Recognized { head }, start: now });
                        self.pen.alts = Some(Alternatives { until: now + pen::ALTERNATIVES_SHOW, ..a });
                    }
                    None => self.pen_unrecognized(ink, now),
                }
            }
            Shape::Rest { at } => match self.pen_write(None, at, None) {
                Some(a) => {
                    self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Recognized { head: at }, start: now });
                    self.pen.alts = Some(Alternatives { until: now + pen::ALTERNATIVES_SHOW, ..a });
                }
                None => self.pen_unrecognized(strokes, now),
            },
            Shape::Accidental { alter, at, right } => {
                // it waits for its head (the ink stays)
                let mid = self.pen_staff_at(at).map(|s| s.1).unwrap_or(at.y);
                self.pen.acc = Some((alter, right, mid, now, strokes));
            }
            Shape::Scribble { rect } => {
                if !self.pen_scribble(rect) {
                    self.pen_unrecognized(strokes, now);
                } else {
                    self.pen.feedback.push(Feedback { strokes, kind: FeedbackKind::Recognized { head: rect.center() }, start: now });
                }
            }
            Shape::Unknown => self.pen_unrecognized(strokes, now),
        }
    }

    /// Writes a note (`value` Some) or a quarter rest at `head` as the Pencil's click would (F-36): the pitch from the
    /// line or space, the beat from x on the grid; a head within `pen::CHORD_DX` of a note of the voice joins its
    /// chord. Returns what was written (for the alternatives), or None when nothing could be written.
    pub fn pen_write(&mut self, value: Option<u8>, head: Pos2, acc: Option<i8>) -> Option<Alternatives> {
        let sp = self.geo.sp;
        let v = value.unwrap_or(3);
        let l = sv::locate(&self.score, &self.geo, head, grid_for(v))?;
        // the chord: a note of this voice drawn at about the same x
        let voice = self.voice;
        let mut tick = l.tick;
        if value.is_some() {
            let near = self.geo.notes.iter().filter(|h| h.r.staff == l.staff && h.r.measure == l.measure && h.pitch.is_some() && (h.rect.center().x - head.x).abs() <= pen::CHORD_DX * sp).filter_map(|h| self.score.events[l.staff][l.measure].get(h.r.idx)).find(|e| e.voice == voice && !e.grace && !e.is_rest()).map(|e| e.start);
            if let Some(t) = near {
                tick = t;
            }
        }
        let pitch = value.map(|_| {
            let key = self.score.measures[l.measure].key;
            let mut alter = key_alter(key, l.step.rem_euclid(7) as usize);
            // an accidental earlier in the bar on the same line carries through
            if let Some(prev) = self.score.events[l.staff][l.measure].iter().filter(|e| e.start < tick).flat_map(|e| e.pitches.iter()).rfind(|q| q.step == l.step) {
                alter = prev.alter;
            }
            Pitch { step: l.step, alter: acc.unwrap_or(alter) }
        });
        let (old_v, old_d) = (self.cur_value, self.cur_dots);
        self.cur_value = v;
        self.cur_dots = 0;
        let r = self.enter_clicked(l.staff, l.measure, tick, pitch, v, 0);
        self.cur_value = old_v;
        self.cur_dots = old_d;
        if r.idx == usize::MAX {
            return None;
        }
        if acc.is_some() {
            if let (Some(e), Some(p)) = (self.ev_mut(r), pitch) {
                // a written accidental is shown even when the key would imply it
                if let Some(k) = e.pitches.iter().position(|q| *q == p) {
                    if k < e.courtesy.len() {
                        e.courtesy[k] = true;
                    }
                }
            }
        }
        let start = self.ev(r).map(|e| e.start).unwrap_or(tick);
        let what = match pitch {
            Some(p) => format!("{} {}", Shape::value_name(v), p.name()),
            None => "quarter rest".into(),
        };
        self.status = format!("Wrote a {what}. Two-finger tap undoes it.");
        Some(Alternatives { staff: l.staff, measure: l.measure, tick: start, pitch, value: value.unwrap_or(0), at: head, until: 0.0 })
    }

    /// A scribble over notes deletes them (one head of a chord: just that note). False when it covers none.
    pub fn pen_scribble(&mut self, rect: Rect) -> bool {
        let covered: Vec<sv::NoteHit> = self
            .geo
            .notes
            .iter()
            .filter(|h| {
                let r = h.rect;
                let w = (rect.right().min(r.right()) - rect.left().max(r.left())).max(0.0);
                let tall = rect.bottom().min(r.bottom()) > rect.top().max(r.top());
                tall && w >= pen::SCRIBBLE_COVER * r.width()
            })
            .cloned()
            .collect();
        if covered.is_empty() {
            return false;
        }
        let mut events: Vec<NoteRef> = covered.iter().map(|h| h.r).collect();
        events.sort_by_key(|r| (r.staff, r.measure, r.idx));
        events.dedup();
        if events.len() == 1 && covered.len() == 1 {
            let h = &covered[0];
            let chord = self.ev(h.r).map(|e| e.pitches.len() > 1).unwrap_or(false);
            match h.pitch {
                Some(k) if chord => self.delete_pitch(h.r, k),
                _ => self.delete_refs(&[h.r]),
            }
        } else {
            self.delete_refs(&events);
        }
        self.status = "Deleted with a scribble. Two-finger tap undoes it.".into();
        true
    }

    /// One of the alternatives: undo the note just written and write this figure (0 = quarter rest) instead.
    pub fn pen_pick_alternative(&mut self, value: u8) {
        let Some(a) = self.pen.alts.clone() else { return };
        self.do_undo();
        self.pen.alts = None;
        if value == 0 {
            let _ = self.pen_write(None, a.at, None);
        } else if let Some(p) = a.pitch {
            let (old_v, old_d) = (self.cur_value, self.cur_dots);
            self.cur_value = value;
            self.cur_dots = 0;
            self.enter_clicked(a.staff, a.measure, a.tick, Some(p), value, 0);
            self.cur_value = old_v;
            self.cur_dots = old_d;
            self.status = format!("Changed to a {} {}.", Shape::value_name(value), p.name());
        } else {
            // a rest changed into a note: the pitch from where it was written
            let _ = self.pen_write(Some(value), a.at, None);
        }
    }

    // ---------------- drawing ----------------

    /// The ink, the guide while writing, the transition, "Not recognized" and the alternatives.
    pub fn pen_draw(&mut self, ui: &mut Ui, painter: &egui::Painter, now: f64) {
        let sp = self.geo.sp;
        let ink = |painter: &egui::Painter, pts: &[Pos2], col: Color32| {
            if pts.len() == 1 {
                painter.circle_filled(pts[0], INK_WIDTH / 2.0, col);
            } else {
                painter.add(egui::Shape::line(pts.to_vec(), EStroke::new(INK_WIDTH, col)));
            }
        };
        // the guide while the pen writes: the beats of the measure, the target beat, the line or space, and the label
        // (the beat and pitch of the symbol come from its first stroke, usually the head, not from the pen's tip:
        // drawing the stem upward must not move the reading)
        if let Some(s) = &self.pen.cur {
            let first = self.pen.pending.first().map(|f| &f.pts).unwrap_or(&s.pts);
            let mut r = Rect::NOTHING;
            for q in first {
                r.extend_with(*q);
            }
            if r.is_positive() || first.len() == 1 {
                self.pen_guide(painter, if first.len() == 1 { first[0] } else { r.center() });
            }
        }
        for f in &self.pen.feedback {
            let age = (now - f.start) as f32;
            match &f.kind {
                FeedbackKind::Recognized { head } => {
                    // shrink toward the note and fade, with a short blue glow on the note
                    let t = (age / pen::RECOGNIZED_FADE as f32).clamp(0.0, 1.0);
                    let k = 1.0 - 0.5 * t;
                    let a = ((1.0 - t) * 255.0) as u8;
                    for s in &f.strokes {
                        let pts: Vec<Pos2> = s.iter().map(|p| *head + (*p - *head) * k).collect();
                        ink(painter, &pts, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), a));
                    }
                    let glow = (1.0 - (age / (pen::RECOGNIZED_FADE as f32 + 0.15)).clamp(0.0, 1.0)) * 70.0;
                    painter.circle_filled(*head, sp * 1.3, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), glow as u8));
                }
                FeedbackKind::Unrecognized => {
                    let fade = (1.0 - ((age - 1.6) / 0.4).clamp(0.0, 1.0)) * 255.0;
                    for s in &f.strokes {
                        ink(painter, s, Color32::from_rgba_unmultiplied(150, 150, 155, fade as u8));
                    }
                }
            }
        }
        // ink still waiting (a lone accidental, the strokes of this symbol, and the stroke being drawn)
        if let Some((_, _, _, _, a)) = &self.pen.acc {
            for s in a {
                ink(painter, s, INK);
            }
        }
        for s in &self.pen.pending {
            ink(painter, &s.pts, INK);
        }
        if let Some(s) = &self.pen.cur {
            ink(painter, &s.pts, INK);
        }
        if let Some((at, _)) = self.pen.balloon {
            self.pen_balloon(ui, at);
        }
        self.pen_alternatives(ui);
        if !self.pen.feedback.is_empty() || self.pen.cur.is_some() || !self.pen.pending.is_empty() || self.pen.alts.is_some() || self.pen.balloon.is_some() || self.pen.acc.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn pen_guide(&self, painter: &egui::Painter, p: Pos2) {
        let sp = self.geo.sp;
        let Some(l) = sv::locate(&self.score, &self.geo, p, self.grid()) else { return };
        let Some(sys) = self.geo.systems.iter().find(|s| s.measures.iter().any(|g| g.covers(l.measure)) && s.staff_y(l.staff).is_some()) else { return };
        let Some(mg) = sys.measures.iter().find(|g| g.covers(l.measure)) else { return };
        let mid = sys.staff_y(l.staff).unwrap_or(p.y);
        let mi = &self.score.measures[l.measure];
        let mt = mi.ticks();
        let beat = (TPQ * 4 / mi.den.max(1) as u32).max(1);
        let (top, bot) = (mid - sp * 3.0, mid + sp * 3.0);
        let faint = Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 70);
        let mut b = 0;
        while b < mt {
            let x = sv::x_for_tick(mg, b, mt);
            painter.extend(egui::Shape::dashed_line(&[pos2(x, top), pos2(x, bot)], EStroke::new(1.0, faint), 3.0, 3.0));
            b += beat;
        }
        // the target beat and the line or space
        let x = l.x;
        painter.rect_filled(Rect::from_center_size(pos2(x, mid), vec2(sp * 1.6, sp * 6.0)), 3.0, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 22));
        let y = mid - l.pos as f32 * sp * 0.5;
        painter.rect_filled(Rect::from_min_max(pos2(mg.content_x, y - sp * 0.25), pos2(mg.x1, y + sp * 0.25)), 0.0, Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), 26));
        // "G4 · beat 3"
        let clef = self.score.staves[l.staff].clef_at(l.measure);
        let key = mi.key;
        let pitch = Pitch { step: clef.middle_step() + l.pos, alter: key_alter(key, (clef.middle_step() + l.pos).rem_euclid(7) as usize) };
        let label = format!("{} · beat {}", pitch.name(), self.beat_of(l.measure, l.tick));
        let g = painter.layout_no_wrap(label, theme::font_semibold(13.0), Color32::WHITE);
        let r = Rect::from_min_size(pos2(x + sp * 1.6, top - 4.0), g.size() + vec2(18.0, 8.0));
        painter.rect_filled(r, r.height() / 2.0, INK);
        painter.galley(r.min + vec2(9.0, 4.0), g, Color32::WHITE);
    }

    fn pen_balloon(&mut self, ui: &mut Ui, at: Pos2) {
        let k = theme::t(ui);
        let mut open = false;
        egui::Area::new(egui::Id::unique("pen-balloon")).order(egui::Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(12, 7)).shadow(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(35) }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("? Not recognized ·").font(theme::font(14.0)).color(k.text));
                    if ui.add(egui::Label::new(egui::RichText::new("See gestures").font(theme::font(14.0)).color(k.accent)).sense(Sense::click())).clicked() {
                        open = true;
                    }
                });
            });
        });
        if open {
            self.pen.sheet = true;
            self.pen.balloon = None;
        }
    }

    fn pen_alternatives(&mut self, ui: &mut Ui) {
        let Some(a) = self.pen.alts.clone() else {
            self.pen.alts_rect = None;
            return;
        };
        let k = theme::t(ui);
        let sp = self.geo.sp;
        let at = pos2(a.at.x, a.at.y + sp * 3.0);
        let mut pick = None;
        let r = egui::Area::new(egui::Id::unique("pen-alternatives")).order(egui::Order::Foreground).pivot(Align2::CENTER_TOP).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(12.0).inner_margin(6.0).shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(40) }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for v in [3u8, 2, 1, 4, 0] {
                        let on = v == a.value;
                        let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                        if on {
                            ui.painter().rect_filled(r, 9.0, k.accent);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 9.0, k.cell_hover);
                        }
                        figure(ui.painter(), r.center(), v, if on { Color32::WHITE } else { k.text });
                        if resp.clicked() && !on {
                            pick = Some(v);
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "↶", theme::font(20.0), k.text);
                    if resp.clicked() {
                        pick = Some(255);
                    }
                });
            });
        });
        self.pen.alts_rect = Some(r.response.rect);
        match pick {
            Some(255) => {
                self.do_undo();
                self.pen.alts = None;
                self.status = "Undone.".into();
            }
            Some(v) => self.pen_pick_alternative(v),
            None => {}
        }
    }

    /// Help ▸ Pencil Gestures, the status bar chip, or "See gestures".
    pub fn pen_sheet(&mut self, ctx: &egui::Context) {
        if !self.pen.sheet {
            return;
        }
        let k = theme::ctx_tokens(ctx);
        let mut open = true;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("pen-sheet")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(pos2(screen.right() - 16.0, screen.bottom() - 40.0)).show(ctx, |ui| {
            egui::Frame::new().fill(k.panel).stroke(EStroke::new(1.0, k.line)).corner_radius(16.0).inner_margin(16.0).shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(50) }).show(ui, |ui| {
                ui.set_width(560.0f32.min(screen.width() - 40.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Pencil gestures").font(theme::font_semibold(17.0)).color(k.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("×").font(theme::font(20.0)).color(k.text_2)).frame(false)).clicked() {
                            open = false;
                        }
                    });
                });
                ui.add(egui::Label::new(egui::RichText::new("Write on the staff with the pen. Height = pitch, left–right = beat. Two-finger tap = Undo. Finger scrolls and zooms.").font(theme::font(13.0)).color(k.text_2)).wrap());
                ui.add_space(6.0);
                let items: [(u8, &str, &str); 8] = [
                    (3, "Filled head + stem", "Quarter note"),
                    (2, "Open head + stem", "Half note"),
                    (1, "Open head, no stem", "Whole note"),
                    (4, "Head + stem + flag", "Eighth note"),
                    (0, "Zig-zag", "Quarter rest"),
                    (10, "♯ ♭ ♮ before a head", "Accidental"),
                    (11, "Scribble over a note", "Delete"),
                    (12, "Same vertical as a note", "Chord"),
                ];
                for pair in items.chunks(2) {
                    ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for (fig, title, sub) in pair.iter() {
                        egui::Frame::new().fill(k.cell).corner_radius(10.0).inner_margin(8.0).show(ui, |ui| {
                            ui.set_width(250.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(56.0, 48.0), Sense::hover());
                                ui.painter().rect_filled(r, 6.0, Color32::WHITE);
                                for l in 0..5 {
                                    let y = r.top() + 8.0 + l as f32 * 8.0;
                                    ui.painter().line_segment([pos2(r.left() + 4.0, y), pos2(r.right() - 4.0, y)], EStroke::new(0.6, Color32::from_gray(190)));
                                }
                                sheet_figure(ui.painter(), r.center(), *fig);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(*title).font(theme::font_semibold(13.5)).color(k.text));
                                    ui.label(egui::RichText::new(*sub).font(theme::font(12.5)).color(k.text_3));
                                });
                            });
                        });
                    }
                    });
                }
            });
        });
        if !open {
            self.pen.sheet = false;
        }
    }

    /// The status bar chip (tablets in the browser): what the pen and the finger do; a tap opens the gestures, or
    /// turns Pen Mode on where no pen has been seen.
    pub fn pen_chip(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        let mode = self.prefs.pen_mode;
        let text = if mode {
            "✎ Pen Mode: On · touch writes · 2 fingers scroll"
        } else if self.pen.pen_seen {
            "✎ Pencil writes · finger scrolls  ⓘ"
        } else {
            "✎ Pen Mode: Off · tap to write with a passive pen"
        };
        let g = ui.painter().layout_no_wrap(text.to_string(), theme::font_semibold(11.5), if mode { Color32::WHITE } else { k.accent });
        let (r, resp) = ui.allocate_exact_size(g.size() + vec2(18.0, 6.0), Sense::click());
        ui.painter().rect_filled(r, r.height() / 2.0, if mode { k.accent } else { theme::alpha(k.accent, 0.12) });
        ui.painter().galley(r.min + vec2(9.0, 3.0), g, Color32::WHITE);
        if resp.clicked() {
            if mode {
                self.prefs.pen_mode = false;
                crate::prefs::save(&self.prefs);
                self.status = "Pen Mode off: one finger works the tool again.".into();
            } else if self.pen.pen_seen {
                self.pen.sheet = !self.pen.sheet;
            } else {
                self.prefs.pen_mode = true;
                crate::prefs::save(&self.prefs);
                self.status = "Pen Mode on: touching the staff writes; scroll with two fingers.".into();
            }
        }
    }
}

/// A note value as a Bravura glyph (0 = quarter rest).
fn figure(p: &egui::Painter, c: Pos2, value: u8, col: Color32) {
    let ch = match value {
        1 => '\u{E1D2}',
        2 => '\u{E1D3}',
        4 => '\u{E1D7}',
        0 => '\u{E4E5}',
        _ => '\u{E1D5}',
    };
    p.text(c + vec2(0.0, if value == 1 { 0.0 } else { 6.0 }), Align2::CENTER_CENTER, ch, FontId::new(26.0, FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into())), col);
}

/// The little drawings of the gesture sheet, in ink blue.
fn sheet_figure(p: &egui::Painter, c: Pos2, fig: u8) {
    let st = EStroke::new(2.0, INK);
    match fig {
        0..=4 => figure(p, c, fig, INK),
        10 => {
            p.text(c + vec2(-10.0, 0.0), Align2::CENTER_CENTER, "♯", theme::font_semibold(22.0), INK);
            p.circle_filled(c + vec2(9.0, 2.0), 4.0, INK);
        }
        11 => {
            let pts: Vec<Pos2> = (0..=10).map(|i| c + vec2(-16.0 + i as f32 * 3.2, if i % 2 == 0 { -9.0 } else { 9.0 })).collect();
            p.circle_filled(c, 4.0, Color32::BLACK);
            p.add(egui::Shape::line(pts, st));
        }
        _ => {
            p.circle_filled(c + vec2(0.0, 6.0), 4.0, Color32::BLACK);
            p.circle_filled(c + vec2(0.0, -6.0), 4.0, INK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen::samples::*;
    use std::collections::HashSet;

    /// A treble staff laid out and drawn as the score area does (so the pen sees the notes' hit areas).
    fn drawn(a: &mut App) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut geo = sv::layout(&a.score, Pos2::ZERO, 1.0, false);
        let (mut hits, mut ghits) = (vec![], vec![]);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
            let (sel, selg) = (HashSet::new(), HashSet::new());
            let st = sv::DrawState { selected: &sel, sel_graphics: &selg, cursor: None, play_tick: None, show_voice: None, show_control_points: false, show_hidden: false, sel_measures: None, hover_ghost: None, flash: None, erase: None };
            sv::draw(&p, &a.score, &geo, &st, &mut hits, &mut ghits);
        });
        out.textures_delta.clear();
        geo.notes = hits;
        geo.graphics = ghits;
        a.geo = geo;
    }

    fn app() -> App {
        let mut a = App::new(true);
        a.voice = 1;
        drawn(&mut a);
        a
    }

    /// Where beat `tick` of measure 0 is, on the line or space `pos` half-spaces above the top staff's middle line.
    fn spot(a: &App, tick: u32, pos: i32) -> Pos2 {
        let sys = &a.geo.systems[0];
        let mg = &sys.measures[0];
        let x = sv::x_for_tick(mg, tick, a.score.measures[0].ticks());
        pos2(x, sys.staves[0].1 - pos as f32 * a.geo.sp * 0.5)
    }

    /// Draws strokes with the pen: each takes 0.25 s, `gap` seconds apart. Returns when the pen last lifted.
    fn write(a: &mut App, strokes: &[Vec<Pos2>], t0: f64, gap: f64) -> f64 {
        let mut t = t0;
        for s in strokes {
            a.pen_feed(0, s[0], t);
            for p in &s[1..] {
                a.pen_feed(1, *p, t + 0.1);
            }
            a.pen_feed(2, *s.last().unwrap(), t + 0.25);
            a.pen_tick(t + 0.25);
            t += 0.25 + gap;
        }
        t - gap
    }

    fn notes(a: &App) -> Vec<(u32, u8, Vec<i32>)> {
        a.score.events[0][0].iter().map(|e| (e.start, e.value, e.pitches.iter().map(|p| p.midi()).collect())).collect()
    }

    #[test]
    fn head_and_stem_half_a_second_apart_are_one_quarter_note_on_g4_beat_3() {
        let mut a = app();
        let sp = a.geo.sp;
        // G4: the second line, two half-spaces under B4 (the middle line)
        let h = spot(&a, 2 * TPQ, -2);
        let up = write(&mut a, &[head(h, sp, true, 5), stem(h, sp, true, 5)], 10.0, 0.5);
        // still waiting: the ink stays until 0.7 s have passed without a new stroke
        a.pen_tick(up + 0.5);
        assert!(notes(&a).is_empty(), "nothing before the pause ends");
        assert_eq!(a.pen.pending.len(), 2, "both strokes wait as one symbol");
        a.pen_tick(up + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a), vec![(2 * TPQ, 3, vec![67])]);
        // the alternatives show for 3 s
        assert!(a.pen.alts.is_some());
        a.pen_tick(up + pen::DEBOUNCE + pen::ALTERNATIVES_SHOW + 0.1);
        assert!(a.pen.alts.is_none());
    }

    #[test]
    fn a_second_apart_they_are_two_symbols() {
        let mut a = app();
        let sp = a.geo.sp;
        let h = spot(&a, TPQ, -2);
        // the head alone (a filled head is a quarter note), then a stem alone a second later: not recognized
        a.pen_feed(0, head(h, sp, true, 2)[0], 1.0);
        for p in head(h, sp, true, 2) {
            a.pen_feed(1, p, 1.1);
        }
        a.pen_feed(2, h, 1.25);
        a.pen_tick(1.25 + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a).len(), 1, "the head was read alone");
        let s = stem(h, sp, true, 2);
        let t = 1.25 + 1.0;
        a.pen_feed(0, s[0], t);
        for p in &s {
            a.pen_feed(1, *p, t + 0.1);
        }
        a.pen_feed(2, *s.last().unwrap(), t + 0.2);
        a.pen_tick(t + 0.2 + pen::DEBOUNCE + 0.01);
        assert_eq!(notes(&a).len(), 1, "a stem alone adds nothing");
        assert!(a.pen.feedback.iter().any(|f| f.kind == FeedbackKind::Unrecognized));
    }

    #[test]
    fn nothing_is_inserted_when_it_is_not_recognized() {
        let mut a = app();
        let sp = a.geo.sp;
        let c = loose_curl(spot(&a, 0, 0), sp, 4);
        let up = write(&mut a, &[c], 1.0, 0.0);
        a.pen_tick(up + pen::DEBOUNCE + 0.01);
        assert!(notes(&a).is_empty());
        assert!(a.pen.balloon.is_some(), "Not recognized · See gestures");
        a.pen_tick(up + pen::DEBOUNCE + pen::UNRECOGNIZED_SHOW + 0.1);
        assert!(a.pen.balloon.is_none() && a.pen.feedback.is_empty(), "gone after 2 s");
    }

    #[test]
    fn chord_accidental_rest_scribble_and_alternatives() {
        let mut a = app();
        let sp = a.geo.sp;
        // a quarter on G4, beat 1
        let g4 = spot(&a, 0, -2);
        let up = write(&mut a, &[head(g4, sp, true, 1), stem(g4, sp, true, 1)], 1.0, 0.3);
        a.pen_tick(up + 1.0);
        drawn(&mut a);
        // a head on B4 in the same vertical: a chord
        let b4 = pos2(g4.x + sp * 0.3, g4.y - sp);
        let up = write(&mut a, &[head(b4, sp, true, 6)], 3.0, 0.0);
        a.pen_tick(up + 1.0);
        assert_eq!(notes(&a), vec![(0, 3, vec![67, 71])]);
        drawn(&mut a);
        // ♯ then a head on F4 at beat 2: F♯4
        let f4 = spot(&a, TPQ, -3);
        let mut strokes = sharp(pos2(f4.x - 2.0 * sp, f4.y), sp, 3);
        strokes.push(head(f4, sp, true, 3));
        let up = write(&mut a, &strokes, 5.0, 0.3);
        a.pen_tick(up + 1.0);
        assert_eq!(notes(&a)[1], (TPQ, 3, vec![66]));
        drawn(&mut a);
        // a quarter rest on beat 3
        let r = quarter_rest(spot(&a, 2 * TPQ, 0), sp, 8);
        let up = write(&mut a, &[r], 8.0, 0.0);
        a.pen_tick(up + 1.0);
        assert_eq!(notes(&a)[2], (2 * TPQ, 3, vec![]));
        // "was it a half note?": one tap on the alternatives
        drawn(&mut a);
        let h4 = head(spot(&a, 3 * TPQ, -2), sp, true, 9);
        let up = write(&mut a, &[h4], 10.0, 0.0);
        a.pen_tick(up + 1.0);
        assert_eq!(notes(&a)[3], (3 * TPQ, 3, vec![67]));
        a.pen_pick_alternative(4);
        assert_eq!(notes(&a)[3], (3 * TPQ, 4, vec![67]), "changed to an eighth note");
        // a scribble over the F♯4 deletes it
        drawn(&mut a);
        let fhit = a.geo.notes.iter().find(|h| h.r.idx == 1 && h.pitch.is_some()).unwrap().rect.center();
        let up = write(&mut a, &[scribble(fhit, sp, 2)], 12.0, 0.0);
        a.pen_tick(up + 1.0);
        assert!(!notes(&a).iter().any(|n| n.2 == vec![66]), "F♯4 was scribbled out");
    }
}

#[cfg(test)]
mod balloon_tests {
    use super::*;
    /// "? Not recognized · See gestures" is drawn while the balloon is up.
    #[test]
    fn the_not_recognized_balloon_is_drawn() {
        let mut a = App::new(true);
        a.pen.balloon = Some((pos2(300.0, 300.0), 1e9));
        let ctx = egui::Context::default();
        crate::theme::install(&ctx, Some(egui::Theme::Light));
        let mut found = false;
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let p = ui.painter().clone();
                a.pen_draw(ui, &p, 0.0);
            });
            out.textures_delta.clear();
            for c in &out.shapes {
                if let egui::Shape::Text(t) = &c.shape {
                    if t.galley.text().contains("Not recognized") {
                        found = true;
                    }
                }
            }
        }
        assert!(found, "balloon text not drawn; balloon state {:?}", a.pen.balloon);
    }
}
