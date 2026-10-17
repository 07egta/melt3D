//! Parametric table lamps made of actual SDF solids, not thumbnail-only models.
//! Three independent shapes: a closed revolved shade cross section, a stand, and a base.

use serde::{Deserialize, Serialize};
use crate::app::App;
use crate::model::{Group, Kind, Material, Op, Pick, Scene, Shape, MAX_GROUPS};
use crate::style;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LampStyle {
    #[default]
    Mushroom,
    Globe,
    Arch,
}
impl LampStyle {
    pub const ALL: [Self; 3] = [Self::Mushroom, Self::Globe, Self::Arch];
    pub fn label(self) -> &'static str {
        match self {
            Self::Mushroom => "マッシュルーム型",
            Self::Globe => "球体型",
            Self::Arch => "アーチ型",
        }
    }
    pub fn english(self) -> &'static str {
        match self {
            Self::Mushroom => "mushroom",
            Self::Globe => "globe",
            Self::Arch => "arch",
        }
    }
}

/// Saved with Group; offset preserves library placement on parameter regeneration.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LampParams {
    pub style: LampStyle,
    pub height: f32,
    pub shade_diameter: f32,
    pub curvature: f32,
    pub thickness: f32,
    pub hollow: bool,
    pub glow: f32,
    pub offset: [f32; 2],
}
impl Default for LampParams {
    fn default() -> Self {
        Self {
            style: LampStyle::Mushroom, height: 128.0, shade_diameter: 88.0,
            curvature: 0.8, thickness: 2.2, hollow: true, glow: 1.6, offset: [0.0; 2],
        }
    }
}
impl LampParams {
    pub fn for_style(style: LampStyle) -> Self {
        match style {
            LampStyle::Mushroom => Self { style, ..Self::default() },
            LampStyle::Globe => Self { style, height: 142.0, shade_diameter: 78.0, curvature: 1.0, ..Self::default() },
            LampStyle::Arch => Self { style, height: 165.0, shade_diameter: 64.0, curvature: 1.0, ..Self::default() },
        }
    }
    pub fn sanitize(&mut self) {
        fn safe(n: &mut f32, min: f32, max: f32, fallback: f32) {
            *n = if n.is_finite() { n.clamp(min, max) } else { fallback };
        }
        safe(&mut self.height, 75.0, 300.0, 128.0);
        safe(&mut self.shade_diameter, 40.0, 160.0, 88.0);
        safe(&mut self.curvature, 0.2, 1.8, 0.8);
        safe(&mut self.thickness, 0.8, 4.0, 2.2);
        self.thickness = self.thickness.min(self.shade_diameter * 0.08);
        safe(&mut self.glow, 0.0, 4.0, 1.6);
        for n in &mut self.offset { safe(n, -1500.0, 1500.0, 0.0); }
    }
}
fn shade_material(style: LampStyle, glow: f32) -> Material {
    let color = match style {
        LampStyle::Mushroom => [0.96, 0.77, 0.54],
        LampStyle::Globe => [0.82, 0.90, 1.0],
        LampStyle::Arch => [0.99, 0.82, 0.58],
    };
    Material { glow, translucency: 0.55, coat: 0.25, ..Material::plain(color, 0.34, 0.0) }
}
fn metal(style: LampStyle) -> Material {
    let color = match style {
        LampStyle::Mushroom => [0.63, 0.48, 0.29],
        LampStyle::Globe => [0.48, 0.52, 0.57],
        LampStyle::Arch => [0.35, 0.34, 0.31],
    };
    Material { coat: 0.35, ..Material::plain(color, 0.30, 0.76) }
}

/// Revolved dome section, relative to its lip (z=0).
/// Outer rim -> outer apex -> inner apex -> inner rim -> closed annular lip.
/// This makes an OPEN-BOTTOM cavity enclosed by a printable solid wall.
fn dome_section(radius: f32, rise: f32, curve: f32, wall: f32, hollow: bool) -> Vec<[f32; 2]> {
    const STEPS: usize = 18;
    let exponent = 1.6 + curve * 0.95;
    let mut outer = Vec::with_capacity(STEPS + 1);
    let mut inner = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let u = 1.0 - i as f32 / STEPS as f32;
        let r = radius * u;
        let z = rise * (1.0 - u.powf(exponent));
        let slope = if u > 0.0 { -rise * exponent / radius * u.powf(exponent - 1.0) } else { 0.0 };
        let n = slope.hypot(1.0);
        outer.push([r, z]);
        inner.push([(r + wall * slope / n).max(0.0), z - wall / n]);
    }
    if hollow {
        outer.extend(inner.into_iter().rev());
    } else {
        outer.push([0.0, 0.0]); // solid dome with filled underside
    }
    outer
}

