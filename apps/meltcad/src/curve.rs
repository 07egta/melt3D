//! The curve (kind 18): a smooth tube through the nodes of `Shape::path` (x, y, z, radius), in the
//! shape's own space. The shader copy is `shader_curve.wgsl`; keep the two
//! in step.
//!
//! The nodes are first resampled with a (centripetal) Catmull-Rom spline into at most `MAX_POINTS` points (the
//! GPU and the CPU both measure the *resampled* points, so they always agree). The tube is then the
//! union of round cones (a capsule whose two ends have different radii) between neighbours. Adjacent
//! spans melt locally where they turn, without repeatedly swelling a long straight tube.
//!
//! Extra numbers (`Shape::params`): 0 = closed loop (0 / 1), 1 = smoothness 0..1 (0 = straight
//! pieces with round joints), 2 = flat ends (0 round / 1 flat), 3 = thickness multiplier (all radii).

use crate::sdf::{Prepared, V3, add, cross, dot, len, mul, norm, sub};

/// Most points one curve gives to the GPU.
pub const MAX_POINTS: usize = 96;
/// Segments per bounding sphere that the GPU copy looks at before measuring them (`shader_curve.wgsl`).
pub const GROUP_SEGMENTS: usize = 8;
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
        .map(|p| {
            [
                p[0].clamp(-COORD_LIMIT, COORD_LIMIT),
                p[1].clamp(-COORD_LIMIT, COORD_LIMIT),
                p[2].clamp(-COORD_LIMIT, COORD_LIMIT),
                p[3].clamp(R_MIN, R_MAX),
            ]
        })
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
#[cfg(test)]
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
    // A resampled closed loop must have exactly coincident endpoints. Catmull
    // evaluation at t=0 can differ from the stored node by a rounding ULP.
    if closed && !out.is_empty() {
        out[0] = nodes[0];
    }
    out.truncate(max_points);
    out
}

