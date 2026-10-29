//! Organic Furniture: editable, all-SDF stool and side table presets.
//!
//! No new mesh primitive or shader path: a rounded seat, overlapping capsule supports,
//! spherical shoulder joints and an optional pedestal foot use the existing Smooth Union
//! path. Parametric metadata belongs to the group; the actual shapes remain ordinary
//! editable / exportable YuraCAD shapes.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::app::{App, Tab};
use crate::model::{Group, Kind, MAX_GROUPS, MAX_SHAPES, Material, Pick, Shape};
use crate::sdf::{V3, rotation};
use crate::style;

const PART_PREFIX: &str = "organic-furniture:";
const MIN_WIDTH: f32 = 50.0;
const MAX_WIDTH: f32 = 1000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Design {
    #[default]
    Stool,
    SideTable,
    Pedestal,
}

impl Design {
    pub const ALL: [Design; 3] = [Design::Stool, Design::SideTable, Design::Pedestal];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stool => "まるいスツール",
            Self::SideTable => "サイドテーブル",
            Self::Pedestal => "一本脚テーブル",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Legs {
    #[default]
    Three,
    Four,
    Centre,
}

impl Legs {
    pub const ALL: [Legs; 3] = [Legs::Three, Legs::Four, Legs::Centre];
    pub fn count(self) -> usize {
        match self {
            Self::Three => 3,
            Self::Four => 4,
            Self::Centre => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Three => "3本",
            Self::Four => "4本",
            Self::Centre => "中央1本",
        }
    }
}

/// Kept in the scene JSON, separate from the generated standard SDF shapes.
/// The design is a starting point, not a locked style: all numbers can be edited afterward.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub design: Design,
    pub legs: Legs,
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub leg_diameter: f32,
    /// 0..1: rounded seat edges and the softness of the join.
    pub roundness: f32,
    /// 0..20 mm: plumps up seat/shoulder and softens the connection.
    pub puff: f32,
    pub symmetric: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::preset(Design::Stool)
    }
}

impl Settings {
    pub fn preset(design: Design) -> Settings {
        match design {
            Design::Stool => Settings {
                design,
                legs: Legs::Three,
                width: 180.0,
                depth: 170.0,
                height: 175.0,
                leg_diameter: 30.0,
                roundness: 0.8,
                puff: 5.0,
                symmetric: true,
            },
            Design::SideTable => Settings {
                design,
                legs: Legs::Four,
                width: 210.0,
                depth: 165.0,
                height: 220.0,
                leg_diameter: 22.0,
                roundness: 0.62,
                puff: 3.0,
                symmetric: true,
            },
            Design::Pedestal => Settings {
                design,
                legs: Legs::Centre,
                width: 175.0,
                depth: 175.0,
                height: 210.0,
                leg_diameter: 62.0,
                roundness: 0.95,
                puff: 7.0,
                symmetric: false,
            },
        }
    }

    /// Imported projects must never introduce NaN, overflow or huge SDF geometry.
    pub fn sanitize(&mut self) {
        fn safe(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
            if v.is_finite() { v.clamp(min, max) } else { fallback }
        }
        let def = Self::preset(self.design);
        self.width = safe(self.width, MIN_WIDTH, MAX_WIDTH, def.width);
        self.depth = safe(self.depth, MIN_WIDTH, MAX_WIDTH, def.depth);
        self.height = safe(self.height, 65.0, 1000.0, def.height);
        // Limit radii relative to seat span so the 3/4 supports remain distinguishable.
        let share = match self.legs {
            Legs::Four => 0.20,
            Legs::Three => 0.28,
            Legs::Centre => 0.40,
        };
        let max_d = (self.width.min(self.depth) * share).min(self.height * 0.42).max(8.0);
        self.leg_diameter = safe(self.leg_diameter, 8.0, max_d, def.leg_diameter.min(max_d));
        self.roundness = safe(self.roundness, 0.0, 1.0, def.roundness);
        self.puff = safe(self.puff, 0.0, 20.0, def.puff);
    }

    fn seat_thickness(self) -> f32 {
        (self.height * 0.115 + self.puff * 0.65).clamp(8.0, self.height * 0.28)
    }

    fn leg_radius(self) -> f32 {
        self.leg_diameter * 0.5
    }

    fn join(self) -> f32 {
        (1.5 + self.leg_radius() * self.roundness * 0.68 + self.puff * 0.3).min(30.0)
    }

