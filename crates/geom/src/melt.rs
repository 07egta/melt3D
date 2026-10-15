//! Melt: melt closed triangle meshes together (a soft union), or carve one out of another with a
//! soft edge (a soft cut), the way clay or liquid flows into itself.
//!
//! Each mesh becomes a signed distance field on a grid (negative inside). Fields are combined
//! with a smooth minimum whose reach is the blend strength in mm, and the zero level of the
//! result is meshed again with marching tetrahedra. Tetrahedra, unlike marching cubes, have no
//! ambiguous cases, so the result is closed and manifold wherever the field is.
//!
//! The mesh that comes back is a new, baked shape: its size follows the grid, not the inputs'
//! triangles. Units are millimetres.

use std::collections::HashMap;

/// Largest number of grid points. A finer grid is refused rather than slowing the app down.
pub const MAX_GRID_POINTS: usize = 12_000_000;

/// A triangle mesh. Triangles are counter-clockwise seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    pub positions: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// How to melt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Blend reach in mm: 0 is a sharp union or cut; the bigger, the more the shapes flow.
    /// The grid also reaches this far past the shapes on every side, so a big reach costs memory.
    pub strength: f64,
    /// Cells along the longest side of the shapes' box (the cell size is that side over this).
    /// Clamped to 16..=400. Doubling it roughly octuples the time and memory.
    pub resolution: usize,
    /// Cut the later meshes out of the first one instead of joining them.
    pub cut: bool,
    /// Rounds of smoothing the result's surface (0: none). It evens out the grid's facets.
    pub smooth: usize,
}

impl Default for Params {
    fn default() -> Self {
        Params { strength: 5.0, resolution: 96, cut: false, smooth: 2 }
    }
}

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The regular grid the fields live on: `n` points per axis, `h` apart, starting at `origin`.
struct Grid {
    origin: V,
    h: f64,
    n: [usize; 3],
}

impl Grid {
    fn len(&self) -> usize {
        self.n[0] * self.n[1] * self.n[2]
    }
    fn idx(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.n[1] + j) * self.n[0] + i
    }
    fn pos(&self, i: usize, j: usize, k: usize) -> V {
        [self.origin[0] + i as f64 * self.h, self.origin[1] + j as f64 * self.h, self.origin[2] + k as f64 * self.h]
    }
    /// The grid index range along `axis` that covers `lo..=hi` (clamped to the grid).
    fn range(&self, axis: usize, lo: f64, hi: f64) -> (usize, usize) {
        let last = self.n[axis].saturating_sub(1) as f64;
        let a = ((lo - self.origin[axis]) / self.h).floor().clamp(0.0, last);
        let b = ((hi - self.origin[axis]) / self.h).ceil().clamp(0.0, last);
        (a as usize, b as usize)
    }
}

