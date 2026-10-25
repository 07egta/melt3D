//! The scene: groups of shapes that are added to or carved out of each other with a soft blend.
//! Everything is millimetres, Z is up, angles are degrees.

use serde::{Deserialize, Serialize};

use crate::mesh::SharedVolume;
use crate::pattern::{Pattern, Xform};

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Sphere,
    Box,
    Cylinder,
    Torus,
    Cone,
    /// A 2D outline (`Shape::outline`) pushed straight up.
    Extrude,
    // ---- more primitives (ids 6..15, `prims.rs` / `shader_prims.wgsl`)
    /// Triangular prism: `size` = circumradius, height.
    Prism3,
    /// Prism with `params[0]` sides: `size` = circumradius, height.
    PrismN,
    Octa,
    Tetra,
    /// Pyramid with `params[0]` sides: `size` = base circumradius, height.
    Pyramid,
    /// `size` = radius, straight length between the two ball centres.
    Capsule,
    /// Dome on the XY plane's upper side: `size` = radius.
    Hemi,
    /// Pipe: `size` = outer radius, wall thickness, height.
    Tube,
    Wedge,
    /// Star prism: `size` = outer radius, height; `params` = points, inner ratio.
    Star,
    // ---- CAD features (ids 16.., `cad.rs` / `shader_cad.wgsl`)
    /// `outline` is a profile (x = distance from the Z axis, y = height) turned about Z.
    Revolve,
    /// `outline` at the bottom blends into `outline2` at the top over `size[0]` mm.
    Loft,
    /// A tube through the `path` nodes (x, y, z, radius).
    Curve,
    /// `outline` pulled along `path`.
    Sweep,
    /// An imported mesh: a baked signed distance volume (`Shape::volume`, `mesh.rs`). It needs a
    /// file, so it is not in `Kind::ALL` or the add menus; the import card makes it.
    Mesh,
}

impl Kind {
    pub const ALL: [Kind; 20] = [
        Kind::Sphere,
        Kind::Box,
        Kind::Cylinder,
        Kind::Torus,
        Kind::Cone,
        Kind::Extrude,
        Kind::Prism3,
        Kind::PrismN,
        Kind::Octa,
        Kind::Tetra,
        Kind::Pyramid,
        Kind::Capsule,
        Kind::Hemi,
        Kind::Tube,
        Kind::Wedge,
        Kind::Star,
        Kind::Revolve,
        Kind::Loft,
        Kind::Curve,
        Kind::Sweep,
    ];
    /// The six always shown on the top bar.
    pub const MAIN: [Kind; 5] = [Kind::Sphere, Kind::Box, Kind::Cylinder, Kind::Torus, Kind::Cone];
    /// The extra solids in the "more" menu.
    pub const MORE: [Kind; 10] =
        [Kind::Prism3, Kind::PrismN, Kind::Octa, Kind::Tetra, Kind::Pyramid, Kind::Capsule, Kind::Hemi, Kind::Tube, Kind::Wedge, Kind::Star];
    /// CAD-style features (profile based).
    pub const CAD: [Kind; 4] = [Kind::Revolve, Kind::Loft, Kind::Curve, Kind::Sweep];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sphere => "Sphere",
            Kind::Box => "Box",
            Kind::Cylinder => "Cylinder",
            Kind::Torus => "Torus",
            Kind::Cone => "Cone",
            Kind::Extrude => "Extrude",
            Kind::Prism3 => "Prism",
            Kind::PrismN => "Polygon Prism",
            Kind::Octa => "Octahedron",
            Kind::Tetra => "Tetrahedron",
            Kind::Pyramid => "Pyramid",
            Kind::Capsule => "Capsule",
            Kind::Hemi => "Hemisphere",
            Kind::Tube => "Pipe",
            Kind::Wedge => "Wedge",
            Kind::Star => "Star Prism",
            Kind::Revolve => "Revolve",
            Kind::Loft => "Loft",
            Kind::Curve => "Curve",
            Kind::Sweep => "Sweep",
            Kind::Mesh => "Mesh",
        }
    }
    pub fn id(self) -> f32 {
        match self {
            Kind::Sphere => 0.0,
            Kind::Box => 1.0,
            Kind::Cylinder => 2.0,
            Kind::Torus => 3.0,
            Kind::Cone => 4.0,
            Kind::Extrude => 5.0,
            Kind::Prism3 => 6.0,
            Kind::PrismN => 7.0,
            Kind::Octa => 8.0,
            Kind::Tetra => 9.0,
            Kind::Pyramid => 10.0,
            Kind::Capsule => 11.0,
            Kind::Hemi => 12.0,
            Kind::Tube => 13.0,
            Kind::Wedge => 14.0,
            Kind::Star => 15.0,
            Kind::Revolve => 16.0,
            Kind::Loft => 17.0,
            Kind::Curve => 18.0,
            Kind::Sweep => 19.0,
            Kind::Mesh => 20.0,
        }
    }
    /// Does `Shape::round` change this kind? A sphere, a capsule, a torus and a curve are round
    /// all over already, so they ignore it.
    pub fn can_round(self) -> bool {
        !matches!(self, Kind::Sphere | Kind::Capsule | Kind::Torus | Kind::Curve | Kind::Mesh)
    }
    /// Does the shape read `Shape::outline` (a 2D profile)?
    pub fn uses_outline(self) -> bool {
        matches!(self, Kind::Extrude | Kind::Revolve | Kind::Loft | Kind::Sweep)
    }
    /// Does the shape read `Shape::outline2` (the loft's top profile)?
    pub fn uses_outline2(self) -> bool {
        self == Kind::Loft
    }
    /// Does the shape read `Shape::path` (3D nodes)?
    pub fn uses_path(self) -> bool {
        matches!(self, Kind::Curve | Kind::Sweep)
    }
    /// What the size numbers mean, for the inspector.
    pub fn size_labels(self) -> [&'static str; 3] {
        match self {
            Kind::Sphere => ["Radius", "", ""],
            Kind::Box => ["Width", "Depth", "Height"],
            Kind::Cylinder => ["Radius", "Height", ""],
            Kind::Torus => ["Major radius", "Minor radius", ""],
            Kind::Cone => ["Radius", "Height", ""],
            Kind::Extrude => ["Height", "", ""],
            Kind::Prism3 | Kind::PrismN | Kind::Star => ["Outer radius", "Height", ""],
            Kind::Octa | Kind::Tetra | Kind::Hemi => ["Radius", "", ""],
            Kind::Pyramid => ["Base radius", "Height", ""],
            Kind::Capsule => ["Radius", "Length", ""],
            Kind::Tube => ["Outer radius", "Wall", "Height"],
            Kind::Wedge => ["Width", "Depth", "Height"],
            Kind::Revolve => ["", "", ""],
            Kind::Loft => ["Height", "", ""],
            Kind::Curve => ["", "", ""],
            Kind::Sweep => ["", "", ""],
            // (the size is the model's size in mm, shown by the mesh inspector, not edited)
            Kind::Mesh => ["", "", ""],
        }
    }
    pub fn default_size(self) -> [f32; 3] {
        match self {
            Kind::Sphere => [20.0, 1.0, 1.0],
            Kind::Box => [30.0, 30.0, 30.0],
            Kind::Cylinder => [15.0, 40.0, 1.0],
            Kind::Torus => [20.0, 7.0, 1.0],
            Kind::Cone => [18.0, 40.0, 1.0],
            Kind::Extrude => [20.0, 1.0, 1.0],
            Kind::Prism3 => [22.0, 40.0, 1.0],
            Kind::PrismN => [20.0, 40.0, 1.0],
            Kind::Octa => [25.0, 1.0, 1.0],
            Kind::Tetra => [25.0, 1.0, 1.0],
            Kind::Pyramid => [22.0, 40.0, 1.0],
            Kind::Capsule => [10.0, 30.0, 1.0],
            Kind::Hemi => [22.0, 1.0, 1.0],
            Kind::Tube => [18.0, 4.0, 40.0],
            Kind::Wedge => [30.0, 30.0, 30.0],
            Kind::Star => [24.0, 14.0, 1.0],
            Kind::Revolve => [1.0, 1.0, 1.0],
            Kind::Loft => [40.0, 1.0, 1.0],
            Kind::Curve => [1.0, 1.0, 1.0],
            Kind::Sweep => [1.0, 1.0, 1.0],
            Kind::Mesh => [1.0, 1.0, 1.0],
        }
    }
    /// Labels of the four extra numbers (`Shape::params`); empty = unused.
    pub fn param_labels(self) -> [&'static str; 4] {
        match self {
            Kind::PrismN => ["Sides", "", "", ""],
            Kind::Pyramid => ["Sides", "", "", ""],
            Kind::Star => ["Points", "Inner ratio", "", ""],
            Kind::Revolve => ["Angle", "", "", ""],
            _ => ["", "", "", ""],
        }
    }
    pub fn default_params(self) -> [f32; 4] {
        match self {
            Kind::PrismN => [6.0, 0.0, 0.0, 0.0],
            Kind::Pyramid => [4.0, 0.0, 0.0, 0.0],
            Kind::Star => [5.0, 0.45, 0.0, 0.0],
            Kind::Revolve => [360.0, 0.0, 0.0, 0.0],
            Kind::Curve => [0.0, 0.7, 0.0, 1.0],
            _ => [0.0; 4],
        }
    }
}

/// Added to the group, carved out of it, or only painted on it (a stain: colour, no shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Add,
    Subtract,
    Paint,
    /// Keep only what overlaps the shapes before it.
    Intersect,
}

