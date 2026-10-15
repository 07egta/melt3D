//! The scene: groups of shapes that are added to or carved out of each other with a soft blend.
//! Everything is millimetres, Z is up, angles are degrees.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Sphere,
    Box,
    Cylinder,
    Torus,
    Cone,
    /// A 2D outline (`Shape::outline`) pushed straight up.
    Extrude,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Sphere, Kind::Box, Kind::Cylinder, Kind::Torus, Kind::Cone, Kind::Extrude];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sphere => "球",
            Kind::Box => "箱",
            Kind::Cylinder => "円柱",
            Kind::Torus => "ドーナツ",
            Kind::Cone => "円すい",
            Kind::Extrude => "押し出し",
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
        }
    }
    /// What the four size numbers mean, for the inspector.
    pub fn size_labels(self) -> [&'static str; 3] {
        match self {
            Kind::Sphere => ["半径", "", ""],
            Kind::Box => ["幅", "奥行", "高さ"],
            Kind::Cylinder => ["半径", "高さ", ""],
            Kind::Torus => ["大半径", "小半径", ""],
            Kind::Cone => ["半径", "高さ", ""],
            Kind::Extrude => ["高さ", "", ""],
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
        }
    }
}

/// Added to the group, carved out of it, or only painted on it (a stain: colour, no shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Add,
    Subtract,
    Paint,
}

impl Op {
    /// What the shader reads: +1 add, -1 carve, 0 paint.
    pub fn sign(self) -> f32 {
        match self {
            Op::Add => 1.0,
            Op::Subtract => -1.0,
            Op::Paint => 0.0,
        }
    }
    /// Plus becomes minus, minus becomes plus (a stain becomes plus).
    pub fn flip(self) -> Op {
        match self {
            Op::Add => Op::Subtract,
            Op::Subtract | Op::Paint => Op::Add,
        }
    }
    /// Add, then carve, then paint, then add again (the list's +/- button).
    pub fn next(self) -> Op {
        match self {
            Op::Add => Op::Subtract,
            Op::Subtract => Op::Paint,
            Op::Paint => Op::Add,
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
}

impl Default for Material {
    fn default() -> Self {
        Material::plain([0.92, 0.55, 0.42], 0.45, 0.0)
    }
}

impl Material {
    pub const fn plain(color: [f32; 3], roughness: f32, metallic: f32) -> Material {
        Material { color, roughness, metallic, glow: 0.0, glass: 0.0, iridescence: 0.0, coat: 0.0, translucency: 0.0 }
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
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub name: String,
    pub kind: Kind,
    pub size: [f32; 3],
    /// Corner rounding of boxes and extrusions, in mm.
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
}

pub const MAX_OUTLINE: usize = 16;

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
            outline: vec![[-15.0, -15.0], [15.0, -15.0], [15.0, 15.0], [-15.0, 15.0]],
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
}

impl Group {
    pub fn new(name: impl Into<String>) -> Group {
        Group { name: name.into(), op: Op::Add, blend: 0.0, visible: true, shapes: Vec::new() }
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
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    /// A scene from JSON, with every number made safe (a damaged or hostile file never panics).
    pub fn from_json(text: &str) -> Result<Scene, String> {
        let mut s: Scene = serde_json::from_str(text).map_err(|e| e.to_string())?;
        s.sanitize();
        Ok(s)
    }
    /// Clamp every number to a range the renderer and the mesher cope with.
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        fix(&mut self.melt.amount, 0.0, 10.0, 1.0);
        fix(&mut self.melt.drip, 0.0, 40.0, 0.0);
        fix(&mut self.melt.ripple, 0.3, 4.0, 1.0);
        self.env.sanitize();
        self.saved.truncate(MAX_SAVED);
        for m in &mut self.saved {
            m.sanitize();
        }
        self.groups.truncate(MAX_GROUPS);
        let mut left = MAX_SHAPES;
        for g in &mut self.groups {
            fix(&mut g.blend, 0.0, 200.0, 0.0);
            g.shapes.truncate(left);
            left -= g.shapes.len();
            for s in &mut g.shapes {
                for v in &mut s.size {
                    fix(v, 0.01, 2000.0, 10.0);
                }
                fix(&mut s.round, 0.0, 500.0, 0.0);
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
            }
        }
    }
}

pub const MAX_GROUPS: usize = 24;
pub const MAX_SHAPES: usize = 64;

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
    fn every_look_is_valid() {
        for (_, e) in Environment::looks() {
            let mut c = e.clone();
            c.sanitize();
            assert_eq!(c, e);
        }
    }
}
