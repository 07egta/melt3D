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

/// Added to the group, or carved out of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Add,
    Subtract,
}

impl Op {
    pub fn sign(self) -> f32 {
        match self {
            Op::Add => 1.0,
            Op::Subtract => -1.0,
        }
    }
    pub fn flip(self) -> Op {
        match self {
            Op::Add => Op::Subtract,
            Op::Subtract => Op::Add,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub color: [f32; 3],
    /// 0 = mirror smooth, 1 = chalky.
    pub roughness: f32,
    /// 0 = plastic, 1 = metal.
    pub metallic: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material { color: [0.92, 0.55, 0.42], roughness: 0.45, metallic: 0.0 }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub groups: Vec<Group>,
    #[serde(default)]
    pub melt: Melt,
}

impl Default for Scene {
    fn default() -> Self {
        let mut g = Group::new("グループ 1");
        g.shapes.push(Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material::default()));
        Scene { groups: vec![g], melt: Melt::default() }
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
                for c in &mut s.material.color {
                    fix(c, 0.0, 1.0, 0.8);
                }
                fix(&mut s.material.roughness, 0.0, 1.0, 0.5);
                fix(&mut s.material.metallic, 0.0, 1.0, 0.0);
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
}
