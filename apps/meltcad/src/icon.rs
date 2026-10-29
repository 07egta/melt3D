//! The MELTCAD mark: the window icon (a black rounded square with the white looped M) and the white
//! mark shown in the title bar. Both are rasterised from `assets/logo/*.svg` (the owner's artwork,
//! redrawn with smooth curves) and stored as raw pixels, so no image or SVG library is needed.

use egui::IconData;

/// Edge length of the window icon in pixels.
const SIZE: u32 = 128;
const ICON_RGBA: &[u8] = include_bytes!("../assets/logo/icon-128.rgba");

/// Size of the title-bar mark's alpha map.
pub const MARK_W: usize = 192;
pub const MARK_H: usize = 131;
const MARK_A8: &[u8] = include_bytes!("../assets/logo/mark-192x131.a8");

/// The 128x128 RGBA window icon (unmultiplied alpha); transparent if the data is malformed.
pub fn data() -> IconData {
    let want = (SIZE * SIZE * 4) as usize;
    let rgba = if ICON_RGBA.len() == want { ICON_RGBA.to_vec() } else { vec![0; want] };
    IconData { rgba, width: SIZE, height: SIZE }
}

/// The white mark with its alpha, for the title bar.
pub fn mark_image() -> egui::ColorImage {
    let px: Vec<egui::Color32> = if MARK_A8.len() == MARK_W * MARK_H {
        MARK_A8.iter().map(|&a| egui::Color32::from_white_alpha(a)).collect()
    } else {
        vec![egui::Color32::TRANSPARENT; MARK_W * MARK_H]
    };
    egui::ColorImage { size: [MARK_W, MARK_H], source_size: egui::vec2(MARK_W as f32, MARK_H as f32), pixels: px }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_is_128_square_rgba_with_transparent_corner() {
        let icon = data();
        assert_eq!((icon.width, icon.height), (128, 128));
        assert_eq!(icon.rgba.len(), 128 * 128 * 4);
        let alpha = |x: usize, y: usize| icon.rgba.get((y * 128 + x) * 4 + 3).copied();
        assert_eq!(alpha(2, 64), Some(255));
        assert_eq!(alpha(0, 0), Some(0));
    }

    #[test]
    fn mark_has_ink_and_is_the_size_we_say() {
        let img = mark_image();
        assert_eq!(img.pixels.len(), MARK_W * MARK_H);
        assert!(img.pixels.iter().filter(|p| p.a() > 200).count() > 1000);
        assert_eq!(img.pixels.first().map(|p| p.a()), Some(0));
    }
}
