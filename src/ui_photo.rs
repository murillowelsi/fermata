//! F-31: the photo-import dialogs' pieces (icons, tips, progress window), kept apart from ui.rs.
use crate::app::*;
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, Ui};

#[derive(Clone, Copy)]
pub enum PhotoIcon {
    Camera,
    Page,
    Sun,
    Frame,
    Pdf,
}

/// The small line icons of the photo-import dialogs, drawn in the accent colour (no icon font).
pub fn draw_photo_icon(p: &egui::Painter, c: egui::Pos2, s: f32, icon: PhotoIcon, col: Color32) {
    let st = Stroke::new(s * 0.09, col);
    match icon {
        PhotoIcon::Camera => {
            let body = Rect::from_center_size(c + vec2(0.0, s * 0.06), vec2(s * 0.9, s * 0.62));
            p.rect_stroke(body, s * 0.12, st, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x - s * 0.16, body.top()), pos2(c.x - s * 0.1, body.top() - s * 0.12)], st);
            p.line_segment([pos2(c.x - s * 0.1, body.top() - s * 0.12), pos2(c.x + s * 0.1, body.top() - s * 0.12)], st);
            p.line_segment([pos2(c.x + s * 0.1, body.top() - s * 0.12), pos2(c.x + s * 0.16, body.top())], st);
            p.circle_stroke(body.center(), s * 0.17, st);
        }
        PhotoIcon::Page | PhotoIcon::Pdf => {
            let r = Rect::from_center_size(c, vec2(s * 0.62, s * 0.82));
            p.rect_stroke(r, s * 0.08, st, egui::StrokeKind::Middle);
            let lines = if matches!(icon, PhotoIcon::Page) { 3 } else { 1 };
            for i in 0..lines {
                let y = r.top() + r.height() * (0.35 + 0.18 * i as f32);
                p.line_segment([pos2(r.left() + s * 0.13, y), pos2(r.right() - s * 0.13, y)], st);
            }
        }
        PhotoIcon::Sun => {
            p.circle_stroke(c, s * 0.17, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.28, c + d * s * 0.4], st);
            }
        }
        PhotoIcon::Frame => {
            let r = Rect::from_center_size(c, vec2(s * 0.8, s * 0.8));
            let l = s * 0.2;
            for (corner, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
                p.line_segment([corner, corner + vec2(dx * l, 0.0)], st);
                p.line_segment([corner, corner + vec2(0.0, dy * l)], st);
            }
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 0.34, s * 0.34)), 1.0, st, egui::StrokeKind::Middle);
        }
    }
}

/// The 44 pt icon tile at the top of the first-use alert.
pub fn photo_icon(ui: &mut Ui, icon: PhotoIcon) {
    let k = theme::t(ui);
    let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
    ui.painter().rect_filled(r, 10.0, k.accent_soft);
    draw_photo_icon(ui.painter(), r.center(), 26.0, icon, k.accent);
}

/// "Couldn't read this image": four tips for a better picture.
pub fn photo_tips(ui: &mut Ui) {
    let k = theme::t(ui);
    let tips = [
        (PhotoIcon::Page, "Paper flat, phone\nstraight above it"),
        (PhotoIcon::Sun, "Bright, even light,\nno shadows"),
        (PhotoIcon::Frame, "One page, all\nedges in the frame"),
        (PhotoIcon::Pdf, "A scanned PDF\nworks best"),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let w = (ui.available_width() - 24.0) / 4.0;
        for (icon, text) in tips {
            let (r, _) = ui.allocate_exact_size(vec2(w, 78.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 8.0, k.cell);
            draw_photo_icon(p, pos2(r.center().x, r.top() + 22.0), 22.0, icon, k.accent);
            p.text(pos2(r.center().x, r.top() + 54.0), Align2::CENTER_CENTER, text, theme::font(11.5), k.text_2);
        }
    });
}

