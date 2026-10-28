//! Export as a job: pick the part of the scene to export, mesh it to the asked precision, smooth and
//! lighten it, work out its colours, convert units and axes, and write the files. It runs on a worker
//! thread with a progress bar and a cancel flag, so the window never freezes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use solidcraft_geom::melt::{self, TriMesh};

use crate::export_decimate::{self, Carry, Reduce};
use crate::export_formats::{self as fmt, Format, Look, Mat8, Surface, Units};
use crate::export_mesh::{self, Meshed, Progress};
use crate::model::{BlendMode, Kind, Op, Pick, Scene, Shape};
use crate::sdf::Field;

pub const MIN_TOLERANCE: f32 = 0.01;
pub const MAX_TOLERANCE: f32 = 5.0;

/// Which part of the scene is exported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Everything, also what is hidden.
    All,
    /// Only the selected shape or group.
    Selected,
    /// What can be seen.
    Visible,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Up {
    Z,
    Y,
}

impl Up {
    pub fn id(self) -> &'static str {
        match self {
            Up::Z => "z",
            Up::Y => "y",
        }
    }
    pub fn from_id(id: &str) -> Option<Up> {
        match id {
            "z" => Some(Up::Z),
            "y" => Some(Up::Y),
            _ => None,
        }
    }
}

/// How to make the mesh lighter.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lighten {
    /// Keep every cell triangle.
    Off,
    /// Merge flat faces (and nothing else) into big triangles.
    Flat,
    /// Reduce to about this many triangles.
    Target(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub format: Format,
    /// The size of a cell of the mesh in mm: the precision.
    pub tolerance: f32,
    pub units: Units,
    /// 100 = as modelled.
    pub scale_pct: f32,
    pub scope: Scope,
    pub up: Up,
    /// Rounds of smoothing (0..=4).
    pub smooth: u8,
    pub lighten: Lighten,
    pub ignore_paint: bool,
    /// Write the colours if the format can hold them.
    pub colour: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            format: Format::Stl,
            tolerance: 0.2,
            units: Units::Mm,
            scale_pct: 100.0,
            scope: Scope::Visible,
            up: Up::Z,
            smooth: 0,
            lighten: Lighten::Flat,
            ignore_paint: false,
            colour: true,
        }
    }
}

impl Options {
    /// Make every number safe to use.
    pub fn sanitized(&self) -> Options {
        let mut o = self.clone();
        o.tolerance = if o.tolerance.is_finite() { o.tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE) } else { 0.2 };
        o.scale_pct = if o.scale_pct.is_finite() { o.scale_pct.clamp(0.01, 100_000.0) } else { 100.0 };
        o.smooth = o.smooth.min(4);
        if let Lighten::Target(n) = o.lighten {
            o.lighten = Lighten::Target(n.clamp(100, 100_000_000));
        }
        o
    }
}

pub fn threads() -> usize {
    std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).clamp(1, 32)
}

// ---------------------------------------------------------------- picking the part of the scene

/// The scene to mesh: the chosen part of it, without stains if they are to be ignored.
/// What is selected: one pick, several (a multi-selection), or nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picks(pub Vec<Pick>);

impl From<Option<Pick>> for Picks {
    fn from(p: Option<Pick>) -> Self {
        Picks(p.into_iter().collect())
    }
}

impl From<Vec<Pick>> for Picks {
    fn from(p: Vec<Pick>) -> Self {
        Picks(p)
    }
}

pub fn prepare(scene: &Scene, sel: impl Into<Picks>, scope: Scope, ignore_paint: bool) -> Result<Scene, String> {
    let sel: Picks = sel.into();
    let mut s = scene.clone();
    match scope {
        Scope::Visible => {}
        Scope::All => {
            for g in &mut s.groups {
                g.visible = true;
                for sh in &mut g.shapes {
                    sh.visible = true;
                }
            }
        }
        Scope::Selected => {
            // everything else is hidden rather than removed: patterns and linked copies find their
            // sources by number, wherever they are in the list
            if sel.0.is_empty() {
                return Err("nothing is selected".into());
            }
            // The groups taken whole (with all they hold), the single shapes taken from a group,
            // and the groups above them, which stay only as containers.
            let mut whole: Vec<(usize, usize)> = Vec::new();
            let mut single: std::collections::HashMap<usize, std::collections::HashSet<usize>> = std::collections::HashMap::new();
            let mut ancestors = std::collections::HashSet::new();
            for pick in &sel.0 {
                let gi = match *pick {
                    Pick::Group(gi) | Pick::Shape(gi, _) => gi,
                };
                if scene.groups.get(gi).is_none() {
                    return Err("the selection is gone".into());
                }
                match *pick {
                    Pick::Group(gi) => whole.push((gi, scene.subtree_end(gi))),
                    Pick::Shape(gi, si) => {
                        if scene.shape(gi, si).is_none() {
                            return Err("the selection is gone".into());
                        }
                        single.entry(gi).or_default().insert(si);
                    }
                }
                let mut at = scene.parent_of(gi);
                while let Some(parent) = at {
                    ancestors.insert(parent);
                    at = scene.parent_of(parent);
                }
            }
            let tops: std::collections::HashSet<usize> = whole.iter().map(|w| w.0).collect();
            for (i, g) in s.groups.iter_mut().enumerate() {
                let in_whole = whole.iter().any(|(a, b)| (*a..*b).contains(&i));
                let some_shapes = single.get(&i);
                let selected = in_whole || some_shapes.is_some();
                let container = !selected && ancestors.contains(&i);
                g.visible = selected || container;
                if container {
                    g.op = Op::Add;
                    g.pattern = None;
                    for sh in &mut g.shapes {
                        sh.visible = false;
                    }
                }
                if tops.contains(&i) || (some_shapes.is_some() && !in_whole) {
                    g.op = Op::Add;
                }
                if let (Some(only), false) = (some_shapes, in_whole) {
                    for (k, sh) in g.shapes.iter_mut().enumerate() {
                        sh.visible = only.contains(&k);
                        if only.contains(&k) {
                            sh.op = Op::Add;
                        }
                    }
                }
            }
        }
    }
    if ignore_paint {
        // Paint groups can contain Add descendants: deleting only the Paint
        // parent accidentally turns those descendants into solid geometry.
        // Hide instead; the existing hierarchy and UID references stay intact.
        for g in &mut s.groups {
            if g.op == Op::Paint {
                g.visible = false;
            }
            for sh in &mut g.shapes {
                if sh.op == Op::Paint {
                    sh.visible = false;
                }
            }
        }
    }
    Ok(s)
}

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
            wide.hypot(sz[0] * 0.5) + f64::from(if s.params[0].is_nan() { 0.0 } else { s.params[0].clamp(0.0, 40.0) })
        }
        _ => s.local_half().iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>().sqrt(),
    };
    let scale = s.scale.iter().map(|v| f64::from(*v)).fold(0.0_f64, f64::max);
    let extra = s.ripple.map_or(0.0, |p| if p.preserve_outline { 0.0 } else { f64::from(p.height) * scale });
    let r = local * scale + extra;
    // hollow shapes and rounded corners never reach farther than the solid
    if r.is_finite() { r } else { 0.0 }
}

/// The box around everything that is added to the scene, with room for blending and drips.
pub fn bounds(scene: &Scene) -> Result<([f64; 3], [f64; 3]), String> {
    let m = scene.melt;
    let factor = match m.mode {
        BlendMode::Smooth => 1.0,
        BlendMode::Round => 3.5,
        BlendMode::Gooey => 6.0,
    };
    let amount = if m.amount.is_finite() { f64::from(m.amount.max(0.0)) } else { 1.0 };
    let drip = if m.drip.is_finite() { f64::from(m.drip.max(0.0)) } else { 0.0 };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for rg in crate::pattern::resolve(scene) {
        let (Some(g), Some(src)) = (scene.groups.get(rg.gi), scene.groups.get(rg.src)) else { continue };
        if g.op != Op::Add {
            continue;
        }
        let one = [crate::pattern::Inst::IDENTITY];
        let insts: &[crate::pattern::Inst] = if rg.insts.is_empty() { &one } else { &rg.insts };
        for s in src.shapes.iter().filter(|s| s.solid() && s.op == Op::Add) {
            let r = reach_of(s) * 1.05 + (f64::from(s.blend.max(0.0)) + f64::from(scene.group_melt_reach(rg.gi))) * amount * factor;
            for inst in insts {
                let at = inst.apply(s.pos);
                let r = r * f64::from(inst.s);
                for a in 0..3 {
                    let c = f64::from(at[a]);
                    lo[a] = lo[a].min(c - r);
                    hi[a] = hi[a].max(c + r);
                }
            }
        }
    }
    lo[2] -= drip;
    if lo.iter().chain(hi.iter()).any(|v| !v.is_finite()) {
        return Err("there is nothing to export: add a shape first".into());
    }
    Ok((lo, hi))
}

