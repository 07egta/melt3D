//! Small line icons drawn directly with the egui painter.
//!
//! Every icon is designed in unit coordinates (0..1 across a square frame)
//! and mapped into the square centred inside the given rect.

use crate::model::{Kind, Op};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

const PI: f32 = std::f32::consts::PI;
const SEG: usize = 24;

/// A list of unit-coordinate points.
pub(crate) type UnitPath = Vec<(f32, f32)>;

/// Square icon area centred in `rect` with a 10% margin on each side.
pub(crate) fn frame(rect: Rect) -> Rect {
    let s = rect.width().min(rect.height()) * 0.8;
    Rect::from_center_size(rect.center(), vec2(s, s))
}

pub(crate) fn pen(rect: Rect, ink: Color32) -> Stroke {
    Stroke::new((rect.width().min(rect.height()) * 0.058).clamp(1.15, 2.1), ink)
}

/// A continuous stroke with rounded exposed ends and corners. Smooth curve samples do not need
/// their own discs: they make the same line heavier and add many tiny draw shapes every frame.
pub(crate) fn round_path(p: &Painter, mut pts: Vec<Pos2>, s: Stroke, closed: bool) {
    if pts.len() < 2 || s.is_empty() {
        return;
    }
    if s.color.a() < 255 {
        // Keep translucent strokes in one path so overlapping caps cannot darken them.
        if closed {
            pts.push(pts[0]);
        }
        p.add(Shape::line(pts, s));
        return;
    }
    let n = pts.len();
    let mut corners = Vec::new();
    for (i, q) in pts.iter().enumerate() {
        let endpoint = !closed && (i == 0 || i + 1 == n);
        let before = *q - pts[(i + n - 1) % n];
        let after = pts[(i + 1) % n] - *q;
        // A shallow turn is already covered by egui's continuous path join.
        let corner = before.dot(after) < 0.9 * (before.length_sq() * after.length_sq()).sqrt();
        if endpoint || corner {
            corners.push(i);
            p.circle_filled(*q, s.width * 0.5, s.color);
        }
    }
    if corners.is_empty() {
        p.add(if closed { Shape::closed_line(pts, s) } else { Shape::line(pts, s) });
        return;
    }
    // Split only at sharp turns. The round cap joins the two butt-ended strokes without a
    // miter protruding beyond it; curves between the turns stay a single continuous path.
    for pair in corners.windows(2) {
        p.add(Shape::line(pts[pair[0]..=pair[1]].to_vec(), s));
    }
    if closed {
        let (Some(&first), Some(&last)) = (corners.first(), corners.last()) else { return };
        let wrap = pts[last..].iter().chain(&pts[..=first]).copied().collect();
        p.add(Shape::line(wrap, s));
    }
}

