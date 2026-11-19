//! F-34: Fermata updates itself (Mac and Windows; the browser has its service worker).
//!
//! - **Check:** the latest release from GitHub's API, over HTTPS only, each time Fermata opens, in the background, with a
//!   short timeout. No connection: nothing happens.
//! - **Download:** the update file and its `.sig` (signed by the release, see `update_sig.rs`), through the system's
//!   own `curl` (macOS, and Windows 10 and later, ship it), so no HTTP or TLS library is built into the app.
//! - **Verify:** nothing is installed without a valid Ed25519 signature and a matching SHA-256.
//! - **Install:** Mac: the new Fermata.app is unpacked with `ditto`; once the app has quit, a small helper swaps the
//!   bundle in place (putting the old one back if anything fails) and opens it again. Windows: the running
//!   `fermata.exe` is renamed to `.old` (deleted at the next start) and the new one takes its place, with the
//!   SoundFont. Any failure leaves the current version as it was.
use crate::update_sig;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const API: &str = "https://api.github.com/repos/murillowelsi/fermata/releases/latest";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The public key that update files must be signed with (hex). The release's key goes in this file; an empty file
/// means this build installs no updates (they all fail the security check).
pub fn public_key() -> &'static str {
    include_str!("../assets/update/fermata-update.pub").trim()
}

/// The release asset this computer installs.
pub fn asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Fermata-Windows-x64.zip"
    } else {
        "Fermata-macOS-update.zip"
    }
}

/// Where to download Fermata by hand when it can't update itself.
pub fn download_page() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://murillowelsi.github.io/fermata/windows/"
    } else {
        "https://murillowelsi.github.io/fermata/mac/"
    }
}

/// The release notes page of a version.
pub fn release_page(version: &str) -> String {
    format!("https://github.com/murillowelsi/fermata/releases/tag/v{version}")
}

// ---------------- versions ----------------

/// A version as (major, minor, patch, pre-release); "v0.3.12" and "0.3.12" read the same.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (s, None),
    };
    let core = core.split('+').next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

/// Is `tag` a newer version than `current`? A pre-release comes before its release (0.3.12-beta < 0.3.12).
pub fn is_newer(tag: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (parse_version(tag), parse_version(current)) else { return false };
    let core = (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2));
    match core {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (&a.3, &b.3) {
            (None, Some(_)) => true,
            (Some(x), Some(y)) => x > y,
            _ => false,
        },
    }
}

// ---------------- releases ----------------

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// "0.3.12" (the tag without its "v")
    pub version: String,
    /// the release's page on GitHub
    pub page: String,
    /// up to three lines of what's new
    pub notes: Vec<String>,
    /// this computer's update file and its signature
    pub asset: Option<Asset>,
    pub sig: Option<Asset>,
}

/// GitHub's answer for a release.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("unexpected answer: no tag")?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    parse_version(&version).ok_or_else(|| format!("unexpected version {tag}"))?;
    let page = v["html_url"].as_str().map(|s| s.to_string()).unwrap_or_else(|| release_page(&version));
    let notes = release_notes(v["body"].as_str().unwrap_or(""));
    let find = |name: &str| -> Option<Asset> {
        v["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name)).and_then(|a| {
            Some(Asset { name: name.to_string(), url: a["browser_download_url"].as_str()?.to_string(), size: a["size"].as_u64().unwrap_or(0) })
        })
    };
    let asset = find(asset_name());
    let sig = find(&format!("{}.sig", asset_name()));
    Ok(Release { version, page, notes, asset, sig })
}

/// What's new: the first three items of the first list in the release notes (lines that start with "- "),
/// without bold marks. No list: nothing (the card shows only the "Release notes" link).
pub fn release_notes(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_list = false;
    for line in body.lines() {
        if let Some(item) = line.strip_prefix("- ") {
            in_list = true;
            let t = item.replace("**", "").replace("__", "");
            let t = t.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            if out.len() == 3 {
                break;
            }
        } else if in_list && !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            // the first list ended (indented lines are its sub-items)
            break;
        }
    }
    out
}

// ---------------- network (the system's curl) ----------------

