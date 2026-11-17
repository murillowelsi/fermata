//! The component kit of direction A (F-13, spec §3): buttons, fields, number fields with arrows, check boxes,
//! radio buttons, sliders, chips, form rows, groups, foldable sections, menu items and the window frames.
//! Every dialog, floating window and menu draws with these, so the look lives in one place.
use crate::theme;
use egui::emath::Numeric;
use egui::{pos2, vec2, Align2, Color32, Rect, Response, Sense, Stroke, Ui};
use std::ops::RangeInclusive;

pub const BTN_H: f32 = 26.0;
pub const BTN_MIN_W: f32 = 80.0;
const RADIUS: f32 = 7.0;

/// macOS window buttons (red closes; the other two are only drawn, as on a sheet that can't be resized).
const TL_RED: Color32 = Color32::from_rgb(0xFF, 0x5F, 0x57);
const TL_YELLOW: Color32 = Color32::from_rgb(0xFE, 0xBC, 0x2E);
const TL_GREEN: Color32 = Color32::from_rgb(0x28, 0xC8, 0x40);

fn disabled_alpha(ui: &Ui, c: Color32) -> Color32 {
    if ui.is_enabled() {
        c
    } else {
        theme::alpha(c, 0.45)
    }
}

// ---------------- buttons ----------------

#[derive(Clone, Copy, PartialEq)]
pub enum BtnKind {
    Secondary,
    Primary,
    Danger,
}

fn button(ui: &mut Ui, text: &str, kind: BtnKind, min_w: f32) -> Response {
    let k = theme::t(ui);
    let font = if kind == BtnKind::Primary { theme::font_semibold(13.0) } else { theme::font_medium(13.0) };
    let g = ui.painter().layout_no_wrap(text.to_string(), font, k.text);
    let w = (g.size().x + 28.0).max(min_w);
    let (r, resp) = ui.allocate_exact_size(vec2(w, BTN_H), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let (fill, fg, ring) = match kind {
        BtnKind::Primary => (if down || resp.hovered() { k.accent_hover } else { k.accent }, k.accent_fg, None),
        BtnKind::Danger => (k.field, k.danger, Some(k.line_2)),
        BtnKind::Secondary => (k.field, k.text, Some(k.line_2)),
    };
    let fill = if kind != BtnKind::Primary && down {
        theme::blend(fill, k.cell_hover)
    } else if kind != BtnKind::Primary && resp.hovered() {
        theme::blend(fill, k.cell)
    } else {
        fill
    };
    let p = ui.painter();
    p.rect(r, RADIUS, disabled_alpha(ui, fill), ring.map(|c| Stroke::new(1.0, disabled_alpha(ui, c))).unwrap_or(Stroke::NONE), egui::StrokeKind::Inside);
    if resp.has_focus() {
        p.rect_stroke(r.expand(2.0), RADIUS + 2.0, Stroke::new(3.0, k.focus), egui::StrokeKind::Middle);
    }
    p.galley(r.center() - g.size() / 2.0, g, disabled_alpha(ui, fg));
    resp
}

/// A normal button (26 pt high, at least 80 wide).
pub fn btn(ui: &mut Ui, text: &str) -> Response {
    button(ui, text, BtnKind::Secondary, BTN_MIN_W)
}
/// The main button of a dialog: blue, always the rightmost.
pub fn btn_primary(ui: &mut Ui, text: &str) -> Response {
    button(ui, text, BtnKind::Primary, BTN_MIN_W)
}
/// A destructive action: red text.
pub fn btn_danger(ui: &mut Ui, text: &str) -> Response {
    button(ui, text, BtnKind::Danger, BTN_MIN_W)
}
/// A button inside a dialog's content (no minimum width).
pub fn btn_inline(ui: &mut Ui, text: &str) -> Response {
    button(ui, text, BtnKind::Secondary, 0.0)
}

// ---------------- check boxes, radio buttons, chips ----------------

fn toggle_label(ui: &mut Ui, text: &str, round: bool, on: bool) -> Response {
    let k = theme::t(ui);
    let wrap = ui.available_width().clamp(120.0, 520.0) - 24.0;
    let g = ui.painter().layout(text.to_string(), theme::font(13.0), k.text, wrap);
    let h = g.size().y.max(20.0);
    let (r, resp) = ui.allocate_exact_size(vec2(16.0 + if text.is_empty() { 0.0 } else { 8.0 + g.size().x }, h), Sense::click());
    let b = Rect::from_center_size(pos2(r.left() + 8.0, r.top() + 10.0), vec2(16.0, 16.0));
    let p = ui.painter();
    let hover = resp.hovered() && ui.is_enabled();
    if round {
        if on {
            p.circle_filled(b.center(), 8.0, disabled_alpha(ui, if hover { k.accent_hover } else { k.accent }));
            p.circle_filled(b.center(), 3.0, disabled_alpha(ui, k.field));
        } else {
            p.circle(b.center(), 7.5, disabled_alpha(ui, k.field), Stroke::new(1.0, disabled_alpha(ui, if hover { k.text_3 } else { k.line_2 })));
        }
    } else if on {
        p.rect_filled(b, 4.5, disabled_alpha(ui, if hover { k.accent_hover } else { k.accent }));
        let c = b.center();
        let s = Stroke::new(1.8, disabled_alpha(ui, k.accent_fg));
        p.line_segment([c + vec2(-3.8, 0.2), c + vec2(-1.2, 2.8)], s);
        p.line_segment([c + vec2(-1.2, 2.8), c + vec2(3.8, -2.6)], s);
    } else {
        p.rect(b, 4.5, disabled_alpha(ui, k.field), Stroke::new(1.0, disabled_alpha(ui, if hover { k.text_3 } else { k.line_2 })), egui::StrokeKind::Inside);
    }
    if resp.has_focus() {
        p.rect_stroke(b.expand(2.0), 6.0, Stroke::new(3.0, k.focus), egui::StrokeKind::Middle);
    }
    if !text.is_empty() {
        p.galley(pos2(b.right() + 8.0, r.top() + (20.0 - 16.0) / 2.0 + 0.5), g, disabled_alpha(ui, k.text));
    }
    resp
}

/// A check box with its label (16 pt box, blue when on).
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: impl ToString) -> Response {
    let mut resp = toggle_label(ui, &text.to_string(), false, *on);
    if resp.clicked() && ui.is_enabled() {
        *on = !*on;
        resp.mark_changed();
    }
    resp
}

