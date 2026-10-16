//! The shape maths on the CPU: the distance to the scene and the material at a point. The GPU
//! shader (`shader.wgsl`) implements the same formulas; this copy picks shapes with the mouse and
//! builds the mesh for export.

use crate::corner;
use crate::model::{BlendMode, Kind, Material, Op, Scene, Shape};

pub type V3 = [f32; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 1e-12 { mul(a, 1.0 / l) } else { [0.0, 0.0, 1.0] }
}

/// Rotation matrix rows for Euler angles (degrees) applied X, then Y, then Z.
pub fn rotation(rot: V3) -> [V3; 3] {
    let (sx, cx) = rot[0].to_radians().sin_cos();
    let (sy, cy) = rot[1].to_radians().sin_cos();
    let (sz, cz) = rot[2].to_radians().sin_cos();
    [[cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx], [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx], [-sy, cy * sx, cy * cx]]
}

/// A shape ready to be measured: its inverse transform and what it is.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub kind: Kind,
    pub size: V3,
    pub round: f32,
    pub pos: V3,
    /// Rows of the rotation's transpose (the inverse rotation).
    pub inv: [V3; 3],
    pub scale: V3,
    pub op: Op,
    pub blend: f32,
    pub material: Material,
    pub outline: Vec<[f32; 2]>,
    pub params: [f32; 4],
    pub path: Vec<[f32; 4]>,
    pub outline2: Vec<[f32; 2]>,
    /// Wall thickness of the hollowed-out shape (0 = solid).
    pub shell: f32,
    /// The corners are cut flat (a chamfer) instead of rounded.
    pub chamfer: bool,
    /// Winding (+1 / -1) and the largest safe rounding of the profile (revolve, sweep, extrude).
    pub poly_orient: f32,
    pub poly_limit: f32,
    /// Half the shape's size along the gradient direction (0 when there is no gradient).
    pub grad_half: f32,
}

#[derive(Clone, Debug)]
pub struct PreparedGroup {
    pub op: Op,
    pub blend: f32,
    pub shapes: Vec<Prepared>,
    /// The scene (group, shape) of each prepared shape.
    pub origin: Vec<(usize, usize)>,
    /// A bounding sphere of each shape, and the blocks and sphere of the whole group (accel.rs):
    /// far from them, a shape, a block or the group cannot change the answer, so it is not measured.
    pub bounds: Vec<crate::accel::Bound>,
    pub info: crate::accel::GroupInfo,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub groups: Vec<PreparedGroup>,
    pub mode: BlendMode,
    pub amount: f32,
    pub drip: f32,
    pub ripple: f32,
}

