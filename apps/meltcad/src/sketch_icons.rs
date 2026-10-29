//! Small line icons for the sketch card (planes, pens, curve style, snap), drawn in code so the
//! card can say things with pictures instead of words.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2};

use crate::sketch_ui::{Pen, PlaneSel};

fn size(r: Rect) -> f32 {
    crate::glyphs::frame(r).width()
}

fn at(r: Rect, u: f32, v: f32) -> Pos2 {
    let s = size(r);
    let o = r.center() - Vec2::splat(s / 2.0);
    pos2(o.x + s * u, o.y + s * v)
}

fn pen(r: Rect, ink: Color32) -> Stroke {
    crate::glyphs::pen(r, ink)
}

fn poly(p: &Painter, r: Rect, pts: &[(f32, f32)], fill: Option<Color32>, stroke: Stroke) {
    let v: Vec<Pos2> = pts.iter().map(|(u, w)| at(r, *u, *w)).collect();
    if let Some(f) = fill.filter(|c| c.a() > 0) {
        p.add(egui::Shape::convex_polygon(v.clone(), f, Stroke::NONE));
    }
    crate::glyphs::round_path(p, v, stroke, true);
}

fn curve(p: &Painter, r: Rect, pts: [(f32, f32); 4], stroke: Stroke) {
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(pts.map(|(u, v)| at(r, u, v)), false, Color32::TRANSPARENT, stroke));
}

fn dots(p: &Painter, r: Rect, pts: &[(f32, f32)], ink: Color32) {
    for (u, v) in pts {
        p.circle_filled(at(r, *u, *v), size(r) * 0.07, ink);
    }
}

/// A cube seen from a corner with the face of the chosen plane filled: top = floor, left = front,
/// right = side. Camera and the others draw their own picture.
pub fn plane(p: &Painter, r: Rect, pl: PlaneSel, ink: Color32) {
    let st = pen(r, ink.gamma_multiply(0.7));
    let fill = ink.gamma_multiply(0.55);
    let top = [(0.5, 0.08), (0.9, 0.29), (0.5, 0.5), (0.1, 0.29)];
    let left = [(0.1, 0.29), (0.5, 0.5), (0.5, 0.94), (0.1, 0.72)];
    let right = [(0.5, 0.5), (0.9, 0.29), (0.9, 0.72), (0.5, 0.94)];
    match pl {
        PlaneSel::Floor | PlaneSel::Front | PlaneSel::Side | PlaneSel::OnShape | PlaneSel::Height => {
            let face = match pl {
                PlaneSel::Front => 1,
                PlaneSel::Side => 2,
                _ => 0,
            };
            for (i, f) in [&top[..], &left[..], &right[..]].into_iter().enumerate() {
                poly(p, r, f, if i == face { Some(fill) } else { None }, st);
            }
            if pl == PlaneSel::Height {
                // an arrow up: the height is a number
                p.line_segment([at(r, 0.5, 0.5), at(r, 0.5, 0.0)], pen(r, ink));
                p.line_segment([at(r, 0.38, 0.14), at(r, 0.5, 0.0)], pen(r, ink));
                p.line_segment([at(r, 0.62, 0.14), at(r, 0.5, 0.0)], pen(r, ink));
            }
        }
        PlaneSel::Camera => {
            poly(p, r, &[(0.08, 0.28), (0.92, 0.28), (0.92, 0.82), (0.08, 0.82)], None, pen(r, ink));
            p.circle_stroke(at(r, 0.5, 0.55), size(r) * 0.17, pen(r, ink));
            poly(p, r, &[(0.3, 0.28), (0.38, 0.14), (0.62, 0.14), (0.7, 0.28)], None, pen(r, ink));
        }
        PlaneSel::Custom => {
            // a tilted plane
            poly(p, r, &[(0.1, 0.62), (0.62, 0.52), (0.9, 0.2), (0.38, 0.3)], Some(fill), pen(r, ink));
            p.line_segment([at(r, 0.1, 0.88), at(r, 0.9, 0.88)], st);
        }
    }
}

