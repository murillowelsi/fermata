//! Automatic backup and recovery (F-28). While a score has unsaved changes, a copy is written every minute and
//! whenever the window loses focus: in the app's support folder (`Autosave/`), in the browser in IndexedDB. The
//! person's own file is never touched. Saving, or closing normally (Save or Don't Save answered), deletes the copy.
//!
//! A copy left behind means the app ended unexpectedly. Each running app touches a heartbeat next to its copy every
//! few seconds, so the copy of a score open in another window (or tab) is not mistaken for a lost one: only copies
//! whose heartbeat is older than `STALE` are offered back ("Recover unsaved changes to ‹name›?").
use serde::{Deserialize, Serialize};

/// How often a score with unsaved changes is copied (seconds).
pub const EVERY: f64 = 60.0;
/// How often a running app says it is still there (seconds).
pub const HEARTBEAT: f64 = 10.0;
/// A copy whose app hasn't said so for this long (seconds) was left behind. Generous, because a browser slows the
/// timers of a background tab down to once a minute, and that tab is still alive.
pub const STALE: f64 = 120.0;
/// How often a running app looks again for copies left behind (seconds).
pub const RESCAN: f64 = 30.0;

/// One backup: what to show and where it came from, and the score itself (the native file format).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    /// The name shown in the question: the file name, or the score's title when it was never saved.
    pub name: String,
    /// The file the score came from, if any: recovering keeps it, so Save writes there again.
    pub path: Option<String>,
    /// Unix time of the copy (ms).
    pub saved_at: f64,
    /// The score, in the native .fmt format (JSON).
    pub score: String,
}

/// Unix time in milliseconds, on the desktop and in the browser.
pub fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    #[cfg(target_arch = "wasm32")]
    let d = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH);
    d.map(|t| t.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// A new id for a score open in this app.
pub fn new_id() -> String {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    let pid = std::process::id();
    #[cfg(target_arch = "wasm32")]
    let pid = crate::clock::subsec_nanos();
    format!("{:x}-{:x}-{n}", now_ms() as u64, pid)
}

/// Whether a copy was left behind: its app hasn't touched the heartbeat for `STALE` seconds (or never did).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn is_orphan(heartbeat_ms: Option<f64>, now: f64) -> bool {
    heartbeat_ms.is_none_or(|t| now - t > STALE * 1000.0)
}

// ---------------- the desktop: files in the support folder ----------------

