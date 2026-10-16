//! Distance functions of the CAD features (Revolve, Loft, Sweep), in the shape's own space.
//! The shader copy is `shader_cad.wgsl`; keep the two in step. (The Curve tube is in `curve.rs`.)

use crate::model::Kind;
use crate::sdf::{Prepared, V3, cross, dot, len, norm, polygon, sub};

/// A loft's two profiles move apart as you go up, so the blended distance is a little too
/// generous. It is shortened by `1 / (1 + |d_top - d_bottom| / height)`, the exact gradient bound.
pub fn loft_safety(d_bottom: f32, d_top: f32, h: f32) -> f32 {
    1.0 / (1.0 + (d_top - d_bottom).abs() / h.max(1e-3))
}

/// How much to shorten a sweep's distance: bends and twists of the frame make it overshoot.
pub const SWEEP_SAFETY: f32 = 0.7;

/// Distance from `q` (local, unscaled) to the shape. Negative inside.
pub fn distance(p: &Prepared, q: V3) -> f32 {
    match p.kind {
        Kind::Revolve => revolve(p, q),
        Kind::Loft => loft(p, q),
        Kind::Sweep => sweep(p, q),
        _ => len(q) - p.size[0],
    }
}

/// Distance from `pt` to the polygon `v`, ignoring edges that lie on the Z axis (x = 0): those are
/// not a surface once the profile is turned about the axis. The sign counts every edge.
pub fn polygon_off_axis(v: &[[f32; 2]], pt: [f32; 2]) -> f32 {
    let n = v.len();
    if n < 3 {
        return 1e9;
    }
    let mut d = 1e18_f32;
    let mut sgn = 1.0_f32;
    let mut j = n - 1;
    for i in 0..n {
        let e = [v[j][0] - v[i][0], v[j][1] - v[i][1]];
        let w = [pt[0] - v[i][0], pt[1] - v[i][1]];
        if !(v[i][0] <= 1e-4 && v[j][0] <= 1e-4) {
            let t = ((w[0] * e[0] + w[1] * e[1]) / (e[0] * e[0] + e[1] * e[1]).max(1e-12)).clamp(0.0, 1.0);
            let b = [w[0] - e[0] * t, w[1] - e[1] * t];
            d = d.min(b[0] * b[0] + b[1] * b[1]);
        }
        let c1 = pt[1] >= v[i][1];
        let c2 = pt[1] < v[j][1];
        let c3 = e[0] * w[1] > e[1] * w[0];
        if (c1 && c2 && c3) || (!c1 && !c2 && !c3) {
            sgn = -sgn;
        }
        j = i;
    }
    sgn * d.sqrt()
}

/// Signed distance to the infinite wedge of angles 0..`deg` about the Z axis, seen from above.
pub fn wedge(x: f32, y: f32, deg: f32) -> f32 {
    let (s, c) = deg.to_radians().sin_cos();
    let ray = |ex: f32, ey: f32| {
        let along = x * ex + y * ey;
        if along > 0.0 { (ex * y - ey * x).abs() } else { x.hypot(y) }
    };
    let d = ray(1.0, 0.0).min(ray(c, s));
    let a = y >= 0.0; // left of the first edge
    let b = c * y - s * x <= 0.0; // right of the second edge: cross(p, e1) >= 0
    let inside = if deg <= 180.0 { a && b } else { a || b };
    if inside { -d } else { d }
}

fn revolve(p: &Prepared, q: V3) -> f32 {
    let d = polygon_off_axis(&p.outline, [q[0].hypot(q[1]), q[2]]);
    let a = if p.params[0].is_finite() { p.params[0].clamp(1.0, 360.0) } else { 360.0 };
    if a < 359.99 { d.max(wedge(q[0], q[1], a)) } else { d }
}