/// Melt `meshes` together (or, with `params.cut`, cut the rest out of the first).
pub fn melt(meshes: &[TriMesh], params: &Params) -> Result<TriMesh, String> {
    if meshes.is_empty() || meshes.iter().any(|m| m.triangles.is_empty()) {
        return Err("every shape needs a closed mesh with triangles".into());
    }
    if !params.strength.is_finite() || params.strength < 0.0 {
        return Err("the strength must be zero or more".into());
    }
    if params.cut && meshes.len() < 2 {
        return Err("a cut needs a shape to cut and at least one to cut with".into());
    }
    if !params.cut && meshes.len() < 2 {
        return Err("choose two or more shapes to melt together".into());
    }
    let resolution = params.resolution.clamp(16, 400);
    for m in meshes {
        let n = u32::try_from(m.positions.len()).map_err(|_| "a mesh is too large".to_string())?;
        if m.positions.iter().flatten().any(|c| !c.is_finite()) || m.triangles.iter().flatten().any(|i| *i >= n) {
            return Err("a mesh has broken coordinates or indices".into());
        }
    }
    // The box around everything, with room for the melted surface to bulge out.
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in meshes.iter().flat_map(|m| m.positions.iter()) {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
    if extent <= 1e-9 {
        return Err("the shapes have no size".into());
    }
    if !extent.is_finite() {
        return Err("the shapes are too big to melt (their coordinates overflow)".into());
    }
    let too_big = || "the shapes are too big for this resolution; lower it or the strength".to_string();
    let h = extent / resolution as f64;
    let margin = params.strength + 3.0 * h;
    // Checked before any cast or multiplication: on 32-bit targets (wasm) `usize` is small, and a
    // huge strength must not wrap around into a grid that looks small enough.
    let mut n = [0usize; 3];
    for a in 0..3 {
        let cells = ((hi[a] - lo[a] + 2.0 * margin) / h).ceil();
        if cells.is_nan() || cells + 1.0 > MAX_GRID_POINTS as f64 {
            return Err(too_big());
        }
        n[a] = cells as usize + 1;
    }
    let points = n[0].checked_mul(n[1]).and_then(|p| p.checked_mul(n[2])).ok_or_else(too_big)?;
    if points > MAX_GRID_POINTS {
        return Err(too_big());
    }
    let grid = Grid { origin: [lo[0] - margin, lo[1] - margin, lo[2] - margin], h, n };
    let mut field = signed_distance(&grid, &meshes[0])?;
    for m in meshes.iter().skip(1) {
        let other = signed_distance(&grid, m)?;
        for (f, o) in field.iter_mut().zip(other.iter()) {
            *f = if params.cut { -smooth_min(-*f, *o, params.strength as f32) } else { smooth_min(*f, *o, params.strength as f32) };
        }
    }
    let mut out = marching_tetrahedra(&grid, &field);
    smooth_surface(&mut out, params.smooth);
    if out.triangles.is_empty() {
        return Err(if params.cut {
            "the cut removes all of the first shape, so nothing is left".into()
        } else {
            "nothing is left after melting: the shapes have no solid inside".into()
        });
    }
    Ok(out)
}

/// A smooth minimum of two distances: `min(a, b)` with the crease rounded over a reach of `k`.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// The point of triangle `abc` nearest to `p` (Ericson, Real-Time Collision Detection).
fn closest_on_triangle(p: V, a: V, b: V, c: V) -> V {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return add(a, mul(ab, v));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return add(a, mul(ac, w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let sum = va + vb + vc;
    if sum.abs() < 1e-300 {
        return a;
    }
    let denom = 1.0 / sum;
    add(a, add(mul(ab, vb * denom), mul(ac, vc * denom)))
}

fn corners(mesh: &TriMesh) -> Vec<[V; 3]> {
    mesh.triangles
        .iter()
        .filter_map(|t| {
            let a = *mesh.positions.get(t[0] as usize)?;
            let b = *mesh.positions.get(t[1] as usize)?;
            let c = *mesh.positions.get(t[2] as usize)?;
            Some([a, b, c])
        })
        .collect()
}

/// The signed distance (negative inside) of a closed mesh at every grid point.
fn signed_distance(grid: &Grid, mesh: &TriMesh) -> Result<Vec<f32>, String> {
    let tris = corners(mesh);
    let unsigned = unsigned_distance(grid, &tris);
    let inside = inside_flags(grid, &tris);
    Ok(unsigned.iter().zip(inside.iter()).map(|(d, inside)| if *inside { -*d } else { *d }).collect())
}

const BIG: f32 = 1e20;

/// Distances to the surface: exact near it, spread outwards by fast sweeping.
fn unsigned_distance(grid: &Grid, tris: &[[V; 3]]) -> Vec<f32> {
    let mut d = vec![BIG; grid.len()];
    let mut fixed = vec![false; grid.len()];
    let reach = 1.8 * grid.h;
    for t in tris {
        let mut lo = t[0];
        let mut hi = t[0];
        for p in &t[1..] {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        let (i0, i1) = grid.range(0, lo[0] - reach, hi[0] + reach);
        let (j0, j1) = grid.range(1, lo[1] - reach, hi[1] + reach);
        let (k0, k1) = grid.range(2, lo[2] - reach, hi[2] + reach);
        for k in k0..=k1 {
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let p = grid.pos(i, j, k);
                    let q = closest_on_triangle(p, t[0], t[1], t[2]);
                    let dist = dot(sub(p, q), sub(p, q)).sqrt() as f32;
                    let id = grid.idx(i, j, k);
                    if let (Some(slot), Some(flag)) = (d.get_mut(id), fixed.get_mut(id)) {
                        if dist < *slot {
                            *slot = dist;
                        }
                        if f64::from(dist) <= reach {
                            *flag = true;
                        }
                    }
                }
            }
        }
    }
    // Points the triangles did not reach exactly start as unknown, then get the distance of
    // the nearest known ones plus the way across, from every corner of the grid in turn.
    for (slot, flag) in d.iter_mut().zip(fixed.iter()) {
        if !*flag {
            *slot = BIG;
        }
    }
    sweep(&mut d, &fixed, grid, 2);
    d
}

fn value(d: &[f32], n: [usize; 3], i: isize, j: isize, k: isize) -> f32 {
    if i < 0 || j < 0 || k < 0 || i as usize >= n[0] || j as usize >= n[1] || k as usize >= n[2] {
        return BIG;
    }
    d.get((k as usize * n[1] + j as usize) * n[0] + i as usize).copied().unwrap_or(BIG)
}

/// The Eikonal update: the distance at a point whose neighbours along each axis have `v`.
fn eikonal(mut v: [f32; 3], h: f32) -> f32 {
    v.sort_by(|x, y| x.total_cmp(y));
    let (a, b, c) = (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
    let h = f64::from(h);
    if v[0] >= BIG {
        return BIG;
    }
    let mut x = a + h;
    if v[1] < BIG && x > b {
        let disc = 2.0 * h * h - (a - b) * (a - b);
        if disc > 0.0 {
            x = (a + b + disc.sqrt()) * 0.5;
        }
        if v[2] < BIG && x > c {
            let s = a + b + c;
            let disc3 = s * s - 3.0 * (a * a + b * b + c * c - h * h);
            if disc3 > 0.0 {
                x = (s + disc3.sqrt()) / 3.0;
            }
        }
    }
    x as f32
}

/// Fast sweeping: relax the distances in all eight diagonal orders, `passes` times.
fn sweep(d: &mut [f32], fixed: &[bool], grid: &Grid, passes: usize) {
    let n = grid.n;
    let h = grid.h as f32;
    let order = |len: usize, up: bool| -> Vec<usize> { if up { (0..len).collect() } else { (0..len).rev().collect() } };
    for _ in 0..passes {
        for dir in 0..8u8 {
            let (xs, ys, zs) = (order(n[0], dir & 1 == 0), order(n[1], dir & 2 == 0), order(n[2], dir & 4 == 0));
            for &k in &zs {
                for &j in &ys {
                    for &i in &xs {
                        let id = grid.idx(i, j, k);
                        if fixed.get(id).copied().unwrap_or(true) {
                            continue;
                        }
                        let (ii, jj, kk) = (i as isize, j as isize, k as isize);
                        let a = value(d, n, ii - 1, jj, kk).min(value(d, n, ii + 1, jj, kk));
                        let b = value(d, n, ii, jj - 1, kk).min(value(d, n, ii, jj + 1, kk));
                        let c = value(d, n, ii, jj, kk - 1).min(value(d, n, ii, jj, kk + 1));
                        let x = eikonal([a, b, c], h);
                        if let Some(slot) = d.get_mut(id)
                            && x < *slot
                        {
                            *slot = x;
                        }
                    }
                }
            }
        }
    }
}

/// Which grid points are inside the mesh: along every row of points in x, count the triangles
/// crossed before each point. A row is nudged off the triangles' edges so that none is hit twice.
fn inside_flags(grid: &Grid, tris: &[[V; 3]]) -> Vec<bool> {
    let n = grid.n;
    let mut rows: Vec<Vec<f32>> = vec![Vec::new(); n[1] * n[2]];
    let (ey, ez) = (grid.h * 1.234_567e-5, grid.h * 2.345_679e-5);
    for t in tris {
        let normal = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        if normal[0].abs() < 1e-14 {
            continue;
        }
        let ys = [t[0][1], t[1][1], t[2][1]];
        let zs = [t[0][2], t[1][2], t[2][2]];
        let (ylo, yhi) = (ys.iter().copied().fold(f64::INFINITY, f64::min), ys.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let (zlo, zhi) = (zs.iter().copied().fold(f64::INFINITY, f64::min), zs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let (j0, j1) = grid.range(1, ylo - 2.0 * ey, yhi + 2.0 * ey);
        let (k0, k1) = grid.range(2, zlo - 2.0 * ez, zhi + 2.0 * ez);
        // Twice the signed area of the triangle seen along x.
        let area = (ys[1] - ys[0]) * (zs[2] - zs[0]) - (ys[2] - ys[0]) * (zs[1] - zs[0]);
        if area.abs() < 1e-18 {
            continue;
        }
        for k in k0..=k1 {
            for j in j0..=j1 {
                let (y, z) = (grid.origin[1] + j as f64 * grid.h + ey, grid.origin[2] + k as f64 * grid.h + ez);
                // Barycentric coordinates of (y, z) in the projected triangle.
                let w1 = ((y - ys[0]) * (zs[2] - zs[0]) - (ys[2] - ys[0]) * (z - zs[0])) / area;
                let w2 = ((ys[1] - ys[0]) * (z - zs[0]) - (y - ys[0]) * (zs[1] - zs[0])) / area;
                let w0 = 1.0 - w1 - w2;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let x = w0 * t[0][0] + w1 * t[1][0] + w2 * t[2][0];
                if let Some(row) = rows.get_mut(k * n[1] + j) {
                    row.push(x as f32);
                }
            }
        }
    }
    let mut inside = vec![false; grid.len()];
    for k in 0..n[2] {
        for j in 0..n[1] {
            let Some(row) = rows.get_mut(k * n[1] + j) else { continue };
            row.sort_by(|a, b| a.total_cmp(b));
            let mut crossed = 0usize;
            for i in 0..n[0] {
                let x = (grid.origin[0] + i as f64 * grid.h) as f32;
                while row.get(crossed).is_some_and(|c| *c < x) {
                    crossed += 1;
                }
                if crossed % 2 == 1
                    && let Some(flag) = inside.get_mut(grid.idx(i, j, k))
                {
                    *flag = true;
                }
            }
        }
    }
    inside
}

/// Mesh the zero level of a ready-made field: `n` points per axis, `h` mm apart from `origin`,
/// x fastest, then y, then z. A value below zero is inside. `smooth` rounds of Taubin smoothing.
pub fn polygonize(origin: [f64; 3], h: f64, n: [usize; 3], field: &[f32], smooth: usize) -> Result<TriMesh, String> {
    if !origin.iter().all(|c| c.is_finite()) || !h.is_finite() || h <= 0.0 {
        return Err("the grid needs a finite origin and a cell size above zero".into());
    }
    if n.iter().any(|c| *c < 2) {
        return Err("the grid needs at least two points along every axis".into());
    }
    let too_big = || "the grid is too big; lower the resolution".to_string();
    let points = n[0].checked_mul(n[1]).and_then(|p| p.checked_mul(n[2])).ok_or_else(too_big)?;
    if points > MAX_GRID_POINTS {
        return Err(too_big());
    }
    if field.len() != points {
        return Err("the field does not match the grid size".into());
    }
    let grid = Grid { origin, h, n };
    let mut out = marching_tetrahedra(&grid, field);
    smooth_surface(&mut out, smooth);
    if out.triangles.is_empty() {
        return Err("nothing is solid: the field has no inside".into());
    }
    Ok(out)
}

/// Corner offsets of a grid cell, and the six tetrahedra around its main diagonal 0–6.
const CORNERS: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]];
const TETS: [[usize; 4]; 6] = [[0, 1, 2, 6], [0, 1, 5, 6], [0, 3, 2, 6], [0, 3, 7, 6], [0, 4, 5, 6], [0, 4, 7, 6]];

/// The zero level of `field` as a triangle mesh, facing outwards (towards positive values).
fn marching_tetrahedra(grid: &Grid, field: &[f32]) -> TriMesh {
    let mut out = TriMesh::default();
    let mut verts: HashMap<(u32, u32), u32> = HashMap::new();
    let n = grid.n;
    if n.iter().any(|c| *c < 2) {
        return out;
    }
    for k in 0..n[2] - 1 {
        for j in 0..n[1] - 1 {
            for i in 0..n[0] - 1 {
                let mut ids = [0usize; 8];
                let mut f = [0f32; 8];
                for (c, o) in CORNERS.iter().enumerate() {
                    let id = grid.idx(i + o[0], j + o[1], k + o[2]);
                    ids[c] = id;
                    // Exactly zero counts as a hair outside, so that no vertex sits on a point.
                    let v = field.get(id).copied().unwrap_or(1.0);
                    f[c] = if v == 0.0 { 1e-6 } else { v };
                }
                if f.iter().all(|v| *v > 0.0) || f.iter().all(|v| *v < 0.0) {
                    continue;
                }
                for tet in &TETS {
                    let inside: Vec<usize> = tet.iter().copied().filter(|c| f[*c] < 0.0).collect();
                    let outside: Vec<usize> = tet.iter().copied().filter(|c| f[*c] >= 0.0).collect();
                    if inside.is_empty() || outside.is_empty() {
                        continue;
                    }
                    let mut vertex = |a: usize, b: usize| -> u32 {
                        let (ia, ib) = (ids[a] as u32, ids[b] as u32);
                        let key = if ia < ib { (ia, ib) } else { (ib, ia) };
                        if let Some(v) = verts.get(&key) {
                            return *v;
                        }
                        let (pa, pb) = (corner_pos(grid, i, j, k, a), corner_pos(grid, i, j, k, b));
                        let (fa, fb) = (f64::from(f[a]), f64::from(f[b]));
                        let t = if (fa - fb).abs() < 1e-30 { 0.5 } else { (fa / (fa - fb)).clamp(0.0, 1.0) };
                        let v = out.positions.len() as u32;
                        out.positions.push(add(pa, mul(sub(pb, pa), t)));
                        verts.insert(key, v);
                        v
                    };
                    // Each triangle also carries the midpoints of the grid edges its vertices sit on.
                    // They decide which way it faces: unlike the vertices (which can crowd onto a
                    // single point where the shape has a tip), they are always well apart.
                    let mid = |a: usize, b: usize| mul(add(corner_pos(grid, i, j, k, a), corner_pos(grid, i, j, k, b)), 0.5);
                    let mut tris: Vec<([u32; 3], [V; 3])> = Vec::with_capacity(2);
                    match (inside.len(), outside.len()) {
                        (1, 3) => {
                            let a = inside[0];
                            tris.push((
                                [vertex(a, outside[0]), vertex(a, outside[1]), vertex(a, outside[2])],
                                [mid(a, outside[0]), mid(a, outside[1]), mid(a, outside[2])],
                            ));
                        }
                        (3, 1) => {
                            let a = outside[0];
                            tris.push((
                                [vertex(a, inside[0]), vertex(a, inside[1]), vertex(a, inside[2])],
                                [mid(a, inside[0]), mid(a, inside[1]), mid(a, inside[2])],
                            ));
                        }
                        (2, 2) => {
                            let (a, b, c, d) = (inside[0], inside[1], outside[0], outside[1]);
                            let (ac, ad, bd, bc) = (vertex(a, c), vertex(a, d), vertex(b, d), vertex(b, c));
                            let (mac, mad, mbd, mbc) = (mid(a, c), mid(a, d), mid(b, d), mid(b, c));
                            tris.push(([ac, ad, bd], [mac, mad, mbd]));
                            tris.push(([ac, bd, bc], [mac, mbd, mbc]));
                        }
                        _ => {}
                    }
                    // Face them towards the outside points of this tetrahedron.
                    let toward = {
                        let mut c = [0.0; 3];
                        for o in &outside {
                            c = add(c, corner_pos(grid, i, j, k, *o));
                        }
                        mul(c, 1.0 / outside.len() as f64)
                    };
                    for (mut t, q) in tris {
                        let normal = cross(sub(q[1], q[0]), sub(q[2], q[0]));
                        let centre = mul(add(add(q[0], q[1]), q[2]), 1.0 / 3.0);
                        if dot(normal, sub(toward, centre)) < 0.0 {
                            t.swap(1, 2);
                        }
                        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                            out.triangles.push(t);
                        }
                    }
                }
            }
        }
    }
    out
}

fn corner_pos(grid: &Grid, i: usize, j: usize, k: usize, corner: usize) -> V {
    let o = CORNERS.get(corner).copied().unwrap_or([0, 0, 0]);
    grid.pos(i + o[0], j + o[1], k + o[2])
}

/// Even out the grid's facets: alternate shrinking and inflating passes (Taubin), which keep the
/// volume, over the mesh's own vertex neighbours.
fn smooth_surface(mesh: &mut TriMesh, rounds: usize) {
    if rounds == 0 || mesh.positions.is_empty() {
        return;
    }
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); mesh.positions.len()];
    for t in &mesh.triangles {
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            if let Some(list) = neighbours.get_mut(a as usize)
                && !list.contains(&b)
            {
                list.push(b);
            }
            if let Some(list) = neighbours.get_mut(b as usize)
                && !list.contains(&a)
            {
                list.push(a);
            }
        }
    }
    for _ in 0..rounds {
        for factor in [0.5, -0.53] {
            let before = mesh.positions.clone();
            for (v, list) in neighbours.iter().enumerate() {
                if list.is_empty() {
                    continue;
                }
                let mut mean = [0.0; 3];
                for nb in list {
                    if let Some(p) = before.get(*nb as usize) {
                        mean = add(mean, *p);
                    }
                }
                mean = mul(mean, 1.0 / list.len() as f64);
                if let (Some(p), Some(slot)) = (before.get(v), mesh.positions.get_mut(v)) {
                    *slot = add(*p, mul(sub(mean, *p), factor));
                }
            }
        }
    }
}