/// Do all the visible shapes look the same? Then the model has one look.
fn single_look(scene: &Scene) -> Option<Mat8> {
    let mut first: Option<Mat8> = None;
    for rg in crate::pattern::resolve(scene) {
        let Some(src) = scene.groups.get(rg.src) else { continue };
        if scene.groups.get(rg.gi).is_some_and(|g| g.op == Op::Paint) {
            return None;
        }
        for s in src.shapes.iter().filter(|s| s.solid()) {
            // One base material does not imply one surface colour: a shape
            // can carry a spatial gradient or a Paint stain.
            if s.material.has_gradient() || s.op == Op::Paint {
                return None;
            }
            let m = look_of(s.material.color, s.material.roughness, s.material.metallic);
            match first {
                None => first = Some(m),
                Some(f) if f == m => {}
                Some(_) => return None,
            }
        }
    }
    first
}

fn byte(v: f32) -> u8 {
    if v.is_finite() { (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 } else { 0 }
}

fn look_of(color: [f32; 3], rough: f32, metal: f32) -> Mat8 {
    Mat8 { rgb: [byte(color[0]), byte(color[1]), byte(color[2])], rough: byte(rough), metal: byte(metal) }
}

// ---------------------------------------------------------------- meshing

/// The mesh of the (already prepared) scene.
pub fn mesh_scene(scene: &Scene, tolerance: f64, threads: usize, prog: &export_mesh::Slice) -> Result<Meshed, String> {
    let field = Field::new(scene);
    let (mut lo, mut hi) = bounds(scene)?;
    let sdf = |p: [f32; 3]| field.distance(p);
    let mut last_err = String::new();
    for _ in 0..4 {
        let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
        if !extent.is_finite() || extent <= 1e-9 {
            return Err("the shapes have no size".into());
        }
        let meshed = match export_mesh::mesh_sparse(&sdf, lo, hi, tolerance, threads, prog) {
            Ok(m) => m,
            Err(e) => {
                last_err = e;
                break;
            }
        };
        // the surface reaching past the box means the box was too tight: grow it and mesh again
        let slack = tolerance * 0.5;
        let out = meshed.mesh.positions.iter().any(|p| (0..3).any(|a| p[a] < lo[a] - slack || p[a] > hi[a] + slack));
        if !out {
            return Ok(meshed);
        }
        for a in 0..3 {
            lo[a] -= extent * 0.3;
            hi[a] += extent * 0.3;
        }
    }
    if last_err.is_empty() { Err("the model is bigger than expected".into()) } else { Err(last_err) }
}

/// The looks of a scene at a point: the field's own material mix, then the stains on top. (The CPU
/// field leaves stains out because they change no distance; the GPU shader paints them, and so
/// does this, with the same soft edge.)
struct Painter {
    field: Field,
}

impl Painter {
    fn new(scene: &Scene) -> Painter {
        // Field::sample already composites Op::Paint masks in exactly the
        // same direction as the CPU renderer. Reapplying masks here used to
        // recolour surfaces OUTSIDE the painted area (and double-blend edges).
        Painter { field: Field::new(scene) }
    }

    fn look(&self, p: [f32; 3]) -> Mat8 {
        let s = self.field.sample(p);
        look_of(s.color, s.roughness, s.metallic)
    }
}

fn sample_looks(painter: &Painter, mesh: &TriMesh, threads: usize, prog: &export_mesh::Slice) -> Result<Vec<Mat8>, String> {
    const CHUNK: usize = 4096;
    let chunks = mesh.positions.len().div_ceil(CHUNK);
    let parts = export_mesh::par_run(threads, chunks, prog, |c| {
        let s = c * CHUNK;
        let e = (s + CHUNK).min(mesh.positions.len());
        let mut v = Vec::with_capacity(e - s);
        for p in mesh.positions.get(s..e).unwrap_or(&[]) {
            v.push(painter.look([p[0] as f32, p[1] as f32, p[2] as f32]));
        }
        Ok(v)
    })?;
    Ok(parts.into_iter().flatten().collect())
}

/// Group the vertices' looks into classes (a coarse palette), as few as it takes to stay under `max`.
fn classify(looks: &[Mat8], max: usize) -> (Vec<u16>, usize) {
    for shift in 4..8u32 {
        let mut map: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
        let mut class = Vec::with_capacity(looks.len());
        let mut ok = true;
        for m in looks {
            let key = (u32::from(m.rgb[0]) >> shift)
                | ((u32::from(m.rgb[1]) >> shift) << 8)
                | ((u32::from(m.rgb[2]) >> shift) << 16)
                | ((u32::from(m.rough) >> (shift + 1)) << 24)
                | ((u32::from(m.metal) >> (shift + 1)) << 28);
            let next = map.len();
            let c = *map.entry(key).or_insert(next as u16);
            if map.len() > max {
                ok = false;
                break;
            }
            class.push(c);
        }
        if ok {
            return (class, map.len());
        }
    }
    (vec![0; looks.len()], 1)
}

/// Turn the carried vertex looks into what the writers want: a palette, a class per triangle and a
/// smooth colour per vertex.
fn finish_look(mesh: &TriMesh, carry: &Carry) -> Look {
    let nclass = carry.class.iter().copied().max().map_or(0, |m| usize::from(m) + 1);
    let mut sum = vec![[0u64; 5]; nclass];
    for (i, c) in carry.class.iter().enumerate() {
        let (Some(s), Some(rgb), Some(rm)) = (sum.get_mut(usize::from(*c)), carry.rgb.get(i), carry.rm.get(i)) else { continue };
        for k in 0..3 {
            s[k] += u64::from(rgb[k]);
        }
        s[3] += u64::from(rm[0]);
        s[4] += u64::from(rm[1]);
    }
    let mut count = vec![0u64; nclass];
    for c in &carry.class {
        if let Some(n) = count.get_mut(usize::from(*c)) {
            *n += 1;
        }
    }
    // only the classes still in use, in order
    let mut renumber = vec![u16::MAX; nclass];
    let mut palette: Vec<Mat8> = Vec::new();
    for c in 0..nclass {
        let n = count.get(c).copied().unwrap_or(0);
        if n == 0 {
            continue;
        }
        if let (Some(s), Some(r)) = (sum.get(c), renumber.get_mut(c)) {
            *r = palette.len() as u16;
            palette.push(Mat8 { rgb: [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8], rough: (s[3] / n) as u8, metal: (s[4] / n) as u8 });
        }
    }
    let class_of = |v: u32| carry.class.get(v as usize).and_then(|c| renumber.get(usize::from(*c))).copied().unwrap_or(0);
    let tri_class: Vec<u16> = mesh
        .triangles
        .iter()
        .map(|t| {
            let c = [class_of(t[0]), class_of(t[1]), class_of(t[2])];
            if c[0] == c[1] || c[0] == c[2] {
                c[0]
            } else if c[1] == c[2] {
                c[1]
            } else {
                c[0]
            }
        })
        .collect();
    if palette.len() <= 1 {
        return match palette.first() {
            Some(m) => Look::uniform(*m),
            None => Look::none(),
        };
    }
    Look { palette, tri_class, vert_rgb: carry.rgb.clone() }
}

/// What a finished build knows about itself.
pub struct Built {
    pub surface: Surface,
    /// Width, depth, height in mm (as modelled).
    pub size_mm: [f64; 3],
    pub tris: usize,
    pub verts: usize,
    pub closed: bool,
}

fn size_of(mesh: &TriMesh) -> [f64; 3] {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &mesh.positions {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    if mesh.positions.is_empty() { [0.0; 3] } else { [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] }
}

/// Mesh, smooth, lighten, colour and convert: everything up to writing.
pub fn build(scene: &Scene, sel: impl Into<Picks>, o: &Options, prog: &Progress) -> Result<Built, String> {
    let o = o.sanitized();
    let th = threads();
    let scene = prepare(scene, sel, o.scope, o.ignore_paint)?;
    let tol = f64::from(o.tolerance);
    let t_start = Instant::now();
    let timing = std::env::var_os("YURA_EXPORT_TIMING").is_some();
    let mut meshed = mesh_scene(&scene, tol, th, &prog.slice(0.0, 0.66))?;
    let complete = meshed.stats.complete;
    if timing {
        let s = meshed.stats;
        eprintln!(
            "  mesh {:.2}s (find {:.2} sample {:.2} join {:.2} build {:.2}) blocks {} cells {} evals {} ({:.1}/cell) tris {}",
            t_start.elapsed().as_secs_f64(),
            s.secs[0],
            s.secs[1],
            s.secs[2],
            s.secs[3],
            s.blocks,
            s.cells,
            s.evals,
            s.evals as f64 / s.cells.max(1) as f64,
            meshed.mesh.triangles.len()
        );
    }
    let t_after = Instant::now();
    let colour = o.colour && o.format.has_colour();
    if o.smooth > 0 {
        prog.set_phase(3);
        export_decimate::smooth(&mut meshed.mesh, &meshed.sharp, usize::from(o.smooth), &prog.slice(0.66, 0.70))?;
    }
    // vertex looks, for the colours and so that the lightening keeps colour borders
    let mut carry = Carry::default();
    let single = if colour { single_look(&scene) } else { None };
    if colour && single.is_none() {
        prog.set_phase(5);
        let painter = Painter::new(&scene);
        let looks = sample_looks(&painter, &meshed.mesh, th, &prog.slice(0.70, 0.78))?;
        let (class, _) = classify(&looks, 250);
        carry = Carry { class, rgb: looks.iter().map(|m| m.rgb).collect(), rm: looks.iter().map(|m| [m.rough, m.metal]).collect() };
    }
    let mut mesh = meshed.mesh;
    let reduce = match o.lighten {
        Lighten::Off => None,
        Lighten::Flat => Some(Reduce { target_tris: 0, max_error: 0.02 * tol, quantum: (0.002 * tol).powi(2), tile_error: 0.02 * tol }),
        Lighten::Target(n) => {
            Some(Reduce { target_tris: n as usize, max_error: f64::INFINITY, quantum: (0.002 * tol).powi(2), tile_error: 0.02 * tol })
        }
    };
    if let Some(r) = reduce {
        prog.set_phase(4);
        export_decimate::reduce(&mut mesh, &mut carry, r, th, &prog.slice(0.78, 0.94))?;
    }
    if prog.cancelled() {
        return Err(export_mesh::CANCELLED.into());
    }
    if timing {
        eprintln!("  smooth/colour/lighten {:.2}s -> {} tris", t_after.elapsed().as_secs_f64(), mesh.triangles.len());
    }
    let look = if !colour {
        Look::none()
    } else if let Some(m) = single {
        Look::uniform(m)
    } else {
        finish_look(&mesh, &carry)
    };
    let size_mm = size_of(&mesh);
    // Never report a closed surface that was not checked, however big the mesh is.
    let closed = complete && melt::is_watertight(&mesh);
    let (tris, verts) = (mesh.triangles.len(), mesh.positions.len());
    convert(&mut mesh, &o);
    Ok(Built { surface: Surface { mesh, look }, size_mm, tris, verts, closed })
}

/// Units, scale and the up axis. glTF is always in metres.
fn convert(mesh: &mut TriMesh, o: &Options) -> f64 {
    let unit = if o.format == Format::Glb { Units::M } else { o.units };
    let k = unit.per_mm() * f64::from(o.scale_pct) / 100.0;
    for p in &mut mesh.positions {
        let q = [p[0] * k, p[1] * k, p[2] * k];
        *p = if o.up == Up::Y { [q[0], q[2], -q[1]] } else { q };
    }
    k
}

/// The unit the file is in (what the 3MF header says).
pub fn file_unit(o: &Options) -> Units {
    if o.format == Format::Glb { Units::M } else { o.units }
}

/// OBJ's mtllib directive accepts whitespace-separated library names, not quoted
/// filenames in every importer. Keep a safe ASCII companion filename and a stable
/// suffix if sanitization was needed, to avoid collisions with other OBJ exports.
fn obj_material_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "model".into());
    let plain: String = stem.chars().take(64).map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let plain = if plain.is_empty() { "model".to_owned() } else { plain };
    let safe = if plain == stem {
        plain
    } else {
        let hash = stem.as_bytes().iter().fold(2166136261u32, |h, b| (h ^ u32::from(*b)).wrapping_mul(16777619));
        format!("{plain}_{hash:08x}")
    };
    path.with_file_name(format!("{safe}.mtl"))
}

