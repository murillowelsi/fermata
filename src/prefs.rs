//! Settings kept between sessions (MIDI Setup and recording options), in the user's settings folder.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub out_port_a: Option<String>,
    #[serde(default)]
    pub out_port_b: Option<String>,
    #[serde(default)]
    pub transmit_sync: [bool; 2],
    #[serde(default)]
    pub record_port: Option<String>,
    #[serde(default)]
    pub receive_sync_port: Option<String>,
    /// MIDI Thru: follow the current staff's port and channel (true) or always send to a fixed one.
    #[serde(default = "yes")]
    pub thru_follow_staff: bool,
    #[serde(default)]
    pub thru_port: u8,
    #[serde(default)]
    pub thru_channel: u8,
    #[serde(default = "yes")]
    pub thru_on: bool,
    #[serde(default)]
    pub sync_external: bool,
    #[serde(default = "q16")]
    pub rec_quantize: u8,
    #[serde(default = "c4")]
    pub rec_split: u8,
    #[serde(default = "one")]
    pub rec_count_in: u8,
    #[serde(default = "yes")]
    pub rec_click: bool,
    /// Click Settings (the original's dialog 1234): bar and beat click sounds, and "Only Use Click When Recording".
    #[serde(default = "bar_click_ch1")]
    pub click_bar: ClickSound,
    #[serde(default = "beat_click_ch1")]
    pub click_beat: ClickSound,
    #[serde(default)]
    pub click_only_rec: bool,
    /// Click On/Off (Setup menu, toolbar metronome button), kept between sessions.
    #[serde(default = "yes")]
    pub click_on: bool,
    /// Send everything to the instrument on channel 1 without changing its voice: for pianos that only play
    /// channel 1 (a Clavinova in MIDI Accord mode receives CH1 = KEYBOARD, every other channel OFF).
    #[serde(default = "yes")]
    pub single_channel: bool,
    /// View ▸ Appearance: follow macOS, or always light or dark (F-15). Older files without it follow macOS.
    #[serde(default)]
    pub appearance: Appearance,
    /// Settings format; older files get the new click defaults.
    #[serde(default)]
    pub version: u32,
}

/// The app's light or dark look: as macOS is set (the default), or always one of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Appearance::System => egui::ThemePreference::System,
            Appearance::Light => egui::ThemePreference::Light,
            Appearance::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClickSound {
    /// 0 = port A, 1 = port B, 2 = the computer's own sound.
    pub port: u8,
    /// MIDI channel 1..16.
    pub chan: u8,
    pub pitch: u8,
    pub vel: u8,
    pub dur_ms: u16,
    /// Instrument for the click on a port (General MIDI program, 0-based; 115 = Woodblock).
    #[serde(default = "woodblock")]
    pub program: u8,
}

fn woodblock() -> u8 {
    115
}

/// Defaults: the click plays on port A (the piano) with the music, on the drum channel 10 with the piano's own
/// metronome sounds (Yamaha drum kits: 34 = Metronome Bell for the bar, 33 = Metronome Click for the beats).
/// Channel 10 is played both in a Clavinova's Song setup and in its MIDI Accord setup (Style Rhythm 2), and the
/// piano's own kit is left as it is. When port A isn't connected the click plays on the computer.
pub fn bar_click() -> ClickSound {
    ClickSound { port: 0, chan: 10, pitch: 34, vel: 127, dur_ms: 100, program: 115 }
}
pub fn beat_click() -> ClickSound {
    ClickSound { port: 0, chan: 10, pitch: 33, vel: 110, dur_ms: 100, program: 115 }
}

/// Click on channel 1 in the piano's own voice: a short high C7 on the bar and G6 on the beats.
pub fn bar_click_ch1() -> ClickSound {
    ClickSound { port: 0, chan: 1, pitch: 96, vel: 110, dur_ms: 60, program: 115 }
}
pub fn beat_click_ch1() -> ClickSound {
    ClickSound { port: 0, chan: 1, pitch: 91, vel: 80, dur_ms: 60, program: 115 }
}

fn yes() -> bool {
    true
}
fn q16() -> u8 {
    5
}
fn c4() -> u8 {
    60
}
fn one() -> u8 {
    1
}

impl Default for Prefs {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// The folder the settings live in.
pub fn dir() -> Option<PathBuf> {
    path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let base = PathBuf::from(home);
    let dir = if cfg!(target_os = "macos") {
        base.join("Library").join("Application Support").join("Fermata")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(base).join("Fermata")
    } else {
        base.join(".config").join("fermata")
    };
    Some(dir.join("prefs.json"))
}

pub const VERSION: u32 = 4;

pub fn load() -> Prefs {
    let mut p = load_raw();
    if p.version < VERSION {
        // earlier versions sent the click to the computer, then to channel 16; it now uses the drum channel
        // a Clavinova in MIDI Accord mode only plays channel 1: send everything there, click included
        p.single_channel = true;
        p.click_bar = bar_click_ch1();
        p.click_beat = beat_click_ch1();
        p.click_on = true;
        p.rec_click = true;
        p.version = VERSION;
        save(&p);
    }
    p
}

/// The saved settings: a file on the desktop, the page's localStorage in the browser.
fn load_raw() -> Prefs {
    #[cfg(not(target_arch = "wasm32"))]
    {
        path().map(|p| load_from(&p)).unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_storage().and_then(|s| s.get_item(WEB_KEY).ok().flatten()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
}

#[cfg(target_arch = "wasm32")]
const WEB_KEY: &str = "fermata.prefs";

#[cfg(target_arch = "wasm32")]
fn web_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// The settings in a file (defaults when it's missing or unreadable).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn load_from(path: &std::path::Path) -> Prefs {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(p: &Prefs) {
    #[cfg(target_arch = "wasm32")]
    {
        if let (Some(s), Ok(j)) = (web_storage(), serde_json::to_string(p)) {
            let _ = s.set_item(WEB_KEY, &j);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = path() {
        save_to(p, &path);
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn save_to(p: &Prefs, path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(path, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_survives_save_and_reopen() {
        let dir = std::env::temp_dir().join(format!("fermata-prefs-test-{}", std::process::id()));
        let file = dir.join("prefs.json");
        for a in [Appearance::Dark, Appearance::Light, Appearance::System] {
            let p = Prefs { appearance: a, ..Prefs::default() };
            save_to(&p, &file);
            assert_eq!(load_from(&file).appearance, a);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn older_settings_without_appearance_follow_the_system() {
        // a settings file from 0.3.1 and before: no "appearance" field
        let old = r#"{"out_port_a":null,"thru_on":true,"click_on":true,"single_channel":true,"version":4}"#;
        let p: Prefs = serde_json::from_str(old).unwrap();
        assert_eq!(p.appearance, Appearance::System);
        assert_eq!(Prefs::default().appearance, Appearance::System);
        assert_eq!(p.appearance.preference(), egui::ThemePreference::System);
    }

    #[test]
    fn appearance_is_written_by_name() {
        let s = serde_json::to_string(&Prefs { appearance: Appearance::Dark, ..Prefs::default() }).unwrap();
        assert!(s.contains("\"appearance\":\"Dark\""), "{s}");
    }
}