    fn part_count(self) -> usize {
        1 + self.legs.count() * 2 + usize::from(self.legs == Legs::Centre)
    }
}

fn material_of(design: Design) -> Material {
    match design {
        Design::Stool => Material::plain([0.87, 0.62, 0.49], 0.62, 0.0),
        Design::SideTable => Material::plain([0.87, 0.83, 0.71], 0.50, 0.0),
        Design::Pedestal => Material::plain([0.65, 0.78, 0.68], 0.52, 0.0),
    }
}

fn part(kind: Kind, name: String, index: usize, pos: V3, m: Material, blend: f32) -> Shape {
    let mut sh = Shape::new(kind, pos, m);
    sh.name = name;
    sh.source = format!("{PART_PREFIX}{index}");
    sh.blend = blend;
    sh
}

/// Local (relative to the seat's XY centre) support positions; asymmetry is deterministic
/// and bounded so even a corner joint remains under the seat.
fn support_xy(p: Settings, index: usize) -> [f32; 2] {
    let (mut x, mut y) = match p.legs {
        Legs::Three => {
            let theta = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / 3.0;
            (p.width * 0.30 * theta.cos(), p.depth * 0.30 * theta.sin())
        }
        Legs::Four => {
            let sx = if index.is_multiple_of(2) { -1.0 } else { 1.0 };
            let sy = if index / 2 == 0 { -1.0 } else { 1.0 };
            (sx * p.width * 0.30, sy * p.depth * 0.30)
        }
        Legs::Centre => (0.0, 0.0),
    };
    if !p.symmetric {
        // Different fixed offsets for each leg, rather than per-frame randomness.
        let offsets = [(0.065, -0.025), (-0.048, 0.045), (0.03, 0.06), (-0.055, -0.047)];
        let j = offsets[index % offsets.len()];
        x += p.width * j.0;
        y += p.depth * j.1;
    }
    let r = p.leg_radius();
    let max_x = (p.width * 0.5 - r * 1.55).max(0.0);
    let max_y = (p.depth * 0.5 - r * 1.55).max(0.0);
    [x.clamp(-max_x, max_x), y.clamp(-max_y, max_y)]
}

/// Build a connected union using existing rounded Box, Capsule, Sphere and Cylinder SDFs.
/// Every support physically overlaps both its shoulder and the seat even when join radius
/// is at its minimum; the smin merely rounds an already connected solid.
fn generate(mut p: Settings) -> Vec<Shape> {
    p.sanitize();
    let t = p.seat_thickness();
    let radius = p.leg_radius();
    let join = p.join();
    let under = p.height - t;
    let m = material_of(p.design);
    let mut out = Vec::with_capacity(p.part_count());

    let mut seat = part(Kind::Box, "座面".into(), 0, [0.0, 0.0, p.height - t * 0.5], m, 0.0);
    seat.size = [p.width + p.puff * 0.7, p.depth + p.puff * 0.7, t];
    seat.round = (t * 0.48 * p.roundness).min(p.width.min(p.depth) * 0.22);
    out.push(seat);

    for i in 0..p.legs.count() {
        let [x, y] = support_xy(p, i);
        // Capsule spans approximately floor Z=0 up to the underside of the seat.
        // Overlap is independent of smooth-union strength (important when melt.amount=0).
        let top = under + radius * 0.72;
        let mut leg = part(Kind::Capsule, format!("脚 {}", i + 1), out.len(), [x, y, top * 0.5], m, join);
        leg.size = [radius, (top - radius * 2.0).max(0.1), 1.0];
        out.push(leg);

        let mut shoulder = part(Kind::Sphere, format!("脚の付け根 {}", i + 1), out.len(), [x, y, under - radius * 0.08], m, join);
        shoulder.size = [radius * (1.35 + 0.16 * p.roundness) + p.puff * 0.28, 1.0, 1.0];
        out.push(shoulder);
    }

    if p.legs == Legs::Centre {
        let foot_height = (p.leg_diameter * 0.42).clamp(10.0, 45.0);
        let mut foot = part(Kind::Cylinder, "一本脚の土台".into(), out.len(), [support_xy(p, 0)[0], support_xy(p, 0)[1], foot_height * 0.5], m, join);
        foot.size = [p.width.min(p.depth) * 0.29, foot_height, 1.0];
        foot.round = foot_height * 0.46 * p.roundness;
        out.push(foot);
    }
    out
}

