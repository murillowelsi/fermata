//! MIDI keyboard input (CoreMIDI on macOS, WinMM on Windows, ALSA on Linux) through midir.
//! Every connected input is opened; notes are passed to the app with the moment they arrived and sounded at once
//! through the built-in synth ("Thru"), so playing feels immediate.
use crate::audio::SynthHandle;
use crate::midi_out::SharedConn;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct MidiMsg {
    pub at: Instant,
    pub on: bool,
    pub note: u8,
    pub vel: u8,
    /// Sustain pedal state change (controller 64), when not a note.
    pub pedal: Option<bool>,
}

pub struct MidiIn {
    conns: Vec<midir::MidiInputConnection<()>>,
    pub names: Vec<String>,
    /// System ids of the open inputs, so a piano switched off and on is reopened.
    pub ids: Vec<String>,
    /// Notes received so far and the last key (255 = none), shown in the status bar.
    pub received: Arc<AtomicU64>,
    pub last_note: Arc<AtomicU8>,
    pub available: Vec<String>,
    tx: Sender<MidiMsg>,
    pub rx: Receiver<MidiMsg>,
    pub last_scan: Option<Instant>,
    pub error: Option<String>,
    /// Instrument the Thru sound uses (the cursor staff's).
    pub thru_program: Arc<AtomicU8>,
    pub thru_on: Arc<AtomicBool>,
    /// Thru to an external output: port 0/1 (255 = built-in sound) and channel.
    pub thru_port: Arc<AtomicU8>,
    pub thru_channel: Arc<AtomicU8>,
    /// Only this input is opened (MIDI Setup ▸ Record Port); None = every input.
    pub record_port: Option<String>,
}

impl MidiIn {
    pub fn new() -> MidiIn {
        let (tx, rx) = channel();
        MidiIn {
            conns: vec![],
            names: vec![],
            ids: vec![],
            received: Arc::new(AtomicU64::new(0)),
            last_note: Arc::new(AtomicU8::new(255)),
            available: vec![],
            tx,
            rx,
            last_scan: None,
            error: None,
            thru_program: Arc::new(AtomicU8::new(0)),
            thru_on: Arc::new(AtomicBool::new(true)),
            thru_port: Arc::new(AtomicU8::new(255)),
            thru_channel: Arc::new(AtomicU8::new(0)),
            record_port: None,
        }
    }

    pub fn connected(&self) -> bool {
        !self.conns.is_empty()
    }

    /// Look for inputs and open every one not open yet.
    pub fn scan(&mut self, ctx: &egui::Context, synth: Option<SynthHandle>, outs: [SharedConn; 2]) {
        self.last_scan = Some(Instant::now());
        let probe = match midir::MidiInput::new("Fermata probe") {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let ports: Vec<(String, String)> = probe.ports().iter().filter_map(|p| probe.port_name(p).ok().map(|n| (n, p.id()))).collect();
        self.available = ports.iter().map(|x| x.0.clone()).collect();
        // Record Port: its exact name, or a port whose name contains it
        let only = self.record_port.clone();
        let wanted = |n: &str| match &only {
            None => true,
            Some(o) => n == o || n.contains(o.as_str()) || o.contains(n),
        };
        // a device that disappeared or came back as a new endpoint (piano switched off and on, cable replugged):
        // drop every connection and reopen what is there
        let ids_now: Vec<&String> = ports.iter().map(|x| &x.1).collect();
        if self.ids.iter().any(|id| !ids_now.contains(&id)) || self.names.iter().any(|n| !wanted(n)) {
            self.conns.clear();
            self.names.clear();
            self.ids.clear();
        }
        for (name, id) in ports.clone() {
            if self.ids.contains(&id) || !wanted(&name) {
                continue;
            }
            let Ok(mut input) = midir::MidiInput::new("Fermata") else { continue };
            input.ignore(midir::Ignore::All);
            let Some(port) = input.find_port_by_id(&id).or_else(|| input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name.as_str()))) else { continue };
            let received = self.received.clone();
            let last_note = self.last_note.clone();
            let tx = self.tx.clone();
            let ctx2 = ctx.clone();
            let synth2 = synth.clone();
            let prog = self.thru_program.clone();
            let thru = self.thru_on.clone();
            let tport = self.thru_port.clone();
            let tch = self.thru_channel.clone();
            let outs2 = outs.clone();
            let res = input.connect(
                &port,
                "fermata-in",
                move |_stamp, msg, _| {
                    if msg.is_empty() {
                        return;
                    }
                    let st = msg[0] & 0xf0;
                    let now = Instant::now();
                    let m = match (st, msg.get(1), msg.get(2)) {
                        (0x90, Some(&n), Some(&v)) if v > 0 => MidiMsg { at: now, on: true, note: n, vel: v, pedal: None },
                        (0x90, Some(&n), _) | (0x80, Some(&n), _) => MidiMsg { at: now, on: false, note: n, vel: 0, pedal: None },
                        (0xb0, Some(64), Some(&v)) => MidiMsg { at: now, on: false, note: 0, vel: v, pedal: Some(v >= 64) },
                        _ => return,
                    };
                    // sound it right here, without waiting for the screen
                    let out_port = tport.load(Ordering::Relaxed);
                    if thru.load(Ordering::Relaxed) && out_port < 2 {
                        // Thru to the external instrument, on the chosen channel
                        let ch = tch.load(Ordering::Relaxed) & 0x0f;
                        let bytes: [u8; 3] = match m.pedal {
                            Some(down) => [0xb0 | ch, 64, if down { 127 } else { 0 }],
                            None if m.on => [0x90 | ch, m.note, m.vel],
                            None => [0x80 | ch, m.note, 0],
                        };
                        if let Ok(mut g) = outs2[out_port as usize].lock() {
                            if let Some(c) = g.as_mut() {
                                let _ = c.send(&bytes);
                            }
                        }
                    } else if thru.load(Ordering::Relaxed) {
                        if let Some(s) = &synth2 {
                            if m.pedal.is_none() {
                                if m.on {
                                    s.note_on(m.note as i32, prog.load(Ordering::Relaxed), m.vel);
                                } else {
                                    s.note_off(m.note as i32);
                                }
                            }
                        }
                    }
                    if m.on && m.pedal.is_none() {
                        received.fetch_add(1, Ordering::Relaxed);
                        last_note.store(m.note, Ordering::Relaxed);
                    }
                    let _ = tx.send(m);
                    ctx2.request_repaint();
                },
                (),
            );
            match res {
                Ok(c) => {
                    self.conns.push(c);
                    self.names.push(name);
                    self.ids.push(id);
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("{name}: {e}")),
            }
        }
    }

    pub fn disconnect_all(&mut self) {
        self.conns.clear();
        self.names.clear();
        self.ids.clear();
    }
}
