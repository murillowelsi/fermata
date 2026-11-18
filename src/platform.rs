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

/// The browser: files come in through the page's file picker (asynchronously, picked up by `take_opened` on a
/// later frame) and go out as downloads. There is no file system to write back to.
#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use wasm_bindgen::JsCast;

    thread_local! {
        static OPENED: RefCell<Vec<Opened>> = const { RefCell::new(Vec::new()) };
    }

    fn pick(dialog: rfd::AsyncFileDialog) {
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(h) = dialog.pick_file().await {
                let bytes = h.read().await;
                OPENED.with(|o| o.borrow_mut().push(Opened { name: h.file_name(), bytes, path: None }));
            }
        });
    }

    pub fn pick_open(desc: &str, exts: &[&str]) -> Option<Opened> {
        pick(rfd::AsyncFileDialog::new().add_filter(desc, exts));
        None
    }

    pub fn pick_any(title: &str) -> Option<Opened> {
        pick(rfd::AsyncFileDialog::new().set_title(title));
        None
    }

    pub fn take_opened() -> Vec<Opened> {
        OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()))
    }

    /// Hands the bytes to the browser as a download named `name`.
    fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
        let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
        let arr = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&arr);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(err)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(err)?.dyn_into().map_err(|_| "no anchor")?;
        a.set_href(&url);
        a.set_download(name);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Ok(())
    }

    pub fn save_as(default_name: &str, ext: &str, _desc: &str, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
        let name = format!("{default_name}.{ext}");
        download(&name, bytes)?;
        Ok(Some(PathBuf::from(name)))
    }

    pub fn pick_save_path(default_name: &str, ext: &str, _desc: &str) -> Option<PathBuf> {
        Some(PathBuf::from(format!("{default_name}.{ext}")))
    }

    pub fn save_to(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
        download(&path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "score.fmt".into()), bytes)
    }

    pub fn read_path(_path: &PathBuf) -> Option<Opened> {
        None
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
