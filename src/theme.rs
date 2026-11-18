//! The app's look: colour tokens for light and dark mode, the Inter interface font and egui's global style.
//!
//! Every colour outside the score page comes from here. The page itself is always white with black ink, in both
//! modes; the theme never reaches inside it.
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Theme, ThemePreference, Ui, Visuals};
use std::sync::Arc;

/// The colours of one mode (light or dark). Names follow the design spec (F-13, direction A).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub accent: Color32,
    pub accent_fg: Color32,
    pub accent_soft: Color32,
    pub accent_hover: Color32,
    pub focus: Color32,
    pub panel: Color32,
    pub panel_2: Color32,
    pub field: Color32,
    pub win: Color32,
    pub canvas: Color32,
    pub line: Color32,
    pub line_2: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub text_3: Color32,
    pub danger: Color32,
    pub danger_fg: Color32,
    pub warn: Color32,
    /// "Connected" in the status bar and MIDI Setup (not in the spec's table; a green that reads in both modes).
    pub ok: Color32,
    pub cell: Color32,
    pub cell_hover: Color32,
    pub cell_press: Color32,
    pub pal_bg: Color32,
    /// The dot in a palette's header.
    pub pal_dot: Color32,
    pub tip_bg: Color32,
    pub tip_fg: Color32,
    pub shadow: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Black or white at a given opacity (0..1), premultiplied as egui wants it.
const fn black(a: f32) -> Color32 {
    Color32::from_black_alpha((a * 255.0 + 0.5) as u8)
}
const fn white(a: f32) -> Color32 {
    let v = (a * 255.0 + 0.5) as u8;
    Color32::from_rgba_premultiplied(v, v, v, v)
}

/// A colour at a given opacity (0..1).
pub fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

pub const LIGHT: Tokens = Tokens {
    dark: false,
    accent: rgb(0x007AFF),
    accent_fg: rgb(0xFFFFFF),
    accent_soft: rgb(0xDDEBFF),
    accent_hover: rgb(0x0069DB),
    focus: Color32::from_rgba_premultiplied(0, 43, 89, 89), // rgba(0,122,255,.35)
    panel: rgb(0xF7F7F5),
    panel_2: rgb(0xEEEEEB),
    field: rgb(0xFFFFFF),
    win: rgb(0xF2F2F0),
    canvas: rgb(0xD2D2CE),
    line: rgb(0xD3D3D0),
    line_2: rgb(0xBDBDB9),
    text: rgb(0x1D1D1F),
    text_2: rgb(0x5C5C60),
    text_3: rgb(0x8A8A8E),
    danger: rgb(0xC8372D),
    danger_fg: rgb(0xFFFFFF),
    warn: rgb(0xB7791F),
    ok: rgb(0x1F8A3B),
    cell: black(0.045),
    cell_hover: black(0.10),
    cell_press: black(0.15),
    pal_bg: Color32::from_rgba_premultiplied(240, 240, 241, 245), // rgba(250,250,251,.96)
    pal_dot: rgb(0xC9C9CD),
    tip_bg: rgb(0xFFFFFF),
    tip_fg: rgb(0x1D1D1F),
    shadow: black(0.20),
};

pub const DARK: Tokens = Tokens {
    dark: true,
    accent: rgb(0x0A84FF),
    accent_fg: rgb(0xFFFFFF),
    accent_soft: rgb(0x173451),
    accent_hover: rgb(0x2A95FF),
    focus: Color32::from_rgba_premultiplied(5, 59, 115, 115), // rgba(10,132,255,.45)
    panel: rgb(0x2C2C2F),
    panel_2: rgb(0x333336),
    field: rgb(0x1E1E20),
    win: rgb(0x29292B),
    canvas: rgb(0x19191B),
    line: rgb(0x3E3E41),
    line_2: rgb(0x505054),
    text: rgb(0xECECEE),
    text_2: rgb(0xABABB0),
    text_3: rgb(0x7D7D82),
    danger: rgb(0xFF6B5E),
    danger_fg: rgb(0x2A0703),
    warn: rgb(0xE9B44C),
    ok: rgb(0x4CC764),
    cell: white(0.06),
    cell_hover: white(0.12),
    cell_press: white(0.17),
    pal_bg: Color32::from_rgba_premultiplied(42, 42, 45, 245), // rgba(44,44,47,.96)
    pal_dot: rgb(0x5A5A5E),
    tip_bg: rgb(0x3A3A3D),
    tip_fg: rgb(0xF2F2F4),
    shadow: black(0.60),
};