impl Op {
    /// What the shader reads: +1 add, -1 carve, 0 paint, 2 intersect.
    pub fn sign(self) -> f32 {
        match self {
            Op::Add => 1.0,
            Op::Subtract => -1.0,
            Op::Paint => 0.0,
            Op::Intersect => 2.0,
        }
    }
    /// Plus becomes minus, minus becomes plus (a stain or an overlap becomes plus).
    pub fn flip(self) -> Op {
        match self {
            Op::Add => Op::Subtract,
            Op::Subtract | Op::Paint | Op::Intersect => Op::Add,
        }
    }
    /// Add, then carve, then paint, then overlap, then add again (the list's +/- button).
    pub fn next(self) -> Op {
        match self {
            Op::Add => Op::Subtract,
            Op::Subtract => Op::Paint,
            Op::Paint => Op::Intersect,
            Op::Intersect => Op::Add,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub color: [f32; 3],
    /// 0 = mirror smooth, 1 = chalky.
    pub roughness: f32,
    /// 0 = plastic, 1 = metal.
    pub metallic: f32,
    /// How much light it gives off (0 = none, 4 = a lamp).
    pub glow: f32,
    /// 0 = solid, 1 = clear glass that bends the light.
    pub glass: f32,
    /// A shifting rainbow sheen (soap bubble, beetle shell).
    pub iridescence: f32,
    /// A glossy clear layer on top (car paint).
    pub coat: f32,
    /// Light seeping through (wax, jelly, ice): soft edges and a glow from behind.
    pub translucency: f32,
    /// Gradient: the second colour the surface blends into (`color` is the start).
    pub color2: [f32; 3],
    /// How much of the gradient shows: 0 = off (one plain colour), 1 = all the way to `color2`.
    pub gradient: f32,
    /// The direction of the gradient: 0 = left to right (X), 1 = front to back (Y), 2 = bottom to top (Z).
    pub grad_axis: u8,
    /// Run the gradient the other way (`color2` at the start).
    pub grad_flip: bool,
    /// Follow the shape's own box (turns and stretches with it) instead of the world's axes.
    pub grad_local: bool,
    /// 0 = a hard edge between the two colours, 1 = a long smooth blend.
    pub grad_soft: f32,
    /// Where the blend starts and ends, as fractions (0..1) of the shape's size along the direction.
    pub grad_from: f32,
    pub grad_to: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material::plain([0.92, 0.55, 0.42], 0.45, 0.0)
    }
}

impl Material {
    pub const fn plain(color: [f32; 3], roughness: f32, metallic: f32) -> Material {
        Material {
            color,
            roughness,
            metallic,
            glow: 0.0,
            glass: 0.0,
            iridescence: 0.0,
            coat: 0.0,
            translucency: 0.0,
            color2: [0.36, 0.52, 0.98],
            gradient: 0.0,
            grad_axis: 2,
            grad_flip: false,
            grad_local: false,
            grad_soft: 0.7,
            grad_from: 0.0,
            grad_to: 1.0,
        }
    }
    /// Is a gradient switched on?
    pub fn has_gradient(&self) -> bool {
        self.gradient > 0.001
    }
    /// How much of `color2` shows at `t`, the place along the gradient (0 = start .. 1 = end of the
    /// shape's size; the flip is already in `t`). The range `grad_from..grad_to` is stretched over
    /// 0..1 and `grad_soft` decides how wide the blend is around its middle. The GPU copy is
    /// `shape_color` in `shader.wgsl`.
    // max/min deliberately give the shader-compatible zero weight for NaN;
    // f32::clamp would propagate it into the material and rendered pixels.
    #[allow(clippy::manual_clamp)]
    pub fn gradient_weight(&self, t: f32) -> f32 {
        let span = (self.grad_to - self.grad_from).max(0.02);
        let u = ((t - self.grad_from) / span).max(0.0).min(1.0);
        let w = (self.grad_soft * 0.5).max(0.001);
        let k = ((u - (0.5 - w)) / (2.0 * w)).max(0.0).min(1.0);
        k * k * (3.0 - 2.0 * k) * self.gradient
    }
    /// Keep every number in the range the renderer expects.
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        for c in &mut self.color {
            fix(c, 0.0, 1.0, 0.8);
        }
        fix(&mut self.roughness, 0.0, 1.0, 0.5);
        fix(&mut self.metallic, 0.0, 1.0, 0.0);
        fix(&mut self.glow, 0.0, 4.0, 0.0);
        fix(&mut self.glass, 0.0, 1.0, 0.0);
        fix(&mut self.iridescence, 0.0, 1.0, 0.0);
        fix(&mut self.coat, 0.0, 1.0, 0.0);
        fix(&mut self.translucency, 0.0, 1.0, 0.0);
        for c in &mut self.color2 {
            fix(c, 0.0, 1.0, 0.8);
        }
        fix(&mut self.gradient, 0.0, 1.0, 0.0);
        self.grad_axis = self.grad_axis.min(2);
        fix(&mut self.grad_soft, 0.0, 1.0, 0.7);
        fix(&mut self.grad_from, 0.0, 1.0, 0.0);
        fix(&mut self.grad_to, 0.0, 1.0, 1.0);
    }
}

/// Optional pointer to the original imported mesh. The vertices are not embedded in the scene:
/// a missing/moved source file must be selected again before re-baking.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshSource {
    pub path: String,
    /// Whether the original mesh was converted from Y-up to Z-up at import.
    pub y_up: bool,
    /// Fingerprint of cleaned vertices/triangles; zero denotes an older unknown source.
    #[serde(default)]
    pub mesh_hash: u64,
}

/// A node on a tube curve can retain the historical Catmull-Rom behaviour, have
/// coupled C1-continuous Bezier handles, or independent handles for a sharp corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveNodeMode {
    #[default]
    Auto,
    Smooth,
    Corner,
}

/// Handle offsets from the node centre, in its local XYZ coordinates (millimetres).
/// These are true cubic Bezier control handles, not interpolated polyline points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CurveHandles {
    #[serde(default)]
    pub mode: CurveNodeMode,
    #[serde(default)]
    pub incoming: [f32; 3],
    #[serde(default)]
    pub outgoing: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub name: String,
    pub kind: Kind,
    pub size: [f32; 3],
    /// Rounding of the corners and edges of every kind that has them (`Kind::can_round`), in mm.
    /// It never makes a shape bigger, and is held to what the shape can take.
    pub round: f32,
    pub pos: [f32; 3],
    /// Euler angles about X, Y, Z (degrees).
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub op: Op,
    /// How far this shape melts into the rest of its group, in mm.
    pub blend: f32,
    pub material: Material,
    pub visible: bool,
    /// A sketch line: drawn on the screen as a line, never part of the solid (not rendered, not
    /// exported, not measured). Extrude, Loft or Revolve turn it into a solid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub guide: bool,
    /// Original sketch axes in the shape's local space. Keeping the origin and axes
    /// makes a saved sketch usable for Revolve without moving its axis to its centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_plane: Option<GuidePlane>,
    /// Corners of an `Extrude` outline, in the shape's own XY plane.
    pub outline: Vec<[f32; 2]>,
    /// Kind-specific extra numbers (`Kind::param_labels`).
    #[serde(default)]
    pub params: [f32; 4],
    /// Nodes (x, y, z, radius) of a `Curve` or `Sweep`, in the shape's own space.
    #[serde(default)]
    pub path: Vec<[f32; 4]>,
    /// Optional, index-aligned handles for Curve nodes. Empty in old files: their
    /// exact Catmull-Rom interpolation is preserved when no handle is authored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve_handles: Vec<CurveHandles>,
    /// The top profile of a `Loft`.
    #[serde(default)]
    pub outline2: Vec<[f32; 2]>,
    /// Wall thickness in mm that hollows out any kind (0 = solid).
    #[serde(default)]
    pub shell: f32,
    /// How `round` treats the edges: a round fillet (default) or a flat 45-degree bevel.
    #[serde(default)]
    pub corner: Corner,
    /// A number patterns and linked copies find this shape by (0 = none; see `Scene::ensure_shape_uid`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub uid: u32,
    /// The baked distance volume of a `Kind::Mesh` shape, in the shape's own space. Shapes copied
    /// from each other share it (it is never copied), and scenes compare it by pointer.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::mesh::lenient_volume")]
    pub volume: Option<SharedVolume>,
    /// Sketch outlines remain vertex arrays. Precision dimensions are applied directly to these
    /// arrays, so existing saves and Extrude/Revolve/Loft/Sweep readers need no schema migration.
    /// Where an imported mesh came from (its file name), shown in the inspector.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Parametric vase controls; ordinary Revolve sketches remain unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vase: Option<crate::organic_vase::VaseParams>,
    /// Optional source reference, absent in all legacy .meltcad files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_source: Option<MeshSource>,
    /// Melt only one face: the rest of the shape keeps its exact size.
    #[serde(default, skip_serializing_if = "FaceMelt::is_off")]
    pub face_melt: FaceMelt,
    /// A solid made in SolidCraft carries its design, so it can be opened there again and edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solid: Option<SolidLink>,
}

/// The SolidCraft design behind a shape that came from SolidCraft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolidLink {
    /// The design file's text (SolidCraft's JSON form).
    pub design: String,
    /// Where the shape stood when it came in: an edit moves the new solid by what the shape has
    /// been moved since, so it stays where it was put.
    pub placed_at: [f32; 3],
}

/// The largest SolidCraft design a shape keeps (bigger ones still come in, just not editable).
pub const MAX_SOLID_DESIGN: usize = 16 * 1024 * 1024;

