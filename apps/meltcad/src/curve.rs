//! The curve (kind 18): a smooth tube through the nodes of `Shape::path` (x, y, z, radius), in the
//! shape's own space, like Womp's line tool. The shader copy is `shader_curve.wgsl`; keep the two
//! in step.
//!
//! The nodes are first resampled with a (centripetal) Catmull-Rom spline into at most `MAX_POINTS` points (the
//! GPU and the CPU both measure the *resampled* points, so they always agree). The tube is then the
//! union of round cones (a capsule whose two ends have different radii) between neighbours.
//!
//! Extra numbers (`Shape::params`): 0 = closed loop (0 / 1), 1 = smoothness 0..1 (0 = straight
//! pieces with round joints), 2 = flat ends (0 round / 1 flat), 3 = thickness multiplier (all radii).

use crate::sdf::{Prepared, V3, dot, len, mul, norm, sub};

/// Most points one curve gives to the GPU.
pub const MAX_POINTS: usize = 96;
const MAX_STEPS: usize = 8;
const COORD_LIMIT: f32 = 1.0e4;
const R_MIN: f32 = 0.01;
const R_MAX: f32 = 2000.0;

/// The settings read from `Shape::params`: (closed, smoothness, flat ends, thickness multiplier).
pub fn settings(params: &[f32; 4]) -> (bool, f32, bool, f32) {
    let ok = |v: f32| if v.is_finite() { v } else { 0.0 };
    let thick = if params[3].is_finite() && params[3] > 0.0 { params[3].clamp(0.05, 20.0) } else { 1.0 };
    (ok(params[0]) > 0.5, ok(params[1]).clamp(0.0, 1.0), ok(params[2]) > 0.5, thick)
}

/// The points the renderer and the mesher measure for a curve shape (thickness applied).
pub fn build(path: &[[f32; 4]], params: &[f32; 4]) -> Vec<[f32; 4]> {
    build_max(path, params, MAX_POINTS)
}

/// Like `build`, with a smaller cap (when the GPU's room for points is running out).
pub fn build_max(path: &[[f32; 4]], params: &[f32; 4], max_points: usize) -> Vec<[f32; 4]> {
    let (closed, smooth, _, thick) = settings(params);
    let mut pts = resample_max(path, smooth, closed, max_points);
    if (thick - 1.0).abs() > 1e-6 {
        for p in &mut pts {
            p[3] = (p[3] * thick).clamp(R_MIN, R_MAX);
        }
    }
    pts
}