impl Prepared {
    pub fn new(s: &Shape) -> Prepared {
        let r = rotation(s.rot);
        let inv = [[r[0][0], r[1][0], r[2][0]], [r[0][1], r[1][1], r[2][1]], [r[0][2], r[1][2], r[2][2]]];
        let round = if s.round.is_finite() { s.round.max(0.0) } else { 0.0 };
        // a revolve profile lives on the +x side of the axis
        let outline: Vec<[f32; 2]> =
            if s.kind == Kind::Revolve { s.outline.iter().map(|p| [p[0].max(0.0), p[1]]).collect() } else { s.outline.clone() };
        let (poly_orient, poly_limit) = match s.kind {
            Kind::Revolve | Kind::Sweep | Kind::Extrude if round > 0.0 => crate::cad::inset_params(&outline, s.kind == Kind::Revolve),
            _ => (1.0, 0.0),
        };
        let grad_half = if s.material.has_gradient() { gradient_half(s, &r) } else { 0.0 };
        Prepared {
            kind: s.kind,
            size: s.size,
            round,
            chamfer: s.corner == crate::model::Corner::Chamfer,
            pos: s.pos,
            inv,
            scale: s.scale,
            op: s.op,
            blend: s.blend,
            material: s.material,
            outline,
            params: s.params,
            path: if s.kind == Kind::Curve { crate::curve::build(&s.path, &s.params) } else { s.path.clone() },
            outline2: s.outline2.clone(),
            shell: if s.shell.is_finite() { s.shell.max(0.0) } else { 0.0 },
            poly_orient,
            poly_limit,
            grad_half,
        }
    }
    /// The point in the shape's own space.
    pub fn local(&self, p: V3) -> V3 {
        let d = sub(p, self.pos);
        let q = [dot(self.inv[0], d), dot(self.inv[1], d), dot(self.inv[2], d)];
        [q[0] / self.scale[0], q[1] / self.scale[1], q[2] / self.scale[2]]
    }
    /// The surface colour at `p`: the material's colour, or its gradient (see `Material::gradient_weight`).
    pub fn color_at(&self, p: V3) -> V3 {
        let m = &self.material;
        if !m.has_gradient() {
            return m.color;
        }
        let ax = usize::from(m.grad_axis).min(2);
        let c = if m.grad_local { self.local(p)[ax] } else { p[ax] - self.pos[ax] };
        let t = c * (if m.grad_flip { -0.5 } else { 0.5 }) / self.grad_half.max(1e-3) + 0.5;
        mix3(m.color, m.color2, m.gradient_weight(t.max(0.0).min(1.0)))
    }
    pub fn min_scale(&self) -> f32 {
        self.scale[0].min(self.scale[1]).min(self.scale[2])
    }
    /// The distance to this one shape (negative inside).
    pub fn distance(&self, p: V3) -> f32 {
        let d = self.local_distance(self.local(p)) * self.min_scale();
        // hollow walls: the solid's skin of thickness `shell` (negative inside the wall)
        // (a curve hollows itself: its flat ends are cut after the hollowing, see curve::distance)
        if self.shell > 0.0 && self.kind != Kind::Curve { (d + self.shell * 0.5).abs() - self.shell * 0.5 } else { d }
    }
    #[cfg(test)]
    pub fn local_distance_for_test(&self, q: V3) -> f32 {
        self.local_distance(q)
    }
    fn local_distance(&self, q: V3) -> f32 {
        let s = self.size;
        match self.kind {
            Kind::Sphere => len(q) - s[0],
            Kind::Box => {
                let b = [s[0] * 0.5, s[1] * 0.5, s[2] * 0.5];
                let r = self.round.min(b[0]).min(b[1]).min(b[2]);
                if self.chamfer {
                    return corner::cut_box(q, b, r);
                }
                let d = [q[0].abs() - b[0] + r, q[1].abs() - b[1] + r, q[2].abs() - b[2] + r];
                len([d[0].max(0.0), d[1].max(0.0), d[2].max(0.0)]) + d[0].max(d[1]).max(d[2]).min(0.0) - r
            }
            Kind::Cylinder => {
                let r = self.round.min(s[0]).min(s[1] * 0.5);
                if self.chamfer {
                    return corner::pair(q[0].hypot(q[1]) - s[0], q[2].abs() - s[1] * 0.5, r);
                }
                let d = [(q[0].hypot(q[1])) - s[0] + r, q[2].abs() - s[1] * 0.5 + r];
                d[0].max(d[1]).min(0.0) + d[0].max(0.0).hypot(d[1].max(0.0)) - r
            }
            Kind::Torus => {
                let a = q[0].hypot(q[1]) - s[0];
                a.hypot(q[2]) - s[1]
            }
            Kind::Cone if self.round > 0.0 && self.chamfer => crate::prims::chamfered_cone(q, s, self.round),
            Kind::Cone if self.round > 0.0 => crate::prims::rounded_cone(q, s, self.round),
            Kind::Cone => {
                // Capped cone, base radius s0 at the bottom, a point at the top, height s1.
                let h = s[1] * 0.5;
                let (r1, r2) = (s[0], 0.0_f32);
                let qx = q[0].hypot(q[1]);
                let qy = q[2];
                let k1 = [r2, h];
                let k2 = [r2 - r1, 2.0 * h];
                let ca = [qx - qx.min(if qy < 0.0 { r1 } else { r2 }), qy.abs() - h];
                let t = (((k1[0] - qx) * k2[0] + (k1[1] - qy) * k2[1]) / (k2[0] * k2[0] + k2[1] * k2[1]).max(1e-9)).clamp(0.0, 1.0);
                let cb = [qx - k1[0] + k2[0] * t, qy - k1[1] + k2[1] * t];
                let sg = if cb[0] < 0.0 && ca[1] < 0.0 { -1.0 } else { 1.0 };
                sg * (ca[0] * ca[0] + ca[1] * ca[1]).min(cb[0] * cb[0] + cb[1] * cb[1]).sqrt()
            }
            Kind::Extrude => {
                let xy = [q[0], q[1]];
                let hh = s[0] * 0.5;
                if self.chamfer && self.round > 0.0 {
                    let d2 = corner::cut_polygon(polygon(&self.outline, xy), xy, &self.outline, self.poly_orient, self.round, false);
                    return corner::pair(d2, q[2].abs() - hh, self.round.min(hh));
                }
                // every convex corner of the outline and the top and bottom edges are rounded
                let r = if self.round > 0.0 { self.round.min(hh).min(self.poly_limit) } else { 0.0 };
                let d2 = if r > 0.0 { crate::cad::polygon_inset(&self.outline, xy, r, self.poly_orient, false) } else { polygon(&self.outline, xy) };
                let w = [d2, q[2].abs() - hh + r];
                w[0].max(w[1]).min(0.0) + w[0].max(0.0).hypot(w[1].max(0.0)) - r
            }
            Kind::Curve => crate::curve::distance(self, q),
            k if k.id() < 16.0 => crate::prims::distance(self, q),
            _ => crate::cad::distance(self, q),
        }
    }
}