/// A saved sketch's two plane axes and inner contours. The outer contour remains
/// in `Shape::path`, so moving or editing its nodes does not leave a stale copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuidePlane {
    pub u: [f32; 3],
    pub v: [f32; 3],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<[f32; 2]>>,
}

/// A melt that touches just one face of a shape (and a little way into it).
/// `face`: 0 = none, 1..6 = +x -x +y -y +z -z in the shape's own axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FaceMelt {
    #[serde(default)]
    pub face: u8,
    /// How far the melted face swells out (+) or sinks in (-), in mm.
    #[serde(default)]
    pub amount: f32,
    /// How deep the melt reaches from the face, in mm.
    #[serde(default)]
    pub reach: f32,
}

impl FaceMelt {
    pub const FACES: [(&'static str, &'static str); 6] =
        [("+X", "+x face"), ("-X", "-x face"), ("+Y", "+y face"), ("-Y", "-y face"), ("+Z", "+z face"), ("-Z", "-z face")];
    pub fn is_off(&self) -> bool {
        self.face == 0 || self.face > 6 || !self.amount.is_finite() || self.amount == 0.0
    }
    /// The values the renderers use: (axis, +1/-1, amount, reach); nothing if the melt is off.
    pub fn parts(&self) -> Option<(usize, f32, f32, f32)> {
        if self.is_off() || !self.reach.is_finite() {
            return None;
        }
        let i = usize::from(self.face - 1);
        Some((i / 2, if i % 2 == 0 { 1.0 } else { -1.0 }, self.amount.clamp(-60.0, 60.0), self.reach.clamp(1.0, 200.0)))
    }
}

/// The style of a shape's cut edges: what `Shape::round` makes of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corner {
    /// A flat bevel (chamfer): the corner is cut off straight, `round` mm along each side.
    Chamfer,
    /// A rounded edge (fillet): the shape shrunk by `round` and grown again with a ball. (Also what
    /// an unknown name in a file becomes.)
    #[default]
    #[serde(other)]
    Round,
}

impl Corner {
    pub const ALL: [Corner; 2] = [Corner::Round, Corner::Chamfer];
}

pub const MAX_OUTLINE: usize = 64;
/// The largest rounding the inspector offers (mm); files may hold up to `MAX_ROUND`.
pub const ROUND_MAX: f32 = 40.0;
pub const MAX_ROUND: f32 = 500.0;
/// Most nodes of a curve or sweep path (a traced pipe uses up to `stroke::PIPE_NODES` of them).
pub const MAX_PATH: usize = 48;

impl Shape {
    /// Shown and part of the solid (a hidden shape or a sketch line is not).
    pub fn solid(&self) -> bool {
        self.visible && !self.guide
    }

    pub fn new(kind: Kind, pos: [f32; 3], material: Material) -> Shape {
        Shape {
            name: kind.label().to_string(),
            kind,
            size: kind.default_size(),
            round: 0.0,
            pos,
            rot: [0.0; 3],
            scale: [1.0; 3],
            op: Op::Add,
            blend: 6.0,
            material,
            visible: true,
            guide: false,
            guide_plane: None,
            outline: kind.default_outline(),
            params: kind.default_params(),
            path: kind.default_path(),
            curve_handles: Vec::new(),
            outline2: kind.default_outline2(),
            shell: 0.0,
            corner: Corner::Round,
            uid: 0,
            volume: None,
            source: String::new(),
            mesh_source: None,
            face_melt: FaceMelt::default(),
            solid: None,
            vase: None,
        }
    }

    /// Grid widths in actual model millimetres after nonuniform scaling.
    /// A stored SDF has one cell size; on screen it may have three different pitches.
    #[cfg(test)]
    pub fn mesh_cell_mm(&self) -> Option<[f32; 3]> {
        let v = self.volume.as_ref()?;
        (self.kind == Kind::Mesh).then(|| [0, 1, 2].map(|a| v.cell * self.scale[a].abs()))
    }

    /// An imported mesh: `volume` is its baked distances, `model_size` its size in mm.
    pub fn mesh(volume: SharedVolume, name: &str, source: &str, model_size: [f32; 3], pos: [f32; 3], material: Material) -> Shape {
        let mut s = Shape::new(Kind::Mesh, pos, material);
        s.name = name.to_string();
        s.source = source.to_string();
        s.size = model_size;
        s.volume = Some(volume);
        s
    }

    /// Half the size of the shape's box in its own axes, before scaling. Rough for the fancy kinds
    /// (used for framing, picking and the gizmo).
    pub fn local_half(&self) -> [f32; 3] {
        let s = self.size;
        let mx = |v: &[[f32; 2]], i: usize| v.iter().map(|p| p[i].abs()).fold(1.0_f32, f32::max);
        match self.kind {
            Kind::Sphere => [s[0]; 3],
            Kind::Box | Kind::Wedge => [s[0] * 0.5, s[1] * 0.5, s[2] * 0.5],
            Kind::Cylinder | Kind::Cone => [s[0], s[0], s[1] * 0.5],
            Kind::Torus => [s[0] + s[1], s[0] + s[1], s[1]],
            Kind::Extrude => [mx(&self.outline, 0), mx(&self.outline, 1), s[0] * 0.5],
            Kind::Prism3 | Kind::PrismN | Kind::Star | Kind::Pyramid => [s[0], s[0], s[1] * 0.5],
            Kind::Octa | Kind::Tetra => [s[0]; 3],
            // the dome fills z in -r/2 ..= r/2 (flat face at the bottom of its box)
            Kind::Hemi => [s[0], s[0], s[0] * 0.5],
            Kind::Capsule => [s[0], s[0], s[1] * 0.5 + s[0]],
            Kind::Tube => [s[0], s[0], s[2] * 0.5],
            Kind::Revolve => {
                // The angular waves can extend past the 2D profile: include their full
                // amplitude in the bounds, otherwise culling could clip the petals.
                let wave = if self.vase.is_some() { self.params[2].abs().min(6.0) } else { 0.0 };
                let r = mx(&self.outline, 0) + wave;
                [r, r, mx(&self.outline, 1)]
            }
            Kind::Loft => {
                let x = mx(&self.outline, 0).max(mx(&self.outline2, 0));
                let y = mx(&self.outline, 1).max(mx(&self.outline2, 1));
                [x, y, s[0] * 0.5]
            }
            Kind::Curve => crate::curve::half_extent_shape(self),
            // the box of the volume's nodes as seen from the shape's origin (the origin need not be its middle)
            Kind::Mesh => match &self.volume {
                Some(v) => {
                    let (lo, hi) = v.bounds();
                    [0, 1, 2].map(|a| lo[a].abs().max(hi[a].abs()).max(0.5))
                }
                None => [s[0] * 0.5, s[1] * 0.5, s[2] * 0.5],
            },
            Kind::Sweep => {
                let prof = mx(&self.outline, 0).max(mx(&self.outline, 1)) * 1.5;
                let mut h = [1.0_f32; 3];
                for p in &self.path {
                    let r = if self.kind == Kind::Curve { p[3].abs() } else { prof * p[3].abs().max(1.0) };
                    for i in 0..3 {
                        h[i] = h[i].max(p[i].abs() + r);
                    }
                }
                h
            }
        }
    }
    /// Atomically replace a sketch-driven outline. Validate before touching the model:
    /// invalid drags must never enter the 3D renderer, undo history or saved file.
    /// The coordinates are already in the shape's local plane, so position, rotation,
    /// height, material and the other loft profile remain unchanged.
    pub fn set_sketch_outline(&mut self, top: bool, points: Vec<[f32; 2]>) -> Result<bool, String> {
        if !self.kind.uses_outline() || (top && !self.kind.uses_outline2()) {
            return Err("この形には編集できる輪かくがありません".to_string());
        }
        if points.iter().any(|p| p.iter().any(|v| !v.is_finite() || v.abs() > crate::sketch::COORD_MAX)) {
            return Err("輪かくの座標は±2000mm以内にしてください".to_string());
        }
        let checked = crate::sketch::validate_profile(&points).map_err(|e| e.message())?;
        if checked.len() != points.len() {
            return Err(crate::sketch::SketchError::TooSmall.message());
        }
        if self.kind == Kind::Revolve && points.iter().any(|p| p[0] < 0.0) {
            return Err(crate::sketch::SketchError::LeftOfAxis.message());
        }
        // Keep the original vertex order, so selection indices do not jump if a
        // legacy model happens to have clockwise winding.
        let original = if top { &mut self.outline2 } else { &mut self.outline };
        if *original == points {
            return Ok(false);
        }
        *original = points;
        Ok(true)
    }
}

impl Kind {
    /// The starting profile of kinds that use one.
    pub fn default_outline(self) -> Vec<[f32; 2]> {
        match self {
            // half a rounded vase: x = radius, y = height
            Kind::Revolve => vec![[0.0, -20.0], [16.0, -20.0], [20.0, -6.0], [12.0, 8.0], [9.0, 20.0], [0.0, 20.0]],
            Kind::Sweep => vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]],
            _ => vec![[-15.0, -15.0], [15.0, -15.0], [15.0, 15.0], [-15.0, 15.0]],
        }
    }
    pub fn default_outline2(self) -> Vec<[f32; 2]> {
        match self {
            // a triangle on top of the square
            Kind::Loft => vec![[-10.0, -8.0], [10.0, -8.0], [0.0, 12.0]],
            _ => Vec::new(),
        }
    }
    pub fn default_path(self) -> Vec<[f32; 4]> {
        match self {
            Kind::Curve => vec![[-30.0, 0.0, 0.0, 6.0], [0.0, 0.0, 22.0, 6.0], [30.0, 0.0, 0.0, 6.0]],
            Kind::Sweep => vec![[-30.0, 0.0, 0.0, 1.0], [0.0, 0.0, 25.0, 1.0], [30.0, 0.0, 0.0, 1.0]],
            _ => Vec::new(),
        }
    }
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// A saved blend amount made safe: negative and non-finite values melt nothing.
fn clean_blend(blend: f32) -> f32 {
    if blend.is_finite() { blend.max(0.0) } else { 0.0 }
}

