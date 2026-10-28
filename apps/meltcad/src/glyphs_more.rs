//! Line icons for the extra solids (Prism3 .. Star) and the CAD features (Revolve .. Sweep).
//! Same style as `glyphs.rs`: unit coordinates (0..1) in a square frame, drawn with painter
//! primitives only. y grows downwards.

use crate::model::Kind;
use egui::epaint::Mesh;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

type P = (f32, f32);

const PI: f32 = std::f32::consts::PI;

fn frame(rect: Rect) -> Rect {
    crate::glyphs::frame(rect)
}

fn pen(rect: Rect, ink: Color32) -> Stroke {
    crate::glyphs::pen(rect, ink)
}

fn at(f: Rect, u: f32, v: f32) -> Pos2 {
    pos2(f.left() + f.width() * u, f.top() + f.height() * v)
}

fn px(f: Rect, pts: &[P]) -> Vec<Pos2> {
    pts.iter().map(|&(u, v)| at(f, u, v)).collect()
}

fn arc(cu: f32, cv: f32, ru: f32, rv: f32, a0: f32, a1: f32, n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            (cu + ru * a.cos(), cv + rv * a.sin())
        })
        .collect()
}

fn ellipse(cu: f32, cv: f32, ru: f32, rv: f32) -> Vec<P> {
    let mut v = arc(cu, cv, ru, rv, 0.0, 2.0 * PI, 24);
    v.pop();
    v
}

fn quad(a: P, c: P, b: P, n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let m = 1.0 - t;
            (m * m * a.0 + 2.0 * m * t * c.0 + t * t * b.0, m * m * a.1 + 2.0 * m * t * c.1 + t * t * b.1)
        })
        .collect()
}

fn polyline(p: &Painter, f: Rect, pts: &[P], s: Stroke) {
    crate::glyphs::round_path(p, px(f, pts), s, false);
}

fn segment(p: &Painter, f: Rect, a: P, b: P, s: Stroke) {
    crate::glyphs::round_path(p, vec![at(f, a.0, a.1), at(f, b.0, b.1)], s, false);
}

/// Convex filled polygon with an outline.
fn poly(p: &Painter, f: Rect, pts: &[P], fill: Color32, s: Stroke) {
    if fill.a() > 0 {
        p.add(Shape::convex_polygon(px(f, pts), fill, Stroke::NONE));
    }
    if s.width > 0.0 {
        crate::glyphs::round_path(p, px(f, pts), s, true);
    }
}

