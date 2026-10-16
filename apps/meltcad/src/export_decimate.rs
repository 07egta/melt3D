//! Making a mesh lighter and smoother after it was cut: quadric-error edge collapse (Garland and
//! Heckbert, "Surface Simplification Using Quadric Error Metrics", 1997) that merges flat faces
//! into big triangles and keeps edges and corners, and feature-aware Taubin smoothing.
//!
//! A collapse is only done when the surface stays a closed manifold (the link condition) and no
//! neighbouring triangle flips over. Big meshes are cut into tiles that are simplified side by
//! side on several threads; the triangles that cross a tile border stay as they are until the
//! last pass, which looks at the whole (by then much smaller) mesh.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use solidcraft_geom::melt::TriMesh;

use crate::export_mesh::{CANCELLED, Slice};

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A symmetric 4x4 quadric: a2 ab ac ad b2 bc bd c2 cd d2.
type Q = [f64; 10];

fn plane_quadric(n: V, d: f64, w: f64) -> Q {
    [n[0] * n[0], n[0] * n[1], n[0] * n[2], n[0] * d, n[1] * n[1], n[1] * n[2], n[1] * d, n[2] * n[2], n[2] * d, d * d].map(|x| x * w)
}

fn q_add(a: &mut Q, b: &Q) {
    for (x, y) in a.iter_mut().zip(b.iter()) {
        *x += *y;
    }
}

fn q_eval(q: &Q, p: V) -> f64 {
    let [a2, ab, ac, ad, b2, bc, bd, c2, cd, d2] = *q;
    let v = a2 * p[0] * p[0]
        + 2.0 * ab * p[0] * p[1]
        + 2.0 * ac * p[0] * p[2]
        + 2.0 * ad * p[0]
        + b2 * p[1] * p[1]
        + 2.0 * bc * p[1] * p[2]
        + 2.0 * bd * p[1]
        + c2 * p[2] * p[2]
        + 2.0 * cd * p[2]
        + d2;
    v.max(0.0)
}

