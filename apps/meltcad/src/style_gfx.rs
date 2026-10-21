//! Painter helpers for the soft 3D look: colour mixing, gradient meshes, layered shadows, glossy balls.
//!
//! Everything here is plain painting (no widgets); `style.rs` builds buttons, sliders and cards on top.
//! It is included from `style.rs` as `style::gfx`.

use egui::epaint::{Mesh, Shadow};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use std::cell::RefCell;

const PI: f32 = std::f32::consts::PI;

// ----- where shapes go -----

/// Anything that can take shapes: a `Painter` (drawn right away) or a `Buf` (kept to be placed later,
/// e.g. behind content whose size is not known yet).
pub trait Sink {
    fn put(&self, shape: Shape);
}

impl Sink for Painter {
    fn put(&self, shape: Shape) {
        self.add(shape);
    }
}

/// Collects shapes so they can be set into a reserved slot (`Painter::set`) later.
#[derive(Default)]
pub struct Buf(RefCell<Vec<Shape>>);

impl Sink for Buf {
    fn put(&self, shape: Shape) {
        self.0.borrow_mut().push(shape);
    }
}

impl Buf {
    pub fn into_shape(self) -> Shape {
        Shape::Vec(self.0.into_inner())
    }
}

// ----- numbers and colours -----

/// Clamps to 0..1 and maps NaN to 0.
pub fn unit(t: f32) -> f32 {
    if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 }
}

/// Fast start, soft landing (cubic ease-out). Input and output are 0..1.
#[allow(dead_code)]
pub fn ease_out(t: f32) -> f32 {
    let m = 1.0 - unit(t);
    1.0 - m * m * m
}

/// Smooth S curve between 0 and 1.
pub fn smooth(t: f32) -> f32 {
    let t = unit(t);
    t * t * (3.0 - 2.0 * t)
}

/// Mixes two colours (in gamma space, like most UI work); `t` is clamped.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, unit(t))
}

/// A colour moved towards white by `t`, alpha kept.
pub fn lighten(c: Color32, t: f32) -> Color32 {
    let a = c.a();
    mix(c, Color32::from_rgba_premultiplied(a, a, a, a), t)
}

/// A colour moved towards black by `t`, alpha kept.
pub fn darken(c: Color32, t: f32) -> Color32 {
    mix(c, Color32::from_rgba_premultiplied(0, 0, 0, c.a()), t)
}

/// Brightness scale: below 1 darkens, above 1 lightens (1.3 is about a third of the way to white).
pub fn lit(c: Color32, k: f32) -> Color32 {
    if k >= 1.0 { lighten(c, (k - 1.0).min(1.0)) } else { darken(c, 1.0 - k.max(0.0)) }
}

/// Piecewise linear colour ramp. `stops` are `(position, colour)` sorted by position.
pub fn ramp(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let Some(&(p0, c0)) = stops.first() else { return Color32::TRANSPARENT };
    if !t.is_finite() || t <= p0 {
        return c0;
    }
    for w in stops.windows(2) {
        let (pa, ca) = w[0];
        let (pb, cb) = w[1];
        if t <= pb {
            let span = (pb - pa).max(1e-6);
            return mix(ca, cb, (t - pa) / span);
        }
    }
    stops.last().map_or(c0, |s| s.1)
}

/// `a` towards `b` by `t` for plain numbers.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * unit(t)
}

// ----- shapes -----

/// Points of a rounded rectangle, clockwise from the top-left corner's start. `seg` points per corner.
pub fn round_rect_points(rect: Rect, radius: f32, seg: usize) -> Vec<Pos2> {
    let r = radius.max(0.0).min(rect.width().min(rect.height()) * 0.5);
    let seg = seg.max(1);
    let mut out = Vec::with_capacity(4 * (seg + 1));
    // (corner centre, start angle)
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), PI),
        (pos2(rect.right() - r, rect.top() + r), PI * 1.5),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), PI * 0.5),
    ];
    for (c, a0) in corners {
        for i in 0..=seg {
            let a = a0 + PI * 0.5 * i as f32 / seg as f32;
            out.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    out
}

/// Fills a convex polygon with a colour that depends on the position (exact when `f` is linear).
pub fn fan(out: &impl Sink, pts: &[Pos2], f: impl Fn(Pos2) -> Color32) {
    if pts.len() < 3 {
        return;
    }
    let n = pts.len() as f32;
    let centre = pts.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / n;
    let centre = pos2(centre.x, centre.y);
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(pts.len() + 1);
    mesh.colored_vertex(centre, f(centre));
    for p in pts {
        mesh.colored_vertex(*p, f(*p));
    }
    let m = pts.len() as u32;
    for i in 0..m {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % m);
    }
    out.put(Shape::mesh(mesh));
}

