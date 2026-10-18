//! A signed distance volume baked from an imported triangle mesh: a regular grid of distances
//! (negative inside) that the CPU distance field and the shaders can sample with trilinear
//! interpolation, so an imported model can melt with the other shapes like any primitive.
//!
//! Baking:
//! * the exact (unsigned) distance to the closest triangle comes from a bounding volume hierarchy
//!   (BVH), queried with the closest triangle of the neighbouring node as the first guess;
//! * the sign comes from rays along the three grid axes: for each axis every triangle is
//!   rasterised onto the grid rows it crosses, giving the crossings along each row. A mesh whose
//!   triangles all agree about "outside" (consistently oriented) is judged by the winding number
//!   (so overlapping shells are fine); any other mesh by the parity of the crossings (so flipped
//!   triangles are fine). The three axes then vote, so a hole in the surface that one axis sees
//!   through is outvoted by the other two (an open box is still "inside" in its middle);
//! * all of it runs on several threads (`std::thread::scope`), can be cancelled and reports its
//!   progress.
//!
//! Serialisation (`to_bytes`/`from_bytes`, `to_base64`/`from_base64`): distances are quantised to
//! 1/64 of a cell, predicted from the already seen neighbours (a 3-D "Lorenzo" predictor), the
//! residuals are split into two byte planes and packed with our own DEFLATE; a 128^3 volume of a
//! typical model is a few hundred KB.
//!
//! Hostile input: the mesh is checked (finite, indices), the grid is capped (`MAX_CELLS`), the
//! number of ray crossings is capped, and `from_bytes` checks every length before allocating.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Instant;

use crate::import_inflate::inflate;
use crate::import_mesh::{ImportedMesh, MAX_TRIANGLES};

/// The most grid points a volume may have (also checked when reading a saved one).
pub const MAX_CELLS: usize = 8_388_608;
/// The most points along one axis.
pub const MAX_AXIS: u32 = 1024;
/// The default number of cells along the longest side of the model.
pub const DEFAULT_RES: u32 = 128;
/// Total grid points aimed at when baking (a bit more than 128^3).
pub const DEFAULT_MAX_CELLS: usize = 4_194_304;
/// Ray crossings kept per axis; a mesh that needs more is refused.
const MAX_CROSSINGS: usize = 12_000_000;
/// Quantisation of the saved distances: this many steps per cell.
const STEPS_PER_CELL: u8 = 64;
const MAGIC: &[u8; 4] = b"MSDF";
const VERSION: u8 = 1;
const HEADER: usize = 4 + 1 + 1 + 12 + 12 + 4;
/// A saved volume (as text) may not be longer than this.
pub const MAX_SERIAL_CHARS: usize = 32 * 1024 * 1024;
/// What `distance` says for a point that is not a number.
const FAR: f32 = 1.0e9;

pub const ERR_EMPTY: &str = "形が空です（三角形がありません）";
pub const ERR_BAD_MESH: &str = "メッシュの数値か番号がおかしいです";
pub const ERR_CANCEL: &str = "中止しました";
pub const ERR_BAD_DATA: &str = "距離データがこわれています";

/// The baked distances. Node `(i, j, k)` sits at `origin + (i, j, k) * cell`; `data` is ordered
/// with `i` fastest.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub dims: [u32; 3],
    pub origin: [f32; 3],
    pub cell: f32,
    pub data: Vec<f32>,
}

/// How to bake.
#[derive(Clone, Copy, Debug)]
pub struct BakeOptions {
    /// Cells along the longest side of the model (4..=512); lowered when the grid would be too big.
    pub res: u32,
    /// Empty cells around the model, so that blending outside the surface has room.
    pub pad: u32,
    /// Most grid points in total.
    pub max_cells: usize,
    /// Worker threads (0 = all the machine has, up to 16).
    pub threads: usize,
    /// Blocks of nodes that are all at least this many cells away from the surface are
    /// interpolated from exact values at their corners instead of measured (much faster; the value
    /// there may be a little too small near the middle of thick parts). 0 measures every node.
    pub far_band: f32,
}

impl Default for BakeOptions {
    fn default() -> BakeOptions {
        BakeOptions { res: DEFAULT_RES, pad: 4, max_cells: DEFAULT_MAX_CELLS, threads: 0, far_band: 10.0 }
    }
}

/// Shared with the window: progress (0..=1000) and a flag to stop.
#[derive(Debug, Default)]
pub struct BakeControl {
    pub cancel: AtomicBool,
    pub progress: AtomicU32,
}

impl BakeControl {
    pub fn new() -> BakeControl {
        BakeControl::default()
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn set(&self, permille: u32) {
        self.progress.store(permille.min(1000), Ordering::Relaxed);
    }
}

/// What a bake did (for the log and the tests).
#[derive(Clone, Copy, Debug, Default)]
pub struct BakeStats {
    pub triangles: usize,
    pub cells: usize,
    pub bvh_secs: f64,
    pub sign_secs: f64,
    pub dist_secs: f64,
    /// The signs came from the winding number (consistently oriented mesh); otherwise from parity.
    pub used_winding: bool,
    /// Nodes where the three axes did not agree.
    pub split_votes: usize,
    /// Tree nodes and triangles looked at while measuring distances (a count of the work done).
    pub tree_steps: u64,
}

// ---------------------------------------------------------------- small vector helpers

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn axpy(a: V3, k: f64, b: V3) -> V3 {
    [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]]
}

#[derive(Clone, Copy)]
struct Tri {
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
}