impl Tokens {
    /// The palette panel's colour without its slight transparency.
    pub fn pal_bg_opaque(&self) -> Color32 {
        let a = self.pal_bg.a().max(1) as f32 / 255.0;
        let u = |v: u8| (v as f32 / a).round().min(255.0) as u8;
        Color32::from_rgb(u(self.pal_bg.r()), u(self.pal_bg.g()), u(self.pal_bg.b()))
    }
}

/// The score page: always white paper with black ink, whatever the mode.
pub const PAGE: Color32 = Color32::WHITE;

/// The Keyboard window's piano keys: a picture of a real keyboard, the same in both modes.
pub const PIANO_WHITE: Color32 = Color32::WHITE;
pub const PIANO_BLACK: Color32 = rgb(0x1D1D1F);
pub const PIANO_LINE: Color32 = rgb(0x8A8A8E);

/// The tokens of the mode a `Ui` is drawn in.
pub fn t(ui: &Ui) -> &'static Tokens {
    of(ui.visuals().dark_mode)
}

/// The tokens of the mode the app is in now.
pub fn ctx_tokens(ctx: &egui::Context) -> &'static Tokens {
    of(ctx.global_style().visuals.dark_mode)
}

pub fn of(dark: bool) -> &'static Tokens {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

// ---------------- fonts ----------------

#[cfg(not(target_arch = "wasm32"))]
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
#[cfg(not(target_arch = "wasm32"))]
const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
#[cfg(not(target_arch = "wasm32"))]
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
// the browser downloads the app: Inter cut to Latin, punctuation, arrows and the key symbols (tools/subset_web_fonts.sh)
#[cfg(target_arch = "wasm32")]
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/web/Inter-Regular.ttf");
#[cfg(target_arch = "wasm32")]
const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/web/Inter-Medium.ttf");
#[cfg(target_arch = "wasm32")]
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/web/Inter-SemiBold.ttf");

pub const REGULAR: &str = "inter";
pub const MEDIUM: &str = "inter-medium";
pub const SEMIBOLD: &str = "inter-semibold";

/// Interface text in Inter Regular.
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(REGULAR.into()))
}
/// Interface text in Inter Medium (buttons, toolbar labels).
pub fn font_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}
/// Interface text in Inter SemiBold (titles, group and palette labels).
pub fn font_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Inter in three weights as named families. The `Proportional` family is left as it was, because the score page
/// draws its text with it and the theme must not change the page.
fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let fallback: Vec<String> = f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, data) in [("Inter-Regular", INTER_REGULAR), ("Inter-Medium", INTER_MEDIUM), ("Inter-SemiBold", INTER_SEMIBOLD)] {
        f.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    }
    for (family, data) in [(REGULAR, "Inter-Regular"), (MEDIUM, "Inter-Medium"), (SEMIBOLD, "Inter-SemiBold")] {
        let mut list = vec![data.to_owned()];
        list.extend(fallback.iter().cloned());
        f.families.insert(FontFamily::Name(family.into()), list);
    }
    // Bravura (SMuFL) for the palettes' music symbols
    f.font_data.insert("Bravura".to_owned(), Arc::new(FontData::from_static(crate::palettes::BRAVURA)));
    f.families.insert(FontFamily::Name(crate::palettes::BRAVURA_FAMILY.into()), vec!["Bravura".to_owned()]);
    f
}

// ---------------- egui style ----------------

fn shadow(blur: u8, y: i8, c: Color32) -> Shadow {
    Shadow { offset: [0, y], blur, spread: 0, color: c }
}