/// Evaluate a true cubic Bezier segment when any node has explicit handle mode.
/// Legacy shapes without modes continue using the original Catmull-Rom path.
pub fn segment_shape(shape: &crate::model::Shape, seg: usize, t: f32) -> [f32; 4] {
    use crate::model::CurveNodeMode;
    let n = shape.path.len();
    let (closed, smooth, _, _) = settings(&shape.params);
    if n < 2 || seg >= (if closed && n >= 3 { n } else { n - 1 }) {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if !shape.curve_handles.iter().any(|h| h.mode != CurveNodeMode::Auto) {
        return segment_point(&shape.path, seg, t, smooth, closed);
    }
    let j = (seg + 1) % n;
    let a = shape.path[seg];
    let b = shape.path[j];
    let tangent = |idx: usize| -> [f32; 3] {
        if smooth <= 1e-4 {
            return [0.0; 3];
        }
        // A derivative of the *existing* interpolant at each join, with
        // the segment's own time scale. Smooth nodes share one direction.
        let prev = if idx == 0 { if closed { n - 1 } else { 0 } } else { idx - 1 };
        let next = if idx + 1 == n { if closed { 0 } else { n - 1 } } else { idx + 1 };
        let p = shape.path[prev];
        let q = shape.path[next];
        [0, 1, 2].map(|k| (q[k] - p[k]) * smooth / 6.0)
    };
    let handles = |idx: usize, outgoing: bool| -> [f32; 3] {
        let h = shape.curve_handles.get(idx).copied().unwrap_or_default();
        match h.mode {
            CurveNodeMode::Auto => {
                let v = tangent(idx);
                if outgoing { v } else { v.map(|x| -x) }
            }
            CurveNodeMode::Smooth | CurveNodeMode::Corner => {
                if outgoing {
                    h.outgoing
                } else {
                    h.incoming
                }
            }
        }
    };
    let hout = handles(seg, true);
    let hin = handles(j, false);
    let p1 = [0, 1, 2].map(|k| a[k] + hout[k]);
    let p2 = [0, 1, 2].map(|k| b[k] + hin[k]);
    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;
    let mut out = [0.0; 4];
    for k in 0..3 {
        out[k] = u * u * u * a[k] + 3.0 * u * u * t * p1[k] + 3.0 * u * t * t * p2[k] + t * t * t * b[k];
    }
    out[3] = (a[3] * u + b[3] * t).clamp(R_MIN, R_MAX);
    out
}

/// Detect planar crossings of the centreline after subdivision (useful for
/// self-intersecting sketch paths; crossings in 3D projections are advisory).
#[cfg(test)]
pub fn self_intersections_xy(points: &[[f32; 4]], closed: bool) -> Vec<(usize, usize)> {
    fn orient(a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> f32 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    let m = points.len().saturating_sub(1);
    let mut intersections = Vec::new();
    for i in 0..m {
        let (a, b) = (points[i], points[i + 1]);
        for j in (i + 2)..m {
            if closed && i == 0 && j == m - 1 {
                continue;
            }
            let (c, d) = (points[j], points[j + 1]);
            let (ab1, ab2) = (orient(a, b, c), orient(a, b, d));
            let (cd1, cd2) = (orient(c, d, a), orient(c, d, b));
            if ab1 * ab2 < -1e-8 && cd1 * cd2 < -1e-8 {
                intersections.push((i, j));
            }
        }
    }
    intersections
}

/// Shared geometry for viewport, GPU and export. For manually edited curves,
/// adapt subdivision to curvature while honoring the GPU's 96-point limit.
pub fn build_shape(shape: &crate::model::Shape) -> Vec<[f32; 4]> {
    build_shape_max(shape, MAX_POINTS)
}

pub fn build_shape_max(shape: &crate::model::Shape, cap: usize) -> Vec<[f32; 4]> {
    use crate::model::CurveNodeMode;
    if !shape.curve_handles.iter().any(|h| h.mode != CurveNodeMode::Auto) {
        return build_max(&shape.path, &shape.params, cap);
    }
    let n = shape.path.len();
    if n < 2 {
        return build_max(&shape.path, &shape.params, cap);
    }
    let (closed, _, _, thick) = settings(&shape.params);
    let segs = if closed && n >= 3 { n } else { n - 1 };
    let cap = cap.clamp(4, MAX_POINTS);
    // Always retain endpoints of every segment. If the user exceeds the GPU
    // budget, use the established legacy point cap as a safe fallback.
    if segs + 1 > cap {
        return build_max(&shape.path, &shape.params, cap);
    }
    let mut divisions = vec![1usize; segs];
    // Iteratively split the segment with greatest midpoint chord error.
    // Additional quarter probes guard against inflection cancellation.
    for _ in 0..cap.saturating_sub(segs + 1) {
        let mut best = None;
        let mut score = 0.0_f32;
        for (seg, &count) in divisions.iter().enumerate() {
            if count >= 64 {
                continue;
            }
            let mut worst = 0.0_f32;
            for j in 0..count {
                let a = j as f32 / count as f32;
                let b = (j + 1) as f32 / count as f32;
                let pa = segment_shape(shape, seg, a);
                let pb = segment_shape(shape, seg, b);
                for fraction in [0.25, 0.5, 0.75] {
                    let t = a + (b - a) * fraction;
                    let actual = segment_shape(shape, seg, t);
                    let chord = lerp4(pa, pb, fraction);
                    let err = (0..3).map(|k| (actual[k] - chord[k]).powi(2)).sum::<f32>().sqrt();
                    worst = worst.max(err);
                }
            }
            if worst > score {
                score = worst;
                best = Some(seg);
            }
        }
        // 0.02mm in local space is a useful upper error target, not a
        // guaranteed screen-space tolerance when the GPU cap is exhausted.
        if score < 0.02 {
            break;
        }
        let Some(seg) = best else { break };
        divisions[seg] += 1;
    }
    let mut out = Vec::with_capacity(cap);
    for (seg, &count) in divisions.iter().enumerate() {
        for step in 0..count {
            out.push(segment_shape(shape, seg, step as f32 / count as f32));
        }
    }
    out.push(if closed && n >= 3 { shape.path[0] } else { shape.path[n - 1] });
    for p in &mut out {
        p[3] = (p[3] * thick).clamp(R_MIN, R_MAX);
    }
    out
}

/// Half the box around the sampled curve, centred on the shape and at least 1 mm.
/// Uses the full shape so explicit curve handles and node modes are respected.
pub fn half_extent_shape(shape: &crate::model::Shape) -> V3 {
    let mut h = [1.0_f32; 3];
    // the rounded-square section reaches further out than the round one
    let radius_factor = if shape.size[2].is_finite() && shape.size[2] >= 1.5 { 1.5 } else { 1.25 };
    for p in build_shape(shape) {
        for i in 0..3 {
            h[i] = h[i].max(p[i].abs() + p[3] * radius_factor);
        }
    }
    h
}

#[cfg(test)]
/// A box around the points, grown by their largest radius: (low corner, high corner).
pub fn bounds(points: &[[f32; 4]]) -> (V3, V3) {
    bounds_profile(points, false)
}

/// The rounded-square option reaches up to 2^(1/4) times the circular radius
/// across a diagonal. Reserve enough margin for the existing local smooth joints.
pub fn bounds_profile(points: &[[f32; 4]], squircle: bool) -> (V3, V3) {
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
    // A smooth join can extend at most k/4 beyond a round cone. joint_blend caps k below
    // 0.71 * the local radius, so 0.25 radius covers every join, including a group boundary.
    let g = rmax * if squircle { 1.5 } else { 1.25 } + 0.01;
    ([lo[0] - g, lo[1] - g, lo[2] - g], [hi[0] + g, hi[1] + g, hi[2] + g])
}

#[cfg(test)]
/// One bounding sphere (centre, radius) for every `GROUP_SEGMENTS` segments of the resampled
/// points, thick enough to hold the tube there: every round cone of the group lies inside it, so
/// the distance to the group is at least the distance to its sphere, and the tube has a point
/// inside each sphere (the distance is at most the distance to the centre plus the radius).
pub fn group_spheres(points: &[[f32; 4]]) -> Vec<[f32; 4]> {
    group_spheres_profile(points, false)
}

pub fn group_spheres_profile(points: &[[f32; 4]], squircle: bool) -> Vec<[f32; 4]> {
    let radius_factor = if squircle { 1.5 } else { 1.25 };
    let segs = points.len().saturating_sub(1);
    (0..segs.div_ceil(GROUP_SEGMENTS))
        .map(|g| {
            let from = g * GROUP_SEGMENTS;
            let to = (from + GROUP_SEGMENTS).min(points.len() - 1);
            let part = points.get(from..=to).unwrap_or(&[]);
            let (lo, hi) = part.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
                ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
            });
            let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
            let r = part.iter().map(|p| len(sub([p[0], p[1], p[2]], c)) + p[3] * radius_factor).fold(0.0_f32, f32::max);
            [c[0], c[1], c[2], r * 1.0005 + 0.005]
        })
        .collect()
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

/// Distance of a segment to a selectable circular, oval or rounded-square section.
/// The segment's centreline and existing radius interpolation stay unchanged.
/// With aspect=1 and squircle=false this is exactly the original round cone.
///
/// The transverse norm is 1-Lipschitz after multiplying the distance by
/// aspect <= 1; this is important for conservative SDF meshing/ray stepping.
pub fn section_cone(q: V3, a: V3, ra: f32, b: V3, rb: f32, aspect: f32, squircle: bool) -> f32 {
    let aspect = if aspect.is_finite() { aspect.clamp(0.45, 1.0) } else { 1.0 };
    if aspect >= 0.9999 && !squircle {
        return round_cone(q, a, ra, b, rb);
    }
    let delta = sub(b, a);
    if len(delta) < 1.0e-4 {
        // Coincident control points still produce a finite, closed ball.
        return round_cone(q, a, ra, b, rb);
    }
    let tangent = norm(delta);
    // A local cross-section orthogonal to the span; choosing a fallback
    // prevents a singular basis at vertical tangents.
    // Blend the frame's reference axis instead of snapping it at near-
    // vertical tangents. A hard axis switch creates a visible half-twist
    // in oval/rounded-square sections even though the SDF stays finite.
    let w = ((tangent[2].abs() - 0.80) / 0.18).clamp(0.0, 1.0);
    let w = w * w * (3.0 - 2.0 * w);
    let reference = [0.0, w, 1.0 - w];
    let side = norm(cross(reference, tangent));
    let up = cross(tangent, side);
    let v = sub(q, a);
    let x = dot(v, side) / aspect;
    let y = dot(v, up);
    let transverse = if squircle {
        (x.powi(4) + y.powi(4)).sqrt().sqrt()
    } else {
        x.hypot(y)
    };
    let q2 = add(a, add(mul(tangent, dot(v, tangent)), mul(side, transverse)));
    round_cone(q2, a, ra, b, rb) * aspect
}

/// Distance to the tube through already-resampled `points`; `flat` cuts the two ends flat.
#[cfg(test)]
pub fn tube(points: &[[f32; 4]], q: V3, flat: bool) -> f32 {
    let mut d = 1.0e6_f32;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        d = d.min(round_cone(q, [a[0], a[1], a[2]], a[3], [b[0], b[1], b[2]], b[3]));
    }
    if flat { d.max(end_cut(points, q)) } else { d }
}

