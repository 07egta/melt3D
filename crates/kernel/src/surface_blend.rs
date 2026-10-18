//! Blend surface: a patch that joins an open edge of one surface to an open edge of another,
//! leaving each at the continuity asked for, and the measurement of how well two surfaces
//! really meet along an edge.
//!
//! The patch is built from one quintic Bézier curve per sample along the edges. Each curve
//! starts on one edge and ends on the other, and is given, at each end, the position (G0), the
//! position and the direction leaving the surface (G1), or those and the surface's normal
//! curvature in that direction (G2). The curves are interpolated along the edges by a cubic
//! B-spline, which makes a NURBS surface of degree 3 × 5. Only the samples are exact: between
//! them the surface follows the interpolation, so the achieved continuity is measured by
//! [`continuity`] and not taken for granted.
//!
//! Neither G3 nor "Class A" is promised: the curvature's rate of change is not constrained.
//! [`continuity`] reports an estimate of its jump, nothing more.

use solidcraft_geom::Vec3;
use solidcraft_geom::bspline::{CurveN, HPoint, NurbsSurface};
use truck_modeling as mt;

use crate::body::{Body, from_p3};
use crate::plane_fit::EdgeSampler;
use crate::record::{Rebuild, rebuilt_body};
use crate::{KernelError, Result, guard};

/// How a blend meets a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Continuity {
    /// Positions meet.
    G0,
    /// Positions and tangent planes meet.
    G1,
    /// Positions, tangent planes and the normal curvature across the edge meet.
    G2,
}

/// What a blend is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendOptions {
    /// Continuity at the first and second edge.
    pub continuity: [Continuity; 2],
    /// How far each end's tangent reaches, as a multiple of the gap between the edges (1 is the
    /// usual; less hugs the surface for a shorter stretch, more pulls the blend out and wider).
    pub bulge: [f64; 2],
    /// Samples along the edges (4 to 200).
    pub samples: usize,
}

impl Default for BlendOptions {
    fn default() -> Self {
        BlendOptions { continuity: [Continuity::G1, Continuity::G1], bulge: [1.0, 1.0], samples: 33 }
    }
}

/// One side of the blend: the face and its boundary edge, running along the face's boundary.
struct Side {
    face: mt::Face,
    edge: mt::Edge,
}

fn side_of(body: &Body, at: Vec3) -> Result<Side> {
    body.require_brep("blend")?;
    let edge = crate::surfaces::edge_near(body, at).ok_or_else(|| KernelError::Invalid("no edge near the picked point".into()))?;
    let mut found: Vec<Side> = Vec::new();
    for face in body.solid.face_iter() {
        for wire in face.boundaries() {
            for e in wire.edge_iter() {
                if e.id() == edge.id() {
                    found.push(Side { face: face.clone(), edge: e.clone() });
                }
            }
        }
    }
    match found.len() {
        1 => found.pop().ok_or_else(|| KernelError::Invalid("no edge".into())),
        0 => Err(KernelError::Invalid("the picked edge belongs to no face".into())),
        _ => Err(KernelError::Invalid("pick a free edge of a surface (this edge joins two faces)".into())),
    }
}

fn point_of(v: &mt::Vertex) -> Vec3 {
    from_p3(v.point())
}

/// The nearest point of the face to `p`, the face's normal there and its parameters.
struct Probe {
    point: Vec3,
    normal: Vec3,
    uv: (f64, f64),
}

fn probe(face: &mt::Face, p: Vec3, hint: Option<(f64, f64)>) -> Option<Probe> {
    let (point, normal, uv) = crate::curveblend::project(face, p, hint)?;
    (point.is_finite() && normal.is_finite()).then_some(Probe { point, normal, uv })
}

/// Everything known about a face along its edge at one sample.
struct Station {
    p: Vec3,
    /// Unit normal of the surface.
    n: Vec3,
    /// Second derivative per unit length across the edge (a vector).
    c2: Vec3,
    /// Third derivative per unit length, taken inwards.
    c3_in: Vec3,
}