/// The tags mean the parametric-owned prefix can be safely replaced without deleting
/// shapes somebody added afterward. If components were reordered/deleted, refuse to
/// regenerate and keep the user's geometry intact.
pub fn is_generated_group(g: &Group, p: &Settings) -> bool {
    g.shapes.len() >= p.part_count() && g.shapes.iter().take(p.part_count()).enumerate().all(|(i, s)| s.source == format!("{PART_PREFIX}{i}"))
}

fn transformed_parts(old: Settings, new: Settings, previous: &Shape) -> Vec<Shape> {
    let mut parts = generate(new);
    // Moving/rotating/scaling a whole group with the existing gizmo must not cause
    // its next parameter adjustment to snap back to the world origin.
    let old_seat_z = old.height - old.seat_thickness() * 0.5;
    let new_seat_z = new.height - new.seat_thickness() * 0.5;
    let pivot = previous.pos;
    let rot = rotation(previous.rot);
    let scale = previous.scale;
    let rotate = |q: V3| -> V3 {
        [
            rot[0][0] * q[0] + rot[0][1] * q[1] + rot[0][2] * q[2],
            rot[1][0] * q[0] + rot[1][1] * q[1] + rot[1][2] * q[2],
            rot[2][0] * q[0] + rot[2][1] * q[1] + rot[2][2] * q[2],
        ]
    };
    let z_shift = rotate([0.0, 0.0, (new_seat_z - old_seat_z) * scale[2]]);
    let new_pivot = [pivot[0] + z_shift[0], pivot[1] + z_shift[1], pivot[2] + z_shift[2]];
    for sh in &mut parts {
        let offset = [sh.pos[0] * scale[0], sh.pos[1] * scale[1], (sh.pos[2] - new_seat_z) * scale[2]];
        let q = rotate(offset);
        sh.pos = [new_pivot[0] + q[0], new_pivot[1] + q[1], new_pivot[2] + q[2]];
        sh.rot = previous.rot;
        sh.scale = previous.scale;
    }
    parts
}

/// Apply only when user edited a parameter; no SDF / scene rebuild on unchanged UI frames.
pub fn update_group(g: &mut Group, mut settings: Settings) -> Result<(), &'static str> {
    let old = g.organic_furniture.ok_or("このグループは家具プリセットではありません")?;
    if !is_generated_group(g, &old) {
        return Err("パーツの順序や数が変わっています。もとの形を残すため、自動更新はしません");
    }
    settings.sanitize();
    if settings == old {
        return Ok(());
    }
    let seat = g.shapes[0].clone();
    let mut made = transformed_parts(old, settings, &seat);
    for (index, sh) in made.iter_mut().enumerate() {
        if let Some(prev) = g.shapes.get(index).filter(|prev| prev.source == sh.source) {
            sh.material = prev.material;
            sh.uid = prev.uid;
        } else {
            sh.material = seat.material;
        }
    }
    // Replace only our tagged parts; extra shapes appended by the person are untouched.
    let extras = g.shapes.split_off(old.part_count());
    made.extend(extras);
    g.shapes = made;
    g.organic_furniture = Some(settings);
    Ok(())
}

/// Add a preset as one SDF group (so exporting the selected group is one connected piece).
pub fn insert(app: &mut App, ctx: &egui::Context, design: Design) {
    let mut p = Settings::preset(design);
    p.sanitize();
    if app.scene.groups.len() >= MAX_GROUPS {
        app.say(ctx, format!("グループは{MAX_GROUPS}個までです"));
        return;
    }
    if app.scene.shape_count().saturating_add(p.part_count()) > MAX_SHAPES {
        app.say(ctx, format!("形は{MAX_SHAPES}個までです"));
        return;
    }
    let mut gr = Group::new(format!("Organic Furniture・{}", design.label()));
    gr.organic_furniture = Some(p);
    gr.shapes = generate(p);
    let at = [app.cam.target[0] + p.width * 0.60, app.cam.target[1]];
    for sh in &mut gr.shapes {
        sh.pos[0] += at[0];
        sh.pos[1] += at[1];
    }
    let selected = app.scene.groups.len();
    app.scene.groups.push(gr);
    app.sel = Some(Pick::Group(selected));
    app.tab = Tab::Shape;
    app.frame_view(true);
}

