//! Mesh bodies: triangles without a B-rep (imported 3MF/STL). They render, measure, move and
//! export; B-rep features refuse them.

use std::collections::HashMap;
use std::sync::Arc;

use solidcraft_geom::{Mesh, Vec3};

use crate::body::Body;
use crate::{KernelError, Result};

/// Most triangles in one mesh body.
pub const MAX_TRIANGLES: usize = 20_000_000;

/// Indexed triangles with shared vertices, wound counter-clockwise seen from outside.
#[derive(Clone, Debug, Default)]
pub struct TriMesh {
    pub positions: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

impl TriMesh {
    /// Display/measure mesh: corners split per triangle so normals stay flat.
    pub(crate) fn to_mesh(&self) -> Mesh {
        let mut out = Mesh::default();
        out.positions.reserve(self.triangles.len() * 3);
        for t in &self.triangles {
            let (Some(a), Some(b), Some(c)) =
                (self.positions.get(t[0] as usize), self.positions.get(t[1] as usize), self.positions.get(t[2] as usize))
            else {
                continue;
            };
            let n = (*b - *a).cross(*c - *a).normalized().unwrap_or(Vec3::Z);
            let base = u32::try_from(out.positions.len()).unwrap_or(u32::MAX);
            out.positions.extend([*a, *b, *c]);
            out.normals.extend([n, n, n]);
            out.triangles.push([base, base.saturating_add(1), base.saturating_add(2)]);
            out.tri_face.push(0);
        }
        out
    }

    /// Distinct undirected triangle sides.
    /// The diagonal of the box around the vertices: a mesh body's size ([`Body::size`]),
    /// measured once when the body is made. 0 when it has none.
    pub(crate) fn span(&self) -> f64 {
        let mut b = solidcraft_geom::Aabb3::EMPTY;
        for p in &self.positions {
            b.add(*p);
        }
        let d = b.diagonal();
        if d.is_finite() && d > 0.0 { d } else { 0.0 }
    }

    pub(crate) fn edge_count(&self) -> usize {
        let mut set = std::collections::HashSet::new();
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                set.insert((a.min(b), a.max(b)));
            }
        }
        set.len()
    }

    /// Signed volume ×6.
    fn volume6(&self) -> f64 {
        self.triangles
            .iter()
            .filter_map(|t| Some((self.positions.get(t[0] as usize)?, self.positions.get(t[1] as usize)?, self.positions.get(t[2] as usize)?)))
            .map(|(a, b, c)| a.dot(b.cross(*c)))
            .sum()
    }
}

/// A mesh body from triangles (`positions` in mm). Coincident vertices are merged, degenerate
/// triangles dropped, and a closed mesh with inward winding is turned outward.
pub fn mesh_body(positions: &[Vec3], triangles: &[[u32; 3]]) -> Result<Body> {
    if triangles.len() > MAX_TRIANGLES {
        return Err(KernelError::Invalid(format!("mesh with {} triangles (limit {MAX_TRIANGLES})", triangles.len())));
    }
    if positions.iter().any(|p| !(p.is_finite() && p.x.abs() < 1e9 && p.y.abs() < 1e9 && p.z.abs() < 1e9)) {
        return Err(KernelError::Invalid("mesh with non-finite or huge coordinates".into()));
    }
    // Merge vertices that are exactly equal (bit pattern of the coordinates).
    let mut map: HashMap<[u64; 3], u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(positions.len());
    let mut out = TriMesh::default();
    for p in positions {
        let key = [(p.x + 0.0).to_bits(), (p.y + 0.0).to_bits(), (p.z + 0.0).to_bits()];
        let next = u32::try_from(out.positions.len()).map_err(|_| KernelError::Invalid("too many vertices".into()))?;
        let i = *map.entry(key).or_insert_with(|| {
            out.positions.push(*p);
            next
        });
        remap.push(i);
    }
    for t in triangles {
        let (Some(&a), Some(&b), Some(&c)) = (remap.get(t[0] as usize), remap.get(t[1] as usize), remap.get(t[2] as usize)) else {
            return Err(KernelError::Invalid("mesh triangle refers to a missing vertex".into()));
        };
        if a == b || b == c || c == a {
            continue;
        }
        // Corners in a line have no area and no normal: drop them too (#253).
        let (Some(pa), Some(pb), Some(pc)) = (out.positions.get(a as usize), out.positions.get(b as usize), out.positions.get(c as usize)) else {
            return Err(KernelError::Invalid("mesh triangle refers to a missing vertex".into()));
        };
        let normal = (*pb - *pa).cross(*pc - *pa);
        let length = normal.len();
        if !(length > 0.0 && length.is_finite()) {
            continue;
        }
        out.triangles.push([a, b, c]);
    }
    if out.triangles.is_empty() {
        return Err(KernelError::Invalid("mesh has no triangles".into()));
    }
    let shells = unify_winding(&mut out);
    orient_shells(&mut out, &shells);
    Ok(Body {
        solid: Arc::new(crate::body::Solid::new_unchecked(Vec::new())),
        size: out.span(),
        boundary_tol: None,
        volume: Some(std::sync::Arc::new(std::sync::Mutex::new(None))),
        coarse_missing: None,
        mesh: Some(Arc::new(out)),
        color: None,
        paint: None,
        splits: None,
        face_sources: None,
        history: None,
        display_of: None,
        meshes: None,
        phased: false,
        regularization_protected: false,
    })
}