fn curl() -> Command {
    let program = if cfg!(target_os = "windows") {
        std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("curl.exe")).filter(|p| p.exists()).unwrap_or_else(|| PathBuf::from("curl.exe"))
    } else {
        PathBuf::from("/usr/bin/curl")
    };
    let mut c = Command::new(program);
    // HTTPS only, also after redirects (GitHub's downloads redirect to its file servers)
    c.args(["--proto", "=https", "--proto-redir", "=https", "--tlsv1.2", "--fail", "--location", "--silent", "--show-error"]);
    c.args(["--user-agent", &format!("Fermata/{VERSION}")]);
    c.stdin(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c
}

#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// A newer version.
    Newer(Release),
    /// This is the latest (the latest release's version).
    Latest(String),
}

/// Asks GitHub for the latest release (a few seconds at most). Err: no connection or no answer.
pub fn check_now(timeout_secs: u32) -> Result<Check, String> {
    let out = curl()
        .args(["--max-time", &timeout_secs.to_string(), "--header", "Accept: application/vnd.github+json", API])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("can't run curl: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let rel = parse_release(&String::from_utf8_lossy(&out.stdout))?;
    Ok(if is_newer(&rel.version, VERSION) { Check::Newer(rel) } else { Check::Latest(rel.version) })
}

/// A check running in the background.
pub struct CheckJob {
    rx: mpsc::Receiver<Result<Check, String>>,
}

impl CheckJob {
    pub fn start() -> CheckJob {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(check_now(8));
        });
        CheckJob { rx }
    }
    pub fn try_result(&self) -> Option<Result<Check, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the check stopped".into())),
        }
    }
}

// ---------------- errors ----------------

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateError {
    /// The download didn't finish (connection, server, cut short).
    Download(String),
    /// The file didn't pass the security check (signature, SHA-256, not a Fermata app): it isn't installed.
    Security(String),
    /// Fermata can't replace itself from where it runs (inside the .dmg, a translocated copy, a read-only folder).
    Folder(String),
    /// No space left on the disk.
    Disk,
    Cancelled,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Download(e) => write!(f, "The download didn't finish ({e})."),
            UpdateError::Security(e) => write!(f, "The update didn't pass the security check, so it wasn't installed ({e})."),
            UpdateError::Folder(e) => write!(f, "Fermata can't replace itself from here ({e})."),
            UpdateError::Disk => write!(f, "There isn't enough space on the disk for the update."),
            UpdateError::Cancelled => write!(f, "The update was cancelled."),
        }
    }
}

fn io_err(e: std::io::Error) -> UpdateError {
    // ENOSPC on Mac and Linux; ERROR_DISK_FULL and ERROR_HANDLE_DISK_FULL on Windows
    match e.raw_os_error() {
        Some(28) if !cfg!(target_os = "windows") => UpdateError::Disk,
        Some(112) | Some(39) if cfg!(target_os = "windows") => UpdateError::Disk,
        _ => UpdateError::Download(e.to_string()),
    }
}

/// The folder where downloads and the unpacked update wait.
pub fn work_dir() -> PathBuf {
    std::env::temp_dir().join("Fermata Update")
}

// ---------------- download ----------------

/// Checks an update file against its signature: the file must be this computer's update file, unchanged, signed
/// with the key built into the app.
pub fn verify(zip: &[u8], sig: &str) -> Result<(), UpdateError> {
    update_sig::verify(public_key(), asset_name(), zip, sig).map_err(UpdateError::Security)
}

/// The update file being downloaded in the background.
pub struct DownloadJob {
    progress: Arc<Mutex<(u64, u64)>>,
    cancel: Arc<AtomicBool>,
    started: Instant,
    rx: mpsc::Receiver<Result<PathBuf, UpdateError>>,
}