/// The same settings drive the live viewport, saved file and exported mesh.
pub fn inspector(app: &mut App, ui: &mut Ui, g: usize) {
    let Some(before) = app.scene.groups.get(g).and_then(|gr| gr.organic_furniture) else { return };
    ui.separator();
    ui.label(RichText::new("Organic Furniture｜有機家具").strong().size(13.0));
    ui.label(RichText::new("数字を動かすと形が変わります。すべてmm単位です。").size(11.0).color(style::DIM));
    let mut p = before;

    ui.label(RichText::new("デザインの出発点").color(style::DIM).size(11.5));
    ui.horizontal_wrapped(|ui| {
        for design in Design::ALL {
            if style::pill(ui, design.label(), p.design == design).clicked() && p.design != design {
                p = Settings::preset(design);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("脚の本数");
        for mode in Legs::ALL {
            if style::pill(ui, mode.label(), p.legs == mode).clicked() {
                p.legs = mode;
            }
        }
    });
    egui::Grid::new("organic_furniture_dims").num_columns(2).spacing([8.0, 5.0]).show(ui, |ui| {
        for (label, value, min, max) in [
            ("座面の幅", &mut p.width, MIN_WIDTH, MAX_WIDTH),
            ("座面の奥行", &mut p.depth, MIN_WIDTH, MAX_WIDTH),
            ("高さ", &mut p.height, 65.0, 1000.0),
            ("脚の太さ", &mut p.leg_diameter, 8.0, 300.0),
        ] {
            ui.label(label);
            ui.add(egui::DragValue::new(value).speed(0.8).range(min..=max).suffix(" mm").max_decimals(1));
            ui.end_row();
        }
    });
    style::labeled_slider(ui, "全体の丸み", &mut p.roundness, 0.0..=1.0, "0は控えめ、1は角と接合部がなめらか");
    style::labeled_slider(ui, "膨張量", &mut p.puff, 0.0..=20.0, "座面と脚の付け根をぷっくりさせる (mm)。指定幅より少し大きくなります");
    style::checkbox(ui, &mut p.symmetric, "左右対称").on_hover_text("オフにすると、脚を少しずらした非対称デザインになります");
    ui.label(RichText::new("座面と脚は重ねて滑らかに結合します。パーツ個別の変形は、数値を再変更すると更新されます。").size(11.0).color(style::DIM));

    p.sanitize();
    if p != before {
        let resulting_shapes = app.scene.shape_count().saturating_sub(before.part_count()).saturating_add(p.part_count());
        if resulting_shapes > MAX_SHAPES {
            app.notice = Some(format!("形は{MAX_SHAPES}個までです。ほかの形を減らしてください"));
        } else if let Some(group) = app.scene.groups.get_mut(g)
            && let Err(e) = update_group(group, p)
        {
            app.notice = Some(e.to_string());
        }
    }
    ui.label(
        RichText::new("3Dプリントした形や実物の強度・転倒安全性は保証しません。家具として使う前に別途構造検証が必要です。")
            .size(11.0)
            .color(style::DIM),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scene;
    use crate::sdf::Field;

    fn sample_group(p: Settings) -> Group {
        let mut g = Group::new("furniture");
        g.shapes = generate(p);
        g.organic_furniture = Some(p);
        g
    }

    #[test]
    fn all_three_defaults_are_editable_and_finite() {
        for design in Design::ALL {
            let p = Settings::preset(design);
            let g = sample_group(p);
            assert_eq!(g.shapes.len(), p.part_count());
            assert!(is_generated_group(&g, &p));
            for s in &g.shapes {
                assert!(s.pos.iter().chain(s.size.iter()).all(|v| v.is_finite()));
                assert_eq!(s.op, crate::model::Op::Add);
            }
        }
    }

    #[test]
    fn support_to_seat_is_inside_single_connected_sdf() {
        for legs in Legs::ALL {
            for symmetric in [true, false] {
                let p = Settings { legs, symmetric, roundness: 0.0, puff: 0.0, ..Settings::preset(Design::Stool) };
                let g = sample_group(p);
                let field = Field::new(&Scene { groups: vec![g], ..Scene::default() });
                let under = p.height - p.seat_thickness();
                // Walk vertically through each support/shoulder/seat junction.
                // If anything was disconnected this sequence would turn positive.
                for i in 0..legs.count() {
                    let [x, y] = support_xy(p, i);
                    for step in 0..=24 {
                        let z = under - p.leg_radius() + step as f32 * (p.leg_radius() * 2.0 / 24.0);
                        assert!(field.distance([x, y, z]) < -0.05, "gap at leg {i}, z={z}, {legs:?} symmetric={symmetric}");
                    }
                }
            }
        }
    }

    #[test]
    fn edits_move_existing_gizmos_and_survive_save_reload() {
        let old = Settings::preset(Design::Stool);
        let mut g = sample_group(old);
        for s in &mut g.shapes {
            s.pos[0] += 55.0;
            s.pos[1] -= 18.0;
        }
        let moved_seat = g.shapes[0].pos;
        let p = Settings { width: 210.0, height: 195.0, legs: Legs::Four, symmetric: false, ..old };
        update_group(&mut g, p).unwrap();
        assert_eq!(g.shapes.len(), 9);
        assert_eq!(g.shapes[0].pos[0], moved_seat[0]);
        assert_eq!(g.shapes[0].pos[1], moved_seat[1]);
        let mut scene = Scene { groups: vec![g], ..Scene::default() };
        let restored = Scene::from_json(&scene.to_json()).unwrap();
        assert_eq!(scene, restored);
        let mut n = restored.groups[0].clone();
        let mut second = p;
        second.puff = 10.0;
        update_group(&mut n, second).unwrap();
        assert!(n.shapes[0].size[0] > scene.groups[0].shapes[0].size[0]);
        scene.groups[0] = n;
        assert_eq!(Scene::from_json(&scene.to_json()).unwrap(), scene);
    }

    #[test]
    fn parameter_changes_preserve_extra_shapes_and_refuse_missing_parts() {
        let mut g = sample_group(Settings::preset(Design::Stool));
        let extra = Shape::new(Kind::Sphere, [9.0, 0.0, 4.0], Material::default());
        g.shapes.push(extra.clone());
        let mut new = g.organic_furniture.unwrap();
        new.legs = Legs::Centre;
        update_group(&mut g, new).unwrap();
        assert_eq!(g.shapes.last(), Some(&extra));
        let mut broken = g.clone();
        broken.shapes.remove(0);
        let before = broken.shapes.clone();
        assert!(update_group(&mut broken, Settings { height: 240.0, ..new }).is_err());
        assert_eq!(broken.shapes, before);
    }

    #[test]
    fn a_transformed_group_keeps_its_orientation_when_dimensions_change() {
        let p = Settings::preset(Design::Stool);
        let mut g = sample_group(p);
        let pivot = g.shapes[0].pos;
        let turn = crate::sdf::rotation([0.0, 0.0, 90.0]);
        for sh in &mut g.shapes {
            let rel = [sh.pos[0] - pivot[0], sh.pos[1] - pivot[1], sh.pos[2] - pivot[2]];
            sh.pos = [
                pivot[0] + turn[0][0] * rel[0] + turn[0][1] * rel[1] + turn[0][2] * rel[2],
                pivot[1] + turn[1][0] * rel[0] + turn[1][1] * rel[1] + turn[1][2] * rel[2],
                pivot[2] + turn[2][0] * rel[0] + turn[2][1] * rel[1] + turn[2][2] * rel[2],
            ];
            sh.rot = [0.0, 0.0, 90.0];
            sh.scale = [1.25, 1.25, 1.25];
        }
        let mut next = p;
        next.width = 225.0;
        update_group(&mut g, next).unwrap();
        assert_eq!(g.shapes[0].pos, pivot);
        assert!(g.shapes.iter().all(|sh| sh.rot == [0.0, 0.0, 90.0] && sh.scale == [1.25; 3]));
    }

    #[test]
    fn damaged_furniture_tags_detach_without_discarding_geometry() {
        let mut g = sample_group(Settings::preset(Design::Stool));
        g.shapes[0].source.clear();
        let original = g.shapes.clone();
        let scene = Scene { groups: vec![g], ..Scene::default() };
        let (opened, notes) = Scene::from_json_notes(&scene.to_json()).unwrap();
        assert!(opened.groups[0].organic_furniture.is_none());
        assert_eq!(opened.groups[0].shapes, original);
        assert!(notes.iter().any(|n| n.contains("家具プリセット")));
    }

    #[test]
    fn invalid_import_settings_are_clamped() {
        let mut p = Settings {
            width: f32::NAN,
            depth: -12.0,
            height: f32::INFINITY,
            leg_diameter: 999_999.0,
            roundness: f32::NAN,
            puff: -5.0,
            ..Settings::default()
        };
        p.sanitize();
        assert!(p.width.is_finite() && p.width >= MIN_WIDTH);
        assert!(p.depth >= MIN_WIDTH && p.height <= 1000.0);
        assert!(p.leg_diameter <= p.height * 0.42);
        assert!((0.0..=1.0).contains(&p.roundness) && (0.0..=20.0).contains(&p.puff));
    }

    #[test]
    fn app_undo_redo_remains_one_transaction_for_parameter_edits() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        insert(&mut app, &ctx, Design::Stool);
        app.settle(&ctx);
        let baseline = app.scene.clone();
        let g = app.scene.groups.len() - 1;
        let old = app.scene.groups[g].organic_furniture.unwrap();
        update_group(&mut app.scene.groups[g], Settings { height: old.height + 12.0, ..old }).unwrap();
        app.settle(&ctx);
        let modified = app.scene.clone();
        assert_ne!(modified, baseline);
        app.undo();
        assert_eq!(app.scene, baseline);
        app.redo();
        assert_eq!(app.scene, modified);
    }

    /// Exercise the REAL export SDF mesher, rather than trusting a visual-only preview.
    #[test]
    fn selected_furniture_exports_one_watertight_connected_mesh() {
        use crate::export_mesh::{Progress, mesh_sparse};
        use solidcraft_geom::melt;
        for design in Design::ALL {
            let p = Settings::preset(design);
            let g = sample_group(p);
            let scene = Scene { groups: vec![g], ..Scene::default() };
            let selected = crate::export_job::prepare(&scene, Some(Pick::Group(0)), crate::export_job::Scope::Selected, false).unwrap();
            let field = Field::new(&selected);
            let prog = Progress::new();
            let span = (p.width.max(p.depth).max(p.height) + 45.0) * 0.5;
            // Centre the slice of the world to include Z=0..height.
            let lo = [-f64::from(span), -f64::from(span), -20.0];
            let hi = [f64::from(span), f64::from(span), f64::from(p.height + 30.0)];
            let meshed = mesh_sparse(&|q| field.distance(q), lo, hi, 3.0, 1, &prog.slice(0.0, 1.0)).unwrap();
            assert!(!meshed.mesh.triangles.is_empty(), "{design:?}: no triangles");
            assert!(melt::is_watertight(&meshed.mesh), "{design:?}: non-watertight mesh");
            // Connected components, using shared vertex indices.
            let n = meshed.mesh.positions.len();
            let mut parent: Vec<usize> = (0..n).collect();
            fn root(p: &mut [usize], mut k: usize) -> usize {
                while p[k] != k {
                    p[k] = p[p[k]];
                    k = p[k];
                }
                k
            }
            for tri in &meshed.mesh.triangles {
                let a = root(&mut parent, tri[0] as usize);
                for &v in &tri[1..] {
                    let b = root(&mut parent, v as usize);
                    parent[b] = a;
                }
            }
            let active: std::collections::HashSet<usize> =
                meshed.mesh.triangles.iter().flat_map(|t| t.iter().copied()).map(|v| root(&mut parent, v as usize)).collect();
            assert_eq!(active.len(), 1, "{design:?}: disconnected exported shells");

            // Real file encoders + importers; not only a mesh preview.
            let surf = crate::export_formats::Surface {
                mesh: meshed.mesh.clone(),
                look: crate::export_formats::Look::uniform(crate::export_formats::Mat8 { rgb: [210, 160, 130], rough: 125, metal: 0 }),
            };
            let bytes = crate::export_formats::threemf(&surf, "Organic Furniture", crate::export_formats::Units::Mm).unwrap();
            let back = crate::import_3mf::read_3mf(&bytes).unwrap();
            assert_eq!(back.triangles.len(), surf.mesh.triangles.len(), "3MF roundtrip {design:?}");
            let glb = crate::export_formats::glb(&surf, "Organic Furniture").unwrap();
            let back = crate::import_glb::read_glb(&glb).unwrap();
            assert_eq!(back.triangles.len(), surf.mesh.triangles.len(), "GLB roundtrip {design:?}");
        }
    }
}