/// A radio button showing `on`; the caller acts on the click.
pub fn radio(ui: &mut Ui, on: bool, text: impl ToString) -> Response {
    toggle_label(ui, &text.to_string(), true, on)
}

/// A radio button that sets `value` to `mine` when clicked.
pub fn radio_value<T: PartialEq>(ui: &mut Ui, value: &mut T, mine: T, text: impl ToString) -> Response {
    let mut resp = radio(ui, *value == mine, text);
    if resp.clicked() && ui.is_enabled() && *value != mine {
        *value = mine;
        resp.mark_changed();
    }
    resp
}

/// A chip: a small choice in a row of choices (tuplet ratios, time signature presets, MIDI Thru channels).
pub fn chip(ui: &mut Ui, on: bool, text: &str) -> Response {
    chip_sized(ui, on, text, 0.0, 24.0)
}

pub fn chip_sized(ui: &mut Ui, on: bool, text: &str, min_w: f32, h: f32) -> Response {
    let k = theme::t(ui);
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::font_medium(12.5), k.text);
    let w = (g.size().x + 18.0).max(min_w);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let hover = resp.hovered() && ui.is_enabled();
    let (fill, fg) = if on {
        (if hover { k.accent_hover } else { k.accent }, k.accent_fg)
    } else if resp.is_pointer_button_down_on() {
        (k.cell_press, k.text)
    } else if hover {
        (k.cell_hover, k.text)
    } else {
        (k.cell, k.text)
    };
    ui.painter().rect_filled(r, 6.0, disabled_alpha(ui, fill));
    ui.painter().galley(r.center() - g.size() / 2.0, g, disabled_alpha(ui, fg));
    resp
}

/// A row of a choice list (the New Score templates): blue when chosen.
pub fn list_row(ui: &mut Ui, on: bool, text: &str, width: f32) -> Response {
    let k = theme::t(ui);
    let (r, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if on {
        ui.painter().rect_filled(r, 5.0, k.accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 5.0, k.cell);
    }
    ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, text, theme::font(13.0), if on { k.accent_fg } else { k.text });
    resp
}

// ---------------- number fields and sliders ----------------

/// A number field with up and down arrows: type, drag or click the arrows (one step each).
pub fn num<T: Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>) -> Response {
    num_with(ui, value, range, 72.0, |d| d)
}

