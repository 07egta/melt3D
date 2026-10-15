//! Small line icons drawn directly with the egui painter.
//!
//! Every icon is designed in unit coordinates (0..1 across a square frame)
//! and mapped into the square centred inside the given rect.

use crate::model::{Kind, Op};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

const PI: f32 = std::f32::consts::PI;
const SEG: usize = 24;

/// Square icon area centred in `rect` with a 10% margin on each side.
fn frame(rect: Rect) -> Rect {
    let s = rect.width().min(rect.height()) * 0.8;
    Rect::from_center_size(rect.center(), vec2(s, s))
}

fn pen(rect: Rect, ink: Color32) -> Stroke {
    Stroke::new((rect.width() * 0.07).clamp(1.0, 2.5), ink)
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
    p.add(Shape::line(px(f, pts), s));
}

/// Closed outline (first point is repeated at the end).
fn outline(p: &Painter, f: Rect, pts: &[(f32, f32)], s: Stroke) {
    let mut v = px(f, pts);
    if let Some(&first) = v.first() {
        v.push(first);
    }
    p.add(Shape::line(v, s));
}

fn polygon(p: &Painter, f: Rect, pts: &[(f32, f32)], fill: Color32, s: Stroke) {
    p.add(Shape::convex_polygon(px(f, pts), fill, s));
}

fn segment(p: &Painter, f: Rect, a: (f32, f32), b: (f32, f32), s: Stroke) {
    p.line_segment([at(f, a.0, a.1), at(f, b.0, b.1)], s);
}

/// Draws a solid model primitive icon.
pub fn kind(p: &Painter, rect: Rect, k: Kind, ink: Color32, fill: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    match k {
        Kind::Sphere => sphere(p, f, fill, s),
        Kind::Box => cube(p, f, fill, s),
        Kind::Cylinder => cylinder(p, f, fill, s),
        Kind::Torus => torus(p, f, fill, s),
        Kind::Cone => cone(p, f, fill, s),
        Kind::Extrude => prism(p, f, fill, s),
    }
}

fn sphere(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let r = f.width() * 0.5;
    p.circle_filled(f.center(), r, fill);
    p.circle_stroke(f.center(), r, s);
    polyline(p, f, &arc(0.5, 0.5, 0.32, 0.32, PI * 1.1, PI * 1.45, 8), s);
}

fn cube(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    polygon(p, f, &[(0.5, 0.05), (0.93, 0.28), (0.5, 0.5), (0.07, 0.28)], fill, s);
    let left = fill.gamma_multiply(0.8);
    polygon(p, f, &[(0.07, 0.28), (0.5, 0.5), (0.5, 0.95), (0.07, 0.72)], left, s);
    let right = fill.gamma_multiply(0.6);
    polygon(p, f, &[(0.5, 0.5), (0.93, 0.28), (0.93, 0.72), (0.5, 0.95)], right, s);
}

fn cylinder(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let mut body = arc(0.5, 0.78, 0.42, 0.14, PI, 0.0, 12);
    body.extend(arc(0.5, 0.22, 0.42, 0.14, 0.0, -PI, 12));
    polygon(p, f, &body, fill.gamma_multiply(0.8), s);
    polygon(p, f, &ellipse(0.5, 0.22, 0.42, 0.14), fill, s);
}

fn torus(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    polygon(p, f, &ellipse(0.5, 0.52, 0.44, 0.34), fill, s);
    polygon(p, f, &ellipse(0.5, 0.52, 0.2, 0.12), fill.gamma_multiply(0.5), s);
    polyline(p, f, &arc(0.5, 0.52, 0.32, 0.29, PI * 1.15, PI * 1.85, 8), s);
}

fn cone(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    polygon(p, f, &ellipse(0.5, 0.8, 0.42, 0.13), fill, s);
    let mut body = vec![(0.5, 0.08)];
    body.extend(arc(0.5, 0.8, 0.42, 0.13, PI, 0.0, 12));
    polygon(p, f, &body, fill.gamma_multiply(0.8), s);
}

fn prism(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let top = hex(0.40, 0.36, 0.26, 0.16);
    let bot = hex(0.40, 0.66, 0.26, 0.16);
    outline(p, f, &bot, s);
    for i in 0..4 {
        segment(p, f, top[i], bot[i], s);
    }
    polygon(p, f, &top, fill, s);
    segment(p, f, (0.86, 0.88), (0.86, 0.14), s);
    segment(p, f, (0.86, 0.14), (0.76, 0.26), s);
    segment(p, f, (0.86, 0.14), (0.96, 0.26), s);
}

/// Plus for `Add`, minus bar for `Subtract`, inside a rounded square.
pub fn op(p: &Painter, rect: Rect, o: Op, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    p.rect_stroke(f, f.width() * 0.2, s, StrokeKind::Inside);
    segment(p, f, (0.25, 0.5), (0.75, 0.5), s);
    if matches!(o, Op::Add) {
        segment(p, f, (0.5, 0.25), (0.5, 0.75), s);
    }
}

/// Almond eye with a pupil; a diagonal slash is added when `open` is false.
pub fn eye(p: &Painter, rect: Rect, open: bool, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    let mut almond = quad((0.05, 0.5), (0.5, -0.1), (0.95, 0.5));
    almond.extend(quad((0.95, 0.5), (0.5, 1.1), (0.05, 0.5)).into_iter().skip(1));
    outline(p, f, &almond, s);
    p.circle_filled(at(f, 0.5, 0.5), f.width() * 0.14, ink);
    if !open {
        segment(p, f, (0.15, 0.15), (0.85, 0.85), s);
    }
}

/// Folder outline with a tab.
pub fn group(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    outline(p, f, &[(0.08, 0.25), (0.40, 0.25), (0.48, 0.37), (0.92, 0.37), (0.92, 0.82), (0.08, 0.82)], s);
}

/// Bin with lid, handle and two ribs.
pub fn trash(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    segment(p, f, (0.18, 0.28), (0.82, 0.28), s);
    polyline(p, f, &[(0.40, 0.28), (0.40, 0.14), (0.60, 0.14), (0.60, 0.28)], s);
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

/// Floppy disk: body, shutter and label.
pub fn save(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    outline(p, f, &[(0.12, 0.12), (0.74, 0.12), (0.88, 0.26), (0.88, 0.88), (0.12, 0.88)], s);
    outline(p, f, &[(0.28, 0.12), (0.28, 0.38), (0.70, 0.38), (0.70, 0.12)], s);
    outline(p, f, &[(0.25, 0.60), (0.75, 0.60), (0.75, 0.88), (0.25, 0.88)], s);
}

/// Folder with an arrow pointing out of it.
pub fn open(p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    outline(p, f, &[(0.08, 0.30), (0.40, 0.30), (0.48, 0.40), (0.92, 0.40), (0.92, 0.84), (0.08, 0.84)], s);
    segment(p, f, (0.50, 0.62), (0.50, 0.12), s);
    segment(p, f, (0.38, 0.24), (0.50, 0.12), s);
    segment(p, f, (0.62, 0.24), (0.50, 0.12), s);
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
