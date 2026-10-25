//! The small card for importing an SVG: size, height, rounding, a preview, and the buttons.
//! Its state lives in egui's memory, so `App` needs no new fields.
//!
//! Reading the file, parsing it and building the preview run on a worker thread. The card keeps
//! the previous picture and the scene untouched until that worker succeeds; Stop throws the
//! pending result away.

use egui::{Align2, Color32, Pos2, RichText, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use crate::app::App;
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;
use crate::svg::{self, Artwork, BuildOpts};

#[derive(Clone)]
struct Dlg {
    art: Option<Arc<Artwork>>,
    preview_mesh: Arc<egui::epaint::Mesh>,
    name: String,
    target: f32,
    height: f32,
    round: f32,
    flip: bool,
    msg: String,
    /// The worker reading and previewing the next file. The picture above stays until it finishes.
    job: Option<Arc<SvgJob<Loaded>>>,
    /// A dropped file waiting for a running worker to finish (only one is kept).
    pending: Option<PathBuf>,
}

/// The finished work of one worker: nothing here is visible to the user until it is applied.
struct Loaded {
    art: Artwork,
    preview: egui::epaint::Mesh,
    name: String,
    notes: String,
}

/// What the worker should read.
enum Source {
    File(PathBuf),
    Sample,
}

/// One SVG load on a worker thread; the same pattern as `mesh_dialog::Job` and `document_job::Job`.
/// The result is only published through `take`, and a cancelled job's result is never used.
struct SvgJob<T> {
    slot: Mutex<Option<Result<T, String>>>,
    cancel: AtomicBool,
    progress: AtomicU32,
}

impl<T: Send + 'static> SvgJob<T> {
    fn spawn(ctx: &egui::Context, f: impl FnOnce(&AtomicBool, &AtomicU32) -> Result<T, String> + Send + 'static) -> Arc<SvgJob<T>> {
        let job = Arc::new(SvgJob { slot: Mutex::new(None), cancel: AtomicBool::new(false), progress: AtomicU32::new(0) });
        let permit = match crate::work_budget::try_acquire(ctx) {
            Ok(permit) => permit,
            Err(e) => {
                if let Ok(mut slot) = job.slot.lock() {
                    *slot = Some(Err(e));
                }
                return job;
            }
        };
        let j = job.clone();
        let spawned = std::thread::Builder::new().name("meltcad-svg-import".into()).spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&j.cancel, &j.progress)))
                .unwrap_or_else(|_| Err("The SVG load stopped partway through".into()));
            let res = if j.cancel.load(Ordering::Relaxed) { Err(crate::import_mesh::IMPORT_CANCELLED.into()) } else { res };
            drop(permit); // free the budget before the UI can see a completed read
            if let Ok(mut slot) = j.slot.lock() {
                *slot = Some(res);
            }
        });
        if let Err(e) = spawned
            && let Ok(mut slot) = job.slot.lock()
        {
            *slot = Some(Err(format!("Couldn't start the SVG load: {e}")));
        }
        job
    }
    fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn fraction(&self) -> f32 {
        (self.progress.load(Ordering::Relaxed) as f32 / 1000.0).clamp(0.0, 1.0)
    }
    fn take(&self) -> Option<Result<T, String>> {
        let result = self.slot.lock().ok()?.take();
        match result {
            Some(_) if self.cancelled() => Some(Err(crate::import_mesh::IMPORT_CANCELLED.into())),
            other => other,
        }
    }
}

fn id() -> egui::Id {
    egui::Id::new("svg_import_card")
}

/// Open the card (menu entry "Import SVG…"). The file is chosen inside the card.
pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), Dlg::default()));
}

/// Used by the app to keep scene shortcuts out of the import dialog.
pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<Dlg>(id())).is_some()
}

impl Default for Dlg {
    fn default() -> Dlg {
        Dlg {
            art: None,
            preview_mesh: Arc::new(egui::epaint::Mesh::default()),
            name: "SVG".into(),
            target: 60.0,
            height: 10.0,
            round: 0.0,
            flip: false,
            msg: String::new(),
            job: None,
            pending: None,
        }
    }
}