/// Unit coordinates to screen points.
pub(crate) fn at(f: Rect, u: f32, v: f32) -> Pos2 {
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

/// Closed outline, with a continuous join at the first point too.
fn outline(p: &Painter, f: Rect, pts: &[(f32, f32)], s: Stroke) {
    round_path(p, px(f, pts), s, true);
}

fn polygon(p: &Painter, f: Rect, pts: &[(f32, f32)], fill: Color32, s: Stroke) {
    if fill.a() > 0 {
        p.add(Shape::convex_polygon(px(f, pts), fill, Stroke::NONE));
    }
    if s.width > 0.0 {
        round_path(p, px(f, pts), s, true);
    }
}

fn segment(p: &Painter, f: Rect, a: (f32, f32), b: (f32, f32), s: Stroke) {
    round_path(p, vec![at(f, a.0, a.1), at(f, b.0, b.1)], s, false);
}

/// The one smooth path shared by the pipe, pen, curve and sweep icons, in unit coordinates.
/// It starts low on the left, turns over in the middle and ends high on the right.
pub(crate) fn family_path(n: usize) -> UnitPath {
    let (a, c1, c2, b) = ((0.12, 0.62), (0.42, 0.62), (0.58, 0.38), (0.88, 0.38));
    (0..=n)
        .map(|i| {
            let t = i as f32 / n.max(1) as f32;
            let m = 1.0 - t;
            let w = |p: f32, q: f32, r: f32, s: f32| m * m * m * p + 3.0 * m * m * t * q + 3.0 * m * t * t * r + t * t * t * s;
            (w(a.0, c1.0, c2.0, b.0), w(a.1, c1.1, c2.1, b.1))
        })
        .collect()
}

/// The control-node mark shared by every member of the pipe / pen / curve / sweep family.
pub(crate) fn node_mark(p: &Painter, f: Rect, q: (f32, f32), ink: Color32) {
    p.circle_filled(at(f, q.0, q.1), f.width() * 0.062, ink);
}

/// Marks control nodes at fractions of `path` (0 = start, 1 = end).
pub(crate) fn node_marks(p: &Painter, f: Rect, path: &[(f32, f32)], ts: &[f32], ink: Color32) {
    let last = path.len().saturating_sub(1);
    if last == 0 {
        return;
    }
    for &t in ts {
        let i = (t.clamp(0.0, 1.0) * last as f32).round() as usize;
        if let Some(&q) = path.get(i) {
            node_mark(p, f, q, ink);
        }
    }
}

/// A slim pen nib whose tip is at `tip`; `back` points from the tip along the body.
pub(crate) fn pen_nib(p: &Painter, f: Rect, tip: (f32, f32), back: (f32, f32), s: Stroke) {
    let t = at(f, tip.0, tip.1);
    let d = vec2(back.0, back.1);
    if d.length_sq() < 1e-6 {
        return;
    }
    let d = d.normalized();
    let n = vec2(-d.y, d.x);
    let w = f.width() * 0.05;
    let a = t + d * (f.width() * 0.10);
    let b = t + d * (f.width() * 0.34);
    round_path(p, vec![t, a + n * w, b + n * w * 0.8, b - n * w * 0.8, a - n * w], s, true);
    round_path(p, vec![t, a], s, false);
}

/// Two sides of a band, offset from `path` by `half`.
pub(crate) fn offset_path(path: &[(f32, f32)], half: f32) -> (UnitPath, UnitPath) {
    let mut a = Vec::with_capacity(path.len());
    let mut b = Vec::with_capacity(path.len());
    for (i, &q) in path.iter().enumerate() {
        let p0 = path.get(i.saturating_sub(1)).copied().unwrap_or(q);
        let p1 = path.get(i + 1).copied().unwrap_or(q);
        let (tx, ty) = (p1.0 - p0.0, p1.1 - p0.1);
        let l = tx.hypot(ty).max(1e-6);
        let (nx, ny) = (-ty / l, tx / l);
        a.push((q.0 + nx * half, q.1 + ny * half));
        b.push((q.0 - nx * half, q.1 - ny * half));
    }
    (a, b)
}

/// A filled band between two sides of equal length; no outline.
pub(crate) fn band(p: &Painter, f: Rect, a: &[(f32, f32)], b: &[(f32, f32)], fill: Color32) {
    if fill.a() == 0 {
        return;
    }
    let mut m = egui::epaint::Mesh::default();
    for (&pa, &pb) in a.iter().zip(b.iter()) {
        m.colored_vertex(at(f, pa.0, pa.1), fill);
        m.colored_vertex(at(f, pb.0, pb.1), fill);
    }
    let n = a.len().min(b.len()) as u32;
    for i in 0..n.saturating_sub(1) {
        let k = 2 * i;
        m.add_triangle(k, k + 1, k + 2);
        m.add_triangle(k + 1, k + 3, k + 2);
    }
    p.add(Shape::mesh(m));
}

/// Screen points for unit coordinates along `path`.
pub(crate) fn path_points(f: Rect, path: &[(f32, f32)]) -> Vec<Pos2> {
    path.iter().map(|&(u, v)| at(f, u, v)).collect()
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
    let w = (rect.width().min(rect.height()) * 0.1).clamp(1.8, 3.4);
    let s = Stroke::new(w, ink);
    p.rect_filled(f, f.width() * 0.3, ink.gamma_multiply(0.09));
    match o {
        Op::Add => {
            segment(p, f, (0.27, 0.5), (0.73, 0.5), s);
            segment(p, f, (0.5, 0.27), (0.5, 0.73), s);
        }
        Op::Subtract => {
            segment(p, f, (0.27, 0.5), (0.73, 0.5), s);
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
    fn smooth_closed_icons_use_one_continuous_stroke() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(34.0, 34.0));
            outline(ui.painter(), r, &ellipse(0.5, 0.5, 0.4, 0.4), Stroke::new(1.5, Color32::WHITE));
        });
        assert_eq!(out.shapes.iter().filter(|s| matches!(s.shape, Shape::Circle(_))).count(), 0, "curve samples must not each add a disc");
        assert_eq!(out.shapes.iter().filter(|s| matches!(&s.shape, Shape::Path(p) if p.closed)).count(), 1, "no open seam at the first point");
        out.textures_delta.clear();
    }

    #[test]
    fn sharp_icon_corners_do_not_grow_miter_spikes() {
        let ctx = egui::Context::default();
        let points = vec![pos2(10.0, 30.0), pos2(30.0, 29.0), pos2(50.0, 30.0), pos2(30.0, 31.0)];
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            round_path(ui.painter(), points.clone(), Stroke::new(2.0, Color32::WHITE), true);
        });
        out.textures_delta.clear();
        let bounds = Rect::from_min_max(pos2(8.0, 27.0), pos2(52.0, 33.0));
        for primitive in ctx.tessellate(out.shapes, 2.0) {
            if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                assert!(mesh.vertices.iter().all(|v| bounds.contains(v.pos)), "a rounded tip must stay within the stroke radius and feathering");
            }
        }
    }

    #[test]
    fn family_tool_icons_fit_a_strip_button() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(pos2(16.0, 16.0), vec2(26.0, 26.0));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::curve_tool::glyph(ui.painter(), rect, Color32::WHITE);
            crate::pipe_tool::glyph(ui.painter(), rect, Color32::WHITE);
            crate::sketch_ui::glyph(ui.painter(), rect, Color32::WHITE);
            crate::sketch_icons::pen_tool(ui.painter(), rect, crate::sketch_ui::Pen::Spline, Color32::WHITE);
            crate::sketch_icons::sweep(ui.painter(), rect, Color32::WHITE);
        });
        for shape in &out.shapes {
            let bounds = shape.shape.visual_bounding_rect();
            if bounds.is_positive() {
                assert!(rect.expand(1.0).contains_rect(bounds), "family icon escapes the strip button: {bounds:?}");
            }
        }
        out.textures_delta.clear();
    }

    #[test]
    fn every_icon_draws() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(34.0, 34.0));
            for k in Kind::ALL {
                for ink in [Color32::from_rgb(230, 230, 240), Color32::WHITE] {
                    kind(&p, r, k, ink, ink.gamma_multiply(0.15));
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

/// `YURA_GLYPHS=/path/sheet.ppm cargo test -p meltcad glyph_sheet -- --ignored`: every shape icon,
/// large and at 34 px, plus the pipe / pen / curve / sweep family at tool-strip size.
#[cfg(test)]
#[test]
#[ignore]
fn glyph_sheet() {
    use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};

    fn kind_fill(p: &Painter, r: Rect, ink: Color32, k: Kind) {
        kind(p, r, k, ink, ink.gamma_multiply(0.15));
    }
    fn kind_curve(p: &Painter, r: Rect, ink: Color32) {
        kind_fill(p, r, ink, Kind::Curve);
    }
    fn kind_sweep(p: &Painter, r: Rect, ink: Color32) {
        kind_fill(p, r, ink, Kind::Sweep);
    }
    fn kind_tube(p: &Painter, r: Rect, ink: Color32) {
        kind_fill(p, r, ink, Kind::Tube);
    }
    fn spline(p: &Painter, r: Rect, ink: Color32) {
        crate::sketch_icons::pen_tool(p, r, crate::sketch_ui::Pen::Spline, ink);
    }

    let ppp = 2.0;
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(ppp);
    let mut tex = Textures::new();
    let size = vec2(900.0, 400.0);
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(ui.max_rect(), 0.0, Color32::from_rgb(31, 31, 32));
        let ink = Color32::from_rgb(230, 230, 230);
        for (i, k) in Kind::ALL.iter().enumerate() {
            let (col, row) = (i % 10, i / 10);
            let big = Rect::from_min_size(pos2(8.0 + col as f32 * 74.0, 8.0 + row as f32 * 118.0), vec2(68.0, 68.0));
            kind(&p, big, *k, ink, ink.gamma_multiply(0.15));
            let small = Rect::from_min_size(pos2(big.left() + 17.0, big.bottom() + 6.0), vec2(34.0, 34.0));
            kind(&p, small, *k, ink, ink.gamma_multiply(0.15));
        }
        type Draw = fn(&Painter, Rect, Color32);
        let family: [(&str, Draw); 8] = [
            ("Curve", kind_curve),
            ("Sweep", kind_sweep),
            ("Pipe solid", kind_tube),
            ("Curve tool", crate::curve_tool::glyph),
            ("Pipe tool", crate::pipe_tool::glyph),
            ("Sketch tool", crate::sketch_ui::glyph),
            ("Spline pen", spline),
            ("Sweep button", crate::sketch_icons::sweep),
        ];
        for (i, (label, draw)) in family.iter().enumerate() {
            let x = 8.0 + i as f32 * 110.0;
            let big = Rect::from_min_size(pos2(x, 258.0), vec2(68.0, 68.0));
            draw(&p, big, ink);
            let small = Rect::from_min_size(pos2(big.left() + 17.0, big.bottom() + 6.0), vec2(34.0, 34.0));
            draw(&p, small, ink);
            p.text(pos2(big.center().x, big.bottom() + 48.0), egui::Align2::CENTER_CENTER, *label, egui::FontId::proportional(11.0), ink);
        }
    });
    apply_deltas(&mut tex, &out.textures_delta);
    out.textures_delta.clear();
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let img = raster(&prims, &tex, w, h, ppp, [0.12, 0.12, 0.125]);
    write_ppm(img, w, h, &std::env::var("YURA_GLYPHS").unwrap_or_else(|_| "glyphs.ppm".into()));
}

