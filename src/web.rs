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

#[wasm_bindgen(inline_js = r##"
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
export function is_hidden() { return typeof document !== "undefined" && document.hidden; }
// Automatic backups (F-28): IndexedDB "fermata", store "autosave" (id -> JSON); the heartbeat of the tab that owns
// a copy in localStorage "fermata.alive.<id>". Copies whose heartbeat is old were left behind.
let bkDb = null;
const bkFound = [];
function bkOpen() {
  if (!bkDb) bkDb = new Promise((ok, no) => {
    if (!window.indexedDB) { no(new Error("no IndexedDB")); return; }
    const r = indexedDB.open("fermata", 1);
    r.onupgradeneeded = () => { if (!r.result.objectStoreNames.contains("autosave")) r.result.createObjectStore("autosave"); };
    r.onsuccess = () => ok(r.result);
    r.onerror = () => no(r.error);
  });
  return bkDb;
}
function bkStore(mode) { return bkOpen().then((db) => db.transaction("autosave", mode).objectStore("autosave")); }
export function backup_alive(id) { try { localStorage.setItem("fermata.alive." + id, String(Date.now())); } catch (e) {} }
export function backup_put(id, json) { backup_alive(id); bkStore("readwrite").then((s) => s.put(json, id)).catch(() => {}); }
export function backup_delete(id) {
  try { localStorage.removeItem("fermata.alive." + id); } catch (e) {}
  bkStore("readwrite").then((s) => s.delete(id)).catch(() => {});
}
export function backup_scan(staleMs) {
  bkStore("readonly").then((s) => {
    const req = s.openCursor();
    req.onsuccess = () => {
      const c = req.result;
      if (!c) return;
      let beat = null;
      try { beat = parseFloat(localStorage.getItem("fermata.alive." + c.key)); } catch (e) {}
      if (!(beat > 0) || Date.now() - beat > staleMs) bkFound.push(c.value);
      c.continue();
    };
  }).catch(() => {});
}
export function backup_found() { return bkFound.splice(0); }
// The device (F-25): is the main pointer a finger, and the window size in points. ?device=desktop|touch|reader
// forces a layout, to try the tablet and phone layouts on a computer.
let coarseQuery = null, safeProbe = null;
export function device_info() {
  if (!coarseQuery) coarseQuery = matchMedia("(pointer: coarse)");
  const forced = new URLSearchParams(location.search).get("device") || "";
  return [forced, coarseQuery.matches ? "1" : "0", String(window.innerWidth), String(window.innerHeight)].join(" ");
}
// the iPhone's home bar: room kept free at the bottom (0 elsewhere)
export function safe_bottom() {
  if (!safeProbe) {
    safeProbe = document.createElement("div");
    safeProbe.style.cssText = "position:fixed;left:0;bottom:0;width:0;height:0;visibility:hidden;padding-bottom:env(safe-area-inset-bottom)";
    document.body.appendChild(safeProbe);
  }
  return parseFloat(getComputedStyle(safeProbe).paddingBottom) || 0;
}
export function is_mac() {
  const p = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  return /mac/i.test(p);
}