/// The deepest a group can be nested.
/// Two levels are enough: a big group holds groups, and groups hold shapes. Groups melt into each
/// other inside the big group, and the big group melts with the rest as one piece.
pub const MAX_DEPTH: u8 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub op: Op,
    /// How far the whole group melts into the groups before it, in mm.
    pub blend: f32,
    pub visible: bool,
    pub shapes: Vec<Shape>,
    /// How deep the group sits in the outline: it is part of the nearest group above it with a smaller depth.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub depth: u8,
    /// This group represents one extrusion with independent subtractive sketch holes.
    #[serde(default)]
    pub sketch_holes: bool,
    /// Parametric Designer Bag settings. Stored alongside editable parts in scene files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bag: Option<crate::designer_bag::BagConfig>,
    /// Re-editable Designer Chair parameters; none for ordinary groups and old scenes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designer_chair: Option<crate::designer_chair::Parameters>,
    /// A number linked copies and patterns find this group by (0 = none; see `Scene::ensure_group_uid`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub uid: u32,
    /// Live copies of the whole group (mirror, grid, circle, along a path, on points): no shape slots used.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::pattern::lenient_pattern")]
    pub pattern: Option<Pattern>,
    /// A linked copy of the group with this `uid`: it shows that group's shapes through `xform`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_of: Option<u32>,
    /// Where a linked copy stands (move, turn, scale about the source's middle).
    #[serde(default, skip_serializing_if = "Xform::is_identity")]
    pub xform: Xform,
    /// Set when the group holds a whole file that was added to this one ("Add file"): a piece of the big file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<Piece>,
    /// Editable native Designer Lamp parameters; old projects omit this optional field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lamp: Option<crate::designer_lamp::LampParams>,
    /// Regenerable SDF furniture controls (old scene files simply omit this field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organic_furniture: Option<crate::organic_furniture::Settings>,
}

/// Which file a piece of the big file came from, so it can be read again or saved back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    /// The file's name (no folder).
    pub file: String,
    /// Where it was read from, when that is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The longest file name and path a piece keeps.
pub const MAX_PIECE_FILE: usize = 80;
pub const MAX_PIECE_PATH: usize = 1024;

impl Group {
    /// The order the shapes are melted together in: everything that is not a minus first (in list
    /// order), then the minus shapes. So a minus cuts all the plus shapes of its group, wherever it
    /// sits in the list, and moving it up or down never changes the result.
    pub fn fold_order(&self) -> Vec<usize> {
        let keep = |s: &Shape| s.op != Op::Subtract;
        let firsts = self.shapes.iter().enumerate().filter(|(_, s)| keep(s)).map(|(i, _)| i);
        let cuts = self.shapes.iter().enumerate().filter(|(_, s)| !keep(s)).map(|(i, _)| i);
        firsts.chain(cuts).collect()
    }

    pub fn new(name: impl Into<String>) -> Group {
        Group {
            name: name.into(),
            op: Op::Add,
            blend: 0.0,
            visible: true,
            shapes: Vec::new(),
            depth: 0,
            sketch_holes: false,
            bag: None,
            uid: 0,
            pattern: None,
            instance_of: None,
            xform: Xform::default(),
            piece: None,
            lamp: None,
            designer_chair: None,
            organic_furniture: None,
        }
    }
}

/// The curve a blend follows where two shapes meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    /// A plain soft join.
    Smooth,
    /// A rounder, fuller join (like a fillet).
    Round,
    /// A long, gooey neck that stretches between the shapes.
    Gooey,
}

impl BlendMode {
    pub const ALL: [BlendMode; 3] = [BlendMode::Smooth, BlendMode::Round, BlendMode::Gooey];
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Smooth => "Smooth",
            BlendMode::Round => "Round",
            BlendMode::Gooey => "Gooey",
        }
    }
    pub fn id(self) -> f32 {
        match self {
            BlendMode::Smooth => 0.0,
            BlendMode::Round => 1.0,
            BlendMode::Gooey => 2.0,
        }
    }
}

/// Everything about melting that belongs to the whole scene.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Melt {
    pub mode: BlendMode,
    /// Multiplies every blend reach (1 = as set; 0 = sharp).
    pub amount: f32,
    /// How far the melted material sags down in drips (mm).
    pub drip: f32,
    /// How close together the drips are (1 = about every 18 mm).
    pub ripple: f32,
}

impl Default for Melt {
    fn default() -> Self {
        Melt { mode: BlendMode::Smooth, amount: 1.0, drip: 0.0, ripple: 1.0 }
    }
}

/// One light: where it shines from, how strong, what colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Turn about Z in degrees (0 = from the front, +Y side is behind the scene).
    pub yaw: f32,
    /// Height above the ground in degrees.
    pub pitch: f32,
    pub power: f32,
    pub color: [f32; 3],
}

impl Light {
    /// The direction toward the light.
    pub fn dir(&self) -> [f32; 3] {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        [y.sin() * p.cos(), -y.cos() * p.cos(), p.sin()]
    }
}

pub const MAX_LIGHTS: usize = 3;

/// The look of the room: sky colours (what shiny things reflect), backdrop, lights, exposure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub sky_bottom: [f32; 3],
    pub backdrop: [f32; 3],
    pub exposure: f32,
    /// Draw the grid and axes on the ground.
    pub grid: bool,
    /// The ground catches shadows.
    pub floor: bool,
    /// 0 = no shadows, 1 = full.
    pub shadow: f32,
    pub ambient: f32,
    pub lights: Vec<Light>,
}

impl Default for Environment {
    fn default() -> Self {
        Environment::studio()
    }
}

impl Environment {
    /// Named looks: (label, environment).
    pub fn looks() -> Vec<(&'static str, Environment)> {
        let l = |yaw, pitch, power, color| Light { yaw, pitch, power, color };
        let base = Environment::studio();
        vec![
            ("スタジオ", base.clone()),
            (
                "ひるま",
                Environment {
                    sky_top: [0.45, 0.65, 0.95],
                    sky_horizon: [0.80, 0.88, 0.97],
                    sky_bottom: [0.45, 0.50, 0.45],
                    backdrop: [0.62, 0.78, 0.95],
                    lights: vec![l(35.0, 55.0, 1.4, [1.0, 0.97, 0.88]), l(-125.0, 20.0, 0.3, [0.7, 0.8, 1.0])],
                    ..base.clone()
                },
            ),
            (
                "ゆうやけ",
                Environment {
                    sky_top: [0.35, 0.28, 0.55],
                    sky_horizon: [1.0, 0.55, 0.32],
                    sky_bottom: [0.25, 0.18, 0.22],
                    backdrop: [0.30, 0.20, 0.34],
                    lights: vec![l(75.0, 12.0, 1.7, [1.0, 0.62, 0.32]), l(-110.0, 30.0, 0.45, [0.55, 0.45, 0.95])],
                    exposure: 1.05,
                    ..base.clone()
                },
            ),
            (
                "よる",
                Environment {
                    sky_top: [0.06, 0.08, 0.20],
                    sky_horizon: [0.14, 0.18, 0.38],
                    sky_bottom: [0.03, 0.03, 0.06],
                    backdrop: [0.05, 0.06, 0.12],
                    lights: vec![l(40.0, 40.0, 0.8, [0.6, 0.72, 1.0]), l(-100.0, 15.0, 0.5, [0.9, 0.45, 0.75])],
                    exposure: 1.1,
                    ..base.clone()
                },
            ),
            (
                "キャンディ",
                Environment {
                    sky_top: [1.0, 0.82, 0.92],
                    sky_horizon: [1.0, 0.93, 0.80],
                    sky_bottom: [0.75, 0.70, 0.95],
                    backdrop: [0.99, 0.80, 0.88],
                    lights: vec![l(30.0, 50.0, 1.25, [1.0, 0.96, 0.96]), l(-120.0, 25.0, 0.4, [0.8, 0.85, 1.0])],
                    ..base.clone()
                },
            ),
            (
                "しろ",
                Environment {
                    sky_top: [0.97, 0.97, 0.98],
                    sky_horizon: [0.93, 0.93, 0.94],
                    sky_bottom: [0.70, 0.70, 0.72],
                    backdrop: [0.93, 0.93, 0.94],
                    lights: vec![l(35.0, 58.0, 1.0, [1.0, 1.0, 1.0]), l(-120.0, 25.0, 0.4, [1.0, 1.0, 1.0])],
                    ..base.clone()
                },
            ),
        ]
    }

