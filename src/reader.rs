//! The phone's reader (F-25, browser only): the score to read and hear, nothing to edit. A top bar with the
//! open button, the title and the "⋯" menu (Open..., Export, Appearance); the page fitted to the width (pinch to
//! zoom, one finger scrolls); a play bar with the measure, a progress bar to jump anywhere, stop, play/pause, play
//! from the tapped measure and the tempo. Playback moves the cursor and scrolls the page.
use crate::app::*;
use crate::commands::{self, Cmd};
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, Ui};

/// Whether this browser can reach a MIDI piano (Safari and every browser on an iPad can't).
pub fn has_midi() -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::midi_status() != "unsupported";
    #[cfg(not(target_arch = "wasm32"))]
    return true;
}

fn safe_bottom() -> f32 {
    #[cfg(target_arch = "wasm32")]
    return crate::web::safe_area_bottom();
    #[cfg(not(target_arch = "wasm32"))]
    return 0.0;
}

/// "Piano · 9 compassos": the instruments (each name once) and the number of measures.
pub fn subtitle(score: &crate::model::Score) -> String {
    let mut names: Vec<&str> = vec![];
    for s in &score.staves {
        let n = s.name.trim();
        if !n.is_empty() && !names.contains(&n) {
            names.push(n);
        }
    }
    let inst = match names.len() {
        0 => String::new(),
        1..=2 => names.join(", "),
        _ => format!("{}, {}…", names[0], names[1]),
    };
    let m = score.measures.len();
    let ms = if m == 1 { "1 compasso".to_string() } else { format!("{m} compassos") };
    if inst.is_empty() { ms } else { format!("{inst} · {ms}") }
}