fn v64(p: [f32; 3]) -> V3 {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

/// Squared distance from `p` to a triangle (the closest point is found by the usual region tests).
fn tri_d2(p: V3, t: &Tri) -> f64 {
    let (a, b, c) = (v64(t.a), v64(t.b), v64(t.c));
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    let len2 = |v: V3| dot(v, v);
    if d1 <= 0.0 && d2 <= 0.0 {
        return len2(ap);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return len2(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return len2(sub(p, axpy(a, v, ab)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return len2(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return len2(sub(p, axpy(a, w, ac)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return len2(sub(p, axpy(b, w, sub(c, b))));
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    len2(sub(p, axpy(axpy(a, v, ab), w, ac)))
}

// ---------------------------------------------------------------- BVH

#[derive(Clone, Copy, Default)]
struct Node {
    lo: [f32; 3],
    hi: [f32; 3],
    /// Inner node: index of the left child. Leaf: first triangle.
    a: u32,
    /// Inner node: index of the right child.
    b: u32,
    /// Leaf: number of triangles (0 for an inner node).
    count: u32,
}

struct Item {
    lo: [f32; 3],
    hi: [f32; 3],
    c: [f32; 3],
    id: u32,
}

struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<Tri>,
}

const LEAF: usize = 4;

fn build_rec(items: &mut [Item], base: usize, nodes: &mut Vec<Node>) -> u32 {
    let idx = nodes.len();
    nodes.push(Node::default());
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    let mut clo = [f32::INFINITY; 3];
    let mut chi = [f32::NEG_INFINITY; 3];
    for it in items.iter() {
        for a in 0..3 {
            lo[a] = lo[a].min(it.lo[a]);
            hi[a] = hi[a].max(it.hi[a]);
            clo[a] = clo[a].min(it.c[a]);
            chi[a] = chi[a].max(it.c[a]);
        }
    }
    let node = if items.len() <= LEAF {
        Node { lo, hi, a: base as u32, b: 0, count: items.len() as u32 }
    } else {
        let ext = [chi[0] - clo[0], chi[1] - clo[1], chi[2] - clo[2]];
        let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] {
            0
        } else if ext[1] >= ext[2] {
            1
        } else {
            2
        };
        let mid = items.len() / 2;
        items.select_nth_unstable_by(mid, |x, y| x.c[axis].total_cmp(&y.c[axis]));
        let (l, r) = items.split_at_mut(mid);
        let left = build_rec(l, base, nodes);
        let right = build_rec(r, base + mid, nodes);
        Node { lo, hi, a: left, b: right, count: 0 }
    };
    if let Some(slot) = nodes.get_mut(idx) {
        *slot = node;
    }
    idx as u32
}

impl Bvh {
    fn build(tris: &[Tri]) -> Bvh {
        let mut items: Vec<Item> = tris
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut lo = t.a;
                let mut hi = t.a;
                for p in [t.b, t.c] {
                    for a in 0..3 {
                        lo[a] = lo[a].min(p[a]);
                        hi[a] = hi[a].max(p[a]);
                    }
                }
                Item { lo, hi, c: [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5], id: i as u32 }
            })
            .collect();
        let mut nodes = Vec::with_capacity(items.len() / 2 + 1);
        build_rec(&mut items, 0, &mut nodes);
        let ordered = items.iter().filter_map(|it| tris.get(it.id as usize).copied()).collect();
        Bvh { nodes, tris: ordered }
    }

    fn box_d2(n: &Node, p: [f32; 3]) -> f32 {
        let mut s = 0.0f32;
        for (a, pa) in p.iter().enumerate() {
            let d = (n.lo[a] - pa).max(0.0).max(pa - n.hi[a]);
            s += d * d;
        }
        s
    }

    /// The squared distance to the closest triangle and that triangle's slot. `hint` is a slot to
    /// try first (the closest triangle of a neighbouring query). The answer may be too big by up to
    /// `slack` (a distance): boxes that cannot help by more than that are not visited, and the
    /// search stops as soon as the best triangle is within `slack` of `lower`, a distance the
    /// point is known to be at least away from the surface (a neighbour's distance minus the step).
    fn nearest(&self, p: [f32; 3], hint: usize, lower: f64, slack: f64, work: &mut u64) -> (f64, usize) {
        *work += 1;
        let pd = v64(p);
        let stop = {
            let s = lower + slack;
            if s > 0.0 { s * s } else { 0.0 }
        };
        let shrink = |best: f64| {
            let s = best.sqrt() - slack;
            if s > 0.0 { s * s } else { 0.0 }
        };
        let mut best = f64::INFINITY;
        let mut best_i = 0usize;
        let mut limit = f64::INFINITY;
        if let Some(t) = self.tris.get(hint) {
            *work += 1;
            let d = tri_d2(pd, t);
            if d.is_finite() {
                best = d;
                best_i = hint;
                limit = shrink(best);
                if best <= stop {
                    return (best, best_i);
                }
            }
        }
        let mut stack = [(0u32, 0f32); 64];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let (ni, bd) = stack.get(sp).copied().unwrap_or((0, f32::INFINITY));
            if f64::from(bd) >= limit {
                continue;
            }
            let Some(n) = self.nodes.get(ni as usize) else { continue };
            *work += 1;
            if n.count > 0 {
                let first = n.a as usize;
                for slot in first..first + n.count as usize {
                    if let Some(t) = self.tris.get(slot) {
                        *work += 1;
                        let d = tri_d2(pd, t);
                        if d < best {
                            best = d;
                            best_i = slot;
                            limit = shrink(best);
                        }
                    }
                }
                if best <= stop {
                    break;
                }
            } else {
                let (Some(l), Some(r)) = (self.nodes.get(n.a as usize), self.nodes.get(n.b as usize)) else { continue };
                let (dl, dr) = (Self::box_d2(l, p), Self::box_d2(r, p));
                let (near, far) = if dl <= dr { ((n.a, dl), (n.b, dr)) } else { ((n.b, dr), (n.a, dl)) };
                if sp + 2 <= stack.len() {
                    if f64::from(far.1) < limit {
                        if let Some(s) = stack.get_mut(sp) {
                            *s = far;
                        }
                        sp += 1;
                    }
                    if f64::from(near.1) < limit {
                        if let Some(s) = stack.get_mut(sp) {
                            *s = near;
                        }
                        sp += 1;
                    }
                }
            }
        }
        (best, best_i)
    }
}

// ---------------------------------------------------------------- the grid and the signs

#[derive(Clone, Copy)]
struct Grid {
    n: [usize; 3],
    origin: [f64; 3],
    cell: f64,
}

impl Grid {
    fn idx(&self, i: usize, j: usize, k: usize) -> usize {
        i + self.n[0] * (j + self.n[1] * k)
    }
    fn total(&self) -> usize {
        self.n[0] * self.n[1] * self.n[2]
    }
}

/// The side of the edge `a`-`b` on which `p` lies; the same number (negated) from either end, so
/// that two triangles sharing an edge always agree about a ray that passes close to it.
fn edge_fn(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    if a.0 < b.0 || (a.0 == b.0 && a.1 < b.1) {
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    } else {
        -((a.0 - b.0) * (p.1 - b.1) - (a.1 - b.1) * (p.0 - b.0))
    }
}

/// Small offsets of the rays from the grid lines, so that rays never run exactly along an edge or
/// through a vertex of a model that was made on the same grid (a box with round numbers).
const JITTER: [(f64, f64); 3] = [(0.013_731, 0.007_177), (0.009_113, 0.012_471), (0.011_713, 0.005_337)];

/// One ray crossing: row, position along the ray, +1 where the ray travels into a face's back.
type Crossing = (u32, f32, i8);

fn crossings_for_axis(tris: &[Tri], g: &Grid, axis: usize, threads: usize, ctl: &BakeControl) -> Result<Vec<Crossing>, String> {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let (nu, nv) = (g.n[u], g.n[v]);
    let (ju, jv) = (JITTER[axis].0 * g.cell, JITTER[axis].1 * g.cell);
    let ou = g.origin[u] + ju;
    let ov = g.origin[v] + jv;
    let used = AtomicUsize::new(0);
    let chunk = tris.len().div_ceil(threads.max(1)).max(1);
    let parts: Vec<Result<Vec<Crossing>, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = tris
            .chunks(chunk)
            .map(|part| {
                let used = &used;
                s.spawn(move || -> Result<Vec<Crossing>, String> {
                    let mut out: Vec<Crossing> = Vec::new();
                    for (count, t) in part.iter().enumerate() {
                        if count % 4096 == 0 && ctl.cancelled() {
                            return Err(ERR_CANCEL.into());
                        }
                        let (a, b, c) = (v64(t.a), v64(t.b), v64(t.c));
                        let n = cross(sub(b, a), sub(c, a));
                        if n[axis] == 0.0 || !n[axis].is_finite() {
                            continue;
                        }
                        let (au, bu, cu) = ((a[u], a[v]), (b[u], b[v]), (c[u], c[v]));
                        let umin = au.0.min(bu.0).min(cu.0);
                        let umax = au.0.max(bu.0).max(cu.0);
                        let vmin = au.1.min(bu.1).min(cu.1);
                        let vmax = au.1.max(bu.1).max(cu.1);
                        let cl = |x: f64, hi: usize| x.clamp(-1.0, hi as f64);
                        let j0 = cl(((umin - ou) / g.cell).ceil(), nu).max(0.0) as usize;
                        let j1 = cl(((umax - ou) / g.cell).floor(), nu);
                        let k0 = cl(((vmin - ov) / g.cell).ceil(), nv).max(0.0) as usize;
                        let k1 = cl(((vmax - ov) / g.cell).floor(), nv);
                        if j1 < 0.0 || k1 < 0.0 {
                            continue;
                        }
                        let (j1, k1) = ((j1 as usize).min(nu - 1), (k1 as usize).min(nv - 1));
                        if j0 > j1 || k0 > k1 {
                            continue;
                        }
                        let rows = (j1 - j0 + 1) * (k1 - k0 + 1);
                        if used.fetch_add(rows.min(1 << 20), Ordering::Relaxed) > MAX_CROSSINGS {
                            return Err("メッシュが複雑すぎて、内側と外側を調べられません".into());
                        }
                        let sign: i8 = if n[axis] < 0.0 { 1 } else { -1 };
                        for kk in k0..=k1 {
                            let pv = ov + kk as f64 * g.cell;
                            for jj in j0..=j1 {
                                let pu = ou + jj as f64 * g.cell;
                                let p = (pu, pv);
                                let (w0, w1, w2) = (edge_fn(au, bu, p), edge_fn(bu, cu, p), edge_fn(cu, au, p));
                                let inside = (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0);
                                if !inside {
                                    continue;
                                }
                                // where the ray meets the plane of the triangle
                                let w = a[axis] - (n[u] * (pu - a[u]) + n[v] * (pv - a[v])) / n[axis];
                                if w.is_finite() {
                                    out.push(((jj + nu * kk) as u32, w as f32, sign));
                                }
                            }
                        }
                    }
                    Ok(out)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("内部エラー".to_string()))).collect()
    });
    let mut all: Vec<Crossing> = Vec::new();
    for p in parts {
        all.extend(p?);
        if all.len() > MAX_CROSSINGS {
            return Err("メッシュが複雑すぎて、内側と外側を調べられません".into());
        }
    }
    all.sort_unstable_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
    Ok(all)
}

/// Sets bit `axis` of `votes[node]` where the ray says "inside".
fn vote_axis(g: &Grid, axis: usize, cr: &[Crossing], winding: bool, votes: &mut [u8]) {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let (nu, nv) = (g.n[u], g.n[v]);
    let mut p = 0usize;
    for kk in 0..nv {
        for jj in 0..nu {
            let row = (jj + nu * kk) as u32;
            while cr.get(p).is_some_and(|c| c.0 < row) {
                p += 1;
            }
            let mut q = p;
            let mut acc = 0i32;
            for i in 0..g.n[axis] {
                let t = (g.origin[axis] + i as f64 * g.cell) as f32;
                while let Some(c) = cr.get(q) {
                    if c.0 != row || c.1 >= t {
                        break;
                    }
                    acc += if winding { i32::from(c.2) } else { 1 };
                    q += 1;
                }
                let inside = if winding { acc != 0 } else { acc & 1 == 1 };
                if inside {
                    let mut c = [0usize; 3];
                    c[axis] = i;
                    c[u] = jj;
                    c[v] = kk;
                    if let Some(vt) = votes.get_mut(g.idx(c[0], c[1], c[2])) {
                        *vt |= 1 << axis;
                    }
                }
            }
            p = q;
            while cr.get(p).is_some_and(|c| c.0 == row) {
                p += 1;
            }
        }
    }
}

/// True when no directed edge is used twice, so every neighbouring pair of triangles agrees on
/// which side is outside (or the mesh is entirely turned inside out).
fn consistently_oriented(m: &ImportedMesh) -> bool {
    let mut e: Vec<u64> = Vec::with_capacity(m.triangles.len() * 3);
    for t in &m.triangles {
        for k in 0..3 {
            e.push((u64::from(t[k]) << 32) | u64::from(t[(k + 1) % 3]));
        }
    }
    e.sort_unstable();
    e.windows(2).all(|w| w[0] != w[1])
}

// ---------------------------------------------------------------- baking

fn thread_count(want: usize) -> usize {
    if want > 0 {
        return want.min(64);
    }
    std::thread::available_parallelism().map_or(2, usize::from).min(16)
}

/// Bake a mesh into a distance volume with the default progress/cancel flags.
#[allow(dead_code)]
pub fn bake(mesh: &ImportedMesh, opts: &BakeOptions) -> Result<Volume, String> {
    bake_with(mesh, opts, &BakeControl::new()).map(|(v, _)| v)
}

/// Bake a mesh into a distance volume.
pub fn bake_with(mesh: &ImportedMesh, opts: &BakeOptions, ctl: &BakeControl) -> Result<(Volume, BakeStats), String> {
    let mut stats = BakeStats::default();
    if mesh.triangles.is_empty() || mesh.positions.is_empty() {
        return Err(ERR_EMPTY.into());
    }
    if mesh.triangles.len() > MAX_TRIANGLES {
        return Err(crate::import_mesh::TOO_MANY_TRIS.into());
    }
    let np = mesh.positions.len();
    if !mesh.positions.iter().flatten().all(|c| c.is_finite() && c.abs() <= crate::import_mesh::MAX_COORD)
        || mesh.triangles.iter().flatten().any(|i| *i as usize >= np)
    {
        return Err(ERR_BAD_MESH.into());
    }
    let tris: Vec<Tri> = mesh
        .triangles
        .iter()
        .filter_map(|t| {
            Some(Tri { a: *mesh.positions.get(t[0] as usize)?, b: *mesh.positions.get(t[1] as usize)?, c: *mesh.positions.get(t[2] as usize)? })
        })
        .collect();
    stats.triangles = tris.len();
    let (lo, hi) = mesh.bounds().ok_or(ERR_EMPTY)?;
    let size = [f64::from(hi[0]) - f64::from(lo[0]), f64::from(hi[1]) - f64::from(lo[1]), f64::from(hi[2]) - f64::from(lo[2])];
    let longest = size.into_iter().fold(0.0f64, f64::max);
    if longest <= 0.0 {
        return Err(ERR_EMPTY.into());
    }
    // the grid: `res` cells along the longest side, lowered until it fits
    let pad = opts.pad.clamp(1, 16) as usize;
    let max_cells = opts.max_cells.clamp(4096, MAX_CELLS);
    let mut res = f64::from(opts.res.clamp(4, 512));
    let (cell, n) = loop {
        let cell = longest / res;
        let n: [usize; 3] = [0, 1, 2].map(|a| (size[a] / cell).ceil() as usize + 1 + 2 * pad);
        if n.iter().all(|c| *c <= MAX_AXIS as usize) && n[0].saturating_mul(n[1]).saturating_mul(n[2]) <= max_cells {
            break (cell, n);
        }
        res *= 0.93;
        if res < 4.0 {
            return Err("グリッドを作れません".into());
        }
    };
    let origin = [0, 1, 2].map(|a| {
        let cover = (n[a] - 1) as f64 * cell;
        f64::from(lo[a]) - (cover - size[a]) * 0.5
    });
    let g = Grid { n, origin, cell };
    let total = g.total();
    stats.cells = total;
    let threads = thread_count(opts.threads);

    let t0 = Instant::now();
    let bvh = Bvh::build(&tris);
    stats.bvh_secs = t0.elapsed().as_secs_f64();
    ctl.set(50);
    if ctl.cancelled() {
        return Err(ERR_CANCEL.into());
    }

    // signs
    let t1 = Instant::now();
    let winding = consistently_oriented(mesh);
    stats.used_winding = winding;
    let mut votes = vec![0u8; total];
    for axis in 0..3 {
        let cr = crossings_for_axis(&tris, &g, axis, threads, ctl)?;
        vote_axis(&g, axis, &cr, winding, &mut votes);
        ctl.set(50 + 100 * (axis as u32 + 1));
    }
    stats.split_votes = votes.iter().filter(|v| matches!(v.count_ones(), 1 | 2)).count();
    stats.sign_secs = t1.elapsed().as_secs_f64();
    if ctl.cancelled() {
        return Err(ERR_CANCEL.into());
    }

    // distances. Far from the surface (opts.far_band cells and more) the field is smooth and the
    // exact answer expensive, so blocks of nodes whose corners are all far are interpolated from
    // exact values at the corners; everything else is measured node by node.
    let t2 = Instant::now();
    let slack = g.cell * 0.004; // answers may be too big by this much (0.4% of a cell)
    let step = 4usize;
    let lattice = |len: usize| -> Vec<usize> {
        let mut v: Vec<usize> = (0..len.saturating_sub(1)).step_by(step).collect();
        v.push(len.saturating_sub(1));
        v
    };
    let cp = [lattice(n[0]), lattice(n[1]), lattice(n[2])];
    let use_far = opts.far_band > 0.0 && n.iter().all(|c| *c >= 2 * step);
    let total_work = std::sync::atomic::AtomicU64::new(0);
    let sign_at = |i: usize, j: usize, k: usize| votes.get(g.idx(i, j, k)).is_some_and(|v| v.count_ones() >= 2);
    let measure = |i: usize, j: usize, k: usize, hint: &mut usize, work: &mut u64| -> f32 {
        let p = [(g.origin[0] + i as f64 * g.cell) as f32, (g.origin[1] + j as f64 * g.cell) as f32, (g.origin[2] + k as f64 * g.cell) as f32];
        let (d2, slot) = bvh.nearest(p, *hint, 0.0, slack, work);
        *hint = slot;
        let d = d2.sqrt() as f32;
        if sign_at(i, j, k) { -d } else { d }
    };
    // 1. the exact values at the lattice
    let cn = [cp[0].len(), cp[1].len(), cp[2].len()];
    let mut coarse = vec![0f32; cn[0] * cn[1] * cn[2]];
    if use_far {
        let jobs = Mutex::new(coarse.chunks_mut(cn[0] * cn[1]).enumerate());
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut work = 0u64;
                    loop {
                        let next = match jobs.lock() {
                            Ok(mut w) => w.next(),
                            Err(_) => None,
                        };
                        let Some((ck, out)) = next else { break };
                        if ctl.cancelled() {
                            break;
                        }
                        let mut hint = 0usize;
                        for cj in 0..cn[1] {
                            for ci in 0..cn[0] {
                                let (i, j, k) = (cp[0][ci], cp[1][cj], cp[2][ck]);
                                if let Some(o) = out.get_mut(ci + cn[0] * cj) {
                                    *o = measure(i, j, k, &mut hint, &mut work);
                                }
                            }
                        }
                    }
                    total_work.fetch_add(work, Ordering::Relaxed);
                });
            }
        });
        if ctl.cancelled() {
            return Err(ERR_CANCEL.into());
        }
    }
    ctl.set(400);
    // which lattice cell each node belongs to, and where in it
    let locate = |a: usize| -> Vec<(usize, f32)> {
        let c = &cp[a];
        let mut out = Vec::with_capacity(n[a]);
        let mut ci = 0usize;
        for i in 0..n[a] {
            while ci + 2 < c.len() && c.get(ci + 1).is_some_and(|nx| *nx <= i) {
                ci += 1;
            }
            let (lo, hi) = (c.get(ci).copied().unwrap_or(0), c.get(ci + 1).copied().unwrap_or(0));
            let t = if hi > lo { (i - lo) as f32 / (hi - lo) as f32 } else { 0.0 };
            out.push((ci, t));
        }
        out
    };
    let at = [locate(0), locate(1), locate(2)];
    let far_limit = (opts.far_band * g.cell as f32).max(0.0);
    let cval = |ci: usize, cj: usize, ck: usize| {
        coarse.get(ci.min(cn[0] - 1) + cn[0] * (cj.min(cn[1] - 1) + cn[1] * ck.min(cn[2] - 1))).copied().unwrap_or(0.0)
    };
    let is_far = |ci: usize, cj: usize, ck: usize| -> bool {
        let mut m = f32::INFINITY;
        for (a, b, c) in [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)] {
            m = m.min(cval(ci + a, cj + b, ck + c).abs());
        }
        m >= far_limit
    };
    // 2. every node: interpolated in far blocks, measured elsewhere
    let mut data = vec![0f32; total];
    let slice = n[0] * n[1];
    {
        let jobs = Mutex::new(data.chunks_mut(slice).enumerate());
        let done = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut work = 0u64;
                    loop {
                        let next = match jobs.lock() {
                            Ok(mut w) => w.next(),
                            Err(_) => None,
                        };
                        let Some((k, out)) = next else { break };
                        if ctl.cancelled() {
                            break;
                        }
                        let (ck, tz) = at[2].get(k).copied().unwrap_or((0, 0.0));
                        let mut hint = 0usize;
                        for j in 0..n[1] {
                            let (cj, ty) = at[1].get(j).copied().unwrap_or((0, 0.0));
                            for i in 0..n[0] {
                                let (ci, tx) = at[0].get(i).copied().unwrap_or((0, 0.0));
                                let v = if use_far && is_far(ci, cj, ck) {
                                    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
                                    let c00 = l(cval(ci, cj, ck), cval(ci + 1, cj, ck), tx);
                                    let c10 = l(cval(ci, cj + 1, ck), cval(ci + 1, cj + 1, ck), tx);
                                    let c01 = l(cval(ci, cj, ck + 1), cval(ci + 1, cj, ck + 1), tx);
                                    let c11 = l(cval(ci, cj + 1, ck + 1), cval(ci + 1, cj + 1, ck + 1), tx);
                                    l(l(c00, c10, ty), l(c01, c11, ty), tz)
                                } else {
                                    measure(i, j, k, &mut hint, &mut work)
                                };
                                if let Some(o) = out.get_mut(i + n[0] * j) {
                                    *o = v;
                                }
                            }
                        }
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        ctl.set(400 + (600 * d / n[2].max(1)) as u32);
                    }
                    total_work.fetch_add(work, Ordering::Relaxed);
                });
            }
        });
    }
    if ctl.cancelled() {
        return Err(ERR_CANCEL.into());
    }
    stats.dist_secs = t2.elapsed().as_secs_f64();
    stats.tree_steps = total_work.load(Ordering::Relaxed);
    ctl.set(1000);
    if !data.iter().all(|d| d.is_finite()) {
        return Err(ERR_BAD_MESH.into());
    }
    let vol = Volume {
        dims: [n[0] as u32, n[1] as u32, n[2] as u32],
        origin: [origin[0] as f32, origin[1] as f32, origin[2] as f32],
        cell: cell as f32,
        data,
    };
    Ok((vol, stats))
}