    pub fn studio() -> Environment {
        Environment {
            sky_top: [0.62, 0.66, 0.74],
            sky_horizon: [0.46, 0.46, 0.48],
            sky_bottom: [0.22, 0.22, 0.23],
            backdrop: [0.27, 0.27, 0.285],
            exposure: 1.0,
            grid: true,
            floor: true,
            shadow: 1.0,
            ambient: 1.0,
            lights: vec![
                Light { yaw: 40.0, pitch: 46.0, power: 1.15, color: [1.0, 0.96, 0.9] },
                Light { yaw: -120.0, pitch: 24.0, power: 0.35, color: [0.7, 0.78, 0.95] },
            ],
        }
    }

    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        for c in self.sky_top.iter_mut().chain(&mut self.sky_horizon).chain(&mut self.sky_bottom).chain(&mut self.backdrop) {
            fix(c, 0.0, 1.0, 0.5);
        }
        fix(&mut self.exposure, 0.2, 4.0, 1.0);
        fix(&mut self.shadow, 0.0, 1.0, 1.0);
        fix(&mut self.ambient, 0.0, 3.0, 1.0);
        self.lights.truncate(MAX_LIGHTS);
        for l in &mut self.lights {
            fix(&mut l.yaw, -3600.0, 3600.0, 0.0);
            fix(&mut l.pitch, -89.0, 89.0, 45.0);
            fix(&mut l.power, 0.0, 4.0, 1.0);
            for c in &mut l.color {
                fix(c, 0.0, 1.0, 1.0);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub groups: Vec<Group>,
    #[serde(default)]
    pub melt: Melt,
    #[serde(default)]
    pub env: Environment,
    /// Materials the person saved to use again.
    #[serde(default)]
    pub saved: Vec<Material>,
    /// Poses on a timeline (empty until the person makes the first one).
    #[serde(default, skip_serializing_if = "crate::anim::Anim::is_empty")]
    pub anim: crate::anim::Anim,
}

pub const MAX_SAVED: usize = 24;

impl Default for Scene {
    fn default() -> Self {
        let mut g = Group::new("Group 1");
        g.shapes.push(Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material::default()));
        Scene { groups: vec![g], melt: Melt::default(), env: Environment::default(), saved: Vec::new(), anim: Default::default() }
    }
}

/// Which thing is picked: a shape (group, index) or a whole group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Shape(usize, usize),
    Group(usize),
}