/// Fill of any star-shaped polygon (seen from `c`) as a triangle fan, then its outline.
fn fan(p: &Painter, f: Rect, pts: &[P], c: P, fill: Color32, s: Stroke) {
    if fill.a() > 0 {
        let mut m = Mesh::default();
        m.colored_vertex(at(f, c.0, c.1), fill);
        for &(u, v) in pts {
            m.colored_vertex(at(f, u, v), fill);
        }
        let n = pts.len() as u32;
        for i in 0..n {
            m.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        p.add(Shape::mesh(m));
    }
    crate::glyphs::round_path(p, px(f, pts), s, true);
}

/// A strip of quads between two sides of equal length (for ribbons), no outline.
fn strip(p: &Painter, f: Rect, a: &[P], b: &[P], fill: Color32) {
    if fill.a() == 0 {
        return;
    }
    let mut m = Mesh::default();
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

fn faint(c: Color32) -> Color32 {
    c.gamma_multiply(0.4)
}

/// Vertices of a polygon squashed into an ellipse (a flat top face seen from above).
fn ring(n: usize, cu: f32, cv: f32, ru: f32, rv: f32, start: f32) -> Vec<P> {
    (0..n)
        .map(|k| {
            let a = start + 2.0 * PI * k as f32 / n as f32;
            (cu + ru * a.cos(), cv + rv * a.sin())
        })
        .collect()
}

/// A flat top face pushed down by `drop`: the side faces (far ones first), then the top.
fn slab(p: &Painter, f: Rect, top: &[P], concave_centre: Option<P>, drop: f32, fill: Color32, s: Stroke) {
    let n = top.len();
    let mut order: Vec<usize> = (0..n).collect();
    let mid = |i: usize| {
        let (a, b) = (top[i], top[(i + 1) % n]);
        ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
    };
    order.sort_by(|&i, &j| mid(i).1.total_cmp(&mid(j).1));
    for i in order {
        let (a, b) = (top[i], top[(i + 1) % n]);
        let m = mid(i);
        let shade = 0.9 - 0.45 * m.0.clamp(0.0, 1.0);
        poly(p, f, &[a, b, (b.0, b.1 + drop), (a.0, a.1 + drop)], fill.gamma_multiply(shade), s);
    }
    match concave_centre {
        Some(c) => fan(p, f, top, c, fill, s),
        None => poly(p, f, top, fill, s),
    }
}

fn prism3(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a point-up triangle on top, dropped straight down: a prism, not a floating triangle
    let top = ring(3, 0.5, 0.34, 0.42, 0.22, -PI * 0.5);
    slab(p, f, &top, None, 0.42, fill, s);
}

fn prism_n(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // the default six-sided profile, so it reads as a polygon prism next to the round cylinder
    let top = ring(6, 0.5, 0.34, 0.42, 0.20, PI * 0.5);
    slab(p, f, &top, None, 0.38, fill, s);
}

fn star(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a plain five-point star, drawn flat with a little thickness underneath
    let pts = |dv: f32| -> Vec<P> {
        (0..10)
            .map(|k| {
                let a = -PI * 0.5 + PI * k as f32 / 5.0;
                let r = if k % 2 == 0 { 0.46 } else { 0.2 };
                (0.5 + r * a.cos(), 0.52 + dv + r * a.sin())
            })
            .collect()
    };
    fan(p, f, &pts(0.0), (0.5, 0.52), fill, s);
}

fn octa(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let (t, b, l, r, fr) = ((0.5, 0.04), (0.5, 0.96), (0.06, 0.5), (0.94, 0.5), (0.5, 0.70));
    poly(p, f, &[t, l, fr], fill.gamma_multiply(0.95), s);
    poly(p, f, &[t, fr, r], fill.gamma_multiply(0.7), s);
    poly(p, f, &[l, fr, b], fill.gamma_multiply(0.6), s);
    poly(p, f, &[fr, r, b], fill.gamma_multiply(0.4), s);
}

fn tetra(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // seen down the apex: a triangular base with one face per edge. Its triangular silhouette
    // cannot be mistaken for the square-based pyramid or the cube.
    let (t, l, r, c) = ((0.5, 0.08), (0.08, 0.84), (0.92, 0.84), (0.5, 0.60));
    poly(p, f, &[c, l, r], fill.gamma_multiply(0.95), s);
    poly(p, f, &[t, c, r], fill.gamma_multiply(0.7), s);
    poly(p, f, &[t, l, c], fill.gamma_multiply(0.5), s);
}

fn pyramid(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a square base seen as a diamond, with four edges climbing to one apex
    let (t, l, fr, r, k) = ((0.5, 0.05), (0.06, 0.72), (0.5, 0.95), (0.94, 0.72), (0.5, 0.53));
    poly(p, f, &[l, k, r, fr], fill.gamma_multiply(0.35), Stroke::NONE);
    poly(p, f, &[t, l, fr], fill.gamma_multiply(0.9), s);
    poly(p, f, &[t, fr, r], fill.gamma_multiply(0.55), s);
}

fn capsule(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let mut body = arc(0.5, 0.27, 0.23, 0.23, PI, 2.0 * PI, 12);
    body.extend(arc(0.5, 0.73, 0.23, 0.23, 0.0, PI, 12));
    poly(p, f, &body, fill, s);
    // seams where the ends meet the barrel
    polyline(p, f, &arc(0.5, 0.27, 0.23, 0.06, 0.0, PI, 8), Stroke::new(s.width * 0.7, faint(s.color)));
    polyline(p, f, &arc(0.5, 0.73, 0.23, 0.06, 0.0, PI, 8), Stroke::new(s.width * 0.7, faint(s.color)));
    polyline(p, f, &arc(0.5, 0.27, 0.15, 0.15, PI * 1.05, PI * 1.45, 6), s);
}

fn hemi(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let (cv, rx) = (0.7, 0.45);
    // the flat face, seen from a little above
    poly(p, f, &ellipse(0.5, cv, rx, 0.15), fill.gamma_multiply(0.6), s);
    let mut dome = arc(0.5, cv, rx, 0.62, PI, 2.0 * PI, 16);
    dome.extend(arc(0.5, cv, rx, 0.15, 0.0, PI, 12));
    poly(p, f, &dome, fill, s);
    polyline(p, f, &arc(0.5, cv, 0.3, 0.42, PI * 1.12, PI * 1.5, 8), s);
}

fn tube(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let mut body = arc(0.5, 0.78, 0.42, 0.14, PI, 0.0, 12);
    body.extend(arc(0.5, 0.22, 0.42, 0.14, 0.0, -PI, 12));
    poly(p, f, &body, fill.gamma_multiply(0.8), s);
    poly(p, f, &ellipse(0.5, 0.22, 0.42, 0.14), fill, s);
    // the hole, and its far wall
    poly(p, f, &ellipse(0.5, 0.22, 0.26, 0.085), fill.gamma_multiply(0.35), s);
    polyline(p, f, &arc(0.5, 0.22, 0.26, 0.085, 0.0, PI, 8), s);
}

fn wedge(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a ramp: the profile triangle in front, the slope running back and up to the right
    let (a, b, c) = ((0.05, 0.94), (0.66, 0.94), (0.05, 0.44));
    let d = (0.28, -0.3);
    let sh = |q: P| (q.0 + d.0, q.1 + d.1);
    let (b2, c2) = (sh(b), sh(c));
    poly(p, f, &[c, b, b2, c2], fill.gamma_multiply(0.7), s);
    poly(p, f, &[a, b, c], fill, s);
}

fn revolve(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    let mut left = vec![(0.40, 0.08)];
    left.extend(quad((0.40, 0.20), (0.03, 0.45), (0.30, 0.90), 12));
    let right: Vec<P> = left.iter().rev().map(|&(u, v)| (1.0 - u, v)).collect();
    let mut body = left.clone();
    body.extend(arc(0.5, 0.90, 0.20, 0.05, PI, 0.0, 8));
    body.extend(right);
    fan(p, f, &body, (0.5, 0.5), fill, s);
    // the turned rim and a ring around the belly
    poly(p, f, &ellipse(0.5, 0.08, 0.10, 0.035), fill.gamma_multiply(0.6), s);
    polyline(p, f, &arc(0.5, 0.52, 0.335, 0.07, 0.0, PI, 10), Stroke::new(s.width * 0.8, faint(s.color)));
}

fn loft(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a square at the bottom, a triangle at the top
    let (l, fr, r, k) = ((0.08, 0.76), (0.5, 0.96), (0.92, 0.76), (0.5, 0.58));
    let (tl, tr, tf) = ((0.34, 0.16), (0.66, 0.16), (0.5, 0.30));
    poly(p, f, &[l, k, r, fr], fill.gamma_multiply(0.45), Stroke::new(s.width * 0.7, faint(s.color)));
    poly(p, f, &[l, fr, tf, tl], fill.gamma_multiply(0.85), s);
    poly(p, f, &[fr, r, tr, tf], fill.gamma_multiply(0.6), s);
    poly(p, f, &[tl, tr, tf], fill, s);
}

/// Points of an S shaped path from left to right.
fn s_path(n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            (0.1 + 0.8 * t, 0.5 + 0.3 * (2.0 * PI * t).sin())
        })
        .collect()
}

