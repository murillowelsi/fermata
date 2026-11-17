//! Files: the system's open and save windows.
use std::path::PathBuf;

pub struct Opened {
    pub name: String,
    pub bytes: Vec<u8>,
    pub path: Option<PathBuf>,
}

pub const OPEN_EXTS: [&str; 7] = ["fmt", "json", "enc", "ENC", "mid", "midi", "musicxml"];

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