impl Scene {
    pub fn shape(&self, g: usize, s: usize) -> Option<&Shape> {
        self.groups.get(g)?.shapes.get(s)
    }
    pub fn shape_mut(&mut self, g: usize, s: usize) -> Option<&mut Shape> {
        self.groups.get_mut(g)?.shapes.get_mut(s)
    }
    pub fn shape_count(&self) -> usize {
        self.groups.iter().map(|g| g.shapes.len()).sum()
    }
    #[cfg(test)]
    pub fn to_json(&self) -> String {
        self.try_to_json().unwrap_or_default()
    }
    /// Serialization must report errors to disk writers (never save an empty design).
    pub fn try_to_json(&self) -> Result<String, String> {
        let _guard = crate::mesh::SaveGuard::begin();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| serde_json::to_string_pretty(self)))
            .map_err(|_| "作品のシリアライズに失敗しました".to_owned())?
            .map_err(|e| e.to_string())
    }
    /// A scene from JSON, with every number made safe (a damaged or hostile file never panics).
    pub fn from_json(text: &str) -> Result<Scene, String> {
        Scene::from_json_notes(text).map(|(s, _)| s)
    }
    /// `from_json`, and short Japanese notes about what had to be dropped (meshes that were
    /// damaged or too many).
    pub fn from_json_notes(text: &str) -> Result<(Scene, Vec<String>), String> {
        let mut s: Scene = {
            let _guard = crate::mesh::LoadGuard::begin();
            serde_json::from_str(text).map_err(|e| e.to_string())?
        };
        // Sanitization still truncates impossible in-memory scenes as a
        // defensive last resort. File Open must be stricter: accepting an
        // oversized design and then silently discarding its groups or shapes
        // makes the next Save permanently destroy the original work.
        if s.groups.len() > MAX_GROUPS {
            return Err(format!(
                "This project contains {} groups (maximum {MAX_GROUPS}). Nothing was loaded; the original file was preserved.",
                s.groups.len()
            ));
        }
        let shapes = s
            .groups
            .iter()
            .try_fold(0usize, |total, g| total.checked_add(g.shapes.len()))
            .ok_or("This project has too many shapes; the original file was preserved")?;
        if shapes > MAX_SHAPES {
            return Err(format!(
                "This project contains {shapes} shapes (maximum {MAX_SHAPES}). Nothing was loaded; the original file was preserved."
            ));
        }
        let notes = s.sanitize_notes();
        Ok((s, notes))
    }
    // ----- nested groups: `Group::depth` makes the flat list an outline -----

    /// Make the depths a valid outline: the first group is at the top, and no group sits more
    /// than one level below the one before it.
    pub fn normalize_depths(&mut self) {
        let mut prev: i32 = -1;
        for g in &mut self.groups {
            let d = i32::from(g.depth).min(prev + 1).min(i32::from(MAX_DEPTH));
            g.depth = d.max(0) as u8;
            prev = i32::from(g.depth);
        }
    }

    /// The groups from `g` on that belong to it (itself and everything nested in it): `g..end`.
    pub fn subtree_end(&self, g: usize) -> usize {
        let Some(top) = self.groups.get(g) else { return g };
        let mut e = g + 1;
        while self.groups.get(e).is_some_and(|n| n.depth > top.depth) {
            e += 1;
        }
        e
    }

    /// The group `g` sits in, if any.
    pub fn parent_of(&self, g: usize) -> Option<usize> {
        let d = self.groups.get(g)?.depth;
        if d == 0 {
            return None;
        }
        (0..g).rev().find(|&i| self.groups[i].depth < d)
    }

    /// How far a group melts into what comes before it: its own Melt amount, raised to the amount
    /// of the group around it when that is larger. Melt on a parent therefore also melts the
    /// groups inside it, without changing any saved amount. The scene's `Melt.amount` multiplies
    /// this later.
    pub fn group_melt_reach(&self, g: usize) -> f32 {
        let Some(group) = self.groups.get(g) else { return 0.0 };
        let mut reach = clean_blend(group.blend);
        let mut above = self.parent_of(g);
        while let Some(p) = above {
            let Some(parent) = self.groups.get(p) else { break };
            reach = reach.max(clean_blend(parent.blend));
            above = self.parent_of(p);
        }
        reach
    }

    /// Put `g` (with what is nested in it) into the group just above it.
    pub fn indent_group(&mut self, g: usize) -> bool {
        let e = self.subtree_end(g);
        let Some(top) = self.groups.get(g) else { return false };
        let above = match g.checked_sub(1).and_then(|i| self.groups.get(i)) {
            Some(a) => a.depth,
            None => return false,
        };
        let deepest = self.groups[g..e].iter().map(|x| x.depth).max().unwrap_or(0);
        if top.depth > above || deepest >= MAX_DEPTH {
            return false;
        }
        for x in &mut self.groups[g..e] {
            x.depth += 1;
        }
        true
    }

    /// Take `g` (with what is nested in it) out of its group: it lands right after that group.
    /// Returns the new index.
    pub fn outdent_group(&mut self, g: usize) -> Option<usize> {
        let p = self.parent_of(g)?;
        let (e, pe) = (self.subtree_end(g), self.subtree_end(p));
        let mut block: Vec<Group> = self.groups.drain(g..e).collect();
        for x in &mut block {
            x.depth -= 1;
        }
        let at = pe - (e - g);
        for (i, x) in block.into_iter().enumerate() {
            self.groups.insert(at + i, x);
        }
        Some(at)
    }

    /// The first group before `g` at the same level (inside the same group), if any.
    pub fn prev_sibling(&self, g: usize) -> Option<usize> {
        let d = self.groups.get(g)?.depth;
        for i in (0..g).rev() {
            let x = self.groups[i].depth;
            if x == d {
                return Some(i);
            }
            if x < d {
                return None;
            }
        }
        None
    }

    /// The group right after `g`'s subtree, if it is at the same level.
    pub fn next_sibling(&self, g: usize) -> Option<usize> {
        let e = self.subtree_end(g);
        (self.groups.get(e)?.depth == self.groups.get(g)?.depth).then_some(e)
    }

    /// Move `g` (with what is in it) up or down one place among its siblings. Returns where it is now.
    pub fn shift_group(&mut self, g: usize, down: bool) -> Option<usize> {
        let d = self.groups.get(g)?.depth;
        let size = self.subtree_end(g) - g;
        if down {
            let ns = self.next_sibling(g)?;
            let ne = self.subtree_end(ns);
            Some(self.move_subtree(g, ne - size, d))
        } else {
            let ps = self.prev_sibling(g)?;
            Some(self.move_subtree(g, ps, d))
        }
    }

    /// Wrap `g` (with what is in it) in a new, empty group that takes its place. Returns the new group's index.
    pub fn wrap_in_group(&mut self, g: usize, name: String) -> Option<usize> {
        if g >= self.groups.len() || self.groups.len() >= MAX_GROUPS || self.groups[g..self.subtree_end(g)].iter().any(|x| x.depth >= MAX_DEPTH) {
            return None;
        }
        let e = self.subtree_end(g);
        let mut parent = Group::new(name);
        parent.depth = self.groups[g].depth;
        for x in &mut self.groups[g..e] {
            x.depth += 1;
        }
        self.groups.insert(g, parent);
        Some(g)
    }

    /// Put `g` (with what is nested in it) inside `parent`, after what is already there. `None` when
    /// it cannot go there (into itself, or nesting deeper than [`MAX_DEPTH`]). Returns its new index.
    pub fn move_group_into(&mut self, g: usize, parent: usize) -> Option<usize> {
        let n = self.groups.len();
        if g >= n || parent >= n || g == parent {
            return None;
        }
        let e = self.subtree_end(g);
        if (g..e).contains(&parent) {
            return None;
        }
        let base = self.groups[g].depth;
        let deepest = self.groups[g..e].iter().map(|x| x.depth - base).max().unwrap_or(0);
        let depth = self.groups[parent].depth.checked_add(1)?;
        if depth.saturating_add(deepest) > MAX_DEPTH {
            return None;
        }
        let pe = self.subtree_end(parent);
        let removed = e - g;
        let to = if pe >= e { pe - removed } else { pe };
        Some(self.move_subtree(g, to, depth))
    }

    /// Remove `g`; what was nested in it moves up one level.
    pub fn remove_group(&mut self, g: usize) {
        if g >= self.groups.len() {
            return;
        }
        let e = self.subtree_end(g);
        for x in &mut self.groups[g + 1..e] {
            x.depth = x.depth.saturating_sub(1);
        }
        self.groups.remove(g);
        self.normalize_depths();
    }

    /// Move `g` and everything nested in it so that it starts at index `to` of the list as it is
    /// without them; it takes the depth `depth`. Returns where it starts now.
    pub fn move_subtree(&mut self, g: usize, to: usize, depth: u8) -> usize {
        let e = self.subtree_end(g);
        let base = i32::from(self.groups[g].depth);
        let mut block: Vec<Group> = self.groups.drain(g..e).collect();
        for x in &mut block {
            x.depth = (i32::from(x.depth) - base + i32::from(depth)).clamp(0, i32::from(MAX_DEPTH)) as u8;
        }
        let at = to.min(self.groups.len());
        for (i, x) in block.into_iter().enumerate() {
            self.groups.insert(at + i, x);
        }
        self.normalize_depths();
        at
    }

    /// Clamp every number to a range the renderer and the mesher cope with.
    pub fn sanitize(&mut self) {
        let _ = self.sanitize_notes();
    }
    /// `sanitize`, saying what it had to take out (see `from_json_notes`).
    pub fn sanitize_notes(&mut self) -> Vec<String> {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        let (mut lost, mut over) = (0usize, 0usize);
        let mut broken_furniture = 0usize;
        let mut vol_shapes = 0usize;
        let mut vol_nodes = 0usize;
        let mut vol_seen: Vec<usize> = Vec::new();
        self.normalize_depths();
        fix(&mut self.melt.amount, 0.0, 10.0, 1.0);
        fix(&mut self.melt.drip, 0.0, 40.0, 0.0);
        fix(&mut self.melt.ripple, 0.3, 4.0, 1.0);
        self.env.sanitize();
        self.anim.sanitize();
        self.saved.truncate(MAX_SAVED);
        for m in &mut self.saved {
            m.sanitize();
        }
        self.groups.truncate(MAX_GROUPS);
        self.dedupe_uids();
        let mut left = MAX_SHAPES;
        for g in &mut self.groups {
            fix(&mut g.blend, 0.0, 200.0, 0.0);
            if let Some(p) = &mut g.piece {
                p.file = p.file.chars().filter(|c| !c.is_control()).take(MAX_PIECE_FILE).collect();
                if let Some(path) = &mut p.path {
                    path.truncate(path.char_indices().nth(MAX_PIECE_PATH).map_or(path.len(), |(i, _)| i));
                }
            }
            if let Some(bag) = &mut g.bag {
                bag.sanitize();
            }
            if let Some(p) = &mut g.designer_chair {
                p.sanitize();
            }
            if let Some(p) = &mut g.pattern {
                p.sanitize();
            }
            g.xform.sanitize();
            if let Some(params) = &mut g.organic_furniture {
                params.sanitize();
            }
            if g.instance_of.is_some() {
                g.organic_furniture = None;
                // a linked copy shows its source's shapes, not its own
                g.shapes.clear();
            }
            g.shapes.truncate(left);
            left -= g.shapes.len();
            if let Some(lamp) = &mut g.lamp {
                lamp.sanitize();
            }
            for s in &mut g.shapes {
                for v in &mut s.size {
                    fix(v, 0.01, 2000.0, 10.0);
                }
                fix(&mut s.round, 0.0, MAX_ROUND, 0.0);
                fix(&mut s.blend, 0.0, 200.0, 0.0);
                for v in &mut s.pos {
                    fix(v, -5000.0, 5000.0, 0.0);
                }
                for v in &mut s.rot {
                    fix(v, -3600.0, 3600.0, 0.0);
                }
                for v in &mut s.scale {
                    fix(v, 0.01, 100.0, 1.0);
                }
                s.material.sanitize();
                s.outline.truncate(MAX_OUTLINE);
                for p in &mut s.outline {
                    fix(&mut p[0], -2000.0, 2000.0, 0.0);
                    fix(&mut p[1], -2000.0, 2000.0, 0.0);
                }
                if s.outline.len() < 3 {
                    s.outline = vec![[-15.0, -15.0], [15.0, -15.0], [0.0, 15.0]];
                }
                for v in &mut s.params {
                    fix(v, -5000.0, 5000.0, 0.0);
                }
                fix(&mut s.shell, 0.0, 500.0, 0.0);
                s.outline2.truncate(MAX_OUTLINE);
                for p in &mut s.outline2 {
                    fix(&mut p[0], -2000.0, 2000.0, 0.0);
                    fix(&mut p[1], -2000.0, 2000.0, 0.0);
                }
                if s.kind.uses_outline2() && s.outline2.len() < 3 {
                    s.outline2 = s.kind.default_outline2();
                }
                // A finished sketch already contains its sampled outline. Do not
                // silently drop its last 16 vertices when reopening the document.
                s.path.truncate(if s.guide { MAX_OUTLINE } else { MAX_PATH });
                for p in &mut s.path {
                    fix(&mut p[0], -5000.0, 5000.0, 0.0);
                    fix(&mut p[1], -5000.0, 5000.0, 0.0);
                    fix(&mut p[2], -5000.0, 5000.0, 0.0);
                    fix(&mut p[3], 0.1, 500.0, 5.0);
                }
                if s.kind.uses_path() && s.path.len() < 2 {
                    s.path = s.kind.default_path();
                }
                // one handle pair per node at most, each finite and inside the world (#215): a file
                // with a huge handle list must not slow every curve sample down
                s.curve_handles.truncate(s.path.len());
                for h in &mut s.curve_handles {
                    for v in h.incoming.iter_mut().chain(h.outgoing.iter_mut()) {
                        fix(v, -5000.0, 5000.0, 0.0);
                    }
                }
                if !s.guide || s.kind != Kind::Curve {
                    s.guide_plane = None;
                } else if let Some(plane) = &mut s.guide_plane {
                    if let Some(frame) = crate::sketch::Frame::from_axes([0.0; 3], plane.u, plane.v) {
                        plane.u = frame.u;
                        plane.v = frame.v;
                        plane.holes.truncate(crate::sketch::MAX_HOLES);
                        for hole in &mut plane.holes {
                            hole.truncate(MAX_OUTLINE);
                        }
                        plane.holes.retain(|hole| {
                            hole.iter().flatten().all(|v| v.is_finite() && v.abs() <= crate::sketch::COORD_MAX)
                                && crate::sketch::validate_profile(hole).is_ok()
                        });
                    } else {
                        s.guide_plane = None;
                    }
                }
                if s.kind != Kind::Mesh {
                    s.volume = None;
                    s.mesh_source = None;
                }
                if s.mesh_source.as_ref().is_some_and(|src| src.path.is_empty() || src.path.len() > 4096) {
                    s.mesh_source = None;
                }
                // Restore the derived profile after loading user-edited vase parameters.
                // Kind changes abandon the parametric generator instead of overwriting geometry.
                if s.kind == Kind::Revolve && s.vase.is_some() {
                    crate::organic_vase::apply(s);
                } else {
                    s.vase = None;
                }
            }
            // a mesh whose data is gone or damaged cannot be drawn: it goes, with a note; so do
            // the meshes past the limits of a scene (mesh.rs)
            g.shapes.retain(|s| {
                if s.kind != Kind::Mesh {
                    return true;
                }
                let Some(v) = &s.volume else {
                    lost += 1;
                    return false;
                };
                let new = !vol_seen.contains(&v.key());
                if vol_shapes >= crate::mesh::MAX_VOLUME_SHAPES || (new && vol_nodes.saturating_add(v.nodes()) > crate::mesh::MAX_TOTAL_NODES) {
                    over += 1;
                    return false;
                }
                vol_shapes += 1;
                if new {
                    vol_seen.push(v.key());
                    vol_nodes += v.nodes();
                }
                true
            });
            if g.organic_furniture.as_ref().is_some_and(|p| !crate::organic_furniture::is_generated_group(g, p)) {
                // A user may have deleted or reordered the managed shapes. Keep the geometry
                // exactly as it is, but drop metadata that could overwrite the wrong parts.
                g.organic_furniture = None;
                broken_furniture += 1;
            }
        }
        let mut notes = Vec::new();
        if lost > 0 {
            notes.push(format!("こわれていて読めないメッシュが{lost}個あったので、取りのぞきました"));
        }
        if over > 0 {
            notes.push(format!(
                "メッシュが多すぎるので、{over}個を取りのぞきました（{}個・データ量に上限があります）",
                crate::mesh::MAX_VOLUME_SHAPES
            ));
        }
        if broken_furniture > 0 {
            notes.push(format!("家具プリセットの部品が変わった{broken_furniture}組は、形を残して通常のグループとして開きました"));
        }
        notes
    }
}

pub const MAX_GROUPS: usize = 96;
pub const MAX_SHAPES: usize = 512;

#[cfg(test)]
mod tests {
    use super::*;