/// A convex polygon with one colour per corner.
#[allow(dead_code)]
pub fn shaded_poly(out: &impl Sink, pts: &[Pos2], cols: &[Color32]) {
    if pts.len() < 3 || cols.len() != pts.len() {
        return;
    }
    let n = pts.len() as f32;
    let centre = pts.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / n;
    let centre = pos2(centre.x, centre.y);
    let avg = |get: fn(Color32) -> u8| cols.iter().map(|c| get(*c) as u32).sum::<u32>() / cols.len() as u32;
    let mid = Color32::from_rgba_premultiplied(avg(|c| c.r()) as u8, avg(|c| c.g()) as u8, avg(|c| c.b()) as u8, avg(|c| c.a()) as u8);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, mid);
    for (p, c) in pts.iter().zip(cols) {
        mesh.colored_vertex(*p, *c);
    }
    let m = pts.len() as u32;
    for i in 0..m {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % m);
    }
    out.put(Shape::mesh(mesh));
}

/// A rounded rectangle with a vertical gradient (`top` to `bottom`).
pub fn vgrad(out: &impl Sink, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    // an anti-aliased base so the mesh edge (which is not smoothed) is hidden
    let opaque = top.a() == 255 && bottom.a() == 255;
    let inner = if opaque {
        out.put(Shape::rect_filled(rect, radius, mix(top, bottom, 0.5)));
        rect.shrink(0.6)
    } else {
        rect
    };
    let pts = round_rect_points(inner, (radius - if opaque { 0.6 } else { 0.0 }).max(0.0), 5);
    let (y0, h) = (inner.top(), inner.height().max(1.0));
    fan(out, &pts, |p| mix(top, bottom, (p.y - y0) / h));
}

/// A glossy ball: rings from the light spot (`light`, in radii from the centre) out to the edge.
/// `stops` run from the light spot (0.0) to the rim (1.0).
pub fn ball(out: &impl Sink, centre: Pos2, radius: f32, light: Vec2, stops: &[(f32, Color32)]) {
    let r = radius.max(0.5);
    let rim = ramp(stops, 1.0);
    out.put(Shape::circle_filled(centre, r, rim));
    let inner_r = r - 0.4;
    let rings = if r < 10.0 { 5 } else { 8 };
    let seg = if r < 10.0 { 20 } else { 28 };
    // keep the light spot inside the disc
    let l = if light.length() > 0.9 { light * (0.9 / light.length()) } else { light };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre + l * inner_r, ramp(stops, 0.0));
    for k in 1..=rings {
        let s = k as f32 / rings as f32;
        let c = centre + l * inner_r * (1.0 - s);
        let col = ramp(stops, s);
        for i in 0..seg {
            let a = 2.0 * PI * i as f32 / seg as f32;
            mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * inner_r * s, col);
        }
    }
    let seg_u = seg as u32;
    for i in 0..seg_u {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % seg_u);
    }
    for k in 1..rings as u32 {
        let a0 = 1 + (k - 1) * seg_u;
        let a1 = 1 + k * seg_u;
        for i in 0..seg_u {
            let j = (i + 1) % seg_u;
            mesh.add_triangle(a0 + i, a1 + i, a1 + j);
            mesh.add_triangle(a0 + i, a1 + j, a0 + j);
        }
    }
    out.put(Shape::mesh(mesh));
}

/// Layers of soft shadow under `rect`: `(offset_y, blur, spread, alpha)` per layer.
pub fn shadow(out: &impl Sink, rect: Rect, radius: f32, layers: &[(i8, u8, u8, u8)]) {
    for &(dy, blur, spread, alpha) in layers {
        if alpha == 0 {
            continue;
        }
        let s = Shadow { offset: [0, dy], blur, spread, color: Color32::from_black_alpha(alpha) };
        out.put(s.as_shape(rect, radius).into());
    }
}

/// A coloured glow around `rect`.
#[allow(dead_code)]
pub fn glow(out: &impl Sink, rect: Rect, radius: f32, color: Color32, blur: u8) {
    if color.a() == 0 {
        return;
    }
    let s = Shadow { offset: [0, 0], blur, spread: 1, color };
    out.put(s.as_shape(rect, radius).into());
}

/// A 1px bright line just inside the top edge (the "lip" of a raised surface).
#[allow(dead_code)]
pub fn top_lip(out: &impl Sink, rect: Rect, radius: f32, color: Color32) {
    let inset = radius.min(rect.width() * 0.5) * 0.85 + 1.0;
    if rect.width() <= inset * 2.0 {
        return;
    }
    let y = rect.top() + 1.5;
    out.put(Shape::line_segment([pos2(rect.left() + inset, y), pos2(rect.right() - inset, y)], Stroke::new(1.0, color)));
}

/// A 1px dark line just inside the bottom edge (the shaded underside).
#[allow(dead_code)]
pub fn bottom_shade(out: &impl Sink, rect: Rect, radius: f32, color: Color32) {
    let inset = radius.min(rect.width() * 0.5) * 0.85 + 1.0;
    if rect.width() <= inset * 2.0 {
        return;
    }
    let y = rect.bottom() - 1.5;
    out.put(Shape::line_segment([pos2(rect.left() + inset, y), pos2(rect.right() - inset, y)], Stroke::new(1.0, color)));
}