/// The files of a build, named after `path` (an OBJ with colours also gets its MTL).
pub fn serialize(b: &Built, o: &Options, path: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    // All formats must report broken faces, including the legacy infallible STL
    // byte writers, instead of producing an apparently successful empty mesh.
    fmt::check(&b.surface.mesh)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "MELTCAD".into());
    let main = path.to_path_buf();
    Ok(match o.format {
        Format::Stl => vec![(main, fmt::stl_binary(&b.surface.mesh, &stem))],
        Format::StlAscii => vec![(main, fmt::stl_ascii(&b.surface.mesh, &stem).into_bytes())],
        Format::Obj => {
            let mtl_path = obj_material_path(path);
            let mtl_name = mtl_path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "model.mtl".into());
            let (obj, mtl) = fmt::obj_files(&b.surface, &stem, &mtl_name)?;
            let mut v = vec![(main, obj.into_bytes())];
            if let Some(m) = mtl {
                v.push((mtl_path, m.into_bytes()));
            }
            v
        }
        Format::ThreeMf => vec![(main, fmt::threemf(&b.surface, &stem, file_unit(o))?)],
        Format::Ply => vec![(main, fmt::ply(&b.surface, &stem)?)],
        Format::Glb => vec![(main, fmt::glb(&b.surface, &stem)?)],
    })
}

// ---------------------------------------------------------------- the worker

/// What a finished export tells the person.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Report {
    pub path: PathBuf,
    pub tris: usize,
    pub verts: usize,
    pub bytes: u64,
    pub secs: f64,
    pub size_mm: [f64; 3],
    pub closed: bool,
}

pub const NAMES: [&str; 6] = ["Searching space…", "Measuring the shape…", "Joining faces…", "Smoothing…", "Reducing triangles…", "Checking colours…"];

/// An export running on its own thread.
pub struct Job {
    pub progress: Arc<Progress>,
    rx: std::sync::Mutex<Receiver<Result<Report, String>>>,
    started: Instant,
    pub label: String,
    // 0 = cancellable; 1 = cancel requested; 2 = final commit (cannot cancel).
    writing: Arc<AtomicU8>,
}

impl Job {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn start(scene: Scene, sel: impl Into<Picks>, opts: Options, path: PathBuf) -> Job {
        Self::start_with_permit(scene, sel, opts, path, None)
    }

    /// The UI passes an exclusive high-memory permit; it remains owned by the worker until exit.
    pub fn start_with_permit(scene: Scene, sel: impl Into<Picks>, opts: Options, path: PathBuf, permit: Option<crate::work_budget::Permit>) -> Job {
        let sel: Picks = sel.into();
        let progress = Arc::new(Progress::new());
        let writing = Arc::new(AtomicU8::new(0));
        let (tx, rx) = channel();
        let p = progress.clone();
        let w = writing.clone();
        let label = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let spawned = std::thread::Builder::new().name("yura-export".into()).spawn(move || {
            let t0 = Instant::now();
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&scene, sel.clone(), &opts, &path, &p, &w, t0)));
            let res = res.unwrap_or_else(|_| Err("the export stopped unexpectedly".into()));
            drop(permit); // unlock before the completion is observed on the next UI frame
            let _ = tx.send(res);
        });
        if let Err(e) = spawned {
            let (tx2, rx2) = channel();
            let _ = tx2.send(Err(format!("could not start the export: {e}")));
            return Job { progress, rx: std::sync::Mutex::new(rx2), started: Instant::now(), label, writing };
        }
        Job { progress, rx: std::sync::Mutex::new(rx), started: Instant::now(), label, writing }
    }

    /// The result, once there is one.
    pub fn poll(&self) -> Option<Result<Report, String>> {
        let rx = self.rx.lock().ok()?;
        match rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the export stopped unexpectedly".into())),
        }
    }

    /// Cancellation wins only while the worker has not entered the final file commit.
    pub fn cancel(&self) -> bool {
        if self.writing.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            self.progress.cancel();
            true
        } else {
            false
        }
    }

    pub fn can_cancel(&self) -> bool {
        self.writing.load(Ordering::Acquire) == 0
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// 0..=1.
    pub fn fraction(&self) -> f32 {
        self.progress.permille() as f32 / 1000.0
    }

    pub fn phase_name(&self) -> &'static str {
        if self.writing.load(Ordering::Acquire) == 2 {
            return "Writing the file (this last step cannot be stopped)…";
        }
        if self.writing.load(Ordering::Acquire) == 1 {
            return "Waiting for the stop to finish…";
        }
        NAMES.get(self.progress.phase() as usize).copied().unwrap_or("Building…")
    }
}

