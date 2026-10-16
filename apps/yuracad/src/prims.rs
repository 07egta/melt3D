//! Distance functions of the extra solids (Prism3 .. Star), measured in the shape's own space.
//! The shader copy is `shader_prims.wgsl`; keep the two in step (same names with a `pm_` prefix there).
//!
//! All shapes are centred on their local origin with Z up. The distances are exact (or a very good
//! bound) and 1-Lipschitz, so they also work with rounding (`Shape::round`): a rounded shape is the
//! same shape shrunk by `r` and then grown by `r` again.

// `x.max(lo).min(hi)` is used on purpose: `clamp` panics when a bound is NaN or out of order, and sizes come from files.
#![allow(clippy::manual_clamp)]

use crate::corner;
use crate::model::Kind;
use crate::sdf::{Prepared, V3, cross, dot, len, mul, sub};
use std::f32::consts::PI;

const SQRT3_2: f32 = 0.866_025_4;
/// 2 * sqrt(2) / 3: the sideways part of a tetrahedron face normal.
const TET_S: f32 = 0.942_809_04;
/// sqrt(1/2) as the bevel of the octahedron's edges needs it.
const S2: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// `a` modulo `m`, always in `0..m` (the same formula as the shader).
fn modp(a: f32, m: f32) -> f32 {
    a - m * (a / m).floor()
}

/// A whole-number setting (sides, points) kept inside `lo..=hi`; `d` when it is not a number.
fn count(v: f32, lo: f32, hi: f32, d: f32) -> f32 {
    if v.is_finite() { (v + 0.5).floor().max(lo).min(hi) } else { d }
}