/// The drawing tools.
pub fn pen_tool(p: &Painter, r: Rect, t: Pen, ink: Color32) {
    let st = pen(r, ink);
    match t {
        Pen::Polygon => {
            let pts = [(0.5, 0.1), (0.9, 0.4), (0.76, 0.88), (0.24, 0.88), (0.1, 0.4)];
            poly(p, r, &pts, None, st);
            dots(p, r, &pts, ink);
        }
        Pen::Rect => {
            let pts = [(0.14, 0.24), (0.86, 0.24), (0.86, 0.76), (0.14, 0.76)];
            poly(p, r, &pts, None, st);
            dots(p, r, &[pts[0], pts[2]], ink);
        }
        Pen::Circle => {
            p.circle_stroke(at(r, 0.5, 0.5), size(r) * 0.38, st);
            p.line_segment([at(r, 0.5, 0.5), at(r, 0.88, 0.5)], pen(r, ink.gamma_multiply(0.7)));
            dots(p, r, &[(0.5, 0.5), (0.88, 0.5)], ink);
        }
        Pen::Spline => {
            curve(p, r, [(0.1, 0.5), (0.22, 0.12), (0.38, 0.12), (0.5, 0.5)], st);
            curve(p, r, [(0.5, 0.5), (0.62, 0.88), (0.78, 0.88), (0.9, 0.5)], st);
            dots(p, r, &[(0.1, 0.5), (0.5, 0.5), (0.9, 0.5)], ink);
        }
        Pen::Line => {
            p.line_segment([at(r, 0.14, 0.82), at(r, 0.86, 0.18)], st);
            dots(p, r, &[(0.14, 0.82), (0.86, 0.18)], ink);
        }
    }
}

/// Curve style: smooth wave or sharp zigzag.
pub fn smooth(p: &Painter, r: Rect, on: bool, ink: Color32) {
    let st = pen(r, ink);
    if on {
        curve(p, r, [(0.08, 0.5), (0.2, 0.12), (0.38, 0.12), (0.5, 0.5)], st);
        curve(p, r, [(0.5, 0.5), (0.62, 0.88), (0.8, 0.88), (0.92, 0.5)], st);
    } else {
        let pts: Vec<Pos2> = [(0.08, 0.7), (0.3, 0.3), (0.5, 0.7), (0.7, 0.3), (0.92, 0.7)].iter().map(|(u, v)| at(r, *u, *v)).collect();
        p.add(egui::Shape::line(pts, st));
    }
}

/// Angle snap: two lines and the angle between them.
pub fn snap(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.line_segment([at(r, 0.12, 0.82), at(r, 0.9, 0.82)], st);
    p.line_segment([at(r, 0.12, 0.82), at(r, 0.72, 0.2)], st);
    let pts: Vec<Pos2> = (0..=8)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_4 * i as f32 / 8.0;
            pos2(at(r, 0.12, 0.82).x + size(r) * 0.4 * a.cos(), at(r, 0.12, 0.82).y + size(r) * 0.4 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, pen(r, ink.gamma_multiply(0.7))));
}

/// Extrude: an outline pushed up into a prism.
pub fn extrude(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    poly(p, r, &[(0.18, 0.86), (0.62, 0.86), (0.62, 0.5), (0.18, 0.5)], None, st);
    p.line_segment([at(r, 0.18, 0.5), at(r, 0.38, 0.28)], st);
    p.line_segment([at(r, 0.62, 0.5), at(r, 0.82, 0.28)], st);
    p.line_segment([at(r, 0.62, 0.86), at(r, 0.82, 0.64)], st);
    p.line_segment([at(r, 0.38, 0.28), at(r, 0.82, 0.28)], st);
    p.line_segment([at(r, 0.82, 0.28), at(r, 0.82, 0.64)], st);
}

/// Solid mode: a cube with three shaded faces (top light, left mid, right dark), so it reads as a
/// filled body rather than an outline.
pub fn solid(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    let face = |a: u8| ink.gamma_multiply(a as f32 / 255.0);
    poly(p, r, &[(0.5, 0.1), (0.88, 0.3), (0.5, 0.5), (0.12, 0.3)], Some(face(210)), st);
    poly(p, r, &[(0.12, 0.3), (0.5, 0.5), (0.5, 0.92), (0.12, 0.7)], Some(face(110)), st);
    poly(p, r, &[(0.5, 0.5), (0.88, 0.3), (0.88, 0.7), (0.5, 0.92)], Some(face(50)), st);
}

/// Both sides: a plane line with arrows pointing up and down from it.
pub fn both_sides(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.line_segment([at(r, 0.1, 0.5), at(r, 0.9, 0.5)], Stroke::new(st.width * 0.7, ink.gamma_multiply(0.6)));
    for (y0, y1, w) in [(0.42, 0.1, 0.12), (0.58, 0.9, -0.12)] {
        p.line_segment([at(r, 0.5, y0), at(r, 0.5, y1)], st);
        p.line_segment([at(r, 0.5, y1), at(r, 0.5 - 0.16, y1 + w * 1.4)], st);
        p.line_segment([at(r, 0.5, y1), at(r, 0.5 + 0.16, y1 + w * 1.4)], st);
    }
}

