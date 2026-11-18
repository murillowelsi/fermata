//! The browser build only: small JavaScript helpers for what Rust can't reach directly or simply.
//! - Web MIDI output with timestamps (precise timing without threads), the permission state, and whether the
//!   browser has Web MIDI at all (Safari doesn't).
//! - Opening and saving files: the File System Access API where it exists (Chrome, Edge), so saving again writes
//!   to the same file; otherwise a file input to open and a download to save.
//! - Files handed over by the installed app (PWA file handlers, the Launch Queue).
//! - Whether the page runs on a Mac (for the shortcut alternatives, which only apply off the Mac).
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen(inline_js = r#"
let access = null, pending = null, midiErr = null;
export function midi_supported() { return typeof navigator !== "undefined" && !!navigator.requestMIDIAccess; }
export function midi_request() {
  if (access || pending || !midi_supported()) return;
  // the same request midir makes (sysex), so the person grants one permission, not two
  // tracked: the page keeps repainting until the answer arrives, so the status bar and MIDI Setup update by themselves
  pending = navigator.requestMIDIAccess({ sysex: true }).then(
    (a) => { access = a; pending = null; midiErr = null; },
    (e) => { midiErr = String((e && e.name) || e); pending = null; });
  track(pending.then(() => {}, () => {}));
}
export function midi_state() {
  if (!midi_supported()) return "unsupported";
  if (access) return "granted";
  if (midiErr) return "denied";
  return "pending";
}
export function midi_send(name, bytes, ts) {
  if (!access) return false;
  for (const o of access.outputs.values()) {
    if (o.name === name) {
      try { if (ts > 0) o.send(bytes, ts); else o.send(bytes); return true; } catch (e) { return false; }
    }
  }
  return false;
}
// Playback to the piano: a timer in a tiny worker hands the browser the next moment's messages with Web MIDI
// timestamps, and the browser sends them on time. It doesn't depend on screen frames, and a worker's timer keeps
// running in a background tab, where page timers slow down to once a second or less.
let play = null, worker = null, timer = 0;
function ticker(on) {
  if (worker === null && on) {
    try {
      const src = "let t = 0; onmessage = (e) => { clearInterval(t); t = e.data ? setInterval(() => postMessage(0), 25) : 0; };";
      worker = new Worker(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
      worker.onmessage = tick;
    } catch (e) { worker = false; }
  }
  if (worker) worker.postMessage(on ? 1 : 0);
  else { clearInterval(timer); timer = on ? setInterval(tick, 25) : 0; }
}
function output(name) {
  if (!access || !name) return null;
  for (const o of access.outputs.values()) if (o.name === name) return o;
  return null;
}
function tick() {
  const p = play;
  if (!p) return;
  const now = performance.now(), ahead = document.hidden ? 1500 : 200;
  while (p.next < p.msgs.length && p.msgs[p.next].t <= now + ahead) {
    const m = p.msgs[p.next++], t = Math.max(m.t, now);
    p.last = Math.max(p.last, t);
    if (!m.o) continue;
    try { m.o.send(m.d, t); } catch (e) { continue; }
    const s = m.d[0] & 0xf0, key = m.o.id + " " + (m.d[0] & 15) + " " + m.d[1];
    if (s === 0x90 && m.d[2] > 0) p.sounding.set(key, [m.o, m.d[0] & 15, m.d[1]]);
    else if (s === 0x80 || s === 0x90) p.sounding.delete(key);
  }
  if (p.next >= p.msgs.length) ticker(false);
}
export function midi_play(nameA, nameB, ts, ports, flat, lens) {
  midi_stop();
  const outs = [output(nameA), output(nameB)], msgs = [];
  for (let i = 0, k = 0; i < ts.length; k += lens[i], i++) msgs.push({ t: ts[i], o: outs[ports[i] ? 1 : 0], d: flat.slice(k, k + lens[i]) });
  play = { msgs, next: 0, outs, last: 0, sounding: new Map() };
  tick();
  if (play && play.next < msgs.length) ticker(true);
}
export function midi_playing() { return !!play && (play.next < play.msgs.length || performance.now() < play.last); }
export function midi_stop() {
  const p = play;
  play = null;
  if (!p) return;
  ticker(false);
  // drop what's scheduled where the browser can, then release whatever was started
  let cleared = true;
  for (const o of p.outs) if (o) { if (o.clear) { try { o.clear(); } catch (e) { cleared = false; } } else cleared = false; }
  const now = performance.now(), at = (cleared ? now : Math.max(p.last, now)) + 1;
  for (const [o, ch, key] of p.sounding.values()) { try { o.send([0x80 | ch, key, 0], at); } catch (e) {} }
  if (p.next < p.msgs.length || p.sounding.size) {
    for (const o of new Set(p.outs.filter(Boolean))) {
      for (let ch = 0; ch < 16; ch++) { try { o.send([0xb0 | ch, 64, 0], at); o.send([0xb0 | ch, 123, 0], at); } catch (e) {} }
    }
  }
}
export function now_ms() { return performance.now(); }
export function is_mac() {
  const p = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  return /mac/i.test(p);
}

let docHandle = null, busy = 0;
const opened = [], saved = [];
function track(p) { busy++; return p.finally(() => { busy--; }); }
function readInto(file, handle) {
  return file.arrayBuffer().then((b) => { opened.push({ name: file.name, bytes: new Uint8Array(b), handle }); });
}
function download(name, bytes) {
  const u = URL.createObjectURL(new Blob([bytes]));
  const a = document.createElement("a");
  a.href = u; a.download = name; document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(u), 10000);
}
export function busy_count() { return busy; }
export function open_file(exts) {
  if (window.showOpenFilePicker) {
    track(window.showOpenFilePicker({ types: [{ description: "Scores, Encore files and MIDI", accept: { "application/octet-stream": exts } }] })
      .then(async ([h]) => { await readInto(await h.getFile(), h); }).catch(() => {}));
  } else {
    const i = document.createElement("input");
    i.type = "file"; i.accept = exts.join(",");
    i.onchange = () => { if (i.files && i.files[0]) track(readInto(i.files[0], null)); };
    i.click();
  }
}
export function take_opened() {
  const r = opened.splice(0);
  for (const o of r) { if (/\.fmt$/i.test(o.name)) docHandle = o.handle; else docHandle = null; }
  return r;
}
export function save_doc(name, bytes, ask) {
  const copy = bytes.slice();
  track((async () => {
    try {
      if (window.showSaveFilePicker) {
        const h = (!ask && docHandle) ? docHandle : await window.showSaveFilePicker({ suggestedName: name, types: [{ description: "Fermata score", accept: { "application/octet-stream": [".fmt"] } }] });
        const w = await h.createWritable(); await w.write(copy); await w.close();
        docHandle = h; saved.push({ name: h.name, ok: true, err: "" });
      } else { download(name, copy); saved.push({ name, ok: true, err: "" }); }
    } catch (e) {
      if (!(e && e.name === "AbortError")) saved.push({ name, ok: false, err: String((e && e.message) || e) });
    }
  })());
}
export function export_file(name, bytes, ext, desc) {
  const copy = bytes.slice();
  track((async () => {
    try {
      if (window.showSaveFilePicker) {
        const h = await window.showSaveFilePicker({ suggestedName: name, types: [{ description: desc, accept: { "application/octet-stream": ["." + ext] } }] });
        const w = await h.createWritable(); await w.write(copy); await w.close();
      } else download(name, copy);
    } catch (e) {}
  })());
}
export function take_saved() { return saved.splice(0); }
// closing the tab with unsaved changes: the browser asks first
let unsaved = false;
export function set_unsaved(v) { unsaved = v; }
export function set_title(t) { document.title = t; }
export function open_tab(u) { window.open(new URL(u, location.href).href, "_blank", "noopener"); }
if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", (e) => { if (unsaved) { e.preventDefault(); e.returnValue = ""; } });
}
export function forget_doc() { docHandle = null; }
if (typeof window !== "undefined" && "launchQueue" in window) {
  window.launchQueue.setConsumer(async (params) => {
    await track((async () => {
      for (const h of params.files || []) { try { await readInto(await h.getFile(), h); } catch (e) {} }
    })());
  });
}
"#)]
extern "C" {
    fn midi_supported() -> bool;
    fn midi_request();
    fn midi_state() -> String;
    fn midi_send(name: &str, bytes: &[u8], ts: f64) -> bool;
    fn midi_play(name_a: &str, name_b: &str, ts: &[f64], ports: &[u8], flat: &[u8], lens: &[u32]);
    fn midi_playing() -> bool;
    fn midi_stop();
    fn now_ms() -> f64;
    fn is_mac() -> bool;
    fn busy_count() -> u32;
    fn open_file(exts: Vec<JsValue>);
    fn take_opened() -> js_sys::Array;
    fn save_doc(name: &str, bytes: &[u8], ask: bool);
    fn export_file(name: &str, bytes: &[u8], ext: &str, desc: &str);
    fn take_saved() -> js_sys::Array;
    fn set_unsaved(v: bool);
    fn set_title(t: &str);
    fn open_tab(u: &str);
    fn forget_doc();
}