/// egui's widget colours built from one mode's tokens.
pub fn visuals(k: &Tokens) -> Visuals {
    let mut v = if k.dark { Visuals::dark() } else { Visuals::light() };
    let r7 = CornerRadius::same(7);
    v.override_text_color = None;
    v.panel_fill = k.win;
    v.window_fill = k.panel;
    v.window_stroke = Stroke::new(1.0, k.line);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(9);
    v.window_shadow = shadow(30, 10, k.shadow);
    v.popup_shadow = shadow(18, 6, k.shadow);
    v.extreme_bg_color = k.field;
    v.text_edit_bg_color = Some(k.field);
    v.faint_bg_color = k.cell;
    v.code_bg_color = k.panel_2;
    v.hyperlink_color = k.accent;
    v.warn_fg_color = k.warn;
    v.error_fg_color = k.danger;
    v.selection.bg_fill = k.accent;
    // the outline of a focused field (selected items in lists and menus set their own white text)
    v.selection.stroke = Stroke::new(1.0, k.accent);
    v.text_cursor.stroke = Stroke::new(2.0, k.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = k.panel;
    w.noninteractive.weak_bg_fill = k.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, k.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, k.text);
    w.noninteractive.corner_radius = r7;

    w.inactive.bg_fill = k.field;
    w.inactive.weak_bg_fill = k.field;
    w.inactive.bg_stroke = Stroke::new(1.0, k.line_2);
    w.inactive.fg_stroke = Stroke::new(1.0, k.text);
    w.inactive.corner_radius = r7;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = k.field;
    w.hovered.weak_bg_fill = blend(k.field, k.cell);
    w.hovered.bg_stroke = Stroke::new(1.0, k.line_2);
    w.hovered.fg_stroke = Stroke::new(1.5, k.text);
    w.hovered.corner_radius = r7;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = blend(k.field, k.cell_hover);
    w.active.weak_bg_fill = blend(k.field, k.cell_hover);
    w.active.bg_stroke = Stroke::new(1.0, k.accent);
    w.active.fg_stroke = Stroke::new(2.0, k.text);
    w.active.corner_radius = r7;
    w.active.expansion = 0.0;

    w.open.bg_fill = k.field;
    w.open.weak_bg_fill = blend(k.field, k.cell);
    w.open.bg_stroke = Stroke::new(1.0, k.line_2);
    w.open.fg_stroke = Stroke::new(1.0, k.text);
    w.open.corner_radius = r7;
    v
}

/// `top` laid over `base` (top is premultiplied, as egui keeps colours).
pub fn blend(base: Color32, top: Color32) -> Color32 {
    let a = top.a() as f32 / 255.0;
    let mix = |b: u8, t: u8| (t as f32 + b as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(base.r(), top.r()), mix(base.g(), top.g()), mix(base.b(), top.b()))
}

fn style_for(style: &mut egui::Style, k: &Tokens) {
    style.visuals = visuals(k);
    let ts = &mut style.text_styles;
    ts.insert(TextStyle::Body, font(13.0));
    ts.insert(TextStyle::Button, font(13.0));
    ts.insert(TextStyle::Small, font(11.5));
    ts.insert(TextStyle::Heading, font_semibold(13.0));
    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(8.0, 6.0);
    sp.button_padding = egui::vec2(10.0, 3.0);
    sp.interact_size.y = 26.0;
    sp.window_margin = Margin::same(12);
    sp.menu_margin = Margin::same(5);
}

/// Installs the fonts and both styles, and follows the system's light or dark mode.
/// `force` pins one mode (used by `--theme light|dark` for screenshots).
pub fn install(ctx: &egui::Context, force: Option<Theme>) {
    ctx.set_fonts(fonts());
    ctx.style_mut_of(Theme::Light, |s| style_for(s, &LIGHT));
    ctx.style_mut_of(Theme::Dark, |s| style_for(s, &DARK));
    ctx.set_theme(match force {
        Some(t) => ThemePreference::from(t),
        None => ThemePreference::System,
    });
}

// ---------------- shortcuts as Mac key symbols ----------------

/// The key part of a shortcut as a Mac menu shows it ("T", "[", "⌫", "F4").
fn key_glyph(k: egui::Key) -> String {
    use egui::Key::*;
    match k {
        OpenBracket => "[".into(),
        CloseBracket => "]".into(),
        Equals => "=".into(),
        Minus => "-".into(),
        Semicolon => ";".into(),
        Quote => "'".into(),
        Slash => "/".into(),
        Period => ".".into(),
        Comma => ",".into(),
        Backspace => "⌫".into(),
        Delete => "⌦".into(),
        Enter => "↩".into(),
        Escape => "Esc".into(),
        Tab => "⇥".into(),
        Space => "Space".into(),
        ArrowLeft => "←".into(),
        ArrowRight => "→".into(),
        ArrowUp => "↑".into(),
        ArrowDown => "↓".into(),
        Insert => "Ins".into(),
        k => k.name().trim_start_matches("Num").to_string(),
    }
}