// docHandle: the file Save writes to again. A file that arrives only becomes it once the app has really opened it
// (after the unsaved-changes question), through adopt().
let docHandle = null, busy = 0, nextId = 1;
const opened = [], saved = [], handles = new Map();
function track(p) { busy++; return p.finally(() => { busy--; }); }
function bytesOf(file) {
  if (file.arrayBuffer) return file.arrayBuffer();
  return new Promise((ok, no) => { const r = new FileReader(); r.onload = () => ok(r.result); r.onerror = () => no(r.error); r.readAsArrayBuffer(file); });
}
function readInto(file, handle, picked) {
  return bytesOf(file).then((b) => {
    const id = nextId++;
    if (handle) handles.set(id, handle);
    opened.push({ id, name: file.name, bytes: new Uint8Array(b), picked });
  });
}
function download(name, bytes) {
  const u = URL.createObjectURL(new Blob([bytes]));
  const a = document.createElement("a");
  a.href = u; a.download = name; document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(u), 10000);
}
export function busy_count() { return busy; }
// Opening files (F-26).
// - No type filter: iOS and iPadOS turn extensions into system file types and grey out the ones they don't know
//   (.enc, .fmt). The app recognises a file by its content instead.
// - On iPhone and iPad (and touch screens generally) the browser opens the file window only from inside the tap
//   itself, and the app's logic runs a frame later. So the app tells this page, every frame, where its "open"
//   targets are (the reader's folder, File > Open and Import when nothing is unsaved, Don't Save after Open); a tap
//   that ends on one opens the window at once, from the tap. Ctrl/Cmd+O on a hardware keyboard does the same.
// - When the window is asked for outside a tap (after saving first, the .enc model for Export), a real button
//   appears ("Choose file…"): tapping it opens the window.
// - The input stays in the page (iOS loses the choice of an input that isn't in the document).
let fileInput = null, gestureOpenAt = -1e9, openTargets = [], openZoom = 1, keysOpen = false, sheet = null;
function touchWebKit() {
  const p = navigator.platform || "";
  return /iPad|iPhone|iPod/.test(navigator.userAgent) || (p === "MacIntel" && navigator.maxTouchPoints > 1);
}
function needsGesture() {
  return touchWebKit() || (!!window.matchMedia && matchMedia("(pointer: coarse)").matches);
}
function getInput() {
  if (!fileInput) {
    fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.id = "fermata_file";
    fileInput.setAttribute("aria-hidden", "true");
    fileInput.style.cssText = "position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none";
    fileInput.addEventListener("change", () => {
      const f = fileInput.files && fileInput.files[0];
      if (f) track(readInto(f, null, true));
      hideSheet();
    });
    document.body.appendChild(fileInput);
  }
  return fileInput;
}
function openNow() {
  hideSheet();
  gestureOpenAt = performance.now();
  const i = getInput();
  i.value = "";
  i.click();
}
function hideSheet() { if (sheet) { sheet.remove(); sheet = null; } }
function showSheet() {
  hideSheet();
  const dark = !!window.matchMedia && matchMedia("(prefers-color-scheme: dark)").matches;
  sheet = document.createElement("div");
  sheet.id = "fermata_open_sheet";
  sheet.style.cssText = "position:fixed;left:50%;bottom:calc(24px + env(safe-area-inset-bottom));transform:translateX(-50%);z-index:20;display:flex;gap:10px;align-items:center;padding:12px;border-radius:16px;box-shadow:0 10px 30px rgba(0,0,0,.25);font:15px -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;" + (dark ? "background:#2C2C2F;color:#ECECEE" : "background:#FFFFFF;color:#1D1D1F");
  const go = document.createElement("button");
  go.type = "button"; go.textContent = "Choose file…";
  go.style.cssText = "border:0;border-radius:10px;padding:12px 20px;min-height:44px;background:" + (dark ? "#0A84FF" : "#007AFF") + ";color:#fff;font:600 16px inherit";
  go.addEventListener("click", openNow);
  const no = document.createElement("button");
  no.type = "button"; no.textContent = "Cancel";
  no.style.cssText = "border:0;border-radius:10px;padding:12px 16px;min-height:44px;background:transparent;color:inherit;font:16px inherit";
  no.addEventListener("click", hideSheet);
  sheet.append(go, no);
  document.body.appendChild(sheet);
}
function onTapEnd(x, y) {
  if (!needsGesture() || performance.now() - gestureOpenAt < 800) return;
  const c = document.getElementById("fermata_canvas");
  if (!c) return;
  const r = c.getBoundingClientRect();
  const px = (x - r.left) / openZoom, py = (y - r.top) / openZoom;
  for (const t of openTargets) {
    if (px >= t[0] && py >= t[1] && px <= t[0] + t[2] && py <= t[1] + t[3]) { openNow(); return; }
  }
}
if (typeof window !== "undefined") {
  document.addEventListener("touchend", (e) => { const t = e.changedTouches && e.changedTouches[0]; if (t) onTapEnd(t.clientX, t.clientY); }, true);
  document.addEventListener("pointerup", (e) => { if (e.pointerType !== "touch") onTapEnd(e.clientX, e.clientY); }, true);
  document.addEventListener("keydown", (e) => {
    if (needsGesture() && keysOpen && (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key || "").toLowerCase() === "o") openNow();
  }, true);
}
// every frame: the "open" targets (x, y, w, h in the app's points), the app's zoom, and whether Ctrl+O opens
export function set_open_targets(flat, zoom, keys) {
  openTargets = [];
  for (let k = 0; k + 3 < flat.length; k += 4) openTargets.push([flat[k], flat[k + 1], flat[k + 2], flat[k + 3]]);
  openZoom = zoom || 1;
  keysOpen = !!keys;
}
export function open_file() {
  // the tap already opened it
  if (performance.now() - gestureOpenAt < 2500) return;
  if (needsGesture()) { showSheet(); return; }
  if (window.showOpenFilePicker) {
    track(window.showOpenFilePicker()
      .then(async ([h]) => { await readInto(await h.getFile(), h, true); }).catch(() => {}));
  } else {
    openNow();
  }
}
export function take_opened() { return opened.splice(0); }
export function adopt(id, name) {
  if (!id) return;
  docHandle = /\.fmt$/i.test(name) ? (handles.get(id) || null) : null;
  handles.delete(id);
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
      // a closed save window comes back with an empty error: nothing saved, and nothing that waited for it goes on
      saved.push({ name, ok: false, err: (e && e.name === "AbortError") ? "" : String((e && e.message) || e) || "unknown error" });
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
      for (const h of params.files || []) { try { await readInto(await h.getFile(), h, false); } catch (e) {} }
    })());
  });
}
"##)]
extern "C" {
    fn midi_supported() -> bool;
    fn midi_request();
    fn midi_state() -> String;
    fn midi_send(name: &str, bytes: &[u8], ts: f64) -> bool;
    fn midi_play(name_a: &str, name_b: &str, ts: &[f64], ports: &[u8], flat: &[u8], lens: &[u32]);
    fn midi_playing() -> bool;
    fn midi_stop();
    fn now_ms() -> f64;
    fn is_hidden() -> bool;
    pub fn backup_alive(id: &str);
    pub fn backup_put(id: &str, json: &str);
    pub fn backup_delete(id: &str);
    #[wasm_bindgen(js_name = backup_scan)]
    fn backup_scan_js(stale_ms: f64);
    #[wasm_bindgen(js_name = backup_found)]
    fn backup_found_js() -> js_sys::Array;
    fn device_info() -> String;
    fn safe_bottom() -> f64;
    fn is_mac() -> bool;
    fn busy_count() -> u32;
    fn open_file();
    fn set_open_targets(flat: &[f32], zoom: f32, keys: bool);
    fn take_opened() -> js_sys::Array;
    fn adopt(id: u32, name: &str);
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
    /// The bytes of a save the browser hasn't confirmed yet.
    static PENDING_SAVE: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
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

/// Opens the file picker, with every file selectable (see `open_file`); the file arrives later through
/// `opened_files`.
pub fn pick_file() {
    open_file();
}

/// Files picked or handed over by the installed app since the last call: (name, bytes).
/// A file from the open window or the installed app: (id, name, bytes, from the open window).
pub fn opened_files() -> Vec<(u32, String, Vec<u8>, bool)> {
    let mut out = vec![];
    for v in take_opened().iter() {
        let get = |k: &str| js_sys::Reflect::get(&v, &k.into()).ok();
        let id = get("id").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32;
        let name = get("name").and_then(|x| x.as_string()).unwrap_or_default();
        let bytes = get("bytes").and_then(|x| x.dyn_into::<js_sys::Uint8Array>().ok()).map(|a| a.to_vec()).unwrap_or_default();
        let picked = get("picked").and_then(|x| x.as_bool()).unwrap_or(false);
        out.push((id, name, bytes, picked));
    }
    out
}

/// The app opened this file: Save now writes to it, and Revert goes back to these bytes (id 0: a revert).
pub fn opened_document(id: u32, name: &str, bytes: &[u8]) {
    if id == 0 {
        return;
    }
    adopt(id, name);
    let doc = name.to_lowercase().ends_with(".fmt").then(|| (name.to_string(), bytes.to_vec()));
    LAST_DOC.with(|d| *d.borrow_mut() = doc);
}

/// Saves the score: to the same file again when `ask` is false and the browser lets us, else asks where (or
/// downloads). The result arrives through `saved_files`.
pub fn save_score(name: &str, bytes: &[u8], ask: bool) {
    // Revert's copy changes only once the browser says the file was written
    PENDING_SAVE.with(|d| *d.borrow_mut() = Some(bytes.to_vec()));
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
        if ok {
            if let Some(b) = PENDING_SAVE.with(|d| d.borrow_mut().take()) {
                LAST_DOC.with(|d| *d.borrow_mut() = Some((name.clone(), b)));
            }
        }
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

/// The state of the app's sound in the browser: "running", "suspended" (no tap yet, or iOS blocked it),
/// "interrupted" (iOS: a call or another app), or "none" (not created yet). See index.html.
pub fn audio_state() -> String {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"fermataAudioState".into()).ok())
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&JsValue::NULL).ok())
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "none".into())
}

