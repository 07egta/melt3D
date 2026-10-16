//! Procedurally generated window icon for MELTCAD.
//!
//! Drawn entirely in code (no image assets): a dark rounded square holding two
//! blobs, a coral one and a violet one, melted together by a smooth-min join
//! on their signed distance fields. Edges are anti-aliased from the distance
//! field, and a dome-shaped bevel gives soft light from the top-left.

use egui::IconData;

/// Edge length of the icon in pixels.
const SIZE: u32 = 128;
/// Canvas centre in pixel coordinates.
const CENTRE: f32 = 64.0;

/// Background colour of the rounded square.
const BG: [f32; 3] = [38.0, 38.0, 42.0];
const CORAL: [f32; 3] = [240.0, 140.0, 115.0];
const VIOLET: [f32; 3] = [146.0, 126.0, 255.0];

/// Half-size and corner radius of the rounded square.
const SQUARE_HALF: f32 = 60.0;
const SQUARE_RADIUS: f32 = 26.0;

/// Blob centres and radii, in pixel coordinates.
const CORAL_CENTRE: (f32, f32) = (50.0, 52.0);
const CORAL_R: f32 = 28.0;
const VIOLET_CENTRE: (f32, f32) = (80.0, 78.0);
const VIOLET_R: f32 = 26.0;

/// Width of the smooth-min neck where the two blobs meet.
const MELT: f32 = 16.0;
/// Half-width of the colour transition between coral and violet.
const COLOUR_BLEND: f32 = 20.0;
/// Width of the anti-aliased edge ramp, in pixels.
const AA_WIDTH: f32 = 1.2;

/// Depth over which the dome bevel rises from the outline.
const DOME_DEPTH: f32 = 20.0;
/// Strength of the bevel slope in the normal.
const BUMP: f32 = 4.0;
/// Light direction (towards the light, from the top-left and above).
const LIGHT: [f32; 3] = [-0.5, -0.55, 0.67];

/// Returns the 128x128 RGBA window icon (unmultiplied alpha).
pub fn data() -> IconData {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let pixel = render_pixel(px as f32 + 0.5, py as f32 + 0.5);
            rgba.extend_from_slice(&pixel);
        }
    }
    IconData { rgba, width: SIZE, height: SIZE }
}

/// Renders one pixel at the given sample position as unmultiplied RGBA.
fn render_pixel(x: f32, y: f32) -> [u8; 4] {
    let square_cover = coverage(square_sdf(x, y));
    let blob_cover = coverage(blob_sdf(x, y));
    let light = shade(x, y);

    let body = scale3(blob_colour(x, y), light);
    let rgb = mix3(BG, body, blob_cover);

    [to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]), to_u8(square_cover * 255.0)]
}

/// Signed distance to the rounded square (negative inside).
fn square_sdf(x: f32, y: f32) -> f32 {
    let inner = SQUARE_HALF - SQUARE_RADIUS;
    let qx = (x - CENTRE).abs() - inner;
    let qy = (y - CENTRE).abs() - inner;
    length2(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - SQUARE_RADIUS
}

/// Signed distance to a circle (negative inside).
fn circle_sdf(x: f32, y: f32, centre: (f32, f32), radius: f32) -> f32 {
    length2(x - centre.0, y - centre.1) - radius
}

/// Signed distance to the two blobs joined by a smooth minimum.
fn blob_sdf(x: f32, y: f32) -> f32 {
    let coral = circle_sdf(x, y, CORAL_CENTRE, CORAL_R);
    let violet = circle_sdf(x, y, VIOLET_CENTRE, VIOLET_R);
    smooth_min(coral, violet, MELT)
}

/// Polynomial smooth minimum of two distance fields, blending over width `k`.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// Colour of the blob at a point, blending from coral to violet across the neck.
fn blob_colour(x: f32, y: f32) -> [f32; 3] {
    let d_coral = circle_sdf(x, y, CORAL_CENTRE, CORAL_R);
    let d_violet = circle_sdf(x, y, VIOLET_CENTRE, VIOLET_R);
    let w = (0.5 + (d_coral - d_violet) / (2.0 * COLOUR_BLEND)).clamp(0.0, 1.0);
    mix3(CORAL, VIOLET, w)
}

/// Dome-shaped height of the blob: 0 at the outline, 1 well inside.
fn dome_height(x: f32, y: f32) -> f32 {
    let t = (-blob_sdf(x, y) / DOME_DEPTH).clamp(0.0, 1.0);
    (1.0 - (1.0 - t) * (1.0 - t)).sqrt()
}

/// Soft lighting factor from the bevel normal, lighter top-left, darker bottom-right.
fn shade(x: f32, y: f32) -> f32 {
    let gx = (dome_height(x + 1.0, y) - dome_height(x - 1.0, y)) * 0.5;
    let gy = (dome_height(x, y + 1.0) - dome_height(x, y - 1.0)) * 0.5;
    let normal = normalize3([-gx * BUMP, -gy * BUMP, 1.0]);
    let light = normalize3(LIGHT);
    let s = normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2];
    0.84 + 0.30 * s
}

/// Anti-aliased coverage in 0..=1 from a signed distance, with a ~1.2px ramp.
fn coverage(distance: f32) -> f32 {
    (0.5 - distance / AA_WIDTH).clamp(0.0, 1.0)
}

fn length2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale3(c: [f32; 3], s: f32) -> [f32; 3] {
    [c[0] * s, c[1] * s, c[2] * s]
}

fn to_u8(v: f32) -> u8 {
    // `as` saturates for out-of-range floats, so this cannot wrap or panic.
    (v.clamp(0.0, 255.0) + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(rgba: &[u8], x: usize, y: usize) -> Option<u8> {
        rgba.get((y * SIZE as usize + x) * 4 + 3).copied()
    }

    #[test]
    fn icon_is_128_square_rgba_with_transparent_corner() {
        let icon = data();
        assert_eq!(icon.width, 128);
        assert_eq!(icon.height, 128);
        assert_eq!(icon.rgba.len(), 128 * 128 * 4);
        assert_eq!(alpha_at(&icon.rgba, 64, 64), Some(255));
        assert_eq!(alpha_at(&icon.rgba, 0, 0), Some(0));
    }
}
