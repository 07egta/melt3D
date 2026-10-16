//! Export: sample the CPU distance field on a grid, mesh it, and write a binary STL (mm, Z up).

use crate::model::{BlendMode, Kind, Op, Scene, Shape};
use crate::sdf::Field;
use solidcraft_geom::melt::{self, TriMesh};

/// Cells along the longest side of the model.
const MIN_RES: usize = 24;
const MAX_RES: usize = 220;

/// A radius around the shape's position that surely holds all of it (before blending).
fn reach_of(s: &Shape) -> f64 {
    let sz = [f64::from(s.size[0]), f64::from(s.size[1]), f64::from(s.size[2])];
    let local = match s.kind {
        Kind::Sphere => sz[0],
        Kind::Box => 0.5 * (sz[0] * sz[0] + sz[1] * sz[1] + sz[2] * sz[2]).sqrt(),
        Kind::Cylinder | Kind::Cone => sz[0].hypot(sz[1] * 0.5),
        Kind::Torus => sz[0] + sz[1],
        Kind::Extrude => {
            let wide = s.outline.iter().map(|p| f64::from(p[0]).hypot(f64::from(p[1]))).fold(0.0_f64, f64::max);
            wide.hypot(sz[0] * 0.5)
        }
        _ => s.local_half().iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>().sqrt(),
    };
    let scale = s.scale.iter().map(|v| f64::from(*v)).fold(0.0_f64, f64::max);
    local * scale
}

/// The box around everything that is added to the scene, with room for blending and drips.
fn bounds(scene: &Scene) -> Result<([f64; 3], [f64; 3]), String> {
    let m = scene.melt;
    let factor = match m.mode {
        BlendMode::Smooth => 1.0,
        BlendMode::Round => 3.5,
        BlendMode::Gooey => 6.0,
    };
    let amount = if m.amount.is_finite() { f64::from(m.amount.max(0.0)) } else { 1.0 };
    let drip = if m.drip.is_finite() { f64::from(m.drip.max(0.0)) } else { 0.0 };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for g in scene.groups.iter().filter(|g| g.visible && g.op == Op::Add) {
        for s in g.shapes.iter().filter(|s| s.visible && s.op == Op::Add) {
            let r = reach_of(s) + (f64::from(s.blend.max(0.0)) + f64::from(g.blend.max(0.0))) * amount * factor;
            for a in 0..3 {
                let c = f64::from(s.pos[a]);
                lo[a] = lo[a].min(c - r);
                hi[a] = hi[a].max(c + r);
            }
        }
    }
    lo[2] -= drip;
    if lo.iter().chain(hi.iter()).any(|v| !v.is_finite()) {
        return Err("there is nothing to export: add a shape first".into());
    }
    Ok((lo, hi))
}

/// Mesh the scene. `resolution` is the number of cells along the longest side (24..=220).
pub fn mesh(scene: &Scene, resolution: usize, smooth: usize) -> Result<TriMesh, String> {
    let (lo, hi) = bounds(scene)?;
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
    if !extent.is_finite() || extent <= 1e-9 {
        return Err("the shapes have no size".into());
    }
    let h = extent / resolution.clamp(MIN_RES, MAX_RES) as f64;
    let pad = 3.0 * h;
    let origin = [lo[0] - pad, lo[1] - pad, lo[2] - pad];
    let mut n = [0usize; 3];
    for a in 0..3 {
        let cells = ((hi[a] - lo[a] + 2.0 * pad) / h).ceil();
        if !cells.is_finite() || cells + 1.0 > melt::MAX_GRID_POINTS as f64 {
            return Err("the model is too big for this resolution".into());
        }
        n[a] = cells as usize + 1;
    }
    let points = n[0]
        .checked_mul(n[1])
        .and_then(|p| p.checked_mul(n[2]))
        .filter(|p| *p <= melt::MAX_GRID_POINTS)
        .ok_or_else(|| "the model is too big for this resolution".to_string())?;
    let field = Field::new(scene);
    let mut values = vec![1.0_f32; points];
    let row = n[0];
    let rows = n[1] * n[2];
    let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).clamp(1, 64);
    let per = rows.div_ceil(threads).max(1);
    let result: Result<(), String> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (t, chunk) in values.chunks_mut(per * row).enumerate() {
            let field = &field;
            let job = move || {
                for (r, line) in chunk.chunks_mut(row).enumerate() {
                    let rr = t * per + r;
                    let (j, k) = (rr % n[1], rr / n[1]);
                    for (i, v) in line.iter_mut().enumerate() {
                        let p = [(origin[0] + i as f64 * h) as f32, (origin[1] + j as f64 * h) as f32, (origin[2] + k as f64 * h) as f32];
                        let d = field.distance(p);
                        *v = if d.is_finite() { d } else { 1.0 };
                    }
                }
            };
            handles.push(std::thread::Builder::new().spawn_scoped(scope, job).map_err(|e| e.to_string())?);
        }
        for handle in handles {
            handle.join().map_err(|_| "a worker thread failed".to_string())?;
        }
        Ok(())
    });
    result?;
    melt::polygonize(origin, h, n, &values, smooth)
}

