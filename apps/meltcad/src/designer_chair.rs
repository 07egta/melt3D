//! Parametric Designer Chair presets. Geometry is ONLY existing MeltCAD SDF primitives
//! (Loft, Extrude, Curve, Cylinder), so the normal viewport, undo, save, and
//! STL/3MF/OBJ exporters all see the exact same editable shapes.
//!
//! A whole-chair slider regenerates marked procedural parts; manually added
//! non-procedural shapes are not removed. Individual shape edits are left as-is
//! until the user explicitly changes a whole-chair parameter.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::model::{Group, Kind, Material, Shape};
use crate::style;

pub const GENERATOR_TAG: &str = "meltcad:designer-chair:v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    Shell,
    #[default]
    FourLeg,
    Monocoque,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Shell, Style::FourLeg, Style::Monocoque];

    pub fn label(self) -> &'static str {
        match self {
            Style::Shell => "シェル型",
            Style::FourLeg => "4本脚型",
            Style::Monocoque => "一体成形型",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Shell => "Designer Chair / Shell",
            Style::FourLeg => "Designer Chair / Four-Leg",
            Style::Monocoque => "Designer Chair / Monocoque",
        }
    }
}

/// Dimensions in millimetres; the back angle tilts backwards from vertical.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Parameters {
    pub style: Style,
    pub seat_height: f32,
    pub seat_width: f32,
    pub seat_depth: f32,
    pub back_angle: f32,
    pub leg_count: u8,
    pub leg_radius: f32,
    pub roundness: f32,
}

impl Default for Parameters {
    fn default() -> Self {
        Self::for_style(Style::FourLeg)
    }
}

impl Parameters {
    pub fn for_style(style: Style) -> Self {
        let mut p = Parameters {
            style,
            seat_height: 45.0,
            seat_width: 53.0,
            seat_depth: 48.0,
            back_angle: 12.0,
            leg_count: 4,
            leg_radius: 2.8,
            roundness: 3.3,
        };
        match style {
            Style::Shell => {
                p.seat_width = 55.0;
                p.seat_depth = 49.0;
                p.back_angle = 14.0;
                p.leg_radius = 2.6;
                p.roundness = 4.5;
            }
            Style::FourLeg => {}
            Style::Monocoque => {
                p.seat_height = 44.0;
                p.seat_width = 57.0;
                p.seat_depth = 52.0;
                p.back_angle = 17.0;
                p.leg_count = 1;
                p.leg_radius = 5.0;
                p.roundness = 5.2;
            }
        }
        p
    }

    /// Sanitize untrusted JSON before generation or GUI sliders.
    pub fn sanitize(&mut self) {
        fn safe(v: &mut f32, lo: f32, hi: f32, fallback: f32) {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        }
        let default = Self::for_style(self.style);
        safe(&mut self.seat_height, 30.0, 85.0, default.seat_height);
        safe(&mut self.seat_width, 35.0, 95.0, default.seat_width);
        safe(&mut self.seat_depth, 30.0, 85.0, default.seat_depth);
        safe(&mut self.back_angle, -5.0, 35.0, default.back_angle);
        self.leg_count = self.leg_count.clamp(1, 4);
        safe(&mut self.leg_radius, 1.8, 9.0, default.leg_radius);
        safe(&mut self.roundness, 0.8, 7.0, default.roundness);
    }
}

fn material(rgb: [f32; 3], roughness: f32, metallic: f32) -> Material {
    Material::plain(rgb, roughness, metallic)
}

/// In the 3D canvas the front of the chair faces -Y and the vertical is +Z.
fn parts_colors(style: Style) -> (Material, Material, Material) {
    match style {
        Style::Shell => {
            (material([0.25, 0.60, 0.56], 0.34, 0.06), material([0.25, 0.60, 0.56], 0.34, 0.06), material([0.69, 0.49, 0.32], 0.30, 0.68))
        }
        Style::FourLeg => {
            (material([0.78, 0.49, 0.29], 0.58, 0.0), material([0.65, 0.38, 0.23], 0.58, 0.0), material([0.28, 0.31, 0.36], 0.28, 0.70))
        }
        Style::Monocoque => {
            (material([0.93, 0.87, 0.72], 0.40, 0.02), material([0.93, 0.87, 0.72], 0.40, 0.02), material([0.93, 0.87, 0.72], 0.40, 0.02))
        }
    }
}