/// Wind every connected piece one way: a triangle that runs a side in the same direction as
/// its neighbour is turned over (files with stray inverted facets). Sides shared by more than
/// two triangles are left alone. Returns the pieces (edge-connected triangles).
fn unify_winding(m: &mut TriMesh) -> Vec<Vec<usize>> {
    let mut sides: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, t) in m.triangles.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            sides.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let runs = |t: &[u32; 3], a: u32, b: u32| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].contains(&(a, b));
    let mut done = vec![false; m.triangles.len()];
    let mut shells = Vec::new();
    for start in 0..m.triangles.len() {
        if done[start] {
            continue;
        }
        done[start] = true;
        let mut shell = Vec::new();
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            shell.push(i);
            let Some(t) = m.triangles.get(i).copied() else { continue };
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let Some(list) = sides.get(&(a.min(b), a.max(b))) else { continue };
                if list.len() != 2 {
                    continue;
                }
                for &j in list {
                    if j == i || done[j] {
                        continue;
                    }
                    done[j] = true;
                    if let Some(u) = m.triangles.get_mut(j)
                        && runs(u, a, b)
                    {
                        u.swap(1, 2);
                    }
                    stack.push(j);
                }
            }
        }
        shells.push(shell);
    }
    shells
}

/// Most triangle-ray tests the shell nesting check may take before it leaves the rest of the
/// mesh as [`unify_winding`] wound it.
const NESTING_BUDGET: usize = 4_000_000;

/// What orienting a shell needs to know about it.
struct ShellInfo {
    /// Signed volume ×6 (positive when wound outward).
    volume6: f64,
    /// Every side is shared by exactly two of its triangles, running opposite ways.
    closed: bool,
    bounds: solidcraft_geom::Aabb3,
}

fn shell_info(m: &TriMesh, tris: &[usize]) -> ShellInfo {
    let mut volume6 = 0.0;
    let mut bounds = solidcraft_geom::Aabb3::EMPTY;
    let mut sides: HashMap<(u32, u32), i32> = HashMap::new();
    for &i in tris {
        let Some(t) = m.triangles.get(i) else { continue };
        let (Some(a), Some(b), Some(c)) = (m.positions.get(t[0] as usize), m.positions.get(t[1] as usize), m.positions.get(t[2] as usize)) else {
            continue;
        };
        volume6 += a.dot(b.cross(*c));
        bounds.add(*a);
        bounds.add(*b);
        bounds.add(*c);
        for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *sides.entry((u, v)).or_default() += 1;
        }
    }
    let closed = sides.iter().all(|(&(a, b), &n)| n == 1 && sides.get(&(b, a)) == Some(&1));
    ShellInfo { volume6, closed, bounds }
}

/// Orient each closed shell by its nesting: outer shells outward and shells held by an odd
/// number of other shells (cavities) inward (#252). A shell whose nesting cannot be told
/// (open, flat, or the budget spent) is left as [`unify_winding`] wound it.
fn orient_shells(m: &mut TriMesh, shells: &[Vec<usize>]) {
    if shells.len() == 1 {
        // The one shell has no other to be inside: outward, as a whole mesh always was.
        if m.volume6() < 0.0 && is_closed(m) {
            turn(&mut m.triangles);
        }
        return;
    }
    let info: Vec<ShellInfo> = shells.iter().map(|t| shell_info(m, t)).collect();
    let mut meshes: Vec<Option<Mesh>> = vec![None; shells.len()];
    let mut budget = NESTING_BUDGET;
    let mut flip = vec![false; shells.len()];
    for i in 0..shells.len() {
        if info[i].closed
            && let Some(odd) = inside_odd_count(m, i, shells, &info, &mut meshes, &mut budget)
        {
            flip[i] = (info[i].volume6 < 0.0) != odd;
        }
    }
    for (i, &f) in flip.iter().enumerate() {
        if f {
            for &t in &shells[i] {
                if let Some(tri) = m.triangles.get_mut(t) {
                    tri.swap(1, 2);
                }
            }
        }
    }
}

fn turn(triangles: &mut [[u32; 3]]) {
    for t in triangles {
        t.swap(1, 2);
    }
}

/// The middle of a triangle of the mesh.
fn shell_triangle_centroid(m: &TriMesh, tri: usize) -> Option<Vec3> {
    let t = m.triangles.get(tri)?;
    let a = m.positions.get(t[0] as usize)?;
    let b = m.positions.get(t[1] as usize)?;
    let c = m.positions.get(t[2] as usize)?;
    Some((*a + *b + *c) / 3.0)
}