/// Six dots in two columns: the "you can drag this" grip.
pub fn grip(out: &impl Sink, centre: Pos2, color: Color32) {
    for dx in [-2.0, 2.0] {
        for dy in [-4.0, 0.0, 4.0] {
            out.put(Shape::circle_filled(centre + vec2(dx, dy), 1.1, color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_easing_stay_in_range() {
        assert_eq!(unit(f32::NAN), 0.0);
        assert_eq!(unit(-3.0), 0.0);
        assert_eq!(unit(9.0), 1.0);
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            assert!((0.0..=1.0).contains(&ease_out(t)));
            assert!((0.0..=1.0).contains(&smooth(t)));
        }
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5, "ease-out starts fast");
        assert!(smooth(0.25) < 0.25 && smooth(0.75) > 0.75);
    }

    #[test]
    fn mixing_hits_the_ends_and_middle() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(200, 100, 50);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 5.0), b);
        assert_eq!(mix(a, b, f32::NAN), a);
        let m = mix(a, b, 0.5);
        assert!((m.r() as i32 - 100).abs() <= 2 && (m.g() as i32 - 50).abs() <= 2);
    }

    #[test]
    fn lit_goes_both_ways_and_keeps_alpha() {
        let c = Color32::from_rgb(100, 100, 100);
        assert!(lit(c, 1.3).r() > 100);
        assert!(lit(c, 0.5).r() < 100);
        assert_eq!(lit(c, 1.0), c);
        assert_eq!(lit(c, 2.0).r(), 255);
        assert_eq!(lighten(c, 0.5).a(), 255);
        assert_eq!(darken(c, 1.0).r(), 0);
        let soft = Color32::from_rgba_unmultiplied(100, 100, 100, 80);
        assert_eq!(lighten(soft, 0.3).a(), 80);
    }

    #[test]
    fn ramp_follows_the_stops() {
        let stops = [(0.0, Color32::from_rgb(0, 0, 0)), (0.5, Color32::from_rgb(100, 100, 100)), (1.0, Color32::from_rgb(100, 200, 100))];
        assert_eq!(ramp(&stops, -1.0), stops[0].1);
        assert_eq!(ramp(&stops, 0.5), stops[1].1);
        assert_eq!(ramp(&stops, 2.0), stops[2].1);
        assert_eq!(ramp(&stops, f32::NAN), stops[0].1);
        let m = ramp(&stops, 0.75);
        assert!((m.g() as i32 - 150).abs() <= 2);
        assert_eq!(ramp(&[], 0.3), Color32::TRANSPARENT);
        assert_eq!(ramp(&stops[..1], 0.9), stops[0].1);
    }

    #[test]
    fn rounded_rect_points_stay_inside() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(40.0, 20.0));
        for r in [0.0, 4.0, 10.0, 99.0] {
            let pts = round_rect_points(rect, r, 5);
            assert_eq!(pts.len(), 24);
            for p in pts {
                assert!(rect.expand(0.01).contains(p), "{p:?} with radius {r}");
            }
        }
    }

    #[test]
    fn painting_helpers_do_not_panic() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(60.0, 30.0));
            vgrad(&p, rect, 8.0, Color32::WHITE, Color32::BLACK);
            vgrad(&p, rect, 8.0, Color32::from_white_alpha(40), Color32::from_black_alpha(40));
            vgrad(&p, Rect::from_min_size(pos2(0.0, 0.0), vec2(0.2, 0.2)), 3.0, Color32::WHITE, Color32::BLACK);
            ball(&p, pos2(50.0, 50.0), 12.0, vec2(-0.4, -0.4), &[(0.0, Color32::WHITE), (1.0, Color32::BLUE)]);
            ball(&p, pos2(50.0, 50.0), 0.0, vec2(5.0, 5.0), &[]);
            shadow(&p, rect, 8.0, &[(2, 6, 0, 60), (6, 16, 0, 0)]);
            glow(&p, rect, 8.0, Color32::from_rgba_unmultiplied(150, 120, 255, 90), 10);
            top_lip(&p, rect, 8.0, Color32::WHITE);
            bottom_shade(&p, Rect::from_min_size(pos2(0.0, 0.0), vec2(3.0, 3.0)), 8.0, Color32::BLACK);
            grip(&p, pos2(10.0, 10.0), Color32::GRAY);
            shaded_poly(&p, &[pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(5.0, 9.0)], &[Color32::RED, Color32::GREEN, Color32::BLUE]);
            shaded_poly(&p, &[pos2(0.0, 0.0)], &[Color32::RED]);
            fan(&p, &[], |_| Color32::RED);
        });
        out.textures_delta.clear();
    }
}
