//! Editable gravity-melt preset built from existing Sphere/Box + Curve SDF shapes.
//! No mesh approximation: the viewport and mesh exporter see the same Scene/Field.
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, TextureHandle, TextureOptions};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::model::{Group, Kind, Material, Melt, Op, Pick, Scene, Shape, MAX_GROUPS, MAX_SHAPES};

const MAX_DRIPS: u32 = 12;
const MIN_DIAMETER: f32 = 1.6;
const PREVIEW_SIZE: usize = 112;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Base {
    #[default]
    Sphere,
    Box,
}

impl Base {
    pub fn label(self) -> &'static str {
        match self {
            Base::Sphere => "球体",
            Base::Box => "箱",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub base: Base,
    /// External dimensions in mm; a sphere reads dimensions[0] as its diameter.
    pub dimensions: [f32; 3],
    pub length: f32,
    /// Requested diameter; reduced if necessary to keep neighbouring drips apart.
    pub diameter: f32,
    pub count: u32,
    pub tilt_deg: f32,
    pub azimuth_deg: f32,
    pub viscosity: f32,
    pub seed: u32,
    pub keep_base_size: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            base: Base::Sphere,
            dimensions: [72.0, 62.0, 60.0],
            length: 48.0,
            diameter: 9.0,
            count: 5,
            tilt_deg: 12.0,
            azimuth_deg: 0.0,
            viscosity: 0.85,
            seed: 42,
            keep_base_size: true,
        }
    }
}

