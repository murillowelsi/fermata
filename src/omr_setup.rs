//! F-31: the photo reader (Audiveris, with its own Java) downloaded on first use into Fermata's support folder
//! (Mac: ~/Library/Application Support/Fermata/omr; Windows: %APPDATA%\Fermata\omr), never installed in the system:
//! nothing in Applications, Program Files or the PATH, no installer window. Fermata only runs it as a separate
//! program (AGPL-3.0); its licence and where to get its source are kept beside the copy (NOTICE.txt).
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub const AUDIVERIS_VERSION: &str = "5.11.0";
const AGPL: &str = include_str!("../licenses/Audiveris-AGPL-3.0.txt");

/// One official release file: where, its size and SHA-256 (from the release page, github.com/Audiveris/audiveris).
#[derive(Clone, Debug)]
pub struct Dist {
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// The release file for this computer, or None where Fermata can't set it up (Linux, Windows on ARM).
pub fn dist() -> Option<Dist> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(Dist {
            url: "https://github.com/Audiveris/audiveris/releases/download/5.11.0/Audiveris-5.11.0-macosx-arm64.dmg",
            size: 84_929_243,
            sha256: "17491af8b6d40153b031dd1f0815e37213f0999ef23f10586af46706e59b2eb6",
        })
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(Dist {
            url: "https://github.com/Audiveris/audiveris/releases/download/5.11.0/Audiveris-5.11.0-macosx-x86_64.dmg",
            size: 87_065_977,
            sha256: "11794424c6f1698617836a77d2c5818c46405a3fe7388281c623c4e383fa33cb",
        })
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        // the console build: it runs in batch mode without opening a window
        Some(Dist {
            url: "https://github.com/Audiveris/audiveris/releases/download/5.11.0/Audiveris-5.11.0-windowsConsole-x86_64.msi",
            size: 85_062_430,
            sha256: "5f1b4e96a12c53c7da426814b76e599363c4181e291855996e0a6878dda95f71",
        })
    } else {
        None
    }
}

/// Fermata's own folder for the reader (`FERMATA_OMR_DIR` overrides it, for tests).
pub fn home() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FERMATA_OMR_DIR") {
        return Some(PathBuf::from(d));
    }
    crate::prefs::dir().map(|d| d.join("omr"))
}

/// The downloaded program, if it is there.
pub fn installed() -> Option<PathBuf> {
    let h = home()?;
    if cfg!(target_os = "macos") {
        let p = h.join("Audiveris.app/Contents/MacOS/Audiveris");
        return p.is_file().then_some(p);
    }
    find_named(&h.join("app"), &["Audiveris.exe", "audiveris.exe"])
}

fn find_named(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()).map(|n| names.contains(&n)).unwrap_or(false) {
                return Some(p);
            }
        }
    }
    None
}

/// Space the downloaded reader takes (bytes), for a "remove" item.
pub fn installed_size() -> u64 {
    fn walk(p: &Path) -> u64 {
        match std::fs::symlink_metadata(p) {
            Ok(m) if m.is_dir() => std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| walk(&e.path())).sum()).unwrap_or(0),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    home().map(|h| walk(&h)).unwrap_or(0)
}

/// Remove the downloaded reader (it comes back on the next photo import).
pub fn remove() -> Result<(), String> {
    match home() {
        Some(h) if h.exists() => std::fs::remove_dir_all(&h).map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), to check the download without another library
// ---------------------------------------------------------------------------------------------------------------

pub struct Sha256 {
    h: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 { h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19], buf: Vec::with_capacity(64), len: 0 }
    }
}

impl Sha256 {
    fn block(&mut self, b: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b2, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b2) ^ (a & c) ^ (b2 & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b2;
            b2 = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in self.h.iter_mut().zip([a, b2, c, d, e, f, g, h]) {
            *x = x.wrapping_add(y);
        }
    }
    pub fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        if !self.buf.is_empty() {
            let take = (64 - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == 64 {
                let b = std::mem::take(&mut self.buf);
                self.block(&b);
            }
        }
        while data.len() >= 64 {
            self.block(&data[..64]);
            data = &data[64..];
        }
        self.buf.extend_from_slice(data);
    }
    pub fn hex(mut self) -> String {
        let bits = self.len * 8;
        let mut tail = std::mem::take(&mut self.buf);
        tail.push(0x80);
        while tail.len() % 64 != 56 {
            tail.push(0);
        }
        tail.extend(bits.to_be_bytes());
        for b in tail.chunks(64) {
            self.block(b);
        }
        self.h.iter().map(|x| format!("{x:08x}")).collect()
    }
}

pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut s = Sha256::default();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(&buf[..n]);
    }
    Ok(s.hex())
}