/// A number field of a given width; `custom` can add a formatter or a speed to the inner drag value.
pub fn num_with<T: Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>, width: f32, custom: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>) -> Response {
    let k = theme::t(ui);
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    let (r, _) = ui.allocate_exact_size(vec2(width, BTN_H), Sense::hover());
    let arrows = Rect::from_min_max(pos2(r.right() - 17.0, r.top() + 2.0), pos2(r.right() - 2.0, r.bottom() - 2.0));
    let field = Rect::from_min_max(r.min, pos2(arrows.left() - 1.0, r.bottom()));
    // the drag value without its own frame, inside the field's frame
    // a child Ui that doesn't move the parent's cursor (the whole field was allocated above)
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let mut resp = Some(&mut child)
        .map(|ui| {
            let v = ui.visuals_mut();
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
                w.weak_bg_fill = Color32::TRANSPARENT;
                w.bg_fill = Color32::TRANSPARENT;
                w.bg_stroke = Stroke::NONE;
            }
            v.text_edit_bg_color = Some(Color32::TRANSPARENT);
            v.selection.stroke = Stroke::NONE;
            ui.spacing_mut().button_padding = vec2(9.0, 3.0);
            ui.add_sized(field.size(), custom(egui::DragValue::new(value).range(range.clone())))
        })
        .unwrap();
    let focused = resp.has_focus() || resp.dragged();
    let p = ui.painter();
    p.rect(r, RADIUS, Color32::TRANSPARENT, Stroke::new(1.0, if focused { k.accent } else { k.line_2 }), egui::StrokeKind::Inside);
    if focused {
        p.rect_stroke(r.expand(2.0), RADIUS + 2.0, Stroke::new(3.0, k.focus), egui::StrokeKind::Middle);
    }
    p.line_segment([pos2(arrows.left() - 1.0, arrows.top() + 1.0), pos2(arrows.left() - 1.0, arrows.bottom() - 1.0)], Stroke::new(1.0, k.line));
    // the arrows: the upper half adds one, the lower half takes one away
    let up = Rect::from_min_max(arrows.min, pos2(arrows.right(), arrows.center().y));
    let down = Rect::from_min_max(pos2(arrows.left(), arrows.center().y), arrows.max);
    let ru = ui.interact(up, resp.id.with("up"), Sense::click());
    let rd = ui.interact(down, resp.id.with("down"), Sense::click());
    for (rr, re, dir) in [(up, &ru, -1.0f32), (down, &rd, 1.0)] {
        let c = rr.center();
        let col = if re.hovered() { k.text } else { k.text_2 };
        let s = Stroke::new(1.2, disabled_alpha(ui, col));
        p.line_segment([c + vec2(-2.8, 1.3 * -dir), c + vec2(0.0, -1.5 * -dir)], s);
        p.line_segment([c + vec2(0.0, -1.5 * -dir), c + vec2(2.8, 1.3 * -dir)], s);
    }
    let step = |v: &mut T, d: f64| {
        let n = (v.to_f64() + d).clamp(lo, hi);
        *v = T::from_f64(n);
    };
    if ui.is_enabled() {
        if ru.clicked() {
            step(value, 1.0);
            resp.mark_changed();
        }
        if rd.clicked() {
            step(value, -1.0);
            resp.mark_changed();
        }
    }
    resp
}

/// Shows and reads a 0-based measure index as 1-based (measure 1 is index 0).
pub fn from1(dv: egui::DragValue<'_>) -> egui::DragValue<'_> {
    dv.custom_formatter(|v, _| format!("{}", v as usize + 1)).custom_parser(|s| s.trim().parse::<f64>().ok().map(|v| v - 1.0))
}

/// A row of buttons along the bottom of a dialog, laid out from the right (the main button first).
pub fn footer<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    // as wide as the content above it (not the widest the window could be)
    let w = ui.min_rect().width().max(240.0);
    ui.allocate_ui_with_layout(vec2(w, BTN_H), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        add(ui)
    })
    .inner
}