/// Ellipsoidal globe shade, polar angle from open lower lip to north pole.
fn globe_section(radius: f32, axial: f32, wall: f32, hollow: bool) -> Vec<[f32; 2]> {
    const STEPS: usize = 20;
    const LIP_ANGLE: f32 = 2.38;
    let mut outer = Vec::with_capacity(STEPS + 1);
    let mut inner = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let a = LIP_ANGLE * (1.0 - i as f32 / STEPS as f32);
        outer.push([radius * a.sin(), axial * (a.cos() - 1.0)]);
        inner.push([(radius - wall) * a.sin(), (axial - wall) * a.cos() - axial]);
    }
    if hollow {
        outer.extend(inner.into_iter().rev());
    } else {
        outer.push([0.0, axial * (LIP_ANGLE.cos() - 1.0)]);
    }
    outer
}

fn shade(p: LampParams, x: f32) -> Shape {
    let r = p.shade_diameter * 0.5;
    let (origin_z, points) = match p.style {
        LampStyle::Globe => {
            let axial = (r * (0.85 + p.curvature * 0.18)).min((p.height - 14.0) * 0.54);
            (p.height, globe_section(r, axial, p.thickness, p.hollow))
        }
        LampStyle::Mushroom | LampStyle::Arch => {
            let rise = (r * (0.39 + p.curvature * 0.23)).min(p.height * 0.43);
            let top = if p.style == LampStyle::Arch { p.height - 13.0 } else { p.height };
            (top - rise, dome_section(r, rise, p.curvature, p.thickness, p.hollow))
        }
    };
    let mut sh = Shape::new(Kind::Revolve, [p.offset[0] + x, p.offset[1], origin_z], shade_material(p.style, p.glow));
    sh.name = "傘 / Shade".into();
    sh.outline = points;
    sh.params[0] = 360.0;
    sh.round = 0.0;
    sh.shell = 0.0; // ring profile is already hollow; generic shell would double-hollow
    sh.blend = 0.0;
    sh
}
fn stand(p: LampParams, base_x: f32, shade_x: f32, base_h: f32) -> Shape {
    if p.style == LampStyle::Arch {
        let reach = shade_x - base_x;
        let h = p.height;
        let tip_z = h - 13.0 - p.thickness + 0.8;
        let path = [
            [base_x, 0.0, base_h, 3.8],
            [base_x, 0.0, h * 0.37, 3.7],
            [base_x + reach * 0.08, 0.0, h * 0.71, 3.6],
            [base_x + reach * 0.25, 0.0, h * (0.85 + p.curvature * 0.04), 3.3],
            [base_x + reach * 0.49, 0.0, h * 0.985, 3.1],
            [base_x + reach * 0.72, 0.0, h * 0.985, 3.0],
            [base_x + reach * 0.91, 0.0, h * 0.95, 2.9],
            [shade_x, 0.0, h - 6.0, 2.9],
            [shade_x, 0.0, tip_z, 2.9],
        ];
        let mut sh = Shape::new(Kind::Curve, [p.offset[0], p.offset[1], 0.0], metal(p.style));
        sh.name = "支柱 / Arch arm".into();
        sh.path = path.to_vec();
        sh.blend = 0.0;
        sh
    } else {
        let top = p.height - p.thickness + 0.8;
        let mut sh = Shape::new(Kind::Cylinder, [p.offset[0], p.offset[1], (base_h + top) * 0.5], metal(p.style));
        sh.name = "支柱 / Stem".into();
        sh.size = [if p.style == LampStyle::Globe { 3.5 } else { 4.2 }, top - base_h, 1.0];
        sh.round = 1.0;
        sh.blend = 0.0;
        sh
    }
}
fn base(p: LampParams, x: f32, h: f32) -> Shape {
    let r = p.shade_diameter * match p.style {
        LampStyle::Arch => 0.32, LampStyle::Globe => 0.245, LampStyle::Mushroom => 0.26,
    };
    let mut sh = Shape::new(Kind::Cylinder, [p.offset[0] + x, p.offset[1], h * 0.5], metal(p.style));
    sh.name = "土台 / Base".into();
    sh.size = [r, h, 1.0];
    sh.round = 1.0;
    sh.blend = 0.0;
    sh
}

