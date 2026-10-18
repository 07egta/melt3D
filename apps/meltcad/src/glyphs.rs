//! Small line icons drawn directly with the egui painter.
//!
//! Every icon is designed in unit coordinates (0..1 across a square frame)
//! and mapped into the square centred inside the given rect.

use crate::model::{Kind, Op};
use crate::style::gfx;
use egui::epaint::Mesh;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

const PI: f32 = std::f32::consts::PI;
const SEG: usize = 24;

/// Square icon area centred in `rect` with a 10% margin on each side.
fn frame(rect: Rect) -> Rect {
    let s = rect.width().min(rect.height()) * 0.8;
    Rect::from_center_size(rect.center(), vec2(s, s))
}

fn pen(rect: Rect, ink: Color32) -> Stroke {
    Stroke::new((rect.width() * 0.058).clamp(1.15, 2.1), ink)
}

/// A stroked path with round joins and ends (no mitre spikes at sharp corners), open or closed.
pub(crate) fn round_path(p: &Painter, mut pts: Vec<Pos2>, s: Stroke, closed: bool) {
    if closed && let Some(&first) = pts.first() {
        pts.push(first);
    }
    if s.width > 0.0 && s.color.a() == 255 {
        for q in &pts {
            p.circle_filled(*q, s.width * 0.5, s.color);
        }
    }
    p.add(Shape::line(pts, s));
}

/// Unit coordinates to screen points.
fn at(f: Rect, u: f32, v: f32) -> Pos2 {
    pos2(f.left() + f.width() * u, f.top() + f.height() * v)
}

fn px(f: Rect, pts: &[(f32, f32)]) -> Vec<Pos2> {
    pts.iter().map(|&(u, v)| at(f, u, v)).collect()
}

/// Points along an elliptical arc; angles in radians, screen y grows downwards.
fn arc(cu: f32, cv: f32, ru: f32, rv: f32, a0: f32, a1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            (cu + ru * a.cos(), cv + rv * a.sin())
        })
        .collect()
}

/// Full ellipse as a polygon without a repeated end point.
fn ellipse(cu: f32, cv: f32, ru: f32, rv: f32) -> Vec<(f32, f32)> {
    (0..SEG)
        .map(|i| {
            let a = 2.0 * PI * i as f32 / SEG as f32;
            (cu + ru * a.cos(), cv + rv * a.sin())
        })
        .collect()
}

/// Regular hexagon with vertices at 0, 60, ... degrees; `rv` < `ru` gives a tilted top.
fn hex(cu: f32, cv: f32, ru: f32, rv: f32) -> Vec<(f32, f32)> {
    (0..6)
        .map(|k| {
            let a = k as f32 * PI / 3.0;
            (cu + ru * a.cos(), cv + rv * a.sin())
        })
        .collect()
}

/// Quadratic Bezier from `a` to `b` with control point `c`.
fn quad(a: (f32, f32), c: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    let n = SEG / 2;
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let m = 1.0 - t;
            (m * m * a.0 + 2.0 * m * t * c.0 + t * t * b.0, m * m * a.1 + 2.0 * m * t * c.1 + t * t * b.1)
        })
        .collect()
}

/// Open polyline.
fn polyline(p: &Painter, f: Rect, pts: &[(f32, f32)], s: Stroke) {
    round_path(p, px(f, pts), s, false);
}

/// Closed outline (first point is repeated at the end).
fn outline(p: &Painter, f: Rect, pts: &[(f32, f32)], s: Stroke) {
    round_path(p, px(f, pts), s, true);
}

fn polygon(p: &Painter, f: Rect, pts: &[(f32, f32)], fill: Color32, s: Stroke) {
    p.add(Shape::convex_polygon(px(f, pts), fill, Stroke::NONE));
    if s.width > 0.0 {
        round_path(p, px(f, pts), s, true);
    }
}

fn segment(p: &Painter, f: Rect, a: (f32, f32), b: (f32, f32), s: Stroke) {
    round_path(p, vec![at(f, a.0, a.1), at(f, b.0, b.1)], s, false);
}