fn part(kind: Kind, name: impl Into<String>, pos: [f32; 3], mat: Material, round: f32, blend: f32) -> Shape {
    let mut s = Shape::new(kind, pos, mat);
    s.name = name.into();
    s.round = round;
    s.blend = blend;
    s.source = GENERATOR_TAG.to_string();
    s
}

/// A smooth, rounded-rectangle outline, clockwise or counter-clockwise matters
/// only to the existing signed-distance function.
fn rounded_outline(half_x: f32, half_y: f32, exponent: f32) -> Vec<[f32; 2]> {
    let n = 24;
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * (i as f32) / n as f32;
            let (sn, cs) = a.sin_cos();
            [cs.signum() * cs.abs().powf(exponent) * half_x, sn.signum() * sn.abs().powf(exponent) * half_y]
        })
        .collect()
}

/// Rounded upper silhouette, pinched slightly at the waist for a sculptural bucket back.
fn back_outline(width: f32, height: f32) -> Vec<[f32; 2]> {
    let hw = width / 2.0;
    let hh = height / 2.0;
    [
        [-0.34, -0.96],
        [-0.46, -0.82],
        [-0.49, -0.54],
        [-0.48, -0.08],
        [-0.50, 0.38],
        [-0.43, 0.76],
        [-0.30, 0.96],
        [0.0, 1.0],
        [0.30, 0.96],
        [0.43, 0.76],
        [0.50, 0.38],
        [0.48, -0.08],
        [0.49, -0.54],
        [0.46, -0.82],
        [0.34, -0.96],
        [0.0, -1.0],
    ]
    .into_iter()
    .map(|q| [q[0] * 2.0 * hw, q[1] * hh])
    .collect()
}

fn curve(name: impl Into<String>, nodes: Vec<[f32; 4]>, mat: Material, blend: f32, smoothness: f32) -> Shape {
    let mut s = part(Kind::Curve, name, [0.0, 0.0, 0.0], mat, 0.0, blend);
    s.path = nodes;
    // The existing curve builder supports splined variable-radius paths.
    s.params = [0.0, smoothness.clamp(0.0, 1.0), 0.0, 1.0];
    s
}

fn leg_anchors(count: u8, width: f32, depth: f32) -> Vec<[f32; 2]> {
    let (x, y) = (width * 0.37, depth * 0.36);
    match count {
        1 => vec![[0.0, 0.0]],
        2 => vec![[-x, 0.0], [x, 0.0]],
        3 => vec![[0.0, -y], [-x, y], [x, y]],
        _ => vec![[-x, -y], [x, -y], [-x, y], [x, y]],
    }
}

