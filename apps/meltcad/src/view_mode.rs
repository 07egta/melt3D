//! Analysis view modes: a final colour for every surface pixel that shows the shape of the surface
//! instead of its material (zebra stripes, contour rings, up / down for print overhangs).
//!
//! A mode is a viewer setting, like the camera: it is not saved in the design, does not make the
//! document dirty and has no undo step. `shader.wgsl` (`view_mode_color`) has the same formulas;
//! this file is the CPU reference (the simple view uses it) and draws the round swatches.

use crate::sdf::{V3, dot};

/// The five ways to look at the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Normal,
    ZebraH,
    ZebraV,
    Rings,
    UpDown,
}

impl ViewMode {
    pub const ALL: [ViewMode; 5] = [ViewMode::Normal, ViewMode::ZebraH, ViewMode::ZebraV, ViewMode::Rings, ViewMode::UpDown];

    /// The number the shader reads (`G.hover.z`).
    pub fn id(self) -> u32 {
        match self {
            ViewMode::Normal => 0,
            ViewMode::ZebraH => 1,
            ViewMode::ZebraV => 2,
            ViewMode::Rings => 3,
            ViewMode::UpDown => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Normal => "Normal",
            ViewMode::ZebraH => "Zebra (horizontal stripes)",
            ViewMode::ZebraV => "Zebra (vertical stripes)",
            ViewMode::Rings => "Rings (contour bands)",
            ViewMode::UpDown => "Up / Down (green up, red down, blue = vertical wall)",
        }
    }
}

/// Stripes per unit of the reflected direction, and bands per unit of normal . view.
const ZEBRA_K: f32 = 5.0;
const RINGS_K: f32 = 9.0;

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// 1 inside a band (the first half of each period), 0 between, with soft edges.
fn band(x: f32) -> f32 {
    let f = x - x.floor();
    smooth(0.0, 0.06, f) - smooth(0.5, 0.56, f)
}

/// The colour (display values 0..1) a surface point with unit normal `n`, seen along the unit ray
/// `rd` by a camera whose screen axes are `right` and `up`, gets in `mode`; `None` for Normal
/// (keep the usual shading).
pub fn color(mode: u32, n: V3, rd: V3, right: V3, up: V3) -> Option<V3> {
    if mode == 0 || mode > 4 {
        return None;
    }
    let d = dot(rd, n);
    let r = [rd[0] - 2.0 * d * n[0], rd[1] - 2.0 * d * n[1], rd[2] - 2.0 * d * n[2]];
    let ndv = (-d).clamp(0.0, 1.0);
    let c = match mode {
        1 | 2 => {
            let x = if mode == 2 { dot(r, right) } else { dot(r, up) };
            mix([0.06, 0.06, 0.08], [0.96, 0.96, 0.94], band(x * ZEBRA_K))
        }
        3 => mix([0.10, 0.16, 0.34], [0.88, 0.94, 1.0], band(ndv * RINGS_K)),
        _ => {
            let base = if n[2] > 0.0 { [0.22, 0.72, 0.34] } else { [0.85, 0.22, 0.20] };
            let shade = 0.7 + 0.3 * ndv;
            let lit = [base[0] * shade, base[1] * shade, base[2] * shade];
            mix(lit, [0.10, 0.30, 1.0], 1.0 - smooth(0.02, 0.05, n[2].abs()))
        }
    };
    Some(c)
}

/// A round swatch of `mode`, `size` points wide, as a clickable button; true when clicked.
pub fn swatch(ui: &mut egui::Ui, mode: ViewMode, size: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let resp = resp.on_hover_text(mode.label());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    let (c, rad) = (rect.center(), size * 0.5 - 2.0);
    let cells = (rad * 2.0).round().clamp(8.0, 48.0) as i32;
    let cell = rad * 2.0 / cells as f32;
    // a camera looking along +y... seen from the side: screen right = x, screen up = z
    let (rd, right, up) = ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    for iy in 0..cells {
        for ix in 0..cells {
            let (u, v) = ((ix as f32 + 0.5) / cells as f32 * 2.0 - 1.0, 1.0 - (iy as f32 + 0.5) / cells as f32 * 2.0);
            let r2 = u * u + v * v;
            if r2 > 1.0 {
                continue;
            }
            let n = [u, (1.0 - r2).sqrt(), v];
            let col = match color(mode.id(), n, rd, right, up) {
                Some(col) => col,
                None => {
                    let l = (0.25 + 0.75 * (n[0] * -0.4 + n[1] * 0.5 + n[2] * 0.75).max(0.0)).clamp(0.0, 1.0);
                    [0.55 * l + 0.1, 0.62 * l + 0.1, 0.78 * l + 0.1]
                }
            };
            let to = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            let min = egui::pos2(c.x - rad + ix as f32 * cell, c.y - rad + iy as f32 * cell);
            p.rect_filled(egui::Rect::from_min_size(min, egui::vec2(cell + 0.4, cell + 0.4)), 0.0, egui::Color32::from_rgb(to(col[0]), to(col[1]), to(col[2])));
        }
    }
    if mode == ViewMode::UpDown {
        // (a sampled line this thin would be missed by the cells)
        p.line_segment([egui::pos2(c.x - rad, c.y), egui::pos2(c.x + rad, c.y)], egui::Stroke::new(1.6, egui::Color32::from_rgb(26, 77, 255)));
    }
    let ring = if selected { egui::Stroke::new(2.0, ui.visuals().selection.stroke.color) } else { egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color) };
    p.circle_stroke(c, rad + 1.0, ring);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_gives_finite_colours_in_range() {
        let (right, up) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        for mode in 0..=5u32 {
            for i in 0..200 {
                let a = i as f32 * 0.37;
                let n = crate::sdf::norm([a.sin(), a.cos() * 0.7, (a * 1.3).sin()]);
                let rd = crate::sdf::norm([0.1, -1.0, -0.3 + (a * 0.5).cos() * 0.2]);
                if let Some(c) = color(mode, n, rd, right, up) {
                    assert!(c.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x)), "mode {mode}: {c:?}");
                } else {
                    assert!(mode == 0 || mode > 4);
                }
            }
        }
    }

    #[test]
    fn up_is_green_down_is_red_and_a_wall_is_blue() {
        let rd = [0.0, -1.0, 0.0];
        let (r, u) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let up_c = color(4, [0.0, 0.0, 1.0], rd, r, u).expect("up");
        let down_c = color(4, [0.0, 0.0, -1.0], rd, r, u).expect("down");
        let wall = color(4, [0.0, 1.0, 0.0], rd, r, u).expect("wall");
        assert!(up_c[1] > up_c[0] && up_c[1] > up_c[2]);
        assert!(down_c[0] > down_c[1] && down_c[0] > down_c[2]);
        assert!(wall[2] > wall[0] && wall[2] > wall[1]);
    }
}