/// Half the shape's size along its gradient direction: in the world (the box of the turned,
/// stretched shape) or in the shape's own box.
fn gradient_half(s: &Shape, rot: &[V3; 3]) -> f32 {
    let ax = usize::from(s.material.grad_axis).min(2);
    let half = s.local_half();
    let h = if s.material.grad_local {
        half[ax]
    } else {
        (0..3).map(|j| rot[ax][j].abs() * s.scale[j].abs() * half[j]).sum()
    };
    if h.is_finite() { h.max(1e-3) } else { 1.0 }
}

pub fn polygon(v: &[[f32; 2]], p: [f32; 2]) -> f32 {
    let n = v.len();
    if n < 3 {
        return 1e9;
    }
    let mut d = (p[0] - v[0][0]).powi(2) + (p[1] - v[0][1]).powi(2);
    let mut sgn = 1.0_f32;
    let mut j = n - 1;
    for i in 0..n {
        let e = [v[j][0] - v[i][0], v[j][1] - v[i][1]];
        let w = [p[0] - v[i][0], p[1] - v[i][1]];
        let t = ((w[0] * e[0] + w[1] * e[1]) / (e[0] * e[0] + e[1] * e[1]).max(1e-12)).clamp(0.0, 1.0);
        let b = [w[0] - e[0] * t, w[1] - e[1] * t];
        d = d.min(b[0] * b[0] + b[1] * b[1]);
        let c1 = p[1] >= v[i][1];
        let c2 = p[1] < v[j][1];
        let c3 = e[0] * w[1] > e[1] * w[0];
        if (c1 && c2 && c3) || (!c1 && !c2 && !c3) {
            sgn = -sgn;
        }
        j = i;
    }
    sgn * d.sqrt()
}

/// A soft minimum of two distances over a reach `k`, and how much of `a` it keeps (for mixing
/// materials): 1 = all `a`, 0 = all `b`.
pub fn smin(a: f32, b: f32, k: f32, mode: BlendMode) -> (f32, f32) {
    if k <= 1e-6 {
        return if a < b { (a, 1.0) } else { (b, 0.0) };
    }
    match mode {
        BlendMode::Smooth => {
            let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
            (b + (a - b) * h - k * h * (1.0 - h), h)
        }
        BlendMode::Round => {
            // A true circular fillet of radius r between the two surfaces.
            let r = k * 1.3;
            let u = [(r - a).max(0.0), (r - b).max(0.0)];
            let d = r.max(a.min(b)) - u[0].hypot(u[1]);
            (d, (0.5 + 0.5 * (b - a) / r).clamp(0.0, 1.0))
        }
        BlendMode::Gooey => {
            let kk = k * 6.0;
            let h = (kk - (a - b).abs()).max(0.0) / kk;
            let m = h * h * h * 0.5;
            let s = m * kk / 3.0;
            if a < b { (a - s, 1.0 - m * 0.5) } else { (b - s, m * 0.5) }
        }
    }
}

/// The drip warp: moves a sample point down by an amount that varies across the scene, so melted
/// edges hang in uneven drips.
pub const DRIP_FREQ: f32 = 0.12;

/// How much to shorten distance steps so the warped field is still safe to march: one over the
/// warp's steepness bound.
pub fn drip_safety(drip: f32, ripple: f32) -> f32 {
    if drip <= 0.0 {
        return 1.0;
    }
    let slope = drip * 1.32 * DRIP_FREQ * ripple;
    1.0 / (1.0 + slope * slope).sqrt()
}

pub fn warp(p: V3, drip: f32, ripple: f32) -> V3 {
    if drip <= 0.0 {
        return p;
    }
    let f = DRIP_FREQ * ripple;
    let w = 0.5 + 0.5 * (p[0] * f + (p[1] * f * 1.3).sin()).sin() * (p[1] * f * 0.8 + (p[0] * f * 0.6).cos()).cos();
    [p[0], p[1], p[2] + drip * w]
}

fn mix3(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// What a point measures: the distance to the scene, the material there, and the (group, shape)
/// that is nearest.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub dist: f32,
    pub color: V3,
    pub roughness: f32,
    pub metallic: f32,
    pub nearest: Option<(usize, usize)>,
}