/// Draws a solid model primitive icon as a flat line drawing in `ink` (a thin outline, like the other
/// icons). A `fill` that is not nearly transparent tints the inside a little.
pub fn kind(p: &Painter, rect: Rect, k: Kind, ink: Color32, fill: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    let dim = Stroke::new(s.width * 0.8, Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), 90));
    match k {
        Kind::Sphere => {
            polygon(p, f, &ellipse(0.5, 0.5, 0.42, 0.42), fill, s);
            polyline(p, f, &arc(0.5, 0.5, 0.42, 0.14, 0.0, PI, 14), s);
            polyline(p, f, &arc(0.5, 0.5, 0.42, 0.14, PI, 2.0 * PI, 14), dim);
        }
        Kind::Box => {
            let h = hex(0.5, 0.5, 0.44, 0.44);
            polygon(p, f, &h, fill, s);
            segment(p, f, (0.5, 0.5), h[1], s);
            segment(p, f, (0.5, 0.5), h[3], s);
            segment(p, f, (0.5, 0.5), h[5], s);
        }
        Kind::Cylinder => {
            polygon(p, f, &ellipse(0.5, 0.24, 0.34, 0.13), fill, s);
            polyline(p, f, &arc(0.5, 0.76, 0.34, 0.13, 0.0, PI, 14), s);
            segment(p, f, (0.16, 0.24), (0.16, 0.76), s);
            segment(p, f, (0.84, 0.24), (0.84, 0.76), s);
        }
        Kind::Torus => {
            polygon(p, f, &ellipse(0.5, 0.5, 0.45, 0.27), fill, s);
            outline(p, f, &ellipse(0.5, 0.5, 0.2, 0.09), s);
        }
        Kind::Cone => {
            let apex = (0.5, 0.1);
            p.add(Shape::convex_polygon(px(f, &[apex, (0.14, 0.78), (0.86, 0.78)]), fill, Stroke::NONE));
            segment(p, f, apex, (0.14, 0.78), s);
            segment(p, f, apex, (0.86, 0.78), s);
            polyline(p, f, &arc(0.5, 0.78, 0.36, 0.13, 0.0, PI, 14), s);
            polyline(p, f, &arc(0.5, 0.78, 0.36, 0.13, PI, 2.0 * PI, 14), dim);
        }
        Kind::Extrude => {
            // an L-shaped outline pushed up
            let base = [(0.12, 0.88), (0.62, 0.88), (0.62, 0.66), (0.4, 0.66), (0.4, 0.46), (0.12, 0.46)];
            let up: Vec<(f32, f32)> = base.iter().map(|&(u, v)| (u + 0.2, v - 0.3)).collect();
            outline(p, f, &base, s);
            outline(p, f, &up, s);
            for i in [0usize, 1, 2, 4, 5] {
                if let (Some(&a), Some(&b)) = (base.get(i), up.get(i)) {
                    segment(p, f, a, b, if i == 4 { dim } else { s });
                }
            }
        }
        _ => crate::glyphs_more::kind_more(p, rect, k, ink, fill),
    }
}