fn loft(p: &Prepared, q: V3) -> f32 {
    let h = p.size[0].max(1e-3);
    let xy = [q[0], q[1]];
    let (d1, d2) = (polygon(&p.outline, xy), polygon(&p.outline2, xy));
    let t = (q[2] / h + 0.5).clamp(0.0, 1.0);
    let dm = d1 + (d2 - d1) * t;
    let w = [dm, q[2].abs() - h * 0.5];
    let d = w[0].max(w[1]).min(0.0) + w[0].max(0.0).hypot(w[1].max(0.0));
    d * loft_safety(d1, d2, h)
}

fn sweep(p: &Prepared, q: V3) -> f32 {
    let n = p.path.len();
    if n < 2 || p.outline.len() < 3 {
        return 1e5;
    }
    // the nearest point on the polyline
    let mut best = (f32::MAX, 0usize, 0.0_f32);
    for i in 0..n - 1 {
        let (a, b) = (p.path[i], p.path[i + 1]);
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ap = sub(q, [a[0], a[1], a[2]]);
        let t = (dot(ap, ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
        let c = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
        let d2 = dot(c, c);
        if d2 < best.0 {
            best = (d2, i, t);
        }
    }
    let (_, i, t) = best;
    let (a, b) = (p.path[i], p.path[i + 1]);
    let tan = norm([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    let up = if tan[2].abs() > 0.99 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let nrm = norm(cross(up, tan));
    let bin = cross(tan, nrm);
    let c = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    let r = (a[3] + (b[3] - a[3]) * t).abs().max(1e-3);
    let d = sub(q, c);
    let d2 = polygon(&p.outline, [dot(d, nrm) / r, dot(d, bin) / r]) * r;
    // flat end caps: how far past the first / last node along the path
    let mut axial = -1e5_f32;
    if i == 0 && t <= 0.0 {
        axial = -dot(sub(q, [a[0], a[1], a[2]]), tan);
    }
    if i == n - 2 && t >= 1.0 {
        axial = axial.max(dot(sub(q, [b[0], b[1], b[2]]), tan));
    }
    let w = [d2, axial];
    (w[0].max(w[1]).min(0.0) + w[0].max(0.0).hypot(w[1].max(0.0))) * SWEEP_SAFETY
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Material, Shape};

    fn shape(k: Kind) -> Shape {
        Shape::new(k, [0.0; 3], Material::default())
    }
    fn dist(s: &Shape, p: V3) -> f32 {
        Prepared::new(s).distance(p)
    }
    fn probes() -> Vec<V3> {
        let mut out = Vec::new();
        let mut seed = 12345_u32;
        let mut r = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0 * 90.0 - 45.0
        };
        for _ in 0..400 {
            out.push([r(), r(), r()]);
        }
        out
    }

    #[test]
    fn a_revolved_rectangle_is_a_cylinder() {
        let mut rev = shape(Kind::Revolve);
        rev.outline = vec![[0.0, -20.0], [15.0, -20.0], [15.0, 20.0], [0.0, 20.0]];
        let mut cyl = shape(Kind::Cylinder);
        cyl.size = [15.0, 40.0, 1.0];
        for p in probes() {
            let (a, b) = (dist(&rev, p), dist(&cyl, p));
            assert!((a - b).abs() < 1e-3, "{p:?}: {a} vs {b}");
        }
        // points left of the axis in the profile are pushed onto it
        rev.outline = vec![[-9.0, -20.0], [15.0, -20.0], [15.0, 20.0], [-9.0, 20.0]];
        assert!((dist(&rev, [25.0, 0.0, 0.0]) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn a_partial_revolve_is_cut_away_by_a_wedge() {
        let mut rev = shape(Kind::Revolve);
        rev.outline = vec![[0.0, -20.0], [15.0, -20.0], [15.0, 20.0], [0.0, 20.0]];
        rev.params[0] = 180.0;
        assert!((dist(&rev, [0.0, 10.0, 0.0]) + 5.0).abs() < 1e-3, "inside the kept half");
        assert!((dist(&rev, [0.0, -10.0, 0.0]) - 10.0).abs() < 1e-3, "in the cut-away half");
        rev.params[0] = 90.0;
        assert!((dist(&rev, [-5.0, 5.0, 0.0]) - 5.0).abs() < 1e-3);
        assert!(dist(&rev, [5.0, 5.0, 0.0]) < 0.0);
        rev.params[0] = 270.0;
        assert!(dist(&rev, [-5.0, 5.0, 0.0]) < 0.0 && dist(&rev, [5.0, -5.0, 0.0]) > 0.0);
        for a in [0.0, f32::NAN, 1e9, -5.0] {
            rev.params[0] = a;
            assert!(probes().iter().all(|p| dist(&rev, *p).is_finite()));
        }
    }

    #[test]
    fn a_loft_of_one_profile_is_an_extrusion() {
        let mut loft = shape(Kind::Loft);
        loft.size[0] = 40.0;
        loft.outline2 = loft.outline.clone();
        let mut ext = shape(Kind::Extrude);
        ext.size[0] = 40.0;
        for p in probes() {
            let (a, b) = (dist(&loft, p), dist(&ext, p));
            assert!((a - b).abs() < 1e-3, "{p:?}: {a} vs {b}");
        }
    }

    #[test]
    fn a_loft_morphs_between_its_two_profiles() {
        let mut loft = shape(Kind::Loft); // square 30 at the bottom, triangle on top
        loft.size[0] = 40.0;
        assert!(dist(&loft, [0.0, 0.0, 0.0]) < 0.0);
        assert!(dist(&loft, [0.0, 0.0, 30.0]) > 5.0 && dist(&loft, [40.0, 0.0, 0.0]) > 5.0);
        // near the bottom the square's corner is still there, near the top it is gone
        assert!(dist(&loft, [14.0, -14.0, -19.0]) < 0.0);
        assert!(dist(&loft, [14.0, -14.0, 19.0]) > 0.0);
    }

    #[test]
    fn a_straight_sweep_is_a_scaled_box() {
        let mut sw = shape(Kind::Sweep);
        sw.outline = vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]];
        sw.path = vec![[0.0, 0.0, -20.0, 1.0], [0.0, 0.0, 20.0, 1.0]];
        assert!((dist(&sw, [10.0, 0.0, 0.0]) - 5.0 * SWEEP_SAFETY).abs() < 1e-3);
        assert!((dist(&sw, [0.0, 0.0, 30.0]) - 10.0 * SWEEP_SAFETY).abs() < 1e-3, "flat end cap");
        assert!(dist(&sw, [0.0, 0.0, 0.0]) < 0.0);
        sw.path = vec![[0.0, 0.0, -20.0, 2.0], [0.0, 0.0, 20.0, 2.0]];
        assert!((dist(&sw, [15.0, 0.0, 0.0]) - 5.0 * SWEEP_SAFETY).abs() < 1e-3, "radius scales the profile");
        // a bent default sweep is hollow nowhere on its path and empty far away
        let d = shape(Kind::Sweep);
        assert!(dist(&d, [0.0, 0.0, 25.0]) < 0.0 && dist(&d, [0.0, 80.0, 0.0]) > 10.0);
    }

    #[test]
    fn the_cad_fields_never_overshoot_and_stay_finite() {
        for kind in [Kind::Revolve, Kind::Loft, Kind::Sweep] {
            let mut s = shape(kind);
            s.params[0] = 250.0;
            let pts = probes();
            let ds: Vec<f32> = pts.iter().map(|p| dist(&s, *p)).collect();
            assert!(ds.iter().all(|d| d.is_finite()), "{kind:?}");
            for i in 0..pts.len() - 1 {
                let step = len(sub(pts[i], pts[i + 1]));
                assert!((ds[i] - ds[i + 1]).abs() <= step * 1.01 + 1e-3, "{kind:?}: {} vs {} over {step}", ds[i], ds[i + 1]);
            }
        }
    }
}
