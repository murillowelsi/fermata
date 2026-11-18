//! Files: the system's open and save windows.
use std::path::PathBuf;

pub struct Opened {
    pub name: String,
    pub bytes: Vec<u8>,
    pub path: Option<PathBuf>,
}

pub const OPEN_EXTS: [&str; 7] = ["fmt", "json", "enc", "ENC", "mid", "midi", "musicxml"];

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;

    pub fn pick_open(desc: &str, exts: &[&str]) -> Option<Opened> {
        let p = rfd::FileDialog::new().add_filter(desc, exts).pick_file()?;
        let bytes = std::fs::read(&p).ok()?;
        Some(Opened { name: p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), bytes, path: Some(p) })
    }

    /// Pick any file (no type filter: the Mac's file window greys out file types it doesn't know, like .enc).
    pub fn pick_any(title: &str) -> Option<Opened> {
        let p = rfd::FileDialog::new().set_title(title).pick_file()?;
        let bytes = std::fs::read(&p).ok()?;
        Some(Opened { name: p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), bytes, path: Some(p) })
    }

    pub fn take_opened() -> Vec<Opened> {
        vec![]
    }

    /// Ask where to save, then write. Ok(None) when the person cancelled.
    pub fn save_as(default_name: &str, ext: &str, desc: &str, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
        let Some(p) = pick_save_path(default_name, ext, desc) else { return Ok(None) };
        std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
        Ok(Some(p))
    }

    pub fn pick_save_path(default_name: &str, ext: &str, desc: &str) -> Option<PathBuf> {
        // the original's .enc is a type the Mac doesn't know; a filter for it can leave the Save button disabled
        let mut d = rfd::FileDialog::new().set_file_name(format!("{default_name}.{ext}"));
        if ext != "enc" {
            d = d.add_filter(desc, &[ext]);
        } else {
            d = d.set_title(desc);
        }
        let mut p = d.save_file()?;
        if p.extension().map(|e| !e.eq_ignore_ascii_case(ext)).unwrap_or(true) {
            p.set_extension(ext);
        }
        Some(p)
    }

    pub fn save_to(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub fn read_path(path: &PathBuf) -> Option<Opened> {
        let bytes = std::fs::read(path).ok()?;
        Some(Opened { name: path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), bytes, path: Some(path.clone()) })
    }
}

/// The browser (see `web.rs`): files come in through the browser's file picker or the installed app's file
/// handlers, picked up by `take_opened` on a later frame; saving writes to the same file where the File System
/// Access API exists, else downloads. Exports always ask where (or download).
#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;

    pub fn pick_open(_desc: &str, exts: &[&str]) -> Option<Opened> {
        crate::web::pick_file(exts);
        None
    }

    pub fn pick_any(_title: &str) -> Option<Opened> {
        crate::web::pick_file(&OPEN_EXTS);
        None
    }

    pub fn take_opened() -> Vec<Opened> {
        crate::web::opened_files().into_iter().map(|(name, bytes)| Opened { path: Some(PathBuf::from(&name)), name, bytes }).collect()
    }

    /// A score ("fmt") is saved as a document (the result comes later, see `take_saved`); anything else is an export.
    pub fn save_as(default_name: &str, ext: &str, desc: &str, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
        let name = format!("{default_name}.{ext}");
        if ext == "fmt" {
            crate::web::save_score(&name, bytes, true);
            Ok(None)
        } else {
            crate::web::export(&name, bytes, ext, desc);
            Ok(Some(PathBuf::from(name)))
        }
    }

    pub fn pick_save_path(default_name: &str, ext: &str, _desc: &str) -> Option<PathBuf> {
        Some(PathBuf::from(format!("{default_name}.{ext}")))
    }

    /// Save again: the same file when the browser allows, else a download with the same name.
    pub fn save_to(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "score.fmt".into());
        crate::web::save_score(&name, bytes, false);
        Ok(())
    }

    /// An export of something that isn't the score (the page image).
    pub fn export_download(path: &std::path::Path, bytes: &[u8]) {
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "page.png".into());
        let ext = path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "png".into());
        crate::web::export(&name, bytes, &ext, "Page image");
    }

    /// Revert to Saved: the score as last opened or saved in this page.
    pub fn read_path(path: &PathBuf) -> Option<Opened> {
        let want = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        crate::web::last_saved().filter(|(n, _)| *n == want).map(|(name, bytes)| Opened { path: Some(path.clone()), name, bytes })
    }

    /// Results of saving the score: (file name, Ok or the error).
    pub fn take_saved() -> Vec<(String, Result<(), String>)> {
        crate::web::saved_files()
    }
}

pub use imp::*;

/// A PNG file from a screenshot of the window.
pub fn png_bytes(img: &egui::ColorImage) -> Vec<u8> {
    let [w, h] = img.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    if let Some(im) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
        let _ = im.write_to(&mut out, image::ImageFormat::Png);
    }
    out.into_inner()
}
