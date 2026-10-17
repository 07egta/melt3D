//! Parametric Designer Bag library models.
//!
//! All geometry uses MELTCAD's existing SDF primitives, Loft/Extrude, Boolean cuts and
//! editable Curve paths. The parameter record is stored on the Group, while each part
//! stays a normal, independently editable shape. No baked display-only mesh is used.

use serde::{Deserialize, Serialize};

use crate::model::{Group, Kind, Material, Op, Shape};
use crate::style;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BagStyle {
    #[default]
    Tote,
    HalfMoon,
    Origami,
}

impl BagStyle {
    pub const ALL: [Self; 3] = [Self::Tote, Self::HalfMoon, Self::Origami];

    pub fn name(self) -> &'static str {
        match self {
            Self::Tote => "デザイナーバッグ・トート",
            Self::HalfMoon => "デザイナーバッグ・半円",
            Self::Origami => "デザイナーバッグ・折り紙",
        }
    }
}

/// Dimensions are millimetres. The XY offset is kept so editing a placed library
/// model never teleports it back to the scene origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BagConfig {
    pub style: BagStyle,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub wall: f32,
    /// Rise above the bag rim.
    pub handle_length: f32,
    /// Handle diameter, not radius.
    pub handle_thickness: f32,
    /// Arch profile exponent; larger values give a narrower, taller crown.
    pub handle_curve: f32,
    pub origin_xy: [f32; 2],
}

impl Default for BagConfig {
    fn default() -> Self {
        Self {
            style: BagStyle::Tote,
            width: 116.0,
            height: 80.0,
            depth: 42.0,
            wall: 2.4,
            handle_length: 34.0,
            handle_thickness: 5.6,
            handle_curve: 0.9,
            origin_xy: [0.0, 0.0],
        }
    }
}

impl BagConfig {
    pub fn for_style(style: BagStyle) -> Self {
        let mut p = Self { style, ..Self::default() };
        match style {
            BagStyle::Tote => {}
            BagStyle::HalfMoon => {
                p.width = 128.0;
                p.height = 75.0;
                p.depth = 34.0;
                p.handle_length = 29.0;
            }
            BagStyle::Origami => {
                p.width = 110.0;
                p.height = 88.0;
                p.depth = 42.0;
                p.handle_length = 32.0;
            }
        }
        p
    }

    pub fn sanitize(&mut self) {
        fn clamp(v: &mut f32, lo: f32, hi: f32, fallback: f32) {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        }
        clamp(&mut self.width, 35.0, 280.0, 116.0);
        clamp(&mut self.height, 30.0, 240.0, 80.0);
        clamp(&mut self.depth, 18.0, 150.0, 42.0);
        clamp(&mut self.wall, 1.2, 8.0, 2.4);
        self.wall = self.wall.min(self.depth * 0.18).min(self.width * 0.08);
        clamp(&mut self.handle_length, 12.0, 150.0, 34.0);
        clamp(&mut self.handle_thickness, 2.4, 15.0, 5.6);
        self.handle_thickness = self.handle_thickness.min(self.depth * 0.3);
        clamp(&mut self.handle_curve, 0.55, 2.2, 0.9);
        for v in &mut self.origin_xy {
            clamp(v, -4000.0, 4000.0, 0.0);
        }
    }
}

fn colour() -> Material {
    Material::plain([0.70, 0.60, 0.47], 0.52, 0.04)
}

fn trim_colour() -> Material {
    Material::plain([0.48, 0.36, 0.26], 0.43, 0.02)
}

/// Eight corners with short chamfers: stable, closed profiles for Loft.
fn chamfered_rect(width: f32, depth: f32, chamfer: f32) -> Vec<[f32; 2]> {
    let (w, d) = (width * 0.5, depth * 0.5);
    let r = chamfer.min(w * 0.25).min(d * 0.25);
    vec![
        [-w + r, -d],
        [w - r, -d],
        [w, -d + r],
        [w, d - r],
        [w - r, d],
        [-w + r, d],
        [-w, d - r],
        [-w, -d + r],
    ]
}

/// Accordion-like perimeter; the changing front/back notches create crisp
/// faceted pleats while preserving a single continuous wall.
fn folded_outline(width: f32, depth: f32, upper: bool) -> Vec<[f32; 2]> {
    let (w, d) = (width * 0.5, depth * 0.5);
    let (outer, inner) = if upper { (0.88, 0.79) } else { (0.76, 0.90) };
    vec![
        [-w, -d * outer],
        [-w * 0.72, -d],
        [-w * 0.24, -d * inner],
        [w * 0.24, -d],
        [w * 0.72, -d * inner],
        [w, -d * outer],
        [w, d * outer],
        [w * 0.72, d],
        [w * 0.24, d * inner],
        [-w * 0.24, d],
        [-w * 0.72, d * inner],
        [-w, d * outer],
    ]
}