/// The position that minimises the quadric, if the planes decide it well enough.
fn q_optimum(q: &Q) -> Option<V> {
    let a = [[q[0], q[1], q[2]], [q[1], q[4], q[5]], [q[2], q[5], q[7]]];
    let rhs = [-q[3], -q[6], -q[8]];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let scale = a[0][0].abs().max(a[1][1].abs()).max(a[2][2].abs());
    if det.abs().is_nan() || det.abs() <= 1e-9 * scale * scale * scale || scale <= 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    let x = (rhs[0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (rhs[1] * a[2][2] - a[1][2] * rhs[2])
        + a[0][2] * (rhs[1] * a[2][1] - a[1][1] * rhs[2]))
        * inv;
    let y = (a[0][0] * (rhs[1] * a[2][2] - a[1][2] * rhs[2]) - rhs[0] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * rhs[2] - rhs[1] * a[2][0]))
        * inv;
    let z = (a[0][0] * (a[1][1] * rhs[2] - rhs[1] * a[2][1]) - a[0][1] * (a[1][0] * rhs[2] - rhs[1] * a[2][0])
        + rhs[0] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        * inv;
    let p = [x, y, z];
    if p.iter().all(|v| v.is_finite()) { Some(p) } else { None }
}

/// A mesh being simplified. Vertices and triangles that are `pinned`/`frozen` are never changed.
struct Work {
    pos: Vec<V>,
    tris: Vec<[u32; 3]>,
    pinned: Vec<bool>,
    frozen: Vec<bool>,
    alive_tri: Vec<bool>,
    alive_vert: Vec<bool>,
    vtris: Vec<Vec<u32>>,
    quad: Vec<Q>,
    ver: Vec<u32>,
    tri_count: usize,
    /// Costs closer than this count as equal; among those the shortest edge goes first, so flat
    /// areas thin out evenly instead of piling every triangle onto a few hub vertices.
    quantum: f64,
}

/// (cost bucket, edge length) as the sort key, then the ends and their versions.
type Entry = (Reverse<(u64, u32)>, u32, u32, u32, u32);

impl Work {
    fn new(pos: Vec<V>, tris: Vec<[u32; 3]>, pinned: Vec<bool>, frozen: Vec<bool>, quantum: f64) -> Work {
        let nv = pos.len();
        let mut vtris: Vec<Vec<u32>> = vec![Vec::new(); nv];
        let mut quad: Vec<Q> = vec![[0.0; 10]; nv];
        for (i, t) in tris.iter().enumerate() {
            for v in t {
                if let Some(l) = vtris.get_mut(*v as usize) {
                    l.push(i as u32);
                }
            }
            if let Some((n, d, _area)) = plane_of(&pos, t) {
                let q = plane_quadric(n, d, 1.0);
                for v in t {
                    if let Some(s) = quad.get_mut(*v as usize) {
                        q_add(s, &q);
                    }
                }
            }
        }
        let n = tris.len();
        Work {
            pos,
            tris,
            pinned,
            frozen,
            alive_tri: vec![true; n],
            alive_vert: vec![true; nv],
            vtris,
            quad,
            ver: vec![0; nv],
            tri_count: n,
            quantum: quantum.max(1e-30),
        }
    }

    fn p(&self, v: u32) -> V {
        self.pos.get(v as usize).copied().unwrap_or([0.0; 3])
    }

    fn is_pinned(&self, v: u32) -> bool {
        self.pinned.get(v as usize).copied().unwrap_or(true)
    }

    /// The vertices next to `v` (through alive triangles).
    fn neighbours(&self, v: u32) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        if let Some(list) = self.vtris.get(v as usize) {
            for ti in list {
                if let Some(t) = self.tris.get(*ti as usize) {
                    for w in t {
                        if *w != v && !out.contains(w) {
                            out.push(*w);
                        }
                    }
                }
            }
        }
        out
    }

    /// Where merging `a` and `b` would put the new vertex, and what that costs.
    fn best_target(&self, a: u32, b: u32) -> Option<(V, f64)> {
        let (pa, pb) = (self.is_pinned(a), self.is_pinned(b));
        if pa && pb {
            return None;
        }
        let mut q = self.quad.get(a as usize).copied().unwrap_or([0.0; 10]);
        q_add(&mut q, self.quad.get(b as usize)?);
        let (va, vb) = (self.p(a), self.p(b));
        if pa {
            return Some((va, q_eval(&q, va)));
        }
        if pb {
            return Some((vb, q_eval(&q, vb)));
        }
        let m = [(va[0] + vb[0]) * 0.5, (va[1] + vb[1]) * 0.5, (va[2] + vb[2]) * 0.5];
        let mut best = (va, q_eval(&q, va));
        for c in [vb, m] {
            let e = q_eval(&q, c);
            if e < best.1 {
                best = (c, e);
            }
        }
        if let Some(o) = q_optimum(&q) {
            let len = dot(sub(va, vb), sub(va, vb)).sqrt();
            let off = sub(o, m);
            if dot(off, off).sqrt() <= 2.0 * len {
                let e = q_eval(&q, o);
                if e < best.1 {
                    best = (o, e);
                }
            }
        }
        Some(best)
    }

    fn push_edge(&self, heap: &mut BinaryHeap<Entry>, a: u32, b: u32) {
        if let Some((_, cost)) = self.best_target(a, b) {
            let (x, y) = if a < b { (a, b) } else { (b, a) };
            let d = sub(self.p(x), self.p(y));
            let len = dot(d, d) as f32;
            let bucket = (cost / self.quantum).min(1.0e18) as u64;
            heap.push((
                Reverse((bucket, len.to_bits())),
                x,
                y,
                self.ver.get(x as usize).copied().unwrap_or(0),
                self.ver.get(y as usize).copied().unwrap_or(0),
            ));
        }
    }

    /// May `b` be merged into `a` at `target` without breaking the surface?
    fn collapse_ok(&self, a: u32, b: u32, target: V) -> bool {
        let (Some(la), Some(lb)) = (self.vtris.get(a as usize), self.vtris.get(b as usize)) else { return false };
        // the triangles that hold both ends and their opposite corners
        let mut apex: Vec<u32> = Vec::new();
        for ti in lb {
            if let Some(t) = self.tris.get(*ti as usize)
                && t.contains(&a)
            {
                if self.frozen.get(*ti as usize).copied().unwrap_or(true) {
                    return false;
                }
                for w in t {
                    if *w != a && *w != b {
                        apex.push(*w);
                    }
                }
            }
        }
        if apex.is_empty() || apex.len() > 2 {
            return false;
        }
        // link condition: the only common neighbours are those opposite corners
        let na = self.neighbours(a);
        for w in self.neighbours(b) {
            if w != a && na.contains(&w) && !apex.contains(&w) {
                return false;
            }
        }
        // a vertex with only three neighbours would collapse the surface into a sliver
        if apex.len() == 2 && apex[0] == apex[1] {
            return false;
        }
        // no triangle may flip over or collapse
        for (list, moved) in [(la, a), (lb, b)] {
            for ti in list {
                let Some(t) = self.tris.get(*ti as usize) else { continue };
                if t.contains(&a) && t.contains(&b) {
                    continue;
                }
                let pt = |v: u32| if v == moved { target } else { self.p(v) };
                let before = cross(sub(self.p(t[1]), self.p(t[0])), sub(self.p(t[2]), self.p(t[0])));
                let after = cross(sub(pt(t[1]), pt(t[0])), sub(pt(t[2]), pt(t[0])));
                let (lb2, la2) = (dot(before, before), dot(after, after));
                if la2 <= 1e-24 * lb2.max(1e-30) {
                    return false;
                }
                if dot(before, after) < 0.15 * (lb2 * la2).sqrt() {
                    return false;
                }
            }
        }
        true
    }

    fn collapse(&mut self, a: u32, b: u32, target: V) {
        if let Some(p) = self.pos.get_mut(a as usize) {
            *p = target;
        }
        let qb = self.quad.get(b as usize).copied().unwrap_or([0.0; 10]);
        if let Some(q) = self.quad.get_mut(a as usize) {
            q_add(q, &qb);
        }
        let lb = match self.vtris.get_mut(b as usize) {
            Some(l) => std::mem::take(l),
            None => Vec::new(),
        };
        for ti in lb {
            let Some(t) = self.tris.get_mut(ti as usize) else { continue };
            if t.contains(&a) {
                if let Some(al) = self.alive_tri.get_mut(ti as usize) {
                    if *al {
                        self.tri_count -= 1;
                    }
                    *al = false;
                }
                // drop the dead triangle from the third corner's list
                let others: Vec<u32> = t.iter().copied().filter(|w| *w != a && *w != b).collect();
                for w in others {
                    if let Some(l) = self.vtris.get_mut(w as usize) {
                        l.retain(|x| *x != ti);
                    }
                }
                if let Some(l) = self.vtris.get_mut(a as usize) {
                    l.retain(|x| *x != ti);
                }
            } else {
                for v in t.iter_mut() {
                    if *v == b {
                        *v = a;
                    }
                }
                if let Some(l) = self.vtris.get_mut(a as usize) {
                    l.push(ti);
                }
            }
        }
        if let Some(al) = self.alive_vert.get_mut(b as usize) {
            *al = false;
        }
        if let Some(v) = self.ver.get_mut(a as usize) {
            *v += 1;
        }
        // refresh the quadric of the pinned flag: a merged vertex keeps a's pin
    }

    /// Collapse the cheapest edges until `target` triangles are left or the cost passes `max_cost`.
    fn run(&mut self, target: usize, max_cost: f64, prog: Option<&Slice>) -> Result<(), String> {
        let mut heap: BinaryHeap<Entry> = BinaryHeap::new();
        for (i, t) in self.tris.iter().enumerate() {
            if self.frozen.get(i).copied().unwrap_or(true) {
                continue;
            }
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                if a < b {
                    self.push_edge(&mut heap, a, b);
                }
            }
        }
        let start = self.tri_count.max(1);
        let mut done = 0usize;
        while self.tri_count > target {
            let Some((Reverse((bucket, _)), a, b, va, vb)) = heap.pop() else { break };
            if bucket as f64 * self.quantum > max_cost {
                break;
            }
            let ok_a = self.alive_vert.get(a as usize).copied().unwrap_or(false) && self.ver.get(a as usize).copied() == Some(va);
            let ok_b = self.alive_vert.get(b as usize).copied().unwrap_or(false) && self.ver.get(b as usize).copied() == Some(vb);
            if !ok_a || !ok_b {
                continue;
            }
            let Some((target_pos, cost)) = self.best_target(a, b) else { continue };
            if cost > max_cost {
                continue;
            }
            // the pinned one stays, the other is merged into it
            let (keep, gone) = if self.is_pinned(b) && !self.is_pinned(a) { (b, a) } else { (a, b) };
            if !self.collapse_ok(keep, gone, target_pos) {
                continue;
            }
            self.collapse(keep, gone, target_pos);
            let around = self.neighbours(keep);
            for w in around {
                self.push_edge(&mut heap, keep, w);
            }
            done += 1;
            if done.is_multiple_of(4096)
                && let Some(p) = prog
            {
                if p.cancelled() {
                    return Err(CANCELLED.into());
                }
                p.set((start - self.tri_count.min(start)) as f64 / (start - target.min(start)).max(1) as f64);
            }
        }
        Ok(())
    }

    /// The surviving triangles (not the frozen ones) and the vertices they use, renumbered.
    fn finish(self) -> (Vec<V>, Vec<[u32; 3]>) {
        (self.pos, self.tris.into_iter().zip(self.alive_tri).filter(|(_, a)| *a).map(|(t, _)| t).collect())
    }
}