/// The volume a closed mesh holds (mm³), by the divergence theorem.
pub fn volume(mesh: &TriMesh) -> f64 {
    corners(mesh).iter().map(|t| dot(t[0], cross(t[1], t[2])) / 6.0).sum()
}

/// Whether every edge of the mesh is shared by exactly two triangles, once in each direction.
pub fn is_watertight(mesh: &TriMesh) -> bool {
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for t in &mesh.triangles {
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            *edges.entry((a, b)).or_insert(0) += 1;
        }
    }
    edges.iter().all(|((a, b), count)| *count == 1 && edges.get(&(*b, *a)) == Some(&1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A latitude-longitude sphere around `centre`, facing outwards.
    fn sphere(centre: V, r: f64, rings: usize, sectors: usize) -> TriMesh {
        let mut m = TriMesh::default();
        m.positions.push(add(centre, [0.0, 0.0, r]));
        for i in 1..rings {
            let phi = std::f64::consts::PI * i as f64 / rings as f64;
            for s in 0..sectors {
                let th = std::f64::consts::TAU * s as f64 / sectors as f64;
                m.positions.push(add(centre, [r * phi.sin() * th.cos(), r * phi.sin() * th.sin(), r * phi.cos()]));
            }
        }
        m.positions.push(add(centre, [0.0, 0.0, -r]));
        let ring = |i: usize, s: usize| (1 + (i - 1) * sectors + s % sectors) as u32;
        for s in 0..sectors {
            m.triangles.push([0, ring(1, s), ring(1, s + 1)]);
        }
        for i in 1..rings - 1 {
            for s in 0..sectors {
                m.triangles.push([ring(i, s), ring(i + 1, s), ring(i + 1, s + 1)]);
                m.triangles.push([ring(i, s), ring(i + 1, s + 1), ring(i, s + 1)]);
            }
        }
        let last = (m.positions.len() - 1) as u32;
        for s in 0..sectors {
            m.triangles.push([last, ring(rings - 1, s + 1), ring(rings - 1, s)]);
        }
        m
    }

    fn ball(r: f64) -> f64 {
        4.0 / 3.0 * std::f64::consts::PI * r * r * r
    }

    #[test]
    fn the_test_sphere_is_closed_and_has_the_right_volume() {
        let s = sphere([0.0; 3], 10.0, 32, 48);
        assert!(is_watertight(&s));
        assert!((volume(&s) / ball(10.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_sharp_union_of_two_spheres_matches_their_analytic_volume() {
        let a = sphere([0.0; 3], 10.0, 32, 48);
        let b = sphere([12.0, 0.0, 0.0], 10.0, 32, 48);
        let params = Params { strength: 0.0, resolution: 80, cut: false, smooth: 0 };
        let u = melt(&[a, b], &params).unwrap();
        assert!(is_watertight(&u), "the melted mesh must be closed");
        // Two balls of radius 10 whose centres are 12 apart share a lens of height 8 (two caps of 4).
        let cap = std::f64::consts::PI * 4.0 * 4.0 * (3.0 * 10.0 - 4.0) / 3.0;
        let expected = 2.0 * ball(10.0) - 2.0 * cap;
        let got = volume(&u);
        assert!((got / expected - 1.0).abs() < 0.03, "volume {got} vs {expected}");
    }

    #[test]
    fn melt_strength_fills_the_crease_and_keeps_the_mesh_closed() {
        let a = sphere([0.0; 3], 10.0, 32, 48);
        let b = sphere([21.0, 0.0, 0.0], 10.0, 32, 48);
        let sharp = melt(&[a.clone(), b.clone()], &Params { strength: 0.0, resolution: 80, cut: false, smooth: 0 }).unwrap();
        let soft = melt(&[a, b], &Params { strength: 8.0, resolution: 80, cut: false, smooth: 2 }).unwrap();
        assert!(is_watertight(&soft));
        // Apart, the balls do not touch, yet the soft union bridges them.
        assert!(volume(&soft) > volume(&sharp) * 1.02, "{} vs {}", volume(&soft), volume(&sharp));
    }

    #[test]
    fn a_soft_cut_removes_material_with_a_rounded_edge() {
        let big = sphere([0.0; 3], 10.0, 32, 48);
        let tool = sphere([10.0, 0.0, 0.0], 6.0, 32, 48);
        let cut = melt(&[big, tool], &Params { strength: 3.0, resolution: 80, cut: true, smooth: 1 }).unwrap();
        assert!(is_watertight(&cut));
        assert!(volume(&cut) < ball(10.0) * 0.95);
        assert!(volume(&cut) > ball(10.0) * 0.5);
    }

    #[test]
    fn a_sharp_cut_removes_the_analytic_lens() {
        // A ball of radius 6 centred on the rim of a ball of radius 10 removes a lens of volume
        // pi (R + r - d)^2 (d^2 + 2dr - 3r^2 + 2dR + 6rR - 3R^2) / (12 d) = pi * 36 * 372 / 120.
        let big = sphere([0.0; 3], 10.0, 32, 48);
        let tool = sphere([10.0, 0.0, 0.0], 6.0, 32, 48);
        let cut = melt(&[big, tool], &Params { strength: 0.0, resolution: 80, cut: true, smooth: 0 }).unwrap();
        assert!(is_watertight(&cut));
        let lens = std::f64::consts::PI * 36.0 * 372.0 / 120.0;
        let expected = ball(10.0) - lens;
        let got = volume(&cut);
        assert!((got / expected - 1.0).abs() < 0.03, "volume {got} vs {expected}");
    }

    #[test]
    fn hostile_sizes_are_refused_before_any_grid_is_built() {
        let a = sphere([0.0; 3], 10.0, 16, 24);
        // A reach so large that the grid would wrap around in 32-bit arithmetic.
        let reach = Params { strength: 1e300, resolution: 96, cut: false, smooth: 0 };
        assert!(melt(&[a.clone(), a.clone()], &reach).is_err());
        // A fine grid around a large reach is far over the point cap.
        let fine = Params { strength: 100.0, resolution: 400, cut: false, smooth: 0 };
        assert!(melt(&[a.clone(), a.clone()], &fine).is_err());
        // Coordinates so large that the box's extent overflows.
        let far_a = sphere([1e308, 0.0, 0.0], 10.0, 16, 24);
        let far_b = sphere([-1e308, 0.0, 0.0], 10.0, 16, 24);
        assert!(melt(&[far_a, far_b], &Params::default()).is_err());
    }

    #[test]
    fn bad_input_is_an_error_and_never_a_panic() {
        let s = sphere([0.0; 3], 10.0, 16, 24);
        assert!(melt(&[s.clone()], &Params::default()).is_err());
        assert!(melt(&[], &Params::default()).is_err());
        assert!(melt(&[s.clone(), TriMesh::default()], &Params::default()).is_err());
        let mut broken = s.clone();
        broken.triangles.push([0, 1, 9999]);
        assert!(melt(&[s.clone(), broken], &Params::default()).is_err());
        let mut nan = s.clone();
        nan.positions[0][0] = f64::NAN;
        assert!(melt(&[s.clone(), nan], &Params::default()).is_err());
        assert!(melt(&[s.clone(), s], &Params { strength: -1.0, ..Params::default() }).is_err());
    }

    #[test]
    fn polygonize_meshes_a_sphere_field_and_refuses_bad_grids() {
        let (n, h, r) = (40usize, 0.5_f64, 8.0_f64);
        let o = -(n as f64 - 1.0) * h / 2.0;
        let mut field = Vec::new();
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let p = [o + i as f64 * h, o + j as f64 * h, o + k as f64 * h];
                    field.push(((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r) as f32);
                }
            }
        }
        let m = polygonize([o; 3], h, [n; 3], &field, 2).unwrap();
        assert!(is_watertight(&m));
        assert!((volume(&m) / ball(r) - 1.0).abs() < 0.03, "volume {}", volume(&m));
        assert!(polygonize([o; 3], h, [n; 3], &field[1..], 0).is_err());
        assert!(polygonize([o; 3], 0.0, [n; 3], &field, 0).is_err());
        assert!(polygonize([f64::NAN; 3], h, [n; 3], &field, 0).is_err());
        assert!(polygonize([o; 3], h, [1, n, n], &field, 0).is_err());
        assert!(polygonize([o; 3], h, [usize::MAX, 2, 2], &field, 0).is_err());
        assert!(polygonize([o; 3], h, [n; 3], &vec![1.0; n * n * n], 0).is_err());
    }
}