/// A D-shaped silhouette in the local XY plane. Rotating 90 degrees about X
/// makes the flat edge the top opening and the ellipse the rounded base.
fn moon_outline(width: f32, height: f32) -> Vec<[f32; 2]> {
    (0..=16)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 - i as f32 / 16.0);
            [width * 0.5 * a.cos(), height * (0.5 - a.sin())]
        })
        .collect()
}

fn body(p: &BagConfig) -> Shape {
    let xy = p.origin_xy;
    let mut s = match p.style {
        BagStyle::HalfMoon => {
            let mut s = Shape::new(Kind::Extrude, [xy[0], xy[1], p.height * 0.5], colour());
            s.size[0] = p.depth;
            s.rot[0] = 90.0;
            s.outline = moon_outline(p.width, p.height);
            s
        }
        BagStyle::Tote | BagStyle::Origami => {
            let mut s = Shape::new(Kind::Loft, [xy[0], xy[1], p.height * 0.5], colour());
            s.size[0] = p.height;
            match p.style {
                BagStyle::Tote => {
                    s.outline = chamfered_rect(p.width * 0.87, p.depth * 0.82, p.wall * 1.6);
                    s.outline2 = chamfered_rect(p.width, p.depth, p.wall * 1.7);
                    s.round = (p.wall * 0.65).min(2.0);
                }
                BagStyle::Origami => {
                    s.outline = folded_outline(p.width * 0.78, p.depth * 0.80, false);
                    s.outline2 = folded_outline(p.width, p.depth, true);
                    s.round = 0.35;
                }
                BagStyle::HalfMoon => {}
            }
            s
        }
    };
    s.name = "本体（中空）".into();
    s.shell = p.wall;
    s.blend = 0.0;
    s
}

/// The base is a distinct solid plate, overlapping the wall at its lower edge.
/// This reinforces the floor without filling the cavity.
fn bottom(p: &BagConfig) -> Shape {
    let (w, d) = match p.style {
        BagStyle::Tote => (p.width * 0.87, p.depth * 0.82),
        BagStyle::HalfMoon => (p.width * 0.16, p.depth),
        BagStyle::Origami => (p.width * 0.76, p.depth * 0.77),
    };
    let mut s = Shape::new(Kind::Box, [p.origin_xy[0], p.origin_xy[1], p.wall * 0.58], trim_colour());
    s.name = "底面".into();
    s.size = [(w - p.wall * 0.55).max(3.0), (d - p.wall * 0.55).max(3.0), p.wall * 1.16];
    s.round = (p.wall * 0.2).min(0.9);
    s.blend = p.wall * 0.4;
    s
}

/// Remove the upper cap of the shell. A real Boolean opening, not just a
/// dark rectangle: the cutter overlaps the already hollow interior.
fn mouth_cut(p: &BagConfig) -> Shape {
    let mut s = Shape::new(
        Kind::Box,
        [p.origin_xy[0], p.origin_xy[1], p.height + p.wall * 1.25],
        colour(),
    );
    s.name = "開口部（くり抜き）".into();
    s.op = Op::Subtract;
    s.blend = 0.0;
    let opening_depth = if p.style == BagStyle::Origami {
        // Folded outer edges are indented: keep a strong, continuous upper rim.
        p.depth * 0.60
    } else {
        p.depth - p.wall * 3.3
    };
    s.size = [
        p.width - p.wall * 3.3,
        opening_depth,
        p.wall * 7.5,
    ];
    s.round = p.wall.min(2.0);
    s
}

fn handle(p: &BagConfig, front: bool) -> Shape {
    let sign = if front { -1.0 } else { 1.0 };
    let y = match p.style {
        BagStyle::Origami => sign * (p.depth * 0.395),
        _ => sign * (p.depth * 0.5 - p.wall * 0.8),
    };
    let mut s = Shape::new(Kind::Curve, [p.origin_xy[0], p.origin_xy[1], 0.0], trim_colour());
    s.name = if front { "持ち手・前" } else { "持ち手・後" }.into();
    s.params = [0.0, 0.78, 0.0, 1.0];
    s.blend = (p.wall * 0.65).min(2.4);
    let r = p.handle_thickness * 0.5;
    let embed = (r * 0.85).max(p.wall * 0.75);
    let span = p.width * 0.315;
    s.path = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            let a = std::f32::consts::PI * t;
            let z = p.height - embed + (p.handle_length + embed) * a.sin().max(0.0).powf(p.handle_curve);
            [-span + 2.0 * span * t, y, z, r]
        })
        .collect();
    s
}

