//! F-42: a tablet pen (Wacom and the like) on macOS. winit hands it to egui as a plain mouse, but AppKit marks its
//! events: a mouse event from the pen has the subtype `TabletPoint`, and a `TabletProximity` event says a pen
//! (`pointingDeviceType` Pen) came near the tablet or left it. A local event monitor reads those marks, outside winit,
//! and lets every event through unchanged; the pen frame then takes that press, and its drag, as the pen.
use block2::RcBlock;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType, NSPointingDeviceType};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

struct State {
    /// The last left-button press came from the pen.
    press_is_pen: AtomicBool,
    /// A pen is near the tablet.
    pen_near: AtomicBool,
    /// A pen was seen since the app started.
    seen: AtomicBool,
}

impl State {
    const fn new() -> State {
        State { press_is_pen: AtomicBool::new(false), pen_near: AtomicBool::new(false), seen: AtomicBool::new(false) }
    }
}

static STATE: State = State::new();
static INSTALL: Once = Once::new();

/// Starts listening (once; on the main thread, after the app's window exists).
pub fn install() {
    INSTALL.call_once(|| {
        let bit = |t: NSEventType| 1u64 << t.0;
        let mask = NSEventMask(bit(NSEventType::LeftMouseDown) | bit(NSEventType::TabletProximity) | bit(NSEventType::TabletPoint));
        let block = RcBlock::new(|e: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands the monitor a valid event for the duration of the call
            let ev = unsafe { e.as_ref() };
            observe(&STATE, ev.r#type(), || ev.subtype(), || (ev.pointingDeviceType(), ev.isEnteringProximity()));
            e.as_ptr()
        });
        // SAFETY: the handler returns the event it was given (never a dangling pointer); the monitor lives as long as
        // the app (its token is kept on purpose)
        let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        std::mem::forget(token);
    });
}

/// Called every frame: runs the self-test once, a moment after the window is up.
pub fn selftest_tick(now: f64) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if now > 2.0 && !DONE.swap(true, Ordering::Relaxed) {
        if let Ok(v) = std::env::var("FERMATA_TABLET_SELFTEST") {
            selftest(&v);
        }
    }
}

/// A check without a tablet (`FERMATA_TABLET_SELFTEST="x,y x,y …"`, points from the window's top left): the app
/// posts itself what a tablet driver sends: the pen comes near (a TabletProximity event), then a stroke of mouse
/// events.
fn selftest(points: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEventModifierFlags};
    use objc2_core_foundation::CGPoint;
    use objc2_core_graphics::{CGEvent, CGEventField, CGEventType};
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let Some(w) = app.windows().firstObject() else { return };
    let (num, h) = (w.windowNumber(), w.frame().size.height);
    // the pen comes near
    if let Some(e) = CGEvent::new(None) {
        CGEvent::set_type(Some(&e), CGEventType::TabletProximity);
        CGEvent::set_integer_value_field(Some(&e), CGEventField::TabletProximityEventPointerType, 1);
        CGEvent::set_integer_value_field(Some(&e), CGEventField::TabletProximityEventEnterProximity, 1);
        if let Some(ns) = NSEvent::eventWithCGEvent(&e) {
            app.postEvent_atStart(&ns, false);
        }
    }
    let pts: Vec<CGPoint> = points.split_whitespace().filter_map(|p| {
        let (x, y) = p.split_once(',')?;
        Some(CGPoint::new(x.parse().ok()?, h - y.parse::<f64>().ok()?))
    }).collect();
    let n = pts.len();
    let steps = pts.first().map(|p| (*p, NSEventType::MouseMoved)).into_iter().chain(pts.iter().enumerate().map(|(i, p)| {
        (*p, if i == 0 { NSEventType::LeftMouseDown } else if i + 1 == n { NSEventType::LeftMouseUp } else { NSEventType::LeftMouseDragged })
    }));
    for (k, (p, t)) in steps.enumerate() {
        let ev = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(t, p, NSEventModifierFlags(0), k as f64 * 0.01, num, None, k as isize, 1, 0.6);
        if let Some(ev) = ev {
            app.postEvent_atStart(&ev, false);
        }
    }
    eprintln!("tablet selftest: posted the pen near and {n} points to window {num}");
}

/// What one event says about the pen. (Only the fields an event of that type has are read.)
fn observe(st: &State, t: NSEventType, subtype: impl Fn() -> NSEventSubtype, proximity: impl Fn() -> (NSPointingDeviceType, bool)) {
    if t == NSEventType::TabletProximity {
        let (device, entering) = proximity();
        let pen = device == NSPointingDeviceType::Pen;
        st.pen_near.store(pen && entering, Ordering::Relaxed);
        if pen {
            st.seen.store(true, Ordering::Relaxed);
        }
    } else if t == NSEventType::LeftMouseDown {
        let pen = subtype() == NSEventSubtype::TabletPoint || st.pen_near.load(Ordering::Relaxed);
        st.press_is_pen.store(pen, Ordering::Relaxed);
        if pen {
            st.seen.store(true, Ordering::Relaxed);
        }
    } else if t == NSEventType::TabletPoint {
        st.seen.store(true, Ordering::Relaxed);
    }
}

pub fn press_is_pen() -> bool {
    STATE.press_is_pen.load(Ordering::Relaxed)
}

pub fn seen() -> bool {
    STATE.seen.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pen_press_is_told_from_a_mouse_press() {
        let st = State::new();
        let no_prox = || (NSPointingDeviceType::Unknown, false);
        observe(&st, NSEventType::LeftMouseDown, || NSEventSubtype::MouseEvent, no_prox);
        assert!(!st.press_is_pen.load(Ordering::Relaxed), "a mouse click");
        observe(&st, NSEventType::LeftMouseDown, || NSEventSubtype::TabletPoint, no_prox);
        assert!(st.press_is_pen.load(Ordering::Relaxed) && st.seen.load(Ordering::Relaxed), "a pen tap");
        observe(&st, NSEventType::LeftMouseDown, || NSEventSubtype::MouseEvent, no_prox);
        assert!(!st.press_is_pen.load(Ordering::Relaxed), "the mouse again");
        // a pen near the tablet: its presses are the pen's even when the driver leaves the subtype out
        observe(&st, NSEventType::TabletProximity, || NSEventSubtype::MouseEvent, || (NSPointingDeviceType::Pen, true));
        observe(&st, NSEventType::LeftMouseDown, || NSEventSubtype::MouseEvent, no_prox);
        assert!(st.press_is_pen.load(Ordering::Relaxed));
        observe(&st, NSEventType::TabletProximity, || NSEventSubtype::MouseEvent, || (NSPointingDeviceType::Pen, false));
        observe(&st, NSEventType::LeftMouseDown, || NSEventSubtype::MouseEvent, no_prox);
        assert!(!st.press_is_pen.load(Ordering::Relaxed), "the pen left: a mouse click");
    }
}