/// Finite, clamped copies of the nodes (a hostile file never gets past here).
fn clean(path: &[[f32; 4]]) -> Vec<[f32; 4]> {
    path.iter()
        .filter(|p| p.iter().all(|v| v.is_finite()))
        .map(|p| [p[0].clamp(-COORD_LIMIT, COORD_LIMIT), p[1].clamp(-COORD_LIMIT, COORD_LIMIT), p[2].clamp(-COORD_LIMIT, COORD_LIMIT), p[3].clamp(R_MIN, R_MAX)])
        .collect()
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// Centripetal Catmull-Rom between `p1` and `p2` (Barry-Goldman form). Unlike the uniform kind it
/// does not overshoot or loop when neighbouring nodes are very unevenly spaced, which is what a
/// traced pipe (long straight spans next to tight turns) looks like. It passes through the nodes.
fn catmull(p0: [f32; 4], p1: [f32; 4], p2: [f32; 4], p3: [f32; 4], t: f32) -> [f32; 4] {
    // knot spacing: the square root of the distance (floored so coincident nodes stay finite)
    let gap = |a: [f32; 4], b: [f32; 4]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt().sqrt().max(1.0e-3);
    let t0 = 0.0_f32;
    let t1 = t0 + gap(p0, p1);
    let t2 = t1 + gap(p1, p2);
    let t3 = t2 + gap(p2, p3);
    let tt = t1 + (t2 - t1) * t;
    let mix = |a: [f32; 4], b: [f32; 4], ta: f32, tb: f32| -> [f32; 4] {
        let w = (tt - ta) / (tb - ta);
        lerp4(a, b, w)
    };
    let a1 = mix(p0, p1, t0, t1);
    let a2 = mix(p1, p2, t1, t2);
    let a3 = mix(p2, p3, t2, t3);
    let b1 = mix(a1, a2, t0, t2);
    let b2 = mix(a2, a3, t1, t3);
    mix(b1, b2, t1, t2)
}

/// Node `i` of the curve, where the ends of an open curve continue straight (so a two-node curve
/// stays a straight line) and a closed curve wraps around.
fn node(nodes: &[[f32; 4]], i: isize, closed: bool) -> [f32; 4] {
    let n = nodes.len() as isize;
    let at = |k: isize| nodes.get(k.clamp(0, n - 1) as usize).copied().unwrap_or([0.0; 4]);
    if closed {
        return at(i.rem_euclid(n));
    }
    if i < 0 {
        let (a, b) = (at(0), at(1));
        [2.0 * a[0] - b[0], 2.0 * a[1] - b[1], 2.0 * a[2] - b[2], 2.0 * a[3] - b[3]]
    } else if i >= n {
        let (a, b) = (at(n - 1), at(n - 2));
        [2.0 * a[0] - b[0], 2.0 * a[1] - b[1], 2.0 * a[2] - b[2], 2.0 * a[3] - b[3]]
    } else {
        at(i)
    }
}

/// The point (x, y, z, radius) at `t` (0..1) along segment `seg` of the nodes, with the given
/// smoothness. Segment `n - 1` of a closed curve runs from the last node back to the first.
pub fn segment_point(nodes: &[[f32; 4]], seg: usize, t: f32, smooth: f32, closed: bool) -> [f32; 4] {
    let nodes = &clean(nodes)[..];
    let n = nodes.len();
    if n == 0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let closed = closed && n >= 2;
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let s = seg as isize;
    let (p1, p2) = (node(nodes, s, closed), node(nodes, s + 1, closed));
    let lin = lerp4(p1, p2, t);
    let k = if smooth.is_finite() { smooth.clamp(0.0, 1.0) } else { 0.0 };
    if k <= 1e-4 || n < 2 {
        return lin;
    }
    let cr = catmull(node(nodes, s - 1, closed), p1, p2, node(nodes, s + 2, closed), t);
    let mut out = lerp4(lin, cr, k);
    out[3] = out[3].clamp(R_MIN, R_MAX);
    out
}

/// Turn the nodes into at most `MAX_POINTS` points along the smooth curve (a loop ends on its
/// first point again). Hostile input (NaN, huge numbers, thousands of nodes) gives a short, finite
/// list; no nodes gives none; one node gives a (zero-length) pair, which draws as a ball.
pub fn resample(path: &[[f32; 4]], smooth: f32, closed: bool) -> Vec<[f32; 4]> {
    resample_max(path, smooth, closed, MAX_POINTS)
}

pub fn resample_max(path: &[[f32; 4]], smooth: f32, closed: bool, max_points: usize) -> Vec<[f32; 4]> {
    let max_points = max_points.clamp(4, MAX_POINTS);
    let mut nodes = clean(path);
    if nodes.is_empty() {
        return Vec::new();
    }
    if nodes.len() == 1 {
        return vec![nodes[0], nodes[0]];
    }
    // too many nodes: keep an even spread, ends included
    let keep = max_points - 1;
    if nodes.len() > keep {
        let n = nodes.len();
        nodes = (0..keep).filter_map(|k| nodes.get((k * (n - 1) + (keep - 1) / 2) / (keep - 1))).copied().collect();
    }
    let closed = closed && nodes.len() >= 3;
    let smooth = if smooth.is_finite() { smooth.clamp(0.0, 1.0) } else { 0.0 };
    let n = nodes.len();
    let segs = if closed { n } else { n - 1 };
    let steps = if smooth <= 1e-4 { 1 } else { ((max_points - 1) / segs).clamp(1, MAX_STEPS) };
    let mut out = Vec::with_capacity(segs * steps + 1);
    for seg in 0..segs {
        for j in 0..steps {
            out.push(segment_point(&nodes, seg, j as f32 / steps as f32, smooth, closed));
        }
    }
    out.push(if closed { nodes[0] } else { nodes[n - 1] });
    out.truncate(max_points);
    out
}

/// A box around the points, grown by their largest radius: (low corner, high corner).
pub fn bounds(points: &[[f32; 4]]) -> (V3, V3) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut rmax = 0.0_f32;
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
        rmax = rmax.max(p[3]);
    }
    if points.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }
    let g = rmax + 0.01;
    ([lo[0] - g, lo[1] - g, lo[2] - g], [hi[0] + g, hi[1] + g, hi[2] + g])
}

/// Half the size of the box around the curve (centred on the shape), at least 1 mm.
pub fn half_extent(path: &[[f32; 4]], params: &[f32; 4]) -> V3 {
    let mut h = [1.0_f32; 3];
    for p in build(path, params) {
        for (i, hi) in h.iter_mut().enumerate() {
            *hi = hi.max(p[i].abs() + p[3]);
        }
    }
    h
}