/// Start a worker. It leaves `art`, `preview_mesh`, `name` and `msg` alone: a successful load
/// replaces them together, and a cancelled one changes nothing.
fn start(d: &mut Dlg, ctx: &egui::Context, source: Source) {
    d.job = Some(SvgJob::spawn(ctx, move |cancel, progress| {
        let stopped = || cancel.load(Ordering::Relaxed);
        if stopped() {
            return Err(crate::import_mesh::IMPORT_CANCELLED.into());
        }
        progress.store(50, Ordering::Relaxed);
        let art = match &source {
            Source::File(path) => svg::load_file(path),
            Source::Sample => svg::parse(svg::SAMPLE_SVG),
        }?;
        progress.store(500, Ordering::Relaxed);
        if stopped() {
            return Err(crate::import_mesh::IMPORT_CANCELLED.into());
        }
        let preview = preview_mesh(&art);
        progress.store(950, Ordering::Relaxed);
        if stopped() {
            return Err(crate::import_mesh::IMPORT_CANCELLED.into());
        }
        let mut notes = Vec::new();
        if art.dropped > 0 {
            notes.push(format!("Skipped {} tiny or extra outlines", art.dropped));
        }
        notes.extend(art.warnings.iter().cloned());
        let name = match &source {
            Source::File(path) => stem(path),
            Source::Sample => "Sample star".into(),
        };
        progress.store(1000, Ordering::Relaxed);
        Ok(Loaded { art, preview, name, notes: notes.join("\n") })
    }));
}

/// Apply a finished worker result, if there is one. Cancellation is not an error: the card and
/// the scene keep exactly what they showed before the load started.
fn poll(d: &mut Dlg) {
    let Some(job) = d.job.clone() else { return };
    let Some(result) = job.take() else { return };
    d.job = None;
    match result {
        Ok(loaded) => {
            d.msg = loaded.notes;
            d.preview_mesh = Arc::new(loaded.preview);
            d.art = Some(Arc::new(loaded.art));
            d.name = loaded.name;
        }
        Err(e) if e == crate::import_mesh::IMPORT_CANCELLED => {}
        Err(e) => {
            d.msg = e;
            d.art = None;
            d.preview_mesh = Arc::new(egui::epaint::Mesh::default());
        }
    }
}

fn stem(p: &std::path::Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "SVG".into())
}

/// Dropped `.svg` files open the card (other kinds are left for the rest of the app). The file is
/// queued here and started by `update`, so a drop never parses in the input callback.
fn take_drops(ctx: &egui::Context) {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    for f in files {
        let path = f.path().to_path_buf();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
            continue;
        }
        let mut d = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).unwrap_or_default();
        d.pending = Some(path);
        ctx.data_mut(|m| m.insert_temp(id(), d));
        ctx.request_repaint();
        break;
    }
}