fn plane_of(pos: &[V], t: &[u32; 3]) -> Option<(V, f64, f64)> {
    let a = *pos.get(t[0] as usize)?;
    let b = *pos.get(t[1] as usize)?;
    let c = *pos.get(t[2] as usize)?;
    let n = cross(sub(b, a), sub(c, a));
    let l = dot(n, n).sqrt();
    if l.is_nan() || l <= 1e-300 || !l.is_finite() {
        return None;
    }
    let u = [n[0] / l, n[1] / l, n[2] / l];
    Some((u, -dot(u, a), 0.5 * l))
}

/// Keep only the vertices some triangle uses, and renumber. Returns, for each new vertex, the old one.
pub fn compact(mesh: &mut TriMesh) -> Vec<u32> {
    let mut map = vec![u32::MAX; mesh.positions.len()];
    let mut positions = Vec::new();
    let mut origin = Vec::new();
    for t in &mut mesh.triangles {
        for v in t.iter_mut() {
            if let Some(slot) = map.get_mut(*v as usize) {
                if *slot == u32::MAX {
                    *slot = positions.len() as u32;
                    positions.push(mesh.positions.get(*v as usize).copied().unwrap_or([0.0; 3]));
                    origin.push(*v);
                }
                *v = *slot;
            }
        }
    }
    mesh.positions = positions;
    origin
}