// ---------------------------------------------------------------- sampling

impl Volume {
    fn dims_us(&self) -> [usize; 3] {
        [self.dims[0] as usize, self.dims[1] as usize, self.dims[2] as usize]
    }

    fn valid(&self) -> bool {
        let d = self.dims_us();
        d.iter().all(|x| *x >= 1) && d[0].saturating_mul(d[1]).saturating_mul(d[2]) == self.data.len() && self.cell.is_finite() && self.cell > 0.0
    }

    /// The distance stored at a node (a large positive number outside the grid).
    #[allow(dead_code)]
    pub fn at(&self, i: usize, j: usize, k: usize) -> f32 {
        let d = self.dims_us();
        if i >= d[0] || j >= d[1] || k >= d[2] {
            return FAR;
        }
        self.data.get(i + d[0] * (j + d[1] * k)).copied().unwrap_or(FAR)
    }

    /// The corners of the box the nodes span.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let hi = [0, 1, 2].map(|a| self.origin[a] + self.dims[a].saturating_sub(1) as f32 * self.cell);
        (self.origin, hi)
    }

    /// The signed distance at a point: trilinear inside the grid; outside, the distance to the
    /// grid's box plus the value at the closest point of the box (so it stays continuous and keeps
    /// growing). Anything that is not a number is "very far".
    pub fn distance(&self, p: [f32; 3]) -> f32 {
        if !self.valid() || !p.iter().all(|c| c.is_finite()) {
            return FAR;
        }
        let d = self.dims_us();
        let mut q = [0f32; 3];
        let mut excess = 0f32;
        for a in 0..3 {
            let g = (p[a] - self.origin[a]) / self.cell;
            let top = (d[a] - 1) as f32;
            let c = g.clamp(0.0, top);
            q[a] = c;
            let e = (g - c) * self.cell;
            excess += e * e;
        }
        let mut i0 = [0usize; 3];
        let mut f = [0f32; 3];
        for a in 0..3 {
            let base = q[a].floor().min((d[a] - 1) as f32);
            i0[a] = base as usize;
            f[a] = q[a] - base;
        }
        let i1 = [0, 1, 2].map(|a| (i0[a] + 1).min(d[a] - 1));
        let v = |i: usize, j: usize, k: usize| self.data.get(i + d[0] * (j + d[1] * k)).copied().unwrap_or(FAR);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c00 = lerp(v(i0[0], i0[1], i0[2]), v(i1[0], i0[1], i0[2]), f[0]);
        let c10 = lerp(v(i0[0], i1[1], i0[2]), v(i1[0], i1[1], i0[2]), f[0]);
        let c01 = lerp(v(i0[0], i0[1], i1[2]), v(i1[0], i0[1], i1[2]), f[0]);
        let c11 = lerp(v(i0[0], i1[1], i1[2]), v(i1[0], i1[1], i1[2]), f[0]);
        let val = lerp(lerp(c00, c10, f[1]), lerp(c01, c11, f[1]), f[2]);
        val + excess.sqrt()
    }

    /// The direction in which the distance grows fastest (not normalised), by central differences.
    #[allow(dead_code)]
    pub fn gradient(&self, p: [f32; 3]) -> [f32; 3] {
        let h = self.cell * 0.5;
        let mut g = [0f32; 3];
        for (a, slot) in g.iter_mut().enumerate() {
            let (mut lo, mut hi) = (p, p);
            lo[a] -= h;
            hi[a] += h;
            *slot = (self.distance(hi) - self.distance(lo)) / (2.0 * h);
        }
        g
    }
}

