//! The card for importing a mesh ("メッシュを読み込む"): file name and type, triangles, size, health
//! notes, then the options (target size, which way is up, put it on the floor, put it in the middle,
//! how fine), and a progress bar with a cancel button while the distance volume is baked on a
//! worker thread. The result is a `Kind::Mesh` shape in a new group at the middle of the view.
//! The state lives in egui's memory like the other cards, so `App` needs no new fields.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{Align2, Color32, RichText, Stroke};

use crate::app::App;
use crate::mesh_import::{self, Baked, Loaded, RES_CHOICES, Settings, Up};
use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Material, Pick};
use crate::sdf_volume::{BakeControl, ERR_CANCEL};
use crate::style;

/// A job on another thread whose answer is picked up later.
struct Job<T> {
    slot: Mutex<Option<Result<T, String>>>,
}

impl<T: Send + 'static> Job<T> {
    fn spawn(name: &str, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Arc<Job<T>> {
        let job = Arc::new(Job { slot: Mutex::new(None) });
        let j = job.clone();
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err("途中で止まってしまいました".into()));
            if let Ok(mut s) = j.slot.lock() {
                *s = Some(res);
            }
        });
        if spawned.is_err()
            && let Ok(mut s) = job.slot.lock()
        {
            *s = Some(Err("読みこみを始められませんでした".into()));
        }
        job
    }
    fn take(&self) -> Option<Result<T, String>> {
        self.slot.lock().ok()?.take()
    }
}

struct BakeJob {
    job: Arc<Job<Baked>>,
    ctl: Arc<BakeControl>,
    started: Instant,
    /// What it is baking (the card may be changed meanwhile, the shape is made from these).
    loaded: Arc<Loaded>,
    settings: Settings,
}

#[derive(Clone)]
struct Dlg {
    /// Reading the file.
    reading: Option<(Arc<Job<Loaded>>, PathBuf)>,
    loaded: Option<Arc<Loaded>>,
    st: Settings,
    bake: Option<Arc<BakeJob>>,
    msg: String,
    open: bool,
}

impl Default for Dlg {
    fn default() -> Dlg {
        Dlg { reading: None, loaded: None, st: Settings::new([60.0; 3]), bake: None, msg: String::new(), open: false }
    }
}

fn id() -> egui::Id {
    egui::Id::new("mesh_import_card")
}

/// Menu entry "メッシュを読み込む…": ask for a file, then show the card.
pub fn open(ctx: &egui::Context) {
    let Some(path) = rfd::FileDialog::new().add_filter("メッシュ (STL・OBJ・3MF・PLY・GLB)", &mesh_import::EXTENSIONS).pick_file() else { return };
    open_path(ctx, path);
}

/// Show the card for this file (a dropped file, or the one just chosen).
pub fn open_path(ctx: &egui::Context, path: PathBuf) {
    let mut d = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).filter(|d| d.open).unwrap_or_default();
    if let Some(b) = &d.bake {
        b.ctl.cancel.store(true, Ordering::Relaxed);
    }
    d.bake = None;
    d.loaded = None;
    d.msg.clear();
    d.open = true;
    let p = path.clone();
    d.reading = Some((Job::spawn("meltcad-mesh-read", move || mesh_import::load(&p)), path));
    ctx.data_mut(|m| m.insert_temp(id(), d));
    ctx.request_repaint();
}

/// Dropped mesh files open the card (other kinds are left for the rest of the app).
fn take_drops(ctx: &egui::Context) {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    for f in files {
        let path = f.path().to_path_buf();
        if mesh_import::is_mesh_file(&path) {
            open_path(ctx, path);
            break;
        }
    }
}

fn count_text(n: usize) -> String {
    if n >= 10_000 { format!("{:.1}万", n as f64 / 1.0e4) } else { format!("{n}") }
}

fn time_text(s: f64) -> String {
    if s < 1.5 {
        "1秒くらい".into()
    } else if s < 90.0 {
        format!("{s:.0}秒くらい")
    } else {
        format!("{:.0}分くらい", s / 60.0)
    }
}

fn bytes_text(b: f64) -> String {
    if b >= 1.0e6 { format!("{:.1}MB", b / 1.0e6) } else { format!("{:.0}KB", (b / 1.0e3).max(1.0)) }
}

fn size_text(s: [f32; 3]) -> String {
    format!("{:.1} × {:.1} × {:.1} mm", s[0], s[1], s[2])
}