/// Call once per frame: handles dropped files and shows the card when it is open.
pub fn update(app: &mut App, ctx: &egui::Context) {
    take_drops(ctx);
    let Some(mut d) = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())) else { return };
    poll(&mut d);
    if d.job.is_none()
        && let Some(path) = d.pending.take()
    {
        start(&mut d, ctx, Source::File(path));
    }
    let busy = d.job.is_some();
    let mut close = false;
    let mut go = false;
    let modal = egui::Modal::new(egui::Id::new("svg_import_area")).frame(egui::Frame::new()).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 380.0));
            ui.label(RichText::new("Import SVG").strong().size(16.0));
            ui.label(RichText::new("Turn filled outlines into a solid. Holes stay open.").size(12.0).color(style::DIM));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 170.0).max(140.0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if !busy
                        && style::pill(ui, "Choose SVG…", false).clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).pick_file()
                    {
                        start(&mut d, ctx, Source::File(p));
                    }
                    if !busy && style::pill(ui, "Try a sample", false).clicked() {
                        start(&mut d, ctx, Source::Sample);
                    }
                });
                if d.art.is_some() {
                    ui.label(RichText::new(&d.name).strong());
                } else {
                    ui.label(RichText::new("You can also drop a .svg file into the window.").size(12.0).color(style::DIM));
                }
                preview(ui, &d);
                if let Some(job) = &d.job {
                    ui.add(egui::ProgressBar::new(job.fraction()).show_percentage());
                    let phase = if job.cancelled() {
                        "Stopping…"
                    } else if job.fraction() < 0.5 {
                        "Reading and checking the SVG…"
                    } else {
                        "Building the preview…"
                    };
                    ui.label(RichText::new(phase).size(12.0).color(style::DIM));
                }
                if let Some(a) = &d.art {
                    let solids = a.solids();
                    let holes = a.holes();
                    ui.label(
                        RichText::new(format!(
                            "{:.1} × {:.1} mm  ·  {} {}  ·  {} {}",
                            a.aspect[0] * d.target,
                            a.aspect[1] * d.target,
                            solids,
                            if solids == 1 { "solid" } else { "solids" },
                            holes,
                            if holes == 1 { "hole" } else { "holes" }
                        ))
                        .size(12.0),
                    );
                    ui.label(RichText::new("Red outlines mark holes.").size(12.0).color(style::DIM));
                }
                if !d.msg.is_empty() {
                    ui.add_space(2.0);
                    ui.label(RichText::new("Import notes").strong().color(style::KEEP));
                    for line in d.msg.lines() {
                        ui.label(RichText::new(line).size(12.0).color(style::KEEP));
                    }
                }
                ui.add_space(4.0);
                ui.add_enabled_ui(d.art.is_some() && !busy && !app.scene_structure_locked(), |ui| {
                    egui::Grid::new("svg_import_grid").num_columns(2).spacing([10.0, 7.0]).show(ui, |ui| {
                        ui.label("Longest side");
                        style::field(ui, egui::DragValue::new(&mut d.target).speed(0.5).range(1.0..=500.0).suffix(" mm").max_decimals(1))
                            .on_hover_text("Width or height, whichever is longer. The aspect ratio stays fixed.");
                        ui.end_row();
                        ui.label("Extrude height");
                        style::field(ui, egui::DragValue::new(&mut d.height).speed(0.2).range(0.5..=200.0).suffix(" mm").max_decimals(1));
                        ui.end_row();
                        ui.label("Round edges");
                        let max_round = (d.height * 0.5).min(20.0);
                        d.round = d.round.min(max_round);
                        style::field(ui, egui::DragValue::new(&mut d.round).speed(0.05).range(0.0..=max_round).suffix(" mm").max_decimals(2))
                            .on_hover_text("Optional. Start at 0 mm to preserve fine details.");
                        ui.end_row();
                    });
                    ui.horizontal_wrapped(|ui| {
                        if let Some(a) = &d.art {
                            let natural = a.natural_mm[0].max(a.natural_mm[1]);
                            if style::pill(ui, "Original size", false)
                                .on_hover_text(format!("{:.1} × {:.1} mm (maximum 500 mm)", a.natural_mm[0], a.natural_mm[1]))
                                .clicked()
                            {
                                d.target = natural.clamp(1.0, 500.0);
                            }
                        }
                        ui.checkbox(&mut d.flip, "Flip vertically");
                    });
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if let Some(job) = &d.job {
                    if style::pill(ui, "Stop loading", false).clicked() {
                        job.cancel();
                    }
                } else {
                    ui.add_enabled_ui(d.art.is_some(), |ui| {
                        let b = egui::Button::new(RichText::new("Import solid").strong().color(Color32::WHITE))
                            .fill(style::ACCENT_SOFT)
                            .stroke(Stroke::new(1.0, style::ACCENT));
                        if ui.add_sized([126.0, 32.0], b).clicked() {
                            go = true;
                        }
                    });
                }
                if ui.add_sized([80.0, 32.0], egui::Button::new("Cancel")).on_hover_text("Cancel import · Esc").clicked() {
                    close = true;
                }
            });
        });
    });
    close |= modal.should_close();
    if close {
        // Stop the worker when the card closes; its permit is released only when it really exits.
        if let Some(job) = &d.job {
            job.cancel();
        }
    }
    if go && !close {
        close = import(app, ctx, &d);
    }
    if !close {
        if d.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        ctx.data_mut(|m| m.insert_temp(id(), d));
    }
}