/// Real 3D geometry, in stable slots 0 shade / 1 support / 2 base.
pub fn build(mut p: LampParams) -> Group {
    p.sanitize();
    let r = p.shade_diameter * 0.5;
    let (bx, sx) = if p.style == LampStyle::Arch { (-r * 1.3, r * 0.57) } else { (0.0, 0.0) };
    let h = if p.style == LampStyle::Arch { 7.0 } else { 6.0 };
    let mut g = Group::new(format!("Designer Lamp - {}", p.style.label()));
    g.lamp = Some(p);
    g.shapes = vec![shade(p, sx), stand(p, bx, sx, h), base(p, bx, h)];
    g
}

/// Rebuild geometry on parameter changes, retaining per-part labels/materials,
/// UID, visibility, rotation, scale, and manual translation offsets.
pub fn regenerate(g: &mut Group, mut next: LampParams) {
    let Some(old_p) = g.lamp else { return };
    next.sanitize();
    let previous_base = build(old_p);
    let mut refreshed = build(next);
    if g.shapes.len() == 3 {
        for (i, new) in refreshed.shapes.iter_mut().enumerate() {
            let old = &g.shapes[i];
            if old_p.style == next.style {
                for axis in 0..3 {
                    new.pos[axis] += old.pos[axis] - previous_base.shapes[i].pos[axis];
                }
                new.material = old.material;
                if i == 0 { new.material.glow = next.glow; }
                new.rot = old.rot;
                new.scale = old.scale;
            }
            new.name = old.name.clone();
            new.visible = old.visible;
            new.uid = old.uid;
        }
    }
    g.shapes = refreshed.shapes;
    g.lamp = Some(next);
}

/// Conservative geometric bound used solely to arrange print parts on Z=0.
fn bounds(s: &Shape) -> ([f32; 3], [f32; 3]) {
    let (lo, hi) = match s.kind {
        Kind::Revolve => {
            let r = s.outline.iter().map(|p| p[0]).fold(0.0_f32, f32::max);
            let loz = s.outline.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
            let hiz = s.outline.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
            ([-r, -r, loz], [r, r, hiz])
        }
        Kind::Curve => {
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for node in &s.path {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(node[axis] - node[3].abs() - 1.0);
                    hi[axis] = hi[axis].max(node[axis] + node[3].abs() + 1.0);
                }
            }
            (lo, hi)
        }
        _ => {
            let h = s.local_half();
            ([-h[0], -h[1], -h[2]], h)
        }
    };
    ([lo[0] + s.pos[0], lo[1] + s.pos[1], lo[2] + s.pos[2]],
     [hi[0] + s.pos[0], hi[1] + s.pos[1], hi[2] + s.pos[2]])
}

/// Three groups = three selectable export scopes, each independently watertight.
pub fn split_for_print(scene: &mut Scene, i: usize) -> Result<(), &'static str> {
    let Some(source) = scene.groups.get(i) else { return Err("ランプが見つかりません"); };
    if source.lamp.is_none() || source.shapes.len() != 3 {
        return Err("傘・支柱・土台の3部品が必要です");
    }
    if source.pattern.is_some() || source.instance_of.is_some() || source.uid != 0 {
        return Err("リンクやパターンを解除してから分割してください");
    }
    if scene.groups.len() + 2 > MAX_GROUPS {
        return Err("グループ数が上限に達しています");
    }
    let mut parts = Vec::with_capacity(3);
    let mut left = source.lamp.map_or(0.0, |p| p.offset[0]) - 90.0;
    let centre_y = source.lamp.map_or(0.0, |p| p.offset[1]);
    for part in &source.shapes {
        let mut sh = part.clone();
        let (lo, hi) = bounds(&sh);
        let width = hi[0] - lo[0];
        sh.pos[0] += left - lo[0];
        sh.pos[1] += centre_y - (lo[1] + hi[1]) * 0.5;
        sh.pos[2] -= lo[2];
        sh.op = Op::Add;
        sh.blend = 0.0;
        let mut group = Group::new(format!("Designer Lamp 印刷用 - {}", sh.name));
        group.shapes.push(sh);
        parts.push(group);
        left += width + 14.0;
    }
    scene.groups.splice(i..=i, parts);
    Ok(())
}