impl Field {
    pub fn new(scene: &Scene) -> Field {
        let mut groups = Vec::new();
        for (gi, g) in scene.groups.iter().enumerate() {
            if !g.visible {
                continue;
            }
            let mut shapes = Vec::new();
            let mut origin = Vec::new();
            let mut members: Vec<&Shape> = Vec::new();
            for (si, s) in g.shapes.iter().enumerate() {
                if s.visible {
                    shapes.push(Prepared::new(s));
                    origin.push((gi, si));
                    members.push(s);
                }
            }
            let info = crate::accel::group_info(&members, &scene.melt);
            let bounds = members.iter().map(|s| crate::accel::shape_bound(s)).collect();
            groups.push(PreparedGroup { op: g.op, blend: g.blend, shapes, origin, bounds, info });
        }
        let m = scene.melt;
        Field { groups, mode: m.mode, amount: m.amount, drip: m.drip, ripple: m.ripple }
    }

    /// The same field with every skip switched off (for the tests that prove the skips are exact).
    #[cfg(test)]
    pub fn without_culling(mut self) -> Field {
        for g in &mut self.groups {
            g.bounds = vec![crate::accel::Bound { c: [0.0; 3], r: 1.0e9 }; g.shapes.len()];
            g.info.bound = crate::accel::Bound { c: [0.0; 3], r: 1.0e9 };
            g.info.blocks = vec![crate::accel::Block { bound: g.info.bound, kmax: 0.0, skippable: false, first: 0, count: g.shapes.len() }];
        }
        self
    }

    /// The shapes of one group melted together at `p`. A shape or a whole block that is far from
    /// `p` (see accel.rs) is left out: that changes nothing, it only saves the measuring.
    fn group(&self, g: &PreparedGroup, p: V3) -> Option<Sample> {
        use crate::accel::{block_far, far_enough, paint_far};
        let mut acc: Option<Sample> = None;
        let multi = g.info.blocks.len() > 1;
        for blk in &g.info.blocks {
            if multi && blk.skippable && let Some(a) = &acc && blk.bound.lower(p) >= block_far(a.dist, blk.kmax * self.amount, self.mode) {
                continue;
            }
            for i in blk.first..blk.first + blk.count {
                let Some(s) = g.shapes.get(i) else { continue };
                let k = s.blend * self.amount;
                if let Some(a) = &acc {
                    let lb = g.bounds.get(i).map_or(f32::NEG_INFINITY, |b| b.lower(p));
                    let far = match s.op {
                        Op::Intersect => false,
                        Op::Add => lb >= far_enough(a.dist, k, self.mode),
                        Op::Subtract => lb >= far_enough(-a.dist, k, self.mode),
                        Op::Paint => lb >= paint_far(k),
                    };
                    if far {
                        continue;
                    }
                }
                let me = Sample {
                    dist: s.distance(p),
                    color: s.color_at(p),
                    roughness: s.material.roughness,
                    metallic: s.material.metallic,
                    nearest: g.origin.get(i).copied(),
                };
                acc = Some(match acc {
                    None => {
                        if s.op != Op::Add {
                            continue;
                        }
                        me
                    }
                    Some(a) => combine(a, me, s.op, k, self.mode),
                });
            }
        }
        acc
    }

    /// Measure the scene at `p`.
    pub fn sample(&self, p: V3) -> Sample {
        use crate::accel::{far_enough, paint_far};
        let p = warp(p, self.drip, self.ripple);
        let mut acc: Option<Sample> = None;
        for g in &self.groups {
            if let Some(a) = &acc {
                let lb = g.info.bound.lower(p);
                let k = g.blend * self.amount;
                let far = match g.op {
                    Op::Intersect => false,
                    Op::Add => lb >= far_enough(a.dist, k, self.mode),
                    Op::Subtract => lb >= far_enough(-a.dist, k, self.mode),
                    Op::Paint => lb >= paint_far(k),
                };
                if far {
                    continue;
                }
            }
            let Some(gs) = self.group(g, p) else { continue };
            acc = Some(match acc {
                None => {
                    if g.op != Op::Add {
                        continue;
                    }
                    gs
                }
                Some(a) => combine(a, gs, g.op, g.blend * self.amount, self.mode),
            });
        }
        let mut out = acc.unwrap_or(Sample { dist: 1e6, color: [0.8; 3], roughness: 0.5, metallic: 0.0, nearest: None });
        if self.drip > 0.0 {
            out.dist *= drip_safety(self.drip, self.ripple);
        }
        out
    }

    pub fn distance(&self, p: V3) -> f32 {
        self.sample(p).dist
    }

    /// The first hit along a ray, if any (sphere tracing), for picking.
    pub fn ray(&self, origin: V3, dir: V3, max: f32) -> Option<(f32, Sample)> {
        let mut t = 0.0;
        for _ in 0..256 {
            let s = self.sample(add(origin, mul(dir, t)));
            if s.dist < 0.02 * (1.0 + t * 0.002) {
                return Some((t, s));
            }
            t += s.dist.max(0.01);
            if t > max {
                return None;
            }
        }
        None
    }
}