fn run(scene: &Scene, sel: Picks, o: &Options, path: &Path, prog: &Progress, writing: &AtomicU8, t0: Instant) -> Result<Report, String> {
    let built = build(scene, sel, o, prog)?;
    if prog.cancelled() {
        return Err(export_mesh::CANCELLED.into());
    }
    prog.set(0.95);
    let files = serialize(&built, &o.sanitized(), path)?;
    let bytes = crate::atomic_files::write_files(
        &files,
        &|| prog.cancelled(),
        &|| writing.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire).is_ok(),
        &|p| prog.set(0.96 + f64::from(p) * 0.039),
    )?;
    prog.set(1.0);
    Ok(Report {
        path: path.to_path_buf(),
        tris: built.tris,
        verts: built.verts,
        bytes,
        secs: t0.elapsed().as_secs_f64(),
        size_mm: built.size_mm,
        closed: built.closed,
    })
}

// ---------------------------------------------------------------- print check

/// What a person wants to know before printing.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct PrintCheck {
    /// Width, depth and height of the model in mm.
    pub size: [f64; 3],
    /// Volume in cubic centimetres.
    pub volume_cm3: f64,
    /// Is the surface closed (no holes in the mesh)?
    pub closed: bool,
    /// Share of the surface (percent) where the wall is thinner than `THIN_MM`.
    pub thin_pct: f64,
    /// The thinnest wall found (mm), if any is under the limit.
    pub thinnest: Option<f64>,
    /// Share of the surface (percent) that leans out more than 45 degrees (not counting the bottom).
    pub overhang_pct: f64,
    pub tris: usize,
    pub cell: f64,
}

pub const THIN_MM: f64 = 0.8;

fn tri_normal_area(t: [[f64; 3]; 3]) -> ([f64; 3], f64) {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if l > 1e-300 { ([c[0] / l, c[1] / l, c[2] / l], 0.5 * l) } else { ([0.0, 0.0, 1.0], 0.0) }
}

/// Measure the model for printing. `tolerance` is the precision wanted; the check itself never goes
/// below a cell of a 350th of the model so that it stays quick.
pub fn print_check(scene: &Scene, sel: impl Into<Picks>, o: &Options, prog: &Progress) -> Result<PrintCheck, String> {
    let th = threads();
    let scene = prepare(scene, sel, o.scope, o.ignore_paint)?;
    let (lo, hi) = bounds(&scene)?;
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
    let cell = f64::from(o.tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE)).max(extent / 350.0).max(0.05);
    let meshed = mesh_scene(&scene, cell, th, &prog.slice(0.0, 0.8))?;
    let mesh = &meshed.mesh;
    // Uniform export scaling changes real-world thickness and volume, not just
    // the coordinates written to disk. File units do not change physical mm.
    let physical_scale = f64::from(o.sanitized().scale_pct) / 100.0;
    let size = size_of(mesh).map(|v| v * physical_scale);
    let thin_limit_source = THIN_MM / physical_scale;
    let thin_step_source = 0.1 / physical_scale;
    let inside_source = 0.03_f64.min(thin_limit_source * 0.1);
    let (mut zmin, mut area, mut over) = (f64::INFINITY, 0.0, 0.0);
    for p in &mesh.positions {
        zmin = zmin.min(p[2]);
    }
    let corners = |t: &[u32; 3]| -> Option<[[f64; 3]; 3]> {
        Some([*mesh.positions.get(t[0] as usize)?, *mesh.positions.get(t[1] as usize)?, *mesh.positions.get(t[2] as usize)?])
    };
    let mut vnormal = vec![[0.0f64; 3]; mesh.positions.len()];
    let mut varea = vec![0.0f64; mesh.positions.len()];
    for t in &mesh.triangles {
        let Some(c) = corners(t) else { continue };
        let (n, a) = tri_normal_area(c);
        area += a;
        let on_bed = c.iter().all(|p| p[2] <= zmin + 0.05 + cell * 0.5) && n[2] < -0.9;
        if n[2] < -std::f64::consts::FRAC_1_SQRT_2 && !on_bed {
            over += a;
        }
        for v in t {
            if let (Some(vn), Some(va)) = (vnormal.get_mut(*v as usize), varea.get_mut(*v as usize)) {
                for k in 0..3 {
                    vn[k] += n[k] * a;
                }
                *va += a;
            }
        }
    }
    // thin walls: from sample points on the surface look inwards, along the normal
    prog.set_phase(1);
    let field = Field::new(&scene);
    let step = (mesh.positions.len() / 20_000).max(1);
    let picks: Vec<usize> = (0..mesh.positions.len()).step_by(step).collect();
    let chunks = picks.len().div_ceil(512);
    let parts = export_mesh::par_run(th, chunks, &prog.slice(0.8, 1.0), |c| {
        let mut thin = 0.0;
        let mut total = 0.0;
        let mut thinnest = f64::INFINITY;
        for i in picks.get(c * 512..((c + 1) * 512).min(picks.len())).unwrap_or(&[]) {
            let (Some(p), Some(n), Some(a)) = (mesh.positions.get(*i), vnormal.get(*i), varea.get(*i)) else { continue };
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l < 1e-9 {
                continue;
            }
            let d = [n[0] / l, n[1] / l, n[2] / l];
            total += *a;
            // Ray march in source coordinates, but test the *exported*
            // physical 0.8 mm wall threshold (and report the exported mm).
            let at = |t: f64| field.distance([(p[0] - d[0] * t) as f32, (p[1] - d[1] * t) as f32, (p[2] - d[2] * t) as f32]);
            if at(inside_source) >= 0.0 {
                continue;
            }
            let mut t = thin_step_source;
            while t < thin_limit_source {
                if at(t) >= 0.0 {
                    thin += *a;
                    thinnest = thinnest.min(t * physical_scale);
                    break;
                }
                t += thin_step_source;
            }
        }
        Ok((thin, total, thinnest))
    })?;
    let (thin, total, thinnest) = parts.iter().fold((0.0, 0.0, f64::INFINITY), |a, p| (a.0 + p.0, a.1 + p.1, a.2.min(p.2)));
    Ok(PrintCheck {
        size,
        volume_cm3: melt::volume(mesh).abs() * physical_scale.powi(3) / 1000.0,
        closed: melt::is_watertight(mesh),
        thin_pct: if total > 0.0 { 100.0 * thin / total } else { 0.0 },
        thinnest: thinnest.is_finite().then_some(thinnest),
        overhang_pct: if area > 0.0 { 100.0 * over / area } else { 0.0 },
        tris: mesh.triangles.len(),
        cell: cell * physical_scale,
    })
}

// ---------------------------------------------------------------- the estimate

/// A quick look at the model, used to tell how big and how slow an export will be.
#[derive(Clone, Copy, Debug, Default)]
pub struct Estimate {
    pub size: [f64; 3],
    /// Surface area in mm^2.
    pub area: f64,
    /// Samples taken per surface cell, and seconds per sample on one thread.
    pub evals_per_cell: f64,
    pub secs_per_eval: f64,
}

impl Estimate {
    /// About how many triangles a mesh with cells of `tolerance` mm has before lightening.
    pub fn raw_tris(&self, tolerance: f64) -> f64 {
        3.2 * self.area / (tolerance * tolerance).max(1e-9)
    }
    /// About how many seconds the export takes.
    pub fn secs(&self, tolerance: f64, lighten: bool) -> f64 {
        let cells = self.area / (tolerance * tolerance).max(1e-9);
        let work = cells * self.evals_per_cell * self.secs_per_eval / threads() as f64;
        // plus joining the faces, lightening (about a microsecond a triangle on 4 threads) and writing
        let light = if lighten { cells * 2.0 * 1.0e-6 * 4.0 / threads() as f64 } else { 0.0 };
        1.0 + work + cells * 3.0e-7 + light
    }
}