impl DownloadJob {
    pub fn start(rel: &Release) -> DownloadJob {
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(Mutex::new((0, rel.asset.as_ref().map(|a| a.size).unwrap_or(0))));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c, rel) = (progress.clone(), cancel.clone(), rel.clone());
        std::thread::spawn(move || {
            let _ = tx.send(download(&rel, &work_dir(), &p, &c));
        });
        DownloadJob { progress, cancel, started: Instant::now(), rx }
    }
    /// Bytes so far and the total (0 when unknown).
    pub fn progress(&self) -> (u64, u64) {
        *self.progress.lock().unwrap()
    }
    /// Seconds left at the speed so far.
    pub fn secs_left(&self) -> Option<f64> {
        let (done, total) = self.progress();
        let t = self.started.elapsed().as_secs_f64();
        if done == 0 || total == 0 || t < 1.0 {
            return None;
        }
        Some((total.saturating_sub(done)) as f64 / (done as f64 / t))
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn try_result(&self) -> Option<Result<PathBuf, UpdateError>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(UpdateError::Download("the download stopped".into()))),
        }
    }
}

fn download(rel: &Release, dir: &Path, progress: &Mutex<(u64, u64)>, cancel: &AtomicBool) -> Result<PathBuf, UpdateError> {
    let (Some(asset), Some(sig)) = (&rel.asset, &rel.sig) else {
        return Err(UpdateError::Download(format!("the release has no {} with a signature", asset_name())));
    };
    std::fs::create_dir_all(dir).map_err(io_err)?;
    let zip = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let sig_path = dir.join(&sig.name);
    let _ = std::fs::remove_file(&part);
    // the signature first (small)
    let out = curl().args(["--max-time", "30", "--output"]).arg(&sig_path).arg(&sig.url).stderr(Stdio::piped()).output().map_err(io_err)?;
    if !out.status.success() {
        return Err(UpdateError::Download(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    // the file, watching its size grow; Cancel stops curl and deletes what came
    let mut child = curl()
        .args(["--connect-timeout", "15", "--speed-limit", "1024", "--speed-time", "60", "--output"])
        .arg(&part)
        .arg(&asset.url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_err)?;
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err(UpdateError::Cancelled);
        }
        if let Ok(m) = std::fs::metadata(&part) {
            progress.lock().unwrap().0 = m.len();
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => return Err(io_err(e)),
        }
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            use std::io::Read;
            let _ = e.read_to_string(&mut err);
        }
        let _ = std::fs::remove_file(&part);
        // curl's 23: it couldn't write the file (the disk is full)
        return Err(if status.code() == Some(23) { UpdateError::Disk } else { UpdateError::Download(err.trim().to_string()) });
    }
    let data = std::fs::read(&part).map_err(io_err)?;
    progress.lock().unwrap().0 = data.len() as u64;
    if asset.size > 0 && data.len() as u64 != asset.size {
        let _ = std::fs::remove_file(&part);
        return Err(UpdateError::Download(format!("{} of {} bytes arrived", data.len(), asset.size)));
    }
    let sig_text = std::fs::read_to_string(&sig_path).map_err(io_err)?;
    if let Err(e) = verify(&data, &sig_text) {
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_file(&sig_path);
        return Err(e);
    }
    std::fs::rename(&part, &zip).map_err(io_err)?;
    Ok(zip)
}

// ---------------- install ----------------

/// Where this copy of Fermata lives, if it can replace itself there.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Mac { app: PathBuf },
    Windows { dir: PathBuf, exe: PathBuf },
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".fermata-update-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// This copy's place, or why it can't update itself (then the interface offers the download page).
pub fn target() -> Result<Target, UpdateError> {
    let exe = std::env::current_exe().map_err(|e| UpdateError::Folder(e.to_string()))?;
    target_for(&exe)
}

pub fn target_for(exe: &Path) -> Result<Target, UpdateError> {
    if cfg!(target_os = "windows") {
        let dir = exe.parent().ok_or_else(|| UpdateError::Folder("no folder".into()))?.to_path_buf();
        if !writable(&dir) {
            return Err(UpdateError::Folder(format!("{} is read-only", dir.display())));
        }
        return Ok(Target::Windows { dir, exe: exe.to_path_buf() });
    }
    let s = exe.to_string_lossy();
    if s.contains("/AppTranslocation/") {
        return Err(UpdateError::Folder("macOS runs this copy from a temporary place (move Fermata to Applications)".into()));
    }
    // …/Fermata.app/Contents/MacOS/Fermata
    let app = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).filter(|p| p.extension().map(|e| e == "app").unwrap_or(false));
    let Some(app) = app else {
        return Err(UpdateError::Folder("Fermata isn't running from Fermata.app".into()));
    };
    let parent = app.parent().ok_or_else(|| UpdateError::Folder("no folder".into()))?;
    if !writable(parent) {
        return Err(UpdateError::Folder(format!("{} is read-only (inside the .dmg, or a folder without permission)", parent.display())));
    }
    Ok(Target::Mac { app: app.to_path_buf() })
}

