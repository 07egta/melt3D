//! Organic Blob: a deterministic, editable metaball-style group built entirely from
//! existing Sphere SDF primitives and their smooth-union blend. No shader changes,
//! baked mesh, texture, or high-resolution rebuild is needed when sliders move.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::model::{Group, Kind, Material, Pick, Scene, Shape, MAX_GROUPS, MAX_SHAPES};
use crate::style;

const PREVIEW_SIZE: usize = 96;
const PREVIEW_SETTLE: Duration = Duration::from_millis(140);
const BASE_RADIUS: f32 = 28.0;
const GOLDEN_ANGLE: f32 = 2.399_963_1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// How far the soft lobes protrude beyond the base sphere.
    pub bulge: f32,
    /// Number of lobes and variation of their sizes and directions.
    pub detail: f32,
    /// Existing soft-union reach (not a mesh tessellation level).
    pub roundness: f32,
    /// Height relative to width, applied as editable native sphere scale.
    pub aspect: f32,
    pub seed: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { bulge: 0.64, detail: 0.55, roundness: 0.72, aspect: 1.12, seed: 7 }
    }
}

impl Settings {
    /// Protect against NaN and extreme values (the generator is also callable from tests).
    pub fn sanitized(self) -> Self {
        let safe = |v: f32, lo: f32, hi: f32, default: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { default }
        };
        Self {
            bulge: safe(self.bulge, 0.0, 1.0, 0.64),
            detail: safe(self.detail, 0.0, 1.0, 0.55),
            roundness: safe(self.roundness, 0.0, 1.0, 0.72),
            aspect: safe(self.aspect, 0.60, 1.75, 1.12),
            seed: self.seed,
        }
    }

    pub fn lobe_count(self) -> usize {
        9 + (self.sanitized().detail * 10.0).round() as usize
    }
}

/// SplitMix64: fixed-width integer arithmetic, independent of OS and random libraries.
struct Random(u64);

impl Random {
    fn new(seed: u32) -> Self {
        Self(u64::from(seed) ^ 0x9e37_79b9_7f4a_7c15)
    }

    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let bits = ((z ^ (z >> 31)) >> 40) as u32;
        bits as f32 / 16_777_216.0
    }
}

/// The group uses only editable Sphere shapes and existing per-shape blend values.
/// `sdf::smin` and the matching WGSL shader already support these soft unions.
pub fn generate(settings: Settings) -> Group {
    let p = settings.sanitized();
    let mut rng = Random::new(p.seed);
    let mut group = Group::new(format!("Organic Blob · {}", p.seed));
    let mat = Material::plain([0.83, 0.69, 0.96], 0.27, 0.04);
    // Keep the bottom roughly above Z=0 even at the maximum height/seed.
    let centre_z = 46.0 * p.aspect;
    let centre = [0.0, 0.0, centre_z];

    let mut core = Shape::new(Kind::Sphere, centre, mat);
    core.name = "中心の球".into();
    core.size = [BASE_RADIUS; 3];
    core.scale = [1.0, 1.0, p.aspect];
    core.blend = 0.0;
    group.shapes.push(core);

    let n = p.lobe_count();
    // A near-uniform Fibonacci distribution avoids unnatural polar clustering. Seeded
    // jitter and unequal radii give an organic silhouette without high-frequency noise.
    for i in 0..n {
        let lat = (1.0 - 2.0 * (i as f32 + 0.5) / n as f32
            + (rng.next() - 0.5) * p.detail * 0.17).clamp(-0.98, 0.98);
        let angle = (i as f32 + rng.next() * p.detail * 0.5) * GOLDEN_ANGLE
            + rng.next() * p.detail * 0.30;
        let ring = (1.0 - lat * lat).max(0.0).sqrt();
        let (sn, cs) = angle.sin_cos();
        let dir = [ring * cs, ring * sn, lat];

        let distance = BASE_RADIUS
            * (0.61 + p.bulge * 0.49 + (rng.next() - 0.5) * p.detail * 0.12);
        let radius = BASE_RADIUS
            * (0.32 + p.detail * 0.11 + (rng.next() - 0.5) * p.detail * 0.14);
        let mut lobe = Shape::new(Kind::Sphere, [
            centre[0] + dir[0] * distance,
            centre[1] + dir[1] * distance,
            centre[2] + dir[2] * distance * p.aspect,
        ], mat);
        lobe.name = format!("ふくらみ {:02}", i + 1);
        lobe.size = [radius.max(3.0); 3];
        lobe.scale = [1.0, 1.0, p.aspect];
        // The positive smoothing radius makes joints continuously curved; each
        // lobe overlaps the core, so there are no floating fragments by default.
        lobe.blend = 7.0 + 30.0 * p.roundness;
        group.shapes.push(lobe);
    }
    // Place near the floor at every aspect ratio; account for the smooth union
    // extending beyond the individual sphere surfaces. Do not change any mesh
    // resolution or compute a distance volume just to position the preset.
    let bottom = group.shapes.iter().map(|s| s.pos[2] - s.size[0] * s.scale[2])
        .fold(f32::INFINITY, f32::min);
    let lift = 4.0 + (7.0 + 30.0 * p.roundness) * 0.27 * p.aspect - bottom;
    for shape in &mut group.shapes { shape.pos[2] += lift; }
    group
}

