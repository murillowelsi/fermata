//! F-31: read a score from a photo or a scan (OMR) with Audiveris, run as a separate program the person installed.
//!
//! Fermata doesn't ship Audiveris (AGPL-3.0, ~163 MB with its Java). It looks for it, runs it in batch mode on a
//! background thread (cancel and timeout), and opens the MusicXML it writes with the MusicXML importer (F-07) as a
//! new score to check: OMR reads most pitches but rarely a whole page right (QA spike, qa/F-31/resultado.md).
#![cfg(not(target_arch = "wasm32"))]
use crate::model::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// How long one page may take before it is stopped (the QA measured 17–30 s per page).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Audiveris keeps its own settings in its home folder: Fermata gives it one inside its own folder, so nothing is
/// written elsewhere in the person's home.
pub fn reader_home() -> Option<PathBuf> {
    setup::home().map(|h| h.join("home"))
}

/// The status-bar note on every score read from a photo.
pub const CHECK_IT: &str = "Imported from photo: check it";

#[derive(Debug, Clone, PartialEq)]
pub enum OmrError {
    /// Audiveris wasn't found (no path set in the preferences and none of the usual places).
    NotInstalled,
    /// The picture's type isn't one Audiveris (or this computer) can read.
    Unsupported(String),
    Cancelled,
    TimedOut(u64),
    /// Audiveris ran but wrote no score (with the end of what it printed).
    NoScore(String),
    Failed(String),
}

impl std::fmt::Display for OmrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OmrError::NotInstalled => write!(f, "Reading music from photos isn't available on this computer yet."),
            OmrError::Unsupported(t) => write!(f, "{t}"),
            OmrError::Cancelled => write!(f, "Reading the photo was cancelled."),
            OmrError::TimedOut(s) => write!(f, "Reading the photo took longer than {s} s and was stopped."),
            OmrError::NoScore(log) => write!(f, "Audiveris found no music in the picture.{}", if log.is_empty() { String::new() } else { format!("\n{log}") }),
            OmrError::Failed(e) => write!(f, "Audiveris couldn't run: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Finding Audiveris
// ---------------------------------------------------------------------------------------------------------------

/// The program inside what the person pointed at: an `Audiveris.app` bundle, its folder, or the program itself.
pub fn program_in(p: &Path) -> Option<PathBuf> {
    let tries = [
        p.to_path_buf(),
        p.join("Contents/MacOS/Audiveris"),
        p.join("Audiveris.app/Contents/MacOS/Audiveris"),
        p.join("Audiveris.exe"),
        p.join("bin/Audiveris.bat"),
        p.join("bin/Audiveris"),
    ];
    tries.into_iter().find(|t| t.is_file())
}

/// The usual install places (Mac: /Applications and ~/Applications; Windows: Program Files and the per-user
/// Programs folder; Linux: /opt and /usr).
pub fn default_locations() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = vec![];
    if cfg!(target_os = "macos") {
        v.push("/Applications/Audiveris.app".into());
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Applications/Audiveris.app"));
        }
    } else if cfg!(target_os = "windows") {
        for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(d) = std::env::var_os(var) {
                v.push(PathBuf::from(d).join("Audiveris"));
            }
        }
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(d).join("Programs").join("Audiveris"));
        }
    } else {
        v.push("/opt/audiveris".into());
        v.push("/usr/bin/audiveris".into());
        v.push("/usr/local/bin/audiveris".into());
    }
    v
}