impl App {
    /// F-31: the progress window: the picture, then the download of the reader (MB and time left) or the reading.
    pub fn photo_progress_body(&mut self, ui: &mut Ui) {
        let k = theme::t(ui);
        ui.set_width(360.0);
        let (name, kind, thumb) = self.omr_picture.clone().unwrap_or_default();
        egui::Frame::new().fill(k.cell).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 56.0), Sense::hover());
                let tex = thumb.as_ref().and_then(|b| {
                    let id = egui::Id::unique(("omr-thumb", &name, b.len()));
                    if let Some(t) = ui.ctx().memory(|m| m.data.get_temp::<egui::TextureHandle>(id)) {
                        return Some(t);
                    }
                    let img = image::load_from_memory(b).ok()?.to_rgba8();
                    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
                    let t = ui.ctx().load_texture("omr-thumb", ci, Default::default());
                    ui.ctx().memory_mut(|m| m.data.insert_temp(id, t.clone()));
                    Some(t)
                });
                ui.painter().rect_filled(r, 3.0, Color32::WHITE);
                match tex {
                    Some(t) => {
                        let s = t.size_vec2();
                        let f = (r.width() / s.x).min(r.height() / s.y);
                        let ir = Rect::from_center_size(r.center(), s * f);
                        ui.painter().image(t.id(), ir, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => draw_photo_icon(ui.painter(), r.center(), 30.0, PhotoIcon::Page, k.text_3),
                }
                ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, k.line), egui::StrokeKind::Inside);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(&name).font(theme::font_semibold(13.0)).color(k.text));
                    ui.label(egui::RichText::new(&kind).font(theme::font(11.5)).color(k.text_3));
                });
            });
        });
        ui.add_space(4.0);
        let now = ui.input(|i| i.time);
        let bar = |ui: &mut Ui, frac: Option<f32>| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            ui.painter().rect_filled(r, 3.0, k.line);
            let fill = match frac {
                Some(f) => Rect::from_min_max(r.min, pos2(r.left() + r.width() * f.clamp(0.0, 1.0), r.bottom())),
                // the reader says nothing about its progress: a moving segment
                None => {
                    let t = (now * 0.6).fract() as f32;
                    let x0 = r.left() + (r.width() * 1.3) * t - r.width() * 0.3;
                    Rect::from_min_max(pos2(x0.max(r.left()), r.top()), pos2((x0 + r.width() * 0.3).min(r.right()), r.bottom()))
                }
            };
            ui.painter().rect_filled(fill, 3.0, k.accent);
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((stage, done, total)) = self.omr_setup.as_ref().map(|j| j.progress()) {
            use crate::omr::setup::Stage;
            let title = match stage {
                Stage::Downloading => "Downloading the photo reader... (one time only)",
                Stage::Checking => "Checking the photo reader...",
                Stage::Unpacking => "Setting up the photo reader...",
            };
            ui.label(egui::RichText::new(title).font(theme::font_semibold(13.5)).color(k.text));
            bar(ui, if stage == Stage::Downloading { Some(done as f32 / total.max(1) as f32) } else { None });
            if stage == Stage::Downloading {
                if done > 0 && self.omr_started.is_none() {
                    self.omr_started = Some(now);
                }
                let left = self.omr_started.filter(|s| now - s > 1.5 && done > 0).map(|s| {
                    let rate = done as f64 / (now - s);
                    let secs = total.saturating_sub(done) as f64 / rate.max(1.0);
                    if secs < 60.0 { "less than a minute left".to_string() } else { format!("about {} minutes left", (secs / 60.0).round().max(1.0)) }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{:.0} of {:.0} MB", done as f64 / 1e6, total as f64 / 1e6)).font(theme::font(11.5)).color(k.text_3));
                    if let Some(l) = left {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(l).font(theme::font(11.5)).color(k.text_3));
                        });
                    }
                });
            }
            ui.label(egui::RichText::new("When it finishes, Fermata reads the score right away.").font(theme::font(11.5)).color(k.text_3));
            return;
        }
        ui.label(egui::RichText::new("Reading the score... this can take a minute").font(theme::font_semibold(13.5)).color(k.text));
        bar(ui, None);
    }
}