/// A half-precision float (IEEE 754 binary16) for a 32-bit one: rounded to nearest, too big values
/// become the biggest finite half, not-a-number becomes 0. Useful for a filterable GPU texture.
pub fn f32_to_f16_bits(x: f32) -> u16 {
    if x.is_nan() {
        return 0;
    }
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let mag = x.abs();
    if mag >= 65520.0 {
        return sign | 0x7BFF;
    }
    if mag < 6.103_515_6e-5 {
        // a subnormal half: a multiple of 2^-24
        return sign | (mag * 16_777_216.0).round() as u16;
    }
    let e = ((b >> 23) & 0xFF) as i32 - 127 + 15;
    let mant = b & 0x7F_FFFF;
    let mut h = ((e.max(0) as u32) << 10) | (mant >> 13);
    let rem = mant & 0x1FFF;
    if rem > 0x1000 || (rem == 0x1000 && h & 1 == 1) {
        h += 1;
    }
    sign | h as u16
}

impl Volume {
    /// All distances as half floats (`data` order), for uploading to a 3-D texture.
    /// Kept as the whole-volume reference for the chunked upload regression tests.
    #[cfg(test)]
    pub fn to_f16_bits(&self) -> Vec<u16> {
        self.data.iter().map(|v| f32_to_f16_bits(*v)).collect()
    }
}