/// Add `b` to `a`, or carve `b` out of it.
fn combine(a: Sample, b: Sample, op: Op, k: f32, mode: BlendMode) -> Sample {
    match op {
        Op::Add => {
            let (d, h) = smin(a.dist, b.dist, k, mode);
            Sample {
                dist: d,
                color: mix3(b.color, a.color, h),
                roughness: b.roughness + (a.roughness - b.roughness) * h,
                metallic: b.metallic + (a.metallic - b.metallic) * h,
                nearest: if h >= 0.5 { a.nearest } else { b.nearest },
            }
        }
        // A stain never changes the shape, so the nearest thing stays what it was; inside the
        // stain (and a little way past its edge) the surface takes its material, as in the shader.
        Op::Paint => {
            let soft = (k * 0.35).max(0.3);
            let w = (0.5 - b.dist / soft).clamp(0.0, 1.0);
            Sample {
                dist: a.dist,
                color: mix3(a.color, b.color, w),
                roughness: a.roughness + (b.roughness - a.roughness) * w,
                metallic: a.metallic + (b.metallic - a.metallic) * w,
                nearest: a.nearest,
            }
        }
        Op::Intersect => {
            // smax(a, b) = -smin(-a, -b); the surface belongs to whichever is farther out
            let (d, h) = smin(-a.dist, -b.dist, k, mode);
            Sample {
                dist: -d,
                color: mix3(b.color, a.color, h),
                roughness: b.roughness + (a.roughness - b.roughness) * h,
                metallic: b.metallic + (a.metallic - b.metallic) * h,
                nearest: if h >= 0.5 { a.nearest } else { b.nearest },
            }
        }
        Op::Subtract => {
            // smax(a, -b) = -smin(-a, b)
            let (d, h) = smin(-a.dist, b.dist, k, mode);
            // Where the cut is, the surface takes the colour of the cutter.
            Sample {
                dist: -d,
                color: mix3(b.color, a.color, 1.0 - (1.0 - h).clamp(0.0, 1.0) * (1.0 - h)),
                roughness: a.roughness,
                metallic: a.metallic,
                nearest: if h > 0.5 { a.nearest } else { b.nearest },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Material, Scene};

    fn scene_of(shapes: Vec<Shape>) -> Scene {
        let mut g = Group::new("g");
        g.shapes = shapes;
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() }
    }

    #[test]
    fn the_primitives_measure_their_exact_surfaces() {
        let m = Material::default();
        let f = |k: Kind, p: V3| {
            let mut s = Shape::new(k, [0.0; 3], m);
            s.blend = 0.0;
            Field::new(&scene_of(vec![s])).distance(p)
        };
        assert!((f(Kind::Sphere, [30.0, 0.0, 0.0]) - 10.0).abs() < 1e-4);
        assert!((f(Kind::Box, [25.0, 0.0, 0.0]) - 10.0).abs() < 1e-4);
        assert!((f(Kind::Cylinder, [25.0, 0.0, 0.0]) - 10.0).abs() < 1e-4);
        assert!((f(Kind::Cylinder, [0.0, 0.0, 30.0]) - 10.0).abs() < 1e-4);
        assert!((f(Kind::Torus, [20.0, 0.0, 0.0]) + 7.0).abs() < 1e-4);
        assert!(f(Kind::Cone, [0.0, 0.0, -30.0]) > 9.9 && f(Kind::Cone, [0.0, 0.0, 0.0]) < 0.0);
        assert!((f(Kind::Extrude, [25.0, 0.0, 0.0]) - 10.0).abs() < 1e-3, "{}", f(Kind::Extrude, [25.0, 0.0, 0.0]));
        assert!(f(Kind::Extrude, [0.0, 0.0, 0.0]) < 0.0);
    }

    #[test]
    fn a_shell_hollows_any_shape_into_a_wall() {
        let m = Material::default();
        let mut s = Shape::new(Kind::Sphere, [0.0; 3], m);
        s.size[0] = 20.0;
        s.shell = 4.0;
        let f = |p: V3| Prepared::new(&s).distance(p);
        assert!((f([0.0, 0.0, 0.0]) - 16.0).abs() < 1e-4, "the middle is empty");
        assert!((f([18.0, 0.0, 0.0]) + 2.0).abs() < 1e-4, "the middle of the wall");
        assert!(f([20.0, 0.0, 0.0]).abs() < 1e-4 && f([16.0, 0.0, 0.0]).abs() < 1e-4, "both skins");
        assert!((f([25.0, 0.0, 0.0]) - 5.0).abs() < 1e-4);
        s.shell = f32::NAN;
        assert!((Prepared::new(&s).distance([0.0, 0.0, 0.0]) + 20.0).abs() < 1e-4);
    }

    #[test]
    fn intersect_keeps_only_the_overlap() {
        let m = Material::default();
        let mut a = Shape::new(Kind::Sphere, [-8.0, 0.0, 0.0], m);
        let mut b = Shape::new(Kind::Sphere, [8.0, 0.0, 0.0], m);
        a.blend = 0.0;
        b.blend = 0.0;
        b.op = Op::Intersect;
        let f = Field::new(&scene_of(vec![a, b]));
        assert!(f.distance([0.0, 0.0, 0.0]) < 0.0, "inside both");
        assert!(f.distance([-20.0, 0.0, 0.0]) > 0.0, "inside only the first");
        assert!(f.distance([20.0, 0.0, 0.0]) > 0.0, "inside only the second");
        assert!((f.distance([0.0, 0.0, 14.0]) - (14.0_f32.hypot(8.0) - 20.0)).abs() < 1e-3);
        // rounded overlap stays finite and between the sharp answers
        for mode in BlendMode::ALL {
            let mut sc = scene_of(vec![Shape::new(Kind::Sphere, [-8.0, 0.0, 0.0], m), Shape::new(Kind::Sphere, [8.0, 0.0, 0.0], m)]);
            sc.groups[0].shapes[1].op = Op::Intersect;
            sc.melt.mode = mode;
            assert!(Field::new(&sc).distance([0.0, 0.0, 15.0]).is_finite());
        }
    }

    #[test]
    fn two_spheres_melt_and_a_minus_shape_carves() {
        let m = Material::default();
        let a = Shape::new(Kind::Sphere, [-12.0, 0.0, 0.0], m);
        let mut b = Shape::new(Kind::Sphere, [12.0, 0.0, 0.0], m);
        let mid = [0.0, 0.0, 17.0];
        let mut sc = scene_of(vec![a.clone(), b.clone()]);
        sc.groups[0].shapes[1].blend = 0.0;
        let sharp = Field::new(&sc).distance(mid);
        sc.groups[0].shapes[1].blend = 12.0;
        let soft = Field::new(&sc).distance(mid);
        assert!(soft < sharp - 0.5, "melting fills the crease: {soft} vs {sharp}");
        // Carving a small ball out of the big one leaves a dent.
        b.op = Op::Subtract;
        b.pos = [20.0, 0.0, 0.0];
        b.size[0] = 8.0;
        let carved = Field::new(&scene_of(vec![Shape::new(Kind::Sphere, [0.0; 3], m), b]));
        assert!(carved.distance([20.0, 0.0, 0.0]) > 0.0, "the cut is empty");
        assert!(carved.distance([-10.0, 0.0, 0.0]) < 0.0);
    }

    #[test]
    fn a_ray_finds_the_surface_and_every_blend_mode_stays_finite() {
        let m = Material::default();
        let sc = scene_of(vec![Shape::new(Kind::Sphere, [0.0; 3], m)]);
        let f = Field::new(&sc);
        let (t, s) = f.ray([100.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 500.0).expect("hit");
        assert!((t - 80.0).abs() < 0.1 && s.nearest == Some((0, 0)));
        for mode in BlendMode::ALL {
            for (a, b) in [(-5.0, 3.0), (0.0, 0.0), (1e6, -1e6), (2.0, 2.0)] {
                let (d, h) = smin(a, b, 4.0, mode);
                assert!(d.is_finite() && (0.0..=1.0).contains(&h), "{mode:?} {a} {b}: {d} {h}");
            }
        }
        let mut dripping = sc.clone();
        dripping.melt.drip = 20.0;
        assert!(Field::new(&dripping).distance([0.0, 0.0, 0.0]).is_finite());
    }

    /// A busy scene: `groups` groups of mixed shapes, ops and blends spread over a plane.
    pub(crate) fn busy_scene(groups: usize, per_group: usize, seed: u32) -> Scene {
        let mut state = seed.wrapping_mul(2654435761).wrapping_add(12345);
        let mut rnd = move || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        };
        let kinds = [Kind::Sphere, Kind::Box, Kind::Cylinder, Kind::Torus, Kind::Cone, Kind::Capsule, Kind::Octa];
        let mut sc = Scene::default();
        sc.groups.clear();
        for gi in 0..groups {
            let mut g = Group::new(format!("g{gi}"));
            g.blend = rnd() * 10.0;
            g.op = if gi > 0 && rnd() < 0.15 { Op::Subtract } else { Op::Add };
            for si in 0..per_group {
                let kind = kinds[(rnd() * kinds.len() as f32) as usize % kinds.len()];
                let c = [((gi % 8) as f32 - 4.0) * 60.0 + (rnd() - 0.5) * 50.0, ((gi / 8) as f32 - 2.0) * 60.0 + (rnd() - 0.5) * 50.0, rnd() * 50.0];
                let mut s = Shape::new(kind, c, Material { color: [rnd(), rnd(), rnd()], ..Material::default() });
                s.size = [4.0 + rnd() * 12.0, 4.0 + rnd() * 12.0, 4.0 + rnd() * 12.0];
                s.rot = [rnd() * 90.0, rnd() * 90.0, rnd() * 90.0];
                s.blend = rnd() * 12.0;
                s.op = if si > 0 && rnd() < 0.2 { Op::Subtract } else if si > 0 && rnd() < 0.08 { Op::Intersect } else if si > 0 && rnd() < 0.08 { Op::Paint } else { Op::Add };
                g.shapes.push(s);
            }
            sc.groups.push(g);
        }
        sc
    }

    #[test]
    fn skipping_far_shapes_blocks_and_groups_never_changes_the_answer() {
        let mut worst = 0.0_f32;
        for (seed, mode) in [(1, BlendMode::Smooth), (2, BlendMode::Round), (3, BlendMode::Gooey)] {
            let mut sc = busy_scene(18, 20, seed);
            sc.melt.mode = mode;
            let fast = Field::new(&sc);
            let slow = Field::new(&sc).without_culling();
            assert!(fast.groups.iter().any(|g| g.info.blocks.len() > 1), "the groups are cut into blocks");
            let mut state = seed * 77 + 1;
            let mut rnd = move || {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 8) as f32 / (1u32 << 24) as f32
            };
            for _ in 0..1500 {
                let p = [(rnd() - 0.5) * 600.0, (rnd() - 0.5) * 400.0, rnd() * 120.0 - 20.0];
                let (a, b) = (fast.sample(p), slow.sample(p));
                let dd = (a.dist - b.dist).abs();
                worst = worst.max(dd);
                assert!(dd < 1e-3, "{mode:?} at {p:?}: {} against {}", a.dist, b.dist);
                if a.dist.abs() < 2.0 {
                    assert_eq!(a.nearest, b.nearest, "{mode:?} at {p:?}");
                    assert!((0..3).all(|i| (a.color[i] - b.color[i]).abs() < 2e-3), "{mode:?} colour at {p:?}");
                }
            }
        }
        eprintln!("worst skip error {worst}");
    }

    fn graded(kind: Kind, mat: Material) -> Shape {
        let mut s = Shape::new(kind, [0.0, 0.0, 0.0], mat);
        s.blend = 0.0;
        s
    }

    fn close(a: V3, b: V3) {
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-3), "{a:?} != {b:?}");
    }

    #[test]
    fn a_gradient_runs_from_one_colour_to_the_other_across_the_shape() {
        let (red, blue) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let mat = Material { color: red, color2: blue, gradient: 1.0, grad_soft: 1.0, ..Material::default() };
        let mut s = graded(Kind::Box, mat);
        s.size = [20.0, 20.0, 40.0];
        let p = Prepared::new(&s);
        close(p.color_at([0.0, 0.0, -20.0]), red); // the bottom
        close(p.color_at([0.0, 0.0, 20.0]), blue); // the top
        close(p.color_at([0.0, 0.0, 0.0]), [0.5, 0.0, 0.5]); // halfway
        close(p.color_at([0.0, 0.0, 500.0]), blue); // clamped beyond the ends
        close(p.color_at([0.0, 0.0, -500.0]), red);
        // no gradient: always the plain colour
        let plain = Prepared::new(&graded(Kind::Box, Material { color: red, ..Material::default() }));
        close(plain.color_at([0.0, 0.0, 30.0]), red);
        // flipped
        let mut f = s.clone();
        f.material.grad_flip = true;
        let p = Prepared::new(&f);
        close(p.color_at([0.0, 0.0, -20.0]), blue);
        close(p.color_at([0.0, 0.0, 20.0]), red);
        // a weaker gradient reaches only part of the way
        let mut w = s.clone();
        w.material.gradient = 0.5;
        close(Prepared::new(&w).color_at([0.0, 0.0, 20.0]), [0.5, 0.0, 0.5]);
        // the X axis
        let mut x = s.clone();
        x.material.grad_axis = 0;
        let p = Prepared::new(&x);
        close(p.color_at([-10.0, 0.0, 0.0]), red);
        close(p.color_at([10.0, 0.0, 0.0]), blue);
        close(p.color_at([0.0, 0.0, 15.0]), [0.5, 0.0, 0.5]);
        // a range: all red up to a quarter of the height, all blue from three quarters
        let mut r = s.clone();
        r.material.grad_from = 0.25;
        r.material.grad_to = 0.75;
        let p = Prepared::new(&r);
        close(p.color_at([0.0, 0.0, -10.0]), red);
        close(p.color_at([0.0, 0.0, 10.0]), blue);
        // a hard edge
        let mut h = s.clone();
        h.material.grad_soft = 0.0;
        let p = Prepared::new(&h);
        close(p.color_at([0.0, 0.0, -1.0]), red);
        close(p.color_at([0.0, 0.0, 1.0]), blue);
    }

    #[test]
    fn a_gradient_follows_the_world_or_the_shape() {
        let (red, blue) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let mat = Material { color: red, color2: blue, gradient: 1.0, grad_soft: 1.0, ..Material::default() };
        let mut s = graded(Kind::Box, mat);
        s.size = [40.0, 10.0, 10.0];
        s.rot = [0.0, 90.0, 0.0]; // the long side now stands up: the local X axis points down the world Z
        let world = Prepared::new(&s);
        close(world.color_at([0.0, 0.0, 20.0]), blue); // the world top is the end of the long box
        close(world.color_at([0.0, 0.0, -20.0]), red);
        // in the world the two ends of the local Z axis (world X here) are at the same height: purple
        close(world.color_at([5.0, 0.0, 0.0]), [0.5, 0.0, 0.5]);
        close(world.color_at([-5.0, 0.0, 0.0]), [0.5, 0.0, 0.5]);
        // following the shape the colour runs along the local Z axis, which is only 10 long
        s.material.grad_local = true;
        let local = Prepared::new(&s);
        let (a, b) = (local.color_at([5.0, 0.0, 0.0]), local.color_at([-5.0, 0.0, 0.0]));
        let is = |c: V3, t: V3| (0..3).all(|i| (c[i] - t[i]).abs() < 1e-3);
        assert!((is(a, red) && is(b, blue)) || (is(a, blue) && is(b, red)), "{a:?} {b:?}");
        // and turning the shape turns the colours with it
        let mut t = s.clone();
        t.rot = [0.0, 0.0, 0.0];
        let upright = Prepared::new(&t);
        close(upright.color_at([0.0, 0.0, -5.0]), red);
        close(upright.color_at([0.0, 0.0, 5.0]), blue);
    }

    #[test]
    fn a_stained_gradient_keeps_the_stain_and_the_gradient_elsewhere() {
        let (red, blue, green) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        let mat = Material { color: red, color2: blue, gradient: 1.0, grad_soft: 1.0, ..Material::default() };
        let mut base = graded(Kind::Box, mat);
        base.size = [40.0, 40.0, 40.0];
        let mut stain = graded(Kind::Sphere, Material { color: green, ..Material::default() });
        stain.size = [6.0, 1.0, 1.0];
        stain.pos = [0.0, -20.0, 10.0];
        stain.op = Op::Paint;
        let f = Field::new(&scene_of(vec![base, stain]));
        // on the front face: inside the stain it is green, far from it the gradient shows
        let inside = f.sample([0.0, -20.0, 10.0]);
        close(inside.color, green);
        let top = f.sample([0.0, -20.0, 20.0]);
        close(top.color, blue);
        let bottom = f.sample([0.0, -20.0, -20.0]);
        close(bottom.color, red);
        // a stain never changes the shape
        assert!((f.sample([0.0, -30.0, 10.0]).dist - 10.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod dump {
    use super::*;
    use crate::camera::Camera;

    /// `YT_SCENE=file cargo test -p yuracad dump_cpu_render -- --ignored` writes /tmp/yt/cpu.ppm
    #[test]
    #[ignore]
    fn dump_cpu_render() {
        let Ok(path) = std::env::var("YT_SCENE") else { return };
        let scene = Scene::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        let f = Field::new(&scene);
        let mut cam = Camera::default();
        cam.frame([0.0, 0.0, 20.0], 70.0);
        let (w, h) = (320usize, 200usize);
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in 0..h {
            for x in 0..w {
                let nx = x as f32 / w as f32 * 2.0 - 1.0;
                let ny = 1.0 - y as f32 / h as f32 * 2.0;
                let (o, d) = cam.ray([nx, ny], w as f32 / h as f32);
                let c = match f.ray(o, d, 3000.0) {
                    Some((t, s)) => {
                        let p = add(o, mul(d, t));
                        let e = 0.1;
                        let n = norm([
                            f.distance(add(p, [e, 0.0, 0.0])) - f.distance(sub(p, [e, 0.0, 0.0])),
                            f.distance(add(p, [0.0, e, 0.0])) - f.distance(sub(p, [0.0, e, 0.0])),
                            f.distance(add(p, [0.0, 0.0, e])) - f.distance(sub(p, [0.0, 0.0, e])),
                        ]);
                        let l = dot(n, norm([0.5, -0.6, 0.8])).max(0.0) * 0.8 + 0.2;
                        [s.color[0] * l, s.color[1] * l, s.color[2] * l]
                    }
                    None => [0.2, 0.2, 0.22],
                };
                for k in c {
                    out.push((k.clamp(0.0, 1.0) * 255.0) as u8);
                }
            }
        }
        std::fs::write("/tmp/yt/cpu.ppm", out).unwrap();
    }
}