// ---------------------------------------------------------------------------------------------------------------
// Setting it up in the background
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Downloading,
    Checking,
    Unpacking,
}

/// A setup in the background: `progress` (bytes downloaded of `total`), `stage`, cancel, and the program at the end.
pub struct SetupJob {
    pub total: u64,
    done: Arc<AtomicU64>,
    stage: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<PathBuf, String>>,
}

/// Closing Fermata stops the download (and deletes the partial file).
impl Drop for SetupJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        let _ = self.rx.recv_timeout(Duration::from_secs(3));
    }
}

impl SetupJob {
    pub fn progress(&self) -> (Stage, u64, u64) {
        let st = match self.stage.load(Ordering::SeqCst) {
            0 => Stage::Downloading,
            1 => Stage::Checking,
            _ => Stage::Unpacking,
        };
        (st, self.done.load(Ordering::SeqCst), self.total)
    }
    /// A line for the status bar.
    pub fn describe(&self) -> String {
        let (st, d, t) = self.progress();
        let mb = |x: u64| x as f64 / 1_000_000.0;
        match st {
            Stage::Downloading => format!("Downloading the photo reader (first use only): {:.0} of {:.0} MB...", mb(d), mb(t)),
            Stage::Checking => "Checking the photo reader...".into(),
            Stage::Unpacking => "Setting up the photo reader...".into(),
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn try_result(&self) -> Option<Result<PathBuf, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the download stopped unexpectedly".into())),
        }
    }
    pub fn wait(&self) -> Result<PathBuf, String> {
        self.rx.recv().unwrap_or_else(|_| Err("the download stopped unexpectedly".into()))
    }
}

/// Download, check and unpack the reader on a background thread. `from` replaces the download with a local copy of
/// the release file (tests; still checked against the SHA-256).
pub fn start(from: Option<PathBuf>) -> Result<SetupJob, String> {
    let d = dist().ok_or("Reading music from photos isn't available on this computer yet.")?;
    let home = home().ok_or("No folder for Fermata's files.")?;
    let (tx, rx) = mpsc::channel();
    let done = Arc::new(AtomicU64::new(0));
    let stage = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (d2, s2, c2, total) = (done.clone(), stage.clone(), cancel.clone(), d.size);
    std::thread::spawn(move || {
        let r = setup_now(&d, &home, from.as_deref(), &d2, &s2, &c2);
        let _ = tx.send(r);
    });
    Ok(SetupJob { total, done, stage, cancel, rx })
}

fn setup_now(d: &Dist, home: &Path, from: Option<&Path>, done: &AtomicU64, stage: &AtomicU64, cancel: &AtomicBool) -> Result<PathBuf, String> {
    std::fs::create_dir_all(home).map_err(|e| format!("Couldn't create {}: {e}", home.display()))?;
    let file = home.join(d.url.rsplit('/').next().unwrap_or("audiveris.bin"));
    let part = file.with_extension("part");
    // 1. download (or copy), with progress and cancel
    match from {
        Some(src) => {
            std::fs::copy(src, &part).map_err(|e| e.to_string())?;
            done.store(d.size, Ordering::SeqCst);
        }
        None => {
            let _ = std::fs::remove_file(&part);
            let mut child = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" })
                .args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "-o"])
                .arg(&part)
                .arg(d.url)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("Couldn't start the download: {e}"))?;
            loop {
                if let Ok(m) = std::fs::metadata(&part) {
                    done.store(m.len(), Ordering::SeqCst);
                }
                match child.try_wait() {
                    Ok(Some(st)) if st.success() => break,
                    Ok(Some(_)) => {
                        let mut err = String::new();
                        if let Some(mut e) = child.stderr.take() {
                            let _ = e.read_to_string(&mut err);
                        }
                        let _ = std::fs::remove_file(&part);
                        return Err(format!("The download failed. Check the internet connection and try again.\n{}", err.trim()));
                    }
                    Ok(None) => {}
                    Err(e) => return Err(e.to_string()),
                }
                if cancel.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&part);
                    return Err("Download cancelled.".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    // 2. check
    stage.store(1, Ordering::SeqCst);
    let got = sha256_file(&part).map_err(|e| e.to_string())?;
    if got != d.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("The downloaded photo reader isn't the expected file (checksum {}…), so it wasn't used. Try again later.", &got[..12.min(got.len())]));
    }
    std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&file);
        return Err("Download cancelled.".into());
    }
    // 3. unpack, without an installer
    stage.store(2, Ordering::SeqCst);
    let r = unpack(&file, home);
    let _ = std::fs::remove_file(&file);
    r?;
    let notice = format!(
        "Audiveris {AUDIVERIS_VERSION} — the optical music recognition program Fermata runs, as a separate program, to read scores from photos.\n\
         Licence: GNU Affero General Public License v3.0 (AGPL-3.0), in LICENSE-AGPL-3.0.txt beside this file.\n\
         Source code: https://github.com/Audiveris/audiveris/tree/{AUDIVERIS_VERSION}\n\
         Downloaded unchanged from: {}\nSHA-256: {}\n\
         Remove this folder to remove it; Fermata downloads it again the next time a photo is imported.\n",
        d.url, d.sha256
    );
    let _ = std::fs::write(home.join("NOTICE.txt"), notice);
    let _ = std::fs::write(home.join("LICENSE-AGPL-3.0.txt"), AGPL);
    let _ = std::fs::write(home.join("VERSION"), AUDIVERIS_VERSION);
    installed().ok_or_else(|| "The photo reader was unpacked but its program wasn't found.".to_string())
}