/// A binary STL: 80-byte header, triangle count, then 50 bytes per triangle.
pub fn stl_binary(mesh: &TriMesh, name: &str) -> Vec<u8> {
    let corners: Vec<[[f64; 3]; 3]> = mesh
        .triangles
        .iter()
        .filter_map(|t| {
            let a = mesh.positions.get(t[0] as usize)?;
            let b = mesh.positions.get(t[1] as usize)?;
            let c = mesh.positions.get(t[2] as usize)?;
            Some([*a, *b, *c])
        })
        .take(u32::MAX as usize)
        .collect();
    let mut out = Vec::with_capacity(84 + 50 * corners.len());
    // The header must not start with "solid", or readers take the file for ASCII STL.
    let mut header = format!("YURACAD {name}").into_bytes();
    header.retain(|b| (0x20..0x7f).contains(b));
    header.resize(80, 0);
    out.extend_from_slice(&header);
    out.extend_from_slice(&u32::try_from(corners.len()).unwrap_or(u32::MAX).to_le_bytes());
    for t in &corners {
        let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
        let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
        let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let nrm = if l > 1e-30 { [c[0] / l, c[1] / l, c[2] / l] } else { [0.0, 0.0, 1.0] };
        for x in nrm.iter().chain(t.iter().flatten()) {
            let x = if x.is_finite() { *x as f32 } else { 0.0 };
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

pub fn write_stl(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("could not save {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Material};

    fn ball(r: f64) -> f64 {
        4.0 / 3.0 * std::f64::consts::PI * r * r * r
    }

    fn sphere_at(x: f32) -> Shape {
        Shape::new(Kind::Sphere, [x, 0.0, 20.0], Material::default())
    }

    fn scene_of(shapes: Vec<Shape>) -> Scene {
        let mut g = Group::new("g");
        g.shapes = shapes;
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() }
    }

    #[test]
    fn the_default_sphere_has_the_right_volume_and_is_closed() {
        let m = mesh(&Scene::default(), 80, 2).unwrap();
        assert!(melt::is_watertight(&m));
        let v = melt::volume(&m);
        assert!((v / ball(20.0) - 1.0).abs() < 0.03, "volume {v}");
    }

    #[test]
    fn blended_spheres_hold_less_than_two_and_more_than_one() {
        let one = melt::volume(&mesh(&scene_of(vec![sphere_at(0.0)]), 80, 2).unwrap());
        let two = mesh(&scene_of(vec![sphere_at(-12.5), sphere_at(12.5)]), 80, 2).unwrap();
        let v = melt::volume(&two);
        assert!(melt::is_watertight(&two));
        assert!(v < 2.0 * one && v > one, "{v} vs {one}");
    }

    #[test]
    fn a_subtracted_sphere_takes_material_away() {
        let one = melt::volume(&mesh(&scene_of(vec![sphere_at(0.0)]), 80, 2).unwrap());
        let mut cut = sphere_at(18.0);
        cut.op = Op::Subtract;
        cut.size[0] = 10.0;
        let m = mesh(&scene_of(vec![sphere_at(0.0), cut]), 80, 2).unwrap();
        assert!(melt::volume(&m) < one - 300.0, "{} vs {one}", melt::volume(&m));
    }

    #[test]
    fn stl_has_the_binary_size_and_empty_scenes_are_refused() {
        let m = mesh(&Scene::default(), 24, 0).unwrap();
        let bytes = stl_binary(&m, "test");
        assert_eq!(bytes.len(), 84 + 50 * m.triangles.len());
        assert!(!bytes.starts_with(b"solid"));
        assert!(
            mesh(&Scene { groups: vec![Group::new("empty")], melt: Default::default(), env: Default::default(), saved: Vec::new() }, 40, 0).is_err()
        );
    }
}