fn finite(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(low, high) } else { fallback }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.dimensions[0] = finite(self.dimensions[0], 24.0, 240.0, 72.0);
        self.dimensions[1] = finite(self.dimensions[1], 24.0, 240.0, 62.0);
        self.dimensions[2] = finite(self.dimensions[2], 24.0, 240.0, 60.0);
        self.length = finite(self.length, 6.0, 140.0, 48.0);
        self.diameter = finite(self.diameter, MIN_DIAMETER, 24.0, 9.0);
        self.count = self.count.clamp(1, MAX_DRIPS);
        self.tilt_deg = finite(self.tilt_deg, 0.0, 55.0, 12.0);
        self.azimuth_deg = finite(self.azimuth_deg, 0.0, 360.0, 0.0);
        self.viscosity = finite(self.viscosity, 0.0, 1.0, 0.85);
    }

    fn body_dims(&self) -> [f32; 3] {
        let d = match self.base {
            Base::Sphere => [self.dimensions[0]; 3],
            Base::Box => self.dimensions,
        };
        if self.keep_base_size { d } else { [d[0], d[1], d[2] * 0.78] }
    }

    fn perimeter(&self) -> f32 {
        let d = self.body_dims();
        match self.base {
            Base::Sphere => TAU * (d[0] * 0.5 * 0.97),
            Base::Box => 2.0 * (d[0] + d[1]),
        }
    }

    fn spacing(&self) -> f32 {
        self.perimeter() / self.count.max(1) as f32
    }

    pub fn effective_diameter(&self) -> f32 {
        // Leave room for both random anchor jitter and shape-to-shape smooth union.
        self.diameter.min(self.spacing() * 0.40).max(MIN_DIAMETER)
    }

    pub fn centre_height(&self) -> f32 {
        let h = self.body_dims()[2];
        let fall = self.length * 1.20 * self.tilt_deg.to_radians().cos();
        (h * 0.5 + 5.0).max(fall - h * 0.15 + self.effective_diameter() * 0.5 + 1.0)
    }

    /// Generate editable normal shapes, with exactly one solid body and count curves.
    /// The curves use the same rounded-cone SDF as the viewport and exporter.
    pub fn generate(&self, centre: [f32; 3], material: Material) -> Group {
        let mut cfg = self.clone();
        cfg.sanitize();
        let mut group = Group::new("Melt（とろける形状）");
        group.melt_preset = Some(cfg.clone());
        let dims = cfg.body_dims();
        let mut body = Shape::new(match cfg.base { Base::Sphere => Kind::Sphere, Base::Box => Kind::Box }, centre, material.clone());
        body.name = "元の形状".into();
        match cfg.base {
            Base::Sphere => {
                body.size[0] = dims[0] * 0.5;
                if !cfg.keep_base_size {
                    body.scale[2] = dims[2] / dims[0];
                }
            }
            Base::Box => {
                body.size = dims;
                body.round = dims.iter().copied().fold(f32::INFINITY, f32::min) * 0.12;
            }
        }
        body.blend = 0.0;
        group.shapes.push(body);

        let mut rng = cfg.seed;
        let radius = cfg.effective_diameter() * 0.5;
        let spacing = cfg.spacing();
        let theta = cfg.azimuth_deg.to_radians();
        let tilt = cfg.tilt_deg.to_radians();
        let drift_dir = [theta.cos() * tilt.sin(), theta.sin() * tilt.sin()];
        let fall_factor = tilt.cos();
        let ring_r = dims[0] * 0.5 * 0.97;
        let body_height = dims[2];
        for i in 0..cfg.count {
            let jitter = (rand01(&mut rng) - 0.5) * 0.24;
            let u = (i as f32 + 0.5 + jitter) / cfg.count as f32;
            let (x, y, outward) = match cfg.base {
                Base::Sphere => {
                    let t = TAU * u;
                    (ring_r * t.cos(), ring_r * t.sin(), [t.cos(), t.sin()])
                }
                Base::Box => box_perimeter_point(u, dims[0] * 0.5, dims[1] * 0.5),
            };
            // Keep length variation proportional to free perimeter spacing.
            // Otherwise long, tilted neighbours can cross at different heights.
            let spread = (spacing * 0.15 / (cfg.length * tilt.sin().max(0.1))).min(0.20);
            let individual_length = cfg.length * (1.0 + (2.0 * rand01(&mut rng) - 1.0) * spread);
            let vertical = individual_length * fall_factor;
            let relative_thickness = 0.88 + 0.22 * rand01(&mut rng);
            let r = radius * relative_thickness;
            let sideways = (rand01(&mut rng) - 0.5) * spacing * 0.08;
            let tangent = [-outward[1], outward[0]];
            let xy = [x + tangent[0] * sideways, y + tangent[1] * sideways];
            let outward_distance = r * (0.35 + cfg.viscosity * 0.3);
            let point = |t: f32, edge: f32, z: f32, tip_radius: f32| -> [f32; 4] {
                [
                    xy[0] * edge + outward[0] * outward_distance * t + drift_dir[0] * cfg.length * t,
                    xy[1] * edge + outward[1] * outward_distance * t + drift_dir[1] * cfg.length * t,
                    z,
                    tip_radius,
                ]
            };
            let top = body_height * 0.15;
            let top_radius = (r * (1.23 + 0.24 * cfg.viscosity)).max(MIN_DIAMETER * 0.5);
            let neck_radius = (r * (0.87 + 0.10 * cfg.viscosity)).max(MIN_DIAMETER * 0.5);
            let waist_radius = (r * (0.66 + 0.18 * cfg.viscosity)).max(MIN_DIAMETER * 0.5);
            let tip_radius = (r * (0.53 + 0.12 * cfg.viscosity)).max(MIN_DIAMETER * 0.5);
            // Store the path in the Curve's own space (shared body centre), not
            // in absolute world coordinates: picking, framing and export bounds
            // must remain stable even for a model far from the origin.
            let mut drip = Shape::new(Kind::Curve, centre, material.clone());
            drip.name = format!("液だれ {}", i + 1);
            drip.op = Op::Add;
            drip.blend = (r * (1.15 + cfg.viscosity * 0.55)).min(spacing * 0.18);
            drip.params = [0.0, 0.70, 0.0, 1.0]; // open, smooth, round ends, 1x thickness
            drip.path = vec![
                point(0.0, 0.82, top + body_height * 0.08, top_radius),
                point(0.17, 0.98, top - vertical * 0.22, neck_radius),
                point(0.51, 1.035, top - vertical * 0.59, waist_radius),
                point(1.0, 1.055, top - vertical, tip_radius),
            ];
            group.shapes.push(drip);
        }
        group
    }
}