/// How much the spans on either side of a node melt. A straight line needs no extra material;
/// a turn gets a reach proportional to its smallest ball, capped by its two span lengths. The
/// shader uses the same formula in `curve_joint_blend`.
pub fn joint_blend(a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> f32 {
    let ab = sub([b[0], b[1], b[2]], [a[0], a[1], a[2]]);
    let bc = sub([c[0], c[1], c[2]], [b[0], b[1], b[2]]);
    let (la, lb) = (len(ab), len(bc));
    if la < 1.0e-4 || lb < 1.0e-4 {
        return 0.0;
    }
    let turn = (1.0 - (dot(ab, bc) / (la * lb)).clamp(-1.0, 1.0)).max(0.0).sqrt();
    (a[3].min(b[3]).min(c[3]) * 0.5 * turn).min(la * 0.5).min(lb * 0.5)
}

/// The greatest joint reach of the resampled path, packed into the shader's curve header. It
/// lets the grouped search keep its fast skips while accounting for a join across group borders.
pub fn max_joint_blend(points: &[[f32; 4]], closed: bool) -> f32 {
    let mut max_k = 0.0_f32;
    for i in 1..points.len().saturating_sub(1) {
        max_k = max_k.max(joint_blend(points[i - 1], points[i], points[i + 1]));
    }
    max_k.max(closed_joint_blend(points, closed))
}

/// The reach where a closed curve's last and first spans meet, packed into the high box corner.
pub fn closed_joint_blend(points: &[[f32; 4]], closed: bool) -> f32 {
    if closed && points.len() >= 4 { joint_blend(points[points.len() - 2], points[0], points[1]) } else { 0.0 }
}

/// Polynomial soft minimum for one pair of adjoining spans, matching `curve_smin` in the shader.
fn joint_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 1.0e-6 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// The same distance as `smooth_tube_section`, but a run of spans whose bounding sphere (see
/// `group_spheres_profile`) is farther than everything measured so far is never measured: the
/// way a game skips far objects. Exact: a skipped span, and any soft join with it, can never
/// be below the nearest answer found (`kmax` is the greatest joint reach, so a join pulls at
/// most `kmax / 4` below its spans). The run nearest to the point is measured first, so the
/// answer to beat is tight from the start.
pub fn smooth_tube_grouped(points: &[[f32; 4]], groups: &[[f32; 4]], kmax: f32, q: V3, closed: bool, aspect: f32, squircle: bool) -> f32 {
    const MAX_GROUPS: usize = 16;
    let nseg = points.len().saturating_sub(1);
    // (a rounded-square section is not bounded by the spheres' simple scaling: it measures plainly)
    if squircle || groups.len() < 2 || groups.len() > MAX_GROUPS || groups.len() != nseg.div_ceil(GROUP_SEGMENTS) || !kmax.is_finite() {
        return smooth_tube_section(points, q, closed, aspect, squircle);
    }
    let asp = if aspect.is_finite() { aspect.clamp(0.45, 1.0) } else { 1.0 };
    // an oval section's distance is scaled by the aspect (inside, the sphere's bound still holds)
    let plain = asp >= 0.9999;
    let mut lb = [0.0_f32; MAX_GROUPS];
    let mut ub = f32::MAX;
    let mut nearest = 0;
    let mut best = f32::MAX;
    for (g, s) in groups.iter().enumerate() {
        let dc = len(sub(q, [s[0], s[1], s[2]]));
        // a lower bound of every span's distance in the run (negative when inside the sphere)
        let l = if dc > s[3] { (dc - s[3]) * asp } else { dc - s[3] };
        if let Some(slot) = lb.get_mut(g) {
            *slot = l;
        }
        if plain {
            ub = ub.min(dc + s[3]);
        }
        if l < best {
            best = l;
            nearest = g;
        }
    }
    let cone_at = |i: usize| -> f32 {
        match (points.get(i), points.get(i + 1)) {
            (Some(a), Some(b)) => section_cone(q, [a[0], a[1], a[2]], a[3], [b[0], b[1], b[2]], b[3], aspect, squircle),
            _ => 1.0e6,
        }
    };
    // measure the nearest run first (its spans are kept, they are used again below)
    let n_from = nearest * GROUP_SEGMENTS;
    let n_to = (n_from + GROUP_SEGMENTS).min(nseg);
    let mut cache = [1.0e6_f32; GROUP_SEGMENTS];
    let mut known = 1.0e6_f32;
    for i in n_from..n_to {
        let c = cone_at(i);
        if let Some(slot) = cache.get_mut(i - n_from) {
            *slot = c;
        }
        known = known.min(c);
    }
    // the round tube has a point inside every sphere, so its distance cannot exceed `ub`
    if plain {
        known = known.min(ub + 0.01);
    }
    let mut d = 1.0e6_f32;
    let mut first = None;
    let mut prev: Option<(usize, f32)> = None;
    for g in 0..groups.len() {
        if g != nearest && lb.get(g).copied().unwrap_or(0.0) >= d.min(known) + kmax {
            prev = None;
            continue;
        }
        for i in g * GROUP_SEGMENTS..((g + 1) * GROUP_SEGMENTS).min(nseg) {
            let one = if g == nearest { cache.get(i - n_from).copied().unwrap_or(1.0e6) } else { cone_at(i) };
            d = d.min(one);
            if let Some((pi, before)) = prev
                && pi + 1 == i
                && let (Some(pp), Some(a), Some(b)) = (points.get(i.wrapping_sub(1)), points.get(i), points.get(i + 1))
            {
                d = d.min(joint_min(before, one, joint_blend(*pp, *a, *b)));
            }
            if i == 0 {
                first = Some(one);
            }
            prev = Some((i, one));
        }
    }
    if closed
        && let (Some((li, last)), Some(first)) = (prev, first)
        && li + 1 == nseg
    {
        let k = closed_joint_blend(points, closed);
        d = d.min(joint_min(last, first, k));
    }
    d
}

#[cfg(test)]
/// The rounded path. Each adjacent pair is softened independently, so a long chain never grows
/// with its number of nodes. Unrelated spans and straight joins keep their exact round-cone SDF.
pub fn smooth_tube(points: &[[f32; 4]], q: V3, flat: bool, closed: bool) -> f32 {
    let d = smooth_tube_section(points, q, closed, 1.0, false);
    if flat { d.max(end_cut(points, q)) } else { d }
}

/// Identical local-joint blending to smooth_tube, changing only each span's
/// cross-section. No per-point mesh is generated during interactive editing.
pub fn smooth_tube_section(points: &[[f32; 4]], q: V3, closed: bool, aspect: f32, squircle: bool) -> f32 {
    let mut d = 1.0e6_f32;
    let mut first = None;
    let mut prev = None;
    for (i, w) in points.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let one = section_cone(q, [a[0], a[1], a[2]], a[3], [b[0], b[1], b[2]], b[3], aspect, squircle);
        d = d.min(one);
        if let Some(before) = prev {
            d = d.min(joint_min(before, one, joint_blend(points[i - 1], a, b)));
        } else {
            first = Some(one);
        }
        prev = Some(one);
    }
    if closed && let (Some(last), Some(first)) = (prev, first) {
        let k = closed_joint_blend(points, closed);
        d = d.min(joint_min(last, first, k));
    }
    d
}