/// One key combination: the macOS symbols in the order ⌃⌥⇧⌘ on the Mac ("⌃⇧T", "⌥⇧N", "⌘Q"); words elsewhere
/// ("Ctrl+Shift+T", "Alt+Shift+N"), as Windows menus show them.
pub fn combo_glyphs(s: &egui::KeyboardShortcut) -> String {
    let m = s.modifiers;
    if !crate::commands::on_mac() {
        let mut t = String::new();
        for (on, w) in [(m.ctrl || (m.command && !m.mac_cmd), "Ctrl+"), (m.alt, "Alt+"), (m.shift, "Shift+")] {
            if on {
                t.push_str(w);
            }
        }
        let k = match s.logical_key {
            egui::Key::Backspace => "Backspace".to_string(),
            egui::Key::Delete => "Del".to_string(),
            egui::Key::Enter => "Enter".to_string(),
            k => key_glyph(k),
        };
        return t + &k;
    }
    let mut t = String::new();
    if m.ctrl {
        t.push('⌃');
    }
    if m.alt {
        t.push('⌥');
    }
    if m.shift {
        t.push('⇧');
    }
    if m.mac_cmd || (m.command && !m.ctrl) {
        t.push('⌘');
    }
    t + &key_glyph(s.logical_key)
}

/// Plain-key shortcuts ("Space", "Shift+Z") written with the same symbols ("Space", "⇧Z"); as words off the Mac.
fn plain_glyphs(s: &str) -> String {
    if !crate::commands::on_mac() {
        return s.to_string();
    }
    match s {
        "Enter" => "↩".into(),
        _ => s.replace("Shift+", "⇧").replace("Alt+", "⌥"),
    }
}

/// What menus, tips and Keyboard Shortcuts show for a command. Ctrl is the main key on the Mac too, so a command
/// that has Ctrl shows only the Ctrl combination (its Cmd alternative still works but isn't shown); ⌘ appears only
/// for a command with no Ctrl combination. Only the text changes; the key bindings stay in `commands.rs`.
pub fn shortcut_glyphs(c: crate::commands::Cmd) -> String {
    match crate::commands::shortcut(c) {
        Some(s) => combo_glyphs(&s),
        None => crate::commands::plain_key_text(c).map(plain_glyphs).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::commands::{self, Cmd};

    #[test]
    #[cfg(target_os = "macos")]
    fn spec_examples() {
        // §3 of the spec: Ctrl+T / Cmd+T -> ⌃T; Ctrl+Shift+T -> ⌃⇧T; Ctrl+Alt+X -> ⌃⌥X; Alt+Shift+N -> ⌥⇧N
        assert_eq!(shortcut_glyphs(Cmd::Tie), "⌃T");
        assert_eq!(shortcut_glyphs(Cmd::FlipTie), "⌃⇧T");
        assert_eq!(shortcut_glyphs(Cmd::ChangePitch), "⌃⌥X");
        assert_eq!(shortcut_glyphs(Cmd::Palette(0)), "⌥⇧N");
        assert_eq!(shortcut_glyphs(Cmd::Play), "Space");
        assert_eq!(shortcut_glyphs(Cmd::ZoomOut), "⇧Z");
        assert_eq!(shortcut_glyphs(Cmd::Record), "↩");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn ctrl_commands_never_show_cmd() {
        for c in commands::all_shortcut_cmds() {
            let Some(s) = commands::shortcut(c) else { continue };
            let g = shortcut_glyphs(c);
            if s.modifiers.ctrl {
                assert!(g.starts_with('⌃'), "{c:?}: {g}");
                assert!(!g.contains('⌘'), "{c:?} has Ctrl but shows ⌘: {g}");
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn modifier_order_is_macos() {
        let s = egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::ALT | egui::Modifiers::SHIFT | egui::Modifiers::MAC_CMD, egui::Key::M);
        assert_eq!(combo_glyphs(&s), "⌃⌥⇧⌘M");
    }

    #[test]
    fn accent_is_blue_in_both_modes() {
        assert_eq!(LIGHT.accent, Color32::from_rgb(0x00, 0x7A, 0xFF));
        assert_eq!(DARK.accent, Color32::from_rgb(0x0A, 0x84, 0xFF));
    }

    /// WCAG contrast of interface text on its usual backgrounds.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        };
        let (x, y) = (lum(a) + 0.05, lum(b) + 0.05);
        x.max(y) / x.min(y)
    }

    #[test]
    fn text_reads_on_panels() {
        for k in [&LIGHT, &DARK] {
            for bg in [k.win, k.panel, k.field] {
                assert!(contrast(k.text, bg) >= 7.0, "text on {bg:?}");
                assert!(contrast(k.text_2, bg) >= 4.5, "text-2 on {bg:?}");
            }
            assert!(contrast(k.accent, k.accent_soft) >= 3.0, "active tool icon");
        }
    }
}