// ---------------------------------------------------------------- saving

fn fail<T>() -> Result<T, String> {
    Err(ERR_BAD_DATA.into())
}

/// The prediction of a value from the three neighbours before it, three in a plane, and one in the corner.
fn lorenzo(q: &[u16], d: [usize; 3], i: usize, j: usize, k: usize) -> u16 {
    let at = |a: usize, b: usize, c: usize| -> u16 { q.get(a + d[0] * (b + d[1] * c)).copied().unwrap_or(0) };
    let x = if i > 0 { at(i - 1, j, k) } else { 0 };
    let y = if j > 0 { at(i, j - 1, k) } else { 0 };
    let z = if k > 0 { at(i, j, k - 1) } else { 0 };
    let xy = if i > 0 && j > 0 { at(i - 1, j - 1, k) } else { 0 };
    let xz = if i > 0 && k > 0 { at(i - 1, j, k - 1) } else { 0 };
    let yz = if j > 0 && k > 0 { at(i, j - 1, k - 1) } else { 0 };
    let xyz = if i > 0 && j > 0 && k > 0 { at(i - 1, j - 1, k - 1) } else { 0 };
    x.wrapping_add(y).wrapping_add(z).wrapping_sub(xy).wrapping_sub(xz).wrapping_sub(yz).wrapping_add(xyz)
}

fn zigzag(r: u16) -> u16 {
    let s = r as i16;
    ((s << 1) ^ (s >> 15)) as u16
}