/// Revolve: a profile beside an axis, with an arc turning around it.
pub fn revolve(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.line_segment([at(r, 0.3, 0.08), at(r, 0.3, 0.8)], Stroke::new(st.width * 0.7, ink.gamma_multiply(0.55)));
    p.add(egui::Shape::closed_line(vec![at(r, 0.3, 0.14), at(r, 0.56, 0.26), at(r, 0.46, 0.46), at(r, 0.58, 0.66), at(r, 0.3, 0.72)], st));
    let arc: Vec<Pos2> = (0..=10)
        .map(|i| {
            let a = std::f32::consts::PI * (0.05 + 0.9 * i as f32 / 10.0);
            at(r, 0.3 + 0.26 * a.cos(), 0.86 + 0.07 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(arc, st));
    p.line_segment([at(r, 0.04, 0.87), at(r, 0.13, 0.76)], st);
    p.line_segment([at(r, 0.04, 0.87), at(r, 0.18, 0.91)], st);
}

/// Loft: a wide outline below blending into a small one above.
pub fn loft(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.add(egui::Shape::closed_line(vec![at(r, 0.1, 0.82), at(r, 0.9, 0.82), at(r, 0.78, 0.96), at(r, 0.22, 0.96)], st));
    p.add(egui::Shape::closed_line(vec![at(r, 0.34, 0.1), at(r, 0.66, 0.1), at(r, 0.6, 0.2), at(r, 0.4, 0.2)], st));
    p.line_segment([at(r, 0.1, 0.82), at(r, 0.34, 0.1)], st);
    p.line_segment([at(r, 0.9, 0.82), at(r, 0.66, 0.1)], st);
}

/// Sweep: a small outline pulled along a bending path.
pub fn sweep(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.circle_stroke(at(r, 0.2, 0.8), size(r) * 0.13, st);
    curve(p, r, [(0.2, 0.8), (0.64, 0.8), (0.46, 0.18), (0.9, 0.18)], st);
    p.line_segment([at(r, 0.76, 0.06), at(r, 0.9, 0.18)], st);
    p.line_segment([at(r, 0.76, 0.3), at(r, 0.9, 0.18)], st);
}

/// Accept: a tick.
pub fn check(p: &Painter, r: Rect, ink: Color32) {
    let pts: Vec<Pos2> = [(0.16, 0.54), (0.4, 0.78), (0.86, 0.24)].iter().map(|(u, v)| at(r, *u, *v)).collect();
    p.add(egui::Shape::line(pts, Stroke::new((size(r) * 0.11).clamp(1.8, 3.0), ink)));
}

/// Discard: a cross.
pub fn cross(p: &Painter, r: Rect, ink: Color32) {
    let st = Stroke::new((size(r) * 0.11).clamp(1.8, 3.0), ink);
    p.line_segment([at(r, 0.22, 0.22), at(r, 0.78, 0.78)], st);
    p.line_segment([at(r, 0.78, 0.22), at(r, 0.22, 0.78)], st);
}

/// Height: an arrow pointing up with a base line.
pub fn height(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    p.line_segment([at(r, 0.5, 0.86), at(r, 0.5, 0.16)], st);
    p.line_segment([at(r, 0.28, 0.38), at(r, 0.5, 0.16)], st);
    p.line_segment([at(r, 0.72, 0.38), at(r, 0.5, 0.16)], st);
    p.line_segment([at(r, 0.24, 0.92), at(r, 0.76, 0.92)], pen(r, ink.gamma_multiply(0.7)));
}

/// Hole: a circle inside a square.
pub fn hole(p: &Painter, r: Rect, ink: Color32) {
    poly(p, r, &[(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)], None, pen(r, ink));
    p.circle_stroke(at(r, 0.5, 0.5), size(r) * 0.22, pen(r, ink));
}

/// Re-pick the plane: a crosshair on a square plane.
pub fn pick_plane(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    let dim = Stroke::new(st.width, ink.gamma_multiply(0.7));
    poly(p, r, &[(0.12, 0.62), (0.58, 0.5), (0.9, 0.22), (0.44, 0.34)], Some(ink.gamma_multiply(0.3)), dim);
    let c = at(r, 0.5, 0.56);
    p.circle_stroke(c, size(r) * 0.17, st);
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        let d = Vec2::new(dx, dy) * size(r);
        p.line_segment([c + d * 0.17, c + d * 0.36], st);
    }
}

/// Draw the top outline: a pencil over a small outline.
pub fn pencil_top(p: &Painter, r: Rect, ink: Color32) {
    let st = pen(r, ink);
    poly(p, r, &[(0.1, 0.62), (0.5, 0.62), (0.5, 0.92), (0.1, 0.92)], None, st);
    poly(p, r, &[(0.5, 0.52), (0.84, 0.16), (0.94, 0.26), (0.6, 0.62)], None, st);
    p.line_segment([at(r, 0.5, 0.52), at(r, 0.46, 0.66)], st);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sketch_icons_fit_wide_and_tall_buttons() {
        let icons: [fn(&Painter, Rect, Color32); 13] =
            [snap, extrude, solid, both_sides, revolve, loft, sweep, check, cross, height, hole, pick_plane, pencil_top];
        for dims in [[80.0, 24.0], [24.0, 80.0], [34.0, 34.0]] {
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(pos2(16.0, 16.0), Vec2::from(dims));
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                for f in icons {
                    f(ui.painter(), rect, Color32::WHITE);
                }
                for p in PlaneSel::ALL {
                    plane(ui.painter(), rect, p, Color32::WHITE);
                }
                for t in Pen::ALL {
                    pen_tool(ui.painter(), rect, t, Color32::WHITE);
                }
                smooth(ui.painter(), rect, true, Color32::WHITE);
                smooth(ui.painter(), rect, false, Color32::WHITE);
            });
            for shape in &out.shapes {
                let bounds = shape.shape.visual_bounding_rect();
                if bounds.is_positive() {
                    assert!(rect.expand(1.0).contains_rect(bounds), "icon escapes a {dims:?} button: {bounds:?}");
                }
            }
            out.textures_delta.clear();
        }
    }

    #[test]
    #[ignore]
    fn sketch_icon_sheet() {
        use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};
        type Draw = fn(&Painter, Rect, Color32);
        let items: &[(&str, Draw)] = &[
            ("Floor (XY)", |p, r, c| plane(p, r, PlaneSel::Floor, c)),
            ("Front (XZ)", |p, r, c| plane(p, r, PlaneSel::Front, c)),
            ("Side (YZ)", |p, r, c| plane(p, r, PlaneSel::Side, c)),
            ("Camera plane", |p, r, c| plane(p, r, PlaneSel::Camera, c)),
            ("Pick plane", pick_plane),
            ("Polygon", |p, r, c| pen_tool(p, r, Pen::Polygon, c)),
            ("Rectangle", |p, r, c| pen_tool(p, r, Pen::Rect, c)),
            ("Circle", |p, r, c| pen_tool(p, r, Pen::Circle, c)),
            ("Spline", |p, r, c| pen_tool(p, r, Pen::Spline, c)),
            ("Line", |p, r, c| pen_tool(p, r, Pen::Line, c)),
            ("Extrude", extrude),
            ("Revolve", revolve),
            ("Loft", loft),
            ("Sweep", sweep),
            ("Solid", solid),
            ("Smooth", |p, r, c| smooth(p, r, true, c)),
            ("Sharp", |p, r, c| smooth(p, r, false, c)),
            ("Angle snap", snap),
            ("Both sides", both_sides),
            ("Hole", hole),
            ("Confirm", check),
            ("Cancel", cross),
            ("Height", height),
            ("Top outline", pencil_top),
        ];
        let ctx = egui::Context::default();
        let ppp = 2.0;
        ctx.set_pixels_per_point(ppp);
        let size = egui::vec2(620.0, 450.0);
        let mut tex = Textures::new();
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            let p = ui.painter();
            p.rect_filled(ui.max_rect(), 0.0, crate::style::PANEL);
            for (i, (label, draw)) in items.iter().enumerate() {
                let origin = pos2(12.0 + (i % 5) as f32 * 122.0, 8.0 + (i / 5) as f32 * 88.0);
                draw(p, Rect::from_min_size(origin, Vec2::splat(46.0)), crate::style::TEXT);
                draw(p, Rect::from_min_size(origin + Vec2::new(62.0, 11.0), Vec2::splat(24.0)), crate::style::TEXT);
                p.text(origin + Vec2::new(49.0, 61.0), egui::Align2::CENTER_CENTER, *label, egui::FontId::proportional(12.0), crate::style::TEXT);
            }
        });
        apply_deltas(&mut tex, &out.textures_delta);
        out.textures_delta.clear();
        let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
        let prims = ctx.tessellate(out.shapes, ppp);
        let image = raster(&prims, &tex, w, h, ppp, [0.12; 3]);
        write_ppm(image, w, h, &std::env::var("MELT_SKETCH_ICONS").unwrap_or_else(|_| "/tmp/melt_sketch_icons.ppm".into()));
    }
}