/// Where Audiveris is: the path saved in the preferences first, then `FERMATA_AUDIVERIS`, then the copy Fermata
/// downloaded (`setup`), then the usual places. None = not there yet (Fermata can download it: `setup::start`).
pub fn find_audiveris(pref: Option<&str>) -> Option<PathBuf> {
    let mut tries: Vec<PathBuf> = vec![];
    if let Some(p) = pref.filter(|p| !p.trim().is_empty()) {
        tries.push(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("FERMATA_AUDIVERIS") {
        tries.push(PathBuf::from(p));
    }
    if let Some(p) = setup::installed() {
        tries.push(p);
    }
    tries.extend(default_locations());
    tries.iter().find_map(|p| program_in(p))
}

// ---------------------------------------------------------------------------------------------------------------
// Running it
// ---------------------------------------------------------------------------------------------------------------

/// The picture as Audiveris can read it: PNG, JPEG, TIFF, BMP, GIF or PDF as they are; a HEIC photo from an iPhone
/// is converted to PNG first with the Mac's own `sips` (not on other systems).
fn readable_input(input: &Path, work: &Path) -> Result<PathBuf, OmrError> {
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "gif" | "pdf" => Ok(input.to_path_buf()),
        "heic" | "heif" => {
            if !cfg!(target_os = "macos") {
                return Err(OmrError::Unsupported("HEIC photos can't be read on this computer: save the photo as JPEG or PNG.".into()));
            }
            let out = work.join("photo.png");
            let ok = std::process::Command::new("/usr/bin/sips").args(["-s", "format", "png"]).arg(input).arg("--out").arg(&out).output().map(|o| o.status.success()).unwrap_or(false);
            if ok && out.is_file() {
                Ok(out)
            } else {
                Err(OmrError::Unsupported("Couldn't convert the HEIC photo: save it as JPEG or PNG.".into()))
            }
        }
        other => Err(OmrError::Unsupported(format!("A .{other} file isn't a picture Fermata can read music from (use PNG, JPEG, TIFF or PDF)."))),
    }
}

/// The score Audiveris wrote: the first .mxl (or .musicxml / .xml) under its output folder.
fn find_output(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(p.extension().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).as_deref(), Some("mxl" | "musicxml" | "xml")) {
                found.push(p);
            }
        }
    }
    found.sort();
    // a compressed score first; several movements give several files, the first is the start of the piece
    found.iter().find(|p| p.extension().map(|x| x == "mxl").unwrap_or(false)).or(found.first()).cloned()
}

/// What a finished run gives: the score file's bytes and name, and how long it took.
#[derive(Debug, Clone)]
pub struct OmrOutput {
    pub name: String,
    pub bytes: Vec<u8>,
    pub seconds: f64,
    /// The last lines Audiveris printed (warnings such as "check time signatures").
    pub log: String,
}

/// A run in the background: poll `try_result` (the app, every frame) or `wait` (tests and the command line).
pub struct OmrJob {
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<OmrOutput, OmrError>>,
    pub input: PathBuf,
}

impl OmrJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    /// None while Audiveris is still reading.
    pub fn try_result(&self) -> Option<Result<OmrOutput, OmrError>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(OmrError::Failed("the reading stopped unexpectedly".into()))),
        }
    }
    pub fn wait(&self) -> Result<OmrOutput, OmrError> {
        self.rx.recv().unwrap_or_else(|_| Err(OmrError::Failed("the reading stopped unexpectedly".into())))
    }
}

/// Settings of one run. `home` gives Audiveris another home folder for its own settings (tests: nothing written in
/// the person's home).
#[derive(Clone, Debug)]
pub struct OmrRun {
    pub program: PathBuf,
    pub timeout: Duration,
    pub home: Option<PathBuf>,
}

/// Start reading `input` with Audiveris in batch mode (`-batch -transcribe -export -output <tmp> -- <input>`).
pub fn start(run: OmrRun, input: &Path) -> OmrJob {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let c2 = cancel.clone();
    let input_p = input.to_path_buf();
    std::thread::spawn(move || {
        let r = run_now(&run, &input_p, &c2);
        let _ = tx.send(r);
    });
    OmrJob { cancel, rx, input: input.to_path_buf() }
}

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("fermata-omr-{}-{n}", std::process::id()))
}