#[cfg(target_os = "macos")]
fn unpack(dmg: &Path, home: &Path) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mnt = home.join("mnt");
    let _ = std::fs::create_dir_all(&mnt);
    // mounted out of sight (no Finder window, nothing in the sidebar); the image's licence agreement (AGPL) is
    // answered on its standard input, as the program is only used, not redistributed
    let mut child = Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-noverify", "-mountpoint"])
        .arg(&mnt)
        .arg(dmg)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(b"Y\n");
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Couldn't open the photo reader's disk image: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let app = home.join("Audiveris.app");
    let tmp = home.join("Audiveris.app.part");
    let _ = std::fs::remove_dir_all(&tmp);
    let copied = Command::new("/usr/bin/ditto").arg(mnt.join("Audiveris.app")).arg(&tmp).status().map(|s| s.success()).unwrap_or(false);
    let _ = Command::new("/usr/bin/hdiutil").args(["detach", "-quiet", "-force"]).arg(&mnt).status();
    let _ = std::fs::remove_dir(&mnt);
    if !copied {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("Couldn't copy the photo reader out of its disk image.".into());
    }
    let _ = std::fs::remove_dir_all(&app);
    std::fs::rename(&tmp, &app).map_err(|e| e.to_string())?;
    // Gatekeeper: a copy written by Fermata itself carries no quarantine flag, but if one is there (a download made
    // by a quarantine-aware process), it is removed so the reader and its Java start without asking
    let _ = Command::new("/usr/bin/xattr").args(["-dr", "com.apple.quarantine"]).arg(&app).stdout(Stdio::null()).stderr(Stdio::null()).status();
    Ok(())
}

#[cfg(target_os = "windows")]
fn unpack(msi: &Path, home: &Path) -> Result<(), String> {
    // an "administrative install" only unpacks the files into a folder: no registry, no Start menu, no window
    let target = home.join("app");
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let st = std::process::Command::new("msiexec")
        .arg("/a")
        .arg(msi)
        .arg("/qn")
        .arg(format!("TARGETDIR={}", target.display()))
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("Couldn't unpack the photo reader (msiexec {st})."));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn unpack(_f: &Path, _home: &Path) -> Result<(), String> {
    Err("Reading music from photos isn't available on this computer yet.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vectors() {
        let h = |b: &[u8]| {
            let mut s = Sha256::default();
            s.update(b);
            s.hex()
        };
        assert_eq!(h(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(h(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(h(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        // in pieces of odd sizes
        let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7) as u8).collect();
        let mut s = Sha256::default();
        for c in data.chunks(37) {
            s.update(c);
        }
        assert_eq!(s.hex(), h(&data));
    }

    #[test]
    fn this_computer_has_a_release_with_a_checksum() {
        if let Some(d) = dist() {
            assert!(d.url.contains(AUDIVERIS_VERSION) && d.sha256.len() == 64 && d.size > 50_000_000);
        }
    }

    /// The whole setup from a local copy of the release (FERMATA_AUDIVERIS_DMG), into a test folder; skipped when
    /// that copy isn't given (no 85 MB download in the tests).
    #[test]
    fn setup_from_a_local_copy_and_remove() {
        let Some(src) = std::env::var_os("FERMATA_AUDIVERIS_DMG").map(PathBuf::from) else { return };
        let dir = std::env::temp_dir().join(format!("fermata-omr-setup-{}", std::process::id()));
        std::env::set_var("FERMATA_OMR_DIR", &dir);
        let job = start(Some(src)).unwrap();
        let prog = job.wait().unwrap();
        assert!(prog.is_file());
        assert!(std::fs::read_to_string(dir.join("NOTICE.txt")).unwrap().contains("AGPL-3.0"));
        assert!(installed_size() > 100_000_000);
        remove().unwrap();
        assert!(installed().is_none());
        std::env::remove_var("FERMATA_OMR_DIR");
    }
}
