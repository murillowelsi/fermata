//! MIDI output: the score's playback (and optionally the keyboard's Thru) sent to an external instrument such as a
//! Clavinova, on two ports "A" and "B" like the original's MIDI Setup. Timing runs on its own thread; stopping or
//! starting elsewhere first releases every sounding note and the sustain pedal, so nothing hangs.
use crate::audio::PlayNote;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use crate::clock::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

pub type SharedConn = Arc<Mutex<Option<midir::MidiOutputConnection>>>;

/// One timed message on port 0 (A) or 1 (B).
#[derive(Clone, Debug)]
struct Msg {
    at: f64,
    port: u8,
    bytes: Vec<u8>,
    /// Sort note-offs before note-ons at the same moment.
    order: u8,
}

struct Player {
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    t0: Instant,
    from: f64,
    join: Option<JoinHandle<()>>,
}

/// The browser's player: no threads; the page hands the whole performance to the browser's MIDI scheduler
/// (`web::play_midi`) and keeps only the clock, for the playback cursor.
#[cfg(target_arch = "wasm32")]
struct WebPlayer {
    /// Page clock (ms) at the moment `from` (seconds into the score).
    t0_ms: f64,
    from: f64,
}

pub struct MidiOut {
    /// Chosen port names for A and B (None = built-in sound).
    pub chosen: [Option<String>; 2],
    pub transmit_sync: [bool; 2],
    pub conns: [SharedConn; 2],
    pub connected: [Option<String>; 2],
    /// System id of the connected endpoint, so a replugged or restarted piano is reopened.
    pub connected_id: [Option<String>; 2],
    /// Messages sent per port (shown in MIDI Setup and the status bar).
    pub sent: [Arc<AtomicU64>; 2],
    pub available: Vec<String>,
    pub error: Option<String>,
    pub last_scan: Option<Instant>,
    player: Option<Player>,
    #[cfg(target_arch = "wasm32")]
    web_player: Option<WebPlayer>,
}

fn all_off(conns: &[SharedConn; 2]) {
    for c in conns {
        if let Ok(mut g) = c.lock() {
            if let Some(conn) = g.as_mut() {
                for ch in 0..16u8 {
                    let _ = conn.send(&[0xb0 | ch, 64, 0]); // sustain pedal up
                    let _ = conn.send(&[0xb0 | ch, 123, 0]); // all notes off
                }
            }
        }
    }
}

