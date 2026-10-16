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
    Stroke::new((rect.width() * 0.075).clamp(1.2, 2.6), ink)
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

/// The clay colour the 3D thumbnails are made of: a soft lavender grey, a little brighter when `ink` is.
fn clay(ink: Color32, fill: Color32) -> Color32 {
    if fill.a() >= 128 { fill } else { gfx::mix(Color32::from_rgb(168, 160, 232), ink, 0.32) }
}

/// Icon area for the shaded thumbnails: a bit bigger than the line icons (4% margin).
fn big(rect: Rect) -> Rect {
    let s = rect.width().min(rect.height()) * 0.92;
    Rect::from_center_size(rect.center(), vec2(s, s))
}

/// Draws a solid model primitive icon as a small shaded 3D thumbnail (lit from the top left).
/// `ink` tints the clay a little (brighter when hovered); a `fill` that is not nearly transparent replaces the clay.
pub fn kind(p: &Painter, rect: Rect, k: Kind, ink: Color32, fill: Color32) {
    let b = big(rect);
    let base = clay(ink, fill);
    match k {
        Kind::Sphere => sphere(p, b, base),
        Kind::Box => cube(p, b, base),
        Kind::Cylinder => cylinder(p, b, base),
        Kind::Torus => torus(p, b, base),
        Kind::Cone => cone(p, b, base),
        Kind::Extrude => prism(p, b, base),
        _ => crate::glyphs_more::kind_more(p, rect, k, ink, fill),
    }
}

/// Colour across a lit cylinder or cone: dark left edge, bright shoulder, soft middle, dark right edge.
fn side_ramp(base: Color32, u: f32) -> Color32 {
    gfx::ramp(&[(0.0, gfx::lit(base, 0.5)), (0.26, gfx::lit(base, 1.16)), (0.5, gfx::lit(base, 0.95)), (1.0, gfx::lit(base, 0.36))], u)
}

fn soft_shadow(p: &Painter, f: Rect, cu: f32, cv: f32, ru: f32, rv: f32) {
    p.add(Shape::ellipse_filled(at(f, cu, cv), vec2(f.width() * ru, f.height() * rv), Color32::from_black_alpha(64)));
}

fn sphere(p: &Painter, f: Rect, base: Color32) {
    let c = at(f, 0.5, 0.47);
    let r = f.width() * 0.43;
    soft_shadow(p, f, 0.5, 0.95, 0.30, 0.045);
    gfx::ball(p, c, r, vec2(-0.40, -0.44), &[(0.0, gfx::lit(base, 1.4)), (0.3, gfx::lit(base, 1.08)), (0.72, gfx::lit(base, 0.6)), (1.0, gfx::lit(base, 0.34))]);
    // light bouncing back from the floor
    let rim: Vec<Pos2> = (0..=8).map(|i| c + vec2((PI * (0.1 + 0.4 * i as f32 / 8.0)).cos(), (PI * (0.1 + 0.4 * i as f32 / 8.0)).sin()) * r * 0.92).collect();
    p.add(Shape::line(rim, Stroke::new(1.0, Color32::from_white_alpha(46))));
    // the shiny spot
    let spot = c + vec2(-r * 0.40, -r * 0.44);
    p.circle_filled(spot, r * 0.24, Color32::from_white_alpha(56));
    p.add(Shape::ellipse_filled(spot, vec2(r * 0.14, r * 0.10), Color32::from_white_alpha(235)));
}