/// The app's sound exists but the browser keeps it silent until the next tap (F-27).
pub fn sound_blocked() -> bool {
    matches!(audio_state().as_str(), "suspended" | "interrupted")
}

/// What to tell the person when the sound is blocked.
pub const TAP_FOR_SOUND: &str = "Tap the screen to turn on the sound.";

/// iPhone or iPad without the audio session API: its silent mode mutes the app, and nothing can change that.
pub const SILENT_HINT: &str = "No sound? Turn off Silent Mode on your device.";
pub fn needs_silent_hint() -> bool {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"fermataSilentHint".into()).ok())
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&JsValue::NULL).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Opens a page of the site (a path relative to the app's folder) in a new tab.
pub fn open_page(rel: &str) {
    open_tab(rel);
}

/// Which layout this screen gets (F-25): asked every frame, so turning the tablet or resizing the window
/// switches at once.
pub fn device() -> crate::touch::Device {
    use crate::touch::Device;
    let info = device_info();
    let mut it = info.split(' ');
    let forced = it.next().unwrap_or("");
    let coarse = it.next() == Some("1");
    let w: f32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(1000.0);
    let h: f32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(1000.0);
    match forced {
        "desktop" => Device::Desktop,
        "touch" => Device::Touch,
        "reader" => Device::Reader,
        _ => crate::touch::classify(coarse, w, h),
    }
}

/// Room to keep free under the reader's play bar (the iPhone's home bar), in points.
pub fn safe_area_bottom() -> f32 {
    safe_bottom() as f32
}

thread_local!(static OPEN_TARGETS: RefCell<Vec<egui::Rect>> = const { RefCell::new(Vec::new()) });

/// This frame, a tap on `r` opens a file (F-26: on iPhone and iPad the file window opens only from the tap itself).
pub fn open_target(r: egui::Rect) {
    OPEN_TARGETS.with(|t| t.borrow_mut().push(r));
}

/// End of frame: hand the page this frame's open targets, the zoom, and whether Ctrl/Cmd+O opens a file now.
pub fn publish_open_targets(zoom: f32, keys: bool) {
    let flat: Vec<f32> = OPEN_TARGETS.with(|t| t.borrow_mut().drain(..).flat_map(|r| [r.left(), r.top(), r.width(), r.height()]).collect());
    set_open_targets(&flat, zoom, keys);
}

/// The tab is in the background (on a phone or tablet the app may be closed without warning from there).
pub fn page_hidden() -> bool {
    is_hidden()
}

/// Automatic backups in IndexedDB (F-28; see `autosave.rs`).
pub fn backup_scan(stale_ms: f64) {
    backup_scan_js(stale_ms);
}
pub fn backup_found() -> Vec<String> {
    backup_found_js().iter().filter_map(|v| v.as_string()).collect()
}