/// Cache the filled preview once per import. Reuses the geometry crate's hole-aware triangulation;
/// complex regions fall back to outlines rather than doing unbounded work in a UI frame.
fn preview_mesh(a: &Artwork) -> egui::epaint::Mesh {
    use solidcraft_geom::{Loop2, Region2};
    const MAX_VERTICES: usize = 150_000;
    let mut mesh = egui::epaint::Mesh::default();
    let to_loop = |c: &svg::Contour| Loop2::polygon(&c.pts.iter().map(|p| solidcraft_geom::Vec2::new(p[0] as f64, p[1] as f64)).collect::<Vec<_>>());
    let mut contours = a.contours.iter();
    while let Some(solid) = contours.next() {
        let holes = contours.as_slice().iter().take_while(|c| c.hole).count();
        let holes = contours.by_ref().take(holes).collect::<Vec<_>>();
        let points = solid.pts.len() + holes.iter().map(|c| c.pts.len()).sum::<usize>();
        // A slab triangulation uses at most 2 * points² triangles.
        let bound = points.saturating_mul(points).saturating_mul(6);
        if solid.hole || bound > MAX_VERTICES.saturating_sub(mesh.vertices.len()) {
            continue;
        }
        let region = Region2 { outer: to_loop(solid), holes: holes.into_iter().map(to_loop).collect() };
        let [r, g, b] = solid.color.unwrap_or([0.93, 0.56, 0.45]);
        let ink = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        for tri in region.triangulate(1e-6) {
            if mesh.vertices.len() + 3 > MAX_VERTICES {
                break;
            }
            let base = mesh.vertices.len() as u32;
            for p in tri {
                mesh.colored_vertex(Pos2::new(p.x as f32, p.y as f32), ink);
            }
            mesh.add_triangle(base, base + 1, base + 2);
        }
    }
    mesh
}

/// The exact retained outlines and their cached fill, with holes marked in red.
fn preview(ui: &mut egui::Ui, d: &Dlg) {
    let size = Vec2::new(ui.available_width(), 180.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, Color32::from_rgb(232, 234, 236));
    let Some(a) = &d.art else {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Choose an SVG to preview its outlines",
            egui::FontId::proportional(12.5),
            Color32::from_gray(86),
        );
        return;
    };
    let inner = rect.shrink(14.0);
    // the longest side of the picture fills the box
    let k = (inner.width() / a.aspect[0].max(1e-3)).min(inner.height() / a.aspect[1].max(1e-3));
    let flip = if d.flip { -1.0 } else { 1.0 };
    let at = |q: &[f32; 2]| Pos2::new(inner.center().x + q[0] * k, inner.center().y - q[1] * k * flip);
    let mut mesh = (*d.preview_mesh).clone();
    for v in &mut mesh.vertices {
        v.pos = at(&[v.pos.x, v.pos.y]);
    }
    p.add(egui::Shape::mesh(mesh));
    for c in &a.contours {
        let pts: Vec<Pos2> = c.pts.iter().map(at).collect();
        let col = if c.hole { Color32::from_rgb(165, 42, 62) } else { Color32::from_gray(54) };
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.2, col)));
    }
}