/// A slider: 4 pt track, blue up to the 16 pt knob, the value on the right. Click or drag to set it.
pub fn slider<T: Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>, fmt: impl Fn(f64) -> String) -> Response {
    let k = theme::t(ui);
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    let w = 180.0;
    let (r, mut resp) = ui.allocate_exact_size(vec2(w, BTN_H), Sense::click_and_drag());
    let track = Rect::from_center_size(r.center(), vec2(w - 16.0, 4.0));
    if (resp.dragged() || resp.clicked()) && ui.is_enabled() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
            let mut v = lo + t * (hi - lo);
            if T::INTEGRAL {
                v = v.round();
            }
            let nv = T::from_f64(v);
            if nv.to_f64() != value.to_f64() {
                *value = nv;
                resp.mark_changed();
            }
        }
    }
    let t = if hi > lo { ((value.to_f64() - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let x = track.left() + t * track.width();
    let p = ui.painter();
    p.rect_filled(track, 2.0, disabled_alpha(ui, k.line));
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), 2.0, disabled_alpha(ui, k.accent));
    let knob = pos2(x, r.center().y);
    p.circle_filled(knob + vec2(0.0, 1.0), 8.5, theme::alpha(Color32::BLACK, 0.18));
    p.circle(knob, 8.0, disabled_alpha(ui, Color32::WHITE), Stroke::new(0.5, theme::alpha(Color32::BLACK, 0.3)), );
    let txt = fmt(value.to_f64());
    let (vr, _) = ui.allocate_exact_size(vec2(44.0, BTN_H), Sense::hover());
    ui.painter().text(pos2(vr.left() + 6.0, vr.center().y), Align2::LEFT_CENTER, txt, theme::font(13.0), disabled_alpha(ui, k.text));
    resp
}

// ---------------- layout: forms, groups, sections, help ----------------

/// A form: a two-column grid, labels on the left aligned to the right, controls on the right.
pub fn form<R>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 10.0]).show(ui, add).inner
}

/// The label cell of a form row (right-aligned, medium weight).
pub fn label(ui: &mut Ui, text: &str) {
    let k = theme::t(ui);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(egui::RichText::new(text).font(theme::font_medium(13.0)).color(disabled_alpha(ui, k.text)));
    });
}

/// One form row: label, then the controls laid out left to right.
pub fn row<R>(ui: &mut Ui, text: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    label(ui, text);
    let r = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        add(ui)
    });
    ui.end_row();
    r.inner
}

/// A small grey help line.
pub fn help(ui: &mut Ui, text: &str) {
    let k = theme::t(ui);
    wrapped(ui, egui::Label::new(egui::RichText::new(text).font(theme::font(11.5)).color(k.text_3)).wrap());
}

/// A wrapping label at most 560 pt wide, so a long line never stretches a window that sizes itself.
fn wrapped(ui: &mut Ui, label: egui::Label) {
    ui.scope(|ui| {
        ui.set_max_width(560.0f32.min(ui.available_width()));
        ui.add(label);
    });
}

/// Plain text in the dialog's text style.
pub fn text(ui: &mut Ui, text: &str) {
    wrapped(ui, egui::Label::new(egui::RichText::new(text).font(theme::font(13.0))).wrap());
}

/// A group with its label on the outline (9 pt corners).
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let k = theme::t(ui);
    ui.add_space(8.0);
    let inner = egui::Frame::new().stroke(Stroke::new(1.0, k.line)).corner_radius(9.0).inner_margin(egui::Margin { left: 14, right: 14, top: 18, bottom: 12 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        add(ui)
    });
    let r = inner.response.rect;
    let g = ui.painter().layout_no_wrap(title.to_string(), theme::font_semibold(11.5), k.text_2);
    let at = pos2(r.left() + 10.0, r.top() - g.size().y / 2.0);
    ui.painter().rect_filled(Rect::from_min_size(at - vec2(6.0, 0.0), g.size() + vec2(12.0, 0.0)), 0.0, k.panel);
    ui.painter().galley(at, g, k.text_2);
    inner.inner
}

/// A section that folds open and closed under a title with an arrow.
pub fn section(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, title: &str, add: impl FnOnce(&mut Ui)) {
    let k = theme::t(ui);
    let id = ui.make_persistent_id(id);
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let g = ui.painter().layout_no_wrap(title.to_string(), theme::font_medium(13.0), k.text);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 24.0), Sense::click());
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let c = pos2(r.left() + 6.0, r.center().y);
    let s = Stroke::new(1.4, k.text_2);
    if open {
        ui.painter().line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], s);
        ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], s);
    } else {
        ui.painter().line_segment([c + vec2(-1.5, -3.5), c + vec2(2.0, 0.0)], s);
        ui.painter().line_segment([c + vec2(2.0, 0.0), c + vec2(-1.5, 3.5)], s);
    }
    ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, k.text);
    if open {
        ui.indent(id.with("body"), add);
    }
}