fn rand01(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed >> 8) as f32 / ((1_u32 << 24) as f32)
}

/// Equally spaced *arc length*, not equally spaced angle, even for rectangles.
fn box_perimeter_point(t: f32, hx: f32, hy: f32) -> (f32, f32, [f32; 2]) {
    let mut edge = t.rem_euclid(1.0) * 4.0 * (hx + hy);
    if edge < 2.0 * hx {
        return (-hx + edge, -hy, [0.0, -1.0]);
    }
    edge -= 2.0 * hx;
    if edge < 2.0 * hy {
        return (hx, -hy + edge, [1.0, 0.0]);
    }
    edge -= 2.0 * hy;
    if edge < 2.0 * hx {
        return (hx - edge, hy, [0.0, 1.0]);
    }
    edge -= 2.0 * hx;
    (-hx, hy - edge, [-1.0, 0.0])
}

pub fn syrup_material() -> Material {
    let mut m = Material::plain([0.91, 0.54, 0.22], 0.17, 0.0);
    m.coat = 0.85;
    m.translucency = 0.23;
    m.color2 = [1.0, 0.80, 0.36];
    m.gradient = 0.35;
    m.grad_axis = 2;
    m.grad_soft = 0.8;
    m
}

fn preview_scene(settings: &Settings, melt: Melt, material: Material) -> Scene {
    let mut scene = Scene::default();
    scene.groups = vec![settings.generate([0.0, 0.0, settings.centre_height()], material)];
    scene.melt = melt;
    scene
}

#[derive(Clone, PartialEq)]
struct PreviewKey {
    settings: Settings,
    melt: Melt,
    material: Material,
}

#[derive(Clone)]
struct Editor {
    settings: Settings,
    editing: Option<usize>,
    preview: Option<TextureHandle>,
    preview_key: Option<PreviewKey>,
    running: Option<(PreviewKey, Arc<Mutex<Option<Vec<u8>>>>)>,
    error: Option<String>,
    failed_preview: Option<PreviewKey>,
}

impl Editor {
    fn new(settings: Settings, editing: Option<usize>) -> Editor {
        Editor {
            settings,
            editing,
            preview: None,
            preview_key: None,
            running: None,
            error: None,
            failed_preview: None,
        }
    }
}

fn id() -> egui::Id {
    egui::Id::new("melt_shape_preset")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(id(), Editor::new(Settings::default(), None)));
}

pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Editor>(id())).is_some()
}

pub fn open_edit(ctx: &egui::Context, group_index: usize, settings: Settings) {
    ctx.data_mut(|d| d.insert_temp(id(), Editor::new(settings, Some(group_index))));
}

fn preview_progress(ctx: &egui::Context, editor: &mut Editor, key: PreviewKey) {
    if let Some((pending_key, output)) = editor.running.as_ref() {
        let result = output.lock().ok().and_then(|mut p| p.take());
        if let Some(bytes) = result {
            if bytes.len() == PREVIEW_SIZE * PREVIEW_SIZE * 4 && pending_key == &key {
                let image = egui::ColorImage::from_rgba_unmultiplied([PREVIEW_SIZE; 2], &bytes);
                editor.preview = Some(ctx.load_texture("melt-shape-preview", image, TextureOptions::LINEAR));
                editor.preview_key = Some(key.clone());
            } else if pending_key == &key {
                editor.error = Some("プレビュー画像を作れませんでした".into());
                editor.failed_preview = Some(key.clone());
            }
            editor.running = None;
        }
    }
    if editor.running.is_some() || editor.preview_key.as_ref() == Some(&key) || editor.failed_preview.as_ref() == Some(&key) {
        return;
    }
    let output = Arc::new(Mutex::new(None));
    let shared = output.clone();
    let scene = preview_scene(&key.settings, key.melt, key.material.clone());
    let wake = ctx.clone();
    let spawned = std::thread::Builder::new().name("melt-preset-preview".into()).spawn(move || {
        let bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::cpu_render::thumb(&scene, PREVIEW_SIZE))).unwrap_or_default();
        if let Ok(mut data) = shared.lock() {
            *data = Some(bytes);
        }
        wake.request_repaint();
    });
    if spawned.is_ok() {
        editor.running = Some((key, output));
    } else {
        editor.failed_preview = Some(key);
        editor.error = Some("プレビュースレッドを起動できませんでした".into());
    }
}

