//! A sparse, multi-threaded dual-contouring mesher for export.
//!
//! The model is a signed distance (`Sdf`, negative inside). The space is cut into blocks of 8x8x8
//! cells of the size the person asked for (the *tolerance*). An octree over the blocks skips every
//! block the distance proves empty (the distance is a lower bound of the way to the surface, so a
//! block whose centre is farther than its own radius cannot hold any surface). Only blocks near the
//! surface are sampled, in parallel. Each cell that the surface crosses gets one vertex per connected
//! patch of surface in it, placed by minimising a quadratic error over the surface normals at the
//! edge crossings (so sharp edges and corners stay sharp), and every crossed grid edge becomes a
//! quad of the four cells around it. Cells with a patch each (the "manifold dual contouring" rule:
//! corners inside are kept apart on ambiguous faces) make the result closed and manifold.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use solidcraft_geom::melt::TriMesh;

/// Cells along a block's side.
const B: usize = 8;
/// Most blocks near the surface (a finer model is refused rather than eating the memory).
pub const MAX_BLOCKS: usize = 1_400_000;
/// Most vertices of one mesh.
pub const MAX_VERTS: usize = 16_000_000;
/// Most cells along one axis.
const MAX_AXIS_CELLS: f64 = 1_000_000.0;

/// A signed distance: negative inside, a lower bound of the distance to the surface.
pub trait Sdf: Sync {
    /// Exact field value for mesh vertices, Hermite root finding and normals.
    fn dist(&self, p: [f32; 3]) -> f32;

    /// A safe estimate for deciding whether a whole octree cell can be skipped.
    /// Most SDFs are 1-Lipschitz and can use the exact value. Inverse-mapped
    /// deformations preserve the zero surface but may have Lipschitz > 1,
    /// so the caller may return a smaller magnitude to avoid pruning a surface.
    /// `radius` is the world-space radius of the enclosing octree cell.
    fn pruning_distance(&self, p: [f32; 3], _radius: f32) -> f32 {
        self.dist(p)
    }
}

impl<F> Sdf for F
where
    F: Fn([f32; 3]) -> f32 + Sync,
{
    fn dist(&self, p: [f32; 3]) -> f32 {
        self(p)
    }
}

// ---------------------------------------------------------------- progress and cancel

/// Shared between the worker and the window: how far along, and a flag to stop.
#[derive(Default)]
pub struct Progress {
    cancel: AtomicBool,
    permille: AtomicU32,
    phase: AtomicU32,
}

impl Progress {
    pub fn new() -> Progress {
        Progress::default()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    /// 0..=1000.
    pub fn permille(&self) -> u32 {
        self.permille.load(Ordering::Relaxed)
    }
    pub fn phase(&self) -> u32 {
        self.phase.load(Ordering::Relaxed)
    }
    pub fn set_phase(&self, phase: u32) {
        self.phase.store(phase, Ordering::Relaxed);
    }
    pub fn set(&self, fraction: f64) {
        let v = if fraction.is_finite() { (fraction.clamp(0.0, 1.0) * 1000.0) as u32 } else { 0 };
        self.permille.store(v, Ordering::Relaxed);
    }
    /// A window onto a part of the bar: `set(f)` moves it to `lo + f * (hi - lo)`.
    pub fn slice(&self, lo: f64, hi: f64) -> Slice<'_> {
        Slice { p: self, lo, hi }
    }
}

#[derive(Clone, Copy)]
pub struct Slice<'a> {
    p: &'a Progress,
    lo: f64,
    hi: f64,
}

impl Slice<'_> {
    pub fn set(&self, fraction: f64) {
        self.p.set(self.lo + (self.hi - self.lo) * fraction.clamp(0.0, 1.0));
    }
    pub fn cancelled(&self) -> bool {
        self.p.cancelled()
    }
    pub fn phase(&self, phase: u32) {
        self.p.set_phase(phase);
    }
    pub fn sub(&self, lo: f64, hi: f64) -> Slice<'_> {
        Slice { p: self.p, lo: self.lo + (self.hi - self.lo) * lo, hi: self.lo + (self.hi - self.lo) * hi }
    }
}

pub const CANCELLED: &str = "cancelled";

// ---------------------------------------------------------------- small maths