impl App {
    /// The whole reader screen (instead of the menus, toolbar, palettes and status bar).
    pub fn reader_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let k = theme::ctx_tokens(&ctx);
        let now = ctx.input(|i| i.time);
        // playback that ends by itself starts from the beginning next time
        let playing = self.is_playing();
        if playing {
            if let Some((m, _)) = self.play_position() {
                self.reader_pos = m;
            }
        } else if self.reader_was_playing {
            self.reader_pos = 0;
        }
        self.reader_was_playing = playing;
        if self.reader_hint_until == 0.0 {
            self.reader_hint_until = now + 4.0;
        }
        let safe = safe_bottom();
        egui::Panel::top("reader_top").exact_size(60.0).frame(egui::Frame::new().fill(k.win).inner_margin(egui::Margin::symmetric(8, 0))).show(ui, |ui| self.reader_top(ui));
        egui::Panel::bottom("reader_bar")
            .exact_size(118.0 + safe)
            .frame(egui::Frame::new().fill(k.win).inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: safe.min(60.0) as i8 }))
            .show(ui, |ui| self.reader_bar(ui));
        let page = egui::CentralPanel::default().frame(egui::Frame::new().fill(k.canvas)).show(ui, |ui| self.score_area(ui)).response.rect;
        // the hint over the page: fitted to the width for a few seconds; after a pinch, a button back to the fit
        let show_fit = !self.reader_user_zoom && now < self.reader_hint_until;
        if show_fit || self.reader_user_zoom {
            let label = if self.reader_user_zoom { "⤢  Ajustar à largura" } else { "⤢  Ajustado à largura · pinça para ampliar" };
            let r = egui::Area::new(egui::Id::unique("reader_fit_hint"))
                .fixed_pos(pos2(page.center().x, page.top() + 12.0))
                .pivot(Align2::CENTER_TOP)
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(k.panel)
                        .corner_radius(18.0)
                        .stroke(Stroke::new(1.0, k.line))
                        .shadow(egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: k.shadow })
                        .inner_margin(egui::Margin::symmetric(16, 9))
                        .show(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(label).font(theme::font_medium(14.0)).color(k.text)).sense(Sense::click())))
                        .inner
                });
            if r.inner.clicked() {
                self.reader_user_zoom = false;
                self.reader_hint_until = now + 4.0;
            }
            if show_fit {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        // the phone has no status bar: the sound held by the browser gets its own hint over the page (F-27)
        #[cfg(target_arch = "wasm32")]
        let notice = (self.status == crate::web::TAP_FOR_SOUND && crate::web::sound_blocked())
            || (self.status == crate::web::SILENT_HINT && crate::web::now() < self.notice_until);
        #[cfg(not(target_arch = "wasm32"))]
        let notice = false;
        if notice {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            egui::Area::new(egui::Id::unique("reader_tap_sound"))
                .fixed_pos(pos2(page.center().x, page.top() + 60.0))
                .pivot(Align2::CENTER_TOP)
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(&ctx, |ui| {
                    egui::Frame::new().fill(k.accent).corner_radius(18.0).inner_margin(egui::Margin::symmetric(16, 9)).show(ui, |ui| {
                        ui.label(egui::RichText::new(&self.status).font(theme::font_medium(14.0)).color(k.accent_fg));
                    });
                });
        }
        if self.reader_menu {
            self.reader_sheet(&ctx);
        }
    }

    fn reader_top(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        let r = ui.max_rect();
        // the open button, a folder
        let open = ui.interact(Rect::from_center_size(pos2(r.left() + 26.0, r.center().y), vec2(48.0, 48.0)), ui.scope_id().with("open"), Sense::click());
        crate::platform::open_target(open.rect);
        {
            let p = ui.painter();
            let c = open.rect.center();
            let st = Stroke::new(1.8, k.text);
            let pts = vec![c + vec2(-11.0, -8.0), c + vec2(-3.5, -8.0), c + vec2(-1.0, -5.0), c + vec2(11.0, -5.0), c + vec2(11.0, 8.5), c + vec2(-11.0, 8.5), c + vec2(-11.0, -8.0)];
            p.add(egui::Shape::line(pts, st));
            p.line_segment([c + vec2(-11.0, -2.0), c + vec2(11.0, -2.0)], st);
        }
        if open.clicked() {
            self.exec(Cmd::Open);
        }
        // title and "Piano · 9 compassos"
        let title = if self.score.text.title.trim().is_empty() { "Untitled".to_string() } else { self.score.text.title.trim().to_string() };
        let p = ui.painter();
        let tg = p.layout_no_wrap(title, theme::font_semibold(17.0), k.text);
        let sg = p.layout_no_wrap(subtitle(&self.score), theme::font(13.0), k.text_3);
        let max_w = r.width() - 120.0;
        let tw = tg.size().x.min(max_w);
        p.with_clip_rect(Rect::from_center_size(r.center(), vec2(max_w, r.height()))).galley(pos2(r.center().x - tw / 2.0, r.center().y - 19.0), tg, k.text);
        let sw = sg.size().x.min(max_w);
        p.with_clip_rect(Rect::from_center_size(r.center(), vec2(max_w, r.height()))).galley(pos2(r.center().x - sw / 2.0, r.center().y + 3.0), sg, k.text_3);
        // the "⋯" menu
        let more = ui.interact(Rect::from_center_size(pos2(r.right() - 26.0, r.center().y), vec2(48.0, 48.0)), ui.scope_id().with("more"), Sense::click());
        for dx in [-8.0, 0.0, 8.0] {
            ui.painter().circle_filled(more.rect.center() + vec2(dx, 0.0), 2.6, k.text);
        }
        if more.clicked() {
            self.reader_menu = !self.reader_menu;
            self.reader_export = false;
        }
    }

    fn reader_bar(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        let n = self.score.measures.len().max(1);
        let cur = self.reader_pos.min(n - 1);
        let full = ui.max_rect();
        // the progress bar: where the music is; a tap or a slide jumps to that measure
        let track = Rect::from_min_size(full.min, vec2(full.width(), 28.0));
        let resp = ui.interact(track, ui.scope_id().with("progress"), Sense::click_and_drag());
        let frac = if n > 1 { cur as f32 / (n - 1) as f32 } else { 0.0 };
        let y = track.center().y;
        let p = ui.painter();
        p.line_segment([pos2(track.left(), y), pos2(track.right(), y)], Stroke::new(4.0, k.line));
        let kx = track.left() + track.width() * frac;
        p.line_segment([pos2(track.left(), y), pos2(kx, y)], Stroke::new(4.0, k.accent));
        p.circle(pos2(kx, y), 11.0, Color32::WHITE, Stroke::new(1.0, theme::alpha(Color32::BLACK, 0.2)));
        if resp.clicked() || resp.dragged() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let f = ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0);
                let m = (f * (n - 1) as f32).round() as usize;
                self.reader_pos = m;
                self.cursor = (self.cursor.0, m, 0);
                self.scroll_to_cursor = true;
            }
        }
        if (resp.clicked() || resp.drag_stopped()) && self.is_playing() {
            let m = self.reader_pos;
            self.play_from(m);
            self.reader_was_playing = true;
        }
        // the controls
        let row = Rect::from_min_max(pos2(full.left(), track.bottom() + 6.0), pos2(full.right(), track.bottom() + 76.0));
        let p = ui.painter();
        p.text(pos2(row.left(), row.center().y - 11.0), Align2::LEFT_CENTER, format!("Compasso {}", cur + 1), theme::font_semibold(15.5), k.text);
        p.text(pos2(row.left(), row.center().y + 12.0), Align2::LEFT_CENTER, format!("de {n}"), theme::font(14.0), k.text_2);
        let tempo = self.score.tempo_at(cur);
        p.text(pos2(row.right(), row.center().y - 11.0), Align2::RIGHT_CENTER, format!("♩= {tempo}"), theme::font_semibold(15.5), k.text);
        let sound = match self.midi_out.as_ref().and_then(|m| m.connected[0].clone()) {
            Some(port) => port,
            None => "som do app".into(),
        };
        p.text(pos2(row.right(), row.center().y + 12.0), Align2::RIGHT_CENTER, sound, theme::font(14.0), k.text_3);
        let c = row.center();
        let playing = self.is_playing();
        // stop (48), play or pause (60, blue), play from the tapped measure (48)
        let gap = (row.width() * 0.17).clamp(58.0, 90.0);
        let stop = ui.interact(Rect::from_center_size(c - vec2(gap, 0.0), vec2(48.0, 48.0)), ui.scope_id().with("stop"), Sense::click());
        let play = ui.interact(Rect::from_center_size(c, vec2(60.0, 60.0)), ui.scope_id().with("play"), Sense::click());
        let from = ui.interact(Rect::from_center_size(c + vec2(gap, 0.0), vec2(48.0, 48.0)), ui.scope_id().with("from"), Sense::click());
        let p = ui.painter();
        p.rect_filled(Rect::from_center_size(stop.rect.center(), vec2(16.0, 16.0)), 2.5, if stop.is_pointer_button_down_on() { k.text_3 } else { k.text });
        p.circle_filled(c, 30.0, if play.is_pointer_button_down_on() { k.accent_hover } else { k.accent });
        if playing {
            for dx in [-5.5, 5.5] {
                p.rect_filled(Rect::from_center_size(c + vec2(dx, 0.0), vec2(6.0, 20.0)), 1.5, k.accent_fg);
            }
        } else {
            p.add(egui::Shape::convex_polygon(vec![c + vec2(-7.0, -11.0), c + vec2(-7.0, 11.0), c + vec2(12.0, 0.0)], k.accent_fg, Stroke::NONE));
        }
        let fc = from.rect.center();
        p.rect_filled(Rect::from_center_size(fc + vec2(-8.0, 0.0), vec2(3.0, 18.0)), 1.0, k.text);
        p.add(egui::Shape::convex_polygon(vec![fc + vec2(-3.0, -9.0), fc + vec2(-3.0, 9.0), fc + vec2(10.0, 0.0)], k.text, Stroke::NONE));
        if stop.clicked() {
            self.stop();
            self.reader_pos = 0;
            self.reader_was_playing = false;
        }
        if play.clicked() {
            if playing {
                // pause: the next play starts again at this measure
                self.reader_pos = cur;
                self.stop();
                self.reader_was_playing = false;
            } else {
                self.play_from(cur);
                self.reader_was_playing = true;
            }
        }
        if from.clicked() {
            let m = self.cursor.1.min(n - 1);
            self.reader_pos = m;
            self.play_from(m);
            self.reader_was_playing = true;
        }
    }

    /// The "⋯" sheet from the bottom: Open..., Export (MIDI, MusicXML, PNG), Appearance, and where to edit.
    fn reader_sheet(&mut self, ctx: &egui::Context) {
        let k = theme::ctx_tokens(ctx);
        let screen = ctx.content_rect();
        // the page dims behind the sheet; a tap there closes it
        let dim = egui::Area::new(egui::Id::unique("reader_dim")).fixed_pos(screen.min).order(egui::Order::Middle).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, theme::alpha(Color32::BLACK, 0.25));
            resp
        });
        if dim.inner.clicked() {
            self.reader_menu = false;
            return;
        }
        let w = (screen.width() - 16.0).min(520.0);
        let safe = safe_bottom();
        let mut close = false;
        egui::Area::new(egui::Id::unique("reader_sheet"))
            .fixed_pos(pos2(screen.center().x, screen.bottom() - 8.0 - safe))
            .pivot(Align2::CENTER_BOTTOM)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(k.panel)
                    .corner_radius(24.0)
                    .stroke(Stroke::new(1.0, k.line))
                    .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: k.shadow })
                    .inner_margin(egui::Margin::symmetric(16, 14))
                    .show(ui, |ui| {
                        ui.set_width(w - 32.0);
                        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                        let row = |ui: &mut Ui, label: &str, right: &str, checked: Option<bool>| -> egui::Response {
                            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
                            let on = checked == Some(true);
                            let p = ui.painter();
                            if on {
                                p.rect_filled(r, 14.0, k.accent);
                            } else if resp.is_pointer_button_down_on() {
                                p.rect_filled(r, 14.0, k.cell_press);
                            }
                            let fg = if on { k.accent_fg } else { k.text };
                            let x = if checked.is_some() { r.left() + 50.0 } else { r.left() + 52.0 };
                            if on {
                                let c = pos2(r.left() + 24.0, r.center().y);
                                let s = Stroke::new(2.2, fg);
                                p.line_segment([c + vec2(-6.0, 0.0), c + vec2(-1.5, 4.5)], s);
                                p.line_segment([c + vec2(-1.5, 4.5), c + vec2(7.0, -5.0)], s);
                            }
                            p.text(pos2(x, r.center().y), Align2::LEFT_CENTER, label, theme::font(18.0), fg);
                            if !right.is_empty() {
                                p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, right, theme::font(15.0), k.text_3);
                            }
                            resp
                        };
                        // Open... with its folder, Export with its arrow out of a box
                        let open = row(ui, &commands::label(Cmd::Open), &theme::shortcut_glyphs(Cmd::Open), None);
                        crate::platform::open_target(open.rect);
                        icon_folder(ui.painter(), pos2(open.rect.left() + 24.0, open.rect.center().y), k.text);
                        if open.clicked() {
                            close = true;
                            self.exec(Cmd::Open);
                        }
                        let export = row(ui, "Export", if self.reader_export { "‹" } else { "MIDI · MusicXML · PNG ›" }, None);
                        icon_share(ui.painter(), pos2(export.rect.left() + 24.0, export.rect.center().y), k.text);
                        if export.clicked() {
                            self.reader_export = !self.reader_export;
                        }
                        if self.reader_export {
                            for c in [Cmd::ExportMidi, Cmd::ExportXml, Cmd::ExportPng] {
                                if row(ui, &format!("   {}", commands::label(c)), "", None).clicked() {
                                    close = true;
                                    self.pending_cmd = Some(c);
                                }
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Appearance").font(theme::font_medium(14.0)).color(k.text_3));
                        use crate::prefs::Appearance;
                        let cur = self.prefs.appearance;
                        for (a, name) in [(Appearance::System, "Use System Setting"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")] {
                            if row(ui, name, "", Some(cur == a)).clicked() {
                                self.set_appearance(ui.ctx(), a);
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("Para editar, use um iPad ou computador").font(theme::font(15.0)).color(k.text_3));
                        });
                        ui.add_space(6.0);
                    });
            });
        if close {
            self.reader_menu = false;
            self.reader_export = false;
        }
    }
}