/// The shape order is significant: the opening cuts body+floor, but not the
/// handles added afterwards. Each resulting shape remains editable in the normal inspector.
pub fn make_group(mut config: BagConfig) -> Group {
    config.sanitize();
    let mut g = Group::new(config.style.name());
    g.shapes = vec![
        body(&config),
        bottom(&config),
        mouth_cut(&config),
        handle(&config, true),
        handle(&config, false),
    ];
    g.bag = Some(config);
    g
}

/// Regenerate only the five parametric parts. Keep the group identity, position,
/// visibility, user colour edits and UIDs so other scene features remain stable.
pub fn rebuild(g: &mut Group, mut config: BagConfig) {
    // The ordinary move gizmo changes each shape's world position. Recover
    // that translation before regenerating, otherwise editing one dimension
    // would snap a placed or moved bag back to its original location.
    if let Some(body) = g.shapes.iter().find(|s| s.name == "本体（中空）") {
        config.origin_xy = [body.pos[0], body.pos[1]];
    }
    config.sanitize();
    let mut fresh = make_group(config.clone());
    for s in &mut fresh.shapes {
        if let Some(old) = g.shapes.iter().find(|old| old.name == s.name && old.op == s.op) {
            s.uid = old.uid;
            if s.op != Op::Subtract {
                s.material = old.material;
            }
        }
    }
    g.shapes = fresh.shapes;
    g.bag = Some(config);
}

/// Model library entries. A thumbnail is generated using the same true geometry.
pub fn library_groups() -> [(String, Vec<String>, Group); 3] {
    BagStyle::ALL.map(|variant| {
        let group = make_group(BagConfig::for_style(variant));
        let tags = match variant {
            BagStyle::Tote => vec!["Designer Bag", "tote", "トート", "バッグ", "handbag"],
            BagStyle::HalfMoon => vec!["Designer Bag", "half moon", "semicircle", "半円", "バッグ"],
            BagStyle::Origami => vec!["Designer Bag", "origami", "fold", "折り紙", "バッグ"],
        };
        (variant.name().to_string(), tags.into_iter().map(String::from).collect(), group)
    })
}

