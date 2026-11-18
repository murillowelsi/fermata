
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

// docHandle: the file Save writes to again. A file that arrives only becomes it once the app has really opened it
// (after the unsaved-changes question), through adopt().
let docHandle = null, busy = 0, nextId = 1;
const opened = [], saved = [], handles = new Map();
function track(p) { busy++; return p.finally(() => { busy--; }); }
function readInto(file, handle, picked) {
  return file.arrayBuffer().then((b) => {
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
export function open_file(exts) {
  if (window.showOpenFilePicker) {
    track(window.showOpenFilePicker({ types: [{ description: "Scores, Encore files and MIDI", accept: { "application/octet-stream": exts } }] })
      .then(async ([h]) => { await readInto(await h.getFile(), h, true); }).catch(() => {}));
  } else {
    const i = document.createElement("input");
    i.type = "file"; i.accept = exts.join(",");
    i.onchange = () => { if (i.files && i.files[0]) track(readInto(i.files[0], null, true)); };
    i.click();
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
