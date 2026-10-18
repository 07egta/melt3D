//! Small line icons for the sketch card (planes, pens, curve style, snap), drawn in code so the
//! card can say things with pictures instead of words.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2};

use crate::sketch_ui::{Pen, PlaneSel};

fn at(r: Rect, u: f32, v: f32) -> Pos2 {
    let s = r.width().min(r.height());
    let o = r.center() - Vec2::splat(s / 2.0);
    pos2(o.x + s * u, o.y + s * v)
}

fn pen(r: Rect, ink: Color32) -> Stroke {
    Stroke::new((r.width() * 0.075).clamp(1.3, 2.2), ink)
}

fn poly(p: &Painter, r: Rect, pts: &[(f32, f32)], fill: Option<Color32>, stroke: Stroke) {
    let v: Vec<Pos2> = pts.iter().map(|(u, w)| at(r, *u, *w)).collect();
    match fill {
        Some(f) => {
            p.add(egui::Shape::convex_polygon(v, f, stroke));
        }
        None => {
            p.add(egui::Shape::closed_line(v, stroke));
        }
    }
}

fn dots(p: &Painter, r: Rect, pts: &[(f32, f32)], ink: Color32) {
    for (u, v) in pts {
        p.circle_filled(at(r, *u, *v), r.width() * 0.07, ink);
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
            }
        }
        PlaneSel::Camera => {
            poly(p, r, &[(0.08, 0.28), (0.92, 0.28), (0.92, 0.82), (0.08, 0.82)], None, pen(r, ink));
            p.circle_stroke(at(r, 0.5, 0.55), r.width() * 0.17, pen(r, ink));
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
            p.circle_stroke(at(r, 0.5, 0.5), r.width() * 0.38, st);
            p.line_segment([at(r, 0.5, 0.5), at(r, 0.88, 0.5)], pen(r, ink.gamma_multiply(0.7)));
            dots(p, r, &[(0.5, 0.5), (0.88, 0.5)], ink);
        }
        Pen::Spline => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    at(r, 0.1 + 0.8 * t, 0.5 - 0.3 * (t * std::f32::consts::TAU).sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, st));
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
        let pts: Vec<Pos2> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                at(r, 0.08 + 0.84 * t, 0.5 - 0.28 * (t * 2.0 * std::f32::consts::TAU).sin())
            })
            .collect();
        p.add(egui::Shape::line(pts, st));
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
            let a = -std::f32::consts::FRAC_PI_4 * i as f32 / 8.0 * 1.0 - 0.0;
            pos2(at(r, 0.12, 0.82).x + r.width() * 0.4 * a.cos(), at(r, 0.12, 0.82).y + r.width() * 0.4 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, pen(r, ink.gamma_multiply(0.7))));
}