fn preview_scene(settings: Settings) -> Scene {
    let mut scene = Scene::default();
    scene.groups = vec![generate(settings)];
    scene.env.grid = false;
    scene.env.floor = false;
    scene
}

/// Add in the same way as a library model: its own editable group, beside the
/// existing scene. The normal App::settle() makes this exactly one Undo step.
pub fn insert(app: &mut App, ctx: &egui::Context, settings: Settings) -> Result<(), String> {
    let mut group = generate(settings);
    if app.scene.groups.len() >= MAX_GROUPS {
        return Err(format!("グループは{MAX_GROUPS}個までです"));
    }
    if app.scene.shape_count().saturating_add(group.shapes.len()) > MAX_SHAPES {
        return Err(format!("形は{MAX_SHAPES}個までです"));
    }
    let (centre, radius) = crate::gpu::bounds(&app.scene, None);
    let own = preview_scene(settings);
    let (blob_centre, blob_radius) = crate::gpu::bounds(&own, None);
    // Translate only X/Y; retain the preset's ground clearance on Z.
    let dx = if radius > 0.0 { centre[0] + radius + blob_radius + 6.0 - blob_centre[0] } else { -blob_centre[0] };
    let dy = if radius > 0.0 { centre[1] - blob_centre[1] } else { -blob_centre[1] };
    for shape in &mut group.shapes {
        shape.pos[0] += dx;
        shape.pos[1] += dy;
    }
    app.scene.groups.push(group);
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    app.frame_view(false);
    app.say(ctx, "Organic Blobを追加しました。グループ内の球を自由に動かしたり大きさを変えられます");
    Ok(())
}

#[derive(Clone)]
struct PreviewTask {
    done: Arc<Mutex<Option<(Settings, Vec<u8>)>>>,
}

#[derive(Clone)]
struct Dialog {
    settings: Settings,
    preview: Option<TextureHandle>,
    preview_for: Option<Settings>,
    working: Option<PreviewTask>,
    changed_at: Instant,
    error: String,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            preview: None,
            preview_for: None,
            working: None,
            changed_at: Instant::now() - PREVIEW_SETTLE,
            error: String::new(),
        }
    }
}

fn id() -> egui::Id { egui::Id::new("organic_blob_card") }

/// Called from the library, the shape menu and the command palette.
pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(id(), Dialog::default()));
    ctx.request_repaint();
}

fn start_preview(ctx: &egui::Context, settings: Settings) -> Result<PreviewTask, String> {
    let done = Arc::new(Mutex::new(None));
    let out = done.clone();
    let ctx2 = ctx.clone();
    std::thread::Builder::new().name("meltcad-organic-preview".into()).spawn(move || {
        // Reuse the existing, low-resolution CPU library thumbnail pipeline,
        // away from the UI thread. There is at most one outstanding preview task.
        let pixels = std::panic::catch_unwind(|| crate::cpu_render::thumb(&preview_scene(settings), PREVIEW_SIZE))
            .unwrap_or_default();
        if let Ok(mut d) = out.lock() {
            *d = Some((settings, pixels));
        }
        ctx2.request_repaint();
    }).map_err(|e| format!("プレビューを開始できません: {e}"))?;
    Ok(PreviewTask { done })
}