fn apply(app: &mut App, settings: &Settings, editing: Option<usize>) -> Result<(), String> {
    let mut cfg = settings.clone();
    cfg.sanitize();
    let old = editing.and_then(|i| app.scene.groups.get(i));
    if editing.is_some() && old.is_none() {
        return Err("編集対象のグループが見つかりません".into());
    }
    if old.is_some_and(Group::is_linked) {
        return Err("リンクコピーは元のグループを編集してください".into());
    }
    let material = old.and_then(|g| g.shapes.first()).map(|sh| sh.material.clone()).unwrap_or_else(syrup_material);
    let centre = old.and_then(|g| g.shapes.first()).map(|sh| sh.pos).unwrap_or_else(|| {
        let (c, r) = crate::gpu::bounds(&app.scene, None);
        let pad = cfg.body_dims()[0] * 0.5 + 16.0;
        if app.scene.shape_count() > 0 && r > 0.0 {
            [c[0] + r + pad, c[1], cfg.centre_height()]
        } else {
            [app.cam.target[0], app.cam.target[1], cfg.centre_height()]
        }
    });
    let made = cfg.generate(centre, material);
    let prior = old.map_or(0, |g| g.shapes.len());
    let capacity = app.scene.shape_count().saturating_sub(prior).saturating_add(made.shapes.len());
    if capacity > MAX_SHAPES {
        return Err(format!("形が{MAX_SHAPES}個を超えるため追加できません。本数を減らしてください"));
    }
    if let Some(index) = editing {
        let Some(group) = app.scene.groups.get_mut(index) else {
            return Err("編集中のグループがありません".into());
        };
        // Explicit Apply regenerates the group. Other attributes stay intact.
        group.shapes = made.shapes;
        group.melt_preset = made.melt_preset;
        app.sel = Some(Pick::Group(index));
    } else {
        if app.scene.groups.len() >= MAX_GROUPS {
            return Err(format!("グループは{MAX_GROUPS}個までです"));
        }
        app.scene.groups.push(made);
        app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    }
    app.frame_view(true);
    Ok(())
}