fn cube(p: &Painter, f: Rect, base: Color32) {
    soft_shadow(p, f, 0.5, 0.965, 0.34, 0.04);
    let top = px(f, &[(0.5, 0.05), (0.93, 0.28), (0.5, 0.52), (0.07, 0.28)]);
    let left = px(f, &[(0.07, 0.28), (0.5, 0.52), (0.5, 0.96), (0.07, 0.72)]);
    let right = px(f, &[(0.5, 0.52), (0.93, 0.28), (0.93, 0.72), (0.5, 0.96)]);
    gfx::shaded_poly(p, &top, &[gfx::lit(base, 1.4), gfx::lit(base, 1.18), gfx::lit(base, 1.02), gfx::lit(base, 1.2)]);
    gfx::shaded_poly(p, &left, &[gfx::lit(base, 0.9), gfx::lit(base, 0.78), gfx::lit(base, 0.56), gfx::lit(base, 0.66)]);
    gfx::shaded_poly(p, &right, &[gfx::lit(base, 0.58), gfx::lit(base, 0.5), gfx::lit(base, 0.34), gfx::lit(base, 0.4)]);
    let edge = Stroke::new(1.0, Color32::from_black_alpha(90));
    for face in [&top, &left, &right] {
        let mut o = face.clone();
        o.push(o[0]);
        p.add(Shape::line(o, edge));
    }
    // bright lip along the two top edges
    p.add(Shape::line(vec![at(f, 0.07, 0.28), at(f, 0.5, 0.05), at(f, 0.93, 0.28)], Stroke::new(1.2, Color32::from_white_alpha(120))));
}

