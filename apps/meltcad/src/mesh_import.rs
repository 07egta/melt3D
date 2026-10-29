//! The steps behind the mesh import card: read the file, turn it upright, scale it to the size the
//! person asked for, bake the distance volume, and say where the shape should stand. Everything
//! here runs without a window (the card in `mesh_dialog.rs` calls it from worker threads).
//!
//! The volume is baked around the model's own middle, so the shape's position is the middle of the
//! model and the gizmo turns it about that point; `place` works out the position (on the floor,
//! at the view's centre, or where the file had it).

use std::path::Path;

use crate::import_mesh::{self, ImportOptions, Imported, ImportedMesh, MeshFormat};
use crate::mesh::SharedVolume;
use crate::sdf_volume::{BakeControl, BakeOptions, BakeStats, Volume, bake_with};

/// The choices of the card's 細かさ: cells along the longest side of the model.
pub const RES_CHOICES: [u32; 4] = [64, 96, 128, 160];
pub const DEFAULT_RES: u32 = 128;
/// The default size is the file's own size, but no more than this (mm).
pub const DEFAULT_CAP_MM: f32 = 200.0;
pub const MIN_TARGET_MM: f32 = 1.0;
pub const MAX_TARGET_MM: f32 = 2000.0;
/// The file types the card reads.
pub const EXTENSIONS: [&str; 5] = ["stl", "obj", "3mf", "ply", "glb"];

pub fn is_mesh_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Which way is up in the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Up {
    /// By the file type (OBJ and GLB are Y up, the others Z up).
    Auto,
    Y,
    Z,
}

impl Up {
    pub const ALL: [Up; 3] = [Up::Auto, Up::Y, Up::Z];
    pub fn label(self) -> &'static str {
        match self {
            Up::Auto => "Auto",
            Up::Y => "Y up",
            Up::Z => "Z up",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Up::Auto => "Picks by file type (OBJ and GLB are Y up, others Z up)",
            Up::Y => "Y-up data. Stands it up on import",
            Up::Z => "Z-up data. Imported as is",
        }
    }
    /// Is the file Y up?
    pub fn is_y(self, format: MeshFormat) -> bool {
        match self {
            Up::Auto => format.y_up_by_default(),
            Up::Y => true,
            Up::Z => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The longest side of the model, mm.
    pub target_mm: f32,
    pub up: Up,
    /// The model's bottom goes on the floor.
    pub floor: bool,
    /// The model's middle goes to the middle of the view (else it stays where the file had it).
    pub centre: bool,
    /// Cells along the longest side.
    pub res: u32,
}

impl Settings {
    pub fn new(size: [f32; 3]) -> Settings {
        Settings { target_mm: default_target(size), up: Up::Auto, floor: true, centre: true, res: DEFAULT_RES }
    }
    pub fn sanitized(self) -> Settings {
        Settings {
            target_mm: if self.target_mm.is_finite() { self.target_mm.clamp(MIN_TARGET_MM, MAX_TARGET_MM) } else { 60.0 },
            res: self.res.clamp(16, 256),
            ..self
        }
    }
}

/// The file's own size, but at most `DEFAULT_CAP_MM` (and at least `MIN_TARGET_MM`), to a tenth of a mm.
pub fn default_target(size: [f32; 3]) -> f32 {
    let longest = size.into_iter().fold(0.0f32, f32::max);
    let t = if longest.is_finite() && longest > 0.0 { longest } else { 60.0 };
    ((t.clamp(MIN_TARGET_MM, DEFAULT_CAP_MM)) * 10.0).round() / 10.0
}

/// A file that has been read (not yet turned or scaled).
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The file name with its extension.
    pub file_name: String,
    /// The name without the extension (the shape and group are named after it).
    pub stem: String,
    pub imported: Imported,
    /// The size of the box in the file's own axes and units.
    pub size: [f32; 3],
}