/// One shell's triangles as a mesh with shared positions, for point-in-shell tests.
fn shell_mesh(m: &TriMesh, tris: &[usize]) -> Option<Mesh> {
    let mut out = Mesh::default();
    let mut remap: HashMap<u32, u32> = HashMap::new();
    for &i in tris {
        let t = m.triangles.get(i)?;
        let mut local = [0u32; 3];
        for (slot, &v) in local.iter_mut().zip(t) {
            let p = m.positions.get(v as usize)?;
            let idx = match remap.entry(v) {
                std::collections::hash_map::Entry::Occupied(e) => *e.get(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let idx = u32::try_from(out.positions.len()).ok()?;
                    out.positions.push(*p);
                    e.insert(idx);
                    idx
                }
            };
            *slot = idx;
        }
        out.triangles.push(local);
    }
    Some(out)
}

/// Is shell `i` held by an odd number of the other closed shells? `None` when that cannot be
/// told: it has no volume, a sample point or the budget runs out, or its samples disagree
/// (surfaces that touch or cross). A point of the shell's surface lies inside exactly the other
/// shells that enclose it; up to three spread over the shell are checked.
fn inside_odd_count(
    m: &TriMesh,
    i: usize,
    shells: &[Vec<usize>],
    info: &[ShellInfo],
    meshes: &mut [Option<Mesh>],
    budget: &mut usize,
) -> Option<bool> {
    let own = info.get(i)?;
    if !(own.volume6.is_finite() && own.volume6 != 0.0) {
        return None;
    }
    let tris = shells.get(i)?;
    let mut samples = vec![0usize];
    if tris.len() > 2 {
        samples.push(tris.len() / 2);
        samples.push(tris.len() - 1);
    } else if tris.len() > 1 {
        samples.push(tris.len() - 1);
    }
    let mut odd = None;
    for &k in &samples {
        let centroid = shell_triangle_centroid(m, *tris.get(k)?)?;
        let mut holders = 0usize;
        for j in 0..shells.len() {
            if j == i {
                continue;
            }
            let other = info.get(j)?;
            if !other.closed || !(other.volume6.is_finite() && other.volume6 != 0.0) || !other.bounds.contains(centroid) {
                continue;
            }
            let cost = shells.get(j)?.len().saturating_mul(3);
            if cost > *budget {
                return None;
            }
            *budget -= cost;
            let slot = meshes.get_mut(j)?;
            if slot.is_none() {
                *slot = Some(shell_mesh(m, shells.get(j)?)?);
            }
            if slot.as_ref()?.contains(centroid) {
                holders += 1;
            }
        }
        let here = holders % 2 == 1;
        match odd {
            None => odd = Some(here),
            Some(seen) if seen == here => {}
            Some(_) => return None,
        }
    }
    odd
}

/// Every triangle side is shared by exactly two triangles in opposite directions.
pub(crate) fn is_closed(m: &TriMesh) -> bool {
    let mut count: HashMap<(u32, u32), i32> = HashMap::new();
    for t in &m.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *count.entry((a, b)).or_default() += 1;
        }
    }
    count.iter().all(|(&(a, b), &n)| n == 1 && count.get(&(b, a)) == Some(&1))
}

impl Body {
    /// Shared-vertex triangles of a mesh body.
    pub fn triangle_mesh(&self) -> Option<(&[Vec3], &[[u32; 3]])> {
        self.mesh.as_ref().map(|m| (m.positions.as_slice(), m.triangles.as_slice()))
    }
    /// Does a mesh body enclose a volume (every side shared by two triangles)?
    pub fn is_closed_mesh(&self) -> bool {
        self.mesh.as_ref().is_some_and(|m| is_closed(m))
    }

    /// A mesh body moved by a column-major affine matrix (reflections keep outward winding).
    pub(crate) fn mesh_transformed(&self, m: &[[f64; 4]; 4]) -> Option<Body> {
        let tm = self.mesh.as_ref()?;
        let f = |p: Vec3| {
            Vec3::new(
                m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z + m[3][0],
                m[0][1] * p.x + m[1][1] * p.y + m[2][1] * p.z + m[3][1],
                m[0][2] * p.x + m[1][2] * p.y + m[2][2] * p.z + m[3][2],
            )
        };
        let det = m[0][0] * (m[1][1] * m[2][2] - m[2][1] * m[1][2]) - m[1][0] * (m[0][1] * m[2][2] - m[2][1] * m[0][2])
            + m[2][0] * (m[0][1] * m[1][2] - m[1][1] * m[0][2]);
        let mut out = TriMesh { positions: tm.positions.iter().map(|p| f(*p)).collect(), triangles: tm.triangles.clone() };
        if det < 0.0 {
            for t in &mut out.triangles {
                t.swap(1, 2);
            }
        }
        Some(Body {
            solid: self.solid.clone(),
            size: out.span(),
            boundary_tol: None,
            volume: Some(std::sync::Arc::new(std::sync::Mutex::new(None))),
            coarse_missing: None,
            mesh: Some(Arc::new(out)),
            color: self.color,
            paint: self.paint.clone(),
            splits: self.splits.clone(),
            face_sources: None,
            history: None,
            display_of: None,
            meshes: None,
            phased: false,
            regularization_protected: self.regularization_protected,
        })
    }
}