/// The face's surface at `p` (on or near it): the normal, then the surface's points `k·h` from it
/// along `inward` (projected on the surface), from which the second and third derivatives across
/// the edge follow by finite differences.
fn station(face: &mt::Face, p: Vec3, inward: Vec3, h: f64, hint: &mut Option<(f64, f64)>) -> Option<Station> {
    let q0 = probe(face, p, *hint)?;
    *hint = Some(q0.uv);
    let mut f = [q0.point; 4];
    let mut uv = q0.uv;
    for (k, slot) in f.iter_mut().enumerate().skip(1) {
        let q = probe(face, q0.point + inward * (h * k as f64), Some(uv))?;
        uv = q.uv;
        *slot = q.point;
    }
    // One-sided differences with errors of order h² (second) and h (third).
    let c2 = (f[0] * 2.0 - f[1] * 5.0 + f[2] * 4.0 - f[3]) * (1.0 / (h * h));
    let c3_in = (f[3] - f[2] * 3.0 + f[1] * 3.0 - f[0]) * (1.0 / (h * h * h));
    Some(Station { p: q0.point, n: q0.normal, c2, c3_in })
}

/// The unit direction across the edge leaving the face, at an edge point whose direction along
/// the face's boundary is `t`: the boundary runs counter-clockwise about the normal, so the face
/// lies on the left.
fn leaving(t: Vec3, n: Vec3) -> Option<Vec3> {
    t.cross(n).normalized()
}

fn edge_points(sampler: &mut EdgeSampler, first: Vec3, last: Vec3, n: usize, reversed: bool) -> Result<Vec<Vec3>> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = i as f64 / (n - 1) as f64;
        let s = if reversed { 1.0 - s } else { s };
        let p = if i == 0 {
            if reversed { last } else { first }
        } else if i == n - 1 {
            if reversed { first } else { last }
        } else {
            sampler.at(s).ok_or_else(|| KernelError::Failed("an edge could not be sampled".into()))?
        };
        out.push(p);
    }
    Ok(out)
}

/// Control points of the quintic Bézier curve through `p0` and `p1` with first derivatives
/// `v0`, `v1` and second derivatives `a0`, `a1` at its ends.
fn quintic(p0: Vec3, p1: Vec3, v0: Vec3, v1: Vec3, a0: Vec3, a1: Vec3) -> [Vec3; 6] {
    [p0, p0 + v0 * 0.2, p0 + v0 * 0.4 + a0 * 0.05, p1 - v1 * 0.4 + a1 * 0.05, p1 - v1 * 0.2, p1]
}

