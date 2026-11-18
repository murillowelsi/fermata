//! Touch screens (F-25, the browser version only): which layout the device gets, and the rules of the fingers.
//! - A phone (a touch screen whose shorter side is under 600 pt) gets the reader: the page, playback, open and
//!   export, no editing (`reader.rs`).
//! - A tablet (a touch screen, shorter side 600 pt or more) gets the editor with 44 pt targets, the palettes in a
//!   rail on the left, and the gestures below.
//! - Anything with a mouse or trackpad (also an iPad with a trackpad: the browser then reports a fine pointer)
//!   keeps the desktop layout. The desktop apps are always `Desktop`.
//!
//! Gestures on the score (tablet): one finger uses the active tool; two fingers always scroll and zoom and never
//! write. If the second finger lands within `CANCEL_WINDOW` of the first, whatever the first finger started is
//! undone without an undo step. A press held `LONG_PRESS` seconds counts as a right click (egui reports it through
//! `Response::secondary_clicked`).

/// The three layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Desktop,
    /// A tablet: the editor, made for fingers.
    Touch,
    /// A phone: the reader.
    Reader,
}

/// Below this shorter side (in points) a touch screen is a phone.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub const PHONE_MAX_SIDE: f32 = 600.0;
/// A second finger this soon after the first (seconds) cancels what the first finger started.
pub const CANCEL_WINDOW: f64 = 0.12;
/// A press held this long (seconds) is a right click.
pub const LONG_PRESS: f64 = 0.5;

/// The layout for a screen: `coarse` when the main pointer is a finger (`(pointer: coarse)`), `w` × `h` the
/// window in points. Asked again whenever the window turns or changes size.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn classify(coarse: bool, w: f32, h: f32) -> Device {
    if !coarse {
        Device::Desktop
    } else if w.min(h) < PHONE_MAX_SIDE {
        Device::Reader
    } else {
        Device::Touch
    }
}

/// What the score should do with the first finger's action this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerAct {
    /// Nothing special: one finger, the tool works as with a mouse.
    Normal,
    /// The second finger came within the cancel window: undo what the first one started (no undo step).
    Cancel,
    /// The second finger came later: what the first finger did stays, the press ends there.
    Stop,
}

/// Follows one press on the score: when it started, and whether a second finger joined it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gesture {
    /// When the first finger went down (input time, seconds).
    pub start: Option<f64>,
    /// A second finger joined this press: until every finger is up, the fingers only scroll and zoom.
    pub multi: bool,
}

impl Gesture {
    /// One frame. `pressed`: the first finger went down this frame; `fingers`: how many touch the screen now;
    /// `now`: input time. Returns what to do with the first finger's action, and whether the tool must ignore the
    /// pointer this frame (true while two fingers are or were part of the press).
    pub fn step(&mut self, now: f64, pressed: bool, fingers: usize) -> (FingerAct, bool) {
        if pressed && !self.multi {
            self.start = Some(now);
        }
        let mut act = FingerAct::Normal;
        if fingers >= 2 && !self.multi {
            self.multi = true;
            let soon = self.start.is_some_and(|t| now - t <= CANCEL_WINDOW);
            act = if soon { FingerAct::Cancel } else { FingerAct::Stop };
        }
        let blocked = self.multi;
        if fingers == 0 && !pressed {
            // every finger is up: the next press starts fresh
            *self = Gesture::default();
        }
        (act, blocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_tablet_and_desktop() {
        // iPhone upright and on its side, iPad both ways, a laptop, and an iPad with a trackpad (fine pointer)
        assert_eq!(classify(true, 390.0, 844.0), Device::Reader);
        assert_eq!(classify(true, 844.0, 390.0), Device::Reader);
        assert_eq!(classify(true, 820.0, 1180.0), Device::Touch);
        assert_eq!(classify(true, 1180.0, 820.0), Device::Touch);
        assert_eq!(classify(false, 1440.0, 900.0), Device::Desktop);
        assert_eq!(classify(false, 820.0, 1180.0), Device::Desktop);
        assert_eq!(classify(false, 390.0, 844.0), Device::Desktop);
        // the limit: 600 is already a tablet
        assert_eq!(classify(true, 599.0, 900.0), Device::Reader);
        assert_eq!(classify(true, 600.0, 900.0), Device::Touch);
    }

    #[test]
    fn one_finger_is_a_normal_press() {
        let mut g = Gesture::default();
        assert_eq!(g.step(1.0, true, 1), (FingerAct::Normal, false));
        assert_eq!(g.step(1.2, false, 1), (FingerAct::Normal, false));
        assert_eq!(g.step(1.3, false, 0), (FingerAct::Normal, false));
    }

    #[test]
    fn a_second_finger_within_120_ms_cancels() {
        let mut g = Gesture::default();
        g.step(1.0, true, 1);
        assert_eq!(g.step(1.1, false, 2), (FingerAct::Cancel, true));
        // the press stays a scroll and zoom until every finger is up, also after the second finger leaves
        assert_eq!(g.step(1.3, false, 2), (FingerAct::Normal, true));
        assert_eq!(g.step(1.4, false, 1), (FingerAct::Normal, true));
        assert_eq!(g.step(1.5, false, 0), (FingerAct::Normal, true));
        // then a new press is a normal one again
        assert_eq!(g.step(2.0, true, 1), (FingerAct::Normal, false));
    }

    #[test]
    fn a_later_second_finger_stops_without_cancelling() {
        let mut g = Gesture::default();
        g.step(1.0, true, 1);
        assert_eq!(g.step(1.3, false, 2), (FingerAct::Stop, true));
    }

    #[test]
    fn two_fingers_landing_together_cancel() {
        let mut g = Gesture::default();
        assert_eq!(g.step(1.0, true, 2), (FingerAct::Cancel, true));
    }

    #[test]
    fn the_mouse_never_cancels() {
        // a mouse reports no fingers: presses never become gestures
        let mut g = Gesture::default();
        assert_eq!(g.step(1.0, true, 0), (FingerAct::Normal, false));
        assert_eq!(g.step(1.05, false, 0), (FingerAct::Normal, false));
    }
}