#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use super::*;
    use std::path::{Path, PathBuf};

    /// `<support folder>/Autosave`.
    pub fn default_dir() -> Option<PathBuf> {
        crate::prefs::dir().map(|d| d.join("Autosave"))
    }

    fn file(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.fermata-backup"))
    }
    fn alive(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.alive"))
    }

    /// Writes the copy (to a temporary name first, so a crash mid-write never leaves half a copy).
    pub fn write(dir: &Path, b: &Backup) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec(b).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{}.tmp", b.id));
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, file(dir, &b.id)).map_err(|e| e.to_string())?;
        heartbeat(dir, &b.id);
        Ok(())
    }

    /// This app is still running with this score.
    pub fn heartbeat(dir: &Path, id: &str) {
        let _ = std::fs::write(alive(dir, id), now_ms().to_string());
    }

    pub fn delete(dir: &Path, id: &str) {
        let _ = std::fs::remove_file(file(dir, id));
        let _ = std::fs::remove_file(alive(dir, id));
    }

    /// The copies left behind by an app that ended unexpectedly, newest first.
    pub fn orphans(dir: &Path) -> Vec<Backup> {
        let now = now_ms();
        let mut out = vec![];
        let Ok(rd) = std::fs::read_dir(dir) else { return out };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("fermata-backup") {
                continue;
            }
            let Some(id) = p.file_stem().and_then(|x| x.to_str()).map(str::to_string) else { continue };
            let hb = std::fs::read_to_string(alive(dir, &id)).ok().and_then(|s| s.trim().parse::<f64>().ok());
            if !is_orphan(hb, now) {
                continue;
            }
            match std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Backup>(&b).ok()) {
                Some(b) => out.push(b),
                // unreadable: nothing to recover
                None => delete(dir, &id),
            }
        }
        out.sort_by(|a, b| b.saved_at.partial_cmp(&a.saved_at).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("fermata-autosave-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn backup(id: &str, name: &str) -> Backup {
        Backup { id: id.into(), name: name.into(), path: None, saved_at: now_ms(), score: "{}".into() }
    }

    #[test]
    fn orphan_rule() {
        let now = 1_000_000.0;
        assert!(is_orphan(None, now));
        assert!(is_orphan(Some(now - 121_000.0), now));
        assert!(!is_orphan(Some(now - 60_000.0), now), "a background tab slowed to a beat a minute is still alive");
    }

    #[test]
    fn a_copy_whose_app_is_alive_is_not_offered() {
        let d = tmp_dir("alive");
        store::write(&d, &backup("a", "Song.fmt")).unwrap();
        assert!(store::orphans(&d).is_empty(), "just written: its app is running");
        // the app went away: its heartbeat grows old
        std::fs::write(d.join("a.alive"), (now_ms() - 200_000.0).to_string()).unwrap();
        let o = store::orphans(&d);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].name, "Song.fmt");
        store::delete(&d, "a");
        assert!(store::orphans(&d).is_empty());
        assert!(!d.join("a.fermata-backup").exists() && !d.join("a.alive").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn several_scores_each_their_own_copy() {
        let d = tmp_dir("several");
        for (id, n) in [("x", "One"), ("y", "Two"), ("z", "Three")] {
            store::write(&d, &backup(id, n)).unwrap();
            std::fs::remove_file(d.join(format!("{id}.alive"))).unwrap();
        }
        let mut names: Vec<String> = store::orphans(&d).into_iter().map(|b| b.name).collect();
        names.sort();
        assert_eq!(names, vec!["One", "Three", "Two"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_damaged_copy_is_dropped_quietly() {
        let d = tmp_dir("damaged");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("bad.fermata-backup"), b"not json").unwrap();
        assert!(store::orphans(&d).is_empty());
        assert!(!d.join("bad.fermata-backup").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}

// ---------------- the browser: IndexedDB (see web.rs) ----------------

#[cfg(target_arch = "wasm32")]
pub mod store {
    use super::*;

    pub fn write(b: &Backup) {
        if let Ok(json) = serde_json::to_string(b) {
            crate::web::backup_put(&b.id, &json);
        }
    }
    pub fn heartbeat(id: &str) {
        crate::web::backup_alive(id);
    }
    pub fn delete(id: &str) {
        crate::web::backup_delete(id);
    }
    /// Starts looking for copies left behind; they arrive through `take_orphans` on a later frame.
    pub fn start_scan() {
        crate::web::backup_scan((STALE * 1000.0) as f64);
    }
    pub fn take_orphans() -> Vec<Backup> {
        let mut out: Vec<Backup> = crate::web::backup_found().iter().filter_map(|j| serde_json::from_str(j).ok()).collect();
        out.sort_by(|a, b| b.saved_at.partial_cmp(&a.saved_at).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

// ---------------- the app ----------------

/// The app's backup state for the score it has open.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// Off in screenshots and tests (unless a test gives a folder).
    pub enabled: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub dir: Option<std::path::PathBuf>,
    /// This score's id, and whether a copy of it exists.
    pub id: String,
    pub has_copy: bool,
    /// The score as last copied, so an unchanged score isn't written again.
    pub last_bytes: Vec<u8>,
    /// Times (input clock, s) of the last copy and the last heartbeat; when to look for left-behind copies again.
    pub last_write: f64,
    pub last_beat: f64,
    pub scanned: bool,
    pub rescan_at: Option<f64>,
    /// Copies left behind, waiting to be offered one by one.
    pub queue: Vec<Backup>,
    pub was_focused: bool,
}

impl State {
    pub fn new(enabled: bool) -> State {
        State {
            enabled,
            #[cfg(not(target_arch = "wasm32"))]
            dir: if enabled { store::default_dir() } else { None },
            id: new_id(),
            was_focused: true,
            ..Default::default()
        }
    }
}

use crate::app::{App, Dialog};

impl App {
    fn backup_write(&mut self, b: &Backup) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(d) = self.autosave.dir.clone() {
            if let Err(e) = store::write(&d, b) {
                self.status = format!("Couldn't write the automatic backup: {e}");
                return;
            }
        }
        #[cfg(target_arch = "wasm32")]
        store::write(b);
        self.autosave.has_copy = true;
    }

    fn backup_delete(&mut self) {
        if !self.autosave.has_copy {
            return;
        }
        let id = self.autosave.id.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(d) = &self.autosave.dir {
            store::delete(d, &id);
        }
        #[cfg(target_arch = "wasm32")]
        store::delete(&id);
        self.autosave.has_copy = false;
        self.autosave.last_bytes.clear();
    }

    /// The score was replaced (new, opened, closed) after the person answered for its changes: its copy goes, and
    /// the next score gets its own id.
    pub fn autosave_new_document(&mut self) {
        self.backup_delete();
        self.autosave.id = new_id();
    }

    /// Closing the app normally: nothing to recover.
    pub fn autosave_exit(&mut self) {
        self.backup_delete();
    }

    /// The name shown in the recovery question.
    fn backup_name(&self) -> String {
        self.doc_name()
    }

    /// Every frame: `now` the input clock (s), `focused` whether the window (or tab) has focus.
    pub fn autosave_tick(&mut self, now: f64, focused: bool) {
        if !self.autosave.enabled {
            return;
        }
        // look for copies left behind at the start and then every half minute (a copy whose app just ended is only
        // recognised once its heartbeat is old)
        let rescan = self.autosave.rescan_at.is_some_and(|t| now >= t);
        if !self.autosave.scanned || rescan {
            self.autosave.rescan_at = Some(now + RESCAN);
            self.autosave.scanned = true;
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(d) = self.autosave.dir.clone() {
                let mine = self.autosave.id.clone();
                for b in store::orphans(&d) {
                    if b.id != mine && !self.autosave.queue.iter().any(|q| q.id == b.id) {
                        self.autosave.queue.push(b);
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            store::start_scan();
        }
        #[cfg(target_arch = "wasm32")]
        for b in store::take_orphans() {
            if b.id != self.autosave.id && !self.autosave.queue.iter().any(|q| q.id == b.id) {
                self.autosave.queue.push(b);
            }
        }
        // offer one at a time, only over a score with nothing to lose
        if self.dialog.is_none() && !self.dirty && !self.autosave.queue.is_empty() {
            let b = self.autosave.queue.remove(0);
            self.dialog = Some(Dialog::Recover(Box::new(b)));
        }

        let focus_lost = self.autosave.was_focused && !focused;
        self.autosave.was_focused = focused;
        if self.dirty {
            if focus_lost || now - self.autosave.last_write >= EVERY {
                self.autosave.last_write = now;
                if let Ok(bytes) = crate::io::score_bytes(&self.score) {
                    if bytes != self.autosave.last_bytes || !self.autosave.has_copy {
                        let b = Backup {
                            id: self.autosave.id.clone(),
                            name: self.backup_name(),
                            path: self.path.as_ref().map(|p| p.to_string_lossy().to_string()),
                            saved_at: now_ms(),
                            score: String::from_utf8_lossy(&bytes).into_owned(),
                        };
                        self.backup_write(&b);
                        self.autosave.last_bytes = bytes;
                        self.autosave.last_beat = now;
                    }
                }
            }
        } else {
            // saved (or nothing changed since): no copy needed; the next change starts a fresh minute
            self.backup_delete();
            self.autosave.last_write = now;
        }
        if self.autosave.has_copy && now - self.autosave.last_beat >= HEARTBEAT {
            self.autosave.last_beat = now;
            let id = self.autosave.id.clone();
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(d) = &self.autosave.dir {
                store::heartbeat(d, &id);
            }
            #[cfg(target_arch = "wasm32")]
            store::heartbeat(&id);
        }
    }

    /// "Recover unsaved changes to ‹name›?": Recover brings the copy back as an unsaved score (Save writes to its
    /// file, if it had one); Discard deletes the copy.
    pub fn answer_recover(&mut self, b: Backup, recover: bool) {
        if !recover {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(d) = &self.autosave.dir {
                store::delete(d, &b.id);
            }
            #[cfg(target_arch = "wasm32")]
            store::delete(&b.id);
            self.status = format!("Discarded the automatic backup of {}.", b.name);
            return;
        }
        match crate::io::open_bytes("backup.fmt", b.score.as_bytes()) {
            Ok(sc) => {
                self.autosave_new_document();
                self.new_score(sc);
                self.path = b.path.as_ref().map(std::path::PathBuf::from);
                self.dirty = true;
                // the copy now belongs to this app: it stays until the score is saved or closed
                self.autosave.id = b.id.clone();
                self.autosave.has_copy = true;
                self.autosave.last_bytes = b.score.into_bytes();
                self.status = format!("Recovered the unsaved changes to {}. Save to keep them.", b.name);
            }
            Err(e) => self.dialog = Some(Dialog::Message(crate::io::open_error(&b.name, &e))),
        }
    }
}

#[cfg(test)]
mod app_tests {
    use super::*;
    use crate::model::Pitch;

    fn app_in(tag: &str) -> (App, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("fermata-autosave-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut a = App::new(true);
        a.autosave.enabled = true;
        a.autosave.dir = Some(d.clone());
        (a, d)
    }

    fn edit(a: &mut App) {
        a.enter(0, 0, 0, Some(Pitch::from_midi(60, false)), 3, 0, false);
    }

    fn copies(d: &std::path::Path) -> usize {
        std::fs::read_dir(d).map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "fermata-backup")).count()).unwrap_or(0)
    }

    /// The app "crashes": its heartbeat is made old, as if it stopped long ago.
    fn crash(a: &App, d: &std::path::Path) {
        std::fs::write(d.join(format!("{}.alive", a.autosave.id)), "0").unwrap();
    }

    #[test]
    fn writes_every_minute_and_on_focus_loss_only_with_changes() {
        let (mut a, d) = app_in("write");
        a.autosave_tick(0.0, true);
        assert_eq!(copies(&d), 0, "nothing changed: no copy");
        edit(&mut a);
        a.autosave_tick(10.0, true);
        assert_eq!(copies(&d), 0, "not a minute yet");
        a.autosave_tick(11.0, false);
        assert_eq!(copies(&d), 1, "the window lost focus");
        a.autosave_tick(80.0, false);
        assert_eq!(copies(&d), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn saving_or_closing_deletes_the_copy() {
        let (mut a, d) = app_in("save");
        edit(&mut a);
        a.autosave_tick(61.0, true);
        assert_eq!(copies(&d), 1);
        a.dirty = false; // saved
        a.autosave_tick(62.0, true);
        assert_eq!(copies(&d), 0);
        edit(&mut a);
        a.autosave_tick(130.0, true);
        assert_eq!(copies(&d), 1);
        a.autosave_exit();
        assert_eq!(copies(&d), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recovery_after_an_unexpected_end() {
        let (mut a, d) = app_in("recover");
        a.score.text.title = "Estudo".into();
        edit(&mut a);
        a.autosave_tick(61.0, true);
        let before = a.score.clone();
        crash(&a, &d);
        // the app starts again
        let mut b = App::new(true);
        b.autosave.enabled = true;
        b.autosave.dir = Some(d.clone());
        b.autosave_tick(0.0, true);
        let Some(Dialog::Recover(bk)) = b.dialog.take() else { panic!("no recovery question") };
        assert_eq!(bk.name, "Estudo");
        b.answer_recover(*bk, true);
        assert_eq!(b.score, before);
        assert!(b.dirty, "recovered changes are still unsaved");
        assert_eq!(copies(&d), 1, "the copy stays until the score is saved");
        b.dirty = false;
        b.autosave_tick(1.0, true);
        assert_eq!(copies(&d), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn discard_deletes_the_copy() {
        let (mut a, d) = app_in("discard");
        edit(&mut a);
        a.autosave_tick(61.0, true);
        crash(&a, &d);
        let mut b = App::new(true);
        b.autosave.enabled = true;
        b.autosave.dir = Some(d.clone());
        b.autosave_tick(0.0, true);
        let Some(Dialog::Recover(bk)) = b.dialog.take() else { panic!("no recovery question") };
        let fresh = b.score.clone();
        b.answer_recover(*bk, false);
        assert_eq!(b.score, fresh, "discard keeps the new score");
        assert_eq!(copies(&d), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn several_scores_are_offered_one_after_another() {
        let (_, d) = app_in("several");
        for title in ["One", "Two"] {
            let mut a = App::new(true);
            a.autosave.enabled = true;
            a.autosave.dir = Some(d.clone());
            a.score.text.title = title.into();
            edit(&mut a);
            a.autosave_tick(61.0, true);
            crash(&a, &d);
        }
        let mut b = App::new(true);
        b.autosave.enabled = true;
        b.autosave.dir = Some(d.clone());
        b.autosave_tick(0.0, true);
        let mut seen = vec![];
        for t in 1..=2 {
            let Some(Dialog::Recover(bk)) = b.dialog.take() else { panic!("question {t} missing") };
            seen.push(bk.name.clone());
            b.answer_recover(*bk, false);
            b.autosave_tick(t as f64, true);
        }
        seen.sort();
        assert_eq!(seen, vec!["One", "Two"]);
        assert!(b.dialog.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_left_behind_later_is_found_by_the_next_look() {
        let (mut b, d) = app_in("later");
        b.autosave_tick(0.0, true);
        assert!(b.dialog.is_none());
        // meanwhile another window with unsaved changes ends unexpectedly
        let mut a = App::new(true);
        a.autosave.enabled = true;
        a.autosave.dir = Some(d.clone());
        edit(&mut a);
        a.autosave_tick(61.0, true);
        crash(&a, &d);
        b.autosave_tick(10.0, true);
        assert!(b.dialog.is_none(), "not looked again yet");
        b.autosave_tick(RESCAN + 1.0, true);
        assert!(matches!(b.dialog, Some(Dialog::Recover(_))));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_of_a_score_open_elsewhere_is_left_alone() {
        let (mut a, d) = app_in("elsewhere");
        edit(&mut a);
        a.autosave_tick(61.0, true);
        // a second window starts while the first is running
        let mut b = App::new(true);
        b.autosave.enabled = true;
        b.autosave.dir = Some(d.clone());
        b.autosave_tick(0.0, true);
        assert!(b.dialog.is_none());
        assert_eq!(copies(&d), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