fn cylinder(p: &Painter, f: Rect, base: Color32) {
    soft_shadow(p, f, 0.5, 0.95, 0.38, 0.05);
    let (x0, x1, top) = (0.08, 0.92, 0.24);
    let n = 14;
    let mut mesh = Mesh::default();
    let mut lower = Vec::new();
    for i in 0..=n {
        let u = i as f32 / n as f32;
        let x = x0 + (x1 - x0) * u;
        let e = ((x - 0.5) / 0.42).clamp(-1.0, 1.0);
        let bottom = 0.76 + 0.14 * (1.0 - e * e).max(0.0).sqrt();
        let col = side_ramp(base, u);
        mesh.colored_vertex(at(f, x, top), col);
        mesh.colored_vertex(at(f, x, bottom), col);
        lower.push((x, bottom));
    }
    for i in 0..n as u32 {
        let a = 2 * i;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    p.add(Shape::mesh(mesh));
    let edge = Stroke::new(1.0, Color32::from_black_alpha(80));
    polyline(p, f, &lower, edge);
    segment(p, f, (x0, top), (x0, 0.76), edge);
    segment(p, f, (x1, top), (x1, 0.76), edge);
    // the lid
    let lid = px(f, &ellipse(0.5, top, 0.42, 0.14));
    let (l, w) = (f.left(), f.width());
    gfx::fan(p, &lid, |q| gfx::mix(gfx::lit(base, 1.42), gfx::lit(base, 1.04), ((q.x - l) / w - 0.1) * 1.3 + (q.y - f.top()) / f.height() * 0.5));
    let mut ring = lid;
    ring.push(ring[0]);
    p.add(Shape::line(ring, Stroke::new(1.0, Color32::from_white_alpha(110))));
}

fn torus(p: &Painter, f: Rect, base: Color32) {
    soft_shadow(p, f, 0.5, 0.93, 0.36, 0.04);
    let (cu, cv) = (0.5, 0.5);
    let rings = [(0.45, 0.35), (0.325, 0.245), (0.2, 0.14)];
    let n = 36;
    let light = vec2(-0.55, -0.83);
    let mut mesh = Mesh::default();
    for i in 0..n {
        let a = 2.0 * PI * i as f32 / n as f32;
        let (s, c) = a.sin_cos();
        let d = c * light.x + s * light.y;
        for (j, (ru, rv)) in rings.iter().enumerate() {
            let k = match j {
                0 => 0.6 + 0.32 * d,
                1 => 1.04 + 0.3 * d,
                _ => 0.58 - 0.3 * d,
            };
            mesh.colored_vertex(at(f, cu + ru * c, cv + rv * s), gfx::lit(base, k));
        }
    }
    let nn = n as u32;
    for i in 0..nn {
        let (a, b) = (3 * i, 3 * ((i + 1) % nn));
        for j in 0..2 {
            mesh.add_triangle(a + j, a + j + 1, b + j);
            mesh.add_triangle(a + j + 1, b + j + 1, b + j);
        }
    }
    p.add(Shape::mesh(mesh));
    let edge = Stroke::new(1.0, Color32::from_black_alpha(85));
    outline(p, f, &ellipse(cu, cv, rings[0].0, rings[0].1), edge);
    outline(p, f, &ellipse(cu, cv, rings[2].0, rings[2].1), edge);
    // a shiny streak on the top of the tube
    polyline(p, f, &arc(cu, cv, rings[1].0, rings[1].1, PI * 1.12, PI * 1.5, 8), Stroke::new(1.6, Color32::from_white_alpha(150)));
}

fn cone(p: &Painter, f: Rect, base: Color32) {
    soft_shadow(p, f, 0.5, 0.93, 0.40, 0.05);
    let mut body = vec![(0.5, 0.06), (0.08, 0.78)];
    body.extend(arc(0.5, 0.78, 0.42, 0.14, PI, 0.0, 12).into_iter().skip(1));
    let pts = px(f, &body);
    let (l, w, t, h) = (f.left(), f.width(), f.top(), f.height());
    gfx::fan(p, &pts, |q| {
        let u = (((q.x - l) / w) - 0.08) / 0.84;
        gfx::lit(side_ramp(base, u), 1.0 + 0.16 * (1.0 - (q.y - t) / h))
    });
    let mut o = pts;
    o.push(o[0]);
    p.add(Shape::line(o, Stroke::new(1.0, Color32::from_black_alpha(85))));
    polyline(p, f, &[(0.5, 0.08), (0.30, 0.70)], Stroke::new(1.3, Color32::from_white_alpha(90)));
}

fn prism(p: &Painter, f: Rect, base: Color32) {
    soft_shadow(p, f, 0.5, 0.95, 0.36, 0.04);
    let drop = 0.40;
    let top = hex(0.5, 0.30, 0.40, 0.19);
    let bot: Vec<(f32, f32)> = top.iter().map(|&(u, v)| (u, v + drop)).collect();
    let edge = Stroke::new(1.0, Color32::from_black_alpha(85));
    for (k, shade) in [(0usize, 0.5_f32), (1, 0.74), (2, 0.96)] {
        let face = px(f, &[top[k], top[k + 1], bot[k + 1], bot[k]]);
        let (a, b) = (gfx::lit(base, shade * 1.1), gfx::lit(base, shade * 0.8));
        gfx::shaded_poly(p, &face, &[a, a, b, b]);
        let mut o = face;
        o.push(o[0]);
        p.add(Shape::line(o, edge));
    }
    let lid = px(f, &top);
    let cols: Vec<Color32> = (0..6)
        .map(|k| {
            let a = k as f32 * PI / 3.0;
            gfx::lit(base, 1.16 + 0.24 * (a.cos() * -0.6 + a.sin() * -0.8))
        })
        .collect();
    gfx::shaded_poly(p, &lid, &cols);
    let mut o = lid;
    o.push(o[0]);
    p.add(Shape::line(o, Stroke::new(1.0, Color32::from_white_alpha(100))));
    // a small arrow: pushed straight up
    let tip = at(f, 0.5, 0.30);
    let tail = at(f, 0.5, 0.52);
    let ink = Color32::from_black_alpha(120);
    p.line_segment([tail, tip], Stroke::new(1.4, ink));
    p.line_segment([tip, tip + vec2(-f.width() * 0.07, f.height() * 0.08)], Stroke::new(1.4, ink));
    p.line_segment([tip, tip + vec2(f.width() * 0.07, f.height() * 0.08)], Stroke::new(1.4, ink));
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

/// Almond eye with a pupil; a diagonal slash is added when `open` is false.
pub fn eye(p: &Painter, rect: Rect, open: bool, ink: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    let mut almond = quad((0.05, 0.5), (0.5, -0.1), (0.95, 0.5));
    almond.extend(quad((0.95, 0.5), (0.5, 1.1), (0.05, 0.5)).into_iter().skip(1));
    if open {
        polygon(p, f, &almond, ink.gamma_multiply(0.14), Stroke::NONE);
    }
    outline(p, f, &almond, s);
    p.circle_filled(at(f, 0.5, 0.5), f.width() * 0.15, ink);
    if !open {
        segment(p, f, (0.15, 0.15), (0.85, 0.85), s);
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