    fn named(names: &[(&str, u8)]) -> Scene {
        let groups = names
            .iter()
            .map(|&(n, d)| {
                let mut g = Group::new(n);
                g.depth = d;
                g
            })
            .collect();
        Scene { groups, ..Scene::default() }
    }

    fn layout(sc: &Scene) -> Vec<(String, u8)> {
        sc.groups.iter().map(|g| (g.name.clone(), g.depth)).collect()
    }

    #[test]
    fn a_file_with_a_huge_curve_handle_list_loads_with_one_handle_per_node() {
        let mut sc = Scene::default();
        let mut sh = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        sh.path = vec![[0.0, 0.0, 0.0, 4.0], [10.0, 0.0, 0.0, 4.0], [20.0, 5.0, 0.0, 4.0]];
        sh.curve_handles = vec![CurveHandles::default(); 100_000];
        sh.curve_handles[0].outgoing = [-1e9, 1e9, 2.0];
        sc.groups[0].shapes.push(sh);
        let (back, _) = Scene::from_json_notes(&sc.to_json()).expect("loads");
        let curve = back.groups[0].shapes.iter().find(|s| s.kind == Kind::Curve).expect("the curve");
        assert_eq!(curve.curve_handles.len(), 3, "one handle pair per node (#215)");
        let o = curve.curve_handles[0].outgoing;
        assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 5000.0), "{o:?}");
    }

    #[test]
    fn a_group_dropped_onto_another_goes_inside_it() {
        // A, B, C: put B inside C, then A inside C as well -> C is the big group
        let mut sc = named(&[("A", 0), ("B", 0), ("C", 0)]);
        let at = sc.move_group_into(1, 2).expect("B goes into C");
        assert_eq!(layout(&sc), vec![("A".into(), 0), ("C".into(), 0), ("B".into(), 1)]);
        assert_eq!(sc.groups[at].name, "B");
        sc.move_group_into(0, 1).expect("A goes into C too (a group before its new parent)");
        assert_eq!(layout(&sc), vec![("C".into(), 0), ("B".into(), 1), ("A".into(), 1)]);
    }

    #[test]
    fn nesting_stops_at_two_levels() {
        let mut sc = named(&[("A", 0), ("a1", 1), ("B", 0), ("b1", 1)]);
        assert_eq!(sc.move_group_into(0, 1), None, "not into its own child");
        assert_eq!(sc.move_group_into(0, 0), None);
        assert_eq!(sc.move_group_into(9, 0), None);
        let before = sc.clone();
        assert_eq!(sc.move_group_into(2, 0), None, "a big group cannot go inside another group");
        assert_eq!(sc.move_group_into(3, 1), None, "a group inside a big group cannot take another group");
        assert_eq!(sc, before, "a refused move changes nothing");
        assert!(sc.move_group_into(3, 0).is_some(), "but a group can move from one big group to another");
        assert_eq!(layout(&sc), vec![("A".into(), 0), ("a1".into(), 1), ("b1".into(), 1), ("B".into(), 0)]);
    }

    #[test]
    fn a_parent_melt_reaches_the_groups_inside_it() {
        let mut sc = named(&[("parent", 0), ("child a", 1), ("child b", 1), ("top", 0)]);
        sc.groups[0].blend = 67.38;
        assert_eq!(sc.group_melt_reach(0), 67.38);
        assert_eq!(sc.group_melt_reach(1), 67.38, "a parent's Melt melts the groups inside it");
        assert_eq!(sc.group_melt_reach(2), 67.38);
        assert_eq!(sc.group_melt_reach(3), 0.0, "a group outside the parent keeps its own amount");
        sc.groups[1].blend = 80.0;
        assert_eq!(sc.group_melt_reach(1), 80.0, "a child may melt further than its parent");
        sc.groups[0].blend = 0.0;
        assert_eq!(sc.group_melt_reach(2), 0.0, "Melt off everywhere unions plainly");
        sc.groups[0].blend = f32::NAN;
        assert_eq!(sc.group_melt_reach(2), 0.0, "a hostile amount melts nothing");
    }

    #[test]
    fn a_sketch_outline_edit_updates_the_solid_atomically_and_survives_reload() {
        use crate::sdf::Prepared;

        let built = crate::sketch::extrude(&crate::sketch::Frame::floor(), &crate::sketch::rect([-10.0, -10.0], [10.0, 10.0]), 10.0, false)
            .expect("a rectangle extrudes");
        let mut shape = Shape::new(Kind::Extrude, [0.0; 3], Material::default());
        crate::sketch::apply(&built, &mut shape);
        let original = shape.clone();
        let probe = [13.0, 7.0, 5.0];
        assert!(Prepared::new(&shape).distance(probe) > 0.0);

        let mut crossing = shape.outline.clone();
        crossing[2] = [-20.0, 0.0];
        assert!(shape.set_sketch_outline(false, crossing).is_err());
        assert_eq!(shape, original, "rejecting a crossing is non-destructive");

        let mut repeated = shape.outline.clone();
        repeated[2] = repeated[1];
        assert!(shape.set_sketch_outline(false, repeated).is_err());
        assert_eq!(shape, original, "zero-length edges are also rejected");

        let mut enlarged = shape.outline.clone();
        enlarged[2] = [20.0, 10.0];
        assert_eq!(shape.set_sketch_outline(false, enlarged.clone()), Ok(true));
        assert_eq!(shape.set_sketch_outline(false, enlarged.clone()), Ok(false));
        assert!(Prepared::new(&shape).distance(probe) < 0.0, "3D solid follows the new profile");
        assert_eq!((shape.pos, shape.rot, shape.size), (original.pos, original.rot, original.size));
        assert_eq!(shape.material, original.material);

        let mut scene = Scene::default();
        scene.groups[0].shapes = vec![shape.clone()];
        let restored = Scene::from_json(&scene.to_json()).expect("saved sketch loads");
        let mut reopened = restored.shape(0, 0).expect("shape stays").clone();
        assert_eq!(reopened, shape, "the vertices and other properties survive serialization");
        assert!(Prepared::new(&reopened).distance(probe) < 0.0);
        let mut new_outline = reopened.outline.clone();
        new_outline[2] = [24.0, 10.0];
        assert_eq!(reopened.set_sketch_outline(false, new_outline), Ok(true), "reopened sketch is editable again");

        let mut primitive = Shape::new(Kind::Box, [0.0; 3], Material::default());
        assert!(primitive.set_sketch_outline(false, enlarged).is_err());
        assert!(shape.set_sketch_outline(true, shape.outline.clone()).is_err(), "only lofts have a top outline");
    }

    #[test]
    fn a_scene_round_trips_and_hostile_numbers_are_made_safe() {
        let s = Scene::default();
        assert_eq!(Scene::from_json(&s.to_json()).unwrap(), s);
        let bad = r#"{"groups":[{"name":"g","op":"Add","blend":1e30,"visible":true,"shapes":[{"name":"s","kind":"Box","size":[0,-5,1e30],"round":0,"pos":[1e30,0,0],"rot":[0,0,0],"scale":[0,1,1],"op":"Add","blend":-3,"material":{"color":[2,0,0],"roughness":9,"metallic":0},"visible":true,"outline":[]}]}]}"#;
        let t = Scene::from_json(bad).unwrap();
        let sh = &t.groups[0].shapes[0];
        assert!(sh.size.iter().all(|v| (0.01..=2000.0).contains(v)) && sh.scale[0] >= 0.01 && sh.pos[0] <= 5000.0);
        assert!(t.groups[0].blend <= 200.0 && sh.blend >= 0.0 && sh.material.roughness <= 1.0 && sh.outline.len() >= 3);
        assert!(Scene::from_json("{").is_err());
    }

    #[test]
    fn old_files_without_the_new_fields_still_open() {
        let old = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"shapes":[{"name":"s","kind":"Sphere","size":[20,1,1],"round":0,"pos":[0,0,20],"rot":[0,0,0],"scale":[1,1,1],"op":"Add","blend":6,"material":{"color":[1,0,0],"roughness":0.3,"metallic":1},"visible":true,"outline":[]}]}]}"#;
        let t = Scene::from_json(old).unwrap();
        let m = t.groups[0].shapes[0].material;
        assert_eq!((m.glass, m.glow, m.coat), (0.0, 0.0, 0.0));
        assert_eq!(t.env, Environment::default());
        let hostile = r#"{"groups":[],"env":{"exposure":1e30,"lights":[{"yaw":1,"pitch":99999,"power":-4,"color":[9,9,9]},{"yaw":0,"pitch":0,"power":1,"color":[1,1,1]},{"yaw":0,"pitch":0,"power":1,"color":[1,1,1]},{"yaw":0,"pitch":0,"power":1,"color":[1,1,1]}]}}"#;
        let h = Scene::from_json(hostile).unwrap();
        assert!(h.env.exposure <= 4.0 && h.env.lights.len() == MAX_LIGHTS && h.env.lights[0].pitch <= 89.0);
    }

    #[test]
    fn ops_cycle_and_the_new_fields_round_trip() {
        assert_eq!(Op::Intersect.sign(), 2.0);
        let mut o = Op::Add;
        for _ in 0..4 {
            o = o.next();
        }
        assert_eq!(o, Op::Add);
        assert_eq!(Op::Intersect.flip(), Op::Add);
        let mut sc = Scene::default();
        sc.groups[0].shapes[0].op = Op::Intersect;
        sc.groups[0].shapes[0].shell = 3.0;
        assert_eq!(Scene::from_json(&sc.to_json()).unwrap(), sc);
        sc.groups[0].shapes[0].shell = f32::INFINITY;
        sc.sanitize();
        assert_eq!(sc.groups[0].shapes[0].shell, 0.0);
        let old = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"shapes":[{"name":"s","kind":"Sphere","size":[20,1,1],"round":0,"pos":[0,0,20],"rot":[0,0,0],"scale":[1,1,1],"op":"Add","blend":6,"material":{"color":[1,0,0],"roughness":0.3,"metallic":1},"visible":true,"outline":[]}]}]}"#;
        assert_eq!(Scene::from_json(old).unwrap().groups[0].shapes[0].shell, 0.0);
    }

    #[test]
    fn every_look_is_valid() {
        for (_, e) in Environment::looks() {
            let mut c = e.clone();
            c.sanitize();
            assert_eq!(c, e);
        }
    }

    #[test]
    fn gradient_fields_are_sanitised() {
        let mut m = Material {
            gradient: f32::NAN,
            grad_soft: 9.0,
            grad_from: -3.0,
            grad_to: f32::INFINITY,
            grad_axis: 77,
            color2: [f32::NAN, 5.0, -1.0],
            ..Material::default()
        };
        m.sanitize();
        assert_eq!(m.gradient, 0.0);
        assert_eq!(m.grad_soft, 1.0);
        assert_eq!(m.grad_from, 0.0);
        assert_eq!(m.grad_to, 1.0);
        assert_eq!(m.grad_axis, 2);
        assert!(m.color2.iter().all(|c| (0.0..=1.0).contains(c)));
        assert!(!m.has_gradient());
        let mut big = Material { gradient: 40.0, ..Material::default() };
        big.sanitize();
        assert_eq!(big.gradient, 1.0);
        assert!(big.has_gradient());
    }

    #[test]
    fn the_gradient_weight_follows_range_softness_and_strength() {
        let mut m = Material { gradient: 1.0, grad_soft: 0.0, ..Material::default() };
        // a hard edge in the middle
        assert_eq!(m.gradient_weight(0.2), 0.0);
        assert_eq!(m.gradient_weight(0.8), 1.0);
        // smooth: halfway is halfway, and it only rises
        m.grad_soft = 1.0;
        assert!((m.gradient_weight(0.5) - 0.5).abs() < 1e-5);
        let mut last = -1.0;
        for i in 0..=20 {
            let w = m.gradient_weight(i as f32 / 20.0);
            assert!(w >= last - 1e-6 && (0.0..=1.0).contains(&w));
            last = w;
        }
        assert!(m.gradient_weight(0.0) < 1e-6 && m.gradient_weight(1.0) > 1.0 - 1e-6);
        // strength scales it
        m.gradient = 0.5;
        assert!((m.gradient_weight(1.0) - 0.5).abs() < 1e-5);
        // the range: nothing before `from`, everything after `to`
        m.gradient = 1.0;
        m.grad_from = 0.25;
        m.grad_to = 0.75;
        assert!(m.gradient_weight(0.2) < 1e-6);
        assert!(m.gradient_weight(0.8) > 1.0 - 1e-6);
        assert!((m.gradient_weight(0.5) - 0.5).abs() < 1e-5);
        // a reversed or empty range does not divide by zero
        m.grad_from = 0.9;
        m.grad_to = 0.1;
        assert!(m.gradient_weight(0.5).is_finite());
    }

    #[test]
    fn old_materials_load_without_a_gradient_and_new_ones_round_trip() {
        let old = r#"{"color":[1,0,0],"roughness":0.5,"metallic":0}"#;
        let m: Material = serde_json::from_str(old).unwrap_or_default();
        assert_eq!(m.color, [1.0, 0.0, 0.0]);
        assert!(!m.has_gradient());
        assert_eq!(m.grad_axis, 2);
        let g = Material {
            gradient: 0.7,
            grad_axis: 0,
            grad_flip: true,
            grad_local: true,
            color2: [0.1, 0.2, 0.3],
            grad_soft: 0.3,
            grad_from: 0.1,
            grad_to: 0.9,
            ..Material::default()
        };
        let back: Material = serde_json::from_str(&serde_json::to_string(&g).unwrap_or_default()).unwrap_or_default();
        assert_eq!(back, g);
        // a whole old scene still opens
        let sc = Scene::default().to_json();
        assert!(Scene::from_json(&sc).is_ok());
    }

    #[test]
    fn a_file_with_too_much_is_cut_to_the_new_limits() {
        const {
            assert!(MAX_SHAPES >= 512 && MAX_GROUPS >= 96);
        }
        let mut sc = Scene::default();
        let one = sc.groups[0].shapes[0].clone();
        sc.groups.clear();
        for g in 0..MAX_GROUPS + 20 {
            let mut gr = Group::new(format!("g{g}"));
            gr.shapes = vec![one.clone(); 8];
            sc.groups.push(gr);
        }
        let original = sc.to_json();
        assert!(Scene::from_json(&original).is_err(), "oversized projects must not silently lose groups");
        // In-memory hardening still clamps programmatic hostile scenes.
        sc.sanitize();
        assert_eq!(sc.groups.len(), MAX_GROUPS);
        assert_eq!(sc.shape_count(), MAX_SHAPES, "{} groups of 8 shapes are cut at {MAX_SHAPES}", MAX_GROUPS);
        // a file with a hostile pattern keeps only safe numbers
        let hostile = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"shapes":[],"pattern":{"Rect":{"counts":[4000000000,0,9],"spacing":[1e30,-1e30,0]}},"xform":{"pos":[1e30,0,0],"rot":[0,0,0],"scale":-4}}]}"#;
        let h = Scene::from_json(hostile).unwrap_or_default();
        match &h.groups[0].pattern {
            Some(crate::pattern::Pattern::Rect { counts, spacing }) => {
                assert!(counts.iter().all(|c| (1..=crate::pattern::MAX_SIDE).contains(c)));
                assert!(spacing.iter().all(|s| s.abs() <= 5000.0));
            }
            other => panic!("{other:?}"),
        }
        assert!(h.groups[0].xform.scale >= 0.01 && h.groups[0].xform.pos[0] <= 5000.0);
        // a linked copy keeps no shapes of its own
        let linked = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"uid":3,"shapes":[]},{"name":"c","op":"Add","blend":0,"visible":true,"instance_of":3,"shapes":[{"name":"s","kind":"Sphere","size":[20,1,1],"round":0,"pos":[0,0,20],"rot":[0,0,0],"scale":[1,1,1],"op":"Add","blend":6,"material":{"color":[1,0,0],"roughness":0.3,"metallic":1},"visible":true,"outline":[]}]}]}"#;
        let l = Scene::from_json(linked).unwrap_or_default();
        assert!(l.groups[1].shapes.is_empty() && l.groups[1].instance_of == Some(3));
    }

    #[test]
    fn opening_one_group_with_513_shapes_fails_without_truncation() {
        let mut sc = Scene::default();
        let shape = sc.groups[0].shapes[0].clone();
        sc.groups[0].shapes = vec![shape; MAX_SHAPES + 1];
        let text = sc.to_json();
        let failure = Scene::from_json_notes(&text).expect_err("must reject a 513-shape document");
        assert!(failure.contains("maximum") && failure.contains("preserved"), "{failure}");
        assert!(serde_json::from_str::<Scene>(&text).is_ok(), "the source itself is recoverable");
    }

    #[test]
    fn opening_97_groups_fails_instead_of_discarding_the_last_group() {
        let mut sc = Scene { groups: Vec::new(), ..Scene::default() };
        for i in 0..=MAX_GROUPS {
            sc.groups.push(Group::new(format!("Group {}", i + 1)));
        }
        let text = sc.to_json();
        assert!(Scene::from_json_notes(&text).unwrap_err().contains("groups"));
        assert!(serde_json::from_str::<Scene>(&text).is_ok());
        sc.groups.truncate(MAX_GROUPS);
        assert!(Scene::from_json(&sc.to_json()).is_ok());
    }

    #[test]
    fn rounding_is_held_inside_the_file_limits() {
        let mut sc = Scene::default();
        if let Some(s) = sc.groups[0].shapes.first_mut() {
            s.round = f32::NAN;
        }
        sc.sanitize();
        assert_eq!(sc.groups[0].shapes[0].round, 0.0);
        if let Some(s) = sc.groups[0].shapes.first_mut() {
            s.round = 1e9;
        }
        sc.sanitize();
        assert_eq!(sc.groups[0].shapes[0].round, MAX_ROUND);
        assert!(Kind::Box.can_round() && Kind::Cone.can_round() && Kind::Sweep.can_round());
        assert!(!Kind::Sphere.can_round() && !Kind::Curve.can_round());
    }
    #[test]
    fn mesh_source_is_optional_for_legacy_files_and_roundtrips_when_present() {
        let mut sh = Shape::new(Kind::Mesh, [0.0; 3], Material::default());
        let old_text = serde_json::to_string(&sh).unwrap();
        let old: Shape = serde_json::from_str(&old_text).unwrap();
        assert!(old.mesh_source.is_none());
        sh.mesh_source = Some(MeshSource { path: "original.stl".into(), y_up: true, mesh_hash: 12345 });
        let again: Shape = serde_json::from_str(&serde_json::to_string(&sh).unwrap()).unwrap();
        assert_eq!(again.mesh_source, sh.mesh_source);
        sh.scale = [2.0, 3.0, 4.0];
        sh.volume = Some(crate::mesh::SharedVolume::new(crate::mesh::tests::ball_volume(8)));
        assert_eq!(sh.mesh_cell_mm(), Some([2.0, 3.0, 4.0]));
    }
}
