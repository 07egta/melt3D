//! The scene: groups of shapes that are added to or carved out of each other with a soft blend.
//! Everything is millimetres, Z is up, angles are degrees.

use serde::{Deserialize, Serialize};

use crate::mesh::SharedVolume;
use crate::pattern::{Pattern, Xform};

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
    /// A tube through the `path` nodes (x, y, z, radius), like Womp's curve tool.
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
    pub const MAIN: [Kind; 6] = [Kind::Sphere, Kind::Box, Kind::Cylinder, Kind::Torus, Kind::Cone, Kind::Extrude];
    /// The extra solids in the "more" menu.
    pub const MORE: [Kind; 10] =
        [Kind::Prism3, Kind::PrismN, Kind::Octa, Kind::Tetra, Kind::Pyramid, Kind::Capsule, Kind::Hemi, Kind::Tube, Kind::Wedge, Kind::Star];
    /// CAD-style features (profile based).
    pub const CAD: [Kind; 4] = [Kind::Revolve, Kind::Loft, Kind::Curve, Kind::Sweep];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sphere => "球",
            Kind::Box => "箱",
            Kind::Cylinder => "円柱",
            Kind::Torus => "ドーナツ",
            Kind::Cone => "円すい",
            Kind::Extrude => "押し出し",
            Kind::Prism3 => "三角柱",
            Kind::PrismN => "多角柱",
            Kind::Octa => "正八面体",
            Kind::Tetra => "正四面体",
            Kind::Pyramid => "ピラミッド",
            Kind::Capsule => "カプセル",
            Kind::Hemi => "半球",
            Kind::Tube => "パイプ",
            Kind::Wedge => "くさび",
            Kind::Star => "星柱",
            Kind::Revolve => "回転体",
            Kind::Loft => "ロフト",
            Kind::Curve => "線",
            Kind::Sweep => "スイープ",
            Kind::Mesh => "メッシュ",
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
            Kind::Sphere => ["半径", "", ""],
            Kind::Box => ["幅", "奥行", "高さ"],
            Kind::Cylinder => ["半径", "高さ", ""],
            Kind::Torus => ["大半径", "小半径", ""],
            Kind::Cone => ["半径", "高さ", ""],
            Kind::Extrude => ["高さ", "", ""],
            Kind::Prism3 | Kind::PrismN | Kind::Star => ["外半径", "高さ", ""],
            Kind::Octa | Kind::Tetra | Kind::Hemi => ["半径", "", ""],
            Kind::Pyramid => ["底の半径", "高さ", ""],
            Kind::Capsule => ["半径", "長さ", ""],
            Kind::Tube => ["外半径", "肉厚", "高さ"],
            Kind::Wedge => ["幅", "奥行", "高さ"],
            Kind::Revolve => ["", "", ""],
            Kind::Loft => ["高さ", "", ""],
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
            Kind::PrismN => ["辺の数", "", "", ""],
            Kind::Pyramid => ["辺の数", "", "", ""],
            Kind::Star => ["とがり数", "内側の比", "", ""],
            Kind::Revolve => ["回す角度", "", "", ""],
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
    /// Corners of an `Extrude` outline, in the shape's own XY plane.
    pub outline: Vec<[f32; 2]>,
    /// Kind-specific extra numbers (`Kind::param_labels`).
    #[serde(default)]
    pub params: [f32; 4],
    /// Nodes (x, y, z, radius) of a `Curve` or `Sweep`, in the shape's own space.
    #[serde(default)]
    pub path: Vec<[f32; 4]>,
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
    /// Where an imported mesh came from (its file name), shown in the inspector.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
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
            outline: kind.default_outline(),
            params: kind.default_params(),
            path: kind.default_path(),
            outline2: kind.default_outline2(),
            shell: 0.0,
            corner: Corner::Round,
            uid: 0,
            volume: None,
            source: String::new(),
        }
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
                let r = mx(&self.outline, 0);
                [r, r, mx(&self.outline, 1)]
            }
            Kind::Loft => {
                let x = mx(&self.outline, 0).max(mx(&self.outline2, 0));
                let y = mx(&self.outline, 1).max(mx(&self.outline2, 1));
                [x, y, s[0] * 0.5]
            }
            Kind::Curve => crate::curve::half_extent(&self.path, &self.params),
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub op: Op,
    /// How far the whole group melts into the groups before it, in mm.
    pub blend: f32,
    pub visible: bool,
    pub shapes: Vec<Shape>,
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
}