impl App {
    /// Both a group and any of its parts can show this common parameter panel.
    /// True means a print split replaced the original group.
    pub fn designer_lamp_inspector(&mut self, ui: &mut egui::Ui, gi: usize) -> bool {
        let Some(mut p) = self.scene.groups.get(gi).and_then(|g| g.lamp) else { return false };
        let old = p;
        let mut split = false;
        egui::CollapsingHeader::new("Designer Lamp - パラメータ")
            .default_open(true).id_salt(("designer_lamp", gi))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for v in LampStyle::ALL {
                        if style::pill(ui, v.label(), p.style == v).clicked() { p.style = v; }
                    }
                });
                style::labeled_slider(ui, "全高", &mut p.height, 75.0..=300.0, "ランプの高さ（mm）");
                style::labeled_slider(ui, "傘の直径", &mut p.shade_diameter, 40.0..=160.0, "傘の外径（mm）");
                style::labeled_slider(ui, "曲率", &mut p.curvature, 0.2..=1.8, "傘とアーチのカーブ");
                style::labeled_slider(ui, "壁厚", &mut p.thickness, 0.8..=4.0, "傘の肉厚（mm）。強度保証ではありません");
                ui.checkbox(&mut p.hollow, "傘を中空にする（下面に開口）");
                style::labeled_slider(ui, "発光", &mut p.glow, 0.0..=4.0, "見た目の発光。電気部品は含まれません");
                ui.label(egui::RichText::new("傘・支柱・土台はシーン一覧から個別に編集可能。パラメータ変更で断面は再生成されます。")
                    .size(11.5).color(style::DIM));
                if ui.button("3Dプリント用に3部品へ分割").clicked() { split = true; }
                ui.label(egui::RichText::new("分割すると造形面に別々に並びます。個別選択してSTL/3MFで書き出せます。")
                    .size(11.0).color(style::DIM));
            });
        if p != old && let Some(group) = self.scene.groups.get_mut(gi) {
            regenerate(group, p);
        }
        if split {
            match split_for_print(&mut self.scene, gi) {
                Ok(()) => {
                    self.sel = Some(Pick::Group(gi));
                    self.say(ui.ctx(), "傘・支柱・土台を分割しました。選択中の部品を書き出せます");
                    return true;
                }
                Err(why) => self.say(ui.ctx(), why),
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::Prepared;

    #[test]
    fn styles_have_real_editable_solids_and_survive_sanitization() {
        for kind in LampStyle::ALL {
            let group = build(LampParams::for_style(kind));
            assert_eq!(group.shapes.len(), 3);
            assert_eq!(group.shapes[0].kind, Kind::Revolve);
            assert!(group.shapes[0].outline.len() <= crate::model::MAX_OUTLINE);
            assert_eq!(group.shapes[1].kind, if kind == LampStyle::Arch { Kind::Curve } else { Kind::Cylinder });
            assert_eq!(group.shapes[2].kind, Kind::Cylinder);
            assert!(group.shapes.iter().all(|s| s.op == Op::Add && s.blend == 0.0));
            assert!(group.shapes[0].material.glow > 0.0);
            let mut scene = Scene { groups: vec![group], ..Scene::default() };
            let before = scene.clone();
            scene.sanitize();
            assert_eq!(before, scene);
        }
    }

    #[test]
    fn hollow_shade_has_empty_cavity_but_solid_option_does_not() {
        for style in LampStyle::ALL {
            let mut p = LampParams::for_style(style);
            p.hollow = true;
            let hollow = build(p);
            let shade = &hollow.shapes[0];
            let r = p.shade_diameter * 0.5;
            let sample_z = match style {
                LampStyle::Globe => p.height - r * 0.8,
                LampStyle::Mushroom | LampStyle::Arch => {
                    let rise = (r * (0.39 + p.curvature * 0.23)).min(p.height * 0.43);
                    let top = if style == LampStyle::Arch { p.height - 13.0 } else { p.height };
                    top - rise + 0.2 * rise
                }
            };
            let point = [shade.pos[0] + 6.0, shade.pos[1], sample_z];
            assert!(Prepared::new(shade).distance(point) > 0.0, "cavity {style:?}");
            assert!(Prepared::new(shade).distance([shade.pos[0], shade.pos[1], -10.0]) > 0.0);
            p.hollow = false;
            let solid = build(p);
            assert!(Prepared::new(&solid.shapes[0]).distance(point) < 0.0, "filled {style:?}");
        }
    }

    #[test]
    fn parametric_edit_keeps_independent_parts_and_project_round_trips() {
        let mut g = build(LampParams::default());
        g.shapes[1].pos[0] += 3.0;
        g.shapes[2].material.metallic = 0.2;
        let mut p = g.lamp.unwrap();
        p.height = 180.0;
        p.shade_diameter = 101.0;
        p.curvature = 1.3;
        p.thickness = 3.0;
        p.glow = 2.4;
        regenerate(&mut g, p);
        assert!((g.shapes[1].pos[0] - 3.0).abs() < 1e-3);
        assert_eq!(g.shapes[2].material.metallic, 0.2);
        assert_eq!(g.shapes[0].material.glow, 2.4);
        assert!((g.shapes[0].outline[0][0] - 50.5).abs() < 1e-3);
        let scene = Scene { groups: vec![g.clone()], ..Scene::default() };
        let restored = Scene::from_json(&scene.to_json()).unwrap();
        assert_eq!(restored.groups[0].lamp, g.lamp);
        assert_eq!(restored.groups[0].shapes, g.shapes);
    }

    #[test]
    fn print_split_creates_three_separately_exportable_nonfused_parts() {
        for style in LampStyle::ALL {
            let mut scene = Scene { groups: vec![build(LampParams::for_style(style))], ..Scene::default() };
            split_for_print(&mut scene, 0).unwrap();
            assert_eq!(scene.groups.len(), 3);
            assert!(scene.groups.iter().all(|g| g.shapes.len() == 1 && g.lamp.is_none()));
            let boxes: Vec<_> = scene.groups.iter().map(|g| bounds(&g.shapes[0])).collect();
            for (lo, _) in &boxes { assert!(lo[2].abs() < 1e-3, "{style:?} {lo:?}"); }
            for pair in boxes.windows(2) { assert!(pair[0].1[0] + 10.0 < pair[1].0[0]); }
            for i in 0..scene.groups.len() {
                let scoped = crate::export_job::prepare(&scene, Some(Pick::Group(i)), crate::export_job::Scope::Selected, false).unwrap();
                assert_eq!(scoped.groups.iter().filter(|g| g.visible).count(), 1);
            }
        }
    }

    #[test]
    fn exported_shade_mesh_is_nonempty_and_watertight() {
        let group = build(LampParams::default());
        let shade = &group.shapes[0];
        let field = Prepared::new(shade);
        let (lo, hi) = bounds(shade);
        let lower: [f64; 3] = lo.map(|n| f64::from(n - 2.0));
        let upper: [f64; 3] = hi.map(|n| f64::from(n + 2.0));
        let progress = crate::export_mesh::Progress::new();
        let mesh = crate::export_mesh::mesh_sparse(
            &|point: [f32; 3]| field.distance(point),
            lower, upper, 0.9, 1, &progress.slice(0.0, 1.0),
        ).expect("mesh an actual hollow shade");
        assert!(!mesh.mesh.triangles.is_empty(), "no geometry in export");
        assert!(mesh.stats.complete, "mesher could not close the surface");
        assert!(solidcraft_geom::melt::is_watertight(&mesh.mesh), "mesh must have no open boundaries");
    }

    #[test]
    fn lamp_parameter_card_renders_in_headless_egui() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups = vec![build(LampParams::default())];
        app.sel = Some(Pick::Group(0));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(!app.designer_lamp_inspector(ui, 0));
        });
        assert!(!out.shapes.is_empty());
        out.textures_delta.clear();
    }

    #[test]
    fn malformed_params_are_sanitized_and_bad_split_is_atomic() {
        let mut p = LampParams::default();
        p.thickness = f32::NAN;
        p.curvature = f32::INFINITY;
        p.offset = [f32::NAN, 10_000.0];
        let g = build(p);
        assert!(g.lamp.unwrap().thickness.is_finite());
        assert!(g.lamp.unwrap().offset.iter().all(|v| v.is_finite()));
        let mut scene = Scene { groups: vec![g], ..Scene::default() };
        scene.groups[0].shapes.pop();
        assert!(split_for_print(&mut scene, 0).is_err());
        assert_eq!(scene.groups.len(), 1);
    }
}