/// A blend surface between the free edge of `a` nearest `a_at` and the free edge of `b` nearest
/// `b_at`: one surface body with one face, whose boundary uses those two edges whole. Call it
/// under [`guard`] (the public entry does).
fn blend(a: &Body, a_at: Vec3, b: &Body, b_at: Vec3, opts: &BlendOptions) -> Result<Body> {
    let n = opts.samples;
    if !(4..=200).contains(&n) {
        return Err(KernelError::Invalid("a blend takes 4 to 200 samples".into()));
    }
    if opts.bulge.iter().any(|x| !x.is_finite() || *x <= 0.0 || *x > 20.0) {
        return Err(KernelError::Invalid("the bulge must be between 0 and 20".into()));
    }
    let sa = side_of(a, a_at)?;
    let sb = side_of(b, b_at)?;
    let (mut ea, mut eb) = (
        EdgeSampler::new(&sa.edge).ok_or_else(|| KernelError::Invalid("the first edge has no parameter range".into()))?,
        EdgeSampler::new(&sb.edge).ok_or_else(|| KernelError::Invalid("the second edge has no parameter range".into()))?,
    );
    let (a0, a1) = (point_of(sa.edge.front()), point_of(sa.edge.back()));
    let (b0, b1) = (point_of(sb.edge.front()), point_of(sb.edge.back()));
    // The second edge is walked the way that keeps the blend from twisting.
    let reversed = a0.dist(b1) + a1.dist(b0) < a0.dist(b0) + a1.dist(b1);
    let pa = edge_points(&mut ea, a0, a1, n, false)?;
    let pb = edge_points(&mut eb, b0, b1, n, reversed)?;
    let size = a.size().max(b.size()).max(1e-9);
    let ds = 1.0 / (4.0 * n as f64);
    let h = (size * 2e-3).max(1e-6);
    let mut hint_a = None;
    let mut hint_b = None;
    let mut rows: Vec<[f64; 18]> = Vec::with_capacity(n);
    let mut columns: Vec<[Vec3; 6]> = Vec::with_capacity(n);
    for i in 0..n {
        let s = i as f64 / (n - 1) as f64;
        let chord = pa[i] - pb[i];
        let gap = chord.len();
        if !(gap > 1e-9) {
            return Err(KernelError::Invalid("the edges touch: there is nothing to blend".into()));
        }
        let toward = pb[i] - pa[i];
        // Tangents along each edge from neighbouring samples (the edge's own direction).
        let along = |sampler: &mut EdgeSampler, pts: &[Vec3], rev: bool| -> Option<Vec3> {
            let (lo, hi) = ((s - ds).max(0.0), (s + ds).min(1.0));
            let f = |x: f64, sampler: &mut EdgeSampler| sampler.at(if rev { 1.0 - x } else { x });
            let (p, q) = (f(lo, sampler).unwrap_or(pts[0]), f(hi, sampler).unwrap_or(pts[pts.len() - 1]));
            let t = (q - p).normalized()?;
            // A reversed walk goes against the boundary direction.
            Some(if rev { t * -1.0 } else { t })
        };
        let ta = along(&mut ea, &pa, false).ok_or_else(|| KernelError::Failed("an edge has no direction".into()))?;
        let tb = along(&mut eb, &pb, reversed).ok_or_else(|| KernelError::Failed("an edge has no direction".into()))?;
        let need_a = opts.continuity[0] != Continuity::G0;
        let need_b = opts.continuity[1] != Continuity::G0;
        let (mut v0, mut v1, mut c0, mut c1) = (toward, toward, Vec3::ZERO, Vec3::ZERO);
        if need_a {
            let q = probe(&sa.face, pa[i], hint_a).ok_or_else(|| KernelError::Failed("the first surface could not be probed".into()))?;
            hint_a = Some(q.uv);
            let out = leaving(ta, q.normal).ok_or_else(|| KernelError::Failed("no direction across the first edge".into()))?;
            v0 = out * (gap * opts.bulge[0]);
            if opts.continuity[0] == Continuity::G2 {
                let st = station(&sa.face, pa[i], out * -1.0, h, &mut hint_a)
                    .ok_or_else(|| KernelError::Invalid("the first surface is too narrow to read its curvature".into()))?;
                let k = st.n.dot(st.c2);
                c0 = st.n * (k * v0.len2());
            }
        }
        if need_b {
            let q = probe(&sb.face, pb[i], hint_b).ok_or_else(|| KernelError::Failed("the second surface could not be probed".into()))?;
            hint_b = Some(q.uv);
            let out = leaving(tb, q.normal).ok_or_else(|| KernelError::Failed("no direction across the second edge".into()))?;
            // The blend arrives travelling into the second surface.
            v1 = out * (-gap * opts.bulge[1]);
            if opts.continuity[1] == Continuity::G2 {
                let st = station(&sb.face, pb[i], out * -1.0, h, &mut hint_b)
                    .ok_or_else(|| KernelError::Invalid("the second surface is too narrow to read its curvature".into()))?;
                let k = st.n.dot(st.c2);
                c1 = st.n * (k * v1.len2());
            }
        }
        let ctrl = quintic(pa[i], pb[i], v0, v1, c0, c1);
        let mut row = [0.0; 18];
        for (j, c) in ctrl.iter().enumerate() {
            row[3 * j] = c.x;
            row[3 * j + 1] = c.y;
            row[3 * j + 2] = c.z;
        }
        if row.iter().any(|x| !x.is_finite()) {
            return Err(KernelError::Failed("the blend has a non-finite point".into()));
        }
        rows.push(row);
        columns.push(ctrl);
        let _ = s;
    }
    let params: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let curve = CurveN::<18>::interpolate(&params, &rows, 3).map_err(|e| KernelError::Failed(format!("blend: {e}")))?;
    let nu = curve.ctrl.len();
    let mut pts = Vec::with_capacity(nu * 6);
    for j in 0..6 {
        for c in &curve.ctrl {
            pts.push(Vec3::new(c[3 * j], c[3 * j + 1], c[3 * j + 2]));
        }
    }
    let knots_v: Vec<f64> = [0.0; 6].into_iter().chain([1.0; 6]).collect();
    let surface = NurbsSurface::new([3, 5], [curve.knots.clone(), knots_v], &pts, &vec![1.0; pts.len()])
        .map_err(|e| KernelError::Failed(format!("blend: {e}")))?;
    let surface = crate::spline_conv::surface(&surface)?;
    // The boundary: the first edge forwards, across to the second, the second edge back, and across again.
    let bezier = mt::KnotVec::from(knots_v_bezier());
    let across = |ctrl: &[Vec3; 6], flip: bool| -> Result<mt::Curve> {
        let mut pts: Vec<HPoint> = ctrl.iter().map(|p| HPoint::weighted(*p, 1.0)).collect();
        if flip {
            pts.reverse();
        }
        crate::spline_conv::nurbs_curve(bezier.clone(), &pts)
    };
    let (Some(first), Some(last)) = (columns.first(), columns.last()) else { return Err(KernelError::Failed("no samples".into())) };
    let (va0, va1) = (sa.edge.front(), sa.edge.back());
    let (vb_start, vb_end) = if reversed { (sb.edge.back(), sb.edge.front()) } else { (sb.edge.front(), sb.edge.back()) };
    let side_end = mt::Edge::try_new(va1, vb_end, across(last, false)?).map_err(|e| KernelError::Failed(format!("blend edge: {e}")))?;
    let side_start = mt::Edge::try_new(vb_start, va0, across(first, true)?).map_err(|e| KernelError::Failed(format!("blend edge: {e}")))?;
    // The second edge runs from the end to the start of the sampling.
    let back = if reversed { sb.edge.clone() } else { sb.edge.inverse() };
    let wire: mt::Wire = vec![sa.edge.clone(), side_end, back, side_start].into();
    let face = mt::Face::try_new(vec![wire], surface).map_err(|e| KernelError::Failed(format!("blend face: {e}")))?;
    let mut rec = Rebuild::default();
    rec.made(&face, Vec::new());
    let sources: Vec<&mt::Solid> = if std::sync::Arc::ptr_eq(&a.solid, &b.solid) { vec![&a.solid] } else { vec![&a.solid, &b.solid] };
    Ok(rebuilt_body(crate::surfaces::surface_body(vec![face]), &sources, &rec))
}