/// The new version unpacked and ready to swap in.
#[derive(Clone, Debug, PartialEq)]
pub enum Staged {
    Mac { app: PathBuf, new_app: PathBuf },
    /// (new file, its final name)
    Windows { exe: PathBuf, files: Vec<(PathBuf, PathBuf)> },
}

/// Unpacks a verified update file for `t`. The current version isn't touched.
pub fn stage(zip: &Path, t: &Target) -> Result<Staged, UpdateError> {
    match t {
        Target::Mac { app } => {
            let dir = work_dir().join(format!("staged-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(io_err)?;
            let out = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(zip).arg(&dir).stderr(Stdio::piped()).output().map_err(io_err)?;
            if !out.status.success() {
                let e = String::from_utf8_lossy(&out.stderr).to_string();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(if e.contains("No space") { UpdateError::Disk } else { UpdateError::Security(format!("couldn't unpack the update: {}", e.trim())) });
            }
            let new_app = dir.join("Fermata.app");
            let plist = std::fs::read_to_string(new_app.join("Contents/Info.plist")).unwrap_or_default();
            if !new_app.join("Contents/MacOS/Fermata").is_file() || !plist.contains("app.fermata.notation") {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(UpdateError::Security("the update isn't a Fermata app".into()));
            }
            Ok(Staged::Mac { app: app.clone(), new_app })
        }
        Target::Windows { dir, exe } => {
            let b = std::fs::read(zip).map_err(io_err)?;
            let entries = crate::musicxml::zip_entries(&b).map_err(|e| UpdateError::Security(e.replace(".mxl", "zip")))?;
            let base = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_lowercase();
            let exe_name = exe.file_name().map(|n| n.to_owned()).unwrap_or_else(|| "fermata.exe".into());
            let mut files = vec![];
            for (want, final_name, required) in [("fermata.exe", PathBuf::from(&exe_name), true), ("generaluser-gs.sf2", PathBuf::from("GeneralUser-GS.sf2"), false)] {
                let Some(e) = entries.iter().find(|e| base(&e.0) == want) else {
                    if required {
                        return Err(UpdateError::Security("the update has no fermata.exe".into()));
                    }
                    continue;
                };
                let data = crate::musicxml::zip_read(&b, e).map_err(|e| UpdateError::Security(e.replace(".mxl", "zip")))?;
                let final_path = dir.join(final_name);
                let new = PathBuf::from(format!("{}.new", final_path.display()));
                if let Err(err) = std::fs::write(&new, &data) {
                    for (n, _) in &files {
                        let _ = std::fs::remove_file(n);
                    }
                    let _ = std::fs::remove_file(&new);
                    return Err(io_err(err));
                }
                files.push((new, final_path));
            }
            Ok(Staged::Windows { exe: exe.clone(), files })
        }
    }
}

/// The helper that swaps Fermata.app once the app has quit: arguments are the app's process id, the app, the new
/// app, 1 to open it afterwards, and the log file.
const MAC_HELPER: &str = r#"#!/bin/sh
pid="$1"; app="$2"; new="$3"; reopen="$4"; log="$5"
exec >>"$log" 2>&1
echo "$(date): waiting for Fermata ($pid) to quit"
n=0
while kill -0 "$pid" 2>/dev/null; do
  sleep 0.2; n=$((n+1))
  if [ "$n" -gt 900 ]; then echo "Fermata didn't quit; nothing changed"; exit 1; fi
done
old="$app.old-$$"
if ! mv "$app" "$old"; then echo "can't move the old app; nothing changed"; exit 1; fi
if /usr/bin/ditto "$new" "$app"; then
  rm -rf "$old"
  echo "updated $app"
else
  echo "copy failed; putting the old app back"
  rm -rf "$app"
  mv "$old" "$app"
fi
rm -rf "$(dirname "$new")"
if [ "$reopen" = 1 ]; then /usr/bin/open "$app"; fi
"#;

/// Swaps the new version in. Mac: starts the helper, which waits for this process to end (the caller then quits).
/// Windows: renames and moves the files now and, with `reopen`, starts the new Fermata (the caller then quits).
pub fn install(s: &Staged, reopen: bool) -> Result<(), UpdateError> {
    match s {
        Staged::Mac { app, new_app } => {
            let dir = work_dir();
            std::fs::create_dir_all(&dir).map_err(io_err)?;
            let script = dir.join("swap.sh");
            std::fs::write(&script, MAC_HELPER).map_err(io_err)?;
            Command::new("/bin/sh")
                .arg(&script)
                .arg(std::process::id().to_string())
                .arg(app)
                .arg(new_app)
                .arg(if reopen { "1" } else { "0" })
                .arg(dir.join("update.log"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(io_err)?;
            Ok(())
        }
        Staged::Windows { exe, files } => {
            // the running exe can be renamed (not deleted): it goes to .old, deleted at the next start
            let mut done: Vec<(PathBuf, PathBuf)> = vec![];
            for (new, fin) in files {
                let old = PathBuf::from(format!("{}.old", fin.display()));
                let _ = std::fs::remove_file(&old);
                let moved = !fin.exists() || std::fs::rename(fin, &old).is_ok();
                if moved && std::fs::rename(new, fin).is_ok() {
                    done.push((fin.clone(), old));
                    continue;
                }
                // put back everything already swapped; the current version keeps working
                if !fin.exists() && old.exists() {
                    let _ = std::fs::rename(&old, fin);
                }
                for (f, o) in done.iter().rev() {
                    let _ = std::fs::remove_file(f);
                    let _ = std::fs::rename(o, f);
                }
                for (n, _) in files {
                    let _ = std::fs::remove_file(n);
                }
                return Err(UpdateError::Folder("couldn't replace the files in Fermata's folder".into()));
            }
            if reopen {
                Command::new(exe).arg("--updated").spawn().map_err(io_err)?;
            }
            Ok(())
        }
    }
}

/// At start: what a finished update left behind (Windows' `.old` files, unpacked copies).
pub fn cleanup() {
    if cfg!(target_os = "windows") {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().ends_with(".old") {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(work_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("staged-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

/// `--self-update-from <zip> <sig>`: install from local files (to test without publishing) and open the new version.
pub fn self_update_from(zip: &Path, sig: &Path) -> Result<String, UpdateError> {
    let data = std::fs::read(zip).map_err(|e| UpdateError::Download(format!("{}: {e}", zip.display())))?;
    let sig_text = std::fs::read_to_string(sig).map_err(|e| UpdateError::Download(format!("{}: {e}", sig.display())))?;
    verify(&data, &sig_text)?;
    let t = target()?;
    let staged = stage(zip, &t)?;
    install(&staged, true)?;
    Ok(match t {
        Target::Mac { app } => format!("Signature OK. {} will be replaced and reopened once this process ends.", app.display()),
        Target::Windows { dir, .. } => format!("Signature OK. Fermata in {} was replaced and reopened.", dir.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("v0.3.12", "0.3.11"));
        assert!(is_newer("0.4.0", "0.3.11"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.3.10", "0.3.9"), "numbers, not text");
        assert!(!is_newer("0.3.11", "0.3.11"));
        assert!(!is_newer("0.3.9", "0.3.11"));
        assert!(is_newer("0.3.12", "0.3.12-beta"));
        assert!(!is_newer("0.3.12-beta", "0.3.12"));
        assert!(!is_newer("garbage", "0.3.11"));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3, None)));
    }

    #[test]
    fn release_answer_and_whats_new() {
        let body = "Uma linha antes.\n\n**Novidades**\n- **Imprimir:** a página sai igual\n- Page Setup novo\n  - detalhe\n- Export PDF\n- Quarta linha\n\n**Downloads**\n- Mac";
        let j = serde_json::json!({
            "tag_name": "v0.3.12", "html_url": "https://github.com/murillowelsi/fermata/releases/tag/v0.3.12", "body": body,
            "assets": [
                {"name": asset_name(), "browser_download_url": "https://example.invalid/a.zip", "size": 1234},
                {"name": format!("{}.sig", asset_name()), "browser_download_url": "https://example.invalid/a.zip.sig", "size": 300}
            ]
        });
        let r = parse_release(&j.to_string()).unwrap();
        assert_eq!(r.version, "0.3.12");
        assert_eq!(r.notes, vec!["Imprimir: a página sai igual", "Page Setup novo", "Export PDF"]);
        assert_eq!(r.asset.as_ref().unwrap().size, 1234);
        assert!(r.sig.is_some());
        assert!(release_notes("Only a sentence.").is_empty());
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn signatures_valid_and_invalid() {
        let k = key();
        let public = update_sig::hex(k.verifying_key().as_bytes());
        let data = b"the update".to_vec();
        let sig = update_sig::sign(&k, asset_name(), &data);
        assert_eq!(update_sig::verify(&public, asset_name(), &data, &sig), Ok(()));
        // another key
        let other = update_sig::hex(SigningKey::from_bytes(&[9u8; 32]).verifying_key().as_bytes());
        assert!(update_sig::verify(&other, asset_name(), &data, &sig).is_err());
        // the other computer's file
        assert!(update_sig::verify(&public, "Fermata-Other.zip", &data, &sig).is_err());
        // a signature line changed by hand
        let forged = sig.replace("signature ", "signature 00");
        assert!(update_sig::verify(&public, asset_name(), &data, &forged).is_err());
        // a sha256 line rewritten for another file, with the old signature
        let other_data = b"something else".to_vec();
        let swapped = sig.replace(&update_sig::sha256_hex(&data), &update_sig::sha256_hex(&other_data));
        assert!(update_sig::verify(&public, asset_name(), &other_data, &swapped).is_err());
        // a build without a key installs nothing
        assert!(update_sig::verify("", asset_name(), &data, &sig).is_err());
    }

    #[test]
    fn a_tampered_zip_is_rejected() {
        let k = key();
        let public = update_sig::hex(k.verifying_key().as_bytes());
        // a real zip (stored entry) with fermata.exe
        let mut zip = vec![];
        let name = b"fermata.exe";
        let body = b"MZ new fermata";
        zip.extend([0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        zip.extend((body.len() as u32).to_le_bytes());
        zip.extend((body.len() as u32).to_le_bytes());
        zip.extend((name.len() as u16).to_le_bytes());
        zip.extend([0, 0]);
        zip.extend(name);
        zip.extend(body);
        let sig = update_sig::sign(&k, asset_name(), &zip);
        assert!(update_sig::verify(&public, asset_name(), &zip, &sig).is_ok());
        // one byte of the program changed
        let mut bad = zip.clone();
        let at = bad.len() - 3;
        bad[at] ^= 0x20;
        assert!(update_sig::verify(&public, asset_name(), &bad, &sig).is_err());
        // a byte added at the end
        let mut longer = zip.clone();
        longer.push(0);
        assert!(update_sig::verify(&public, asset_name(), &longer, &sig).is_err());
        // the app's own check (the key built into this build) refuses a file signed with a test key
        if public_key() != public {
            assert!(matches!(verify(&zip, &sig), Err(UpdateError::Security(_))));
        }
    }

    #[test]
    fn where_fermata_can_update_itself() {
        if cfg!(target_os = "windows") {
            return;
        }
        let t = target_for(Path::new("/private/var/folders/x/AppTranslocation/ABC/d/Fermata.app/Contents/MacOS/Fermata"));
        assert!(matches!(t, Err(UpdateError::Folder(_))));
        assert!(matches!(target_for(Path::new("/Volumes/Fermata/Fermata.app/Contents/MacOS/Fermata")), Err(UpdateError::Folder(_))));
        assert!(matches!(target_for(Path::new("/usr/local/bin/fermata")), Err(UpdateError::Folder(_))));
        let dir = std::env::temp_dir().join(format!("fermata-f34-{}", std::process::id()));
        let exe = dir.join("Fermata.app/Contents/MacOS/Fermata");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        assert_eq!(target_for(&exe), Ok(Target::Mac { app: dir.join("Fermata.app") }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stored (uncompressed) zip with these files.
    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = vec![];
        let mut central = vec![];
        for (name, body) in files {
            let off = z.len() as u32;
            let mut h = vec![0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            h.extend((body.len() as u32).to_le_bytes());
            h.extend((body.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend([0, 0]);
            z.extend(&h);
            z.extend(name.as_bytes());
            z.extend(*body);
            let mut c = vec![0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            c.extend((body.len() as u32).to_le_bytes());
            c.extend((body.len() as u32).to_le_bytes());
            c.extend((name.len() as u16).to_le_bytes());
            c.extend([0u8; 12]);
            c.extend(off.to_le_bytes());
            c.extend(name.as_bytes());
            central.extend(c);
        }
        let cd = z.len() as u32;
        z.extend(&central);
        z.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
        z.extend((files.len() as u16).to_le_bytes());
        z.extend((files.len() as u16).to_le_bytes());
        z.extend((central.len() as u32).to_le_bytes());
        z.extend(cd.to_le_bytes());
        z.extend([0, 0]);
        z
    }

    #[test]
    fn windows_swap_replaces_exe_and_soundfont_and_keeps_old_for_cleanup() {
        let dir = std::env::temp_dir().join(format!("fermata-f34-win-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fermata.exe");
        std::fs::write(&exe, b"old exe").unwrap();
        std::fs::write(dir.join("GeneralUser-GS.sf2"), b"old sf2").unwrap();
        std::fs::write(dir.join("CREDITS.md"), b"credits").unwrap();
        let zip = dir.join("Fermata-Windows-x64.zip");
        std::fs::write(&zip, zip_of(&[("fermata.exe", b"new exe"), ("GeneralUser-GS.sf2", b"new sf2"), ("CREDITS.md", b"new credits")])).unwrap();
        let t = Target::Windows { dir: dir.clone(), exe: exe.clone() };
        let staged = stage(&zip, &t).unwrap();
        // staging touches nothing in use
        assert_eq!(std::fs::read(&exe).unwrap(), b"old exe");
        install(&staged, false).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new exe");
        assert_eq!(std::fs::read(dir.join("GeneralUser-GS.sf2")).unwrap(), b"new sf2");
        assert_eq!(std::fs::read(dir.join("fermata.exe.old")).unwrap(), b"old exe", "the running exe goes to .old");
        assert_eq!(std::fs::read(dir.join("CREDITS.md")).unwrap(), b"credits", "only the program and the SoundFont");
        // a zip without the program is refused before anything changes
        std::fs::write(&zip, zip_of(&[("readme.txt", b"x")])).unwrap();
        assert!(matches!(stage(&zip, &t), Err(UpdateError::Security(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real download over HTTPS (the 0.3.11 Windows zip, 37 MB) with a signature that doesn't match: it is
    /// refused and deleted. `cargo test --release -- --ignored real_download` (needs the internet).
    #[test]
    #[ignore]
    fn real_download_with_a_wrong_signature_is_refused() {
        let rel = Release {
            version: "9.9.9".into(),
            page: String::new(),
            notes: vec![],
            asset: Some(Asset { name: asset_name().into(), url: "https://github.com/murillowelsi/fermata/releases/download/v0.3.11/Fermata-Windows-x64.zip".into(), size: 37_568_176 }),
            sig: Some(Asset { name: format!("{}.sig", asset_name()), url: "https://murillowelsi.github.io/fermata/app/sw.js".into(), size: 0 }),
        };
        let dir = std::env::temp_dir().join(format!("fermata-f34-dl-{}", std::process::id()));
        let progress = Mutex::new((0, 37_568_176));
        let t = Instant::now();
        let r = download(&rel, &dir, &progress, &AtomicBool::new(false));
        println!("{:?} after {:.1} s, {} bytes", r, t.elapsed().as_secs_f64(), progress.lock().unwrap().0);
        assert!(matches!(r, Err(UpdateError::Security(_))));
        assert_eq!(progress.lock().unwrap().0, 37_568_176, "it all arrived");
        assert!(!dir.join(asset_name()).exists() && !dir.join(format!("{}.part", asset_name())).exists(), "nothing is kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

}