/// Read a mesh file (checked and cleaned; the up axis is not applied yet).
#[cfg(test)]
pub fn load(path: &Path) -> Result<Loaded, String> {
    load_with_control(path, &std::sync::atomic::AtomicBool::new(false), &std::sync::atomic::AtomicU32::new(0))
}

/// The background file reader updates progress and can stop before parsing or after parsing.
pub fn load_with_control(path: &Path, cancel: &std::sync::atomic::AtomicBool, progress: &std::sync::atomic::AtomicU32) -> Result<Loaded, String> {
    let imported = import_mesh::import_file_with_control(path, &ImportOptions { y_up: Some(false) }, cancel, progress)?;
    let size = imported.mesh.size();
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Mesh".into());
    Ok(Loaded { file_name, stem, imported, size })
}

/// The size of the model once it stands upright (a Y-up file swaps its depth and height).
pub fn upright_size(size: [f32; 3], y_up: bool) -> [f32; 3] {
    if y_up { [size[0], size[2], size[1]] } else { size }
}

/// What a bake is expected to cost: grid nodes, seconds on a typical machine, bytes in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub nodes: usize,
    pub secs: f64,
    pub bytes: f64,
}

/// About how big the grid and the file get (the numbers of IMPORT.md: a 128-cell ball is
/// 137^3 nodes, about 2.5 s, 1.2 MB of text). `size` is the model's box, `tris` its triangles.
pub fn estimate(size: [f32; 3], tris: usize, res: u32) -> Estimate {
    let opts = BakeOptions { res, ..BakeOptions::default() };
    let longest = size.into_iter().fold(0.0f32, f32::max).max(1e-6);
    let mut cells = f64::from(res.clamp(4, 512));
    let nodes_at =
        |cells: f64| -> f64 { size.iter().map(|s| (f64::from(s / longest) * cells).ceil().max(1.0) + 2.0 * f64::from(opts.pad) + 1.0).product() };
    // (the bake lowers the cell count itself when the grid would be too big)
    while nodes_at(cells) > opts.max_cells as f64 && cells > 8.0 {
        cells -= 1.0;
    }
    let nodes = nodes_at(cells);
    Estimate { nodes: nodes as usize, secs: nodes * 1.0e-6 + tris as f64 * 2.0e-6, bytes: nodes * 0.5 }
}

/// A baked mesh, ready to become a shape.
#[derive(Clone, Debug)]
pub struct Baked {
    pub volume: SharedVolume,
    /// The model's size in mm after scaling.
    pub size: [f32; 3],
    /// Where the model's middle was in the file (after turning and scaling), mm.
    pub file_centre: [f32; 3],
    pub stats: BakeStats,
}