fn knots_v_bezier() -> Vec<f64> {
    [0.0; 6].into_iter().chain([1.0; 6]).collect()
}

/// A blend surface joining the free edge of `a` nearest `a_at` to the free edge of `b` nearest
/// `b_at` (see the module). A surface body of one face on those edges.
pub fn blend_surface(a: &Body, a_at: Vec3, b: &Body, b_at: Vec3, opts: &BlendOptions) -> Result<Body> {
    guard("blend surface", || blend(a, a_at, b, b_at, opts))
}

/// How two surfaces meet along an edge, the worst of the samples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeContinuity {
    pub samples: usize,
    /// G0: the largest distance between the surfaces (mm).
    pub gap: f64,
    /// G1: the largest angle between the normals (radians).
    pub angle: f64,
    /// G2: the largest difference of the normal curvatures across the edge (1/mm).
    pub curvature_jump: f64,
    /// The larger normal curvature met (1/mm), to read the jump against.
    pub curvature_max: f64,
    /// An estimate of the jump of the curvature's rate of change (1/mm²). Noisy; only an indication.
    pub g3_jump: f64,
}

/// Gap below which positions count as meeting, as a fraction of the body size.
pub const G0_TOL: f64 = 1e-4;
/// Angle (radians) below which tangent planes count as meeting: half a degree.
pub const G1_TOL: f64 = 0.5 * std::f64::consts::PI / 180.0;
/// Relative curvature jump below which curvatures count as meeting.
pub const G2_TOL: f64 = 0.1;

impl EdgeContinuity {
    /// The highest level reached: 0, 1 or 2 (`None` when not even the positions meet).
    pub fn level(&self, size: f64) -> Option<u8> {
        if self.gap > G0_TOL * size.max(1e-9) + 1e-6 {
            return None;
        }
        if self.angle > G1_TOL {
            return Some(0);
        }
        // Against the larger curvature met, or a gentle one for flat surfaces.
        let scale = self.curvature_max.max(0.1 / size.max(1e-9));
        if self.curvature_jump > G2_TOL * scale { Some(1) } else { Some(2) }
    }
}