/// A coloured dot with a line of text (the connection lines in MIDI Setup).
pub fn status_line(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 3.5, color);
        ui.label(egui::RichText::new(text).font(theme::font(13.0)));
    });
}

// ---------------- menus ----------------

/// The style of menus and their submenus: 24 pt items, blue highlight, no frames on the items.
pub fn menu_style(style: &mut egui::Style) {
    let k = theme::of(style.visuals.dark_mode);
    style.spacing.button_padding = vec2(24.0, 3.0);
    style.spacing.interact_size.y = 24.0;
    style.spacing.item_spacing.y = 0.0;
    style.spacing.menu_margin = egui::Margin::same(5);
    let w = &mut style.visuals.widgets;
    for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        v.bg_stroke = Stroke::NONE;
        v.corner_radius = egui::CornerRadius::same(5);
        v.expansion = 0.0;
    }
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.hovered.weak_bg_fill = k.accent;
    w.hovered.bg_fill = k.accent;
    w.hovered.fg_stroke = Stroke::new(1.0, k.accent_fg);
    w.active.weak_bg_fill = k.accent_hover;
    w.active.fg_stroke = Stroke::new(1.0, k.accent_fg);
    w.open.weak_bg_fill = k.accent;
    w.open.fg_stroke = Stroke::new(1.0, k.accent_fg);
    style.visuals.selection.bg_fill = k.accent;
    style.visuals.selection.stroke = Stroke::new(1.0, k.accent_fg);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(9);
    style.visuals.window_fill = k.panel;
}

/// The same look for the content of a pop-up list (combo boxes, the Voice pop-up).
pub fn menu_look(ui: &mut Ui) {
    menu_style(ui.style_mut());
    ui.spacing_mut().button_padding = vec2(10.0, 3.0);
}

/// A menu item: ✓ column on the left, label, shortcut in grey on the right; blue under the mouse.
pub fn menu_item(ui: &mut Ui, label: &str, shortcut: &str, checked: bool, enabled: bool) -> Response {
    let k = theme::t(ui);
    let lg = ui.painter().layout_no_wrap(label.to_string(), theme::font(13.0), k.text);
    let sg = ui.painter().layout_no_wrap(shortcut.to_string(), theme::font(12.5), k.text_3);
    let desired = 24.0 + lg.size().x + if shortcut.is_empty() { 16.0 } else { 28.0 + sg.size().x + 8.0 };
    let w = if ui.is_sizing_pass() { desired } else { ui.available_width().max(desired) };
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), sense);
    let hl = enabled && (resp.hovered() || resp.has_focus());
    let (fg, fg2) = if !enabled {
        (k.text_3, k.text_3)
    } else if hl {
        (k.accent_fg, k.accent_fg)
    } else {
        (k.text, k.text_3)
    };
    let p = ui.painter();
    if hl {
        p.rect_filled(r, 5.0, k.accent);
    }
    if checked {
        let c = pos2(r.left() + 12.0, r.center().y);
        let s = Stroke::new(1.6, fg);
        p.line_segment([c + vec2(-3.5, 0.0), c + vec2(-1.0, 2.6)], s);
        p.line_segment([c + vec2(-1.0, 2.6), c + vec2(3.8, -2.8)], s);
    }
    p.galley(pos2(r.left() + 24.0, r.center().y - lg.size().y / 2.0), lg, fg);
    if !shortcut.is_empty() {
        p.galley(pos2(r.right() - 8.0 - sg.size().x, r.center().y - sg.size().y / 2.0), sg, fg2);
    }
    resp
}

// ---------------- window frames ----------------

/// The frame of dialogs and floating windows: panel colour, 12 pt corners, soft shadow, no margin (the title
/// bar and the body set their own).
pub fn window_frame(ctx: &egui::Context) -> egui::Frame {
    let k = theme::ctx_tokens(ctx);
    let style = ctx.global_style();
    egui::Frame::new()
        .fill(k.panel)
        .corner_radius(12.0)
        .shadow(style.visuals.window_shadow)
        .stroke(Stroke::new(0.5, if k.dark { theme::alpha(Color32::WHITE, 0.14) } else { theme::alpha(Color32::BLACK, 0.18) }))
}