fn icon_folder(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let st = Stroke::new(1.7, col);
    let pts = vec![c + vec2(-10.0, -7.0), c + vec2(-3.0, -7.0), c + vec2(-0.5, -4.5), c + vec2(10.0, -4.5), c + vec2(10.0, 8.0), c + vec2(-10.0, 8.0), c + vec2(-10.0, -7.0)];
    p.add(egui::Shape::line(pts, st));
    p.line_segment([c + vec2(-10.0, -1.5), c + vec2(10.0, -1.5)], st);
}

fn icon_share(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let st = Stroke::new(1.7, col);
    p.add(egui::Shape::line(vec![c + vec2(-5.0, -2.0), c + vec2(-9.0, -2.0), c + vec2(-9.0, 10.0), c + vec2(9.0, 10.0), c + vec2(9.0, -2.0), c + vec2(5.0, -2.0)], st));
    p.line_segment([c + vec2(0.0, 4.0), c + vec2(0.0, -11.0)], st);
    p.add(egui::Shape::line(vec![c + vec2(-4.5, -6.5), c + vec2(0.0, -11.0), c + vec2(4.5, -6.5)], st));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_names_each_instrument_once() {
        let mut sc = crate::app::default_new_score();
        for s in sc.staves.iter_mut() {
            s.name = "Piano".into();
        }
        let n = sc.measures.len();
        assert_eq!(subtitle(&sc), format!("Piano · {n} compassos"));
        sc.measures.truncate(1);
        assert_eq!(subtitle(&sc), "Piano · 1 compasso");
    }
}