fn generated_shapes(p: Parameters) -> Vec<Shape> {
    let mut p = p;
    p.sanitize();
    let (seat_mat, back_mat, leg_mat) = parts_colors(p.style);
    let mono = p.style == Style::Monocoque;
    let shell = p.style == Style::Shell;
    let seat_thickness = if mono {
        6.4
    } else if shell {
        5.5
    } else {
        5.0
    };
    let shell_blend = if mono {
        3.4
    } else if shell {
        2.2
    } else {
        0.55
    };

    let mut shapes = Vec::with_capacity(12);

    // Seat: a softly tapered Loft rather than a plain cube. It remains a single
    // independently editable shape, with two editable outlines.
    let mut seat = part(Kind::Loft, "座面", [0.0, 0.0, p.seat_height - seat_thickness / 2.0], seat_mat, p.roundness.min(seat_thickness * 0.35), 0.0);
    seat.size[0] = seat_thickness;
    seat.outline = rounded_outline(p.seat_width * 0.47, p.seat_depth * 0.46, 0.73);
    seat.outline2 = rounded_outline(p.seat_width * 0.50, p.seat_depth * 0.50, 0.67);
    shapes.push(seat);

    let back_height = p.seat_width * if shell { 0.87 } else { 0.80 };
    let back_thickness = if mono { 5.2 } else { 4.7 };
    let tilt = p.back_angle.to_radians();
    let bottom_y = p.seat_depth * 0.43;
    let centre_y = bottom_y + back_height / 2.0 * tilt.sin();
    let centre_z = p.seat_height - 2.0 + back_height / 2.0 * tilt.cos();
    let mut back = part(Kind::Extrude, "背もたれ", [0.0, centre_y, centre_z], back_mat, p.roundness.min(back_thickness * 0.32), shell_blend);
    back.outline = back_outline(p.seat_width * if shell { 0.93 } else { 0.88 }, back_height);
    back.size[0] = back_thickness;
    back.rot[0] = 90.0 - p.back_angle;
    shapes.push(back);

    if shell {
        // Two flowing side rails unify the bucket silhouette around the seat.
        for side in [-1.0_f32, 1.0] {
            shapes.push(curve(
                if side < 0.0 { "シェル左アーム" } else { "シェル右アーム" },
                vec![
                    [side * p.seat_width * 0.42, -p.seat_depth * 0.10, p.seat_height + 0.7, 2.0],
                    [side * p.seat_width * 0.48, p.seat_depth * 0.11, p.seat_height + 9.0, 2.5],
                    [side * p.seat_width * 0.44, p.seat_depth * 0.38, p.seat_height + 13.0, 2.6],
                    [side * p.seat_width * 0.40, centre_y, p.seat_height + 20.0, 2.8],
                ],
                seat_mat,
                1.4,
                0.83,
            ));
        }
    }

    if p.style == Style::FourLeg {
        // Thin coordinated edge under the soft seat, editable separately.
        let mut underseat = part(Kind::Loft, "座面下フレーム", [0.0, 0.0, p.seat_height - seat_thickness - 1.0], leg_mat, 1.0, 0.7);
        underseat.size[0] = 2.5;
        underseat.outline = rounded_outline(p.seat_width * 0.41, p.seat_depth * 0.41, 0.68);
        underseat.outline2 = rounded_outline(p.seat_width * 0.45, p.seat_depth * 0.45, 0.68);
        shapes.push(underseat);
    }

    for (i, [x, y]) in leg_anchors(p.leg_count, p.seat_width, p.seat_depth).into_iter().enumerate() {
        let r = p.leg_radius;
        let organic = if mono { 1.55 } else { 1.0 };
        let nodes = if p.leg_count == 1 {
            vec![
                [0.0, 0.0, p.seat_height - seat_thickness, r * 1.85],
                [0.0, 0.0, p.seat_height * 0.72, r * 1.25],
                [0.0, 0.0, p.seat_height * 0.23, r],
                [0.0, 0.0, r, r],
            ]
        } else {
            vec![
                [x * 0.83, y * 0.81, p.seat_height - seat_thickness + 0.8, r * 1.09 * organic],
                [x * 0.91, y * 0.94, p.seat_height * 0.78, r * 1.01 * organic],
                [x * 1.09, y * 1.12, p.seat_height * 0.35, r * 0.91],
                [x * 1.16, y * 1.17, r * 0.82, r * 0.85],
            ]
        };
        // The rounding slider also changes the interpolation through each tapered leg.
        let leg_smooth = (0.25 + p.roundness * 0.09).clamp(0.25, 0.95);
        shapes.push(curve(format!("脚 {}", i + 1), nodes, leg_mat, if mono { 2.2 } else { 0.6 }, leg_smooth));
    }

    if p.leg_count == 1 {
        let mut foot = part(Kind::Cylinder, "台座", [0.0, 0.0, 2.0], leg_mat, p.roundness.min(1.4), if mono { 2.0 } else { 0.8 });
        foot.size = [p.seat_width * 0.30, 4.0, 1.0];
        shapes.push(foot);
    }

    shapes
}