/// Friendly words for an error from the readers or the bake.
fn jp_error(e: &str) -> String {
    if e == ERR_CANCEL { "やめました".into() } else { e.to_string() }
}

fn start_bake(d: &mut Dlg) {
    let Some(loaded) = d.loaded.clone() else { return };
    let settings = d.st.sanitized();
    let ctl = Arc::new(BakeControl::new());
    let (l, c) = (loaded.clone(), ctl.clone());
    let job = Job::spawn("meltcad-mesh-bake", move || mesh_import::bake(&l.imported.mesh, l.imported.format, &settings, &c));
    d.bake = Some(Arc::new(BakeJob { job, ctl, started: Instant::now(), loaded, settings }));
    d.msg.clear();
}

/// The shape and group of a baked mesh go into the scene. Returns whether the card can close.
fn finish(app: &mut App, ctx: &egui::Context, b: &BakeJob, baked: &Baked) -> bool {
    if app.scene.groups.len() >= MAX_GROUPS {
        app.say(ctx, format!("グループは{MAX_GROUPS}個までです"));
        return false;
    }
    if app.scene.shape_count() >= MAX_SHAPES {
        app.say(ctx, format!("形は{MAX_SHAPES}個までです"));
        return false;
    }
    if let Some(why) = mesh_import::refuse(&app.scene, baked.volume.nodes()) {
        app.say(ctx, why);
        return false;
    }
    let st = b.settings;
    let pos = mesh_import::place(baked, st.floor, st.centre, [app.cam.target[0], app.cam.target[1]]);
    let clay = Material::plain([0.80, 0.80, 0.83], 0.5, 0.0);
    let shape = mesh_import::shape_of(baked, &b.loaded.stem, &b.loaded.file_name, pos, clay);
    let mut group = Group::new(b.loaded.stem.clone());
    group.shapes.push(shape);
    app.scene.groups.push(group);
    let gi = app.scene.groups.len() - 1;
    app.sel = Some(Pick::Shape(gi, 0));
    app.frame_view(true);
    let secs = baked.stats.bvh_secs + baked.stats.sign_secs + baked.stats.dist_secs;
    app.say(ctx, format!("メッシュを入れました: {}（{}・{:.1}秒）", b.loaded.file_name, size_text(baked.size), secs));
    true
}

/// Call once per frame: handles dropped files, the running jobs, and shows the card when it is open.
pub fn update(app: &mut App, ctx: &egui::Context) {
    take_drops(ctx);
    let Some(mut d) = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).filter(|d| d.open) else { return };
    // the file has been read
    if let Some((job, path)) = d.reading.clone() {
        match job.take() {
            Some(Ok(l)) => {
                d.st = Settings::new(l.size);
                d.msg.clear();
                d.loaded = Some(Arc::new(l));
                d.reading = None;
            }
            Some(Err(e)) => {
                d.msg = format!("{}: {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), e);
                d.reading = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(80)),
        }
    }
    // the bake has finished
    let mut close = false;
    if let Some(b) = d.bake.clone() {
        match b.job.take() {
            Some(Ok(baked)) => {
                d.bake = None;
                close = finish(app, ctx, &b, &baked);
            }
            Some(Err(e)) => {
                d.bake = None;
                d.msg = jp_error(&e);
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(60)),
        }
    }
    let mut go = false;
    let mut pick = false;
    let busy_read = d.reading.is_some();
    egui::Area::new(egui::Id::new("mesh_import_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("メッシュを読み込む").strong().size(15.0));
            ui.label(RichText::new("STL・OBJ・3MF・PLY・GLB の立体を、ほかの形と同じようにとかしたり くり抜いたりできます").size(11.5).color(style::DIM));
            ui.add_space(4.0);
            if let Some(b) = d.bake.clone() {
                baking(ui, &b);
            } else if busy_read {
                ui.label(RichText::new("ファイルを読んでいます…").color(style::DIM));
                if let Some((_, p)) = &d.reading {
                    ui.label(RichText::new(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).size(11.5).color(style::DIM));
                }
            } else {
                options(ui, &mut d, app, &mut pick);
            }
            if !d.msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&d.msg).size(11.5).color(style::CUT));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if d.bake.is_some() {
                    if style::pill(ui, "中止", false).clicked()
                        && let Some(b) = &d.bake
                    {
                        b.ctl.cancel.store(true, Ordering::Relaxed);
                    }
                } else {
                    ui.add_enabled_ui(d.loaded.is_some(), |ui| {
                        let b = egui::Button::new(RichText::new("読みこむ").strong().color(Color32::WHITE)).fill(style::ACCENT_SOFT).stroke(Stroke::new(1.0, style::ACCENT));
                        if ui.add(b).clicked() {
                            go = true;
                        }
                    });
                    if ui.button("やめる").clicked() {
                        close = true;
                    }
                }
            });
        });
    });
    if pick && let Some(p) = rfd::FileDialog::new().add_filter("メッシュ", &mesh_import::EXTENSIONS).pick_file() {
        ctx.data_mut(|m| m.insert_temp(id(), d));
        open_path(ctx, p);
        return;
    }
    if go {
        let nodes = d.loaded.as_ref().map_or(0, |l| estimate_of(l, &d.st).nodes);
        match mesh_import::refuse(&app.scene, nodes) {
            Some(why) => d.msg = why.to_string(),
            None => start_bake(&mut d),
        }
    }
    if close {
        if let Some(b) = &d.bake {
            b.ctl.cancel.store(true, Ordering::Relaxed);
        }
        return;
    }
    ctx.data_mut(|m| m.insert_temp(id(), d));
}