/// Put the picture into the scene as a new group. Returns whether the card can close.
fn import(app: &mut App, ctx: &egui::Context, d: &Dlg) -> bool {
    if !app.allow_scene_structure_change() {
        return false;
    }
    let Some(art) = &d.art else { return false };
    if app.scene.groups.len() >= MAX_GROUPS {
        app.say(ctx, format!("Up to {MAX_GROUPS} groups"));
        return false;
    }
    let room = MAX_SHAPES.saturating_sub(app.scene.shape_count());
    if room == 0 {
        app.say(ctx, format!("Up to {MAX_SHAPES} shapes"));
        return false;
    }
    let o = BuildOpts { target_mm: d.target, height_mm: d.height, round_mm: d.round, flip: d.flip, at: [app.cam.target[0], app.cam.target[1]] };
    let groups_room = MAX_GROUPS.saturating_sub(app.scene.groups.len());
    let (groups, note) = svg::build_group(art, &d.name, &o, room, groups_room);
    let n: usize = groups.iter().map(|g| g.shapes.len()).sum();
    if n == 0 {
        app.say(ctx, note.unwrap_or_else(|| "No shapes could be added".into()));
        return false;
    }
    let first = app.scene.groups.len();
    app.scene.groups.extend(groups);
    app.sel = Some(Pick::Group(first));
    app.say(ctx, note.unwrap_or_else(|| format!("Added SVG ({n} shapes)")));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn temp_svg(name: &str, text: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("yura_svg_load_{}_{}_{}.svg", std::process::id(), NEXT_FILE.fetch_add(1, Ordering::Relaxed), name));
        std::fs::write(&path, text).expect("write the SVG fixture");
        path
    }

    fn frame(ctx: &egui::Context, app: &mut App) {
        ctx.run_ui(egui::RawInput::default(), |ui| update(app, ui.ctx())).textures_delta.clear();
    }

    /// Start a load and wait for its worker without going through the card (for the setup of a test).
    fn load_now(ctx: &egui::Context, d: &mut Dlg, source: Source) {
        start(d, ctx, source);
        for _ in 0..1200 {
            poll(d);
            if d.job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the SVG worker did not finish");
    }

    /// Let the card draw frames until its worker has published a result.
    fn wait_for_job(ctx: &egui::Context, app: &mut App) {
        for _ in 0..1200 {
            frame(ctx, app);
            if ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.job.is_none()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the SVG worker did not finish");
    }

    /// A large but legal file: enough elements to make the parse measurable.
    fn large_svg() -> String {
        let mut s = String::with_capacity(800_000);
        s.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000">"#);
        for i in 0..12_000 {
            let (x, y) = ((i % 100) * 8, (i / 100) * 8);
            s.push_str(&format!(r##"<rect x="{x}" y="{y}" width="6" height="6" fill="#336699"/>"##));
        }
        s.push_str("</svg>");
        s
    }

    #[derive(Debug)]
    struct FakeDrop(PathBuf);
    impl egui::DroppedFile for FakeDrop {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Err("this card reads the path, not the bytes".into())
        }
    }

    #[test]
    fn the_card_draws_and_imports_the_sample() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let mut d = Dlg::default();
        load_now(&ctx, &mut d, Source::Sample);
        assert!(d.art.is_some() && d.msg.is_empty());
        ctx.data_mut(|m| m.insert_temp(id(), d.clone()));
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let (groups, shapes) = (app.scene.groups.len(), app.scene.shape_count());
        let before = app.scene.clone();
        assert!(import(&mut app, &ctx, &d));
        assert_eq!((app.scene.groups.len(), app.scene.shape_count()), (groups + 2, shapes + 3));
        assert_eq!(app.sel, Some(Pick::Group(groups)));
        // the whole import is one undo step, even though it is several groups
        app.settle(&ctx);
        assert_eq!(app.undo_depth(), 1);
        app.undo();
        assert_eq!(app.scene, before);
        // a bad file only shows a message
        let bad = temp_svg("bad", "hello");
        let mut broken = Dlg::default();
        load_now(&ctx, &mut broken, Source::File(bad.clone()));
        assert!(broken.art.is_none() && !broken.msg.is_empty());
        assert!(!import(&mut app, &ctx, &broken));
        let _ = std::fs::remove_file(bad);
    }

    #[test]
    fn a_large_svg_loads_on_a_worker_without_blocking_the_frame() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        open(&ctx);
        frame(&ctx, &mut app);
        let path = temp_svg("large", &large_svg());
        let input = egui::RawInput {
            dropped_files: vec![Arc::new(FakeDrop(path.clone()))],
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 700.0))),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).expect("the card shows the load");
        assert!(d.art.is_none(), "the drop frame did not read and parse the SVG itself");
        assert!(d.job.is_some(), "a worker owns the file read, the parse and the preview");
        assert_eq!(app.scene, before, "the scene changes only when Import is pressed");
        for _ in 0..1200 {
            frame(&ctx, &mut app);
            if ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.art.is_some() && d.job.is_none()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).expect("the card stays open");
        assert!(d.art.is_some(), "the large file still finishes and shows its preview");
        assert_eq!(app.scene, before, "waiting for the worker never edits the scene");
        assert!(import(&mut app, &ctx, &d), "a normal load still imports after the worker finishes");
        assert!(app.scene.shape_count() > before.shape_count());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_cancelled_load_keeps_the_scene_and_card_state() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx);
        frame(&ctx, &mut app);
        let mut d = ctx.data(|m| m.get_temp::<Dlg>(id())).expect("the card is open");
        load_now(&ctx, &mut d, Source::Sample);
        assert!(d.art.is_some());
        ctx.data_mut(|m| m.insert_temp(id(), d));
        frame(&ctx, &mut app);
        let before_scene = app.scene.clone();
        let before = ctx.data(|m| m.get_temp::<Dlg>(id())).expect("the card is open");
        // Start another load and stop it before it can publish anything.
        let mut d = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).expect("the card is open");
        start(&mut d, &ctx, Source::Sample);
        d.job.as_ref().expect("the worker started").cancel();
        ctx.data_mut(|m| m.insert_temp(id(), d));
        wait_for_job(&ctx, &mut app);
        let after = ctx.data(|m| m.get_temp::<Dlg>(id())).expect("Cancel keeps the card open");
        assert!(after.job.is_none(), "the stopped worker releases the card");
        assert!(Arc::ptr_eq(after.art.as_ref().expect("the old picture is kept"), before.art.as_ref().expect("old")));
        assert!(Arc::ptr_eq(&after.preview_mesh, &before.preview_mesh), "the old preview is kept");
        assert_eq!((after.name.as_str(), after.msg.as_str()), (before.name.as_str(), before.msg.as_str()));
        assert_eq!((after.target, after.height, after.round, after.flip), (before.target, before.height, before.round, before.flip));
        assert_eq!(app.scene, before_scene, "a cancelled load never touches the scene");
    }

    #[test]
    fn escape_closes_the_import_card_without_importing() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        open(&ctx);
        assert!(is_open(&ctx));
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        assert!(!is_open(&ctx));
        assert_eq!(app.scene, before);
    }

    #[test]
    fn filled_preview_preserves_concavity_and_holes() {
        let a = svg::parse(r#"<svg><path fill-rule="evenodd" d="M0 0 H80 V80 H0 Z M20 20 H60 V60 H20 Z"/></svg>"#).unwrap();
        let mesh = preview_mesh(&a);
        let area: f32 = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [p, q, r] = [mesh.vertices[t[0] as usize].pos, mesh.vertices[t[1] as usize].pos, mesh.vertices[t[2] as usize].pos];
                ((q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)).abs() * 0.5
            })
            .sum();
        assert!((area - 0.75).abs() < 1e-5, "the 40 × 40 hole stays open in an 80 × 80 square: {area}");
        let star = svg::parse(svg::SAMPLE_SVG).unwrap();
        let mesh = preview_mesh(&star);
        assert!(!mesh.indices.is_empty());
        assert!(mesh.vertices.iter().all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()));
    }

    /// A review image of the real egui card, including the file's filled outline preview.
    #[test]
    #[ignore]
    fn svg_import_preview() {
        use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};
        let ctx = egui::Context::default();
        style::apply(&ctx);
        crate::fonts::setup(&ctx);
        let mut app = App::new(true, None);
        let mut d = Dlg::default();
        load_now(&ctx, &mut d, Source::Sample);
        ctx.data_mut(|m| m.insert_temp(id(), d));
        let ppp = 2.0;
        ctx.set_pixels_per_point(ppp);
        let size = egui::vec2(620.0, 700.0);
        let mut tex = Textures::new();
        let mut final_shapes = Vec::new();
        for frame in 0..12 {
            let input = egui::RawInput {
                time: Some(frame as f64 / 60.0),
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| update(&mut app, ui.ctx()));
            apply_deltas(&mut tex, &out.textures_delta);
            out.textures_delta.clear();
            final_shapes = out.shapes;
        }
        let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
        let prims = ctx.tessellate(final_shapes, ppp);
        let image = raster(&prims, &tex, w, h, ppp, [0.15; 3]);
        write_ppm(image, w, h, &std::env::var("MELT_SVG_PREVIEW").unwrap_or_else(|_| "/tmp/melt_svg_preview.ppm".into()));
    }
}