/// A plus, a minus or a drop of paint on a soft chip.
pub fn op(p: &Painter, rect: Rect, o: Op, ink: Color32) {
    let f = frame(rect);
    let w = (rect.width() * 0.1).clamp(1.8, 3.4);
    let s = Stroke::new(w, ink);
    p.rect_filled(f, f.width() * 0.3, ink.gamma_multiply(0.09));
    let cap = |a: (f32, f32)| p.circle_filled(at(f, a.0, a.1), w * 0.5, ink);
    match o {
        Op::Add => {
            segment(p, f, (0.27, 0.5), (0.73, 0.5), s);
            segment(p, f, (0.5, 0.27), (0.5, 0.73), s);
            for a in [(0.27, 0.5), (0.73, 0.5), (0.5, 0.27), (0.5, 0.73)] {
                cap(a);
            }
        }
        Op::Subtract => {
            segment(p, f, (0.27, 0.5), (0.73, 0.5), s);
            cap((0.27, 0.5));
            cap((0.73, 0.5));
        }
        // a stain: a drop of paint with a shine
        Op::Paint => {
            let c = at(f, 0.5, 0.5);
            let r = f.width() * 0.2;
            p.circle_filled(c, r, ink);
            p.circle_filled(c + vec2(-r * 0.35, -r * 0.4), r * 0.28, Color32::from_white_alpha(150));
        }
        // overlap: two rings with the common part marked
        Op::Intersect => {
            let r = f.width() * 0.2;
            p.circle_stroke(at(f, 0.38, 0.5), r, s);
            p.circle_stroke(at(f, 0.62, 0.5), r, s);
            p.circle_filled(at(f, 0.5, 0.5), f.width() * 0.07, ink);
        }
    }
}

/// An open eye (almond with a pupil) or a closed one (a lid curving down with three lashes).
pub fn eye(p: &Painter, rect: Rect, open: bool, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    if open {
        let mut almond = quad((0.05, 0.5), (0.5, -0.1), (0.95, 0.5));
        almond.extend(quad((0.95, 0.5), (0.5, 1.1), (0.05, 0.5)).into_iter().skip(1));
        polygon(p, f, &almond, ink.gamma_multiply(0.14), Stroke::NONE);
        outline(p, f, &almond, s);
        p.circle_filled(at(f, 0.5, 0.5), f.width() * 0.15, ink);
    } else {
        // the closed lid, and lashes pointing down and out
        let lid = quad((0.05, 0.42), (0.5, 0.98), (0.95, 0.42));
        polyline(p, f, &lid, s);
        segment(p, f, (0.2, 0.62), (0.12, 0.8), s);
        segment(p, f, (0.5, 0.7), (0.5, 0.9), s);
        segment(p, f, (0.8, 0.62), (0.88, 0.8), s);
    }
}

/// Folder outline with a tab.
pub fn group(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    polygon(p, f, &[(0.08, 0.25), (0.40, 0.25), (0.48, 0.37), (0.08, 0.37)], ink.gamma_multiply(0.22), Stroke::NONE);
    polygon(p, f, &[(0.08, 0.37), (0.92, 0.37), (0.92, 0.82), (0.08, 0.82)], ink.gamma_multiply(0.16), Stroke::NONE);
    outline(p, f, &[(0.08, 0.25), (0.40, 0.25), (0.48, 0.37), (0.92, 0.37), (0.92, 0.82), (0.08, 0.82)], s);
}

/// Bin with lid, handle and two ribs.
pub fn trash(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    segment(p, f, (0.18, 0.28), (0.82, 0.28), s);
    polyline(p, f, &[(0.40, 0.28), (0.40, 0.14), (0.60, 0.14), (0.60, 0.28)], s);
    polygon(p, f, &[(0.26, 0.28), (0.32, 0.88), (0.68, 0.88), (0.74, 0.28)], ink.gamma_multiply(0.16), Stroke::NONE);
    outline(p, f, &[(0.26, 0.28), (0.32, 0.88), (0.68, 0.88), (0.74, 0.28)], s);
    segment(p, f, (0.42, 0.40), (0.42, 0.76), s);
    segment(p, f, (0.58, 0.40), (0.58, 0.76), s);
}

/// Two overlapping squares (duplicate).
pub fn dup(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    polyline(p, f, &[(0.10, 0.65), (0.10, 0.10), (0.65, 0.10), (0.65, 0.35)], s);
    segment(p, f, (0.10, 0.65), (0.35, 0.65), s);
    polygon(p, f, &[(0.35, 0.35), (0.90, 0.35), (0.90, 0.90), (0.35, 0.90)], ink.gamma_multiply(0.16), Stroke::NONE);
    outline(p, f, &[(0.35, 0.35), (0.90, 0.35), (0.90, 0.90), (0.35, 0.90)], s);
}