/// How the free edge of `a` nearest `a_at` and the free edge of `b` nearest `b_at` meet (sampled
/// along the first). The edges are expected to lie together; the surfaces are read where they
/// are, not where they would be extended to.
pub fn continuity(a: &Body, a_at: Vec3, b: &Body, b_at: Vec3) -> Result<EdgeContinuity> {
    guard("continuity", || measure(a, a_at, b, b_at))
}

fn measure(a: &Body, a_at: Vec3, b: &Body, b_at: Vec3) -> Result<EdgeContinuity> {
    const N: usize = 41;
    let sa = side_of(a, a_at)?;
    let sb = side_of(b, b_at)?;
    let mut ea = EdgeSampler::new(&sa.edge).ok_or_else(|| KernelError::Invalid("the first edge has no parameter range".into()))?;
    let size = a.size().max(b.size()).max(1e-9);
    let h = (size * 4e-3).max(1e-6);
    let mut hint_a = None;
    let mut hint_b = None;
    let mut out = EdgeContinuity::default();
    let ds = 1.0 / (4.0 * N as f64);
    // The ends are corners where the neighbours bend: sample inside them.
    for i in 1..N - 1 {
        let s = i as f64 / (N - 1) as f64;
        let Some(p) = ea.at(s) else { continue };
        let (Some(lo), Some(hi)) = (ea.at((s - ds).max(0.0)), ea.at((s + ds).min(1.0))) else { continue };
        let Some(t) = (hi - lo).normalized() else { continue };
        let Some(qa) = probe(&sa.face, p, hint_a) else { continue };
        hint_a = Some(qa.uv);
        let Some(out_dir) = leaving(t, qa.normal) else { continue };
        let Some(st_a) = station(&sa.face, p, out_dir * -1.0, h, &mut hint_a) else { continue };
        // The other surface, entered from the edge: its inward direction is the first's outward.
        let Some(st_b) = station(&sb.face, p, out_dir, h, &mut hint_b) else { continue };
        let gap = st_a.p.dist(st_b.p);
        let angle = st_a.n.dot(st_b.n).abs().clamp(0.0, 1.0).acos();
        let (ka, kb) = (st_a.n.dot(st_a.c2), st_a.n.dot(st_b.c2));
        // Third derivative along the direction leaving the first surface: inward for the second,
        // reversed for the first.
        let g3 = (st_a.n.dot(st_a.c3_in * -1.0) - st_a.n.dot(st_b.c3_in)).abs();
        out.samples += 1;
        out.gap = out.gap.max(gap);
        out.angle = out.angle.max(angle);
        out.curvature_jump = out.curvature_jump.max((ka - kb).abs());
        out.curvature_max = out.curvature_max.max(ka.abs()).max(kb.abs());
        out.g3_jump = out.g3_jump.max(g3);
    }
    if out.samples == 0 {
        return Err(KernelError::Invalid("the surfaces could not be read along the edge".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use solidcraft_geom::{Plane, Seg2, Vec2, Vec3};

    use super::*;
    use crate::measure;

    /// A quarter-circle wall of radius 20 about the z axis (from (20,0) to (0,20)), 10 high, and
    /// a flat wall in the plane y = 30 from x = -60 to x = -30. The arc ends at (0,20) heading in
    /// -x, so the walls' free vertical edges face each other 30 mm apart and 10 mm off to the side:
    /// a straight strip between them leaves the arc wall at 18 degrees.
    fn walls() -> (Body, Body) {
        let arc = vec![Seg2::Arc { center: Vec2::new(0.0, 0.0), radius: 20.0, start: 0.0, sweep: std::f64::consts::FRAC_PI_2 }];
        let flat = vec![Seg2::Line { a: Vec2::new(-60.0, 30.0), b: Vec2::new(-30.0, 30.0) }];
        (crate::extrude_sheet(&Plane::XY, &[arc], 0.0, 10.0).unwrap(), crate::extrude_sheet(&Plane::XY, &[flat], 0.0, 10.0).unwrap())
    }

    const A_EDGE: Vec3 = Vec3 { x: 0.0, y: 20.0, z: 5.0 };
    const B_EDGE: Vec3 = Vec3 { x: -30.0, y: 30.0, z: 5.0 };

    fn blended(ca: Continuity, cb: Continuity) -> (Body, Body, Body) {
        let (a, b) = walls();
        let opts = BlendOptions { continuity: [ca, cb], ..BlendOptions::default() };
        let blend = blend_surface(&a, A_EDGE, &b, B_EDGE, &opts).unwrap();
        (a, b, blend)
    }

    #[test]
    fn a_blend_is_one_face_that_reaches_both_edges() {
        let (a, b, blend) = blended(Continuity::G1, Continuity::G1);
        assert!(blend.is_surface() && blend.face_count() == 1);
        let area = measure(&blend).unwrap().area;
        // About 32 mm across (a little more, it bows) by 10 mm high.
        assert!((290.0..420.0).contains(&area), "area {area}");
        let c = continuity(&a, A_EDGE, &blend, A_EDGE).unwrap();
        assert!(c.gap < 1e-4, "gap at the first edge {}", c.gap);
        let c = continuity(&b, B_EDGE, &blend, B_EDGE).unwrap();
        assert!(c.gap < 1e-4, "gap at the second edge {}", c.gap);
    }

    #[test]
    fn g1_blends_meet_the_tangent_planes_but_not_the_curvature() {
        let (a, b, blend) = blended(Continuity::G1, Continuity::G1);
        let c = continuity(&a, A_EDGE, &blend, A_EDGE).unwrap();
        assert!(c.angle < G1_TOL, "normal angle {} rad", c.angle);
        // The wall bends with radius 20 (curvature 0.05); a G1 blend leaves flat.
        assert!(c.curvature_jump > 0.03, "curvature jump {}", c.curvature_jump);
        assert_eq!(c.level(a.size().max(blend.size())), Some(1));
        let c = continuity(&b, B_EDGE, &blend, B_EDGE).unwrap();
        assert!(c.angle < G1_TOL && c.level(b.size().max(blend.size())) == Some(2), "the flat wall meets a flat end: {c:?}");
    }

    #[test]
    fn g2_blends_carry_the_curvature_across() {
        let (a, _, blend) = blended(Continuity::G2, Continuity::G2);
        let c = continuity(&a, A_EDGE, &blend, A_EDGE).unwrap();
        assert!(c.angle < G1_TOL, "normal angle {}", c.angle);
        assert!(c.curvature_jump < 0.01, "curvature jump {} against 0.05 for a G1 blend", c.curvature_jump);
        assert_eq!(c.level(a.size().max(blend.size())), Some(2), "{c:?}");
    }

    #[test]
    fn g0_blends_are_straight_between_the_edges() {
        let (a, _, blend) = blended(Continuity::G0, Continuity::G0);
        let c = continuity(&a, A_EDGE, &blend, A_EDGE).unwrap();
        assert!(c.gap < 1e-4, "{c:?}");
        // A ruled strip: sqrt(30² + 10²) across, 10 high, and not tangent to the wall it leaves.
        let area = measure(&blend).unwrap().area;
        assert!((area - 316.23).abs() < 3.0, "area {area}");
        assert!(c.angle > 0.25, "a straight strip meets the arc wall at 18 degrees, not {} rad", c.angle);
    }

    #[test]
    fn bad_input_is_an_error() {
        let (a, b) = walls();
        let far = Vec3::new(500.0, 500.0, 500.0);
        assert!(blend_surface(&a, far, &b, B_EDGE, &BlendOptions::default()).is_err());
        assert!(blend_surface(&a, A_EDGE, &b, B_EDGE, &BlendOptions { samples: 2, ..BlendOptions::default() }).is_err());
        assert!(blend_surface(&a, A_EDGE, &b, B_EDGE, &BlendOptions { bulge: [f64::NAN, 1.0], ..BlendOptions::default() }).is_err());
        assert!(blend_surface(&a, A_EDGE, &a, A_EDGE, &BlendOptions::default()).is_err(), "an edge cannot be blended to itself");
    }
}