/// Modal generator and deterministic preview: scene is not changed until Apply.
pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut editor) = ctx.data(|d| d.get_temp::<Editor>(id())) else { return };
    editor.settings.sanitize();
    let material = editor.editing
        .and_then(|g| app.scene.groups.get(g))
        .and_then(|g| g.shapes.first())
        .map(|sh| sh.material.clone())
        .unwrap_or_else(syrup_material);
    let key = PreviewKey { settings: editor.settings.clone(), melt: app.scene.melt, material };
    preview_progress(ctx, &mut editor, key.clone());
    let mut confirm = false;
    let mut cancel = false;
    egui::Modal::new(id()).show(ctx, |ui| {
            ui.set_min_width(350.0);
            ui.heading("Melt — とろける形状");
            ui.label(RichText::new("球・箱から、重力で垂れる水飴のような形を作ります").small());
            ui.horizontal(|ui| {
                ui.label("元の形");
                for kind in [Base::Sphere, Base::Box] {
                    ui.selectable_value(&mut editor.settings.base, kind, kind.label());
                }
            });
            if editor.settings.base == Base::Sphere {
                ui.add(egui::Slider::new(&mut editor.settings.dimensions[0], 24.0..=180.0).text("球の直径 (mm)"));
            } else {
                for (i, name) in ["幅", "奥行", "高さ"].iter().enumerate() {
                    ui.add(egui::Slider::new(&mut editor.settings.dimensions[i], 24.0..=180.0).text(format!("{name} (mm)")));
                }
            }
            ui.checkbox(&mut editor.settings.keep_base_size, "元形状の大きさを維持");
            ui.add(egui::Slider::new(&mut editor.settings.length, 6.0..=120.0).text("垂れの長さ (mm)"));
            ui.add(egui::Slider::new(&mut editor.settings.diameter, MIN_DIAMETER..=20.0).text("太さ (mm)"));
            ui.add(egui::Slider::new(&mut editor.settings.count, 1..=MAX_DRIPS).text("本数"));
            ui.add(egui::Slider::new(&mut editor.settings.tilt_deg, 0.0..=55.0).text("下向きからの傾き (°)"));
            ui.add(egui::Slider::new(&mut editor.settings.azimuth_deg, 0.0..=360.0).text("方向 (°)"));
            ui.add(egui::Slider::new(&mut editor.settings.viscosity, 0.0..=1.0).text("粘り・ふくらみ"));
            ui.horizontal(|ui| {
                ui.label("ランダムシード");
                ui.add(egui::DragValue::new(&mut editor.settings.seed).speed(1.0));
                if ui.button("違う垂れ方").clicked() {
                    editor.settings.seed = editor.settings.seed.wrapping_add(1);
                }
            });
            editor.settings.sanitize();
            let actual = editor.settings.effective_diameter();
            if actual + 0.01 < editor.settings.diameter {
                ui.label(RichText::new(format!("重なり防止のため太さを {:.1} mm に制限しています", actual)).color(Color32::YELLOW).small());
            }
            ui.separator();
            ui.horizontal(|ui| {
                if editor.preview_key.as_ref() == Some(&key) {
                    if let Some(tex) = &editor.preview {
                        ui.image((tex.id(), egui::vec2(148.0, 148.0)));
                    }
                } else if let Some(tex) = &editor.preview {
                    ui.vertical(|ui| {
                        ui.image((tex.id(), egui::vec2(148.0, 148.0)));
                        ui.label("更新中…");
                    });
                } else {
                    ui.add(egui::Spinner::new());
                    ui.label("形のプレビューを準備中…");
                }
                ui.vertical(|ui| {
                    ui.label("実際のSDFを使ったプレビュー");
                    ui.label(RichText::new("端は丸く、元の形へ滑らかにつながります").small());
                    ui.label(RichText::new("各垂れは通常のCurveとして編集できます").small());
                    if app.scene.melt.amount <= 0.0 {
                        ui.label(RichText::new("シーン全体の「とけぐあい」が0です。接合をなめらかにするにはメルト設定を上げてください").small());
                    }
                });
            });
            ui.label(RichText::new("細い先端を出力する場合はメッシュ精度0.4mm以下を推奨します。").small());
            if editor.editing.is_some() {
                ui.label(RichText::new("再適用すると、このグループの手動変更や追加形状が置き換わります。").color(Color32::YELLOW).small());
            }
            if let Some(message) = &editor.error {
                ui.label(RichText::new(message).color(Color32::LIGHT_RED));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(if editor.editing.is_some() { "設定を再適用" } else { "シーンに追加" }).clicked() {
                    confirm = true;
                }
                if ui.button("キャンセル").clicked() {
                    cancel = true;
                }
            });
        });
    if confirm {
        match apply(app, &editor.settings, editor.editing) {
            Ok(()) => {
                app.say(ctx, "Meltプリセットを追加・更新しました");
                ctx.data_mut(|d| d.remove::<Editor>(id()));
                return;
            }
            Err(e) => editor.error = Some(e),
        }
    }
    if cancel {
        ctx.data_mut(|d| d.remove::<Editor>(id()));
    } else {
        ctx.data_mut(|d| d.insert_temp(id(), editor));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::Field;

    fn scene(settings: &Settings) -> Scene {
        preview_scene(settings, Melt::default(), syrup_material())
    }

    #[test]
    fn seed_is_reproducible_and_changes_drips_not_body() {
        let s = Settings::default();
        let a = s.generate([0.0, 0.0, 50.0], syrup_material());
        let b = s.generate([0.0, 0.0, 50.0], syrup_material());
        assert_eq!(a, b);
        let mut shifted = s.clone();
        shifted.seed += 1;
        let c = shifted.generate([0.0, 0.0, 50.0], syrup_material());
        assert_eq!(a.shapes[0], c.shapes[0]);
        assert_ne!(a.shapes[1].path, c.shapes[1].path);
    }

    #[test]
    fn sphere_and_box_generate_editable_round_capped_monotone_curves() {
        for kind in [Base::Sphere, Base::Box] {
            let mut s = Settings { base: kind, count: MAX_DRIPS, diameter: 20.0, ..Settings::default() };
            s.sanitize();
            let g = s.generate([0.0, 0.0, 72.0], syrup_material());
            assert_eq!(g.shapes.len(), (s.count + 1) as usize);
            assert_eq!(g.shapes[0].kind, if kind == Base::Sphere { Kind::Sphere } else { Kind::Box });
            for shape in g.shapes.iter().skip(1) {
                assert_eq!(shape.kind, Kind::Curve);
                assert_eq!(shape.op, Op::Add);
                assert_eq!(shape.pos, g.shapes[0].pos, "drips use local path coordinates");
                assert_eq!(shape.params[2], 0.0, "end caps must be round");
                assert_eq!(shape.path.len(), 4);
                assert!(shape.path.windows(2).all(|p| p[0][2] > p[1][2]));
                assert!(shape.path.iter().all(|p| p.iter().all(|n| n.is_finite()) && p[3] >= MIN_DIAMETER * 0.5));
            }
            let field = Field::new(&scene(&s));
            for x in -2..3 {
                let p = [x as f32 * 14.0, 2.0, 60.0];
                assert!(field.distance(p).is_finite());
            }
        }
    }

    #[test]
    fn body_to_drip_junction_uses_existing_smooth_boolean() {
        for base in [Base::Sphere, Base::Box] {
            let cfg = Settings { base, ..Settings::default() };
            let g = cfg.generate([0.0, 0.0, 55.0], syrup_material());
            let (body, first) = (&g.shapes[0], &g.shapes[1]);
            let p = first.path[0];
            let world = [first.pos[0] + p[0], first.pos[1] + p[1], first.pos[2] + p[2]];
            let a = crate::sdf::Prepared::new(body).distance(world);
            let b = crate::sdf::Prepared::new(first).distance(world);
            let (smooth, _) = crate::sdf::smin(a, b, first.blend, crate::model::BlendMode::Smooth);
            assert!(smooth < a.min(b) - 0.01, "{base:?}: no soft connection: body={a}, drip={b}, soft={smooth}");
        }
    }

    #[test]
    fn keep_original_dimensions_while_drips_change() {
        let mut cfg = Settings::default();
        cfg.base = Base::Sphere;
        let original = cfg.generate([0.0, 0.0, 70.0], syrup_material()).shapes[0].clone();
        cfg.length = 100.0;
        cfg.count = 10;
        let changed = cfg.generate([0.0, 0.0, 70.0], syrup_material());
        assert_eq!(changed.shapes[0].size, original.size);
        assert_eq!(changed.shapes[0].scale, original.scale);
        cfg.keep_base_size = false;
        assert!(cfg.generate([0.0, 0.0, 70.0], syrup_material()).shapes[0].scale[2] < 1.0);
    }

    #[test]
    fn dense_drips_do_not_cross_outside_the_body() {
        for base in [Base::Sphere, Base::Box] {
            for seed in [0, 1, 9, 42, 512, 16801] {
                let cfg = Settings {
                    base,
                    dimensions: [24.0, 24.0, 24.0],
                    count: MAX_DRIPS,
                    diameter: 24.0,
                    seed,
                    tilt_deg: 50.0,
                    ..Settings::default()
                };
                let group = cfg.generate([0.0, 0.0, 80.0], syrup_material());
                for (i, a) in group.shapes.iter().enumerate().skip(1) {
                    for b in group.shapes.iter().skip(i + 1) {
                        // The first path point is purposefully embedded in the
                        // body; compare outside points, where separate drips
                        // must not cross or merge accidentally.
                        let (Some(a0), Some(a1), Some(b0), Some(b1)) =
                            (a.path.get(2), a.path.get(3), b.path.get(2), b.path.get(3))
                        else {
                            continue;
                        };
                        for step_a in 0..=8 {
                            let ta = step_a as f32 / 8.0;
                            let pa = [0, 1, 2, 3].map(|k| a0[k] * (1.0 - ta) + a1[k] * ta);
                            for step_b in 0..=8 {
                                let tb = step_b as f32 / 8.0;
                                let pb = [0, 1, 2, 3].map(|k| b0[k] * (1.0 - tb) + b1[k] * tb);
                                let d = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)
                                    + (pa[2] - pb[2]).powi(2)).sqrt();
                                assert!(
                                    d > (pa[3] + pb[3]) * 0.96,
                                    "{base:?}, seed {seed}, drips {i}, ta={ta}, tb={tb}: {d} vs radii {} + {}",
                                    pa[3], pb[3]
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn applying_is_one_undo_step_and_reediting_preserves_group_identity() {
        let mut app = App::new(true, None);
        let ctx = egui::Context::default();
        let original = app.scene.clone();
        let cfg = Settings::default();
        apply(&mut app, &cfg, None).expect("create preset");
        let index = app.scene.groups.len() - 1;
        app.settle(&ctx);
        assert_eq!(app.scene.groups[index].shapes.len(), cfg.count as usize + 1);
        app.undo();
        assert_eq!(app.scene, original);
        app.redo();
        let id = app.scene.groups[index].uid;
        let mut edit = cfg.clone();
        edit.seed = edit.seed.wrapping_add(1);
        edit.length += 5.0;
        apply(&mut app, &edit, Some(index)).expect("reapply preset");
        assert_eq!(app.scene.groups[index].uid, id);
        assert_eq!(app.scene.groups[index].melt_preset, Some(edit));
        assert_eq!(app.scene.groups.len(), original.groups.len() + 1);
    }

    #[test]
    fn hostile_metadata_is_sanitized_and_saved_settings_round_trip() {
        let mut cfg = Settings::default();
        cfg.length = f32::INFINITY;
        cfg.tilt_deg = f32::NAN;
        cfg.count = u32::MAX;
        cfg.sanitize();
        assert!(cfg.length.is_finite() && cfg.tilt_deg.is_finite() && cfg.count <= MAX_DRIPS);
        let before = scene(&cfg);
        let json = before.to_json();
        let after = Scene::from_json(&json).expect("preset data is persisted");
        assert_eq!(before.groups[0].melt_preset, after.groups[0].melt_preset);
        assert_eq!(before.groups[0].shapes, after.groups[0].shapes);
    }

    #[test]
    fn preview_and_export_use_the_same_field() {
        for base in [Base::Sphere, Base::Box] {
            let cfg = Settings { base, count: 3, length: 30.0, dimensions: [35.0, 36.0, 35.0], ..Settings::default() };
            let source = scene(&cfg);
            let prepared = crate::export_job::prepare(&source, Some(Pick::Group(0)), crate::export_job::Scope::Selected, false).expect("prepared");
            let (visible, exported) = (Field::new(&source), Field::new(&prepared));
            for x in (-25..=25).step_by(5) {
                for z in (0..=65).step_by(5) {
                    let p = [x as f32, 0.0, z as f32];
                    assert!((visible.distance(p) - exported.distance(p)).abs() < 1.0e-3, "{base:?} at {p:?}");
                }
            }
        }
    }

    #[test]
    fn exported_mesh_vertices_follow_the_preview_sdf() {
        let cfg = Settings { base: Base::Box, count: 3, length: 23.0, dimensions: [30.0, 30.0, 30.0], ..Settings::default() };
        let source = scene(&cfg);
        let field = Field::new(&source);
        let progress = crate::export_mesh::Progress::new();
        let mesh = crate::export_job::mesh_scene(&source, 1.5, 1, &progress.slice(0.0, 1.0)).expect("mesh from same SDF");
        assert!(!mesh.mesh.positions.is_empty());
        let step = (mesh.mesh.positions.len() / 250).max(1);
        for p in mesh.mesh.positions.iter().step_by(step) {
            let at = [p[0] as f32, p[1] as f32, p[2] as f32];
            assert!(field.distance(at).abs() < 4.5, "export/preview mismatch at {at:?}");
        }
    }
}