/// Distance to the round cone from `a` (radius `ra`) to `b` (radius `rb`).
pub fn round_cone(q: V3, a: V3, ra: f32, b: V3, rb: f32) -> f32 {
    let ba = sub(b, a);
    let l2 = dot(ba, ba);
    let rr = ra - rb;
    let a2 = l2 - rr * rr;
    let pa = sub(q, a);
    // a point, or one end swallows the other: just the bigger ball
    if l2 < 1.0e-4 || a2 < 1.0e-4 {
        let (c, r) = if ra >= rb { (a, ra) } else { (b, rb) };
        return len(sub(q, c)) - r;
    }
    let il2 = 1.0 / l2;
    let y = dot(pa, ba);
    let z = y - l2;
    let w = sub(mul(pa, l2), mul(ba, y));
    let x2 = dot(w, w);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = rr.signum() * rr * rr * x2;
    if z.signum() * a2 * z2 > k {
        (x2 + z2).sqrt() * il2 - rb
    } else if y.signum() * a2 * y2 < k {
        (x2 + y2).sqrt() * il2 - ra
    } else {
        ((x2 * a2 * il2).sqrt() + y * rr) * il2 - ra
    }
}

/// Distance to the tube through already-resampled `points`; `flat` cuts the two ends flat.
pub fn tube(points: &[[f32; 4]], q: V3, flat: bool) -> f32 {
    let mut d = 1.0e6_f32;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        d = d.min(round_cone(q, [a[0], a[1], a[2]], a[3], [b[0], b[1], b[2]], b[3]));
    }
    if flat { d.max(end_cut(points, q)) } else { d }
}

/// The two half-spaces that cut a tube flat at its first and last point (positive beyond an end).
fn end_cut(points: &[[f32; 4]], q: V3) -> f32 {
    if points.len() < 2 {
        return -1.0e6;
    }
    let (first, second) = (points[0], points[1]);
    let (last, before) = (points[points.len() - 1], points[points.len() - 2]);
    let t0 = norm(sub([second[0], second[1], second[2]], [first[0], first[1], first[2]]));
    let t1 = norm(sub([last[0], last[1], last[2]], [before[0], before[1], before[2]]));
    let c0 = dot(sub(q, [first[0], first[1], first[2]]), mul(t0, -1.0));
    let c1 = dot(sub(q, [last[0], last[1], last[2]]), t1);
    c0.max(c1)
}

/// Turn a solid distance into a hollow wall of thickness `wall` (the skin just inside the surface).
fn hollow(d: f32, wall: f32) -> f32 {
    (d + wall * 0.5).abs() - wall * 0.5
}