impl MidiOut {
    pub fn new() -> MidiOut {
        MidiOut {
            chosen: [None, None],
            transmit_sync: [false, false],
            conns: [Arc::new(Mutex::new(None)), Arc::new(Mutex::new(None))],
            connected: [None, None],
            connected_id: [None, None],
            sent: [Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0))],
            available: vec![],
            error: None,
            last_scan: None,
            player: None,
            #[cfg(target_arch = "wasm32")]
            web_player: None,
        }
    }

    pub fn active(&self) -> bool {
        self.connected.iter().any(|c| c.is_some())
    }

    pub fn port_label(&self, port: u8) -> String {
        match &self.connected[(port as usize).min(1)] {
            Some(n) => n.clone(),
            None => "built-in sound".into(),
        }
    }

    /// List outputs and (re)open the chosen ones; called now and then so a piano turned on later is found.
    pub fn scan(&mut self) {
        self.last_scan = Some(Instant::now());
        let probe = match midir::MidiOutput::new("Fermata probe") {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let ports: Vec<(String, String)> = probe.ports().iter().filter_map(|p| probe.port_name(p).ok().map(|n| (n, p.id()))).collect();
        self.available = ports.iter().map(|x| x.0.clone()).collect();
        for i in 0..2 {
            let want = self.chosen[i].clone();
            // the chosen device: its exact name, or a port whose name contains it (names can gain a prefix)
            let target = want.as_ref().and_then(|w| ports.iter().find(|(n, _)| n == w).or_else(|| ports.iter().find(|(n, _)| n.contains(w.as_str()) || w.contains(n.as_str()))).cloned());
            // dropped, changed, or the same name now belongs to a new endpoint (piano switched off and on, cable replugged)
            let stale = match (&self.connected_id[i], &target) {
                (Some(id), Some((_, tid))) => id != tid,
                (Some(_), None) => true,
                _ => false,
            };
            if self.connected[i].is_some() && (want.is_none() || stale) {
                if let Ok(mut g) = self.conns[i].lock() {
                    *g = None;
                }
                self.connected[i] = None;
                self.connected_id[i] = None;
            }
            if let (None, Some((name, id))) = (&self.connected[i], target) {
                let Ok(out) = midir::MidiOutput::new("Fermata") else { continue };
                let Some(port) = out.find_port_by_id(&id).or_else(|| out.ports().into_iter().find(|p| out.port_name(p).ok().as_deref() == Some(name.as_str()))) else { continue };
                match out.connect(&port, "fermata-out") {
                    Ok(c) => {
                        if let Ok(mut g) = self.conns[i].lock() {
                            *g = Some(c);
                        }
                        self.connected[i] = Some(name);
                        self.connected_id[i] = Some(id);
                        self.error = None;
                    }
                    Err(e) => self.error = Some(format!("{name}: {e}")),
                }
            }
        }
    }

    /// Plays a short C–E–G–C on channel 1 straight to a port, to check the cable and the instrument.
    pub fn test(&self, port: usize) -> bool {
        let conn = self.conns[port.min(1)].clone();
        let sent = self.sent[port.min(1)].clone();
        if conn.lock().map(|g| g.is_none()).unwrap_or(true) {
            return false;
        }
        // the browser: the four notes go out at once, each with its own Web MIDI timestamp
        #[cfg(target_arch = "wasm32")]
        {
            let Some(name) = self.connected[port.min(1)].clone() else { return false };
            let t = crate::web::now();
            let mut n = 0;
            for b in [&[0xb0u8, 0, 0][..], &[0xb0, 32, 0], &[0xc0, 0], &[0xb0, 7, 110]] {
                n += crate::web::send_midi(&name, b, 0.0) as u64;
            }
            for (i, k) in [60u8, 64, 67, 72].into_iter().enumerate() {
                let at = t + i as f64 * 350.0;
                n += crate::web::send_midi(&name, &[0x90, k, 100], at) as u64;
                n += crate::web::send_midi(&name, &[0x80, k, 0], at + 350.0) as u64;
            }
            sent.fetch_add(n, Ordering::Relaxed);
            let _ = conn;
            n > 0
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
        std::thread::spawn(move || {
            let send = |b: &[u8]| {
                if let Ok(mut g) = conn.lock() {
                    if let Some(c) = g.as_mut() {
                        if c.send(b).is_ok() {
                            sent.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            };
            send(&[0xb0, 0, 0]);
            send(&[0xb0, 32, 0]);
            send(&[0xc0, 0]);
            send(&[0xb0, 7, 110]);
            for k in [60u8, 64, 67, 72] {
                send(&[0x90, k, 100]);
                std::thread::sleep(Duration::from_millis(350));
                send(&[0x80, k, 0]);
            }
        });
        true
        }
    }

    /// Text describing what the program sees, to send when asking for help.
    pub fn diagnostics(&self) -> String {
        let mut t = String::new();
        t.push_str(&format!("MIDI outputs found: {:?}\n", self.available));
        for i in 0..2 {
            t.push_str(&format!(
                "Port {}: chosen {:?}, connected {:?} (id {:?}), messages sent {}\n",
                if i == 0 { "A" } else { "B" },
                self.chosen[i],
                self.connected[i],
                self.connected_id[i],
                self.sent[i].load(Ordering::Relaxed)
            ));
        }
        if let Some(e) = &self.error {
            t.push_str(&format!("Last error: {e}\n"));
        }
        t.push_str(&format!("Playing now: {}\n", self.is_playing()));
        t
    }

    pub fn choose(&mut self, port: usize, name: Option<String>) {
        self.stop();
        self.chosen[port.min(1)] = name;
        self.scan();
    }

    pub fn send(&self, port: u8, bytes: &[u8]) {
        if let Ok(mut g) = self.conns[(port as usize).min(1)].lock() {
            if let Some(c) = g.as_mut() {
                if c.send(bytes).is_ok() {
                    self.sent[(port as usize).min(1)].fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Play the score's notes from `from` seconds. Instruments, volume and pan go first, per channel.
    pub fn play(&mut self, notes: &[PlayNote], setup: Vec<(u8, Vec<u8>)>, from: f64, qpm: f64) {
        self.stop();
        let mut msgs: Vec<Msg> = vec![];
        for n in notes {
            if n.at + n.len < from && n.cc.is_none() {
                continue;
            }
            let ch = n.channel & 0x0f;
            if let Some((cc, val)) = n.cc {
                if n.at >= from {
                    msgs.push(Msg { at: n.at, port: n.port, bytes: vec![0xb0 | ch, cc, val], order: 1 });
                }
                continue;
            }
            if n.at < from {
                continue;
            }
            let (ch, key) = (ch, n.midi.clamp(0, 127) as u8);
            let vel = ((n.vel * if n.click { 1.0 } else { 1.0 }) * 127.0).round().clamp(1.0, 127.0) as u8;
            msgs.push(Msg { at: n.at, port: n.port, bytes: vec![0x90 | ch, key, vel], order: 2 });
            msgs.push(Msg { at: n.at + n.len.max(0.02), port: n.port, bytes: vec![0x80 | ch, key, 0], order: 0 });
        }
        msgs.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap().then(a.order.cmp(&b.order)));
        #[cfg(target_arch = "wasm32")]
        {
            let _ = qpm;
            for (port, b) in &setup {
                self.send_web(*port, b, 0.0);
            }
            let t0_ms = crate::web::now() + 30.0;
            let timed: Vec<(f64, u8, &[u8])> =
                msgs.iter().map(|m| (t0_ms + (m.at - from) * 1000.0, m.port.min(1), m.bytes.as_slice())).collect();
            for m in &timed {
                if let Some(c) = self.sent.get(m.1 as usize).filter(|_| self.connected[m.1 as usize].is_some()) {
                    c.fetch_add(1, Ordering::Relaxed);
                }
            }
            let name = |i: usize| self.connected[i].clone().unwrap_or_default();
            crate::web::play_midi(&name(0), &name(1), &timed);
            self.web_player = Some(WebPlayer { t0_ms, from });
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.play_thread(msgs, setup, from, qpm);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn play_thread(&mut self, msgs: Vec<Msg>, setup: Vec<(u8, Vec<u8>)>, from: f64, qpm: f64) {
        let conns = self.conns.clone();
        let sent = self.sent.clone();
        let sync = self.transmit_sync;
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (stop2, done2) = (stop.clone(), done.clone());
        let t0 = Instant::now();
        let join = std::thread::spawn(move || {
            let send = |port: u8, b: &[u8]| {
                if let Ok(mut g) = conns[(port as usize).min(1)].lock() {
                    if let Some(c) = g.as_mut() {
                        if c.send(b).is_ok() {
                            sent[(port as usize).min(1)].fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            };
            for (port, b) in &setup {
                send(*port, b);
            }
            for p in 0..2u8 {
                if sync[p as usize] {
                    send(p, &[0xfa]); // start
                }
            }
            let mut sounding: HashSet<(u8, u8, u8)> = HashSet::new();
            let mut i = 0;
            // MIDI clock: 24 per quarter is tempo-dependent; sent as wall-clock ticks the receiver follows
            let mut next_clock = 0.0f64;
            let clock_step = 60.0 / qpm.max(10.0) / 24.0;
            while i < msgs.len() {
                if stop2.load(Ordering::Relaxed) {
                    break;
                }
                let now = from + t0.elapsed().as_secs_f64();
                if sync.iter().any(|s| *s) && now >= from + next_clock {
                    for p in 0..2u8 {
                        if sync[p as usize] {
                            send(p, &[0xf8]);
                        }
                    }
                    next_clock += clock_step;
                }
                let m = &msgs[i];
                if m.at <= now {
                    send(m.port, &m.bytes);
                    let st = m.bytes[0] & 0xf0;
                    let key = (m.port, m.bytes[0] & 0x0f, m.bytes.get(1).copied().unwrap_or(0));
                    if st == 0x90 {
                        sounding.insert(key);
                    } else if st == 0x80 {
                        sounding.remove(&key);
                    }
                    i += 1;
                    continue;
                }
                let wait = (m.at - now).min(0.002);
                std::thread::sleep(Duration::from_secs_f64(wait.max(0.0005)));
            }
            // release whatever still sounds (stopped early)
            for (port, ch, key) in sounding.drain() {
                send(port, &[0x80 | ch, key, 0]);
            }
            for p in 0..2u8 {
                if sync[p as usize] {
                    send(p, &[0xfc]); // stop
                }
            }
            if stop2.load(Ordering::Relaxed) {
                all_off(&conns);
            }
            done2.store(true, Ordering::Relaxed);
        });
        self.player = Some(Player { stop, done, t0, from, join: Some(join) });
    }

    /// The browser: one port's message, now (`at_ms` 0) or at a page-clock time.
    #[cfg(target_arch = "wasm32")]
    fn send_web(&self, port: u8, bytes: &[u8], at_ms: f64) {
        let i = (port as usize).min(1);
        if let Some(name) = &self.connected[i] {
            if crate::web::send_midi(name, bytes, at_ms) {
                self.sent[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn stop(&mut self) {
        // the browser: its scheduler drops what it can and releases the notes it started
        #[cfg(target_arch = "wasm32")]
        if self.web_player.take().is_some() {
            crate::web::stop_midi();
        }
        if let Some(mut p) = self.player.take() {
            p.stop.store(true, Ordering::Relaxed);
            if let Some(j) = p.join.take() {
                let _ = j.join();
            }
        }
    }

    /// Panic: silence everything on both ports.
    pub fn all_notes_off(&mut self) {
        self.stop();
        all_off(&self.conns);
    }

    pub fn is_playing(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        if self.web_player.is_some() {
            return crate::web::midi_is_playing();
        }
        self.player.as_ref().map(|p| !p.done.load(Ordering::Relaxed)).unwrap_or(false)
    }

    pub fn time(&self) -> f64 {
        #[cfg(target_arch = "wasm32")]
        if let Some(p) = &self.web_player {
            return p.from + (crate::web::now() - p.t0_ms) / 1000.0;
        }
        self.player.as_ref().map(|p| p.from + p.t0.elapsed().as_secs_f64()).unwrap_or(0.0)
    }

    /// A short note for auditioning while editing.
    pub fn audition(&self, port: u8, ch: u8, key: u8, vel: u8) {
        let conn = self.conns[(port as usize).min(1)].clone();
        let ch = ch & 0x0f;
        if let Ok(mut g) = conn.lock() {
            if let Some(c) = g.as_mut() {
                let _ = c.send(&[0x90 | ch, key, vel.max(1)]);
            } else {
                return;
            }
        }
        // the browser: the note-off goes with a timestamp instead of a sleeping thread
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(name) = &self.connected[(port as usize).min(1)] {
                crate::web::send_midi(name, &[0x80 | ch, key, 0], crate::web::now() + 450.0);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(450));
            if let Ok(mut g) = conn.lock() {
                if let Some(c) = g.as_mut() {
                    let _ = c.send(&[0x80 | ch, key, 0]);
                }
            }
        });
    }
}

impl Drop for MidiOut {
    fn drop(&mut self) {
        self.stop();
        all_off(&self.conns);
    }
}