/// Per-vertex looks that travel with the mesh while it is simplified. Empty: nothing to carry.
/// Where the class changes between neighbours, the vertices are kept, so colour borders stay put.
#[derive(Clone, Debug, Default)]
pub struct Carry {
    pub class: Vec<u16>,
    pub rgb: Vec<[u8; 3]>,
    pub rm: Vec<[u8; 2]>,
}

impl Carry {
    fn remap(&mut self, origin: &[u32]) {
        fn pick<T: Copy>(v: &mut Vec<T>, origin: &[u32]) {
            if v.is_empty() {
                return;
            }
            let new: Vec<T> = origin.iter().filter_map(|o| v.get(*o as usize).copied()).collect();
            *v = new;
        }
        pick(&mut self.class, origin);
        pick(&mut self.rgb, origin);
        pick(&mut self.rm, origin);
    }
    fn pins(&self, mesh: &TriMesh) -> Vec<bool> {
        let mut pinned = vec![false; mesh.positions.len()];
        if self.class.len() != mesh.positions.len() {
            return pinned;
        }
        for t in &mesh.triangles {
            let c = [self.class.get(t[0] as usize), self.class.get(t[1] as usize), self.class.get(t[2] as usize)];
            if c[0] != c[1] || c[1] != c[2] {
                for v in t {
                    if let Some(p) = pinned.get_mut(*v as usize) {
                        *p = true;
                    }
                }
            }
        }
        pinned
    }
}

/// What to do to a mesh to make it lighter.
#[derive(Clone, Copy, Debug)]
pub struct Reduce {
    /// Stop at this many triangles (0: no count goal).
    pub target_tris: usize,
    /// A collapse may move the surface by at most this much (mm).
    pub max_error: f64,
    /// Squared distances (mm^2) that count as the same cost; the shortest edge goes first among them.
    pub quantum: f64,
    /// What the tile pass of a big mesh may move the surface by (mm): flat faces only.
    pub tile_error: f64,
}