/// Distance from `q` (local, unscaled) to the shape. Negative inside. A shell (wall thickness, in
/// world mm) hollows the tube here rather than in `Prepared::distance`, so that a pipe with flat
/// ends is cut open *after* it is hollowed: the bore shows at the ends.
pub fn distance(p: &Prepared, q: V3) -> f32 {
    let (closed, _, flat, _) = settings(&p.params);
    if p.path.len() < 2 {
        return 1.0e6;
    }
    let cut = flat && !closed;
    let wall = if p.shell.is_finite() { p.shell.max(0.0) / p.min_scale().max(1.0e-6) } else { 0.0 };
    if wall > 0.0 {
        let d = hollow(tube(&p.path, q, false), wall);
        return if cut { d.max(end_cut(&p.path, q)) } else { d };
    }
    tube(&p.path, q, cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material, Shape};

    fn capsule(q: V3, a: V3, b: V3, r: f32) -> f32 {
        let pa = sub(q, a);
        let ba = sub(b, a);
        let h = (dot(pa, ba) / dot(ba, ba)).clamp(0.0, 1.0);
        len(sub(pa, mul(ba, h))) - r
    }

    fn lcg(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (*seed >> 8) as f32 / (1 << 24) as f32
    }

    #[test]
    fn a_straight_two_node_curve_is_a_capsule() {
        let (a, b, r) = ([-20.0, 5.0, 3.0], [25.0, -10.0, 12.0], 4.0);
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[a[0], a[1], a[2], r], [b[0], b[1], b[2], r]];
        for smooth in [0.0, 0.5, 1.0] {
            s.params = [0.0, smooth, 0.0, 1.0];
            let prep = Prepared::new(&s);
            let mut seed = 7;
            for _ in 0..300 {
                let q = [lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 120.0 - 60.0];
                let want = capsule(q, a, b, r);
                let got = distance(&prep, q);
                assert!((got - want).abs() < 2e-3, "smooth {smooth} q {q:?}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn the_field_measures_a_curve_like_a_capsule() {
        let mut s = Shape::new(Kind::Curve, [5.0, 0.0, 0.0], Material::default());
        s.path = vec![[-10.0, 0.0, 0.0, 3.0], [10.0, 0.0, 0.0, 3.0]];
        s.blend = 0.0;
        let g = crate::model::Group { shapes: vec![s], ..crate::model::Group::new("g") };
        let scene = crate::model::Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() };
        let f = crate::sdf::Field::new(&scene);
        assert!((f.distance([5.0, 10.0, 0.0]) - 7.0).abs() < 1e-3);
        assert!((f.distance([20.0, 0.0, 0.0]) - 2.0).abs() < 1e-3);
    }

    #[test]
    fn the_radius_changes_along_a_cone() {
        let d = round_cone([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 10.0, [100.0, 0.0, 0.0], 2.0);
        assert!((d + 10.0).abs() < 1e-3, "{d}");
        // far on the thin side the surface is about 2 away from the end
        let d = round_cone([110.0, 0.0, 0.0], [0.0, 0.0, 0.0], 10.0, [100.0, 0.0, 0.0], 2.0);
        assert!((d - 8.0).abs() < 1e-3, "{d}");
        // coincident nodes and a swallowed end are balls
        assert!((round_cone([5.0, 0.0, 0.0], [0.0; 3], 3.0, [0.0; 3], 3.0) - 2.0).abs() < 1e-4);
        assert!((round_cone([30.0, 0.0, 0.0], [0.0; 3], 20.0, [1.0, 0.0, 0.0], 2.0) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn resampling_stays_in_bounds_and_finite() {
        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let hostile: Vec<Vec<[f32; 4]>> = vec![
            vec![],
            vec![[1.0, 2.0, 3.0, 4.0]],
            vec![[nan, 0.0, 0.0, 1.0], [0.0, inf, 0.0, 1.0]],
            vec![[0.0, 0.0, 0.0, -5.0], [1e30, -1e30, 0.0, 1e30], [0.0, 0.0, 0.0, 0.0]],
            (0..1000).map(|i| [i as f32, (i * i) as f32, 0.0, 2.0]).collect(),
            vec![[0.0, 0.0, 0.0, 3.0]; 20],
        ];
        for path in &hostile {
            for smooth in [-1.0, 0.0, 0.3, 1.0, 5.0, f32::NAN] {
                for closed in [false, true] {
                    for cap in [0, 4, 10, 48, 500] {
                        let pts = resample_max(path, smooth, closed, cap);
                        assert!(pts.len() <= MAX_POINTS, "{} points", pts.len());
                        assert!(pts.iter().all(|p| p.iter().all(|v| v.is_finite()) && p[3] > 0.0));
                        let (lo, hi) = bounds(&pts);
                        assert!(lo.iter().chain(hi.iter()).all(|v| v.is_finite()));
                    }
                }
            }
        }
        assert!(resample(&[], 0.5, false).is_empty());
        assert_eq!(resample(&[[1.0, 2.0, 3.0, 4.0]], 0.5, false).len(), 2);
    }

    #[test]
    fn smoothing_passes_through_the_nodes_and_loops_close() {
        let path = [[0.0, 0.0, 0.0, 4.0], [20.0, 10.0, 0.0, 6.0], [40.0, -10.0, 5.0, 3.0], [60.0, 0.0, 0.0, 4.0]];
        let pts = resample(&path, 1.0, false);
        assert!(pts.len() <= MAX_POINTS && pts.len() > path.len());
        let steps = (pts.len() - 1) / 3;
        for (i, n) in path.iter().enumerate() {
            let p = pts[(i * steps).min(pts.len() - 1)];
            assert!(sub([p[0], p[1], p[2]], [n[0], n[1], n[2]]).iter().all(|v| v.abs() < 1e-3), "node {i}");
        }
        let straight = resample(&path, 0.0, false);
        assert_eq!(straight.len(), 4, "no smoothing keeps the nodes");
        let ring = resample(&path, 0.8, true);
        assert_eq!(ring.first(), ring.last());
        assert!(ring.len() <= MAX_POINTS);
    }

    #[test]
    fn flat_ends_cut_the_tube_short() {
        let pts = [[0.0, 0.0, 0.0, 5.0], [40.0, 0.0, 0.0, 5.0]];
        // beyond the end: round caps reach 5 further, flat ones stop at the end
        assert!((tube(&pts, [45.0, 0.0, 0.0], false)).abs() < 1e-4);
        assert!((tube(&pts, [45.0, 0.0, 0.0], true) - 5.0).abs() < 1e-4);
        assert!((tube(&pts, [20.0, 0.0, 0.0], true) + 5.0).abs() < 1e-4);
    }

    #[test]
    fn the_thickness_multiplier_scales_every_radius() {
        let path = [[0.0, 0.0, 0.0, 2.0], [10.0, 0.0, 0.0, 4.0]];
        let a = build(&path, &[0.0, 0.0, 0.0, 1.0]);
        let b = build(&path, &[0.0, 0.0, 0.0, 2.0]);
        assert!((b[0][3] - 2.0 * a[0][3]).abs() < 1e-5);
        // an old file with no multiplier (0) means 1
        assert_eq!(build(&path, &[0.0; 4]), build(&path, &[0.0, 0.0, 0.0, 1.0]));
    }
}