fn refresh_preview(ctx: &egui::Context, d: &mut Dialog) {
    if let Some(task) = &d.working {
        let result = task.done.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((settings, pixels)) = result {
            d.working = None;
            if settings == d.settings {
                // Failed previews should not repeatedly spawn a worker every UI frame.
                d.preview_for = Some(settings);
                if pixels.len() == PREVIEW_SIZE * PREVIEW_SIZE * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([PREVIEW_SIZE, PREVIEW_SIZE], &pixels);
                    d.preview = Some(ctx.load_texture("organic_blob_preview", image, TextureOptions::LINEAR));
                } else {
                    d.error = "プレビューを表示できません。形の追加と編集はできます".into();
                }
            }
        }
    }
    if d.preview_for == Some(d.settings) { return; }
    if d.working.is_none() && d.changed_at.elapsed() >= PREVIEW_SETTLE {
        match start_preview(ctx, d.settings) {
            Ok(task) => d.working = Some(task),
            Err(e) => {
                d.error = e;
                d.preview_for = Some(d.settings);
            }
        }
    }
    if d.working.is_some() || d.changed_at.elapsed() < PREVIEW_SETTLE {
        ctx.request_repaint_after(Duration::from_millis(80));
    }
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = ctx.data_mut(|data| data.remove_temp::<Dialog>(id())) else { return };
    refresh_preview(ctx, &mut d);
    let before = d.settings;
    let mut apply = false;
    let mut close = false;
    egui::Area::new(egui::Id::new("organic_blob_area")).order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            style::card().show(ui, |ui| {
                ui.set_width(372.0);
                ui.label(RichText::new("Organic Blob · ぷっくりブロブ").strong().size(15.0));
                ui.label(RichText::new("やわらかい球のふくらみを、滑らかにつなげます").color(style::DIM).size(11.5));
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(160.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 12.0, style::PANEL_DARK);
                    if let Some(tex) = &d.preview {
                        ui.painter().image(tex.id(), rect.shrink(6.0),
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    } else {
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, "プレビューを計算中…",
                            egui::FontId::proportional(12.0), style::DIM);
                    }
                    if d.preview_for != Some(d.settings) {
                        ui.label(RichText::new("小さなプレビューを更新中…").size(10.5).color(style::DIM));
                    } else {
                        ui.label(RichText::new("SDFの簡易陰影プレビュー").size(10.5).color(style::DIM));
                    }
                });
                ui.add_space(4.0);
                ui.add(egui::Slider::new(&mut d.settings.bulge, 0.0..=1.0).text("ふくらみ").fixed_decimals(2))
                    .on_hover_text("大きくするほど、ふくらみが外側へ飛び出します");
                ui.add(egui::Slider::new(&mut d.settings.detail, 0.0..=1.0).text("凹凸・ばらつき").fixed_decimals(2))
                    .on_hover_text("ふくらみの数と大小のばらつきを変えます");
                ui.add(egui::Slider::new(&mut d.settings.roundness, 0.0..=1.0).text("丸み・なめらかさ").fixed_decimals(2))
                    .on_hover_text("球同士のなめらかな接合幅を変えます（既存のSDFメルト）");
                ui.add(egui::Slider::new(&mut d.settings.aspect, 0.60..=1.75).text("縦横比（高さ／幅）").fixed_decimals(2))
                    .on_hover_text("1.0は球形、1より大きいと縦長、小さいと平たい形");
                ui.horizontal(|ui| {
                    ui.label("シード");
                    ui.add(egui::DragValue::new(&mut d.settings.seed).range(0..=u32::MAX).speed(1.0));
                    if ui.button("別の形 ↻").clicked() {
                        d.settings.seed = d.settings.seed.wrapping_add(1);
                    }
                });
                ui.label(RichText::new(format!("編集できる球 {}個で生成（高解像度メッシュ化はしません）", d.settings.lobe_count() + 1))
                    .size(11.0).color(style::DIM));
                if app.scene.melt.amount <= 0.0 {
                    ui.label(RichText::new("メルト量が0です。滑らかな融合にはメルト量を上げてください")
                        .size(11.0).color(style::CUT));
                }
                if !d.error.is_empty() { ui.label(RichText::new(&d.error).color(style::CUT)); }
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("ブロブを追加").strong().color(Color32::WHITE))
                        .fill(style::ACCENT_SOFT).stroke(egui::Stroke::new(1.0, style::ACCENT))).clicked() {
                        apply = true;
                    }
                    if ui.button("やめる").clicked() { close = true; }
                });
                ui.label(RichText::new("追加後は通常の形として、移動・変形・複製・削除できます").size(10.5).color(style::DIM));
            });
        });

    if d.settings != before {
        d.settings = d.settings.sanitized();
        d.changed_at = Instant::now();
        d.error.clear();
        // The last slider event may be followed by no further input; schedule the
        // eventual low-res refresh without forcing continuous full-screen repaints.
        ctx.request_repaint_after(PREVIEW_SETTLE);
    }
    if apply {
        match insert(app, ctx, d.settings) {
            Ok(()) => close = true,
            Err(e) => d.error = e,
        }
    }
    if !close {
        ctx.data_mut(|data| data.insert_temp(id(), d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_formats::Format;
    use crate::export_job::{Options, Scope};
    use crate::export_mesh::Progress;
    use crate::sdf::Field;

    #[test]
    fn seed_is_reproducible_and_generates_editable_native_spheres() {
        let p = Settings::default();
        let a = generate(p);
        assert_eq!(a, generate(p));
        assert_ne!(a.shapes, generate(Settings { seed: p.seed + 1, ..p }).shapes);
        assert_eq!(a.shapes.len(), 1 + p.lobe_count());
        assert!(a.shapes.iter().all(|s| s.kind == Kind::Sphere && s.op == crate::model::Op::Add));
        assert!(a.shapes.iter().all(|s| s.pos.iter().all(|v| v.is_finite())));
        assert!(a.shapes.iter().skip(1).all(|s| s.blend > 0.0));
        assert!(a.shapes.iter().skip(1).any(|s| s.pos[0].abs() > 1.0));
    }

    #[test]
    fn slider_extremes_are_finite_and_every_lobe_overlaps_the_core() {
        for settings in [
            Settings { bulge: 0.0, detail: 0.0, roundness: 0.0, aspect: 0.60, seed: 0 },
            Settings { bulge: 1.0, detail: 1.0, roundness: 1.0, aspect: 1.75, seed: u32::MAX },
            Settings { bulge: f32::NAN, detail: f32::INFINITY, roundness: -5.0, aspect: 0.0, seed: 13 },
        ] {
            let g = generate(settings);
            let core = &g.shapes[0];
            for lobe in g.shapes.iter().skip(1) {
                assert!(lobe.size[0] > 0.0 && lobe.size[0].is_finite());
                let dx = lobe.pos[0] - core.pos[0];
                let dy = lobe.pos[1] - core.pos[1];
                let dz = (lobe.pos[2] - core.pos[2]) / core.scale[2];
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(distance < core.size[0] + lobe.size[0], "lobe detached at {distance}");
            }
            let sc = preview_scene(settings);
            let field = Field::new(&sc);
            for i in -12..=12 {
                let p = [i as f32 * 4.0, 3.5, core.pos[2]];
                assert!(field.distance(p).is_finite(), "invalid SDF distance at {p:?}");
            }
        }
    }

    #[test]
    fn save_load_and_undo_redo_keep_the_full_editable_blob() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        insert(&mut app, &ctx, Settings::default()).expect("insert blob");
        let built = app.scene.clone();
        assert_eq!(built.groups.len(), before.groups.len() + 1);
        assert_eq!(app.sel, Some(Pick::Group(built.groups.len() - 1)));
        let json = built.to_json();
        let after = Scene::from_json(&json).expect("load saved scene");
        assert_eq!(after, built, "ordinary spheres must round-trip without a special format");

        app.settle(&ctx);
        app.undo();
        assert_eq!(app.scene, before);
        app.redo();
        assert_eq!(app.scene, built);

        // Normal tools are not locked after preset insertion.
        let index = built.groups.len() - 1;
        app.scene.groups[index].shapes[1].pos[0] += 8.0;
        assert_ne!(app.scene, built);
    }

    /// Full mesh export uses the existing SDF/Boolean mesher; run in a release build.
    /// cargo test --release -p meltcad organic_blob_stl_export_roundtrip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn organic_blob_stl_export_roundtrip() {
        let mut scene = preview_scene(Settings::default());
        scene.melt.amount = 1.0;
        let opts = Options { scope: Scope::All, format: Format::ThreeMf, tolerance: 2.0, ..Options::default() };
        let built = crate::export_job::build(&scene, None, &opts, &Progress::new()).expect("mesh SDF blob");
        assert!(built.tris > 200, "too few triangles: {}", built.tris);
        for (format, extension) in [(Format::Stl, "stl"), (Format::ThreeMf, "3mf")] {
            let opt = Options { format, ..opts.clone() };
            let path = std::env::temp_dir().join(format!("organic_blob_{}.{}", std::process::id(), extension));
            let files = crate::export_job::serialize(&built, &opt, &path).expect("serialize mesh");
            assert_eq!(files.len(), 1);
            std::fs::write(&path, &files[0].1).expect("write completed output");
            let bytes = std::fs::read(&path).expect("reload exported file");
            let reloaded = crate::import_mesh::import_bytes(&bytes, &format!("organic_blob.{extension}"),
                &crate::import_mesh::ImportOptions::default()).expect("parse exported mesh");
            assert!(!reloaded.mesh.triangles.is_empty(), "empty {extension}");
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn dialog_only_mutates_the_scene_when_apply_is_pressed() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        open(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        assert_eq!(app.scene, before);
    }
}