thread_local! {
    static MAC: Cell<Option<bool>> = const { Cell::new(None) };
    /// The score as last opened or saved, for Revert to Saved (the browser can't read the file back).
    static LAST_DOC: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) };
}

/// Is the page on a Mac? (asked once)
pub fn on_mac() -> bool {
    MAC.with(|m| {
        if let Some(v) = m.get() {
            return v;
        }
        let v = is_mac();
        m.set(Some(v));
        v
    })
}

/// Web MIDI: "unsupported" (Safari), "pending", "granted" or "denied". Asks for access the first time.
pub fn midi_status() -> String {
    midi_request();
    midi_state()
}

/// Sends to the output port with this name, at `at_ms` on the page clock (0 = now). False when it isn't there.
pub fn send_midi(port_name: &str, bytes: &[u8], at_ms: f64) -> bool {
    midi_request();
    midi_send(port_name, bytes, at_ms)
}

/// Plays messages on the two output ports (`""` = not connected): (page-clock ms, port 0/1, bytes) in time order.
pub fn play_midi(port_a: &str, port_b: &str, msgs: &[(f64, u8, &[u8])]) {
    midi_request();
    let ts: Vec<f64> = msgs.iter().map(|m| m.0).collect();
    let ports: Vec<u8> = msgs.iter().map(|m| m.1).collect();
    let lens: Vec<u32> = msgs.iter().map(|m| m.2.len() as u32).collect();
    let flat: Vec<u8> = msgs.iter().flat_map(|m| m.2.iter().copied()).collect();
    midi_play(port_a, port_b, &ts, &ports, &flat, &lens);
}