fn unzigzag(z: u16) -> u16 {
    ((z >> 1) as i16 ^ -((z & 1) as i16)) as u16
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl Volume {
    /// The compact form: a small header and the deflated residuals of the quantised distances.
    pub fn to_bytes(&self) -> Vec<u8> {
        let d = self.dims_us();
        let step = self.cell / f32::from(STEPS_PER_CELL);
        let q: Vec<u16> = self
            .data
            .iter()
            .map(|v| {
                let x = if step > 0.0 && v.is_finite() { (v / step).round() } else { 0.0 };
                (x.clamp(-32767.0, 32767.0) as i16) as u16
            })
            .collect();
        let n = q.len();
        let mut planes = vec![0u8; n * 2];
        let mut idx = 0usize;
        for k in 0..d[2] {
            for j in 0..d[1] {
                for i in 0..d[0] {
                    let cur = q.get(idx).copied().unwrap_or(0);
                    let z = zigzag(cur.wrapping_sub(lorenzo(&q, d, i, j, k)));
                    if let Some(b) = planes.get_mut(idx) {
                        *b = (z & 0xFF) as u8;
                    }
                    if let Some(b) = planes.get_mut(n + idx) {
                        *b = (z >> 8) as u8;
                    }
                    idx += 1;
                }
            }
        }
        let mut out = Vec::with_capacity(HEADER + n / 2);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(STEPS_PER_CELL);
        for x in self.dims {
            out.extend_from_slice(&x.to_le_bytes());
        }
        for x in self.origin {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&self.cell.to_le_bytes());
        out.extend_from_slice(&crate::export_deflate::deflate(&planes));
        out
    }

    /// Read what `to_bytes` wrote. Every length is checked before anything is allocated.
    pub fn from_bytes(b: &[u8]) -> Result<Volume, String> {
        if b.len() < HEADER || b.get(..4) != Some(MAGIC.as_slice()) || b.get(4) != Some(&VERSION) {
            return fail();
        }
        let steps = b.get(5).copied().filter(|s| *s > 0).ok_or(ERR_BAD_DATA)?;
        let rd = |o: usize| b.get(o..o + 4).map(|x| [x[0], x[1], x[2], x[3]]).ok_or(ERR_BAD_DATA.to_string());
        let dims = [u32::from_le_bytes(rd(6)?), u32::from_le_bytes(rd(10)?), u32::from_le_bytes(rd(14)?)];
        let origin = [f32::from_le_bytes(rd(18)?), f32::from_le_bytes(rd(22)?), f32::from_le_bytes(rd(26)?)];
        let cell = f32::from_le_bytes(rd(30)?);
        if dims.iter().any(|x| *x == 0 || *x > MAX_AXIS)
            || !origin.iter().all(|x| x.is_finite() && x.abs() < 1.0e9)
            || !cell.is_finite()
            || !(1.0e-6..=1.0e6).contains(&cell)
        {
            return fail();
        }
        let d = [dims[0] as usize, dims[1] as usize, dims[2] as usize];
        let n = d[0] * d[1] * d[2];
        if n > MAX_CELLS {
            return fail();
        }
        let planes = inflate(b.get(HEADER..).ok_or(ERR_BAD_DATA)?, n * 2).map_err(|e| e.to_string())?;
        if planes.len() != n * 2 {
            return fail();
        }
        let mut q = vec![0u16; n];
        let mut idx = 0usize;
        for k in 0..d[2] {
            for j in 0..d[1] {
                for i in 0..d[0] {
                    let lo = planes.get(idx).copied().unwrap_or(0);
                    let hi = planes.get(n + idx).copied().unwrap_or(0);
                    let z = u16::from(lo) | (u16::from(hi) << 8);
                    let v = unzigzag(z).wrapping_add(lorenzo(&q, d, i, j, k));
                    if let Some(s) = q.get_mut(idx) {
                        *s = v;
                    }
                    idx += 1;
                }
            }
        }
        let step = cell / f32::from(steps);
        let data = q.iter().map(|v| f32::from(*v as i16) * step).collect();
        Ok(Volume { dims, origin, cell, data })
    }

    /// `to_bytes` as base64 text, to go inside a JSON scene file.
    pub fn to_base64(&self) -> String {
        let bytes = self.to_bytes();
        let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for c in bytes.chunks(3) {
            let n = (u32::from(c[0]) << 16) | (u32::from(c.get(1).copied().unwrap_or(0)) << 8) | u32::from(c.get(2).copied().unwrap_or(0));
            let ch = |sh: u32| char::from(B64.get(((n >> sh) & 63) as usize).copied().unwrap_or(b'A'));
            s.push(ch(18));
            s.push(ch(12));
            s.push(if c.len() > 1 { ch(6) } else { '=' });
            s.push(if c.len() > 2 { ch(0) } else { '=' });
        }
        s
    }

    /// Read what `to_base64` wrote (whitespace is ignored).
    pub fn from_base64(text: &str) -> Result<Volume, String> {
        if text.len() > MAX_SERIAL_CHARS {
            return fail();
        }
        let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
        let mut acc = 0u32;
        let mut nbits = 0u32;
        let mut pad = 0usize;
        for c in text.bytes() {
            if c.is_ascii_whitespace() {
                continue;
            }
            if c == b'=' {
                pad += 1;
                continue;
            }
            if pad > 0 {
                return fail();
            }
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return fail(),
            };
            acc = (acc << 6) | u32::from(v);
            nbits += 6;
            if nbits >= 8 {
                nbits -= 8;
                bytes.push((acc >> nbits) as u8);
                acc &= (1 << nbits) - 1;
            }
        }
        if pad > 2 {
            return fail();
        }
        Volume::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_mesh::tests::{box_mesh, sphere_mesh};

    /// Brute-force distance to the mesh surface (the reference for the BVH).
    fn brute(m: &ImportedMesh, p: [f32; 3]) -> f64 {
        let pd = v64(p);
        m.triangles
            .iter()
            .map(|t| tri_d2(pd, &Tri { a: m.positions[t[0] as usize], b: m.positions[t[1] as usize], c: m.positions[t[2] as usize] }))
            .fold(f64::INFINITY, f64::min)
            .sqrt()
    }

    fn rng(seed: u32) -> impl FnMut() -> f32 {
        let mut x = seed;
        move || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / 16_777_216.0
        }
    }

    fn quick(res: u32) -> BakeOptions {
        BakeOptions { res, ..BakeOptions::default() }
    }

    fn node_pos(v: &Volume, i: usize, j: usize, k: usize) -> [f32; 3] {
        [v.origin[0] + i as f32 * v.cell, v.origin[1] + j as f32 * v.cell, v.origin[2] + k as f32 * v.cell]
    }

    fn box_sdf(p: [f32; 3], lo: [f32; 3], hi: [f32; 3]) -> f32 {
        let c = [0, 1, 2].map(|a| (lo[a] + hi[a]) * 0.5);
        let h = [0, 1, 2].map(|a| (hi[a] - lo[a]) * 0.5);
        let q = [0, 1, 2].map(|a| (p[a] - c[a]).abs() - h[a]);
        let o = q.map(|x| x.max(0.0));
        (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt() + q[0].max(q[1]).max(q[2]).min(0.0)
    }

    #[test]
    fn the_bvh_matches_brute_force() {
        let m = sphere_mesh(10.0, 20, 40);
        let tris: Vec<Tri> =
            m.triangles.iter().map(|t| Tri { a: m.positions[t[0] as usize], b: m.positions[t[1] as usize], c: m.positions[t[2] as usize] }).collect();
        let bvh = Bvh::build(&tris);
        assert_eq!(bvh.tris.len(), tris.len());
        let mut r = rng(3);
        let mut hint = 0;
        for _ in 0..2000 {
            let p = [r() * 40.0 - 20.0, r() * 40.0 - 20.0, r() * 40.0 - 20.0];
            let (d2, slot) = bvh.nearest(p, hint, 0.0, 0.0, &mut 0);
            hint = slot;
            assert!((d2.sqrt() - brute(&m, p)).abs() < 1e-9, "{p:?}");
        }
        // a bad hint does no harm
        assert!((bvh.nearest([0.0; 3], 99_999_999, 0.0, 0.0, &mut 0).0.sqrt() - brute(&m, [0.0; 3])).abs() < 1e-9);
    }

    #[test]
    fn triangle_distance_covers_every_region() {
        let t = Tri { a: [0.0, 0.0, 0.0], b: [4.0, 0.0, 0.0], c: [0.0, 4.0, 0.0] };
        let d = |p: [f64; 3]| tri_d2(p, &t).sqrt();
        assert!((d([1.0, 1.0, 3.0]) - 3.0).abs() < 1e-12); // face
        assert!((d([-1.0, -1.0, 0.0]) - 2f64.sqrt()).abs() < 1e-12); // vertex a
        assert!((d([5.0, -1.0, 0.0]) - 2f64.sqrt()).abs() < 1e-12); // vertex b
        assert!((d([-1.0, 5.0, 0.0]) - 2f64.sqrt()).abs() < 1e-12); // vertex c
        assert!((d([2.0, -3.0, 0.0]) - 3.0).abs() < 1e-12); // edge ab
        assert!((d([-3.0, 2.0, 0.0]) - 3.0).abs() < 1e-12); // edge ac
        assert!((d([3.0, 3.0, 0.0]) - 2f64.sqrt()).abs() < 1e-12);
        assert!(tri_d2([1.0, 1.0, 1.0], &Tri { a: [0.0; 3], b: [0.0; 3], c: [0.0; 3] }).is_finite());
    }

    #[test]
    fn a_sphere_gives_the_right_distances_and_signs() {
        let r = 10.0f32;
        let m = sphere_mesh(r, 96, 192);
        let (v, st) = bake_with(&m, &quick(48), &BakeControl::new()).unwrap();
        assert!(st.used_winding);
        assert!(st.split_votes * 1000 < st.cells, "a closed mesh: the three rays agree nearly everywhere ({})", st.split_votes);
        assert_eq!(v.dims[0] as usize * v.dims[1] as usize * v.dims[2] as usize, v.data.len());
        let tol = 0.01 * v.cell;
        let mut worst = 0f32;
        let d = v.dims.map(|x| x as usize);
        for k in 0..d[2] {
            for j in 0..d[1] {
                for i in 0..d[0] {
                    let p = node_pos(&v, i, j, k);
                    let truth = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r;
                    let got = v.at(i, j, k);
                    if truth.abs() > 0.2 * v.cell {
                        assert_eq!(got < 0.0, truth < 0.0, "sign at {p:?}");
                    }
                    if truth.abs() < 6.0 * v.cell {
                        worst = worst.max((got - truth).abs());
                    } else {
                        // far from the surface the field is interpolated: close, but only to a cell or so
                        assert!((got - truth).abs() < 3.0 * v.cell, "far value at {p:?}: {got} vs {truth}");
                    }
                }
            }
        }
        assert!(worst < tol.max(0.01), "worst {worst} vs 1% of a cell {tol}");
        // measuring every node gives the 1% accuracy everywhere
        let exact = bake(&m, &BakeOptions { far_band: 0.0, ..quick(48) }).unwrap();
        let mut worst = 0f32;
        for k in 0..d[2] {
            for j in 0..d[1] {
                for i in 0..d[0] {
                    let p = node_pos(&exact, i, j, k);
                    worst = worst.max((exact.at(i, j, k) - ((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r)).abs());
                }
            }
        }
        assert!(worst < tol.max(0.01), "exact worst {worst}");
        // the padding is at least `pad` cells wide on every side
        let (lo, hi) = v.bounds();
        assert!(lo[0] <= -r - 4.0 * v.cell + 1e-3 && hi[2] >= r + 4.0 * v.cell - 1e-3);
        // sampling at the centre and just outside
        assert!((v.distance([0.0, 0.0, 0.0]) + r).abs() < 0.05);
        assert!((v.distance([r + 1.0, 0.0, 0.0]) - 1.0).abs() < 0.05);
    }

    #[test]
    fn a_box_matches_its_analytic_distance() {
        let (lo, hi) = ([-3.0f32, -2.0, -1.0], [3.0f32, 2.0, 1.0]);
        let m = box_mesh(lo, hi);
        let v = bake(&m, &quick(60)).unwrap();
        let d = v.dims.map(|x| x as usize);
        let mut worst = 0f32;
        for k in 0..d[2] {
            for j in 0..d[1] {
                for i in 0..d[0] {
                    let p = node_pos(&v, i, j, k);
                    let truth = box_sdf(p, lo, hi);
                    // nodes within a hair of the box's faces may be classified either way
                    if truth.abs() > 0.02 * v.cell {
                        assert_eq!(v.at(i, j, k) < 0.0, truth < 0.0, "sign at {p:?}");
                        worst = worst.max((v.at(i, j, k) - truth).abs());
                    }
                }
            }
        }
        assert!(worst < 0.01 * v.cell, "{worst}");
    }

    #[test]
    fn an_open_box_is_still_inside_in_the_middle() {
        // a unit-ish box without its top: 10 of the 12 triangles
        let mut m = box_mesh([0.0; 3], [10.0; 3]);
        m.triangles.truncate(10);
        let (v, st) = bake_with(&m, &quick(40), &BakeControl::new()).unwrap();
        assert!(st.used_winding);
        assert!(v.distance([5.0, 5.0, 2.0]) < 0.0, "deep inside the cup");
        assert!(v.distance([5.0, 5.0, 4.0]) < 0.0);
        assert!(v.distance([5.0, 5.0, 14.0]) > 0.0, "above the opening");
        assert!(v.distance([-3.0, 5.0, 5.0]) > 0.0, "outside the wall");
        assert!(v.distance([5.0, -3.0, 5.0]) > 0.0);
        // magnitudes are the true distance to the surface wherever the sign is clear
        assert!((v.distance([-3.0, 5.0, 5.0]) - 3.0).abs() < 0.01);
        assert!(v.data.iter().all(|d| d.is_finite()));
    }

    #[test]
    fn overlapping_shells_are_solid_and_flipped_faces_do_not_matter() {
        let a = sphere_mesh(8.0, 48, 96);
        let mut b = sphere_mesh(8.0, 48, 96);
        for p in &mut b.positions {
            p[0] += 6.0;
        }
        let n = a.positions.len() as u32;
        let mut two = a.clone();
        two.positions.extend_from_slice(&b.positions);
        two.triangles.extend(b.triangles.iter().map(|t| [t[0] + n, t[1] + n, t[2] + n]));
        let (v, st) = bake_with(&two, &quick(48), &BakeControl::new()).unwrap();
        assert!(st.used_winding);
        // the lens where the spheres overlap is inside (parity alone would call it outside)
        assert!(v.distance([3.0, 0.0, 0.0]) < -1.0, "{}", v.distance([3.0, 0.0, 0.0]));
        assert!(v.distance([-6.0, 0.0, 0.0]) < 0.0);
        assert!(v.distance([12.0, 0.0, 0.0]) < 0.0);
        assert!(v.distance([3.0, 0.0, 12.0]) > 0.0);
        // a sphere where every 5th triangle is turned around: parity takes over and still works
        let mut s = sphere_mesh(8.0, 48, 96);
        for (i, t) in s.triangles.iter_mut().enumerate() {
            if i % 5 == 0 {
                t.swap(1, 2);
            }
        }
        let (v, st) = bake_with(&s, &quick(40), &BakeControl::new()).unwrap();
        assert!(!st.used_winding);
        assert!(v.distance([0.0; 3]) < -7.0);
        assert!(v.distance([5.0, 2.0, 1.0]) < -1.0);
        assert!(v.distance([12.0, 0.0, 0.0]) > 3.0);
        // an entirely inverted sphere has the same inside
        let mut inv = sphere_mesh(8.0, 48, 96);
        for t in &mut inv.triangles {
            t.swap(1, 2);
        }
        let v = bake(&inv, &quick(40)).unwrap();
        assert!(v.distance([0.0; 3]) < -7.0 && v.distance([12.0, 0.0, 0.0]) > 3.0);
    }

    #[test]
    fn trilinear_sampling_is_exact_at_nodes_continuous_and_extrapolates() {
        let m = sphere_mesh(10.0, 40, 80);
        let v = bake(&m, &quick(32)).unwrap();
        let d = v.dims.map(|x| x as usize);
        for &(i, j, k) in &[(0, 0, 0), (5, 7, 9), (d[0] - 1, d[1] - 1, d[2] - 1), (d[0] / 2, d[1] / 2, d[2] / 2)] {
            assert!((v.distance(node_pos(&v, i, j, k)) - v.at(i, j, k)).abs() < 1e-4);
        }
        // the cell-centre value is the mean of the eight corners
        let c = [v.origin[0] + 6.5 * v.cell, v.origin[1] + 7.5 * v.cell, v.origin[2] + 8.5 * v.cell];
        let mut mean = 0.0;
        for (a, b, e) in [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)] {
            mean += v.at(6 + a, 7 + b, 8 + e) / 8.0;
        }
        assert!((v.distance(c) - mean).abs() < 1e-4);
        // continuity and a Lipschitz bound, also across the border of the box
        let (lo, hi) = v.bounds();
        let mut r = rng(11);
        let mut worst = 0f32;
        for _ in 0..4000 {
            let p = [0, 1, 2].map(|a| lo[a] - 6.0 + r() * (hi[a] - lo[a] + 12.0));
            let dir = [r() - 0.5, r() - 0.5, r() - 0.5];
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
            let h = 0.05 * v.cell;
            let q = [0, 1, 2].map(|a| p[a] + dir[a] / len * h);
            let step = (v.distance(p) - v.distance(q)).abs();
            worst = worst.max(step / h);
        }
        assert!(worst < 1.8, "slope {worst}");
        // far away: the distance to the box plus the border value
        let far = [hi[0] + 100.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let edge = [hi[0], far[1], far[2]];
        assert!((v.distance(far) - (v.distance(edge) + 100.0)).abs() < 1e-3);
        assert!(v.distance(far) > 100.0);
        // not a number and degenerate volumes
        assert!(v.distance([f32::NAN, 0.0, 0.0]) > 1.0e8);
        assert!(v.distance([f32::INFINITY, 0.0, 0.0]) > 1.0e8);
        let broken = Volume { dims: [2, 2, 2], origin: [0.0; 3], cell: 1.0, data: vec![0.0; 3] };
        assert!(broken.distance([0.5; 3]) > 1.0e8);
        let flat = Volume { dims: [1, 1, 1], origin: [0.0; 3], cell: 1.0, data: vec![-2.0] };
        assert!((flat.distance([0.0; 3]) + 2.0).abs() < 1e-6);
        assert!((flat.distance([3.0, 4.0, 0.0]) - 3.0).abs() < 1e-5);
        // the gradient of a sphere points outwards, with length about 1
        let g = v.gradient([10.0 + 2.0, 0.0, 0.0]);
        assert!(g[0] > 0.8 && g[1].abs() < 0.2 && g[2].abs() < 0.2, "{g:?}");
    }

    #[test]
    fn the_grid_follows_the_model_and_the_caps() {
        let m = box_mesh([0.0; 3], [100.0, 10.0, 1.0]);
        let v = bake(&m, &quick(100)).unwrap();
        assert!((v.cell - 1.0).abs() < 1e-4);
        // 100 cells + 1 node + 2 * 4 padding on the long side; thin sides are padded too
        assert_eq!(v.dims, [109, 19, 10]);
        // a smaller budget lowers the resolution
        let small = bake(&m, &BakeOptions { res: 100, max_cells: 5000, ..BakeOptions::default() }).unwrap();
        assert!(small.data.len() <= 5000 && small.cell > v.cell);
        // a flat triangle (no thickness) still bakes: unsigned distance everywhere
        let flat = ImportedMesh { positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]], triangles: vec![[0, 1, 2]] };
        let f = bake(&flat, &quick(20)).unwrap();
        assert!(f.distance([2.0, 2.0, 3.0]) > 2.5 && f.distance([2.0, 2.0, 3.0]) < 3.5);
        assert!(f.data.iter().all(|d| d.is_finite()));
    }

    #[test]
    fn hostile_meshes_are_refused() {
        let ok = box_mesh([0.0; 3], [1.0; 3]);
        assert!(bake(&ImportedMesh::default(), &quick(16)).is_err());
        let mut nan = ok.clone();
        nan.positions[0][0] = f32::NAN;
        assert!(bake(&nan, &quick(16)).is_err());
        let mut inf = ok.clone();
        inf.positions[3][1] = f32::INFINITY;
        assert!(bake(&inf, &quick(16)).is_err());
        let mut idx = ok.clone();
        idx.triangles[0][2] = 99;
        assert!(bake(&idx, &quick(16)).is_err());
        let mut huge = ok.clone();
        huge.positions[0][0] = 3.0e9;
        assert!(bake(&huge, &quick(16)).is_err());
        let point = ImportedMesh { positions: vec![[1.0; 3]; 3], triangles: vec![[0, 1, 2]] };
        assert!(bake(&point, &quick(16)).is_err());
        // absurd options are clamped, not obeyed
        let v = bake(&ok, &BakeOptions { res: u32::MAX, pad: u32::MAX, max_cells: usize::MAX, threads: 1000, far_band: f32::NAN }).unwrap();
        assert!(v.data.len() <= MAX_CELLS && v.dims.iter().all(|d| *d <= MAX_AXIS));
        let v = bake(&ok, &BakeOptions { res: 0, pad: 0, max_cells: 0, threads: 1, far_band: -3.0 }).unwrap();
        assert!(v.data.len() <= 100_000);
        // thousands of giant overlapping triangles: refused or finished, never a memory bomb
        let mut big = ImportedMesh {
            positions: vec![[-1000.0, -1000.0, 0.0], [1000.0, -1000.0, 0.0], [0.0, 1000.0, 0.0], [0.0, 0.0, 1000.0], [1.0, 1.0, 1.0]],
            triangles: Vec::new(),
        };
        for i in 0..3000u32 {
            big.triangles.push([0, 1, 2]);
            big.triangles.push([3, (i % 2) + 1, 4]);
        }
        let _ = bake(&big, &BakeOptions { res: 64, ..BakeOptions::default() });
    }

    #[test]
    fn cancel_and_progress_work() {
        let m = sphere_mesh(10.0, 40, 80);
        let ctl = BakeControl::new();
        ctl.cancel.store(true, Ordering::Relaxed);
        assert_eq!(bake_with(&m, &quick(32), &ctl).unwrap_err(), ERR_CANCEL);
        let ctl = BakeControl::new();
        bake_with(&m, &quick(32), &ctl).unwrap();
        assert_eq!(ctl.progress.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn single_and_many_threads_give_the_same_volume() {
        let m = sphere_mesh(10.0, 30, 60);
        let a = bake(&m, &BakeOptions { res: 32, threads: 1, ..BakeOptions::default() }).unwrap();
        let b = bake(&m, &BakeOptions { res: 32, threads: 7, ..BakeOptions::default() }).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn saving_round_trips_within_the_quantisation() {
        let m = sphere_mesh(10.0, 60, 120);
        let v = bake(&m, &quick(48)).unwrap();
        let bytes = v.to_bytes();
        let w = Volume::from_bytes(&bytes).unwrap();
        assert_eq!((w.dims, w.origin, w.cell), (v.dims, v.origin, v.cell));
        let step = v.cell / f32::from(STEPS_PER_CELL);
        let worst = v.data.iter().zip(&w.data).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(worst <= step * 0.5 + 1e-5, "{worst} vs {step}");
        // saving twice is stable
        assert_eq!(Volume::from_bytes(&w.to_bytes()).unwrap(), w);
        let text = v.to_base64();
        assert!(text.is_ascii() && !text.contains(char::is_whitespace));
        assert_eq!(Volume::from_base64(&text).unwrap(), w);
        let wrapped: String = text.as_bytes().chunks(76).map(|c| String::from_utf8_lossy(c).into_owned() + "\n").collect();
        assert_eq!(Volume::from_base64(&wrapped).unwrap(), w);
        // the saved form is much smaller than the floats
        assert!(bytes.len() * 4 < v.data.len() * 4, "{} bytes for {} cells", bytes.len(), v.data.len());
        // extreme values do not break the predictor
        let wild = Volume { dims: [3, 3, 3], origin: [0.0; 3], cell: 1.0, data: (0..27).map(|i| if i % 2 == 0 { 1.0e6 } else { -1.0e6 }).collect() };
        let back = Volume::from_bytes(&wild.to_bytes()).unwrap();
        assert!(back.data.iter().all(|d| d.abs() > 500.0 && d.is_finite()));
    }

    #[test]
    fn hostile_saved_volumes_are_refused() {
        let v = Volume { dims: [4, 4, 4], origin: [0.0; 3], cell: 1.0, data: (0..64).map(|i| i as f32 * 0.1).collect() };
        let good = v.to_bytes();
        assert!(Volume::from_bytes(&good).is_ok());
        let put = |o: usize, bytes: &[u8]| {
            let mut b = good.clone();
            b[o..o + bytes.len()].copy_from_slice(bytes);
            b
        };
        assert!(Volume::from_bytes(&put(0, b"XXXX")).is_err());
        assert!(Volume::from_bytes(&put(4, &[9])).is_err());
        assert!(Volume::from_bytes(&put(5, &[0])).is_err());
        assert!(Volume::from_bytes(&put(6, &0u32.to_le_bytes())).is_err());
        assert!(Volume::from_bytes(&put(6, &u32::MAX.to_le_bytes())).is_err());
        assert!(Volume::from_bytes(&put(6, &1024u32.to_le_bytes())).is_err()); // 1024*4*4 cells need more data than there is
        let mut three_big = good.clone();
        for o in [6, 10, 14] {
            three_big[o..o + 4].copy_from_slice(&1024u32.to_le_bytes());
        }
        assert!(Volume::from_bytes(&three_big).is_err()); // 2^30 cells: refused before any allocation
        assert!(Volume::from_bytes(&put(18, &f32::NAN.to_le_bytes())).is_err());
        assert!(Volume::from_bytes(&put(30, &f32::NAN.to_le_bytes())).is_err());
        assert!(Volume::from_bytes(&put(30, &0.0f32.to_le_bytes())).is_err());
        assert!(Volume::from_bytes(&put(30, &1.0e9f32.to_le_bytes())).is_err());
        for cut in 0..good.len() {
            assert!(Volume::from_bytes(&good[..cut]).is_err() || cut == good.len());
        }
        let mut x = 1u32;
        for i in HEADER..good.len() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut b = good.clone();
            b[i] = (x >> 24) as u8;
            let _ = Volume::from_bytes(&b);
        }
        // a deflate bomb that expands to far more than the volume holds
        let mut bomb = good[..HEADER].to_vec();
        bomb.extend_from_slice(&crate::export_deflate::deflate(&vec![0u8; 10_000_000]));
        assert!(Volume::from_bytes(&bomb).is_err());
        // too few bytes after the header
        let mut short = good[..HEADER].to_vec();
        short.extend_from_slice(&crate::export_deflate::deflate(&[0u8; 10]));
        assert!(Volume::from_bytes(&short).is_err());
        // base64 mistakes
        for bad in ["", "!!!!", "TVNE", "TVNERg=x", "====", &"A".repeat(100)] {
            assert!(Volume::from_base64(bad).is_err(), "{bad}");
        }
        assert!(Volume::from_base64(&"A".repeat(MAX_SERIAL_CHARS + 1)).is_err());
        let text = v.to_base64();
        for cut in 0..text.len() {
            let _ = Volume::from_base64(&text[..cut]);
        }
    }

    #[test]
    fn half_floats() {
        for (x, h) in [
            (0.0f32, 0u16),
            (1.0, 0x3C00),
            (-2.0, 0xC000),
            (0.5, 0x3800),
            (65504.0, 0x7BFF),
            (1.0e9, 0x7BFF),
            (-1.0e9, 0xFBFF),
            (6.103_515_6e-5, 0x0400),
            (1.0e-9, 0),
            (f32::NAN, 0),
            (0.333_333_34, 0x3555),
            (100.1, 0x5642),
        ] {
            assert_eq!(f32_to_f16_bits(x), h, "{x}");
        }
        // within half a unit in the last place for ordinary values
        let mut r = rng(8);
        for _ in 0..2000 {
            let x = (r() - 0.5) * 200.0;
            let h = f32_to_f16_bits(x);
            let back = {
                let e = i32::from((h >> 10) & 31);
                let m = f32::from(h & 1023);
                let v = if e == 0 { m * 2f32.powi(-24) } else { (1.0 + m / 1024.0) * 2f32.powi(e - 15) };
                if h & 0x8000 != 0 { -v } else { v }
            };
            assert!((back - x).abs() <= x.abs() * 0.000_5 + 1e-7, "{x} -> {back}");
        }
        let v = Volume { dims: [2, 1, 1], origin: [0.0; 3], cell: 1.0, data: vec![1.0, -2.0] };
        assert_eq!(v.to_f16_bits(), vec![0x3C00, 0xC000]);
    }

    #[test]
    fn zigzag_and_the_predictor_invert() {
        for v in [0u16, 1, 2, 3, 0x7FFF, 0x8000, 0xFFFF, 12345, 54321] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
    }

    /// A torus (ring of radius `big`, tube radius `small`) with `u * v * 2` triangles.
    fn torus_mesh(big: f32, small: f32, u: usize, v: usize) -> ImportedMesh {
        let mut positions = Vec::new();
        for i in 0..u {
            let a = std::f32::consts::TAU * i as f32 / u as f32;
            for j in 0..v {
                let b = std::f32::consts::TAU * j as f32 / v as f32;
                let r = big + small * b.cos();
                positions.push([r * a.cos(), r * a.sin(), small * b.sin()]);
            }
        }
        let id = |i: usize, j: usize| ((i % u) * v + j % v) as u32;
        let mut triangles = Vec::new();
        for i in 0..u {
            for j in 0..v {
                triangles.push([id(i, j), id(i + 1, j), id(i, j + 1)]);
                triangles.push([id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        ImportedMesh { positions, triangles }
    }

    /// A lumpy blob: a sphere whose radius wobbles.
    fn blob_mesh(rings: usize, segs: usize) -> ImportedMesh {
        let mut m = sphere_mesh(1.0, rings, segs);
        for p in &mut m.positions {
            let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            let (th, ph) = ((p[2] / l).acos(), p[1].atan2(p[0]));
            let r = 25.0 * (1.0 + 0.25 * (3.0 * th).sin() * (2.0 * ph).cos() + 0.1 * (7.0 * ph).sin() * (5.0 * th).cos());
            *p = [p[0] / l * r, p[1] / l * r, p[2] / l * r];
        }
        m
    }

    /// Run with `--ignored --nocapture` to see the timings and sizes.
    #[test]
    #[ignore = "timing report"]
    fn report_timing_and_size_for_100k_triangles() {
        let shapes: Vec<(&str, ImportedMesh)> = vec![
            ("sphere 100k", sphere_mesh(30.0, 160, 320)),
            ("torus 100k", torus_mesh(30.0, 10.0, 360, 140)),
            ("blob 100k", blob_mesh(160, 320)),
            ("blob 14k", blob_mesh(60, 120)),
        ];
        for (name, m) in shapes {
            println!("{name}: {} triangles", m.triangles.len());
            for res in [64u32, 128, 160] {
                let t = Instant::now();
                let (v, st) = bake_with(&m, &BakeOptions { res, ..BakeOptions::default() }, &BakeControl::new()).unwrap();
                let total = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let bytes = v.to_bytes();
                let save = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let back = Volume::from_bytes(&bytes).unwrap();
                let load = t.elapsed().as_secs_f64();
                let b64 = v.to_base64().len();
                println!(
                    "res {res}: dims {:?} ({} cells) bake {total:.2}s ({} steps per node) [bvh {:.2}s, signs {:.2}s, distances {:.2}s] save {save:.2}s load {load:.2}s -> {} bytes, base64 {} chars, winding {}",
                    v.dims,
                    st.cells,
                    st.tree_steps / st.cells as u64,
                    st.bvh_secs,
                    st.sign_secs,
                    st.dist_secs,
                    bytes.len(),
                    b64,
                    st.used_winding
                );
                assert_eq!(back.dims, v.dims);
            }
        }
    }
}