#[cfg(test)]
/// The two half-spaces that cut a tube flat at its first and last point (positive beyond an end).
fn end_cut(points: &[[f32; 4]], q: V3) -> f32 {
    end_cut_amount(points, q, 1.0)
}

/// 0 = original fully rounded ends; 1 = flat caps through the end nodes.
/// Intermediate values move the cut inward continuously without opening a hole.
fn end_cut_amount(points: &[[f32; 4]], q: V3, flatness: f32) -> f32 {
    if points.len() < 2 {
        return -1.0e6;
    }
    let (first, second) = (points[0], points[1]);
    let (last, before) = (points[points.len() - 1], points[points.len() - 2]);
    let t0 = norm(sub([second[0], second[1], second[2]], [first[0], first[1], first[2]]));
    let t1 = norm(sub([last[0], last[1], last[2]], [before[0], before[1], before[2]]));
    let round = 1.0 - flatness.clamp(0.0, 1.0);
    let c0 = dot(sub(q, [first[0], first[1], first[2]]), mul(t0, -1.0)) - round * first[3];
    let c1 = dot(sub(q, [last[0], last[1], last[2]]), t1) - round * last[3];
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
    let (closed, _, _, _) = settings(&p.params);
    if p.path.len() < 2 {
        return 1.0e6;
    }
    let flatness = if p.params[2].is_finite() { p.params[2].clamp(0.0, 1.0) } else { 0.0 };
    let aspect = if p.size[1].is_finite() { p.size[1].clamp(0.45, 1.0) } else { 1.0 };
    let squircle = p.size[2].is_finite() && p.size[2] >= 1.5;
    let mut d = smooth_tube_grouped(&p.path, &p.curve_groups, p.curve_kmax, q, closed, aspect, squircle);
    let wall = if p.shell.is_finite() { p.shell.max(0.0) / p.min_scale().max(1.0e-6) } else { 0.0 };
    if wall > 0.0 {
        d = hollow(d, wall);
    }
    if !closed && flatness > 0.0 {
        d = d.max(end_cut_amount(&p.path, q, flatness));
    }
    d
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
    fn old_circular_tube_remains_numerically_unchanged() {
        let path = [[-16.0, 0.0, 0.0, 5.0], [0.0, 6.0, 0.0, 7.0], [15.0, -2.0, 0.0, 4.0]];
        let mut seed = 47;
        for _ in 0..250 {
            let q = [
                lcg(&mut seed) * 75.0 - 37.5,
                lcg(&mut seed) * 75.0 - 37.5,
                lcg(&mut seed) * 40.0 - 20.0,
            ];
            let original = smooth_tube(&path, q, false, false);
            let profiled = smooth_tube_section(&path, q, false, 1.0, false);
            assert_eq!(original, profiled, "default sections must keep old curves exactly");
        }
    }

    #[test]
    fn oval_and_rounded_square_sections_are_stable_and_distinct() {
        let (a, b) = ([-25.0, 0.0, 0.0], [25.0, 0.0, 0.0]);
        // The section's transverse axes are Y/Z for this X-oriented path.
        let oval_side = section_cone([0.0, 2.5, 0.0], a, 5.0, b, 5.0, 0.5, false);
        let oval_top = section_cone([0.0, 0.0, 5.0], a, 5.0, b, 5.0, 0.5, false);
        assert!(oval_side.abs() < 1e-4, "{oval_side}");
        assert!(oval_top.abs() < 1e-4, "{oval_top}");
        let round = section_cone([0.0, 4.0, 4.0], a, 5.0, b, 5.0, 1.0, false);
        let soft_square = section_cone([0.0, 4.0, 4.0], a, 5.0, b, 5.0, 1.0, true);
        assert!(round > 0.1 && soft_square < -0.1, "{round}, {soft_square}");
        // A tight bend or a nearly vertical segment may not divide by zero.
        for b in [[0.0, 0.0, 40.0], [0.0, 0.0, -40.0], [0.001, 0.001, 0.001]] {
            for squircle in [false, true] {
                for aspect in [0.45, 0.7, 1.0] {
                    for x in [-10.0, 0.0, 10.0] {
                        assert!(section_cone([x, 1.0, 3.0], [0.0; 3], 5.0, b, 2.0, aspect, squircle).is_finite());
                    }
                }
            }
        }
    }

    #[test]
    fn cap_roundness_is_continuous_and_preserves_flat_end_compatibility() {
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[-15.0, 0.0, 0.0, 5.0], [15.0, 0.0, 0.0, 5.0]];
        for flatness in [0.0, 0.25, 0.5, 0.75, 1.0] {
            s.params = [0.0, 0.0, flatness, 1.0];
            let d = Prepared::new(&s).distance([-17.0, 0.0, 0.0]);
            assert!(d.is_finite());
            if flatness == 0.0 {
                assert!(d < -1.0, "rounded cap must still surround the point: {d}");
            }
            if flatness == 1.0 {
                assert!(d > 1.0, "flat cap must cut at the end node: {d}");
            }
        }
    }

    #[test]
    fn tight_hairpins_keep_finite_closed_sdf_for_both_sections() {
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[-22.0, 0.0, 0.0, 4.0], [0.0, 0.0, 0.0, 5.0], [-0.6, 1.0, 0.0, 5.0], [-22.0, 3.0, 0.0, 3.0]];
        s.params = [0.0, 1.0, 0.0, 1.0];
        for aspect in [0.5, 1.0] {
            for squared in [1.0, 2.0] {
                s.size[1] = aspect;
                s.size[2] = squared;
                let prepared = Prepared::new(&s);
                for p in &s.path {
                    let d = prepared.distance([p[0], p[1], p[2]]);
                    assert!(d.is_finite() && d < -0.05, "hairpin control point must remain inside: {d}");
                }
                let points = build(&s.path, &s.params);
                let (lo, hi) = bounds_profile(&points, squared >= 1.5);
                for a in 0..3 {
                    assert!(lo[a] < hi[a] && lo[a].is_finite() && hi[a].is_finite());
                }
            }
        }
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
        let scene =
            crate::model::Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new(), anim: Default::default() };
        let f = crate::sdf::Field::new(&scene);
        assert!((f.distance([5.0, 10.0, 0.0]) - 7.0).abs() < 1e-3);
        assert!((f.distance([20.0, 0.0, 0.0]) - 2.0).abs() < 1e-3);
    }

    #[test]
    fn a_bent_chain_of_round_nodes_has_a_full_rounded_elbow() {
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[-20.0, 0.0, 0.0, 4.0], [0.0, 0.0, 0.0, 4.0], [0.0, 20.0, 0.0, 4.0]];
        s.params = [0.0, 0.0, 0.0, 1.0];
        let p = Prepared::new(&s);
        let d = p.distance([-4.0, 4.0, 0.0]);
        assert!(d < -0.3, "two spherical spans should melt at the outside elbow, got {d}");

        // Collinear spans already make one exact cylinder and must not swell at their shared node.
        s.path = vec![[-20.0, 0.0, 0.0, 4.0], [0.0, 0.0, 0.0, 4.0], [20.0, 0.0, 0.0, 4.0]];
        let straight = Prepared::new(&s);
        assert!(straight.distance([0.0, 4.0, 0.0]).abs() < 1e-4);
    }

    #[test]
    fn the_join_of_a_closed_chain_is_rounded_too() {
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[-20.0, -20.0, 0.0, 4.0], [20.0, -20.0, 0.0, 4.0], [20.0, 20.0, 0.0, 4.0], [-20.0, 20.0, 0.0, 4.0]];
        s.params = [1.0, 0.0, 0.0, 1.0];
        let d = Prepared::new(&s).distance([-16.0, -16.0, 0.0]);
        assert!(d < -0.3, "the last and first spans should melt at the loop join, got {d}");
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

    /// The grouped search the shader does (look at the spheres, measure only groups that can hold
    /// something nearer than what is known) gives exactly the distance of the plain loop over all
    /// the segments. This is that search on the CPU, on random tubes and random points.
    #[test]
    fn the_grouped_search_equals_the_plain_loop() {
        let mut seed = 99;
        for round in 0..40 {
            let n = 2 + (lcg(&mut seed) * 40.0) as usize;
            let nodes: Vec<[f32; 4]> = (0..n)
                .map(|_| [lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 60.0, 0.5 + lcg(&mut seed) * 6.0])
                .collect();
            let pts = build(&nodes, &[0.0, 1.0, 0.0, 1.0]);
            let spheres = group_spheres(&pts);
            assert_eq!(spheres.len(), (pts.len() - 1).div_ceil(GROUP_SEGMENTS));
            for _ in 0..60 {
                let q = [lcg(&mut seed) * 200.0 - 100.0, lcg(&mut seed) * 200.0 - 100.0, lcg(&mut seed) * 120.0 - 30.0];
                let want = tube(&pts, q, false);
                // the grouped search
                let ub = spheres.iter().map(|s| len(sub(q, [s[0], s[1], s[2]])) + s[3]).fold(f32::MAX, f32::min);
                let lbmin = spheres.iter().map(|s| len(sub(q, [s[0], s[1], s[2]])) - s[3]).fold(f32::MAX, f32::min);
                assert!(want <= ub + 1e-3, "round {round}: the upper bound {ub} is below the distance {want}");
                assert!(want >= lbmin - 1e-3, "round {round}: the lower bound {lbmin} is above the distance {want}");
                let mut d = 1.0e6_f32;
                for (g, s) in spheres.iter().enumerate() {
                    if len(sub(q, [s[0], s[1], s[2]])) - s[3] > d.min(ub) {
                        continue;
                    }
                    let from = g * GROUP_SEGMENTS;
                    for k in 0..GROUP_SEGMENTS {
                        let i = from + k;
                        if i + 1 >= pts.len() {
                            break;
                        }
                        let (a, b) = (pts[i], pts[i + 1]);
                        d = d.min(round_cone(q, [a[0], a[1], a[2]], a[3], [b[0], b[1], b[2]], b[3]));
                    }
                }
                assert!((d - want).abs() < 1e-4, "round {round}: grouped {d} vs plain {want}");
            }
        }
    }

    /// The skipping search (nearest run first, far runs never measured) answers exactly what the
    /// plain loop over every span answers, for every section and for open and closed tubes.
    #[test]
    fn the_skipping_distance_equals_the_plain_distance() {
        let mut seed = 7;
        let mut skipped_any = false;
        let mut worst = 0.0_f32;
        for round in 0..60 {
            let n = 3 + (lcg(&mut seed) * 40.0) as usize;
            let nodes: Vec<[f32; 4]> = (0..n)
                .map(|_| [lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 120.0 - 60.0, lcg(&mut seed) * 60.0, 0.5 + lcg(&mut seed) * 6.0])
                .collect();
            let closed = round % 3 == 0;
            let pts = build(&nodes, &[if closed { 1.0 } else { 0.0 }, 1.0, 0.0, 1.0]);
            let groups_for = |squircle| group_spheres_profile(&pts, squircle);
            let kmax = max_joint_blend(&pts, closed);
            for (aspect, squircle) in [(1.0, false), (0.6, false), (0.45, false), (1.0, true), (0.7, true)] {
                let groups = groups_for(squircle);
                for _ in 0..80 {
                    let q = [lcg(&mut seed) * 200.0 - 100.0, lcg(&mut seed) * 200.0 - 100.0, lcg(&mut seed) * 120.0 - 30.0];
                    let want = smooth_tube_section(&pts, q, closed, aspect, squircle);
                    let got = smooth_tube_grouped(&pts, &groups, kmax, q, closed, aspect, squircle);
                    // (a soft join with a skipped span differs from it only by float rounding)
                    let err = (want - got).abs() / (1.0 + want.abs());
                    worst = worst.max(err);
                    assert!(err < 2.0e-6, "round {round} aspect {aspect} squircle {squircle} closed {closed} at {q:?}: {want} vs {got}");
                    skipped_any |= groups.len() > 1;
                }
            }
        }
        assert!(skipped_any);
        eprintln!("worst relative difference between the skipping and the plain distance: {worst:e}");
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
    #[test]
    fn cubic_bezier_endpoints_and_smooth_joint() {
        use crate::model::{CurveHandles, CurveNodeMode};
        let mut s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        s.path = vec![[0.0, 0.0, 0.0, 2.0], [10.0, 0.0, 0.0, 2.0], [20.0, 0.0, 0.0, 2.0]];
        s.curve_handles = vec![CurveHandles::default(); 3];
        s.curve_handles[1] = CurveHandles { mode: CurveNodeMode::Smooth, incoming: [-3.0, -4.0, 0.0], outgoing: [3.0, 4.0, 0.0] };
        assert_eq!(segment_shape(&s, 0, 1.0), s.path[1]);
        assert_eq!(segment_shape(&s, 1, 0.0), s.path[1]);
        let e = 1e-3;
        let left = segment_shape(&s, 0, 1.0 - e);
        let right = segment_shape(&s, 1, e);
        let joint = s.path[1];
        let a = [0, 1, 2].map(|k| (joint[k] - left[k]) / e);
        let b = [0, 1, 2].map(|k| (right[k] - joint[k]) / e);
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 0.05));
        let sampled = build_shape(&s);
        assert!(sampled.len() <= MAX_POINTS);
        assert!(sampled.iter().all(|p| p.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn legacy_shape_sampling_is_unchanged() {
        let s = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        assert_eq!(build_shape(&s), build(&s.path, &s.params));
    }

    #[test]
    fn crossing_detection_distinguishes_a_loop_from_a_bowtie() {
        let bowtie = [[0.0, 0.0, 0.0, 1.0], [10.0, 10.0, 0.0, 1.0], [0.0, 10.0, 0.0, 1.0], [10.0, 0.0, 0.0, 1.0]];
        assert_eq!(self_intersections_xy(&bowtie, false), vec![(0, 2)]);
        let square = [[0.0, 0.0, 0.0, 1.0], [10.0, 0.0, 0.0, 1.0], [10.0, 10.0, 0.0, 1.0], [0.0, 10.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]];
        assert!(self_intersections_xy(&square, true).is_empty());
    }

    #[test]
    fn authored_handles_survive_json_round_trip() {
        use crate::model::{CurveHandles, CurveNodeMode};
        let mut shape = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        shape.curve_handles = vec![CurveHandles { mode: CurveNodeMode::Corner, incoming: [-2.0, 3.0, 0.0], outgoing: [3.0, 1.0, 0.0] }];
        let json = serde_json::to_string(&shape).unwrap();
        let loaded: Shape = serde_json::from_str(&json).unwrap();
        assert_eq!(shape.curve_handles, loaded.curve_handles);
        let mut old_json: serde_json::Value = serde_json::from_str(&json).unwrap();
        old_json.as_object_mut().unwrap().remove("curve_handles");
        let old: Shape = serde_json::from_value(old_json).unwrap();
        assert!(old.curve_handles.is_empty());
        assert_eq!(build_shape(&old), build(&old.path, &old.params));
    }
}