/// Whether the piano playback still has messages to send or notes to finish.
pub fn midi_is_playing() -> bool {
    midi_playing()
}

/// Stops the piano playback and releases its notes.
pub fn stop_midi() {
    midi_stop();
}

/// The page clock in milliseconds (the same clock Web MIDI timestamps use).
pub fn now() -> f64 {
    now_ms()
}

/// A file picker or a save still waiting: the app keeps redrawing so the result shows at once.
pub fn busy() -> bool {
    busy_count() > 0
}

/// Opens the file picker; the file arrives later through `opened_files`.
pub fn pick_file(exts: &[&str]) {
    open_file(exts.iter().map(|e| JsValue::from_str(&format!(".{e}"))).collect());
}

/// Files picked or handed over by the installed app since the last call: (name, bytes).
pub fn opened_files() -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    for v in take_opened().iter() {
        let name = js_sys::Reflect::get(&v, &"name".into()).ok().and_then(|x| x.as_string()).unwrap_or_default();
        let bytes = js_sys::Reflect::get(&v, &"bytes".into()).ok().and_then(|x| x.dyn_into::<js_sys::Uint8Array>().ok()).map(|a| a.to_vec()).unwrap_or_default();
        if name.to_lowercase().ends_with(".fmt") {
            LAST_DOC.with(|d| *d.borrow_mut() = Some((name.clone(), bytes.clone())));
        }
        out.push((name, bytes));
    }
    out
}

/// Saves the score: to the same file again when `ask` is false and the browser lets us, else asks where (or
/// downloads). The result arrives through `saved_files`.
pub fn save_score(name: &str, bytes: &[u8], ask: bool) {
    LAST_DOC.with(|d| *d.borrow_mut() = Some((name.to_string(), bytes.to_vec())));
    save_doc(name, bytes, ask);
}

/// Exports (MIDI, MusicXML, PNG, .enc): asks where, or downloads.
pub fn export(name: &str, bytes: &[u8], ext: &str, desc: &str) {
    export_file(name, bytes, ext, desc);
}

/// Results of `save_score`: (file name, Ok or the error).
pub fn saved_files() -> Vec<(String, Result<(), String>)> {
    let mut out = vec![];
    for v in take_saved().iter() {
        let get = |k: &str| js_sys::Reflect::get(&v, &k.into()).ok();
        let name = get("name").and_then(|x| x.as_string()).unwrap_or_default();
        let ok = get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        let err = get("err").and_then(|x| x.as_string()).unwrap_or_default();
        out.push((name, if ok { Ok(()) } else { Err(err) }));
    }
    out
}

/// The score as last opened or saved (Revert to Saved).
pub fn last_saved() -> Option<(String, Vec<u8>)> {
    LAST_DOC.with(|d| d.borrow().clone())
}

/// A new score: Save asks where again.
pub fn new_document() {
    forget_doc();
    LAST_DOC.with(|d| *d.borrow_mut() = None);
}

/// Whether the score has unsaved changes, so closing the tab asks first.
pub fn unsaved_changes(v: bool) {
    thread_local!(static LAST: Cell<Option<bool>> = const { Cell::new(None) });
    if LAST.with(|l| l.replace(Some(v))) != Some(v) {
        set_unsaved(v);
    }
}

/// The tab's (or the installed app window's) title, as the desktop app's window title.
pub fn title(t: &str) {
    thread_local!(static LAST: RefCell<String> = const { RefCell::new(String::new()) });
    LAST.with(|l| {
        if *l.borrow() != t {
            *l.borrow_mut() = t.to_string();
            set_title(t);
        }
    });
}

/// Opens a page of the site (a path relative to the app's folder) in a new tab.
pub fn open_page(rel: &str) {
    open_tab(rel);
}