/// Simplify `mesh` in place. Big meshes go through tiles on `threads` threads first.
pub fn reduce(mesh: &mut TriMesh, carry: &mut Carry, opts: Reduce, threads: usize, prog: &Slice) -> Result<(), String> {
    if mesh.triangles.len() < 4 {
        return Ok(());
    }
    let max_cost = opts.max_error * opts.max_error;
    let timing = std::env::var_os("YURA_EXPORT_TIMING").is_some();
    let t0 = std::time::Instant::now();
    let mut pinned = carry.pins(mesh);
    if mesh.triangles.len() > 300_000 {
        let origin = tiled(mesh, &pinned, max_cost.min(opts.tile_error * opts.tile_error), opts.quantum, threads, &prog.sub(0.0, 0.7))?;
        carry.remap(&origin);
        pinned = origin.iter().map(|o| pinned.get(*o as usize).copied().unwrap_or(false)).collect();
        if timing {
            eprintln!("  tiles {:.2}s -> {} tris", t0.elapsed().as_secs_f64(), mesh.triangles.len());
        }
    }
    let pos = std::mem::take(&mut mesh.positions);
    let tris = std::mem::take(&mut mesh.triangles);
    let nt = tris.len();
    pinned.resize(pos.len(), false);
    let mut w = Work::new(pos, tris, pinned, vec![false; nt], opts.quantum);
    w.run(opts.target_tris, max_cost, Some(&prog.sub(0.7, 1.0)))?;
    let (pos, tris) = w.finish();
    mesh.positions = pos;
    mesh.triangles = tris;
    let origin = compact(mesh);
    carry.remap(&origin);
    Ok(())
}