fn estimate_of(l: &Loaded, st: &Settings) -> mesh_import::Estimate {
    let size = mesh_import::upright_size(l.size, st.up.is_y(l.imported.format));
    // the model's size after scaling to the target
    let longest = size.into_iter().fold(0.0f32, f32::max).max(1e-6);
    let k = st.target_mm.clamp(mesh_import::MIN_TARGET_MM, mesh_import::MAX_TARGET_MM) / longest;
    mesh_import::estimate([size[0] * k, size[1] * k, size[2] * k], l.imported.mesh.triangles.len(), st.res)
}

fn baking(ui: &mut egui::Ui, b: &BakeJob) {
    ui.label(RichText::new(format!("かたちをはかっています… {}", b.loaded.file_name)).size(13.0));
    ui.label(RichText::new("まわりの空間までの きょりを 計算しています").size(11.5).color(style::DIM));
    let frac = (b.ctl.progress.load(Ordering::Relaxed) as f32 / 1000.0).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 5.0, style::PANEL_DARK);
    let w = rect.width() * frac;
    if w > 1.0 {
        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height())), 5.0, style::ACCENT);
    }
    ui.label(RichText::new(format!("{:.0}%　{:.0}秒", frac * 100.0, b.started.elapsed().as_secs_f64())).size(11.5).color(style::DIM));
}

fn options(ui: &mut egui::Ui, d: &mut Dlg, app: &App, pick: &mut bool) {
    ui.horizontal(|ui| {
        if style::pill(ui, "ファイルをえらぶ…", false).clicked() {
            *pick = true;
        }
    });
    ui.label(RichText::new("（ファイルを ここにドラッグしてもOK）").size(11.0).color(style::DIM));
    let Some(l) = d.loaded.clone() else { return };
    ui.add_space(2.0);
    let imp = &l.imported;
    let dropped = if imp.dropped_triangles > 0 { format!("（面積のない {} 個はのぞきました）", imp.dropped_triangles) } else { String::new() };
    ui.label(RichText::new(format!("{}　{}　三角形 {}{}", l.file_name, imp.format.label(), count_text(imp.mesh.triangles.len()), dropped)).size(12.5));
    let y_up = d.st.up.is_y(imp.format);
    ui.label(RichText::new(format!("もとの大きさ {}", size_text(mesh_import::upright_size(l.size, y_up)))).size(11.5).color(style::DIM));
    for n in &imp.notes {
        ui.label(RichText::new(format!("・{n}")).size(11.5).color(style::KEEP));
    }
    ui.add_space(2.0);
    egui::Grid::new("mesh_import_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label(RichText::new("目標の大きさ").color(style::DIM));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut d.st.target_mm).speed(0.5).range(mesh_import::MIN_TARGET_MM..=mesh_import::MAX_TARGET_MM).suffix(" mm").max_decimals(1))
                .on_hover_text("いちばん長い辺の長さ");
            if style::pill(ui, "もとの大きさ", false).on_hover_text("ファイルの大きさのまま（200mmより大きいときは200mm）").clicked() {
                d.st.target_mm = mesh_import::default_target(l.size);
            }
        });
        ui.end_row();
        ui.label(RichText::new("向き").color(style::DIM));
        let items: Vec<(&str, &str)> = Up::ALL.iter().map(|u| (u.label(), u.hint())).collect();
        let cur = Up::ALL.iter().position(|u| *u == d.st.up);
        if let Some(i) = style::segmented(ui, "mesh_up", &items, cur, false)
            && let Some(u) = Up::ALL.get(i)
        {
            d.st.up = *u;
        }
        ui.end_row();
        ui.label("");
        ui.vertical(|ui| {
            style::checkbox(ui, &mut d.st.floor, "床に置く").on_hover_text("いちばん下を 床（高さ0）にそろえます");
            style::checkbox(ui, &mut d.st.centre, "まんなかに置く").on_hover_text("いま見ている場所の まんなかに置きます。切ると、ファイルの位置のまま");
        });
        ui.end_row();
        ui.label(RichText::new("細かさ").color(style::DIM));
        ui.horizontal(|ui| {
            for r in RES_CHOICES {
                if style::pill(ui, &r.to_string(), d.st.res == r).on_hover_text("いちばん長い辺を 何マスに分けるか。大きいほど きれいで、重くなります").clicked() {
                    d.st.res = r;
                }
            }
        });
        ui.end_row();
    });
    let e = estimate_of(&l, &d.st);
    let refuse = mesh_import::refuse(&app.scene, e.nodes);
    let col = if refuse.is_some() { style::CUT } else { style::DIM };
    ui.label(RichText::new(format!("目安: {}・ファイル {}・マス {}", time_text(e.secs), bytes_text(e.bytes), count_text(e.nodes))).size(11.5).color(col));
    if let Some(why) = refuse {
        ui.label(RichText::new(why).size(11.5).color(style::CUT));
    }
}