fn run_now(run: &OmrRun, input: &Path, cancel: &AtomicBool) -> Result<OmrOutput, OmrError> {
    if !input.is_file() {
        return Err(OmrError::Failed(format!("{} doesn't exist", input.display())));
    }
    let work = temp_dir();
    std::fs::create_dir_all(&work).map_err(|e| OmrError::Failed(e.to_string()))?;
    let result = (|| {
        let pic = readable_input(input, &work)?;
        let out_dir = work.join("out");
        let log_path = work.join("audiveris.log");
        let log = std::fs::File::create(&log_path).map_err(|e| OmrError::Failed(e.to_string()))?;
        let mut cmd = std::process::Command::new(&run.program);
        cmd.args(["-batch", "-transcribe", "-export", "-output"]).arg(&out_dir).arg("--").arg(&pic);
        cmd.stdin(std::process::Stdio::null()).stdout(log.try_clone().map_err(|e| OmrError::Failed(e.to_string()))?).stderr(log);
        if let Some(h) = &run.home {
            let _ = std::fs::create_dir_all(h);
            cmd.env("HOME", h);
        }
        let t0 = Instant::now();
        let mut child = cmd.spawn().map_err(|e| OmrError::Failed(e.to_string()))?;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(e) => return Err(OmrError::Failed(e.to_string())),
            }
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OmrError::Cancelled);
            }
            if t0.elapsed() > run.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OmrError::TimedOut(run.timeout.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        let tail: String = text.lines().filter(|l| l.contains("WARN") || l.contains("ERROR") || l.contains("check")).rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        let Some(f) = find_output(&out_dir) else { return Err(OmrError::NoScore(tail)) };
        let bytes = std::fs::read(&f).map_err(|e| OmrError::Failed(e.to_string()))?;
        let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("photo");
        let ext = f.extension().and_then(|x| x.to_str()).unwrap_or("mxl");
        Ok(OmrOutput { name: format!("{stem}.{ext}"), bytes, seconds: t0.elapsed().as_secs_f64(), log: tail })
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

// ---------------------------------------------------------------------------------------------------------------
// From Audiveris's MusicXML to a score to check
// ---------------------------------------------------------------------------------------------------------------

/// The score read from Audiveris's output, with the piano put back together, and the status-bar line.
pub fn to_score(out: &OmrOutput) -> Result<(Score, String), String> {
    let imp = crate::musicxml::import(&out.name, &out.bytes)?;
    let mut sc = imp.score;
    let merged = join_split_staves(&mut sc);
    let mut note = format!("{CHECK_IT} ({:.0} s with Audiveris", out.seconds);
    if merged > 0 {
        note.push_str(&format!("; {merged} staves joined"));
    }
    note.push_str(").");
    if !imp.skipped.is_empty() {
        let parts: Vec<String> = imp.skipped.iter().map(|(k, n)| if *n > 1 { format!("{k} ×{n}") } else { k.clone() }).collect();
        note.push_str(&format!(" Not imported: {}.", parts.join(", ")));
    }
    Ok((sc, note))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Upper,
    Lower,
}

fn role(st: &Staff, evs: &[Vec<Event>]) -> Role {
    let clef = (0..evs.len()).find(|&m| evs[m].iter().any(|e| !e.is_rest())).map(|m| st.clef_at(m)).unwrap_or(st.clef_at(0));
    match clef {
        Clef::Bass | Clef::Bass8vb => Role::Lower,
        Clef::Treble | Clef::Treble8va | Clef::Treble8vb | Clef::Percussion => Role::Upper,
        // C clefs: by where the notes are
        Clef::Alto | Clef::Tenor => {
            let ps: Vec<i32> = evs.iter().flatten().flat_map(|e| e.pitches.iter().map(|p| p.midi())).collect();
            if !ps.is_empty() && ps.iter().sum::<i32>() / (ps.len() as i32) < 57 { Role::Lower } else { Role::Upper }
        }
    }
}

/// Audiveris names a part from the text beside its first system ("Piano", "Pia.") or calls it "Voice".
fn same_instrument(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().trim_end_matches('.').to_lowercase();
    let (a, b) = (n(a), n(b));
    a.is_empty() || b.is_empty() || a == "voice" || b == "voice" || a == b || (a.len() >= 3 && b.starts_with(&a)) || (b.len() >= 3 && a.starts_with(&b))
}

/// Audiveris often splits one instrument into a part per system (the piano of the QA study came out as "Piano" for
/// measures 4–6 and two single staves for the others), or a grand staff into two parts. Staves of the same
/// instrument and register that never play in the same measure are joined into one (clef changes kept), and when
/// what is left is one upper and one lower staff, they become a braced grand staff. Returns how many staves were
/// joined into others.
pub fn join_split_staves(sc: &mut Score) -> usize {
    let n = sc.measures.len();
    let played = |sc: &Score, s: usize| -> Vec<bool> { (0..n).map(|m| sc.events[s][m].iter().any(|e| !e.is_rest())).collect() };
    let mut joined = 0;
    let mut s = 1;
    while s < sc.staves.len() {
        let rs = role(&sc.staves[s], &sc.events[s]);
        let ps = played(sc, s);
        // the first earlier staff of the same register and instrument that is silent wherever this one plays;
        // a braced (piano) staff is preferred
        let mut target: Option<usize> = None;
        for t in 0..s {
            let pt = played(sc, t);
            if role(&sc.staves[t], &sc.events[t]) != rs || !same_instrument(&sc.staves[t].name, &sc.staves[s].name) || (0..n).any(|m| ps[m] && pt[m]) {
                continue;
            }
            let braced = |i: usize| sc.staves[i].brace_with_next || (i > 0 && sc.staves[i - 1].brace_with_next);
            if target.is_none() || (braced(t) && !braced(target.unwrap())) {
                target = Some(t);
            }
        }
        let Some(t) = target else {
            s += 1;
            continue;
        };
        // move the measures where s plays, with its clef there
        let base_clef: Vec<Clef> = (0..n).map(|m| sc.staves[t].clef_at(m)).collect();
        let mut clef_of: Vec<Clef> = base_clef.clone();
        for m in 0..n {
            if ps[m] {
                sc.events[t][m] = std::mem::take(&mut sc.events[s][m]);
                clef_of[m] = sc.staves[s].clef_at(m);
            }
        }
        let mut clefs: Vec<(usize, Clef)> = vec![];
        for (m, c) in clef_of.iter().enumerate() {
            if m == 0 || clefs.last().map(|x| x.1) != Some(*c) {
                clefs.push((m, *c));
            }
        }
        sc.staves[t].clefs = clefs;
        if sc.staves[t].name.trim().is_empty() || sc.staves[t].name.eq_ignore_ascii_case("voice") {
            let nm = sc.staves[s].name.clone();
            if !nm.trim().is_empty() && !nm.eq_ignore_ascii_case("voice") {
                sc.staves[t].name = nm;
            }
        }
        // its slurs, hairpins and texts go with its notes
        for g in sc.graphics.iter_mut().filter(|g| g.staff == s) {
            g.staff = t;
        }
        sc.delete_staff(s);
        joined += 1;
    }
    // one upper and one lower staff left: a piano-style grand staff
    if sc.staves.len() == 2 && role(&sc.staves[0], &sc.events[0]) == Role::Upper && role(&sc.staves[1], &sc.events[1]) == Role::Lower && same_instrument(&sc.staves[0].name, &sc.staves[1].name) {
        let name = [&sc.staves[0].name, &sc.staves[1].name].into_iter().find(|x| !x.trim().is_empty() && !x.eq_ignore_ascii_case("voice")).cloned().unwrap_or_else(|| "Piano".into());
        let name = if name.to_lowercase().starts_with("pia") { "Piano".to_string() } else { name };
        sc.staves[0].name = name.clone();
        sc.staves[0].abbrev = if name == "Piano" { "Pno.".into() } else { sc.staves[0].abbrev.clone() };
        sc.staves[0].brace_with_next = true;
        sc.staves[0].bracket_with_next = false;
        sc.staves[0].barline_to_next = true;
        sc.staves[1].name = String::new();
        sc.staves[1].abbrev = String::new();
        let ch = sc.staves[0].channel;
        sc.staves[1].channel = ch;
        sc.staves[1].program = sc.staves[0].program;
    }
    joined
}

/// Pitch accuracy against a reference score: for the upper and the lower register, the longest common run of the
/// notes' pitches in time order (chords bottom to top) over the reference's count. (pitches right, reference notes)
pub fn pitch_match(reference: &Score, read: &Score) -> (usize, usize) {
    fn seq(sc: &Score, r: Role) -> Vec<i32> {
        let mut v: Vec<(usize, u32, i32)> = vec![];
        for s in 0..sc.staves.len() {
            if role(&sc.staves[s], &sc.events[s]) != r {
                continue;
            }
            for (m, evs) in sc.events[s].iter().enumerate() {
                for e in evs.iter().filter(|e| !e.grace) {
                    for p in &e.pitches {
                        v.push((m, e.start, p.midi()));
                    }
                }
            }
        }
        // measures can be off by one in OMR output: order by time within the piece, not by measure number
        v.sort();
        v.into_iter().map(|x| x.2).collect()
    }
    fn lcs(a: &[i32], b: &[i32]) -> usize {
        let mut prev = vec![0usize; b.len() + 1];
        for x in a {
            let mut cur = vec![0usize; b.len() + 1];
            for (j, y) in b.iter().enumerate() {
                cur[j + 1] = if x == y { prev[j] + 1 } else { cur[j].max(prev[j + 1]) };
            }
            prev = cur;
        }
        prev[b.len()]
    }
    let mut ok = 0;
    let mut total = 0;
    for r in [Role::Upper, Role::Lower] {
        let (a, b) = (seq(reference, r), seq(read, r));
        ok += lcs(&a, &b);
        total += a.len();
    }
    (ok, total)
}

#[path = "omr_setup.rs"]
pub mod setup;

#[cfg(test)]
#[path = "omr_tests.rs"]
mod tests;