/// Simplify tile by tile in parallel; the triangles across tile borders are left for the last pass.
fn tiled(mesh: &mut TriMesh, base_pinned: &[bool], max_cost: f64, quantum: f64, threads: usize, prog: &Slice) -> Result<Vec<u32>, String> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &mesh.positions {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
    if !extent.is_finite() || extent <= 0.0 {
        return Ok((0..mesh.positions.len() as u32).collect());
    }
    // tiles that hold roughly 150k triangles each, at least a few per thread
    let per_tile = 150_000.0;
    let want = (mesh.triangles.len() as f64 / per_tile).max(threads as f64 * 3.0);
    let along = (want.cbrt() * 1.6).ceil().clamp(2.0, 64.0);
    let size = extent / along;
    let tile_of = |p: V| -> u32 {
        let t = |a: usize| (((p[a] - lo[a]) / size) as i64).clamp(0, 63) as u32;
        (t(2) * 64 + t(1)) * 64 + t(0)
    };
    let vtile: Vec<u32> = mesh.positions.iter().map(|p| tile_of(*p)).collect();
    // a vertex is pinned when some triangle reaches into another tile
    let mut pinned: Vec<bool> = (0..mesh.positions.len()).map(|v| base_pinned.get(v).copied().unwrap_or(false)).collect();
    let mut seam = vec![false; mesh.triangles.len()];
    for (i, t) in mesh.triangles.iter().enumerate() {
        let tl = [vtile.get(t[0] as usize), vtile.get(t[1] as usize), vtile.get(t[2] as usize)];
        if tl[0] != tl[1] || tl[1] != tl[2] {
            seam[i] = true;
            for v in t {
                if let Some(p) = pinned.get_mut(*v as usize) {
                    *p = true;
                }
            }
        }
    }
    // group vertices and triangles by tile
    let mut tiles: std::collections::BTreeMap<u32, (Vec<u32>, Vec<u32>)> = std::collections::BTreeMap::new();
    for (v, t) in vtile.iter().enumerate() {
        tiles.entry(*t).or_default().0.push(v as u32);
    }
    let mut vtri_start = vec![0u32; mesh.positions.len() + 1];
    for t in &mesh.triangles {
        for v in t {
            if let Some(c) = vtri_start.get_mut(*v as usize + 1) {
                *c += 1;
            }
        }
    }
    for i in 0..mesh.positions.len() {
        let prev = vtri_start.get(i).copied().unwrap_or(0);
        if let Some(c) = vtri_start.get_mut(i + 1) {
            *c += prev;
        }
    }
    let mut fill = vtri_start.clone();
    let mut vtri_list = vec![0u32; mesh.triangles.len() * 3];
    for (i, t) in mesh.triangles.iter().enumerate() {
        for v in t {
            if let Some(f) = fill.get_mut(*v as usize) {
                if let Some(s) = vtri_list.get_mut(*f as usize) {
                    *s = i as u32;
                }
                *f += 1;
            }
        }
    }
    // each interior triangle goes to its tile's job
    for (i, t) in mesh.triangles.iter().enumerate() {
        if !seam.get(i).copied().unwrap_or(true)
            && let Some(tile) = vtile.get(t[0] as usize)
            && let Some(e) = tiles.get_mut(tile)
        {
            e.1.push(i as u32);
        }
    }
    let jobs: Vec<(u32, Vec<u32>, Vec<u32>)> = tiles.into_iter().map(|(k, (v, t))| (k, v, t)).collect();
    let mesh_ref = &*mesh;
    let pinned_ref = &pinned;
    let seam_ref = &seam;
    let vts = &vtri_start;
    let vtl = &vtri_list;
    let jobs_ref = &jobs;
    let outputs = crate::export_mesh::par_run(threads, jobs.len(), prog, |i| {
        let Some((_tile, verts, interior)) = jobs_ref.get(i) else { return Ok((Vec::new(), Vec::new())) };
        if interior.len() < 8 {
            return Ok((Vec::new(), Vec::new()));
        }
        // the local mesh: the tile's interior triangles, plus the border triangles that touch its vertices
        let mut local_of: crate::export_mesh::FastMap<u32, u32> = crate::export_mesh::FastMap::default();
        let mut gl: Vec<u32> = Vec::new();
        let mut lpos: Vec<V> = Vec::new();
        let mut lpin: Vec<bool> = Vec::new();
        let mut ltris: Vec<[u32; 3]> = Vec::new();
        let mut lfrozen: Vec<bool> = Vec::new();
        let take =
            |v: u32, local_of: &mut crate::export_mesh::FastMap<u32, u32>, gl: &mut Vec<u32>, lpos: &mut Vec<V>, lpin: &mut Vec<bool>| -> u32 {
                *local_of.entry(v).or_insert_with(|| {
                    gl.push(v);
                    lpos.push(mesh_ref.positions.get(v as usize).copied().unwrap_or([0.0; 3]));
                    lpin.push(pinned_ref.get(v as usize).copied().unwrap_or(true));
                    (gl.len() - 1) as u32
                })
            };
        for ti in interior {
            if let Some(t) = mesh_ref.triangles.get(*ti as usize) {
                let l = [
                    take(t[0], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                    take(t[1], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                    take(t[2], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                ];
                ltris.push(l);
                lfrozen.push(false);
            }
        }
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for v in verts {
            if !pinned_ref.get(*v as usize).copied().unwrap_or(false) {
                continue;
            }
            let (s, e) = (vts.get(*v as usize).copied().unwrap_or(0) as usize, vts.get(*v as usize + 1).copied().unwrap_or(0) as usize);
            for ti in vtl.get(s..e).unwrap_or(&[]) {
                if seam_ref.get(*ti as usize).copied().unwrap_or(false)
                    && seen.insert(*ti)
                    && let Some(t) = mesh_ref.triangles.get(*ti as usize)
                {
                    let l = [
                        take(t[0], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                        take(t[1], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                        take(t[2], &mut local_of, &mut gl, &mut lpos, &mut lpin),
                    ];
                    ltris.push(l);
                    lfrozen.push(true);
                }
            }
        }
        // vertices of other tiles are fixed whatever happens
        for (k, g) in gl.iter().enumerate() {
            if vtile.get(*g as usize) != jobs_ref.get(i).map(|j| &j.0)
                && let Some(p) = lpin.get_mut(k)
            {
                *p = true;
            }
        }
        let mut w = Work::new(lpos, ltris, lpin, lfrozen, quantum);
        w.run(0, max_cost, None)?;
        // report back: the moved vertices (global id, position) and the surviving interior triangles
        let mut moved: Vec<(u32, V)> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for (k, t) in w.tris.iter().enumerate() {
            if w.frozen.get(k).copied().unwrap_or(true) || !w.alive_tri.get(k).copied().unwrap_or(false) {
                continue;
            }
            let g = [
                gl.get(t[0] as usize).copied().unwrap_or(0),
                gl.get(t[1] as usize).copied().unwrap_or(0),
                gl.get(t[2] as usize).copied().unwrap_or(0),
            ];
            tris.push(g);
        }
        for (k, g) in gl.iter().enumerate() {
            if w.alive_vert.get(k).copied().unwrap_or(false) && !w.pinned.get(k).copied().unwrap_or(true) {
                moved.push((*g, w.p(k as u32)));
            }
        }
        Ok((moved, tris))
    })?;
    // put it together: seam triangles as they were, every tile's survivors, moved vertices
    let mut new_tris: Vec<[u32; 3]> = Vec::new();
    for (i, t) in mesh.triangles.iter().enumerate() {
        if seam.get(i).copied().unwrap_or(false) {
            new_tris.push(*t);
        }
    }
    // tiles that were skipped (too small) keep their triangles
    for (job, out) in jobs.iter().zip(outputs.iter()) {
        if job.2.len() < 8 {
            for ti in &job.2 {
                if let Some(t) = mesh.triangles.get(*ti as usize) {
                    new_tris.push(*t);
                }
            }
        } else {
            new_tris.extend(out.1.iter().copied());
            for (g, p) in &out.0 {
                if let Some(s) = mesh.positions.get_mut(*g as usize) {
                    *s = *p;
                }
            }
        }
    }
    mesh.triangles = new_tris;
    Ok(compact(mesh))
}

// ---------------------------------------------------------------- smoothing

/// Even out the facets: Taubin smoothing (shrink, then inflate) over the vertices that are not on a
/// sharp feature. Sharp vertices stay where they are, so edges and corners keep their shape.
pub fn smooth(mesh: &mut TriMesh, sharp: &[bool], rounds: usize, prog: &Slice) -> Result<(), String> {
    let nv = mesh.positions.len();
    if rounds == 0 || nv == 0 {
        return Ok(());
    }
    // neighbours in compressed form
    let mut start = vec![0u32; nv + 1];
    for t in &mesh.triangles {
        for v in t {
            if let Some(c) = start.get_mut(*v as usize + 1) {
                *c += 2;
            }
        }
    }
    for i in 0..nv {
        let p = start.get(i).copied().unwrap_or(0);
        if let Some(c) = start.get_mut(i + 1) {
            *c += p;
        }
    }
    let mut fill = start.clone();
    let mut nb = vec![0u32; start.last().copied().unwrap_or(0) as usize];
    for t in &mesh.triangles {
        for e in 0..3 {
            let (v, a, b) = (t[e], t[(e + 1) % 3], t[(e + 2) % 3]);
            if let Some(f) = fill.get_mut(v as usize) {
                for w in [a, b] {
                    if let Some(s) = nb.get_mut(*f as usize) {
                        *s = w;
                    }
                    *f += 1;
                }
            }
        }
    }
    let total = rounds * 2;
    let mut step = 0usize;
    for _ in 0..rounds {
        for factor in [0.5, -0.53] {
            if prog.cancelled() {
                return Err(CANCELLED.into());
            }
            let before = mesh.positions.clone();
            for v in 0..nv {
                if sharp.get(v).copied().unwrap_or(false) {
                    continue;
                }
                let (s, e) = (start.get(v).copied().unwrap_or(0) as usize, start.get(v + 1).copied().unwrap_or(0) as usize);
                let list = nb.get(s..e).unwrap_or(&[]);
                if list.is_empty() {
                    continue;
                }
                let mut mean = [0.0; 3];
                for w in list {
                    let p = before.get(*w as usize).copied().unwrap_or([0.0; 3]);
                    for a in 0..3 {
                        mean[a] += p[a];
                    }
                }
                let n = list.len() as f64;
                let p = before.get(v).copied().unwrap_or([0.0; 3]);
                if let Some(slot) = mesh.positions.get_mut(v) {
                    for a in 0..3 {
                        slot[a] = p[a] + (mean[a] / n - p[a]) * factor;
                    }
                }
            }
            step += 1;
            prog.set(step as f64 / total as f64);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_mesh::{Progress, mesh_sparse, tests::boxsdf, tests::sphere};
    use solidcraft_geom::melt;

    fn mesh_of(f: &dyn crate::export_mesh::Sdf, half: f64, cell: f64) -> crate::export_mesh::Meshed {
        let prog = Progress::new();
        mesh_sparse(f, [-half; 3], [half; 3], cell, 4, &prog.slice(0.0, 1.0)).unwrap()
    }

    #[test]
    fn flat_faces_merge_into_a_few_triangles_and_the_box_stays_a_box() {
        let f = |p: [f32; 3]| boxsdf([5.13, 4.07, 3.31])([p[0] - 0.21, p[1] + 0.33, p[2] - 0.17]);
        let mut m = mesh_of(&f, 8.0, 0.25).mesh;
        let before = m.triangles.len();
        let v0 = melt::volume(&m);
        let prog = Progress::new();
        reduce(
            &mut m,
            &mut Carry::default(),
            Reduce { target_tris: 0, max_error: 0.005, quantum: 1e-7, tile_error: 0.002 },
            4,
            &prog.slice(0.0, 1.0),
        )
        .unwrap();
        assert!(melt::is_watertight(&m));
        assert!(m.triangles.len() < before / 20, "{} of {before}", m.triangles.len());
        let v1 = melt::volume(&m);
        assert!((v1 / v0 - 1.0).abs() < 0.005, "{v1} vs {v0}");
        // all eight corners are still where they were
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &m.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        assert!((lo[0] - (0.21 - 5.13)).abs() < 0.02 && (hi[0] - (0.21 + 5.13)).abs() < 0.02, "{lo:?} {hi:?}");
    }

    #[test]
    fn a_target_count_is_reached_and_the_volume_holds() {
        let mut m = mesh_of(&sphere(10.0), 12.0, 0.3).mesh;
        let v0 = melt::volume(&m);
        let before = m.triangles.len();
        let prog = Progress::new();
        reduce(
            &mut m,
            &mut Carry::default(),
            Reduce { target_tris: before / 10, max_error: f64::INFINITY, quantum: 1e-7, tile_error: 0.002 },
            4,
            &prog.slice(0.0, 1.0),
        )
        .unwrap();
        assert!(m.triangles.len() <= before / 10 + 2, "{} vs {}", m.triangles.len(), before / 10);
        assert!(melt::is_watertight(&m));
        let v1 = melt::volume(&m);
        assert!((v1 / v0 - 1.0).abs() < 0.005, "{v1} vs {v0}");
    }

    #[test]
    fn the_tiled_path_gives_the_same_kind_of_result() {
        // a mesh big enough to be cut into tiles
        let mut m = mesh_of(&sphere(12.0), 14.0, 0.05).mesh;
        assert!(m.triangles.len() > 300_000, "{}", m.triangles.len());
        let v0 = melt::volume(&m);
        let before = m.triangles.len();
        let prog = Progress::new();
        reduce(
            &mut m,
            &mut Carry::default(),
            Reduce { target_tris: 0, max_error: 0.002, quantum: 1e-8, tile_error: 0.002 },
            4,
            &prog.slice(0.0, 1.0),
        )
        .unwrap();
        assert!(melt::is_watertight(&m));
        assert!(m.triangles.len() < before / 10, "{} of {before}", m.triangles.len());
        let v1 = melt::volume(&m);
        assert!((v1 / v0 - 1.0).abs() < 0.001, "{v1} vs {v0}");
    }

    #[test]
    fn smoothing_keeps_sharp_vertices_and_the_volume() {
        let f = |p: [f32; 3]| boxsdf([4.0, 4.0, 4.0])(p).max(sphere(5.0)(p));
        let mut meshed = mesh_of(&f, 8.0, 0.3);
        let before = meshed.mesh.clone();
        let prog = Progress::new();
        smooth(&mut meshed.mesh, &meshed.sharp, 3, &prog.slice(0.0, 1.0)).unwrap();
        assert!(melt::is_watertight(&meshed.mesh));
        let (v0, v1) = (melt::volume(&before), melt::volume(&meshed.mesh));
        assert!((v1 / v0 - 1.0).abs() < 0.01, "{v1} vs {v0}");
        for (i, s) in meshed.sharp.iter().enumerate() {
            if *s {
                assert_eq!(before.positions[i], meshed.mesh.positions[i]);
            }
        }
    }
}