/// Editable parametric controls appear when the bag group is selected. Normal
/// inspector controls on individual shapes still work independently.
pub fn inspector(ui: &mut egui::Ui, g: &mut Group) {
    let Some(original) = g.bag.clone() else { return };
    let mut c = original.clone();
    ui.add_space(5.0);
    ui.label(egui::RichText::new("Designer Bag | 寸法・形状").strong());
    ui.horizontal_wrapped(|ui| {
        for (style, label) in [
            (BagStyle::Tote, "トート"),
            (BagStyle::HalfMoon, "半円"),
            (BagStyle::Origami, "折り紙"),
        ] {
            if style::pill(ui, label, c.style == style).clicked() {
                c.style = style;
            }
        }
    });
    style::labeled_slider(ui, "横幅", &mut c.width, 35.0..=280.0, "バッグ本体の幅（mm）");
    style::labeled_slider(ui, "高さ", &mut c.height, 30.0..=240.0, "持ち手を除く高さ（mm）");
    style::labeled_slider(ui, "奥行き", &mut c.depth, 18.0..=150.0, "本体の前後の厚み（mm）");
    style::labeled_slider(ui, "壁厚", &mut c.wall, 1.2..=8.0, "中空にした本体の壁の厚さ（mm）");
    style::labeled_slider(ui, "持ち手長さ", &mut c.handle_length, 12.0..=150.0, "バッグ上端からの持ち手の立ち上がり（mm）");
    style::labeled_slider(ui, "持ち手太さ", &mut c.handle_thickness, 2.4..=15.0, "持ち手の直径（mm）");
    style::labeled_slider(ui, "持ち手曲率", &mut c.handle_curve, 0.55..=2.2, "アーチの膨らみ方");
    c.sanitize();
    if c != original {
        rebuild(g, c.clone());
    }
    if c.wall < 1.6 || c.handle_thickness < 3.0 {
        ui.label(egui::RichText::new("薄い部分があります。印刷前に肉厚を確認してください").color(style::KEEP).size(11.5));
    } else {
        ui.label(egui::RichText::new(format!("設計壁厚 {:.1} mm（実際の最薄部はプリントチェックで確認）", c.wall)).color(style::DIM).size(11.5));
    }
    if style::pill(ui, "プリントチェック", false).on_hover_text("シーン全体のメッシュ肉厚・穴・張り出しを確認").clicked() {
        crate::export_dialog::open_check(ui.ctx());
    }
    ui.label(egui::RichText::new("パーツ別の編集は左のツリーで「本体」「底面」「持ち手」を選択。寸法を変えるとパーツ形状を再生成します。").color(style::DIM).size(11.5));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_formats::Format;
    use crate::export_job::{self, Options};
    use crate::export_mesh::Progress;
    use crate::model::Scene;
    use crate::sdf::Field;

    fn scene(p: BagConfig) -> Scene {
        Scene { groups: vec![make_group(p)], ..Scene::default() }
    }

    #[test]
    fn all_styles_are_editable_and_have_a_real_open_top_and_bottom() {
        for style in BagStyle::ALL {
            let p = BagConfig::for_style(style);
            let sc = scene(p.clone());
            let g = &sc.groups[0];
            assert_eq!(g.shapes.len(), 5);
            assert_eq!(g.shapes[0].shell, p.wall);
            assert_eq!(g.shapes[1].name, "底面");
            assert_eq!(g.shapes[2].op, Op::Subtract);
            assert!(g.shapes[3..].iter().all(|s| s.kind == Kind::Curve && s.path.len() == 9));
            let f = Field::new(&sc);
            assert!(f.distance([0.0, 0.0, p.height * 0.45]) > 0.1, "{style:?}: cavity must be empty");
            assert!(f.distance([0.0, 0.0, p.height + 1.0]) > 0.1, "{style:?}: mouth must be open");
            assert!(f.distance([0.0, 0.0, p.wall * 0.6]) < 0.0, "{style:?}: floor must remain solid");
            assert!(f.distance([p.width * 0.85, 0.0, p.height * 0.45]) > 0.0, "{style:?}: outside must be empty");
        }
    }

    #[test]
    fn edits_round_trip_save_without_losing_position_or_manual_material() {
        let mut p = BagConfig::for_style(BagStyle::Origami);
        p.origin_xy = [315.0, -120.0];
        let mut g = make_group(p.clone());
        g.shapes[3].material = Material::plain([0.2, 0.4, 0.8], 0.6, 0.0);
        let handle_colour = g.shapes[3].material;
        p.width = 135.0;
        p.handle_curve = 1.8;
        rebuild(&mut g, p.clone());
        assert_eq!(g.shapes[3].material, handle_colour);
        assert!((g.shapes[0].pos[0] - 315.0).abs() < 1e-3);
        let sc = Scene { groups: vec![g], ..Scene::default() };
        let json = sc.to_json();
        let read = Scene::from_json(&json).expect("scene reload");
        assert_eq!(read.groups[0].bag.as_ref(), Some(&p));
        assert_eq!(read.groups[0].shapes[3].material, handle_colour);
        let f = Field::new(&read);
        assert!(f.distance([315.0, -120.0, p.height + 2.0]) > 0.0);
    }

    #[test]
    fn changing_dimensions_after_moving_keeps_the_bag_in_place() {
        let mut g = make_group(BagConfig::default());
        for s in &mut g.shapes {
            s.pos[0] += 270.0;
            s.pos[1] -= 70.0;
        }
        let mut edited = g.bag.clone().expect("bag settings");
        edited.width = 140.0;
        rebuild(&mut g, edited);
        assert!((g.shapes[0].pos[0] - 270.0).abs() < 1e-3);
        assert!((g.shapes[0].pos[1] + 70.0).abs() < 1e-3);
        assert_eq!(g.bag.as_ref().map(|b| b.origin_xy), Some([270.0, -70.0]));
    }

    #[test]
    fn bad_values_clamp_before_geometry_and_scene_sanitization() {
        let mut p = BagConfig::default();
        p.width = f32::NAN;
        p.depth = 0.0;
        p.wall = f32::INFINITY;
        p.handle_curve = -50.0;
        p.origin_xy = [f32::INFINITY, -999999.0];
        let mut g = make_group(p);
        assert!(g.bag.as_ref().is_some_and(|p| p.width.is_finite() && p.wall > 1.0));
        let before = g.clone();
        let mut sc = Scene { groups: vec![std::mem::replace(&mut g, Group::new("moved"))], ..Scene::default() };
        sc.sanitize();
        assert_eq!(sc.groups[0], before, "generated dimensions must already be valid");
    }

    #[test]
    fn tote_exports_through_existing_3mf_and_obj_mesh_pipeline() {
        let sc = scene(BagConfig::for_style(BagStyle::Tote));
        for format in [Format::ThreeMf, Format::Obj] {
            let o = Options { format, tolerance: 1.0, ..Options::default() };
            let build = export_job::build(&sc, None, &o, &Progress::new()).expect("mesh generation");
            assert!(build.closed, "{format:?}: bag must form a closed printable skin");
            assert!(build.tris > 100);
            let path = std::env::temp_dir().join(format!("designer_bag_test.{}", format.ext()));
            let files = export_job::serialize(&build, &o, &path).expect("serialise export");
            assert!(!files.is_empty());
            assert!(files.iter().all(|(_, bytes)| !bytes.is_empty()));
        }
    }
}
