//! The shape maths on the CPU: the distance to the scene and the material at a point. The GPU
//! shader (`shader.wgsl`) implements the same formulas; this copy picks shapes with the mouse and
//! builds the mesh for export.

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
}

#[derive(Clone, Debug)]
pub struct PreparedGroup {
    pub op: Op,
    pub blend: f32,
    pub shapes: Vec<Prepared>,
    /// The scene (group, shape) of each prepared shape.
    pub origin: Vec<(usize, usize)>,
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
        Prepared {
            kind: s.kind,
            size: s.size,
            round: s.round,
            pos: s.pos,
            inv,
            scale: s.scale,
            op: s.op,
            blend: s.blend,
            material: s.material,
            // a revolve profile lives on the +x side of the axis
            outline: if s.kind == Kind::Revolve { s.outline.iter().map(|p| [p[0].max(0.0), p[1]]).collect() } else { s.outline.clone() },
            params: s.params,
            path: s.path.clone(),
            outline2: s.outline2.clone(),
            shell: if s.shell.is_finite() { s.shell.max(0.0) } else { 0.0 },
        }
    }
    /// The point in the shape's own space.
    pub fn local(&self, p: V3) -> V3 {
        let d = sub(p, self.pos);
        let q = [dot(self.inv[0], d), dot(self.inv[1], d), dot(self.inv[2], d)];
        [q[0] / self.scale[0], q[1] / self.scale[1], q[2] / self.scale[2]]
    }
    pub fn min_scale(&self) -> f32 {
        self.scale[0].min(self.scale[1]).min(self.scale[2])
    }
    /// The distance to this one shape (negative inside).
    pub fn distance(&self, p: V3) -> f32 {
        let d = self.local_distance(self.local(p)) * self.min_scale();
        // hollow walls: the solid's skin of thickness `shell` (negative inside the wall)
        if self.shell > 0.0 { (d + self.shell * 0.5).abs() - self.shell * 0.5 } else { d }
    }
    fn local_distance(&self, q: V3) -> f32 {
        let s = self.size;
        match self.kind {
            Kind::Sphere => len(q) - s[0],
            Kind::Box => {
                let b = [s[0] * 0.5, s[1] * 0.5, s[2] * 0.5];
                let r = self.round.min(b[0]).min(b[1]).min(b[2]);
                let d = [q[0].abs() - b[0] + r, q[1].abs() - b[1] + r, q[2].abs() - b[2] + r];
                len([d[0].max(0.0), d[1].max(0.0), d[2].max(0.0)]) + d[0].max(d[1]).max(d[2]).min(0.0) - r
            }
            Kind::Cylinder => {
                let r = self.round.min(s[0]).min(s[1] * 0.5);
                let d = [(q[0].hypot(q[1])) - s[0] + r, q[2].abs() - s[1] * 0.5 + r];
                d[0].max(d[1]).min(0.0) + d[0].max(0.0).hypot(d[1].max(0.0)) - r
            }
            Kind::Torus => {
                let a = q[0].hypot(q[1]) - s[0];
                a.hypot(q[2]) - s[1]
            }
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
                let d2 = polygon(&self.outline, [q[0], q[1]]);
                let r = self.round.min(s[0] * 0.5);
                let w = [d2 + r, q[2].abs() - s[0] * 0.5 + r];
                w[0].max(w[1]).min(0.0) + w[0].max(0.0).hypot(w[1].max(0.0)) - r
            }
            Kind::Curve => crate::curve::distance(self, q),
            k if k.id() < 16.0 => crate::prims::distance(self, q),
            _ => crate::cad::distance(self, q),
        }
    }
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
            for (si, s) in g.shapes.iter().enumerate() {
                if s.visible {
                    shapes.push(Prepared::new(s));
                    origin.push((gi, si));
                }
            }
            groups.push(PreparedGroup { op: g.op, blend: g.blend, shapes, origin });
        }
        let m = scene.melt;
        Field { groups, mode: m.mode, amount: m.amount, drip: m.drip, ripple: m.ripple }
    }

    fn group(&self, g: &PreparedGroup, p: V3) -> Option<Sample> {
        let mut acc: Option<Sample> = None;
        for (i, s) in g.shapes.iter().enumerate() {
            let d = s.distance(p);
            let me = Sample {
                dist: d,
                color: s.material.color,
                roughness: s.material.roughness,
                metallic: s.material.metallic,
                nearest: Some(g.origin[i]),
            };
            acc = Some(match acc {
                None => {
                    if s.op != Op::Add {
                        continue;
                    }
                    me
                }
                Some(a) => combine(a, me, s.op, s.blend * self.amount, self.mode),
            });
        }
        acc
    }

    /// Measure the scene at `p`.
    pub fn sample(&self, p: V3) -> Sample {
        let p = warp(p, self.drip, self.ripple);
        let mut acc: Option<Sample> = None;
        for g in &self.groups {
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
        // A stain never changes the shape, so the nearest thing stays what it was.
        Op::Paint => a,
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
                nearest: if h > 0.5 { b.nearest } else { a.nearest },
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