type V = [f64; 3];

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Eigen decomposition of a symmetric 3x3 matrix by Jacobi rotations: (values, vectors as columns).
fn jacobi(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..12 {
        let off = a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2];
        if off < 1e-30 {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for row in &mut a {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (before_q, from_q) = a.split_at_mut(q);
            for (apk, aqk) in before_q[p].iter_mut().zip(&mut from_q[0]) {
                let (old_p, old_q) = (*apk, *aqk);
                *apk = c * old_p - s * old_q;
                *aqk = s * old_p + c * old_q;
            }
            for row in v.iter_mut() {
                let (vkp, vkq) = (row[p], row[q]);
                row[p] = c * vkp - s * vkq;
                row[q] = s * vkp + c * vkq;
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

/// The point that best fits the planes through `points` with `normals` (least squares on the
/// distances to the planes), pulled towards their mean where the planes do not decide it, and kept
/// inside the box `lo..hi`. Also says whether two or more independent directions decided it (a
/// sharp edge or corner).
pub fn qef(points: &[V], normals: &[V], lo: V, hi: V) -> (V, bool) {
    let n = points.len().min(normals.len());
    if n == 0 {
        return (mid(lo, hi), false);
    }
    let mut c = [0.0; 3];
    for p in points.iter().take(n) {
        for a in 0..3 {
            c[a] += p[a];
        }
    }
    for x in &mut c {
        *x /= n as f64;
    }
    let mut ata = [[0.0; 3]; 3];
    let mut atb = [0.0; 3];
    let mut used = 0;
    for (p, nm) in points.iter().zip(normals.iter()).take(n) {
        if dot(*nm, *nm) < 0.25 {
            continue;
        }
        used += 1;
        let b = dot(*nm, [p[0] - c[0], p[1] - c[1], p[2] - c[2]]);
        for r in 0..3 {
            for k in 0..3 {
                ata[r][k] += nm[r] * nm[k];
            }
            atb[r] += nm[r] * b;
        }
    }
    if used == 0 {
        return (clamp3(c, lo, hi), false);
    }
    let (vals, vecs) = jacobi(ata);
    let vmax = vals.iter().copied().fold(0.0_f64, f64::max);
    let cutoff = 0.05 * vmax;
    let mut y = [0.0; 3];
    let mut rank = 0;
    for i in 0..3 {
        if vals[i] > cutoff && vals[i] > 1e-12 {
            rank += 1;
            let col = [vecs[0][i], vecs[1][i], vecs[2][i]];
            let w = dot(col, atb) / vals[i];
            for a in 0..3 {
                y[a] += w * col[a];
            }
        }
    }
    (clamp3([c[0] + y[0], c[1] + y[1], c[2] + y[2]], lo, hi), rank >= 2)
}

fn mid(a: V, b: V) -> V {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

fn clamp3(p: V, lo: V, hi: V) -> V {
    let f = |x: f64| if x.is_finite() { x } else { 0.0 };
    [f(p[0]).clamp(lo[0], hi[0]), f(p[1]).clamp(lo[1], hi[1]), f(p[2]).clamp(lo[2], hi[2])]
}

// ---------------------------------------------------------------- the cell table

/// For each of the 256 corner sign patterns of a cell: how many separate patches of surface it
/// holds and which patch each of its 12 edges belongs to (255: the edge is not crossed).
///
/// Corner `c` sits at x = bit 0, y = bit 1, z = bit 2. Edge `e = axis * 4 + u + 2 * v` starts at the
/// corner with the other two axes (b = axis + 1, c = axis + 2, mod 3) at `u` and `v`.
struct CellTable {
    ncomp: [u8; 256],
    edge: [[u8; 12]; 256],
}

fn edge_corners(e: usize) -> (usize, usize) {
    let a = e / 4;
    let (u, v) = ((e % 4) & 1, (e % 4) >> 1);
    let mut o = [0usize; 3];
    o[(a + 1) % 3] = u;
    o[(a + 2) % 3] = v;
    let c0 = o[0] + 2 * o[1] + 4 * o[2];
    (c0, c0 | (1 << a))
}

fn find(parent: &mut [usize; 12], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn build_table() -> CellTable {
    let mut t = CellTable { ncomp: [0; 256], edge: [[255; 12]; 256] };
    for mask in 0..256usize {
        let inside = |c: usize| (mask >> c) & 1 == 1;
        let mut crossed = [false; 12];
        for (e, flag) in crossed.iter_mut().enumerate() {
            let (c0, c1) = edge_corners(e);
            *flag = inside(c0) != inside(c1);
        }
        let mut parent = [0usize; 12];
        for (i, p) in parent.iter_mut().enumerate() {
            *p = i;
        }
        for f in 0..3 {
            for s in 0..2 {
                // the face's four edges: other axes' edges whose start corner has coordinate f == s
                let edges: Vec<usize> = (0..12).filter(|e| e / 4 != f && ((edge_corners(*e).0 >> f) & 1) == s).collect();
                let cr: Vec<usize> = edges.iter().copied().filter(|e| crossed[*e]).collect();
                if cr.len() == 2 {
                    let (x, y) = (find(&mut parent, cr[0]), find(&mut parent, cr[1]));
                    parent[x] = y;
                } else if cr.len() == 4 {
                    // ambiguous face: keep the inside corners apart (one patch around each)
                    for c in 0..8usize {
                        if (c >> f) & 1 != s || !inside(c) {
                            continue;
                        }
                        let own: Vec<usize> = edges.iter().copied().filter(|e| edge_corners(*e).0 == c || edge_corners(*e).1 == c).collect();
                        if own.len() == 2 {
                            let (x, y) = (find(&mut parent, own[0]), find(&mut parent, own[1]));
                            parent[x] = y;
                        }
                    }
                }
            }
        }
        let mut ids: Vec<usize> = Vec::new();
        for (e, crossed) in crossed.iter().enumerate() {
            if !crossed {
                continue;
            }
            let r = find(&mut parent, e);
            let k = match ids.iter().position(|x| *x == r) {
                Some(k) => k,
                None => {
                    ids.push(r);
                    ids.len() - 1
                }
            };
            t.edge[mask][e] = k as u8;
        }
        t.ncomp[mask] = ids.len() as u8;
    }
    t
}

fn table() -> &'static CellTable {
    static T: OnceLock<CellTable> = OnceLock::new();
    T.get_or_init(build_table)
}

// ---------------------------------------------------------------- the grid

#[derive(Clone, Copy)]
struct Grid {
    origin: V,
    h: f64,
    /// Cells per axis.
    n: [i64; 3],
}

impl Grid {
    fn pos(&self, g: [i64; 3]) -> V {
        [self.origin[0] + g[0] as f64 * self.h, self.origin[1] + g[1] as f64 * self.h, self.origin[2] + g[2] as f64 * self.h]
    }
    fn pos32(&self, g: [i64; 3]) -> [f32; 3] {
        let p = self.pos(g);
        [p[0] as f32, p[1] as f32, p[2] as f32]
    }
    fn on_border(&self, g: [i64; 3]) -> bool {
        (0..3).any(|a| g[a] <= 0 || g[a] >= self.n[a])
    }
}

fn clean(d: f32, h: f64) -> f32 {
    if d.is_finite() { d } else { h as f32 }
}

#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }
    fn write_u64(&mut self, v: u64) {
        let x = (v ^ (v >> 29)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = x ^ (x >> 32);
    }
}

pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;

fn key_of(b: [i64; 3]) -> u64 {
    ((b[2] as u64) << 42) | ((b[1] as u64) << 21) | (b[0] as u64)
}

/// What one block found.
struct BlockOut {
    key: u64,
    /// Which cells hold surface (bit `lc`, `lc = (k * 8 + j) * 8 + i`).
    occ: [u64; 8],
    /// Corner sign pattern of each active cell, in `lc` order.
    mask: Vec<u8>,
    /// Index (within this block's vertices) of each active cell's first vertex.
    first: Vec<u32>,
    verts: Vec<[f32; 3]>,
    sharp: Vec<bool>,
    /// Crossed edges this block owns: `lc | axis << 9 | (corner below the edge is inside) << 11`.
    edges: Vec<u16>,
    /// Where this block's vertices start in the whole mesh.
    vbase: u32,
}

impl BlockOut {
    fn rank(&self, lc: usize) -> Option<usize> {
        let (w, b) = (lc / 64, lc % 64);
        let word = *self.occ.get(w)?;
        if (word >> b) & 1 == 0 {
            return None;
        }
        let before: usize = self.occ.iter().take(w).map(|x| x.count_ones() as usize).sum();
        Some(before + (word & ((1u64 << b) - 1)).count_ones() as usize)
    }
}

/// What a thread needs while it works on blocks.
struct Scratch {
    vals: Vec<f32>,
    herm: Vec<[f32; 6]>,
    hstate: Vec<u8>,
    evals: u64,
}

impl Scratch {
    fn new() -> Scratch {
        Scratch { vals: vec![f32::NAN; 729], herm: vec![[0.0; 6]; 3 * 729], hstate: vec![0; 3 * 729], evals: 0 }
    }
}

/// The result: the mesh, which vertices lie on sharp features, and some counters.
pub struct Meshed {
    pub mesh: TriMesh,
    pub sharp: Vec<bool>,
    pub stats: Stats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub blocks: usize,
    pub cells: usize,
    pub evals: u64,
    /// Seconds spent finding blocks, sampling them, joining the faces and building the mesh.
    pub secs: [f64; 4],
    /// Every crossed edge found its four cells (the mesh is closed by construction).
    pub complete: bool,
}

/// Is the block's centre farther from the surface than the block is wide? Then it holds none.
fn near(d: f32, side_cells: f64, h: f64) -> bool {
    let r = side_cells * h * 0.866_025_4;
    f64::from(d).abs() <= 1.1 * r + 0.5 * h
}

/// Run `f` on `0..count` over `threads` workers, which take turns grabbing `chunk` items.
fn par_for<T: Send>(
    threads: usize,
    count: usize,
    chunk: usize,
    prog: &Slice,
    init: impl Fn() -> T + Sync,
    f: impl Fn(&mut T, usize) -> Result<(), String> + Sync,
) -> Result<Vec<T>, String> {
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let chunk = chunk.max(1);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..threads.max(1) {
            let job = || -> Result<T, String> {
                let mut state = init();
                loop {
                    if failed.load(Ordering::Relaxed) || prog.cancelled() {
                        return Err(CANCELLED.into());
                    }
                    let start = next.fetch_add(chunk, Ordering::Relaxed);
                    if start >= count {
                        break;
                    }
                    for i in start..(start + chunk).min(count) {
                        if let Err(e) = f(&mut state, i) {
                            failed.store(true, Ordering::Relaxed);
                            return Err(e);
                        }
                    }
                    prog.set(next.load(Ordering::Relaxed).min(count) as f64 / count.max(1) as f64);
                }
                Ok(state)
            };
            match std::thread::Builder::new().spawn_scoped(scope, job) {
                Ok(h) => handles.push(h),
                Err(e) => {
                    failed.store(true, Ordering::Relaxed);
                    return Err(format!("could not start a worker: {e}"));
                }
            }
        }
        let mut out = Vec::new();
        let mut err: Option<String> = None;
        for h in handles {
            match h.join() {
                Ok(Ok(s)) => out.push(s),
                Ok(Err(e)) => {
                    if err.as_deref().is_none_or(|old| old == CANCELLED) {
                        err = Some(e);
                    }
                }
                Err(_) => err = Some("a worker thread failed".into()),
            }
        }
        match err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    })
}

/// Run `f` for every index in `0..count` on `threads` workers; the results come back in index order.
pub fn par_run<T: Send>(threads: usize, count: usize, prog: &Slice, f: impl Fn(usize) -> Result<T, String> + Sync) -> Result<Vec<T>, String> {
    let parts = par_for(threads, count, 1, prog, Vec::<(usize, T)>::new, |st, i| {
        st.push((i, f(i)?));
        Ok(())
    })?;
    let mut all: Vec<(usize, T)> = parts.into_iter().flatten().collect();
    all.sort_unstable_by_key(|x| x.0);
    Ok(all.into_iter().map(|x| x.1).collect())
}

/// Mesh the zero level of `sdf` inside the box `lo..hi` with cells of size `cell`.
pub fn mesh_sparse(sdf: &dyn Sdf, lo: V, hi: V, cell: f64, threads: usize, prog: &Slice) -> Result<Meshed, String> {
    if !lo.iter().chain(hi.iter()).all(|v| v.is_finite()) || !cell.is_finite() || cell <= 0.0 {
        return Err("the model has no usable size".into());
    }
    let h = cell;
    let pad = 2.0 * h;
    let mut n = [0i64; 3];
    for a in 0..3 {
        let cells = ((hi[a] - lo[a]).max(0.0) + 2.0 * pad) / h;
        if !cells.is_finite() || cells > MAX_AXIS_CELLS {
            return Err("the model is too big for this precision; choose a coarser one".into());
        }
        n[a] = (cells.ceil() as i64).max(2);
    }
    let grid = Grid { origin: [lo[0] - pad, lo[1] - pad, lo[2] - pad], h, n };
    let nb = [(n[0] as usize).div_ceil(B), (n[1] as usize).div_ceil(B), (n[2] as usize).div_ceil(B)];
    let threads = threads.clamp(1, 64);

    // 1. the octree over blocks: keep the ones that may hold surface
    prog.phase(0);
    let t0 = std::time::Instant::now();
    let top = nb.iter().copied().max().unwrap_or(1);
    let mut level = 0u32;
    while (1usize << level) < top {
        level += 1;
    }
    let mut frontier: Vec<[u32; 3]> = vec![[0, 0, 0]];
    let mut blocks: Vec<[u32; 3]> = Vec::new();
    let mut evals = 0u64;
    for lvl in (0..=level).rev() {
        let side_blocks = 1usize << lvl;
        let side_cells = (B * side_blocks) as f64;
        let nodes = &frontier;
        let part = prog.sub(f64::from(level - lvl) / f64::from(level + 1), f64::from(level - lvl + 1) / f64::from(level + 1));
        let kept = par_for(
            threads,
            nodes.len(),
            32,
            &part,
            || (Vec::<[u32; 3]>::new(), 0u64),
            |st, i| {
                if let Some(nd) = nodes.get(i) {
                    let c = [
                        (nd[0] as usize * side_blocks * B) as i64 + (side_cells as i64) / 2,
                        (nd[1] as usize * side_blocks * B) as i64 + (side_cells as i64) / 2,
                        (nd[2] as usize * side_blocks * B) as i64 + (side_cells as i64) / 2,
                    ];
                    st.1 += 1;
                    let cell_radius = (side_cells * h * 0.866_025_4) as f32;
                    let d = clean(sdf.pruning_distance(grid.pos32(c), cell_radius), h);
                    if near(d, side_cells, h) {
                        st.0.push(*nd);
                    }
                }
                Ok(())
            },
        )?;
        let mut survivors: Vec<[u32; 3]> = Vec::new();
        for (v, e) in kept {
            survivors.extend(v);
            evals += e;
        }
        survivors.sort_unstable_by_key(|b| (b[2], b[1], b[0]));
        if lvl == 0 {
            blocks = survivors;
        } else {
            let mut next = Vec::with_capacity(survivors.len() * 8);
            for s in &survivors {
                for o in 0..8u32 {
                    let c = [s[0] * 2 + (o & 1), s[1] * 2 + ((o >> 1) & 1), s[2] * 2 + ((o >> 2) & 1)];
                    // a child wholly outside the grid has nothing to show
                    if (c[0] as usize) * (side_blocks / 2) < nb[0]
                        && (c[1] as usize) * (side_blocks / 2) < nb[1]
                        && (c[2] as usize) * (side_blocks / 2) < nb[2]
                    {
                        next.push(c);
                    }
                }
            }
            if next.len() > MAX_BLOCKS * 8 {
                return Err("the model is too big for this precision; choose a coarser one".into());
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
    }
    if blocks.is_empty() {
        return Err("there is nothing to export: the shapes are empty".into());
    }
    // blocks near a surface hold at least about 20 surface cells each: refuse before sampling them all
    if blocks.len() > MAX_BLOCKS || blocks.len().saturating_mul(20) > MAX_VERTS {
        return Err("too fine for this model: choose a coarser precision".into());
    }

    // 2. sample and place the vertices, block by block
    prog.phase(1);
    let t1 = std::time::Instant::now();
    let part = prog.sub(0.0, 1.0);
    let results = par_for(
        threads,
        blocks.len(),
        4,
        &part,
        || (Scratch::new(), Vec::<(usize, BlockOut)>::new()),
        |st, i| {
            let Some(b) = blocks.get(i) else { return Ok(()) };
            let out = process_block(sdf, &grid, *b, &mut st.0);
            st.1.push((i, out));
            Ok(())
        },
    )?;
    let mut slots: Vec<Option<BlockOut>> = Vec::with_capacity(blocks.len());
    slots.resize_with(blocks.len(), || None);
    for (scratch, outs) in results {
        evals += scratch.evals;
        for (i, o) in outs {
            if let Some(s) = slots.get_mut(i) {
                *s = Some(o);
            }
        }
    }
    let mut outs: Vec<BlockOut> = slots.into_iter().flatten().collect();
    outs.sort_unstable_by_key(|o| o.key);
    // vertices of all blocks in one list; blocks look each other up by their key
    let total: usize = outs.iter().map(|o| o.verts.len()).sum();
    if total > MAX_VERTS {
        return Err("too fine for this model: choose a coarser precision".into());
    }
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(total);
    let mut sharp: Vec<bool> = Vec::with_capacity(total);
    let mut cells = 0usize;
    for o in &mut outs {
        o.vbase = positions.len() as u32;
        positions.append(&mut o.verts);
        sharp.append(&mut o.sharp);
        cells += o.first.len();
    }
    let mut map: FastMap<u64, u32> = FastMap::default();
    map.reserve(outs.len());
    for (i, o) in outs.iter().enumerate() {
        map.insert(o.key, i as u32);
    }

    // 3. one quad for every crossed edge, from the four cells around it
    prog.phase(2);
    let t2 = std::time::Instant::now();
    let part = prog.sub(0.0, 1.0);
    let tab = table();
    let positions_ref = &positions;
    let outs_ref = &outs;
    let map_ref = &map;
    let results = par_for(
        threads,
        outs.len(),
        8,
        &part,
        || (Vec::<(usize, Vec<[u32; 3]>)>::new(), 0usize),
        |st, i| {
            let Some(o) = outs_ref.get(i) else { return Ok(()) };
            let mut tris: Vec<[u32; 3]> = Vec::with_capacity(o.edges.len() * 2);
            let bx = (o.key & 0x1F_FFFF) as i64;
            let by = ((o.key >> 21) & 0x1F_FFFF) as i64;
            let bz = ((o.key >> 42) & 0x1F_FFFF) as i64;
            for code in &o.edges {
                let lc = (*code & 511) as usize;
                let axis = ((*code >> 9) & 3) as usize;
                let below_inside = (*code >> 11) & 1 == 1;
                let p = [bx * B as i64 + (lc % 8) as i64, by * B as i64 + ((lc / 8) % 8) as i64, bz * B as i64 + (lc / 64) as i64];
                let (ab, ac) = ((axis + 1) % 3, (axis + 2) % 3);
                let mut q = [0u32; 4];
                let mut ok = true;
                for (k, (db, dc)) in [(0i64, 0i64), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                    let mut cell = p;
                    cell[ab] -= db;
                    cell[ac] -= dc;
                    match vertex_of(outs_ref, map_ref, tab, cell, axis * 4 + db as usize + 2 * dc as usize) {
                        Some(v) => q[k] = v,
                        None => ok = false,
                    }
                }
                if !ok {
                    st.1 += 1;
                    continue;
                }
                if !below_inside {
                    q.swap(1, 3);
                }
                let at = |v: u32| positions_ref.get(v as usize).copied().unwrap_or([0.0; 3]);
                let d02 = dist2(at(q[0]), at(q[2]));
                let d13 = dist2(at(q[1]), at(q[3]));
                if d02 <= d13 {
                    tris.push([q[0], q[1], q[2]]);
                    tris.push([q[0], q[2], q[3]]);
                } else {
                    tris.push([q[0], q[1], q[3]]);
                    tris.push([q[1], q[2], q[3]]);
                }
            }
            st.0.push((i, tris));
            Ok(())
        },
    )?;
    let mut by_block: Vec<Vec<[u32; 3]>> = Vec::with_capacity(outs.len());
    by_block.resize_with(outs.len(), Vec::new);
    let mut skipped = 0usize;
    for (part, miss) in results {
        skipped += miss;
        for (i, t) in part {
            if let Some(s) = by_block.get_mut(i) {
                *s = t;
            }
        }
    }
    let tri_total: usize = by_block.iter().map(Vec::len).sum();
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(tri_total);
    for mut t in by_block {
        triangles.append(&mut t);
    }
    drop(outs);
    let t3 = std::time::Instant::now();
    if triangles.is_empty() {
        return Err("there is nothing to export: the shapes are empty".into());
    }
    let mesh = TriMesh { positions: positions.iter().map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]).collect(), triangles };
    Ok(Meshed {
        mesh,
        sharp,
        stats: Stats {
            blocks: blocks.len(),
            cells,
            evals,
            secs: [(t1 - t0).as_secs_f64(), (t2 - t1).as_secs_f64(), (t3 - t2).as_secs_f64(), t3.elapsed().as_secs_f64()],
            complete: skipped == 0,
        },
    })
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// The vertex of the patch of `cell` that holds its edge `e`, if the cell exists.
fn vertex_of(outs: &[BlockOut], map: &FastMap<u64, u32>, tab: &CellTable, cell: [i64; 3], e: usize) -> Option<u32> {
    if cell.iter().any(|c| *c < 0) {
        return None;
    }
    let b = [cell[0] >> 3, cell[1] >> 3, cell[2] >> 3];
    let slot = *map.get(&key_of(b))? as usize;
    let o = outs.get(slot)?;
    let lc = ((cell[2] & 7) * 64 + (cell[1] & 7) * 8 + (cell[0] & 7)) as usize;
    let rank = o.rank(lc)?;
    let mask = *o.mask.get(rank)? as usize;
    let comp = *tab.edge.get(mask)?.get(e)?;
    if comp == 255 {
        return None;
    }
    Some(o.vbase + *o.first.get(rank)? + u32::from(comp))
}

/// Sample one block and place the vertices of its cells.
fn process_block(sdf: &dyn Sdf, grid: &Grid, b: [u32; 3], sc: &mut Scratch) -> BlockOut {
    let h = grid.h;
    let c0 = [b[0] as i64 * B as i64, b[1] as i64 * B as i64, b[2] as i64 * B as i64];
    let key = key_of([i64::from(b[0]), i64::from(b[1]), i64::from(b[2])]);
    sc.vals.iter_mut().for_each(|v| *v = f32::NAN);
    sc.hstate.iter_mut().for_each(|v| *v = 0);

    // the corner value at local lattice point (i, j, k), measured once
    macro_rules! corner {
        ($i:expr, $j:expr, $k:expr) => {{
            let idx = ($k * 9 + $j) * 9 + $i;
            let cur = sc.vals.get(idx).copied().unwrap_or(1.0);
            if cur.is_nan() {
                let g = [c0[0] + $i as i64, c0[1] + $j as i64, c0[2] + $k as i64];
                let v = if grid.on_border(g) {
                    h as f32
                } else {
                    sc.evals += 1;
                    clean(sdf.dist(grid.pos32(g)), h)
                };
                if let Some(s) = sc.vals.get_mut(idx) {
                    *s = v;
                }
                v
            } else {
                cur
            }
        }};
    }

    // which 2x2x2-cell nodes are near the surface (tested from the 4-cell nodes down)
    let mut near2 = [false; 64];
    for o4 in 0..8usize {
        let (ox, oy, oz) = (o4 & 1, (o4 >> 1) & 1, (o4 >> 2) & 1);
        let d = corner!(ox * 4 + 2, oy * 4 + 2, oz * 4 + 2);
        if !near(d, 4.0, h) {
            continue;
        }
        for o2 in 0..8usize {
            let (px, py, pz) = (o2 & 1, (o2 >> 1) & 1, (o2 >> 2) & 1);
            let (ni, nj, nk) = (ox * 2 + px, oy * 2 + py, oz * 2 + pz);
            let d = corner!(ni * 2 + 1, nj * 2 + 1, nk * 2 + 1);
            if near(d, 2.0, h)
                && let Some(f) = near2.get_mut((nk * 4 + nj) * 4 + ni)
            {
                *f = true;
            }
        }
    }
    // sample all corners of the near nodes and find the cells the surface crosses
    let mut masks = [0u8; 512];
    for (nidx, is_near) in near2.iter().enumerate() {
        if !*is_near {
            continue;
        }
        let (ni, nj, nk) = (nidx % 4, (nidx / 4) % 4, nidx / 16);
        for dk in 0..3 {
            for dj in 0..3 {
                for di in 0..3 {
                    let _ = corner!(ni * 2 + di, nj * 2 + dj, nk * 2 + dk);
                }
            }
        }
        for cidx in 0..8usize {
            let (ci, cj, ck) = (ni * 2 + (cidx & 1), nj * 2 + ((cidx >> 1) & 1), nk * 2 + ((cidx >> 2) & 1));
            if c0[0] + ci as i64 >= grid.n[0] || c0[1] + cj as i64 >= grid.n[1] || c0[2] + ck as i64 >= grid.n[2] {
                continue;
            }
            let mut mask = 0u8;
            for c in 0..8usize {
                let v = sc.vals.get(((ck + (c >> 2)) * 9 + cj + ((c >> 1) & 1)) * 9 + ci + (c & 1)).copied().unwrap_or(1.0);
                if v < 0.0 {
                    mask |= 1 << c;
                }
            }
            if mask != 0
                && mask != 255
                && let Some(m) = masks.get_mut((ck * 8 + cj) * 8 + ci)
            {
                *m = mask;
            }
        }
    }

    let tab = table();
    let mut out =
        BlockOut { key, occ: [0; 8], mask: Vec::new(), first: Vec::new(), verts: Vec::new(), sharp: Vec::new(), edges: Vec::new(), vbase: 0 };
    let e = (0.03 * h).max(1e-4);
    for lc in 0..512usize {
        let mask = masks.get(lc).copied().unwrap_or(0);
        if mask == 0 {
            continue;
        }
        let (ci, cj, ck) = (lc % 8, (lc / 8) % 8, lc / 64);
        if let Some(w) = out.occ.get_mut(lc / 64) {
            *w |= 1u64 << (lc % 64);
        }
        out.mask.push(mask);
        out.first.push(out.verts.len() as u32);
        let ncomp = tab.ncomp.get(mask as usize).copied().unwrap_or(0) as usize;
        let cell_lo = grid.pos([c0[0] + ci as i64, c0[1] + cj as i64, c0[2] + ck as i64]);
        let cell_hi = [cell_lo[0] + h, cell_lo[1] + h, cell_lo[2] + h];
        // at most 12 crossed edges in a cell, so fixed arrays do: no heap allocation per cell
        let mut pts = [[[0.0f64; 3]; 12]; 4];
        let mut nrm = [[[0.0f64; 3]; 12]; 4];
        let mut cnt = [0usize; 4];
        for ed in 0..12usize {
            let comp = tab.edge.get(mask as usize).and_then(|r| r.get(ed)).copied().unwrap_or(255);
            if comp == 255 {
                continue;
            }
            let (c_start, c_end) = edge_corners(ed);
            let a = ed / 4;
            let (si, sj, sk) = (ci + (c_start & 1), cj + ((c_start >> 1) & 1), ck + (c_start >> 2));
            let hidx = a * 729 + (sk * 9 + sj) * 9 + si;
            if sc.hstate.get(hidx).copied().unwrap_or(1) == 0 {
                let fa = sc.vals.get((sk * 9 + sj) * 9 + si).copied().unwrap_or(1.0);
                let (ei, ej, ek) = (si + usize::from(a == 0), sj + usize::from(a == 1), sk + usize::from(a == 2));
                let fb = sc.vals.get((ek * 9 + ej) * 9 + ei).copied().unwrap_or(1.0);
                let _ = c_end;
                let t = {
                    let (fa, fb) = (f64::from(fa), f64::from(fb));
                    if (fa - fb).abs() < 1e-30 { 0.5 } else { (fa / (fa - fb)).clamp(0.0, 1.0) }
                };
                let pa = grid.pos([c0[0] + si as i64, c0[1] + sj as i64, c0[2] + sk as i64]);
                let mut p = pa;
                p[a] += t * h;
                // the surface normal from four samples around the crossing (tetrahedron stencil)
                let offs: [V; 4] = [[1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, 1.0]];
                let mut g = [0.0; 3];
                let mut fsum = 0.0;
                for s in offs {
                    let q = [(p[0] + s[0] * e) as f32, (p[1] + s[1] * e) as f32, (p[2] + s[2] * e) as f32];
                    sc.evals += 1;
                    let f = f64::from(clean(sdf.dist(q), h));
                    fsum += f;
                    for k in 0..3 {
                        g[k] += s[k] * f;
                    }
                }
                let gl2 = dot(g, g);
                let mut rec = [p[0] as f32, p[1] as f32, p[2] as f32, 0.0, 0.0, 0.0];
                if gl2 > 1e-20 {
                    let gl = gl2.sqrt();
                    let nrm_v = [g[0] / gl, g[1] / gl, g[2] / gl];
                    // one Newton step onto the surface: the stencil's mean is the value at the crossing
                    let gradient_len = gl / (4.0 * e);
                    let step = (fsum * 0.25 / gradient_len.max(0.2)).clamp(-h, h);
                    for k in 0..3 {
                        rec[k] = (p[k] - step * nrm_v[k]) as f32;
                        rec[3 + k] = nrm_v[k] as f32;
                    }
                }
                if let Some(s) = sc.herm.get_mut(hidx) {
                    *s = rec;
                }
                if let Some(s) = sc.hstate.get_mut(hidx) {
                    *s = 1;
                }
            }
            let rec = sc.herm.get(hidx).copied().unwrap_or([0.0; 6]);
            let k = comp as usize;
            if let (Some(pl), Some(nl), Some(n)) = (pts.get_mut(k), nrm.get_mut(k), cnt.get_mut(k))
                && let (Some(ps), Some(ns)) = (pl.get_mut(*n), nl.get_mut(*n))
            {
                *ps = [f64::from(rec[0]), f64::from(rec[1]), f64::from(rec[2])];
                *ns = [f64::from(rec[3]), f64::from(rec[4]), f64::from(rec[5])];
                *n += 1;
            }
        }
        for k in 0..ncomp.min(4) {
            let n = cnt.get(k).copied().unwrap_or(0);
            let (p, sharp) =
                qef(pts.get(k).and_then(|v| v.get(..n)).unwrap_or(&[]), nrm.get(k).and_then(|v| v.get(..n)).unwrap_or(&[]), cell_lo, cell_hi);
            out.verts.push([p[0] as f32, p[1] as f32, p[2] as f32]);
            out.sharp.push(sharp);
        }
        // the three edges that start at this cell's lowest corner are owned by this block
        for a in 0..3usize {
            let v0 = sc.vals.get((ck * 9 + cj) * 9 + ci).copied().unwrap_or(1.0);
            let (ei, ej, ek) = (ci + usize::from(a == 0), cj + usize::from(a == 1), ck + usize::from(a == 2));
            let v1 = sc.vals.get((ek * 9 + ej) * 9 + ei).copied().unwrap_or(1.0);
            if (v0 < 0.0) != (v1 < 0.0) {
                out.edges.push(lc as u16 | ((a as u16) << 9) | (u16::from(v0 < 0.0) << 11));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
pub mod tests {
    use super::*;
    use solidcraft_geom::melt;

    pub fn sphere(r: f32) -> impl Fn([f32; 3]) -> f32 + Sync {
        move |p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r
    }

    pub fn boxsdf(half: [f32; 3]) -> impl Fn([f32; 3]) -> f32 + Sync {
        move |p| {
            let q = [p[0].abs() - half[0], p[1].abs() - half[1], p[2].abs() - half[2]];
            let o = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
            (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt() + q[0].max(q[1]).max(q[2]).min(0.0)
        }
    }

    pub fn run(sdf: &dyn Sdf, half: f64, cell: f64, threads: usize) -> Meshed {
        let prog = Progress::new();
        mesh_sparse(sdf, [-half; 3], [half; 3], cell, threads, &prog.slice(0.0, 1.0)).unwrap()
    }

    fn ball(r: f64) -> f64 {
        4.0 / 3.0 * std::f64::consts::PI * r * r * r
    }

    fn euler(m: &TriMesh) -> i64 {
        let mut edges = std::collections::HashSet::new();
        for t in &m.triangles {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                edges.insert((a.min(b), a.max(b)));
            }
        }
        let used: std::collections::HashSet<u32> = m.triangles.iter().flatten().copied().collect();
        used.len() as i64 - edges.len() as i64 + m.triangles.len() as i64
    }

    #[test]
    fn the_cell_table_matches_known_cases() {
        let t = table();
        assert_eq!(t.ncomp[0], 0);
        assert_eq!(t.ncomp[1], 1);
        // two opposite corners inside: two patches
        assert_eq!(t.ncomp[1 | (1 << 6)], 2);
        // an ambiguous face (diagonal corners 0 and 3 inside): kept apart
        assert_eq!(t.ncomp[1 | (1 << 3)], 2);
        // a whole half: one patch with four edges
        assert_eq!(t.ncomp[0b0000_1111], 1);
        assert_eq!(t.edge[0b0000_1111].iter().filter(|c| **c != 255).count(), 4);
        // every pattern: crossed edges carry a patch number below ncomp
        for m in 0..256 {
            for e in 0..12 {
                let c = t.edge[m][e];
                assert!(c == 255 || c < t.ncomp[m]);
            }
        }
    }

    #[test]
    fn a_sphere_is_closed_round_and_the_right_size() {
        for cell in [0.5, 0.25] {
            let m = run(&sphere(10.0), 12.0, cell, 4);
            assert!(melt::is_watertight(&m.mesh), "cell {cell}");
            assert_eq!(euler(&m.mesh), 2);
            let v = melt::volume(&m.mesh);
            assert!((v / ball(10.0) - 1.0).abs() < 0.01, "cell {cell}: volume {v}");
            // every vertex lies within a fraction of a cell from the true surface
            for p in &m.mesh.positions {
                let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                assert!((r - 10.0).abs() < 0.2 * cell, "vertex off by {}", r - 10.0);
            }
        }
    }

    #[test]
    fn a_box_keeps_its_sharp_corners_and_edges() {
        let cell = 0.5;
        // not aligned with the grid, so the corners fall inside cells
        let f = |p: [f32; 3]| boxsdf([5.13, 4.07, 3.31])([p[0] - 0.21, p[1] + 0.33, p[2] - 0.17]);
        let m = run(&f, 8.0, cell, 4);
        assert!(melt::is_watertight(&m.mesh));
        let v = melt::volume(&m.mesh);
        let exact = 2.0 * 5.13 * 2.0 * 4.07 * 2.0 * 3.31;
        assert!((v / exact - 1.0).abs() < 0.002, "volume {v} vs {exact}");
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = [0.21 + sx * 5.13, -0.33 + sy * 4.07, 0.17 + sz * 3.31];
                    let best = m
                        .mesh
                        .positions
                        .iter()
                        .map(|p| ((p[0] - corner[0]).powi(2) + (p[1] - corner[1]).powi(2) + (p[2] - corner[2]).powi(2)).sqrt())
                        .fold(f64::INFINITY, f64::min);
                    assert!(best < 0.5 * cell, "corner {corner:?} is {best} from the nearest vertex");
                }
            }
        }
    }

    #[test]
    fn a_cylinder_and_a_box_with_holes_are_closed_with_the_right_volume() {
        let cyl = |p: [f32; 3]| {
            let d = [(p[0] * p[0] + p[1] * p[1]).sqrt() - 6.0, p[2].abs() - 5.0];
            d[0].max(d[1]).min(0.0) + (d[0].max(0.0).powi(2) + d[1].max(0.0).powi(2)).sqrt()
        };
        let m = run(&cyl, 8.0, 0.25, 4);
        assert!(melt::is_watertight(&m.mesh));
        let v = melt::volume(&m.mesh);
        let exact = std::f64::consts::PI * 36.0 * 10.0;
        assert!((v / exact - 1.0).abs() < 0.01, "cylinder {v} vs {exact}");
        // a block with two round holes through it (sharp rims, three separate hole surfaces)
        let holes = |p: [f32; 3]| {
            let b = boxsdf([8.0, 6.0, 4.0])(p);
            let h1 = 2.0 - ((p[0] - 3.0).powi(2) + p[1].powi(2)).sqrt();
            let h2 = 1.5 - ((p[0] + 3.5).powi(2) + (p[1] - 1.0).powi(2)).sqrt();
            b.max(h1).max(h2)
        };
        let m = run(&holes, 10.0, 0.25, 4);
        assert!(melt::is_watertight(&m.mesh));
        let exact = 16.0 * 12.0 * 8.0 - std::f64::consts::PI * (4.0 + 2.25) * 8.0;
        let v = melt::volume(&m.mesh);
        assert!((v / exact - 1.0).abs() < 0.01, "holes {v} vs {exact}");
        // genus 2: V - E + F = 2 - 2 * 2
        assert_eq!(euler(&m.mesh), -2);
    }

    #[test]
    fn a_shelled_sphere_exports_closed_walls_with_the_cavity_volume() {
        let mut s = crate::model::Shape::new(crate::model::Kind::Sphere, [0.0; 3], crate::model::Material::default());
        s.size[0] = 10.0;
        s.shell = 2.0;
        let prepared = crate::sdf::Prepared::new(&s);
        let m = run(&|p| prepared.distance(p), 12.0, 0.2, 4);
        assert!(melt::is_watertight(&m.mesh));
        assert_eq!(euler(&m.mesh), 4, "the outer ball and the inner cavity are two closed shells");
        let exact = ball(10.0) - ball(8.0);
        let v = melt::volume(&m.mesh);
        assert!((v / exact - 1.0).abs() < 0.02, "shell volume {v} vs {exact}");
    }

    #[test]
    fn a_rounded_box_is_closed_and_smooth_at_the_rim() {
        let rb = |p: [f32; 3]| {
            let r = 2.0;
            boxsdf([6.0 - r, 5.0 - r, 4.0 - r])(p) - r
        };
        let m = run(&rb, 8.0, 0.25, 4);
        assert!(melt::is_watertight(&m.mesh));
        let v = melt::volume(&m.mesh);
        // box 12 x 10 x 8 with edges rounded by r = 2
        let r = 2.0f64;
        let pi = std::f64::consts::PI;
        // the inner box grown by r: faces, quarter cylinders along the edges, and a ball at the corners
        let exact = 8.0 * 6.0 * 4.0 + 2.0 * r * (8.0 * 6.0 + 8.0 * 4.0 + 6.0 * 4.0) + pi * r * r * (8.0 + 6.0 + 4.0) + 4.0 / 3.0 * pi * r.powi(3);
        assert!((v / exact - 1.0).abs() < 0.01, "volume {v} vs {exact}");
    }

    #[test]
    fn a_thin_gap_between_two_slabs_stays_open_and_is_not_filled() {
        // two slabs 0.5 mm apart
        let slabs = |p: [f32; 3]| {
            let a = boxsdf([10.0, 10.0, 2.0])([p[0] + 10.25, p[1], p[2]]);
            let b = boxsdf([10.0, 10.0, 2.0])([p[0] - 10.25, p[1], p[2]]);
            a.min(b)
        };
        let m = run(&slabs, 24.0, 0.1, 4);
        assert!(melt::is_watertight(&m.mesh));
        let v = melt::volume(&m.mesh);
        assert!((v / 3200.0 - 1.0).abs() < 0.005, "volume {v}: the gap was lost");
        // the two solids are separate closed shells (Euler characteristic 2 each)
        assert_eq!(euler(&m.mesh), 4);
    }

    #[test]
    fn the_same_mesh_comes_out_on_every_run_and_thread_count() {
        let f = |p: [f32; 3]| sphere(6.0)(p).min(boxsdf([3.0, 3.0, 9.0])(p));
        let a = run(&f, 10.0, 0.3, 1);
        let b = run(&f, 10.0, 0.3, 4);
        let c = run(&f, 10.0, 0.3, 3);
        assert_eq!(a.mesh, b.mesh);
        assert_eq!(a.mesh, c.mesh);
        assert_eq!(a.sharp, b.sharp);
    }

    #[test]
    fn blocks_far_from_the_surface_are_never_sampled() {
        let m = run(&sphere(10.0), 12.0, 0.1, 4);
        // a uniform grid of the box would need 245^3 = 14.7 million points
        assert!(m.stats.evals < 3_000_000, "{} samples", m.stats.evals);
        assert!(melt::is_watertight(&m.mesh));
    }

    #[test]
    fn cancel_and_hostile_input_are_errors_not_panics() {
        let prog = Progress::new();
        prog.cancel();
        let s = sphere(5.0);
        assert!(mesh_sparse(&s, [-6.0; 3], [6.0; 3], 0.2, 2, &prog.slice(0.0, 1.0)).is_err());
        let prog = Progress::new();
        let sl = prog.slice(0.0, 1.0);
        assert!(mesh_sparse(&s, [f64::NAN; 3], [6.0; 3], 0.2, 2, &sl).is_err());
        assert!(mesh_sparse(&s, [-6.0; 3], [6.0; 3], 0.0, 2, &sl).is_err());
        assert!(mesh_sparse(&s, [-6.0; 3], [6.0; 3], f64::NAN, 2, &sl).is_err());
        assert!(mesh_sparse(&s, [-1e300; 3], [1e300; 3], 0.01, 2, &sl).is_err());
        assert!(mesh_sparse(&s, [-6.0; 3], [6.0; 3], 1e-9, 2, &sl).is_err());
        // an empty field has nothing to mesh
        let empty = |_p: [f32; 3]| 1.0f32;
        assert!(mesh_sparse(&empty, [-6.0; 3], [6.0; 3], 0.5, 2, &sl).is_err());
        let nan = |_p: [f32; 3]| f32::NAN;
        assert!(mesh_sparse(&nan, [-6.0; 3], [6.0; 3], 0.5, 2, &sl).is_err());
    }
}