fn sgn(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// Distance of a point to a (flat) triangle in 3D.
fn ud_triangle(p: V3, a: V3, b: V3, c: V3) -> f32 {
    let ba = sub(b, a);
    let pa = sub(p, a);
    let cb = sub(c, b);
    let pb = sub(p, b);
    let ac = sub(a, c);
    let pc = sub(p, c);
    let nor = cross(ba, ac);
    let d2 = |v: V3| dot(v, v);
    let seg = |e: V3, w: V3| d2(sub(mul(e, (dot(e, w) / d2(e).max(1e-12)).max(0.0).min(1.0)), w));
    let inside = sgn(dot(cross(ba, nor), pa)) + sgn(dot(cross(cb, nor), pb)) + sgn(dot(cross(ac, nor), pc)) >= 2.0;
    let r = if inside { dot(nor, pa) * dot(nor, pa) / d2(nor).max(1e-12) } else { seg(ba, pa).min(seg(cb, pb)).min(seg(ac, pc)) };
    r.sqrt()
}

/// Signed distance of a point to a 2D triangle.
pub(crate) fn sd_triangle(p: [f32; 2], p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> f32 {
    let e0 = [p1[0] - p0[0], p1[1] - p0[1]];
    let e1 = [p2[0] - p1[0], p2[1] - p1[1]];
    let e2 = [p0[0] - p2[0], p0[1] - p2[1]];
    let v0 = [p[0] - p0[0], p[1] - p0[1]];
    let v1 = [p[0] - p1[0], p[1] - p1[1]];
    let v2 = [p[0] - p2[0], p[1] - p2[1]];
    let d2 = |a: [f32; 2]| a[0] * a[0] + a[1] * a[1];
    let pq = |v: [f32; 2], e: [f32; 2]| {
        let t = ((v[0] * e[0] + v[1] * e[1]) / d2(e).max(1e-12)).max(0.0).min(1.0);
        [v[0] - e[0] * t, v[1] - e[1] * t]
    };
    let (q0, q1, q2) = (pq(v0, e0), pq(v1, e1), pq(v2, e2));
    let s = sgn(e0[0] * e2[1] - e0[1] * e2[0]);
    let c0 = [d2(q0), s * (v0[0] * e0[1] - v0[1] * e0[0])];
    let c1 = [d2(q1), s * (v1[0] * e1[1] - v1[1] * e1[0])];
    let c2 = [d2(q2), s * (v2[0] * e2[1] - v2[1] * e2[0])];
    let m = [c0[0].min(c1[0]).min(c2[0]), c0[1].min(c1[1]).min(c2[1])];
    -m[0].sqrt() * sgn(m[1])
}

/// Folds a point into the first sector of a regular `n`-gon whose vertex sits on +X:
/// (distance along the face normal, distance along the face, >= 0).
fn fold_poly(x: f32, y: f32, n: f32) -> (f32, f32) {
    let an = PI / n;
    let bn = modp(y.atan2(x), 2.0 * an) - an;
    let rho = x.hypot(y);
    (rho * bn.cos(), rho * bn.sin().abs())
}

/// Signed distance to a regular `n`-gon with circumradius `r` (a vertex on +X).
fn poly_n(x: f32, y: f32, r: f32, n: f32) -> f32 {
    let an = PI / n;
    let (fx, fy) = fold_poly(x, y, n);
    let px = fx - r * an.cos();
    let ky = r * an.sin();
    let py = fy - ky + (ky - fy).max(0.0).min(ky);
    px.hypot(py) * sgn(px)
}

/// A flat shape (signed distance `d2`, already shrunk by `rr`) pushed to half height `hh`, then grown by `rr`.
fn extrude(d2: f32, z: f32, hh: f32, rr: f32) -> f32 {
    let w0 = d2;
    let w1 = z.abs() - hh + rr;
    w0.max(w1).min(0.0) + w0.max(0.0).hypot(w1.max(0.0)) - rr
}

/// Octahedron with vertices at distance `s` on the axes (exact).
fn octa(q: V3, s: f32) -> f32 {
    let p = [q[0].abs(), q[1].abs(), q[2].abs()];
    let m = p[0] + p[1] + p[2] - s;
    let r = if 3.0 * p[0] < m {
        p
    } else if 3.0 * p[1] < m {
        [p[1], p[2], p[0]]
    } else if 3.0 * p[2] < m {
        [p[2], p[0], p[1]]
    } else {
        return m * 0.577_350_26;
    };
    let k = (0.5 * (r[2] - r[1] + s)).max(0.0).min(s);
    len([r[0], r[1] - s + k, r[2] - k])
}

/// Regular tetrahedron with circumradius `rc`, one vertex straight up (exact).
fn tetra(q: V3, rc: f32) -> f32 {
    let off = rc / 3.0;
    let dn = [TET_S * 0.5, TET_S * SQRT3_2];
    let dpl = (-q[2] - off)
        .max(dn[0] * q[0] + dn[1] * q[1] + q[2] / 3.0 - off)
        .max(-TET_S * q[0] + q[2] / 3.0 - off)
        .max(dn[0] * q[0] - dn[1] * q[1] + q[2] / 3.0 - off);
    if dpl <= 0.0 {
        return dpl;
    }
    let rho = rc * TET_S;
    let t = [0.0, 0.0, rc];
    let b0 = [rho, 0.0, -off];
    let b1 = [-0.5 * rho, SQRT3_2 * rho, -off];
    let b2 = [-0.5 * rho, -SQRT3_2 * rho, -off];
    ud_triangle(q, t, b0, b1).min(ud_triangle(q, t, b1, b2)).min(ud_triangle(q, t, b2, b0)).min(ud_triangle(q, b0, b1, b2))
}

/// Pyramid on a regular `n`-gon with apothem `a`, base at `zb`, apex at `zt` (exact).
fn pyramid(q: V3, n: f32, a: f32, zb: f32, zt: f32) -> f32 {
    let an = PI / n;
    let (fx, fy) = fold_poly(q[0], q[1], n);
    let z = q[2];
    let l = (zt - zb).hypot(a).max(1e-6);
    let din = ((zt - zb) * fx + a * z - a * zt) / l;
    let din = din.max(zb - z);
    if din <= 0.0 {
        return din;
    }
    let e = a * an.tan();
    let tri = ud_triangle([fx, fy, z], [0.0, 0.0, zt], [a, e, zb], [a, -e, zb]);
    let base = len([(fx - a).max(0.0), (fy - e).max(0.0), z - zb]);
    tri.min(base)
}

/// Signed distance to a star outline: `pts` outer points of radius `r`, inner radius `ratio * r`.
fn star2(x: f32, y: f32, r: f32, n: f32, ratio: f32) -> f32 {
    let an = PI / n;
    let bn = (modp(y.atan2(x) + an, 2.0 * an) - an).abs();
    let rho = x.hypot(y);
    let (px, py) = (rho * bn.cos(), rho * bn.sin());
    let e = [ratio * r * an.cos() - r, ratio * r * an.sin()];
    let t = (((px - r) * e[0] + py * e[1]) / (e[0] * e[0] + e[1] * e[1]).max(1e-12)).max(0.0).min(1.0);
    let d = (px - r - e[0] * t).hypot(py - e[1] * t);
    if e[0] * py - e[1] * (px - r) > 0.0 { -d } else { d }
}

/// A cone (base radius `s[0]`, height `s[1]`, point on top) with its edges rounded by `round`:
/// the triangle of its profile, shrunk about its incentre so every side moves in by `rr`, then
/// grown by `rr`. `rr` never exceeds what the cone can take (its incircle radius).
pub fn rounded_cone(q: V3, s: V3, round: f32) -> f32 {
    let (rad, hh) = (s[0], s[1] * 0.5);
    let slant = rad.hypot(s[1]);
    let rin = (rad * s[1] / (rad + slant).max(1e-6)).max(1e-6);
    let rr = round.max(0.0).min(rin * 0.98);
    let k = (rin - rr) / rin;
    let c = [0.0, -hh + rin];
    let g = |x: f32, z: f32| [c[0] + (x - c[0]) * k, c[1] + (z - c[1]) * k];
    sd_triangle([q[0].hypot(q[1]), q[2]], g(-rad, -hh), g(rad, -hh), g(0.0, hh)) - rr
}

/// A cone (base radius `s[0]`, height `s[1]`, point on top) with its two corners (the base rim and
/// the tip) cut off `c` along the sides.
pub fn chamfered_cone(q: V3, s: V3, c: f32) -> f32 {
    let (rad, hh) = (s[0], s[1] * 0.5);
    let p = [q[0].hypot(q[1]), q[2]];
    let (a, b, t) = ([-rad, -hh], [rad, -hh], [0.0, hh]);
    let d = sd_triangle(p, a, b, t);
    let d = corner::cut_vertex(d, p, a, b, t, 1.0, c);
    corner::cut_vertex(d, p, b, t, a, 1.0, c)
}

/// Cuts the symmetric corner at `(r, 0)` (turned so that it is on +X) off a flat shape with distance
/// `d`: the cut runs from `v + leg * (da, db)` to `v + leg * (da, -db)`, a line across X.
fn cut_symmetric_tip(d: f32, px: f32, py: f32, r: f32, leg: f32, da: f32) -> f32 {
    let g = px - (r + leg * da);
    d.max(g.min(leg - (px - r).hypot(py)))
}

/// `(x, y)` turned into the sector of the nearest of `n` corners (corner on +X).
fn fold_to_corner(x: f32, y: f32, n: f32) -> (f32, f32) {
    let an = PI / n;
    let bn = modp(y.atan2(x) + an, 2.0 * an) - an;
    let rho = x.hypot(y);
    (rho * bn.cos(), rho * bn.sin())
}

/// The tip of a regular `n`-gon (circumradius `r`, a vertex on +X) cut off `c` along the sides.
fn cut_poly_tip(d: f32, x: f32, y: f32, r: f32, n: f32, c: f32) -> f32 {
    let an = PI / n;
    let (px, py) = fold_to_corner(x, y, n);
    cut_symmetric_tip(d, px, py, r, c.min(r * an.sin()), -an.sin())
}

/// The outer points of a star (outer radius `r`, inner radius `ratio * r`) cut off `c` along the sides.
fn cut_star_tips(d: f32, x: f32, y: f32, r: f32, n: f32, ratio: f32, c: f32) -> f32 {
    let an = PI / n;
    let e = [ratio * r * an.cos() - r, ratio * r * an.sin()];
    let el = e[0].hypot(e[1]).max(1e-6);
    let (px, py) = fold_to_corner(x, y, n);
    cut_symmetric_tip(d, px, py, r, c.min(el * 0.5), e[0] / el)
}

/// Signed distance to the cap of the ball of radius `rad` (centre at the origin) that lies above the
/// plane `zc = h0`, `h0 >= 0`, seen in the half plane (`rho`, `zc`).
fn ball_cap(rho: f32, zc: f32, rad: f32, h0: f32) -> f32 {
    let dome = rho.hypot(zc) - rad;
    let floor = h0 - zc;
    if dome <= 0.0 && floor <= 0.0 {
        return dome.max(floor);
    }
    let rim = (rad * rad - h0 * h0).max(0.0).sqrt();
    let near_rim = || (rho - rim).hypot(zc - h0);
    if zc >= h0 {
        // the sphere is nearest where the ray from the centre through the point meets the cap
        if zc * rad >= h0 * rho.hypot(zc) { dome } else { near_rim() }
    } else if rho <= rim {
        floor
    } else {
        near_rim()
    }
}

/// Distance from `q` (local, unscaled) to the shape. Negative inside.
pub fn distance(p: &Prepared, q: V3) -> f32 {
    let s = p.size;
    let round = if p.round.is_finite() { p.round.max(0.0) } else { 0.0 };
    let ch = p.chamfer && round > 0.0;
    match p.kind {
        Kind::Prism3 | Kind::PrismN => {
            let n = if p.kind == Kind::Prism3 { 3.0 } else { count(p.params[0], 3.0, 32.0, 6.0) };
            let hh = s[1] * 0.5;
            let an = PI / n;
            if ch {
                let d2 = cut_poly_tip(poly_n(q[0], q[1], s[0], n), q[0], q[1], s[0], n, round);
                return corner::pair(d2, q[2].abs() - hh, round.min(hh));
            }
            let rr = round.min(hh).min(s[0] * an.cos() * 0.98);
            extrude(poly_n(q[0], q[1], s[0] - rr / an.cos(), n), q[2], hh, rr)
        }
        Kind::Octa if ch => {
            // the faces stay; the twelve edges get a flat bevel each
            let c = round.min(s[0] * 0.3);
            let (x, y, z) = (q[0].abs(), q[1].abs(), q[2].abs());
            let f = (x + y + z - s[0]) * 0.577_350_26;
            let e = |a: f32, b: f32| (a + b - s[0]) * S2 + c * corner::S;
            f.max(e(x, y)).max(e(x, z)).max(e(y, z))
        }
        Kind::Octa => {
            let rr = round.min(s[0] * 0.3);
            octa(q, s[0] - rr * 1.732_050_8) - rr
        }
        Kind::Tetra if ch => {
            let c = round.min(s[0] * 0.3);
            let off = s[0] / 3.0;
            let dn = [TET_S * 0.5, TET_S * SQRT3_2];
            let f = [
                -q[2] - off,
                dn[0] * q[0] + dn[1] * q[1] + q[2] / 3.0 - off,
                -TET_S * q[0] + q[2] / 3.0 - off,
                dn[0] * q[0] - dn[1] * q[1] + q[2] / 3.0 - off,
            ];
            // every two faces meet at an edge, and all the angles between the normals are the same
            let mut d = f[0].max(f[1]).max(f[2]).max(f[3]);
            for i in 0..4 {
                for j in (i + 1)..4 {
                    d = d.max(corner::bevel(f[i], f[j], -1.0 / 3.0, c));
                }
            }
            d
        }
        Kind::Tetra => {
            let rr = round.min(s[0] * 0.3);
            tetra(q, s[0] - 3.0 * rr) - rr
        }
        Kind::Pyramid => {
            let n = count(p.params[0], 3.0, 16.0, 4.0);
            let (a, h) = (s[0] * (PI / n).cos(), s[1]);
            let l = h.hypot(a).max(1e-6);
            if ch {
                // the faces stay; the base edges and the slanted edges get a flat bevel each
                let c = round.min(a * h / (l + a) * 0.98);
                let an = PI / n;
                let (fx, fy) = fold_poly(q[0], q[1], n);
                let (zb, zt) = (-h * 0.5, h * 0.5);
                let (kx, kz) = (h / l, a / l);
                let din = (h * fx + a * q[2] - a * zt) / l;
                let dnext = (h * (fx * (2.0 * an).cos() + fy * (2.0 * an).sin()) + a * q[2] - a * zt) / l;
                let db = zb - q[2];
                let d = din.max(db).max(corner::bevel(din, db, -kz, c));
                return d.max(corner::bevel(din, dnext, kx * kx * (2.0 * an).cos() + kz * kz, c));
            }
            let rr = round.min(a * h / (l + a) * 0.98);
            let hh = h * 0.5;
            pyramid(q, n, a - rr * (l + a) / h.max(1e-6), -hh + rr, hh - rr * l / a.max(1e-6)) - rr
        }
        Kind::Capsule => {
            let hl = s[1] * 0.5;
            len([q[0], q[1], q[2] - q[2].max(-hl).min(hl)]) - s[0]
        }
        Kind::Hemi => {
            // a ball of radius r cut at its centre; rounding shrinks the ball by rr, lifts the floor by rr
            // and grows everything again (so the rim becomes a quarter circle)
            let r = s[0];
            if ch {
                // the dome meets the floor at a right angle; the cut is a cone through the rim
                let (rho, zp) = (q[0].hypot(q[1]), q[2] + r * 0.5);
                let c = round.min(r * 0.5);
                return (rho.hypot(zp) - r).max(-zp).max((rho - zp - (r - c)) * corner::S);
            }
            let rr = round.min(r * 0.5);
            ball_cap(q[0].hypot(q[1]), q[2] + r * 0.5, r - rr, rr) - rr
        }
        Kind::Tube => {
            let t = s[1].max(0.0).min(s[0]);
            let (hw, hh) = (t * 0.5, s[2] * 0.5);
            let rr = round.min(hw).min(hh);
            let rho = q[0].hypot(q[1]);
            if ch {
                let dx = if t >= s[0] { rho - s[0] } else { (rho - (s[0] - hw)).abs() - hw };
                return corner::pair(dx, q[2].abs() - hh, rr);
            }
            // a wall as thick as the radius is a solid rod: no inner wall to measure to
            let dx = if t >= s[0] { rho - s[0] + rr } else { (rho - (s[0] - hw)).abs() - hw + rr };
            let d = [dx, q[2].abs() - hh + rr];
            d[0].max(d[1]).min(0.0) + d[0].max(0.0).hypot(d[1].max(0.0)) - rr
        }
        Kind::Wedge => {
            let (w, d, h) = (s[0] * 0.5, s[1] * 0.5, s[2] * 0.5);
            let rin = ((s[0] + s[2] - s[0].hypot(s[2])) * 0.5).max(1e-6);
            if ch {
                let (a, b, c) = ([-w, -h], [w, -h], [-w, h]);
                let pt = [q[0], q[2]];
                let mut d2 = sd_triangle(pt, a, b, c);
                d2 = corner::cut_vertex(d2, pt, c, a, b, 1.0, round);
                d2 = corner::cut_vertex(d2, pt, a, b, c, 1.0, round);
                d2 = corner::cut_vertex(d2, pt, b, c, a, 1.0, round);
                return corner::pair(d2, q[1].abs() - d, round.min(d));
            }
            let rr = round.min(d).min(rin * 0.98);
            // the triangle shrunk about its incentre so that every side moves in by `rr`
            let (cx, cz, k) = (-w + rin, -h + rin, (rin - rr) / rin);
            let g = |x: f32, z: f32| [cx + (x - cx) * k, cz + (z - cz) * k];
            let d2 = sd_triangle([q[0], q[2]], g(-w, -h), g(w, -h), g(-w, h));
            let w0 = d2;
            let w1 = q[1].abs() - d + rr;
            w0.max(w1).min(0.0) + w0.max(0.0).hypot(w1.max(0.0)) - rr
        }
        Kind::Star => {
            let n = count(p.params[0], 3.0, 16.0, 5.0);
            let ratio = if p.params[1].is_finite() { p.params[1].max(0.1).min(0.9) } else { 0.45 };
            let hh = s[1] * 0.5;
            if ch {
                let d2 = cut_star_tips(star2(q[0], q[1], s[0], n, ratio), q[0], q[1], s[0], n, ratio, round);
                return corner::pair(d2, q[2].abs() - hh, round.min(hh));
            }
            let rr = round.min(hh);
            // shrink the star so that every side moves in by `rr` (tips come in along the X axis)
            let an = PI / n;
            let e = [ratio * s[0] * an.cos() - s[0], ratio * s[0] * an.sin()];
            let el = e[0].hypot(e[1]).max(1e-6);
            let rr = rr.min(e[1] * s[0] / el * 0.5);
            let c2 = (e[1] * s[0] - rr * el) / el; // offset line: n . P = c2, n = (e.y, -e.x) / |e|
            let r2 = c2 * el / e[1].max(1e-6);
            let w2 = c2 * el / (e[1] * an.cos() - e[0] * an.sin()).max(1e-6);
            extrude(star2(q[0], q[1], r2, n, w2 / r2.max(1e-6)), q[2], hh, rr)
        }
        _ => len(q) - s[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Material, Scene, Shape};

    const EPS: f32 = 2e-3;

    fn make(kind: Kind, size: [f32; 3], round: f32, params: [f32; 4]) -> Prepared {
        let mut sh = Shape::new(kind, [0.0; 3], Material::default());
        sh.size = size;
        sh.round = round;
        sh.params = params;
        Prepared::new(&sh)
    }

    #[track_caller]
    fn near(a: f32, b: f32) {
        assert!((a - b).abs() <= EPS * (1.0 + b.abs()), "{a} != {b}");
    }

    fn d(p: &Prepared, x: f32, y: f32, z: f32) -> f32 {
        distance(p, [x, y, z])
    }

    #[test]
    fn prism3_and_prismn() {
        let p = make(Kind::Prism3, [22.0, 40.0, 1.0], 0.0, [0.0; 4]);
        near(d(&p, 0.0, 0.0, 0.0), -11.0); // apothem R/2
        near(d(&p, 22.0, 0.0, 0.0), 0.0); // vertex
        near(d(&p, -11.0, 0.0, 0.0), 0.0); // middle of the flat side
        near(d(&p, 0.0, 0.0, 20.0), 0.0);
        near(d(&p, 100.0, 0.0, 0.0), 78.0);
        near(d(&p, 0.0, 0.0, 60.0), 40.0);
        near(d(&p, -111.0, 0.0, 0.0), 100.0);
        // three-fold symmetry
        let (c, s) = (120f32.to_radians().cos(), 120f32.to_radians().sin());
        let (x, y) = (7.0, 3.0);
        near(d(&p, x, y, 5.0), d(&p, c * x - s * y, s * x + c * y, 5.0));
        // a hexagon (params[0] = 6) and the clamps
        let h = make(Kind::PrismN, [20.0, 40.0, 1.0], 0.0, [6.0, 0.0, 0.0, 0.0]);
        let ap = 20.0 * 30f32.to_radians().cos();
        near(d(&h, 0.0, 0.0, 0.0), -ap);
        near(d(&h, 20.0, 0.0, 0.0), 0.0);
        near(d(&h, 30.0, 0.0, 0.0), 10.0);
        near(d(&h, 0.0, ap, 0.0), 0.0);
        near(d(&h, 0.0, ap + 7.0, 0.0), 7.0);
        let sq = make(Kind::PrismN, [20.0, 40.0, 1.0], 0.0, [4.0, 0.0, 0.0, 0.0]);
        near(d(&sq, 0.0, 0.0, 0.0), -(20.0 * 45f32.to_radians().cos()));
        let lo = make(Kind::PrismN, [20.0, 40.0, 1.0], 0.0, [1.0, 0.0, 0.0, 0.0]); // clamps to 3 sides
        near(d(&lo, -10.0, 0.0, 0.0), 0.0);
        let hi = make(Kind::PrismN, [20.0, 40.0, 1.0], 0.0, [500.0, 0.0, 0.0, 0.0]); // 32 sides ~ a cylinder
        assert!((d(&hi, 0.0, 0.0, 0.0) + 20.0 * (PI / 32.0).cos()).abs() < EPS);
        let nan = make(Kind::PrismN, [20.0, 40.0, 1.0], 0.0, [f32::NAN, 0.0, 0.0, 0.0]);
        assert!(d(&nan, 1.0, 2.0, 3.0).is_finite());
    }

    #[test]
    fn rounded_prism_corner() {
        // a square prism (4 sides) of circumradius 22: the corner at (22,0) is cut by a round of 4
        let p = make(Kind::PrismN, [22.0, 40.0, 1.0], 4.0, [4.0, 0.0, 0.0, 0.0]);
        near(d(&p, 32.0, 0.0, 0.0), 10.0 + 4.0 * (2f32.sqrt() - 1.0));
        near(d(&p, 0.0, 0.0, 20.0), 0.0); // flat top stays flat
        // rounding never makes the shape bigger
        let sharp = make(Kind::PrismN, [22.0, 40.0, 1.0], 0.0, [4.0, 0.0, 0.0, 0.0]);
        assert!(d(&p, 21.0, 0.0, 0.0) > d(&sharp, 21.0, 0.0, 0.0));
    }

    #[test]
    fn octa_and_tetra() {
        let o = make(Kind::Octa, [25.0, 1.0, 1.0], 0.0, [0.0; 4]);
        near(d(&o, 0.0, 0.0, 0.0), -25.0 / 3f32.sqrt());
        near(d(&o, 25.0, 0.0, 0.0), 0.0);
        near(d(&o, 0.0, 0.0, -25.0), 0.0);
        near(d(&o, 25.0 / 3.0, 25.0 / 3.0, 25.0 / 3.0), 0.0);
        near(d(&o, 60.0, 0.0, 0.0), 35.0);
        let t = 100.0;
        near(d(&o, t, t, t), (3.0 * t - 25.0) / 3f32.sqrt());
        near(d(&o, 3.0, -4.0, 5.0), d(&o, -5.0, 3.0, 4.0)); // symmetric
        let ro = make(Kind::Octa, [25.0, 1.0, 1.0], 3.0, [0.0; 4]);
        near(d(&ro, 60.0, 0.0, 0.0), 35.0 + 3.0 * 3f32.sqrt() - 3.0);

        let te = make(Kind::Tetra, [25.0, 1.0, 1.0], 0.0, [0.0; 4]);
        near(d(&te, 0.0, 0.0, 0.0), -25.0 / 3.0);
        near(d(&te, 0.0, 0.0, 25.0), 0.0); // apex
        near(d(&te, 0.0, 0.0, -25.0 / 3.0), 0.0); // middle of the base
        near(d(&te, 0.0, 0.0, 75.0), 50.0);
        near(d(&te, 0.0, 0.0, -25.0 / 3.0 - 10.0), 10.0);
        near(d(&te, 25.0 * TET_S, 0.0, -25.0 / 3.0), 0.0); // a base vertex
        near(d(&te, 25.0 * TET_S + 10.0, 0.0, -25.0 / 3.0), 10.0);
        // three-fold symmetry about Z
        let (c, s) = (120f32.to_radians().cos(), 120f32.to_radians().sin());
        near(d(&te, 6.0, 2.0, -3.0), d(&te, c * 6.0 - s * 2.0, s * 6.0 + c * 2.0, -3.0));
        let rt = make(Kind::Tetra, [25.0, 1.0, 1.0], 2.0, [0.0; 4]);
        near(d(&rt, 0.0, 0.0, 0.0), -25.0 / 3.0); // the faces stay where they were
        near(d(&rt, 0.0, 0.0, -25.0 / 3.0 - 10.0), 10.0);
    }

    #[test]
    fn pyramid() {
        let p = make(Kind::Pyramid, [22.0, 40.0, 1.0], 0.0, [4.0, 0.0, 0.0, 0.0]);
        let a = 22.0 * 45f32.to_radians().cos();
        let l = 40f32.hypot(a);
        near(d(&p, 0.0, 0.0, 20.0), 0.0); // apex
        near(d(&p, 0.0, 0.0, -20.0), 0.0); // middle of the base
        near(d(&p, 22.0, 0.0, -20.0), 0.0); // base corner
        let m = a * 0.5 * 45f32.to_radians().cos();
        near(d(&p, m, m, 0.0), 0.0); // middle of a slanted face at half height
        near(d(&p, 11.0, 0.0, 0.0), 0.0); // a slanted edge at half height
        near(d(&p, 0.0, 0.0, 0.0), -a * 20.0 / l); // face is closer than the base
        near(d(&p, 0.0, 0.0, 50.0), 30.0);
        near(d(&p, 0.0, 0.0, -30.0), 10.0);
        near(d(&p, 32.0, 0.0, -20.0), 10.0); // beyond a base corner
        let e = (a + 8.0) * 45f32.to_radians().cos();
        near(d(&p, e, e, -20.0), 8.0); // beyond a base edge, in the base plane
        // four-fold symmetry
        near(d(&p, 5.0, 2.0, 3.0), d(&p, -2.0, 5.0, 3.0));
        // other side counts, clamps
        let t = make(Kind::Pyramid, [22.0, 40.0, 1.0], 0.0, [3.0, 0.0, 0.0, 0.0]);
        near(d(&t, 0.0, 0.0, 20.0), 0.0);
        near(d(&t, 0.0, 0.0, -20.0), 0.0);
        near(d(&t, 0.0, 0.0, 0.0), -(11.0 * 20.0 / 40f32.hypot(11.0)));
        let lo = make(Kind::Pyramid, [22.0, 40.0, 1.0], 0.0, [0.0, 0.0, 0.0, 0.0]);
        near(d(&lo, 0.0, 0.0, 0.0), d(&t, 0.0, 0.0, 0.0));
        let hi = make(Kind::Pyramid, [22.0, 40.0, 1.0], 0.0, [99.0, 0.0, 0.0, 0.0]);
        assert!(d(&hi, 0.0, 0.0, 0.0) < 0.0 && d(&hi, 0.0, 0.0, 20.0).abs() < EPS);
        // rounding: the apex is blunted by about r*(L/a - 1) and the base corner pulled in
        let r = make(Kind::Pyramid, [22.0, 40.0, 1.0], 3.0, [4.0, 0.0, 0.0, 0.0]);
        assert!(d(&r, 0.0, 0.0, 20.0) > 0.5);
        assert!(d(&r, 22.0, 0.0, -20.0) > 0.5);
        near(d(&r, 0.0, 0.0, -30.0), 10.0);
        let big = make(Kind::Pyramid, [22.0, 40.0, 1.0], 1e6, [4.0, 0.0, 0.0, 0.0]);
        assert!(d(&big, 1.0, 2.0, 3.0).is_finite());
    }

    #[test]
    fn capsule_hemi_tube() {
        let c = make(Kind::Capsule, [10.0, 30.0, 1.0], 0.0, [0.0; 4]);
        near(d(&c, 0.0, 0.0, 0.0), -10.0);
        near(d(&c, 0.0, 0.0, 25.0), 0.0);
        near(d(&c, 0.0, 0.0, -25.0), 0.0);
        near(d(&c, 10.0, 0.0, 5.0), 0.0);
        near(d(&c, 50.0, 0.0, 0.0), 40.0);
        near(d(&c, 0.0, 0.0, 65.0), 40.0);
        near(d(&c, 3.0, 4.0, 2.0), d(&c, -5.0, 0.0, -2.0));

        let h = make(Kind::Hemi, [22.0, 1.0, 1.0], 0.0, [0.0; 4]);
        near(d(&h, 0.0, 0.0, 11.0), 0.0); // dome top
        near(d(&h, 0.0, 0.0, -11.0), 0.0); // flat bottom
        near(d(&h, 0.0, 0.0, 0.0), -11.0);
        near(d(&h, 22.0, 0.0, -11.0), 0.0); // rim
        near(d(&h, 0.0, 0.0, 50.0), 39.0);
        near(d(&h, 0.0, 0.0, -21.0), 10.0);
        near(d(&h, 40.0, 0.0, -11.0), 18.0);
        near(d(&h, 40.0, 0.0, -21.0), 18.0f32.hypot(10.0));
        near(d(&h, 22.0 * 0.6, 0.0, -11.0 + 22.0 * 0.8), 0.0); // on the dome
        near(d(&h, 5.0, 0.0, -11.0 + 4.0), d(&h, 0.0, -5.0, -11.0 + 4.0));

        let t = make(Kind::Tube, [18.0, 4.0, 40.0], 0.0, [0.0; 4]);
        near(d(&t, 0.0, 0.0, 0.0), 14.0); // in the hole
        near(d(&t, 16.0, 0.0, 0.0), -2.0);
        near(d(&t, 18.0, 0.0, 0.0), 0.0);
        near(d(&t, 14.0, 0.0, 0.0), 0.0);
        near(d(&t, 16.0, 0.0, 20.0), 0.0);
        near(d(&t, 50.0, 0.0, 0.0), 32.0);
        near(d(&t, 16.0, 0.0, 30.0), 10.0);
        near(d(&t, 0.0, -16.0, 0.0), -2.0);
        let r = make(Kind::Tube, [18.0, 4.0, 40.0], 1.5, [0.0; 4]);
        assert!(d(&r, 18.0, 0.0, 20.0) > 0.3); // outer corner rounded off
        near(d(&r, 16.0, 0.0, 0.0), -2.0);
        let thick = make(Kind::Tube, [18.0, 50.0, 40.0], 0.0, [0.0; 4]); // wall thicker than the radius = solid rod
        near(d(&thick, 0.0, 0.0, 0.0), -18.0);
    }

    #[test]
    fn wedge_and_star() {
        let w = make(Kind::Wedge, [30.0, 30.0, 30.0], 0.0, [0.0; 4]);
        near(d(&w, -15.0, 0.0, 0.0), 0.0); // the tall side
        near(d(&w, 0.0, 0.0, -15.0), 0.0); // the floor
        near(d(&w, 0.0, 0.0, 0.0), 0.0); // the slope
        near(d(&w, -5.0, 0.0, -5.0), -(5.0 + 5.0) / 2f32.sqrt());
        near(d(&w, -14.0, 0.0, -14.0), -1.0);
        near(d(&w, 50.0, 0.0, -15.0), 35.0);
        near(d(&w, 50.0, 0.0, -25.0), 35.0f32.hypot(10.0));
        near(d(&w, -15.0, 25.0, 15.0), 10.0);
        near(d(&w, 5.0, 3.0, -4.0), d(&w, 5.0, -3.0, -4.0));
        near(d(&w, 15.0, 0.0, 10.0), 25.0 / 2f32.sqrt()); // straight out from the slope
        let r = make(Kind::Wedge, [30.0, 30.0, 30.0], 3.0, [0.0; 4]);
        assert!(d(&r, -15.0, 15.0, 15.0) > 1.0); // a rounded corner
        near(d(&r, -5.0, 0.0, -5.0), -(5.0 + 5.0) / 2f32.sqrt());

        let s = make(Kind::Star, [24.0, 14.0, 1.0], 0.0, [5.0, 0.45, 0.0, 0.0]);
        near(d(&s, 24.0, 0.0, 0.0), 0.0); // an outer point
        let an = 36f32.to_radians();
        near(d(&s, 0.45 * 24.0 * an.cos(), 0.45 * 24.0 * an.sin(), 0.0), 0.0); // an inner corner
        near(d(&s, 34.0, 0.0, 0.0), 10.0);
        near(d(&s, 0.0, 0.0, 7.0), 0.0);
        near(d(&s, 0.0, 0.0, 17.0), 10.0);
        near(d(&s, 0.0, 0.0, 0.0), -7.0); // the flat top and bottom are nearer than the sides
        near(d(&s, 0.0, 0.0, 6.0), -1.0);
        // five-fold and mirror symmetry
        let a72 = 72f32.to_radians();
        let (x, y) = (11.0, 4.0);
        near(d(&s, x, y, 2.0), d(&s, a72.cos() * x - a72.sin() * y, a72.sin() * x + a72.cos() * y, 2.0));
        near(d(&s, x, y, 2.0), d(&s, x, -y, 2.0));
        // more points make a different star; clamps keep it finite
        let s8 = make(Kind::Star, [24.0, 14.0, 1.0], 0.0, [8.0, 0.5, 0.0, 0.0]);
        near(d(&s8, 24.0, 0.0, 0.0), 0.0);
        let sx = make(Kind::Star, [24.0, 14.0, 1.0], 0.0, [1000.0, 50.0, 0.0, 0.0]);
        near(d(&sx, 24.0, 0.0, 0.0), 0.0);
        assert!(d(&sx, 3.0, 4.0, 5.0).is_finite());
        let sr = make(Kind::Star, [24.0, 14.0, 1.0], 2.0, [5.0, 0.45, 0.0, 0.0]);
        assert!(d(&sr, 24.0, 0.0, 0.0) > 0.5); // the tip is rounded off
        near(d(&sr, 0.0, 0.0, 17.0), 10.0);
    }

    /// A tiny deterministic random generator for the property tests.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32) / (1u64 << 24) as f32
        }
        fn pt(&mut self, r: f32) -> V3 {
            [(self.next() - 0.5) * 2.0 * r, (self.next() - 0.5) * 2.0 * r, (self.next() - 0.5) * 2.0 * r]
        }
    }

    #[test]
    fn every_more_kind_is_finite_and_never_overestimates() {
        for &k in &Kind::MORE {
            for round in [0.0, 2.5] {
                let mut sh = Shape::new(k, [0.0; 3], Material::default());
                sh.round = round;
                let p = Prepared::new(&sh);
                // finite on a grid (including the origin and the axes)
                for i in -8..=8 {
                    for j in -8..=8 {
                        for l in -8..=8 {
                            let v = distance(&p, [i as f32 * 7.0, j as f32 * 7.0, l as f32 * 7.0]);
                            assert!(v.is_finite(), "{k:?} gave {v}");
                        }
                    }
                }
                // centre is inside, far away is outside
                let inside = match k {
                    Kind::Wedge => [-5.0, 0.0, -5.0],
                    Kind::Tube => [16.0, 0.0, 0.0],
                    _ => [0.0; 3],
                };
                assert!(distance(&p, inside) < 0.0, "{k:?}");
                assert!(distance(&p, [500.0, 300.0, 200.0]) > 100.0, "{k:?}");
                // 1-Lipschitz: the sphere tracer can trust it
                let mut g = Lcg(7 + k.id() as u64);
                for _ in 0..4000 {
                    let (a, b) = (g.pt(60.0), g.pt(60.0));
                    let (da, db) = (distance(&p, a), distance(&p, b));
                    let dist = len(sub(a, b));
                    assert!((da - db).abs() <= dist * 1.001 + 1e-3, "{k:?} round {round}: {da} vs {db} over {dist}");
                }
            }
        }
    }

    #[test]
    fn hostile_numbers_never_panic() {
        let weird = [0.0, -5.0, 1e-9, 1e12, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        for &k in &Kind::MORE {
            for &a in &weird {
                for &b in &weird {
                    let p = make(k, [a, b, a], b, [a, b, 0.0, 0.0]);
                    for q in [[0.0; 3], [1.0, 2.0, 3.0], [a, b, a]] {
                        let _ = distance(&p, q); // must not panic (the value may be anything)
                    }
                }
            }
        }
    }

    #[test]
    fn scenes_with_every_new_kind_round_trip() {
        let mut sc = Scene::default();
        for (i, &k) in Kind::ALL.iter().enumerate() {
            let mut sh = Shape::new(k, [i as f32 * 10.0, 0.0, 0.0], Material::default());
            sh.round = 1.5;
            sc.groups[0].shapes.push(sh);
        }
        let back = Scene::from_json(&sc.to_json()).unwrap_or_default();
        assert_eq!(back, sc);
        for &k in &Kind::MORE {
            let text = sc.to_json().replace("\"Sphere\"", &format!("\"{k:?}\""));
            let t = Scene::from_json(&text).unwrap_or_default();
            assert_eq!(t.groups[0].shapes[0].kind, k);
        }
    }
}