/// `YURA_GLYPHS_TINY=/path/tiny.ppm cargo test -p meltcad glyph_tiny_sheet -- --ignored`:
/// every Kind and the pipe / pen / curve / sweep family at the 24 px and 16 px sizes the
/// review notes ask about. The normal sheet's "small" row is 34 px, which is too generous.
#[cfg(test)]
#[test]
#[ignore]
fn glyph_tiny_sheet() {
    use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};

    fn kind_small(p: &Painter, r: Rect, ink: Color32, k: Kind) {
        kind(p, r, k, ink, ink.gamma_multiply(0.15));
    }
    fn curve_small(p: &Painter, r: Rect, ink: Color32) {
        kind_small(p, r, ink, Kind::Curve);
    }
    fn sweep_small(p: &Painter, r: Rect, ink: Color32) {
        kind_small(p, r, ink, Kind::Sweep);
    }
    fn spline(p: &Painter, r: Rect, ink: Color32) {
        crate::sketch_icons::pen_tool(p, r, crate::sketch_ui::Pen::Spline, ink);
    }

    let ppp = 4.0;
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(ppp);
    let mut tex = Textures::new();
    let size = vec2(820.0, 190.0);
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(ui.max_rect(), 0.0, Color32::from_rgb(31, 31, 32));
        let ink = Color32::from_rgb(230, 230, 230);
        for (i, k) in Kind::ALL.iter().enumerate() {
            let (col, row) = (i % 10, i / 10);
            let x = 8.0 + col as f32 * 80.0;
            let y = 4.0 + row as f32 * 48.0;
            kind(&p, Rect::from_min_size(pos2(x, y), vec2(24.0, 24.0)), *k, ink, ink.gamma_multiply(0.15));
            kind(&p, Rect::from_min_size(pos2(x + 30.0, y + 4.0), vec2(16.0, 16.0)), *k, ink, ink.gamma_multiply(0.15));
        }
        type Draw = fn(&Painter, Rect, Color32);
        let family: [(&str, Draw); 7] = [
            ("Curve", curve_small),
            ("Sweep", sweep_small),
            ("Curve tool", crate::curve_tool::glyph),
            ("Pipe tool", crate::pipe_tool::glyph),
            ("Sketch tool", crate::sketch_ui::glyph),
            ("Spline pen", spline),
            ("Sweep button", crate::sketch_icons::sweep),
        ];
        for (i, (label, draw)) in family.iter().enumerate() {
            let x = 8.0 + i as f32 * 112.0;
            draw(&p, Rect::from_min_size(pos2(x, 112.0), vec2(24.0, 24.0)), ink);
            draw(&p, Rect::from_min_size(pos2(x + 30.0, 116.0), vec2(16.0, 16.0)), ink);
            p.text(pos2(x + 20.0, 166.0), egui::Align2::CENTER_CENTER, *label, egui::FontId::proportional(10.0), ink);
        }
    });
    apply_deltas(&mut tex, &out.textures_delta);
    out.textures_delta.clear();
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let img = raster(&prims, &tex, w, h, ppp, [0.12, 0.12, 0.125]);
    write_ppm(img, w, h, &std::env::var("YURA_GLYPHS_TINY").unwrap_or_else(|_| "glyphs_tiny.ppm".into()));
}