/// The part of the inspector for a mesh shape: where it came from, how big it is, how fine.
pub fn inspector(ui: &mut egui::Ui, sh: &mut crate::model::Shape) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("もとのファイル").color(style::DIM).size(12.0));
        ui.label(RichText::new(if sh.source.is_empty() { "（不明）" } else { &sh.source }).size(12.0));
    });
    let now = [sh.size[0] * sh.scale[0], sh.size[1] * sh.scale[1], sh.size[2] * sh.scale[2]];
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("大きさ").color(style::DIM).size(12.0));
        ui.label(RichText::new(size_text(now)).size(12.0));
    });
    if let Some(v) = &sh.volume {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("細かさ").color(style::DIM).size(12.0));
            ui.label(RichText::new(format!("1マス {:.2} mm　{}×{}×{}マス", v.cell, v.dims[0], v.dims[1], v.dims[2])).size(12.0));
        });
    } else {
        ui.label(RichText::new("データがありません").size(12.0).color(style::CUT));
    }
    if sh.scale != [1.0; 3] && style::pill(ui, "もとの大きさにもどす", false).clicked() {
        sh.scale = [1.0; 3];
    }
}

/// For the tests and the palette: is this a file the card reads?
#[allow(dead_code)]
pub fn reads(path: &Path) -> bool {
    mesh_import::is_mesh_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    /// Run frames until `done` says so (the worker threads need a little time).
    fn pump(app: &mut App, ctx: &egui::Context, done: impl Fn(&App) -> bool) {
        for _ in 0..400 {
            ctx.run_ui(egui::RawInput::default(), |ui| update(app, ui.ctx())).textures_delta.clear();
            if done(app) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    #[test]
    fn the_card_reads_a_file_bakes_it_and_adds_a_selected_mesh_in_a_new_group() {
        let dir = std::env::temp_dir().join(format!("meltcad_card_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("かたまり.stl");
        std::fs::write(&path, mesh_import::tests::sample_stl()).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let (groups, shapes) = (app.scene.groups.len(), app.scene.shape_count());
        open_path(&ctx, path);
        // reading
        pump(&mut app, &ctx, |_| ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.loaded.is_some()));
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        let l = d.loaded.clone().expect("the file was read");
        assert_eq!(l.file_name, "かたまり.stl");
        assert!(l.imported.mesh.triangles.len() > 100 && d.msg.is_empty(), "{}", d.msg);
        // options: smaller and coarse so the test is quick
        let mut d = d;
        d.st.res = 48;
        d.st.target_mm = 30.0;
        d.st.floor = true;
        d.st.centre = true;
        app.cam.target = [40.0, -10.0, 0.0];
        start_bake(&mut d);
        ctx.data_mut(|m| m.insert_temp(id(), d));
        pump(&mut app, &ctx, |a| a.scene.groups.len() > groups);
        assert_eq!((app.scene.groups.len(), app.scene.shape_count()), (groups + 1, shapes + 1), "a new group with one shape");
        let g = app.scene.groups.len() - 1;
        let sh = app.scene.groups[g].shapes[0].clone();
        assert_eq!(sh.kind, Kind::Mesh);
        assert!(sh.volume.is_some());
        assert_eq!((sh.source.as_str(), app.scene.groups[g].name.as_str()), ("かたまり.stl", "かたまり"));
        assert!((sh.pos[0] - 40.0).abs() < 1e-3 && (sh.pos[1] + 10.0).abs() < 1e-3, "at the view's middle: {:?}", sh.pos);
        assert!((sh.pos[2] - sh.size[2] * 0.5).abs() < 1e-3, "on the floor");
        assert!(sh.size.iter().cloned().fold(0.0, f32::max) > 29.0 && sh.size.iter().cloned().fold(0.0, f32::max) < 31.0);
        assert_eq!(app.sel, Some(Pick::Shape(g, 0)));
        // the card closed
        assert!(ctx.data(|m| m.get_temp::<Dlg>(id())).is_none());
        // picking works through the CPU field (settle rebuilds it): the middle of the mesh is hit from above
        app.settle(&ctx);
        let top = sh.pos[2] + sh.size[2];
        let hit = app.field().ray([sh.pos[0], sh.pos[1], top + 40.0], [0.0, 0.0, -1.0], 500.0);
        assert!(hit.is_some_and(|(t, _)| t > 20.0 && t < 60.0), "{hit:?}");
        // undo takes it away again, and redo brings the very same volume back
        app.undo();
        assert_eq!(app.scene.groups.len(), groups);
        app.redo();
        assert!(app.scene.groups[g].shapes[0].volume.as_ref().is_some_and(|v| Some(v) == sh.volume.as_ref()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_file_only_shows_a_message_and_cancel_stops_a_bake() {
        let dir = std::env::temp_dir().join(format!("meltcad_card_bad_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let bad = dir.join("kowareta.stl");
        std::fs::write(&bad, b"solid x\nthis is not a mesh\n").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open_path(&ctx, bad);
        pump(&mut app, &ctx, |_| ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.reading.is_none()));
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        assert!(d.loaded.is_none() && !d.msg.is_empty() && d.open, "{}", d.msg);
        assert_eq!(app.scene.shape_count(), 1);
        // cancelling a bake leaves the scene alone
        let good = dir.join("ok.stl");
        std::fs::write(&good, mesh_import::tests::sample_stl()).unwrap();
        open_path(&ctx, good);
        pump(&mut app, &ctx, |_| ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.loaded.is_some()));
        let mut d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        d.st.res = 160;
        start_bake(&mut d);
        if let Some(b) = &d.bake {
            b.ctl.cancel.store(true, Ordering::Relaxed);
        }
        ctx.data_mut(|m| m.insert_temp(id(), d));
        pump(&mut app, &ctx, |_| ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.bake.is_none()));
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        assert_eq!(app.scene.shape_count(), 1, "nothing was added");
        assert!(d.msg == "やめました" || d.loaded.is_some(), "{}", d.msg);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropped_files_open_the_card_only_for_mesh_types() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        #[derive(Debug)]
        struct Fake(PathBuf);
        impl egui::DroppedFile for Fake {
            fn path(&self) -> &Path {
                &self.0
            }
            fn bytes(&self) -> Result<Vec<u8>, String> {
                Err("not a real file".into())
            }
        }
        let drop = |name: &str| egui::RawInput { dropped_files: vec![Arc::new(Fake(PathBuf::from(name)))], ..Default::default() };
        ctx.run_ui(drop("/nonexistent/a.svg"), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        assert!(ctx.data(|m| m.get_temp::<Dlg>(id())).is_none());
        ctx.run_ui(drop("/nonexistent/model.STL"), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        assert!(ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.open));
        // (the file does not exist: the card says so instead of failing)
        pump(&mut app, &ctx, |_| ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| d.reading.is_none()));
        assert!(ctx.data(|m| m.get_temp::<Dlg>(id())).is_some_and(|d| !d.msg.is_empty()));
    }
}