fn curve(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a smooth line with the nodes you drag
    let path = s_path(24);
    crate::glyphs::round_path(p, px(f, &path), Stroke::new(s.width * 1.25, s.color), false);
    for i in [0, 12, 24] {
        let q = path[i];
        p.circle_filled(at(f, q.0, q.1), f.width() * 0.085, s.color);
        p.circle_filled(at(f, q.0, q.1), f.width() * 0.04, fill);
    }
}

fn sweep(p: &Painter, f: Rect, fill: Color32, s: Stroke) {
    // a ribbon along the path: two edges and a soft fill, with the profile ring at its start
    let path = s_path(24);
    let half = 0.07;
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for i in 0..path.len() {
        let (p0, p1) = (path[i.saturating_sub(1)], path[(i + 1).min(path.len() - 1)]);
        let (tx, ty) = (p1.0 - p0.0, p1.1 - p0.1);
        let l = tx.hypot(ty).max(1e-6);
        let (nx, ny) = (-ty / l, tx / l);
        a.push((path[i].0 + nx * half, path[i].1 + ny * half));
        b.push((path[i].0 - nx * half, path[i].1 - ny * half));
    }
    strip(p, f, &a, &b, fill.gamma_multiply(0.8));
    polyline(p, f, &a, s);
    polyline(p, f, &b, s);
    // the cut across the start
    segment(p, f, a[0], b[0], s);
}