pub fn estimate(scene: &Scene, sel: impl Into<Picks>, o: &Options, prog: &Progress) -> Result<Estimate, String> {
    let scene = prepare(scene, sel, o.scope, o.ignore_paint)?;
    let (lo, hi) = bounds(&scene)?;
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0_f64, f64::max);
    let cell = (extent / 70.0).max(0.05);
    let t0 = Instant::now();
    let th = threads();
    let m = mesh_scene(&scene, cell, th, &prog.slice(0.0, 1.0))?;
    let secs = t0.elapsed().as_secs_f64();
    let mut area = 0.0;
    for t in &m.mesh.triangles {
        let (Some(a), Some(b), Some(c)) =
            (m.mesh.positions.get(t[0] as usize), m.mesh.positions.get(t[1] as usize), m.mesh.positions.get(t[2] as usize))
        else {
            continue;
        };
        area += tri_normal_area([*a, *b, *c]).1;
    }
    Ok(Estimate {
        size: size_of(&m.mesh),
        area,
        evals_per_cell: m.stats.evals as f64 / m.stats.cells.max(1) as f64,
        secs_per_eval: (secs * th as f64 / m.stats.evals.max(1) as f64).clamp(1e-8, 1e-3),
    })
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_formats::tests::{read_stl_binary, unzip};
    use crate::model::{Group, Material};

    #[test]
    fn obj_material_names_are_safe_and_match_actual_companion_file() {
        let paths = ["My Car.obj", "バッグ (青).obj", "Ordinary_Name.obj"];
        for name in paths {
            let path = Path::new(name);
            let mtl = obj_material_path(path);
            let mtl_name = mtl.file_name().unwrap().to_string_lossy();
            assert!(mtl_name.is_ascii());
            assert!(!mtl_name.contains(char::is_whitespace));
            assert!(mtl_name.ends_with(".mtl"));
            assert_eq!(mtl.parent(), path.parent(), "material stays next to OBJ");
        }
        assert_ne!(obj_material_path(Path::new("My Car.obj")), obj_material_path(Path::new("My_Car.obj")));

        let mesh = TriMesh { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]] };
        let built = Built {
            surface: Surface { mesh, look: Look::uniform(Mat8 { rgb: [255, 0, 0], rough: 128, metal: 0 }) },
            size_mm: [1.0, 1.0, 0.0],
            tris: 1,
            verts: 3,
            closed: false,
        };
        let options = Options { format: Format::Obj, ..Options::default() };
        for name in ["My Car.obj", "バッグ (青).obj"] {
            let files = serialize(&built, &options, Path::new(name)).expect("serialize OBJ and MTL");
            assert_eq!(files.len(), 2);
            let mtl = files[1].0.file_name().unwrap().to_string_lossy();
            let obj = String::from_utf8(files[0].1.clone()).unwrap();
            assert!(obj.contains(&format!("mtllib {mtl}\n")), "{obj}");
        }
    }

    #[test]
    fn no_export_format_may_write_invalid_mesh_indices() {
        let mesh = TriMesh { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 9]] };
        let built = Built { surface: Surface { mesh, look: Look::none() }, size_mm: [1.0, 1.0, 0.0], tris: 1, verts: 3, closed: false };
        for format in Format::ALL {
            let options = Options { format, ..Options::default() };
            assert!(serialize(&built, &options, Path::new("bad")).is_err(), "{format:?}");
        }
    }

    fn ball(r: f64) -> f64 {
        4.0 / 3.0 * std::f64::consts::PI * r * r * r
    }

    pub fn scene_of(shapes: Vec<Shape>) -> Scene {
        let mut g = Group::new("g");
        g.shapes = shapes;
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new(), anim: Default::default() }
    }

    fn sphere_at(x: f32, r: f32, m: Material) -> Shape {
        let mut s = Shape::new(Kind::Sphere, [x, 0.0, 20.0], m);
        s.size[0] = r;
        s.blend = 0.0;
        s
    }

    fn opts(format: Format, tol: f32) -> Options {
        Options { format, tolerance: tol, ..Options::default() }
    }

    fn red() -> Material {
        Material::plain([1.0, 0.0, 0.0], 0.3, 0.0)
    }
    fn blue() -> Material {
        Material::plain([0.0, 0.0, 1.0], 0.8, 1.0)
    }

    #[test]
    fn exporting_a_parent_includes_children_and_exporting_a_child_keeps_its_ancestors() {
        let mut parent = Group::new("parent");
        parent.shapes.push(sphere_at(0.0, 5.0, red()));
        let mut child = Group::new("child");
        child.depth = 1;
        child.shapes.push(sphere_at(40.0, 5.0, blue()));
        let sc = Scene { groups: vec![parent, child], ..Scene::default() };

        let both = prepare(&sc, Some(Pick::Group(0)), Scope::Selected, false).unwrap();
        assert!(Field::new(&both).distance([0.0, 0.0, 20.0]) < 0.0);
        assert!(Field::new(&both).distance([40.0, 0.0, 20.0]) < 0.0);

        let only_child = prepare(&sc, Some(Pick::Group(1)), Scope::Selected, false).unwrap();
        assert!(only_child.groups[0].visible && !only_child.groups[0].shapes[0].visible);
        assert!(Field::new(&only_child).distance([40.0, 0.0, 20.0]) < 0.0);
        assert!(Field::new(&only_child).distance([0.0, 0.0, 20.0]) > 0.0);
    }

    #[test]
    fn ignoring_paint_keeps_nested_masks_out_of_geometry() {
        let mut base = Group::new("base");
        base.shapes.push(sphere_at(0.0, 3.0, red()));
        let mut mask = Group::new("paint mask");
        mask.op = Op::Paint;
        let mut child = Group::new("paint child");
        child.depth = 1;
        child.shapes.push(sphere_at(30.0, 3.0, blue()));
        let sc = Scene { groups: vec![base, mask, child], ..Scene::default() };
        let without_stain = prepare(&sc, None, Scope::Visible, true).unwrap();
        assert_eq!(without_stain.groups.len(), 3, "paint parents must not be deleted out from under children");
        assert!(!without_stain.groups[1].visible);
        assert!(Field::new(&without_stain).distance([30.0, 0.0, 20.0]) > 0.0);
    }

    #[test]
    fn export_palette_handles_256_distinct_looks_without_a_shift_panic() {
        let mut looks = Vec::new();
        for red in [0, 64, 128, 192] {
            for green in [0, 64, 128, 192] {
                for blue in [0, 64, 128, 192] {
                    for rough in [0, 128] {
                        for metal in [0, 128] {
                            looks.push(Mat8 { rgb: [red, green, blue], rough, metal });
                        }
                    }
                }
            }
        }
        assert_eq!(looks.len(), 256);
        let (classes, n) = classify(&looks, 250);
        assert_eq!(classes.len(), looks.len());
        assert!((1..=250).contains(&n), "palette size {n}");
    }

    #[test]
    fn export_samples_gradient_colours_instead_of_treating_them_as_uniform() {
        let mut mat = red();
        mat.gradient = 1.0;
        mat.color2 = [0.0, 0.0, 1.0];
        let mut shape = Shape::new(Kind::Sphere, [0.0; 3], mat);
        shape.size[0] = 10.0;
        shape.blend = 0.0;
        let sc = scene_of(vec![shape]);
        assert!(single_look(&sc).is_none());
        let painter = Painter::new(&sc);
        let bottom = painter.look([0.0, 0.0, -9.0]).rgb;
        let top = painter.look([0.0, 0.0, 9.0]).rgb;
        assert_ne!(bottom, top, "the two ends of the sphere need different vertex colours");
    }

    #[test]
    fn paint_only_colours_the_surface_inside_its_mask() {
        let mut base = Shape::new(Kind::Box, [0.0; 3], red());
        base.size = [20.0, 20.0, 20.0];
        base.blend = 0.0;
        let mut stain = Shape::new(Kind::Sphere, [0.0, -10.0, 0.0], Material::plain([0.0, 1.0, 0.0], 0.3, 0.0));
        stain.size[0] = 3.0;
        stain.blend = 0.0;
        stain.op = Op::Paint;
        let sc = scene_of(vec![base, stain]);
        assert!(single_look(&sc).is_none());
        let painter = Painter::new(&sc);
        let inside = painter.look([0.0, -10.0, 0.0]);
        let outside = painter.look([8.0, -10.0, 0.0]);
        assert_eq!(inside.rgb, [0, 255, 0]);
        assert_eq!(outside.rgb, [255, 0, 0], "mask outside should keep the base colour");
    }

    #[test]
    fn incremental_preparation_never_changes_thin_shell_boolean_stl_export() {
        let mut wall = sphere_at(0.0, 12.0, red());
        wall.shell = 1.2; // precision-sensitive hollow wall
        let mut cut = sphere_at(8.0, 5.0, blue());
        cut.op = Op::Subtract;
        let before = scene_of(vec![wall, cut]);
        let previous = Field::new(&before);
        let mut scene = before.clone();
        scene.groups[0].shapes[1].pos[0] += 0.7;
        let incremental = Field::reusing(&scene, Some((&previous, &before)));
        let cold = Field::new(&scene);
        assert_eq!(incremental.prepared_built, 1);
        assert_eq!(incremental.prepared_reused, 1);
        for x in -15..=15 {
            for z in [8.0, 12.0, 20.0, 31.0] {
                let p = [x as f32, 0.0, z];
                assert_eq!(incremental.distance(p).to_bits(), cold.distance(p).to_bits());
            }
        }
        // Export intentionally takes an uncached, exact Field::new. Both exports must be
        // identical at the same tolerance, and the STL must round-trip every triangle.
        let options = opts(Format::Stl, 0.55);
        let output = build(&scene, None, &options, &Progress::new()).unwrap();
        let bytes = crate::export_formats::stl_binary(&output.surface.mesh, "shell-boolean");
        let faces = read_stl_binary(&bytes);
        assert_eq!(faces.len(), output.surface.mesh.triangles.len());
        assert!(!faces.is_empty());
    }

    #[test]
    fn organic_tubes_export_as_closed_meshes_in_stl_and_3mf() {
        use crate::organic_tube::Preset;
        for (preset, format) in [(Preset::Wave, Format::Stl), (Preset::Ring, Format::ThreeMf)] {
            let mut shape = preset.shape();
            if preset == Preset::Wave {
                shape.size[1] = 0.68;
                shape.size[2] = 2.0;
                shape.params[2] = 0.35;
            }
            let scene = scene_of(vec![shape]);
            let result = build(&scene, None, &opts(format, 0.75), &Progress::new())
                .unwrap_or_else(|e| panic!("{preset:?} export failed: {e}"));
            assert!(result.closed, "{preset:?} must export a closed 2-manifold surface");
            assert!(melt::is_watertight(&result.surface.mesh), "{preset:?}: open/non-manifold boundary");
            assert!(melt::volume(&result.surface.mesh) > 10.0, "{preset:?}: no filled solid");
            assert!(!result.surface.mesh.triangles.is_empty());
        }
    }

    /// Slow, high-precision reference: can run on a Windows build runner during
    /// release qualification, without slowing the normal editing test suite.
    #[test]
    #[ignore]
    fn organic_tube_high_precision_stl_is_watertight() {
        let mut shape = crate::organic_tube::Preset::S.shape();
        shape.size[1] = 0.55;
        shape.size[2] = 2.0;
        let result = build(&scene_of(vec![shape]), None, &opts(Format::Stl, 0.25), &Progress::new())
            .expect("high-precision Organic Tube export");
        assert!(result.closed && melt::is_watertight(&result.surface.mesh));
        assert!(melt::volume(&result.surface.mesh) > 10.0);
    }

    #[test]
    fn the_default_sphere_is_closed_has_the_right_volume_and_few_triangles() {
        let b = build(&Scene::default(), None, &opts(Format::Stl, 0.25), &Progress::new()).unwrap();
        assert!(b.closed);
        let v = melt::volume(&b.surface.mesh);
        assert!((v / ball(20.0) - 1.0).abs() < 0.01, "volume {v}");
        assert!(b.size_mm.iter().all(|s| (s - 40.0).abs() < 0.3), "{:?}", b.size_mm);
    }

    #[test]
    fn units_scale_and_the_up_axis_change_the_numbers() {
        let o = Options { units: Units::Inch, scale_pct: 50.0, up: Up::Y, ..opts(Format::Stl, 0.5) };
        let b = build(&Scene::default(), None, &o, &Progress::new()).unwrap();
        // 40 mm * 50 % in inches; the sphere sits 20 mm up (z), which becomes y
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in &b.surface.mesh.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        let k = 0.5 / 25.4;
        assert!(((hi[0] - lo[0]) - 40.0 * k).abs() < 0.01, "{lo:?} {hi:?}");
        assert!((lo[1] - 0.0).abs() < 0.01 && (hi[1] - 40.0 * k).abs() < 0.01, "{lo:?} {hi:?}");
        // still facing outwards after the turn
        assert!(melt::volume(&b.surface.mesh) > 0.0 && melt::is_watertight(&b.surface.mesh));
        // glTF is in metres whatever the unit
        let o = Options { units: Units::Inch, ..opts(Format::Glb, 0.5) };
        let b = build(&Scene::default(), None, &o, &Progress::new()).unwrap();
        let m = &b.surface.mesh;
        let hx = m.positions.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        assert!((hx - 0.02).abs() < 0.001, "{hx}");
    }

    #[test]
    fn a_multi_selection_exports_every_picked_shape_and_group() {
        let ball = |x: f32| {
            let mut b = Shape::new(Kind::Sphere, [x, 0.0, 0.0], Material::default());
            b.blend = 0.0;
            b
        };
        let mut sc = Scene::default();
        sc.groups.clear();
        for (i, x) in [-100.0f32, 0.0, 100.0].into_iter().enumerate() {
            let mut g = crate::model::Group::new(format!("g{i}"));
            g.shapes.push(ball(x));
            sc.groups.push(g);
        }
        let vol = |picks: Vec<Pick>| {
            let o = Options { scope: Scope::Selected, tolerance: 2.0, ..Options::default() };
            build(&sc, picks, &o, &Progress::new()).map(|b| melt::volume(&b.surface.mesh))
        };
        let one = vol(vec![Pick::Shape(0, 0)]).unwrap();
        let two = vol(vec![Pick::Shape(0, 0), Pick::Group(2)]).unwrap();
        assert!((two / one - 2.0).abs() < 0.05, "two picks give two balls: {two} vs {one}");
        let three = vol(vec![Pick::Group(0), Pick::Group(1), Pick::Shape(2, 0)]).unwrap();
        assert!((three / one - 3.0).abs() < 0.08);
        assert!(vol(vec![]).is_err());
        assert!(vol(vec![Pick::Group(0), Pick::Group(9)]).is_err());
    }

    #[test]
    fn selected_visible_and_all_pick_different_parts_and_stains_can_be_ignored() {
        let mut hidden = sphere_at(40.0, 10.0, red());
        hidden.visible = false;
        let mut stain = sphere_at(-31.0, 6.0, blue());
        stain.op = Op::Paint;
        let sc = scene_of(vec![sphere_at(-40.0, 10.0, red()), hidden, stain]);
        let vol = |scope: Scope, sel: Option<Pick>| {
            let o = Options { scope, ..opts(Format::Stl, 0.4) };
            build(&sc, sel, &o, &Progress::new()).map(|b| melt::volume(&b.surface.mesh))
        };
        let one = ball(10.0);
        assert!((vol(Scope::Visible, None).unwrap() / one - 1.0).abs() < 0.02);
        assert!((vol(Scope::All, None).unwrap() / (2.0 * one) - 1.0).abs() < 0.02);
        assert!((vol(Scope::Selected, Some(Pick::Shape(0, 1))).unwrap() / one - 1.0).abs() < 0.02);
        assert!((vol(Scope::Selected, Some(Pick::Group(0))).unwrap() / one - 1.0).abs() < 0.02);
        assert!(vol(Scope::Selected, None).is_err());
        assert!(vol(Scope::Selected, Some(Pick::Shape(7, 7))).is_err());
        // a stain changes colours, not the shape; ignoring it leaves one colour
        let o = Options { format: Format::ThreeMf, ..opts(Format::ThreeMf, 0.4) };
        let with = build(&sc, None, &o, &Progress::new()).unwrap();
        assert!(with.surface.look.palette.len() >= 2, "{:?}", with.surface.look.palette);
        let without = build(&sc, None, &Options { ignore_paint: true, ..o }, &Progress::new()).unwrap();
        assert_eq!(without.surface.look.palette.len(), 1);
    }

    #[test]
    fn patterns_and_linked_copies_are_exported_whole() {
        use crate::pattern::{Pattern, Xform};
        let mut sc = scene_of(vec![sphere_at(-60.0, 10.0, red())]);
        sc.groups[0].pattern = Some(Pattern::Rect { counts: [3, 1, 1], spacing: [60.0, 0.0, 0.0] });
        let vol = |sc: &Scene, scope: Scope, sel: Option<Pick>| {
            let o = Options { scope, ..opts(Format::Stl, 0.4) };
            build(sc, sel, &o, &Progress::new()).map(|b| melt::volume(&b.surface.mesh))
        };
        let one = ball(10.0);
        // the row of three balls is in the mesh, and in the box around it
        let (lo, hi) = bounds(&sc).unwrap();
        assert!(lo[0] < -69.0 && hi[0] > 59.0, "{lo:?} {hi:?}");
        assert!((vol(&sc, Scope::Visible, None).unwrap() / (3.0 * one) - 1.0).abs() < 0.03);
        assert!((vol(&sc, Scope::Selected, Some(Pick::Group(0))).unwrap() / (3.0 * one) - 1.0).abs() < 0.03);
        // a linked copy is one more ball (and exports by itself when it is the selection)
        let uid = sc.ensure_group_uid(0).unwrap_or(0);
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [0.0, 80.0, 0.0], ..Xform::default() };
        sc.groups.push(copy);
        assert!((vol(&sc, Scope::Visible, None).unwrap() / (4.0 * one) - 1.0).abs() < 0.03);
        assert!((vol(&sc, Scope::Selected, Some(Pick::Group(1))).unwrap() / one - 1.0).abs() < 0.03);
        // a hidden source still shows in its copy; ignoring stains does not drop the copy
        sc.groups[0].visible = false;
        assert!((vol(&sc, Scope::Visible, None).unwrap() / one - 1.0).abs() < 0.03);
        let o = Options { ignore_paint: true, ..opts(Format::Stl, 0.4) };
        assert!((build(&sc, None, &o, &Progress::new()).map(|b| melt::volume(&b.surface.mesh)).unwrap() / one - 1.0).abs() < 0.03);
        // the colours of the copies are found too
        let o = Options { format: Format::ThreeMf, ..opts(Format::ThreeMf, 0.4) };
        let b = build(&sc, None, &o, &Progress::new()).unwrap();
        assert_eq!(b.surface.look.palette.len(), 1);
    }

    #[test]
    fn colours_are_sampled_and_survive_lightening() {
        // a red ball and a blue ball far apart
        let sc = scene_of(vec![sphere_at(-25.0, 10.0, red()), sphere_at(25.0, 10.0, blue())]);
        let o = Options { lighten: Lighten::Flat, ..opts(Format::ThreeMf, 0.4) };
        let b = build(&sc, None, &o, &Progress::new()).unwrap();
        let look = &b.surface.look;
        assert_eq!(look.palette.len(), 2);
        assert_eq!(look.tri_class.len(), b.surface.mesh.triangles.len());
        let red_i = look.palette.iter().position(|m| m.rgb == [255, 0, 0]).unwrap();
        let blue_i = look.palette.iter().position(|m| m.rgb == [0, 0, 255]).unwrap();
        assert_eq!(look.palette[blue_i].metal, 255);
        // every triangle on the left is red and on the right blue
        for (i, t) in b.surface.mesh.triangles.iter().enumerate() {
            let x = (b.surface.mesh.positions[t[0] as usize][0]
                + b.surface.mesh.positions[t[1] as usize][0]
                + b.surface.mesh.positions[t[2] as usize][0])
                / 3.0;
            assert_eq!(usize::from(look.tri_class[i]), if x < 0.0 { red_i } else { blue_i });
        }
        // the 3MF carries both colours
        let files = serialize(&b, &o, Path::new("/tmp/x.3mf")).unwrap();
        let xml = String::from_utf8(unzip(&files[0].1).into_iter().find(|f| f.0.ends_with(".model")).unwrap().1).unwrap();
        assert!(xml.contains("#FF0000") && xml.contains("#0000FF"));
        // no colours asked for: nothing about them
        let b = build(&sc, None, &Options { colour: false, ..o.clone() }, &Progress::new()).unwrap();
        assert!(!b.surface.look.is_colour());
    }

    #[test]
    fn two_colours_meeting_on_a_flat_wall_keep_their_border() {
        // a red and a blue block side by side, melted into one flat slab
        let mut a = Shape::new(Kind::Box, [-10.0, 0.0, 5.0], red());
        a.size = [20.0, 20.0, 10.0];
        a.blend = 0.0;
        let mut b = Shape::new(Kind::Box, [10.0, 0.0, 5.0], blue());
        b.size = [20.0, 20.0, 10.0];
        b.blend = 0.0;
        let sc = scene_of(vec![a, b]);
        let o = Options { lighten: Lighten::Flat, ..opts(Format::ThreeMf, 0.25) };
        let built = build(&sc, None, &o, &Progress::new()).unwrap();
        let m = &built.surface.mesh;
        let look = &built.surface.look;
        assert!(built.closed);
        // on the top face, the border is at x = 0: no triangle of one colour reaches over the other's side by more than a cell
        let mut wrong = 0;
        for (i, t) in m.triangles.iter().enumerate() {
            let pts = [m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]];
            if pts.iter().any(|p| p[2] < 9.9) {
                continue;
            }
            let class = look.palette[look.tri_class[i] as usize].rgb;
            let is_red = class == [255, 0, 0];
            if pts.iter().all(|p| p[0] > 0.6) && is_red || pts.iter().all(|p| p[0] < -0.6) && !is_red {
                wrong += 1;
            }
        }
        assert_eq!(wrong, 0);
        assert!(m.triangles.len() < 5000, "{} triangles", m.triangles.len());
    }

    #[test]
    fn obj_material_reference_uses_safe_matching_filename() {
        let built = Built {
            surface: Surface { mesh: crate::export_formats::tests::tetra(10.0), look: crate::export_formats::tests::two_colours() },
            tris: 4,
            verts: 4,
            size_mm: [10.0; 3],
            closed: true,
        };
        let options = opts(Format::Obj, 0.5);
        for stem in ["My Car", "日本語 (バッグ)", "My_Car", "model"] {
            let path = Path::new("exports").join(format!("{stem}.obj"));
            let files = serialize(&built, &options, &path).expect("serialize OBJ+MTL");
            assert_eq!(files.len(), 2);
            let mtl_name = files[1].0.file_name().unwrap().to_str().unwrap();
            assert!(mtl_name.is_ascii() && !mtl_name.chars().any(char::is_whitespace));
            assert_eq!(files[0].0, path);
            let obj = String::from_utf8(files[0].1.clone()).unwrap();
            assert!(obj.lines().any(|line| line == format!("mtllib {mtl_name}")));
            assert!(String::from_utf8(files[1].1.clone()).unwrap().contains("newmtl"));
        }
        let a = serialize(&built, &options, Path::new("My Car.obj")).unwrap();
        let b = serialize(&built, &options, Path::new("My_Car.obj")).unwrap();
        assert_ne!(a[1].0, b[1].0, "escaped filenames cannot overwrite literal underscores");
    }

    #[test]
    fn every_format_writes_a_file_that_reads_back() {
        let dir = std::env::temp_dir().join(format!("yura_export_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sc = scene_of(vec![sphere_at(-25.0, 10.0, red()), sphere_at(25.0, 10.0, blue())]);
        for f in Format::ALL {
            let o = Options { lighten: Lighten::Off, ..opts(f, 1.0) };
            let path = dir.join(format!("m.{}", f.ext()));
            let job = Job::start(sc.clone(), None, o, path.clone());
            let r = loop {
                if let Some(r) = job.poll() {
                    break r;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let r = r.unwrap();
            assert!(r.closed && r.tris > 100, "{f:?}");
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(bytes.len() as u64, r.bytes - if f == Format::Obj { std::fs::metadata(dir.join("m.mtl")).unwrap().len() } else { 0 });
            match f {
                Format::Stl => assert_eq!(read_stl_binary(&bytes).len(), r.tris),
                Format::StlAscii => assert_eq!(String::from_utf8(bytes).unwrap().matches("endfacet").count(), r.tris),
                Format::Obj => {
                    let t = String::from_utf8(bytes).unwrap();
                    assert_eq!(t.lines().filter(|l| l.starts_with("f ")).count(), r.tris);
                    assert!(std::fs::read_to_string(dir.join("m.mtl")).unwrap().contains("newmtl"));
                }
                Format::ThreeMf => assert!(unzip(&bytes).iter().any(|f| f.0 == "3D/3dmodel.model")),
                Format::Ply => assert!(bytes.starts_with(b"ply\n")),
                Format::Glb => assert!(bytes.starts_with(b"glTF")),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_worker_reports_progress_and_can_be_cancelled() {
        let dir = std::env::temp_dir().join(format!("yura_export_c_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.stl");
        let job = Job::start(Scene::default(), None, opts(Format::Stl, 0.03), path.clone());
        job.cancel();
        let r = loop {
            if let Some(r) = job.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(r.unwrap_err(), export_mesh::CANCELLED);
        assert!(!path.exists());
        // a normal job ends with the bar full
        let job = Job::start(Scene::default(), None, opts(Format::Stl, 0.5), path.clone());
        let r = loop {
            if let Some(r) = job.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(r.is_ok());
        assert_eq!(job.fraction(), 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_options_never_panic() {
        let o = Options { tolerance: f32::NAN, scale_pct: f32::INFINITY, smooth: 200, lighten: Lighten::Target(0), ..Options::default() };
        let s = o.sanitized();
        assert!(s.tolerance.is_finite() && s.scale_pct.is_finite() && s.smooth <= 4);
        assert!(build(&Scene::default(), None, &o, &Progress::new()).is_ok());
        let tiny = Options { tolerance: 0.0, ..Options::default() };
        assert_eq!(tiny.sanitized().tolerance, MIN_TOLERANCE);
        // an empty scene
        let empty =
            Scene { groups: vec![Group::new("e")], melt: Default::default(), env: Default::default(), saved: Vec::new(), anim: Default::default() };
        assert!(build(&empty, None, &Options::default(), &Progress::new()).is_err());
        assert!(build(&Scene { groups: vec![], ..empty.clone() }, None, &Options::default(), &Progress::new()).is_err());
        // a shape with broken numbers
        let mut bad = Shape::new(Kind::Sphere, [f32::NAN, 0.0, 0.0], Material::default());
        bad.size[0] = f32::INFINITY;
        let sc = scene_of(vec![bad]);
        let _ = build(&sc, None, &Options::default(), &Progress::new());
        // a model far too big for the asked precision is refused, not attempted
        let mut huge = Shape::new(Kind::Sphere, [0.0, 0.0, 0.0], Material::default());
        huge.size[0] = 100_000.0;
        let r = build(&scene_of(vec![huge]), None, &Options { tolerance: 0.01, ..Options::default() }, &Progress::new());
        assert!(r.is_err());
    }

    #[test]
    fn the_print_check_measures_a_ball_a_thin_plate_and_an_overhang() {
        let c = print_check(&Scene::default(), None, &Options { tolerance: 0.3, ..Options::default() }, &Progress::new()).unwrap();
        assert!(c.closed);
        assert!(c.size.iter().all(|s| (s - 40.0).abs() < 1.0), "{:?}", c.size);
        assert!((c.volume_cm3 - 33.5).abs() < 0.8, "{}", c.volume_cm3);
        assert!(c.thin_pct < 1.0, "{}", c.thin_pct);
        // the cap of a ball that leans out more than 45 degrees is (1 - cos 45) / 2 of its surface
        assert!((c.overhang_pct - 14.6).abs() < 1.0, "{}", c.overhang_pct);
        // a 0.5 mm plate is thin everywhere
        let mut plate = Shape::new(Kind::Box, [0.0, 0.0, 0.25], Material::default());
        plate.size = [30.0, 30.0, 0.5];
        plate.blend = 0.0;
        let c = print_check(&scene_of(vec![plate]), None, &Options { tolerance: 0.1, ..Options::default() }, &Progress::new()).unwrap();
        assert!(c.thin_pct > 80.0, "{}", c.thin_pct);
        assert!(c.thinnest.is_some_and(|t| t < 0.8));
        assert!(c.overhang_pct < 1.0, "the bottom lies on the bed: {}", c.overhang_pct);
        // a block is not thin and does not overhang
        let mut block = Shape::new(Kind::Box, [0.0, 0.0, 10.0], Material::default());
        block.size = [20.0, 20.0, 20.0];
        block.blend = 0.0;
        let c = print_check(&scene_of(vec![block]), None, &Options { tolerance: 0.2, ..Options::default() }, &Progress::new()).unwrap();
        assert!(c.thin_pct < 2.0 && c.overhang_pct < 1.0, "{} {}", c.thin_pct, c.overhang_pct);
    }

    #[test]
    fn print_check_measures_the_physical_wall_after_export_scale() {
        let mut plate = Shape::new(Kind::Box, [0.0, 0.0, 1.0], Material::default());
        plate.size = [20.0, 20.0, 2.0];
        plate.blend = 0.0;
        let sc = scene_of(vec![plate]);
        let opts = Options { scale_pct: 10.0, tolerance: 0.15, ..Options::default() };
        let check = print_check(&sc, None, &opts, &Progress::new()).unwrap();
        assert!((check.size[0] - 2.0).abs() < 0.1, "{:?}", check.size);
        assert!((check.size[2] - 0.2).abs() < 0.1, "{:?}", check.size);
        assert!(check.thin_pct > 5.0, "a scaled 0.2 mm wall must be thin: {:?}", check.thin_pct);
        assert!(check.thinnest.is_some_and(|t| t < THIN_MM), "{:?}", check.thinnest);
    }

    #[test]
    fn the_estimate_is_in_the_right_range() {
        let e = estimate(&Scene::default(), None, &Options::default(), &Progress::new()).unwrap();
        let area = 4.0 * std::f64::consts::PI * 400.0;
        assert!((e.area / area - 1.0).abs() < 0.1, "{}", e.area);
        let b = build(&Scene::default(), None, &Options { lighten: Lighten::Off, tolerance: 0.25, ..Options::default() }, &Progress::new()).unwrap();
        let guess = e.raw_tris(0.25);
        assert!(guess > b.tris as f64 * 0.6 && guess < b.tris as f64 * 1.6, "guess {guess} vs {}", b.tris);
        assert!(e.secs(0.1, false) > e.secs(0.5, false) && e.secs(0.1, true) > e.secs(0.1, false));
    }

    /// Timings on a few real models: `cargo test -p meltcad --release bench_models -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_models() {
        let mut scenes: Vec<(String, Scene)> = vec![("default sphere".into(), Scene::default())];
        let mut big = Shape::new(Kind::Box, [0.0, 0.0, 50.0], Material::default());
        big.size = [100.0, 100.0, 100.0];
        big.blend = 0.0;
        scenes.push(("100 mm box".into(), scene_of(vec![big])));
        let mut round = Shape::new(Kind::Sphere, [0.0, 0.0, 100.0], Material::default());
        round.size[0] = 100.0;
        round.blend = 0.0;
        scenes.push(("r100 sphere".into(), scene_of(vec![round])));
        for (ci, ii) in [(0usize, 0usize), (7, 0), (10, 0), (10, 5), (8, 0), (2, 1)] {
            if let Some(item) = crate::library::all().get(ci).and_then(|c| c.items.get(ii)) {
                let sc = Scene { groups: vec![item.group.clone()], ..Scene::default() };
                scenes.push((format!("lib {} / {}", crate::library::all()[ci].name, item.name), sc));
            }
        }
        let only = std::env::var("YURA_BENCH").unwrap_or_default();
        for (name, sc) in &scenes {
            if !only.is_empty() && !name.contains(&only) {
                continue;
            }
            let tols: Vec<f32> = std::env::var("YURA_BENCH_TOL")
                .ok()
                .map(|t| t.split(',').filter_map(|x| x.parse().ok()).collect())
                .unwrap_or_else(|| vec![0.2, 0.1, 0.05]);
            for tol in tols {
                for (lighten, tag) in [(Lighten::Off, "raw "), (Lighten::Flat, "flat")] {
                    if std::env::var_os("YURA_BENCH_FLAT_ONLY").is_some() && lighten == Lighten::Off {
                        continue;
                    }
                    let o = Options { tolerance: tol, lighten, format: Format::ThreeMf, ..Options::default() };
                    let prog = Progress::new();
                    let t0 = Instant::now();
                    match build(sc, None, &o, &prog) {
                        Ok(b) => println!(
                            "{name:40} tol {tol:5} {tag}: {:8.2}s  {:9} tris  closed {}  size {:.0}x{:.0}x{:.0}",
                            t0.elapsed().as_secs_f64(),
                            b.tris,
                            b.closed,
                            b.size_mm[0],
                            b.size_mm[1],
                            b.size_mm[2]
                        ),
                        Err(e) => println!("{name:40} tol {tol:5} {tag}: error {e} after {:.2}s", t0.elapsed().as_secs_f64()),
                    }
                }
            }
        }
    }
}