/// A title bar: the three window buttons on the left (the red one closes) and the centred title.
/// `height` is 32 for dialogs and 28 (with a line under it) for floating windows. Returns true when closed.
pub fn title_bar(ui: &mut Ui, title: &str, width: f32, height: f32, line: bool) -> bool {
    let k = theme::t(ui);
    let (r, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    let y = r.center().y;
    let mut closed = false;
    for (n, c) in [TL_RED, TL_YELLOW, TL_GREEN].into_iter().enumerate() {
        let ctr = pos2(r.left() + 14.0 + n as f32 * 18.0, y);
        if n == 0 {
            let resp = ui.interact(Rect::from_center_size(ctr, vec2(14.0, 14.0)), ui.next_auto_id().with("close"), Sense::click());
            p.circle(ctr, 6.0, c, Stroke::new(0.5, theme::alpha(Color32::BLACK, 0.2)));
            if resp.hovered() {
                let s = Stroke::new(1.2, Color32::from_rgb(0x7A, 0x10, 0x0A));
                p.line_segment([ctr + vec2(-2.5, -2.5), ctr + vec2(2.5, 2.5)], s);
                p.line_segment([ctr + vec2(-2.5, 2.5), ctr + vec2(2.5, -2.5)], s);
            }
            closed = resp.clicked();
        } else {
            p.circle(ctr, 6.0, c, Stroke::new(0.5, theme::alpha(Color32::BLACK, 0.2)));
        }
    }
    p.text(pos2(r.center().x, y), Align2::CENTER_CENTER, title, theme::font_semibold(if height < 30.0 { 12.0 } else { 13.0 }), k.text);
    if line {
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, k.line));
    }
    closed
}

/// A floating window (Staff Sheet, Keyboard, Tempo): frame, 28 pt title bar with a line, then the content.
pub fn float_window(ctx: &egui::Context, title: &str, open: &mut bool, default_pos: egui::Pos2, add: impl FnOnce(&mut Ui)) {
    egui::Window::new(title).id(egui::Id::unique(("float", title))).title_bar(false).resizable(false).auto_sized().default_pos(default_pos).frame(window_frame(ctx)).show(ctx, |ui| {
        let w = ui.min_rect().width().max(ui.available_width().min(1400.0));
        let width = ui.memory(|m| m.data.get_temp::<f32>(egui::Id::unique(("float_w", title)))).unwrap_or(w);
        if title_bar(ui, title, width, 28.0, true) {
            *open = false;
        }
        let inner = egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 14 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            add(ui)
        });
        let wnow = inner.response.rect.width();
        ui.memory_mut(|m| m.data.insert_temp(egui::Id::unique(("float_w", title)), wnow));
    });
}

/// The square icon of an alert: a note in a soft blue square, or a warning sign.
pub fn alert_icon(ui: &mut Ui, warn: bool) {
    let k = theme::t(ui);
    let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
    let (bg, fg) = if warn { (theme::alpha(k.warn, 0.18), k.warn) } else { (k.accent_soft, k.accent) };
    ui.painter().rect_filled(r, 10.0, bg);
    if warn {
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "⚠", theme::font_semibold(22.0), fg);
    } else {
        let font = egui::FontId::new(26.0, egui::FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into()));
        // a quarter note (SMuFL noteQuarterUp), centred by eye in the square
        ui.painter().text(r.center() + vec2(-3.0, 9.0), Align2::CENTER_CENTER, "\u{E1D5}", font, fg);
    }
}

/// The tip shown under the mouse: the name and each key in a key cap (delay 0.5 s, from the style).
pub fn tip(resp: Response, name: &str, keys: &str) -> Response {
    crate::ui::tip(resp, name, keys)
}

/// Splits a tip like "Sharp (S)" into its name and key ("Sharp", "S"); tips without a key stay whole.
pub fn split_tip(t: &str) -> (&str, &str) {
    if let (Some(open), true) = (t.rfind(" ("), t.ends_with(')')) {
        let key = &t[open + 2..t.len() - 1];
        if !key.is_empty() && key.len() <= 12 && !key.contains(' ') || key.starts_with("Shift+") {
            return (&t[..open], key);
        }
    }
    (t, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_split_name_and_key() {
        assert_eq!(split_tip("Sharp (S)"), ("Sharp", "S"));
        assert_eq!(split_tip("Double sharp (Shift+S)"), ("Double sharp", "Shift+S"));
        assert_eq!(split_tip("Quarter note (3)"), ("Quarter note", "3"));
        assert_eq!(split_tip("Slur (drag from note to note)"), ("Slur (drag from note to note)", ""));
        assert_eq!(split_tip("Color (applies to selection)"), ("Color (applies to selection)", ""));
        assert_eq!(split_tip("Tempo"), ("Tempo", ""));
    }
}