/// Draws the icon of an extra solid or a CAD feature. Other kinds draw nothing here.
pub fn kind_more(p: &Painter, rect: Rect, k: Kind, ink: Color32, fill: Color32) {
    let f = frame(rect);
    let s = pen(rect, ink);
    match k {
        Kind::Prism3 => prism3(p, f, fill, s),
        Kind::PrismN => prism_n(p, f, fill, s),
        Kind::Octa => octa(p, f, fill, s),
        Kind::Tetra => tetra(p, f, fill, s),
        Kind::Pyramid => pyramid(p, f, fill, s),
        Kind::Capsule => capsule(p, f, fill, s),
        Kind::Hemi => hemi(p, f, fill, s),
        Kind::Tube => tube(p, f, fill, s),
        Kind::Wedge => wedge(p, f, fill, s),
        Kind::Star => star(p, f, fill, s),
        Kind::Revolve => revolve(p, f, fill, s),
        Kind::Loft => loft(p, f, fill, s),
        Kind::Curve => curve(p, f, fill, s),
        Kind::Sweep => sweep(p, f, fill, s),
        _ => {}
    }
}

/// Melt: two round blobs joined by a smooth neck.
pub fn melt(p: &Painter, rect: Rect, ink: Color32) {
    // What it does: the picked shapes flow into one smooth piece. So: two balls with a soft neck
    // between them, the outline of two blobs melting together.
    let side = rect.width().min(rect.height()) * 0.92;
    let c = rect.center();
    let (a, b) = (c + egui::vec2(-side * 0.2, side * 0.04), c + egui::vec2(side * 0.22, -side * 0.04));
    let (ra, rb) = (side * 0.23, side * 0.19);
    let field = |q: Pos2| ra * ra / (q - a).length_sq().max(1e-4) + rb * rb / (q - b).length_sq().max(1e-4);
    let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let outline: Vec<Pos2> = (0..72)
        .map(|i| {
            let ang = std::f32::consts::TAU * i as f32 / 72.0;
            let dir = egui::vec2(ang.cos(), ang.sin());
            // the edge is where the field drops to 1: step out, then halve the step back and forth
            let (mut lo, mut hi) = (0.0_f32, side);
            for _ in 0..18 {
                let m = (lo + hi) * 0.5;
                if field(mid + dir * m) > 1.0 { lo = m } else { hi = m }
            }
            mid + dir * lo
        })
        .collect();
    let mut mesh = Mesh::default();
    mesh.colored_vertex(mid, ink.gamma_multiply(0.35));
    for q in &outline {
        mesh.colored_vertex(*q, ink.gamma_multiply(0.35));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
    let mut closed = outline.clone();
    if let Some(first) = outline.first() {
        closed.push(*first);
    }
    p.add(Shape::line(closed, Stroke::new((side * 0.075).clamp(1.2, 2.2), ink)));
    // a highlight on each ball: two round shapes, now one piece
    p.circle_filled(a + egui::vec2(-ra * 0.35, -ra * 0.35), ra * 0.2, ink);
    p.circle_filled(b + egui::vec2(-rb * 0.35, -rb * 0.35), rb * 0.2, ink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    /// Runs `kind_more` for every kind and returns the shapes it produced.
    fn shapes_of(k: Kind) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            kind_more(&painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 40.0)), k, Color32::BLACK, Color32::LIGHT_BLUE);
        });
        out.textures_delta.clear();
        out.shapes
    }

    #[test]
    fn every_extra_and_cad_kind_draws_something() {
        for &k in Kind::MORE.iter().chain(Kind::CAD.iter()) {
            assert!(!shapes_of(k).is_empty(), "{k:?} drew nothing");
        }
        // the six main kinds are drawn by glyphs.rs, not here
        assert!(shapes_of(Kind::Sphere).is_empty());
    }
}