/// Generate once, using only the existing 3D scene/shape data format.
pub fn generate(mut params: Parameters) -> Group {
    params.sanitize();
    let mut group = Group::new(params.style.name());
    group.designer_chair = Some(params);
    group.shapes = generated_shapes(params);
    group
}

/// Regenerate parameter-controlled parts in-place, respecting the scene's placement.
/// Non-generated, manually-added parts survive; matching colours and visibility
/// survive when only numeric dimensions change.
pub fn regenerate(group: &mut Group, next: Parameters) {
    let mut next = next;
    next.sanitize();
    let previous = group.designer_chair.unwrap_or(next);
    let old_expected = generated_shapes(previous);
    let mut replacement = generated_shapes(next);
    let old_parts: Vec<Shape> = group.shapes.iter().filter(|s| s.source == GENERATOR_TAG).cloned().collect();
    let original_seat = old_parts.iter().find(|s| s.name == "座面").or_else(|| old_parts.first());
    let expected_seat = old_expected.first();
    let offset = match (original_seat, expected_seat) {
        (Some(a), Some(b)) => [a.pos[0] - b.pos[0], a.pos[1] - b.pos[1], a.pos[2] - b.pos[2]],
        _ => [0.0; 3],
    };
    for shape in &mut replacement {
        for (a, off) in shape.pos.iter_mut().zip(offset) {
            *a += off;
        }
        // Retain stable part identities and custom material when the user tweaks a
        // dimension. Style changes intentionally use the new style's palette.
        if let Some(old) = old_parts.iter().find(|s| s.name == shape.name) {
            shape.uid = old.uid;
            shape.visible = old.visible;
            if previous.style == next.style {
                shape.material = old.material;
            }
        }
    }
    let mut extras: Vec<Shape> = group.shapes.drain(..).filter(|s| s.source != GENERATOR_TAG).collect();
    replacement.append(&mut extras);
    group.shapes = replacement;
    group.designer_chair = Some(next);
}

/// Render the in-place whole-chair controls for the selected group.
pub fn controls(ui: &mut Ui, group: &mut Group, other_shapes: usize) -> bool {
    let Some(mut params) = group.designer_chair else { return false };
    let before = params;
    ui.separator();
    ui.label(RichText::new("Designer Chair").strong().color(style::ACCENT));
    ui.horizontal_wrapped(|ui| {
        for style_id in Style::ALL {
            if style::pill(ui, style_id.label(), params.style == style_id).clicked() {
                // Preserve custom leg counts; otherwise use the selected style's
                // signature leg layout (monocoque starts with a single pedestal).
                if params.style != style_id {
                    if params.leg_count == Parameters::for_style(params.style).leg_count {
                        params.leg_count = Parameters::for_style(style_id).leg_count;
                    }
                    params.style = style_id;
                }
            }
        }
    });
    ui.add(egui::Slider::new(&mut params.seat_height, 30.0..=85.0).text("座面の高さ mm"));
    ui.add(egui::Slider::new(&mut params.seat_width, 35.0..=95.0).text("座面の幅 mm"));
    ui.add(egui::Slider::new(&mut params.seat_depth, 30.0..=85.0).text("座面の奥行き mm"));
    ui.add(egui::Slider::new(&mut params.back_angle, -5.0..=35.0).text("背もたれ角度 °"));
    ui.add(egui::Slider::new(&mut params.leg_count, 1..=4).text("脚の本数"));
    ui.add(egui::Slider::new(&mut params.leg_radius, 1.8..=9.0).text("脚の太さ (半径) mm"));
    ui.add(egui::Slider::new(&mut params.roundness, 0.8..=7.0).text("全体の丸み mm"));
    ui.label(
        RichText::new("座面・背もたれ・脚は左のシーン一覧から個別編集できます。全体のスライダーを動かすと自動生成パーツの形状を再計算します（手動で追加したパーツは残ります）。")
            .size(11.0).color(style::DIM),
    );
    params.sanitize();
    if params == before {
        return false;
    }
    let manual_parts = group.shapes.iter().filter(|s| s.source != GENERATOR_TAG).count();
    let total = other_shapes.saturating_add(manual_parts).saturating_add(generated_shapes(params).len());
    if total > crate::model::MAX_SHAPES {
        ui.label(
            RichText::new(format!("これ以上パーツを増やせません（シーンは{}個まで）。不要な形を削除してください。", crate::model::MAX_SHAPES))
                .color(style::CUT),
        );
        return false;
    }
    regenerate(group, params);
    true
}