pub fn mesh_hash(mesh: &ImportedMesh) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in &mesh.positions {
        for v in p {
            for b in v.to_bits().to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    for t in &mesh.triangles {
        for v in t {
            for b in v.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// Re-bake at the exact final size, preserving anisotropic scale as geometry.
pub fn rebake(mesh: &ImportedMesh, y_up: bool, final_size: [f32; 3], res: u32, ctl: &BakeControl) -> Result<Baked, String> {
    let mut m = mesh.clone();
    if y_up {
        m.y_up_to_z_up();
    }
    let (lo, hi) = m.bounds().ok_or_else(|| import_mesh::NO_SHAPE.to_string())?;
    let c = [0, 1, 2].map(|a| 0.5 * (lo[a] + hi[a]));
    let orig = m.size();
    if final_size.iter().any(|x| !x.is_finite() || *x <= 0.0) || orig.iter().any(|x| *x <= 0.0) {
        return Err("Invalid dimensions".into());
    }
    let factors = [0, 1, 2].map(|a| final_size[a] / orig[a]);
    for p in &mut m.positions {
        for a in 0..3 {
            p[a] = (p[a] - c[a]) * factors[a];
        }
    }
    let (volume, stats) = bake_with(&m, &BakeOptions { res, ..BakeOptions::default() }, ctl)?;
    Ok(Baked { volume: SharedVolume::new(volume), size: final_size, file_centre: [0.0; 3], stats })
}

/// Bake original Z-up millimetre geometry without applying the interactive import defaults.
/// The volume is centred around the source bounds and `file_centre` restores its original
/// position when the resulting shape is placed in the scene.
pub fn bake_original(mesh: &ImportedMesh, res: u32, ctl: &BakeControl) -> Result<Baked, String> {
    let (lo, hi) = mesh.bounds().ok_or_else(|| import_mesh::NO_SHAPE.to_string())?;
    let size = [0, 1, 2].map(|a| hi[a] - lo[a]);
    let centre = [0, 1, 2].map(|a| lo[a] + size[a] * 0.5);
    if size.iter().any(|x| !x.is_finite() || !(0.01..=2000.0).contains(x)) || centre.iter().any(|x| !x.is_finite() || !(-5000.0..=5000.0).contains(x))
    {
        return Err("立体が Melt の対応範囲を超えています（各辺 0.01〜2000 mm、中心位置 ±5000 mm）。SolidCraft で寸法・位置を調整してください".into());
    }
    let mut centered = mesh.clone();
    for p in &mut centered.positions {
        for a in 0..3 {
            p[a] -= centre[a];
        }
    }
    let (volume, stats) = bake_with(&centered, &BakeOptions { res: res.clamp(16, 256), ..BakeOptions::default() }, ctl)?;
    Ok(Baked { volume: SharedVolume::new(volume), size, file_centre: centre, stats })
}

/// Surface-centroid SDF residual (mean, maximum, number sampled), not Hausdorff error.
pub fn surface_residual(mesh: &ImportedMesh, y_up: bool, final_size: [f32; 3], volume: &Volume) -> (f32, f32, usize) {
    let mut m = mesh.clone();
    if y_up {
        m.y_up_to_z_up();
    }
    let Some((lo, hi)) = m.bounds() else { return (0.0, 0.0, 0) };
    let sz = m.size();
    if sz.iter().any(|x| *x <= 0.0) {
        return (0.0, 0.0, 0);
    }
    let c = [0, 1, 2].map(|a| (lo[a] + hi[a]) * 0.5);
    let mut total = 0.0f64;
    let mut worst = 0.0f32;
    let mut n = 0usize;
    let stride = (m.triangles.len() / 2048).max(1);
    for tri in m.triangles.iter().step_by(stride) {
        let (Some(a), Some(b), Some(d)) = (m.positions.get(tri[0] as usize), m.positions.get(tri[1] as usize), m.positions.get(tri[2] as usize))
        else {
            continue;
        };
        let p = [0, 1, 2].map(|i| ((a[i] + b[i] + d[i]) / 3.0 - c[i]) * final_size[i] / sz[i]);
        let residual = volume.distance(p).abs();
        if residual.is_finite() {
            total += f64::from(residual);
            worst = worst.max(residual);
            n += 1;
        }
    }
    (if n > 0 { (total / n as f64) as f32 } else { 0.0 }, worst, n)
}

/// Turn, centre, scale and bake. Cancelling through `ctl` gives `Err("Stopped")`.
pub fn bake(mesh: &ImportedMesh, format: MeshFormat, st: &Settings, ctl: &BakeControl) -> Result<Baked, String> {
    let st = st.sanitized();
    let mut m = mesh.clone();
    if st.up.is_y(format) {
        m.y_up_to_z_up();
    }
    let (lo, hi) = m.bounds().ok_or_else(|| import_mesh::NO_SHAPE.to_string())?;
    let c0 = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    for p in &mut m.positions {
        *p = [p[0] - c0[0], p[1] - c0[1], p[2] - c0[2]];
    }
    let longest = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max);
    if !(longest.is_finite() && longest > 0.0) {
        return Err(import_mesh::NO_SHAPE.to_string());
    }
    let k = st.target_mm / longest;
    m.scale(k);
    let size = m.size();
    let (volume, stats): (Volume, BakeStats) = bake_with(&m, &BakeOptions { res: st.res, ..BakeOptions::default() }, ctl)?;
    Ok(Baked { volume: SharedVolume::new(volume), size, file_centre: [c0[0] * k, c0[1] * k, c0[2] * k], stats })
}

/// The shape's position: on the floor (the bottom of the model at z = 0), at the view's middle
/// (`view` = x, y), or where the file had it.
pub fn place(b: &Baked, floor: bool, centre: bool, view: [f32; 2]) -> [f32; 3] {
    let xy = if centre { view } else { [b.file_centre[0], b.file_centre[1]] };
    let z = if floor { b.size[2] * 0.5 } else { b.file_centre[2] };
    [xy[0], xy[1], z]
}

/// The shape of a baked mesh.
pub fn shape_of(b: &Baked, name: &str, source: &str, pos: [f32; 3], material: crate::model::Material) -> crate::model::Shape {
    crate::model::Shape::mesh(b.volume.clone(), name, source, b.size, pos, material)
}

/// Why a mesh with this many grid nodes cannot be added to the scene (None = it can).
pub fn refuse(scene: &crate::model::Scene, nodes: usize) -> Option<&'static str> {
    crate::mesh::Usage::of(scene.groups.iter().flat_map(|g| g.shapes.iter())).check_add(nodes, false).err()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::import_mesh::tests::{box_mesh, sphere_mesh};
    use crate::model::{Group, Kind, Material, Scene, Shape};
    use crate::sdf::Field;

    fn scene_with(shape: Shape) -> Scene {
        let mut g = Group::new("g");
        g.shapes.push(shape);
        Scene { groups: vec![g], ..Scene::default() }
    }

    fn baked_sphere(r: f32, res: u32) -> Baked {
        // a sphere of radius r, made 2r across, baked as is
        let st = Settings { target_mm: 2.0 * r, up: Up::Z, floor: true, centre: true, res };
        bake(&sphere_mesh(r, 40, 80), MeshFormat::Stl, &st, &BakeControl::new()).unwrap()
    }

    #[test]
    fn original_bake_keeps_large_and_submillimetre_geometry_and_offset() {
        for (lo, hi, res) in [([-120.0, 20.0, 7.0], [120.0, 60.0, 31.0], 32), ([4.0, -3.0, 0.25], [4.4, -2.7, 0.45], 16)] {
            let b = bake_original(&box_mesh(lo, hi), res, &BakeControl::new()).unwrap();
            let expected_size = [0, 1, 2].map(|a| hi[a] - lo[a]);
            let expected_centre = [0, 1, 2].map(|a| (lo[a] + hi[a]) * 0.5);
            assert_eq!(b.size, expected_size);
            assert_eq!(b.file_centre, expected_centre);
            let local_inside = [0.0, 0.0, 0.0];
            let local_outside = [expected_size[0] * 0.5 + b.volume.cell * 2.0, 0.0, 0.0];
            assert!(b.volume.distance(local_inside) < 0.0);
            assert!(b.volume.distance(local_outside) > 0.0);
            let pos = place(&b, false, false, [0.0; 2]);
            assert_eq!(pos, expected_centre);
            let shape = shape_of(&b, "original", "source", pos, Material::default());
            let field = Field::new(&scene_with(shape));
            assert!(field.distance(expected_centre) < 0.0);
        }
    }

    #[test]
    fn original_bake_rejects_dimensions_and_positions_outside_scene_bounds() {
        assert!(bake_original(&box_mesh([0.0; 3], [2001.0, 10.0, 10.0]), 16, &BakeControl::new()).is_err());
        assert!(bake_original(&box_mesh([0.0; 3], [0.009, 1.0, 1.0]), 16, &BakeControl::new()).is_err());
        assert!(bake_original(&box_mesh([5001.0, 0.0, 0.0], [5002.0, 1.0, 1.0]), 16, &BakeControl::new()).is_err());
    }

    #[test]
    fn a_sphere_mesh_measures_like_the_sphere_within_a_cell() {
        let b = baked_sphere(10.0, 64);
        assert!(b.size.iter().all(|s| (s - 20.0).abs() < 0.1), "{:?}", b.size);
        let pos = place(&b, true, true, [30.0, -12.0]);
        assert!((pos[0] - 30.0).abs() < 1e-5 && (pos[1] + 12.0).abs() < 1e-5 && (pos[2] - b.size[2] * 0.5).abs() < 1e-5);
        let cell = b.volume.cell;
        assert!(cell > 0.2 && cell < 0.4, "{cell}");
        let shape = shape_of(&b, "ball", "ball.stl", pos, Material::default());
        let field = Field::new(&scene_with(shape));
        let mut worst = 0.0f32;
        let mut inside = 0;
        for i in 0..400 {
            // points on a spiral through the volume's box and beyond
            let t = i as f32 / 400.0;
            let a = t * 40.0;
            let r = 2.0 + 24.0 * t;
            let p = [pos[0] + r * a.cos() * (1.0 - t * 0.3), pos[1] + r * a.sin() * 0.7, pos[2] + (t - 0.5) * 26.0];
            let analytic = ((p[0] - pos[0]).powi(2) + (p[1] - pos[1]).powi(2) + (p[2] - pos[2]).powi(2)).sqrt() - 10.0;
            let got = field.distance(p);
            let local = [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2]];
            let (lo, hi) = b.volume.bounds();
            if (0..3).all(|a| local[a] >= lo[a] && local[a] <= hi[a]) {
                // inside the grid: the distance itself, to within a cell
                worst = worst.max((got - analytic).abs());
                assert!((got - analytic).abs() <= cell * 1.0 + 0.05, "at {p:?}: {got} vs {analytic} (cell {cell})");
                inside += 1;
            } else {
                // outside: the distance to the box plus the edge value, never smaller than the true one by more than a cell
                assert!(got >= analytic - cell, "at {p:?}: {got} vs {analytic}");
            }
        }
        assert!(inside > 100, "{inside} points inside the grid");
        eprintln!("worst error {worst:.3} mm, cell {cell:.3} mm ({inside} points inside)");
    }

    #[test]
    fn scaling_and_the_shell_follow_the_other_kinds() {
        let b = baked_sphere(10.0, 48);
        let pos = [0.0, 0.0, 30.0];
        let mut shape = shape_of(&b, "ball", "x.stl", pos, Material::default());
        let cell = b.volume.cell;
        // scaled by 2 in every axis: radius 20
        shape.scale = [2.0; 3];
        let f = Field::new(&scene_with(shape.clone()));
        assert!((f.distance([20.0, 0.0, 30.0])).abs() < 2.0 * cell + 0.05, "{}", f.distance([20.0, 0.0, 30.0]));
        assert!((f.distance([40.0, 0.0, 30.0]) - 20.0).abs() < 2.0 * cell + 0.05);
        // not the same in every axis: the distance is multiplied by the smallest scale (a lower bound)
        shape.scale = [3.0, 1.0, 2.0];
        let f = Field::new(&scene_with(shape.clone()));
        let d = f.distance([60.0, 0.0, 30.0]);
        assert!(d <= 30.0 + 2.0 * cell && d > 8.0, "a lower bound of the true distance 30 (the smallest scale is 1): {d}");
        // a shell hollows it out
        shape.scale = [1.0; 3];
        shape.shell = 2.0;
        let f = Field::new(&scene_with(shape));
        assert!(f.distance([0.0, 0.0, 30.0]) > 0.0, "the middle is empty");
        assert!(f.distance([9.0, 0.0, 30.0]) < 0.0, "the wall is solid");
    }

    #[test]
    fn a_mesh_melts_into_a_primitive_and_carves_like_one() {
        let b = baked_sphere(10.0, 48);
        let mut mesh = shape_of(&b, "ball", "x.stl", [0.0, 0.0, 12.0], Material::default());
        mesh.blend = 8.0;
        let mut sphere = Shape::new(Kind::Sphere, [14.0, 0.0, 12.0], Material::default());
        sphere.size[0] = 6.0;
        sphere.blend = 8.0;
        let scene = scene_with(mesh.clone());
        let mut both = scene.clone();
        both.groups[0].shapes.push(sphere.clone());
        let (a, b2) = (Field::new(&scene), Field::new(&both));
        // at the neck between them the blend is fatter than the mesh alone
        let neck = [8.5, 0.0, 12.0 + 6.0];
        assert!(b2.distance(neck) < a.distance(neck) - 0.5);
        // the sphere as a carving shape removes part of the mesh
        sphere.op = crate::model::Op::Subtract;
        sphere.blend = 0.0;
        let mut carved = scene.clone();
        carved.groups[0].shapes.push(sphere);
        let c = Field::new(&carved);
        assert!(a.distance([9.0, 0.0, 12.0]) < 0.0 && c.distance([9.0, 0.0, 12.0]) > 0.0);
    }

    #[test]
    fn the_up_axis_choice_stands_a_model_upright() {
        // a tall box in a Y-up file: 10 wide, 40 tall (y), 20 deep (z)
        let m = box_mesh([-5.0, 0.0, -10.0], [5.0, 40.0, 10.0]);
        for (up, fmt, tall) in
            [(Up::Auto, MeshFormat::Obj, true), (Up::Auto, MeshFormat::Stl, false), (Up::Y, MeshFormat::Stl, true), (Up::Z, MeshFormat::Obj, false)]
        {
            let st = Settings { target_mm: 40.0, up, floor: true, centre: true, res: 32 };
            let b = bake(&m, fmt, &st, &BakeControl::new()).unwrap();
            assert_eq!(b.size[2] > 30.0, tall, "{up:?} {fmt:?} {:?}", b.size);
        }
        assert_eq!(upright_size([10.0, 40.0, 20.0], true), [10.0, 20.0, 40.0]);
        // where the file had it is kept when centring is off
        let b =
            bake(&m, MeshFormat::Stl, &Settings { target_mm: 40.0, up: Up::Z, floor: false, centre: false, res: 32 }, &BakeControl::new()).unwrap();
        let p = place(&b, false, false, [99.0, 99.0]);
        assert!((p[1] - 20.0).abs() < 1e-3 && p[0].abs() < 1e-3 && p[2].abs() < 1e-3, "{p:?}");
    }

    #[test]
    fn settings_defaults_and_hostile_numbers() {
        assert_eq!(default_target([30.0, 12.0, 8.0]), 30.0);
        assert_eq!(default_target([1000.0, 2.0, 3.0]), DEFAULT_CAP_MM);
        assert_eq!(default_target([0.01, 0.0, 0.0]), MIN_TARGET_MM);
        assert!(default_target([f32::NAN, 1.0, 1.0]).is_finite());
        let s = Settings { target_mm: f32::NAN, up: Up::Auto, floor: true, centre: true, res: 100_000 }.sanitized();
        assert!(s.target_mm.is_finite() && s.res <= 256);
        let s = Settings { target_mm: -5.0, res: 0, ..s }.sanitized();
        assert_eq!((s.target_mm, s.res), (MIN_TARGET_MM, 16));
        // an empty mesh is refused, not a panic
        assert!(bake(&ImportedMesh::default(), MeshFormat::Stl, &Settings::new([1.0; 3]), &BakeControl::new()).is_err());
        // the extensions the card reads
        for n in ["a.STL", "b.obj", "c.3mf", "d.Ply", "e.glb"] {
            assert!(is_mesh_file(Path::new(n)), "{n}");
        }
        assert!(!is_mesh_file(Path::new("a.meltcad")) && !is_mesh_file(Path::new("noext")));
    }

    #[test]
    fn the_estimate_follows_the_measured_numbers() {
        // a ball at 128 cells is about 137^3 nodes, 1.2 MB, a few seconds
        let e = estimate([60.0; 3], 100_000, 128);
        assert!((e.nodes as f64 / (137.0f64.powi(3)) - 1.0).abs() < 0.02, "{}", e.nodes);
        assert!(e.secs > 1.0 && e.secs < 8.0 && e.bytes > 0.8e6 && e.bytes < 1.6e6, "{e:?}");
        // a flat plate has far fewer nodes
        let flat = estimate([100.0, 100.0, 4.0], 1000, 128);
        assert!(flat.nodes < e.nodes / 4);
        // finer is slower and bigger, and the biggest choice stays inside the scene's budget
        let mut last = 0;
        for r in RES_CHOICES {
            let e = estimate([60.0; 3], 1000, r);
            assert!(e.nodes > last);
            last = e.nodes;
            assert!(e.nodes <= crate::sdf_volume::DEFAULT_MAX_CELLS);
        }
    }

    #[test]
    fn the_scene_limits_refuse_a_mesh_that_does_not_fit() {
        let mut sc = Scene::default();
        assert!(refuse(&sc, 4_000_000).is_none());
        assert_eq!(refuse(&sc, crate::mesh::MAX_TOTAL_NODES + 1), Some(crate::mesh::ERR_TOO_BIG));
        let b = baked_sphere(5.0, 16);
        for i in 0..crate::mesh::MAX_VOLUME_SHAPES {
            sc.groups[0].shapes.push(shape_of(&b, "m", "m.stl", [i as f32 * 20.0, 0.0, 5.0], Material::default()));
        }
        assert_eq!(refuse(&sc, 10), Some(crate::mesh::ERR_TOO_MANY_SHAPES));
    }

    /// A small model (a rounded block, a knob, a hole through it) made of primitives.
    pub(crate) fn sample_model() -> Scene {
        let mut g = Group::new("g");
        let mut body = Shape::new(Kind::Box, [0.0, 0.0, 12.0], Material::default());
        body.size = [40.0, 24.0, 24.0];
        body.round = 4.0;
        body.blend = 6.0;
        let mut knob = Shape::new(Kind::Sphere, [18.0, 0.0, 16.0], Material::default());
        knob.size[0] = 9.0;
        knob.blend = 6.0;
        let mut hole = Shape::new(Kind::Cylinder, [-8.0, 0.0, 12.0], Material::default());
        hole.size = [5.0, 40.0, 1.0];
        hole.rot = [90.0, 0.0, 0.0];
        hole.op = crate::model::Op::Subtract;
        hole.blend = 0.0;
        g.shapes = vec![body, knob, hole];
        Scene { groups: vec![g], ..Scene::default() }
    }

    /// The sample model exported by our own exporter as binary STL bytes.
    pub(crate) fn sample_stl() -> Vec<u8> {
        use crate::export_formats::Format;
        use crate::export_job::{Options, build, serialize};
        use crate::export_mesh::Progress;
        let o = Options { format: Format::Stl, tolerance: 0.3, ..Options::default() };
        let built = build(&sample_model(), None, &o, &Progress::new()).unwrap();
        serialize(&built, &o, Path::new("model.stl")).unwrap().into_iter().next().map(|(_, d)| d).unwrap_or_default()
    }

    /// Our own exporter writes an STL of a model, the importer reads it, the bake makes a volume,
    /// and the distance field of the imported shape matches the original's within two cells.
    #[test]
    fn export_our_own_stl_import_it_and_measure_it_again() {
        use crate::export_formats::Format;
        use crate::export_job::{Options, build, serialize};
        use crate::export_mesh::Progress;
        let original = sample_model();
        let dir = std::env::temp_dir().join(format!("meltcad_e2e_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("model.stl");
        let o = Options { format: Format::Stl, tolerance: 0.3, ..Options::default() };
        let built = build(&original, None, &o, &Progress::new()).unwrap();
        for (p, data) in serialize(&built, &o, &path).unwrap() {
            std::fs::write(p, data).unwrap();
        }
        let loaded = load(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(loaded.imported.health.is_closed(), "{:?}", loaded.imported.notes);
        assert_eq!(loaded.file_name, "model.stl");
        // import at the model's own size
        let size = loaded.size;
        let st = Settings { target_mm: default_target(size), up: Up::Z, floor: false, centre: false, res: 96 };
        // (the original's box is the same as the file's, so the file's own place is the original place)
        let baked = bake(&loaded.imported.mesh, loaded.imported.format, &st, &BakeControl::new()).unwrap();
        let pos = place(&baked, false, false, [0.0, 0.0]);
        let imported = scene_with(shape_of(&baked, "m", "model.stl", pos, Material::default()));
        let (fo, fi) = (Field::new(&original), Field::new(&imported));
        let cell = baked.volume.cell;
        let mut checked = 0;
        let mut worst = 0.0f32;
        for ix in -6..=6 {
            for iy in -4..=4 {
                for iz in -1..=7 {
                    let p = [ix as f32 * 4.0, iy as f32 * 4.0, iz as f32 * 4.5];
                    let (a, b) = (fo.distance(p), fi.distance(p));
                    // near the surface and inside the grid (farther in, the blended original is not a true distance)
                    let local = [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2]];
                    let (lo, hi) = baked.volume.bounds();
                    let inside = (0..3).all(|k| local[k] >= lo[k] && local[k] <= hi[k]);
                    if a.abs() < 3.0 && inside {
                        checked += 1;
                        worst = worst.max((a - b).abs());
                        assert!((a - b).abs() <= 2.0 * cell, "at {p:?}: original {a}, imported {b} (cell {cell})");
                    }
                }
            }
        }
        assert!(checked > 300, "{checked}");
        eprintln!("end to end: {checked} points, worst difference {worst:.3} mm, cell {cell:.3} mm, volume {} nodes", baked.volume.nodes());
    }
    #[test]
    fn rebake_reduces_cell_width_and_records_surface_residuals() {
        let mesh = sphere_mesh(5.0, 16, 32);
        let ctrl = BakeControl::new();
        let size = [20.0, 10.0, 10.0];
        let started = std::time::Instant::now();
        let low = rebake(&mesh, false, size, 24, &ctrl).unwrap();
        let low_secs = started.elapsed().as_secs_f64();
        let started = std::time::Instant::now();
        let high = rebake(&mesh, false, size, 48, &ctrl).unwrap();
        let high_secs = started.elapsed().as_secs_f64();
        assert!(high.volume.cell < low.volume.cell);
        assert_eq!(high.size, size);
        let e0 = surface_residual(&mesh, false, size, &low.volume);
        let e1 = surface_residual(&mesh, false, size, &high.volume);
        assert!(e0.2 > 0 && e1.2 > 0);
        assert!(e0.0.is_finite() && e1.0.is_finite());
        eprintln!(
            "rebake centroid residual mean/max low {:.4}/{:.4}mm {:.3}s; high {:.4}/{:.4}mm {:.3}s",
            e0.0, e0.1, low_secs, e1.0, e1.1, high_secs
        );
    }

    #[test]
    fn rebake_cancel_keeps_the_caller_in_control() {
        use std::sync::atomic::Ordering;
        let mesh = box_mesh([-5.0; 3], [5.0; 3]);
        let ctrl = BakeControl::new();
        ctrl.cancel.store(true, Ordering::Relaxed);
        let result = rebake(&mesh, false, [10.0; 3], 64, &ctrl);
        assert_eq!(result.unwrap_err(), crate::sdf_volume::ERR_CANCEL);
    }

    #[test]
    fn cleaned_mesh_fingerprint_is_deterministic_and_detects_changes() {
        let mut mesh = box_mesh([-1.0; 3], [1.0; 3]);
        let before = mesh_hash(&mesh);
        assert_eq!(before, mesh_hash(&mesh));
        mesh.positions[0][0] += 0.1;
        assert_ne!(before, mesh_hash(&mesh));
    }
}