impl Group {
    pub fn new(name: impl Into<String>) -> Group {
        Group { name: name.into(), op: Op::Add, blend: 0.0, visible: true, shapes: Vec::new(), designer_chair: None, uid: 0, pattern: None, instance_of: None, xform: Xform::default() }
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
            BlendMode::Smooth => "なめらか",
            BlendMode::Round => "まるく",
            BlendMode::Gooey => "どろっと",
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
}

pub const MAX_SAVED: usize = 24;

impl Default for Scene {
    fn default() -> Self {
        let mut g = Group::new("グループ 1");
        g.shapes.push(Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material::default()));
        Scene { groups: vec![g], melt: Melt::default(), env: Environment::default(), saved: Vec::new() }
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
    pub fn to_json(&self) -> String {
        // (a volume used by several shapes is written once: see mesh.rs)
        let _guard = crate::mesh::SaveGuard::begin();
        serde_json::to_string_pretty(self).unwrap_or_default()
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
        let notes = s.sanitize_notes();
        Ok((s, notes))
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
        let mut vol_shapes = 0usize;
        let mut vol_nodes = 0usize;
        let mut vol_seen: Vec<usize> = Vec::new();
        fix(&mut self.melt.amount, 0.0, 10.0, 1.0);
        fix(&mut self.melt.drip, 0.0, 40.0, 0.0);
        fix(&mut self.melt.ripple, 0.3, 4.0, 1.0);
        self.env.sanitize();
        self.saved.truncate(MAX_SAVED);
        for m in &mut self.saved {
            m.sanitize();
        }
        self.groups.truncate(MAX_GROUPS);
        self.dedupe_uids();
        let mut left = MAX_SHAPES;
        for g in &mut self.groups {
            fix(&mut g.blend, 0.0, 200.0, 0.0);
            if let Some(p) = &mut g.designer_chair {
                p.sanitize();
            }
            if let Some(p) = &mut g.pattern {
                p.sanitize();
            }
            g.xform.sanitize();
            if g.instance_of.is_some() {
                // a linked copy shows its source's shapes, not its own
                g.shapes.clear();
            }
            g.shapes.truncate(left);
            left -= g.shapes.len();
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
                s.path.truncate(MAX_PATH);
                for p in &mut s.path {
                    fix(&mut p[0], -5000.0, 5000.0, 0.0);
                    fix(&mut p[1], -5000.0, 5000.0, 0.0);
                    fix(&mut p[2], -5000.0, 5000.0, 0.0);
                    fix(&mut p[3], 0.1, 500.0, 5.0);
                }
                if s.kind.uses_path() && s.path.len() < 2 {
                    s.path = s.kind.default_path();
                }
                if s.kind != Kind::Mesh {
                    s.volume = None;
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
        }
        let mut notes = Vec::new();
        if lost > 0 {
            notes.push(format!("こわれていて読めないメッシュが{lost}個あったので、取りのぞきました"));
        }
        if over > 0 {
            notes.push(format!("メッシュが多すぎるので、{over}個を取りのぞきました（{}個・データ量に上限があります）", crate::mesh::MAX_VOLUME_SHAPES));
        }
        notes
    }
}

pub const MAX_GROUPS: usize = 96;
pub const MAX_SHAPES: usize = 512;

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut m = Material { gradient: f32::NAN, grad_soft: 9.0, grad_from: -3.0, grad_to: f32::INFINITY, grad_axis: 77, color2: [f32::NAN, 5.0, -1.0], ..Material::default() };
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
        let g = Material { gradient: 0.7, grad_axis: 0, grad_flip: true, grad_local: true, color2: [0.1, 0.2, 0.3], grad_soft: 0.3, grad_from: 0.1, grad_to: 0.9, ..Material::default() };
        let back: Material = serde_json::from_str(&serde_json::to_string(&g).unwrap_or_default()).unwrap_or_default();
        assert_eq!(back, g);
        // a whole old scene still opens
        let sc = Scene::default().to_json();
        assert!(Scene::from_json(&sc).is_ok());
    }

    #[test]
    fn a_file_with_too_much_is_cut_to_the_new_limits() {
        assert!(MAX_SHAPES >= 512 && MAX_GROUPS >= 96);
        let mut sc = Scene::default();
        let one = sc.groups[0].shapes[0].clone();
        sc.groups.clear();
        for g in 0..MAX_GROUPS + 20 {
            let mut gr = Group::new(format!("g{g}"));
            gr.shapes = vec![one.clone(); 8];
            sc.groups.push(gr);
        }
        let back = Scene::from_json(&sc.to_json()).unwrap_or_default();
        assert_eq!(back.groups.len(), MAX_GROUPS);
        assert_eq!(back.shape_count(), MAX_SHAPES, "{} groups of 8 shapes are cut at {MAX_SHAPES}", MAX_GROUPS);
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
}
