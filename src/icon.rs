//! The app icon "Traço" (F-14): a white fermata drawn as a single-weight arch over a dot, on the blue #007AFF.
//! The pictures are the approved PNGs in `assets/AppIcon.iconset/`, made from `assets/icone-traco-light.svg`;
//! if the icon changes, regenerate them from that source, not here.
// not used by the browser build
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

/// 256 px, for the window and the Dock while the app runs.
const ICON_256: &[u8] = include_bytes!("../assets/AppIcon.iconset/icon_256x256.png");
/// 1024 px, the source for other sizes (`--icon-png PATH SIZE`).
const ICON_1024: &[u8] = include_bytes!("../assets/AppIcon.iconset/icon_512x512@2x.png");

fn decode(png: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(png, image::ImageFormat::Png).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1))
}

/// The icon as RGBA pixels at `size` × `size` (256 and 1024 exactly as drawn, other sizes scaled from 1024).
pub fn rgba(size: u32) -> Vec<u8> {
    let img = match size {
        256 => decode(ICON_256),
        1024 => decode(ICON_1024),
        s => image::imageops::resize(&decode(ICON_1024), s.max(1), s.max(1), image::imageops::FilterType::Lanczos3),
    };
    img.into_raw()
}

/// The window and Dock icon while the app runs.
pub fn icon_data() -> egui::IconData {
    let img = decode(ICON_256);
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_is_the_approved_256_px_png() {
        let d = icon_data();
        assert_eq!((d.width, d.height), (256, 256));
        assert_eq!(d.rgba.len(), 256 * 256 * 4);
        // transparent corner, blue (#007AFF) background inside the rounded square
        let px = |x: u32, y: u32| d.rgba[((y * 256 + x) * 4) as usize..((y * 256 + x) * 4 + 4) as usize].to_vec();
        assert_eq!(px(0, 0)[3], 0, "corner should be transparent");
        let mid = px(40, 128);
        assert!(mid[2] > 200 && mid[0] < 40, "expected the blue background, got {mid:?}");
    }

    #[test]
    fn other_sizes_scale_from_1024() {
        assert_eq!(rgba(64).len(), 64 * 64 * 4);
        assert_eq!(rgba(1024).len(), 1024 * 1024 * 4);
    }
}