/// Curved arrow over the top, pointing left (undo).
pub fn undo(p: &Painter, rect: Rect, ink: Color32) {
    curved_arrow(p, rect, ink, 1.0);
}

/// Curved arrow over the top, pointing right (redo).
pub fn redo(p: &Painter, rect: Rect, ink: Color32) {
    curved_arrow(p, rect, ink, -1.0);
}

/// `sx` = 1.0 mirrors nothing (undo); -1.0 flips horizontally (redo).
fn curved_arrow(p: &Painter, rect: Rect, ink: Color32, sx: f32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    let x = |u: f32| 0.5 + sx * (u - 0.5);
    let curve: Vec<(f32, f32)> = arc(0.5, 0.62, 0.32, 0.32, 0.0, -PI, 12).into_iter().map(|(u, v)| (x(u), v)).collect();
    polyline(p, f, &curve, s);
    let tip = (x(0.18), 0.62);
    segment(p, f, tip, (x(0.05), 0.50), s);
    segment(p, f, tip, (x(0.32), 0.50), s);
}

/// Open box with an arrow leaving it upwards.
#[allow(dead_code)]
pub fn export(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    polyline(p, f, &[(0.12, 0.44), (0.12, 0.88), (0.88, 0.88), (0.88, 0.44)], s);
    segment(p, f, (0.50, 0.66), (0.50, 0.08), s);
    segment(p, f, (0.38, 0.20), (0.50, 0.08), s);
    segment(p, f, (0.62, 0.20), (0.50, 0.08), s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_draws() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(34.0, 34.0));
            for k in Kind::ALL {
                for ink in [Color32::from_rgb(230, 230, 240), Color32::WHITE] {
                    kind(&p, r, k, ink, Color32::from_white_alpha(18));
                }
                kind(&p, r, k, Color32::WHITE, Color32::from_rgb(200, 100, 100));
                kind(&p, Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)), k, Color32::WHITE, Color32::TRANSPARENT);
            }
            for o in [Op::Add, Op::Subtract, Op::Paint] {
                op(&p, r, o, Color32::WHITE);
            }
            eye(&p, r, true, Color32::WHITE);
            eye(&p, r, false, Color32::WHITE);
            group(&p, r, Color32::WHITE);
            trash(&p, r, Color32::WHITE);
            dup(&p, r, Color32::WHITE);
            undo(&p, r, Color32::WHITE);
            redo(&p, r, Color32::WHITE);
            export(&p, r, Color32::WHITE);
        });
        out.textures_delta.clear();
    }
}

/// `YURA_GLYPHS=/path/sheet.ppm cargo test -p meltcad glyph_sheet -- --ignored`: every shape icon, large and at 34 px.
#[cfg(test)]
#[test]
#[ignore]
fn glyph_sheet() {
    use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};
    let ppp = 2.0;
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(ppp);
    let mut tex = Textures::new();
    let size = vec2(760.0, 330.0);
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(ui.max_rect(), 0.0, Color32::from_rgb(31, 31, 32));
        for (i, k) in Kind::ALL.iter().enumerate() {
            let (col, row) = (i % 10, i / 10);
            let big = Rect::from_min_size(pos2(8.0 + col as f32 * 74.0, 8.0 + row as f32 * 118.0), vec2(68.0, 68.0));
            kind(&p, big, *k, Color32::from_rgb(230, 230, 230), Color32::from_white_alpha(18));
            let small = Rect::from_min_size(pos2(big.left() + 17.0, big.bottom() + 6.0), vec2(34.0, 34.0));
            kind(&p, small, *k, Color32::from_rgb(230, 230, 230), Color32::from_white_alpha(18));
        }
    });
    apply_deltas(&mut tex, &out.textures_delta);
    out.textures_delta.clear();
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let img = raster(&prims, &tex, w, h, ppp, [0.12, 0.12, 0.125]);
    write_ppm(img, w, h, &std::env::var("YURA_GLYPHS").unwrap_or_else(|_| "glyphs.ppm".into()));
}
