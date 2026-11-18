
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
// Print (F-06): the score's PDF in a new tab, where the browser's viewer prints it; a download if the tab is blocked
export function open_pdf_tab(name, bytes) {
  const blob = new Blob([bytes.slice()], { type: "application/pdf" });
  const url = URL.createObjectURL(blob);
  const w = window.open(url, "_blank");
  if (!w) download(name, bytes.slice());
  setTimeout(() => URL.revokeObjectURL(url), 120000);
}
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