/// Entries share the same library insertion and asynchronous thumbnail pipeline.
pub fn presets() -> Vec<crate::library::Item> {
    Style::ALL
        .into_iter()
        .map(|style| crate::library::Item {
            name: style.name().to_string(),
            tags: vec!["designer chair".into(), "chair".into(), "organic furniture".into(), "椅子".into(), "いす".into(), style.label().into()],
            group: generate(Parameters::for_style(style)),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::export_formats::Format;
    use crate::export_job::{self, Options, Scope};
    use crate::export_mesh::Progress;
    use crate::model::Op;
    use crate::model::{Pick, Scene};

    fn scene(group: Group) -> Scene {
        Scene { groups: vec![group], ..Scene::default() }
    }

    #[test]
    fn three_styles_have_editable_named_parts_and_valid_limits() {
        for style in Style::ALL {
            let g = generate(Parameters::for_style(style));
            assert_eq!(g.designer_chair.unwrap().style, style);
            assert_eq!(g.shapes[0].name, "座面");
            assert!(g.shapes.iter().any(|s| s.name == "背もたれ"));
            assert_eq!(g.shapes.iter().filter(|s| s.name.starts_with("脚 ")).count(), g.designer_chair.unwrap().leg_count as usize);
            assert!(g.shapes.len() <= 16 && g.shapes.len() >= 3);
            assert!(g.shapes.iter().all(|s| s.op == Op::Add && s.source == GENERATOR_TAG));
            let mut sc = scene(g.clone());
            sc.sanitize();
            assert_eq!(sc.groups[0], g, "generated parts must survive sanitization unchanged");
        }
    }

    #[test]
    fn native_undo_redo_reverts_a_designer_chair_parameter_edit() {
        let ctx = egui::Context::default();
        let mut app = crate::app::App::new(true, None);
        app.scene.groups.clear();
        app.scene.groups.push(generate(Parameters::default()));
        app.sel = Some(Pick::Group(0));
        app.settle(&ctx);
        let prior = app.scene.clone();

        let mut changed = Parameters::default();
        changed.seat_width = 68.0;
        changed.leg_count = 3;
        regenerate(&mut app.scene.groups[0], changed);
        app.settle(&ctx);
        assert_ne!(prior, app.scene);
        let later = app.scene.clone();
        app.undo();
        assert_eq!(app.scene, prior, "Undo returns old chair dimensions");
        app.redo();
        assert_eq!(app.scene, later, "Redo regenerates the same saved shapes");
    }

    #[test]
    fn chair_controls_regenerate_without_erasing_other_parts_or_moving_the_model() {
        let mut g = generate(Parameters::default());
        for s in &mut g.shapes {
            s.pos[0] += 102.0;
            s.pos[1] -= 34.0;
        }
        let mut extra = Shape::new(Kind::Box, [102.0, -34.0, 12.0], Material::default());
        extra.name = "追加した飾り".into();
        g.shapes.push(extra.clone());
        let mut next = Parameters::default();
        next.seat_width = 67.0;
        next.seat_height = 49.0;
        next.back_angle = 23.0;
        next.leg_count = 3;
        next.roundness = 5.0;
        regenerate(&mut g, next);
        assert_eq!(g.shapes[0].pos[0], 102.0);
        assert_eq!(g.shapes[0].pos[1], -34.0);
        assert_eq!(g.shapes[0].pos[2], 49.0 - 2.5);
        assert_eq!(g.shapes.iter().filter(|s| s.name.starts_with("脚 ")).count(), 3);
        assert_eq!(g.shapes.last(), Some(&extra));
        assert_eq!(g.designer_chair.unwrap().back_angle, 23.0);
        // Both group and individually editable shapes survive file roundtrip.
        let serialized = scene(g.clone()).to_json();
        let loaded = Scene::from_json(&serialized).unwrap();
        assert_eq!(loaded.groups[0], g);
        assert!(loaded.groups[0].shapes[1].outline.len() >= 12);
    }

    #[test]
    fn custom_material_visibility_and_part_identity_survive_dimension_changes() {
        let mut g = generate(Parameters::default());
        let custom = material([0.25, 0.80, 0.60], 0.2, 0.0);
        g.shapes[0].uid = 108;
        g.shapes[0].material = custom;
        g.shapes[1].visible = false;
        let mut params = Parameters::default();
        params.seat_depth = 68.0;
        regenerate(&mut g, params);
        assert_eq!(g.shapes[0].uid, 108);
        assert_eq!(g.shapes[0].material, custom);
        assert!(!g.shapes[1].visible);
    }

    #[test]
    fn malformed_dimensions_are_clamped_and_legacy_groups_still_deserialize() {
        let mut bad = Parameters { seat_height: f32::NAN, seat_width: f32::INFINITY, leg_count: 255, ..Parameters::default() };
        bad.sanitize();
        assert!(bad.seat_height.is_finite() && bad.seat_width.is_finite());
        assert_eq!(bad.leg_count, 4);
        let mut old = serde_json::to_value(Group::new("old")).unwrap();
        old.as_object_mut().unwrap().remove("designer_chair");
        let back: Group = serde_json::from_value(old).unwrap();
        assert!(back.designer_chair.is_none());
        assert!(back.shapes.is_empty());
    }

    #[test]
    fn three_presets_make_distinct_nonempty_thumbnails() {
        let previews: Vec<Vec<u8>> = Style::ALL
            .iter()
            .map(|&style| {
                let rgba = crate::cpu_render::thumb(&scene(generate(Parameters::for_style(style))), 62);
                assert_eq!(rgba.len(), 62 * 62 * 4);
                let colours = rgba.as_chunks::<4>().0.iter().filter(|c| c[3] > 0 && c[0] != c[1]).count();
                assert!(colours > 20, "{} did not render a useful thumbnail", style.label());
                rgba
            })
            .collect();
        assert_ne!(previews[0], previews[1]);
        assert_ne!(previews[1], previews[2]);
    }

    #[test]
    fn chair_3mf_obj_use_existing_sdf_mesh_export() {
        // Headless end-to-end export, including a fresh scene loaded from JSON.
        // One mesh per model, two formats with the same serializer; no custom mesher.
        for style in Style::ALL {
            let g = generate(Parameters::for_style(style));
            let sc = Scene::from_json(&scene(g).to_json()).unwrap();
            let opts = Options { format: Format::ThreeMf, tolerance: 1.1, scope: Scope::Selected, ..Options::default() };
            let built =
                export_job::build(&sc, Some(Pick::Group(0)), &opts, &Progress::new()).unwrap_or_else(|e| panic!("{} 3MF mesh: {e}", style.label()));
            assert!(!built.surface.mesh.triangles.is_empty());
            assert!(built.closed, "{} generated an open mesh", style.label());
            for format in [Format::ThreeMf, Format::Obj] {
                let extension = format.ext();
                let filename = format!("DesignerChair.{extension}");
                let output = export_job::serialize(&built, &Options { format, ..opts.clone() }, std::path::Path::new(&filename))
                    .unwrap_or_else(|e| panic!("{} {:?}: {e}", style.label(), format));
                assert!(!output.is_empty());
                assert!(output.iter().all(|(_, bytes)| !bytes.is_empty()));
                let suffix = if format == Format::Obj { "obj" } else { "3mf" };
                assert!(output.iter().any(|(path, _)| path.extension().and_then(|e| e.to_str()) == Some(suffix)));
            }
        }
    }
}
