//! The library card: ready-made models in tabs, with a search box and small pictures. A click puts
//! the model into the scene as a new group next to what is already there.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::library::{self, Item};
use crate::model::{BlendMode, MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;

const THUMB: usize = 62;
const CELL: f32 = 78.0;
const ROW_HEIGHT: f32 = 100.0;
const MAX_QUEUE: usize = 48;
const MAX_DONE: usize = 32;
const MAX_TEXTURES: usize = 96;

fn tex_id(cat: usize, item: usize) -> egui::Id {
    egui::Id::new(("library_thumb", cat, item))
}

type Key = (usize, usize);
type Done = Arc<Mutex<VecDeque<(Key, Vec<u8>)>>>;

/// Bounded pending queue: prioritize the newest visible pass and the first
/// visible card within that pass.
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    shutdown: bool,
}

struct Job {
    epoch: u64,
    key: Key,
    scene: crate::model::Scene,
    ctx: egui::Context,
}

struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    done: Done,
    asked: Arc<Mutex<HashSet<Key>>>,
    failed: Arc<Mutex<HashSet<Key>>>,
    cache_order: Mutex<VecDeque<Key>>,
    epoch: AtomicU64,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        queue.shutdown = true;
        queue.jobs.clear();
        cv.notify_all();
    }
}

fn worker(ctx: &egui::Context) -> Option<Arc<Worker>> {
    let id = egui::Id::new("library_thumbnail_worker");
    if let Some(saved) = ctx.data(|d| d.get_temp::<Option<Arc<Worker>>>(id)) {
        return saved;
    }
    let started = (|| {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let done: Done = Default::default();
        let asked: Arc<Mutex<HashSet<Key>>> = Default::default();
        let failed: Arc<Mutex<HashSet<Key>>> = Default::default();
        let (q, out, sent, errors) = (queue.clone(), done.clone(), asked.clone(), failed.clone());
        std::thread::Builder::new()
            .name("meltcad-thumbs".into())
            .spawn(move || {
                loop {
                    let job = {
                        let (lock, cv) = &*q;
                        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if guard.shutdown {
                                return;
                            }
                            if let Some(best_epoch) = guard.jobs.iter().map(|job| job.epoch).max() {
                                // Newest visible pass first, top-left to bottom-right within it.
                                let position = guard.jobs.iter().position(|job| job.epoch == best_epoch).unwrap_or(0);
                                if let Some(job) = guard.jobs.remove(position) {
                                    break job;
                                }
                            }
                            guard = cv.wait(guard).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    let rgba =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::cpu_render::thumb(&job.scene, THUMB))).unwrap_or_default();
                    if rgba.len() == THUMB * THUMB * 4 {
                        let mut ready = out.lock().unwrap_or_else(|e| e.into_inner());
                        if ready.len() >= MAX_DONE
                            && let Some((discarded, _)) = ready.pop_front()
                        {
                            sent.lock().unwrap_or_else(|e| e.into_inner()).remove(&discarded);
                        }
                        ready.push_back((job.key, rgba));
                    } else {
                        errors.lock().unwrap_or_else(|e| e.into_inner()).insert(job.key);
                    }
                    job.ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Arc::new(Worker { queue, done, asked, failed, cache_order: Mutex::new(VecDeque::new()), epoch: AtomicU64::new(0) }))
    })();
    ctx.data_mut(|d| d.insert_temp(id, started.clone()));
    started
}

fn cached(ctx: &egui::Context, cat: usize, item: usize) -> Option<TextureHandle> {
    let key = (cat, item);
    let tex = ctx.data(|d| d.get_temp::<TextureHandle>(tex_id(cat, item)));
    if tex.is_some()
        && let Some(w) = worker(ctx)
    {
        let mut order = w.cache_order.lock().unwrap_or_else(|e| e.into_inner());
        order.retain(|old| *old != key);
        order.push_back(key);
    }
    tex
}

/// Install the texture in the existing egui handle cache, with bounded LRU eviction.
/// Dropping its last TextureHandle also releases egui's GPU and CPU copy.
fn cache_texture(ctx: &egui::Context, key: Key, rgba: &[u8]) -> TextureHandle {
    let img = egui::ColorImage::from_rgba_unmultiplied([THUMB, THUMB], rgba);
    let texture = ctx.load_texture(format!("lib_{}_{}", key.0, key.1), img, TextureOptions::LINEAR);
    let evicted = if let Some(w) = worker(ctx) {
        let mut order = w.cache_order.lock().unwrap_or_else(|e| e.into_inner());
        order.retain(|old| *old != key);
        order.push_back(key);
        let mut old = Vec::new();
        while order.len() > MAX_TEXTURES {
            if let Some(k) = order.pop_front() {
                old.push(k);
            }
        }
        old
    } else {
        Vec::new()
    };
    ctx.data_mut(|d| {
        d.insert_temp(tex_id(key.0, key.1), texture.clone());
        for k in &evicted {
            d.remove::<TextureHandle>(tex_id(k.0, k.1));
        }
    });
    if let Some(w) = worker(ctx) {
        let mut asked = w.asked.lock().unwrap_or_else(|e| e.into_inner());
        for key in evicted {
            asked.remove(&key);
        }
    }
    texture
}

/// Test utility to exercise the same texture insertion path.
#[cfg(test)]
fn make(ctx: &egui::Context, cat: usize, i: usize, item: &Item) -> TextureHandle {
    let rgba = crate::cpu_render::thumb(&item.scene(), THUMB);
    cache_texture(ctx, (cat, i), &rgba)
}

/// Promote a small batch per frame, rather than uploading all the completed images at once.
fn collect(ctx: &egui::Context) {
    let Some(w) = worker(ctx) else { return };
    let ready = {
        let mut g = w.done.lock().unwrap_or_else(|e| e.into_inner());
        let n = g.len().min(6);
        g.drain(..n).collect::<Vec<_>>()
    };
    for (key, rgba) in ready {
        let _ = cache_texture(ctx, key, &rgba);
    }
    if !w.done.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        ctx.request_repaint();
    }
}

/// Enqueue only cards belonging to visible rows. New scroll positions take
/// priority; discarded jobs can be requested again later.
fn ask(ctx: &egui::Context, cat: usize, item: usize, model: &Item) {
    let Some(w) = worker(ctx) else { return };
    let key = (cat, item);
    let mut asked = w.asked.lock().unwrap_or_else(|e| e.into_inner());
    if !asked.insert(key) {
        return;
    }
    let (lock, cv) = &*w.queue;
    let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
    if q.jobs.len() >= MAX_QUEUE
        && let Some(old) = q.jobs.pop_front()
    {
        asked.remove(&old.key);
    }
    q.jobs.push_back(Job { epoch: w.epoch.load(Ordering::Relaxed), key, scene: model.scene(), ctx: ctx.clone() });
    cv.notify_one();
}

/// Cancel queued CPU work when the library tab closes. Keep generated textures
/// for instant reopening and let the single in-flight thumbnail finish normally.
pub fn suspend(ctx: &egui::Context) {
    let cached_worker = ctx.data(|d| d.get_temp::<Option<Arc<Worker>>>(egui::Id::new("library_thumbnail_worker")));
    let Some(Some(w)) = cached_worker else { return };
    let mut asked = w.asked.lock().unwrap_or_else(|e| e.into_inner());
    let (lock, _) = &*w.queue;
    let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
    for job in queue.jobs.drain(..) {
        asked.remove(&job.key);
    }
}

fn failed(ctx: &egui::Context, cat: usize, item: usize) -> bool {
    worker(ctx).is_some_and(|w| w.failed.lock().unwrap_or_else(|e| e.into_inner()).contains(&(cat, item)))
}

fn cell(ui: &mut egui::Ui, ci: usize, ii: usize, name: &str, ctx: &egui::Context) -> bool {
    let category = library::loaded(ci);
    if category.is_none() {
        let _ = library::request_category(ci, ctx);
    }
    let model = category.and_then(Result::ok).and_then(|cat| cat.items.get(ii));
    ui.allocate_ui_with_layout(Vec2::new(CELL - 3.0, ROW_HEIGHT), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.set_width(CELL - 8.0);
        ui.set_min_height(ROW_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(THUMB as f32 + 8.0), egui::Sense::click());
        ui.painter().rect_filled(rect, 8.0, if response.hovered() { style::HOVER } else { style::INSET_BOTTOM });
        let texture = cached(ctx, ci, ii);
        if texture.is_none()
            && let Some(item) = model
        {
            ask(ctx, ci, ii, item);
        }
        if let Some(texture) = texture {
            ui.painter().image(texture.id(), rect.shrink(4.0), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            let indicator = if worker(ctx).is_none() || category.is_some_and(|c| c.is_err()) || failed(ctx, ci, ii) { "!" } else { "…" };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, indicator, egui::FontId::proportional(14.0), style::DIM);
        }
        let tip = format!("Add {name} to the scene");
        let mut clicked = response.on_hover_text(&tip).clicked();
        // the name works like the picture
        let label = ui.add(egui::Label::new(RichText::new(name).size(11.5)).truncate().sense(egui::Sense::click()));
        clicked |= label.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        clicked
    })
    .inner
}

fn visible_indices(range: std::ops::Range<usize>, per_row: usize, total: usize) -> std::ops::Range<usize> {
    range.start.saturating_mul(per_row).min(total)..range.end.saturating_mul(per_row).min(total)
}

/// The grid place nearest `around` (the middle of the scene) where a disc of radius `r` touches none of `taken`
/// (centre, radius). None when the grid around the middle is full.
pub fn free_spot(taken: &[([f32; 2], f32)], r: f32, around: [f32; 2]) -> Option<[f32; 2]> {
    let r = if r.is_finite() { r.clamp(2.0, 400.0) } else { 20.0 };
    let step = 2.0 * r + 6.0;
    for ring in 0..=24i32 {
        let mut best: Option<(f32, [f32; 2])> = None;
        for i in -ring..=ring {
            for j in -ring..=ring {
                if i.abs().max(j.abs()) != ring {
                    continue;
                }
                let p = [around[0] + i as f32 * step, around[1] + j as f32 * step];
                let free = taken.iter().all(|(c, tr)| ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2)).sqrt() >= tr + r + 2.0);
                let d = (p[0] - around[0]).hypot(p[1] - around[1]);
                if free && best.is_none_or(|b| d < b.0) {
                    best = Some((d, p));
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }
    None
}

/// Put an item into the scene. Returns false when the scene is full.
pub fn insert(app: &mut App, ctx: &egui::Context, item: &Item) -> bool {
    if !app.allow_scene_structure_change() {
        return false;
    }
    if !item.fits(app.scene.groups.len(), app.scene.shape_count()) {
        let why = if app.scene.groups.len() >= MAX_GROUPS { format!("Up to {MAX_GROUPS} groups") } else { format!("Up to {MAX_SHAPES} shapes") };
        app.say(ctx, why);
        return false;
    }
    // stand it just beside the existing scene (or at the middle when the scene is empty)
    let (c, r) = crate::gpu::bounds(&app.scene, None);
    let (_, own_r) = crate::gpu::bounds(&item.scene(), None);
    let at = if r > 0.0 {
        // the first free place on a grid around the middle, so a crowd grows as a block, not as a line
        let discs: Vec<([f32; 2], f32)> = app
            .scene
            .groups
            .iter()
            .filter_map(|g| {
                let mut one = crate::model::Scene::default();
                one.groups.clear();
                one.groups.push(g.clone());
                one.melt = app.scene.melt;
                let (gc, gr) = crate::gpu::bounds(&one, None);
                (gr > 0.0).then_some(([gc[0], gc[1]], gr))
            })
            .collect();
        free_spot(&discs, own_r, [c[0], c[1]]).unwrap_or([c[0] + r + own_r + 6.0, c[1]])
    } else {
        [0.0, 0.0]
    };
    app.scene.groups.push(item.placed(at));
    let gi = app.scene.groups.len() - 1;
    // A single parametric vase should open its size/wave controls immediately.
    app.sel = if app.scene.groups[gi].shapes.len() == 1
        && app.scene.groups[gi].shapes[0].vase.is_some()
    {
        Some(Pick::Shape(gi, 0))
    } else {
        Some(Pick::Group(gi))
    };
    if item.group.fusion.is_some() {
        // The existing SDF/GPU smooth union is scene-wide (Melt.mode).
        // Reuse it rather than introducing another Boolean implementation.
        app.scene.melt.mode = BlendMode::Smooth;
        app.scene.melt.amount = 1.0;
        app.scene.melt.drip = 0.0;
        app.tab = crate::app::Tab::Shape;
        app.lib_open = false; // show individual editable children in the scene list
    }
    app.frame_view(false);
    if item.group.fusion.is_some() {
        app.say(ctx, format!("Added {}; the whole scene's Melt is now Smooth", item.name));
    } else {
        app.say(ctx, format!("Added {}", item.name));
    }
    true
}

/// The assets tab of the left dock. Search reads only names/tags, never every
/// shape; the grid lays out only the rows egui reports as visible.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    app.lib_cat = app.lib_cat.min(library::count().saturating_sub(1));
    collect(&ctx);
    if let Some(w) = worker(&ctx) {
        w.epoch.fetch_add(1, Ordering::Relaxed);
    }
    ui.add(egui::TextEdit::singleline(&mut app.lib_query).hint_text("Search by name").desired_width(f32::INFINITY));
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for ci in 0..library::count() {
            let Some(name) = library::label(ci) else { continue };
            if style::pill(ui, name, app.lib_cat == ci && app.lib_query.trim().is_empty()).clicked() {
                app.lib_cat = ci;
                app.lib_query.clear();
            }
        }
    });
    ui.separator();
    let query = app.lib_query.trim().to_lowercase();
    let searching = !query.is_empty();
    let mut hits: Vec<(usize, usize, &str)> = Vec::new();
    if searching {
        if let Some(index) = library::search_index() {
            for e in &index.entries {
                if e.matches(&query) {
                    hits.push((e.category, e.item, &e.name));
                }
            }
            if index.failed_files > 0 {
                ui.label(RichText::new(format!("Could not read search data for {} categories", index.failed_files)).color(style::DIM));
            }
        } else {
            let ok = library::request_search(&ctx);
            ui.label(if ok { "Preparing search data…" } else { "Could not load search data" });
            return;
        }
    } else {
        match library::loaded(app.lib_cat) {
            Some(Ok(cat)) => {
                hits.extend(cat.items.iter().enumerate().map(|(i, item)| (app.lib_cat, i, item.name.as_str())));
            }
            Some(Err(error)) => {
                ui.label(RichText::new(format!("Could not open this category: {error}")).color(style::DIM));
                return;
            }
            None => {
                let ok = library::request_category(app.lib_cat, &ctx);
                ui.label(if ok { "Loading models…" } else { "Could not load models" });
                return;
            }
        }
    }
    if hits.is_empty() {
        ui.label(RichText::new("No results").color(style::DIM));
        return;
    }

    let per_row = ((ui.available_width() / CELL).floor() as usize).max(1);
    let rows = hits.len().div_ceil(per_row);
    let mut picked = None;
    egui::ScrollArea::vertical().id_salt(("library_grid", app.lib_cat, searching)).auto_shrink([false, false]).show_rows(
        ui,
        ROW_HEIGHT,
        rows,
        |ui, visible_rows| {
            for row in visible_rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for n in visible_indices(row..row + 1, per_row, hits.len()) {
                        let (ci, ii, name) = hits[n];
                        if cell(ui, ci, ii, name, &ctx) {
                            picked = Some((ci, ii));
                        }
                    }
                });
            }
        },
    );

    if let Some((ci, ii)) = picked {
        if let Some(Ok(category)) = library::loaded(ci) {
            if let Some(item) = category.items.get(ii) {
                insert(app, &ctx, item);
            }
        } else if let Some(Err(error)) = library::loaded(ci) {
            app.say(&ctx, format!("Could not load this model: {error}"));
        } else {
            app.say(&ctx, "The model is still loading; add it once it has loaded");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_crowd_grows_as_a_block_not_a_line() {
        let mut taken: Vec<([f32; 2], f32)> = Vec::new();
        for _ in 0..30 {
            let Some(p) = super::free_spot(&taken, 10.0, [0.0, 0.0]) else { panic!("no spot") };
            assert!(taken.iter().all(|(c, r)| (c[0] - p[0]).hypot(c[1] - p[1]) >= r + 10.0), "no overlap");
            taken.push((p, 10.0));
        }
        let xs = taken.iter().map(|(c, _)| c[0]);
        let width = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
        assert!(width < 30.0 * 26.0 / 2.0, "30 items spread {width} wide");
        assert!(super::free_spot(&[], f32::NAN, [0.0, 0.0]).is_some());
    }

    use super::*;
    use crate::model::{Group, Kind, Material, Shape};

    fn item() -> Item {
        let mut g = Group::new("テスト");
        g.shapes.push(Shape::new(Kind::Sphere, [0.0, 0.0, 10.0], Material::default()));
        Item { name: "たま".into(), tags: vec![], group: g }
    }

    #[test]
    fn inserting_adds_a_group_beside_the_scene_and_respects_limits() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let it = item();
        assert!(insert(&mut app, &ctx, &it));
        assert_eq!(app.scene.groups.len(), 2);
        assert_eq!(app.sel, Some(Pick::Group(1)));
        let (a, b) = (app.scene.groups[0].shapes[0].pos, app.scene.groups[1].shapes[0].pos);
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) > 10.0, "the new model must not sit inside the old one");
        for _ in 0..100 {
            insert(&mut app, &ctx, &it);
        }
        assert!(app.scene.groups.len() <= MAX_GROUPS && app.scene.shape_count() <= MAX_SHAPES);
    }

    #[test]
    fn a_new_model_lands_beside_a_scene_that_was_moved_away_from_the_origin() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        for g in &mut app.scene.groups {
            for s in &mut g.shapes {
                s.pos[0] += 800.0;
            }
        }
        assert!(insert(&mut app, &ctx, &item()));
        let (a, b) = (app.scene.groups[0].shapes[0].pos, app.scene.groups[1].shapes[0].pos);
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 300.0, "near the old model, not at the origin: {a:?} {b:?}");
    }

    #[test]
    fn adding_an_organic_vase_opens_its_parametric_inspector() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let category = library::all().iter().find(|c| c.name == "花瓶・Vase").expect("vases");
        assert!(insert(&mut app, &ctx, &category.items[0]));
        assert_eq!(app.sel, Some(Pick::Shape(1, 0)));
        let shape = app.scene.shape(1, 0).expect("vase");
        assert!(shape.vase.is_some());
        assert_eq!(shape.kind, Kind::Revolve);
    }

    #[test]
    fn fusion_uses_the_existing_library_path_and_restores_smooth_union() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.melt.mode = BlendMode::Gooey;
        app.scene.melt.drip = 8.0;
        app.scene.melt.amount = 2.0;
        app.lib_open = true;
        let item = crate::fusion::sample(6).unwrap();
        assert!(insert(&mut app, &ctx, &item));
        assert_eq!(app.scene.groups.last().unwrap().shapes.len(), 6);
        assert!(app.scene.groups.last().unwrap().fusion.is_some());
        assert_eq!(app.scene.melt.mode, BlendMode::Smooth);
        assert_eq!(app.scene.melt.amount, 1.0);
        assert_eq!(app.scene.melt.drip, 0.0);
        assert_eq!(app.tab, crate::app::Tab::Shape);
        assert!(!app.lib_open);
    }

    #[test]
    fn the_panel_draws() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| panel(&mut app, ui)).textures_delta.clear();
        }
        app.lib_query = "zzzz".into();
        ctx.run_ui(egui::RawInput::default(), |ui| panel(&mut app, ui)).textures_delta.clear();
    }

    #[test]
    fn thumbnails_arrive_from_the_thread_without_the_panel_waiting() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let t = std::time::Instant::now();
        ctx.run_ui(egui::RawInput::default(), |ui| panel(&mut app, ui)).textures_delta.clear();
        assert!(t.elapsed().as_millis() < 1500, "the first frame of the card asks, it does not draw ({} ms)", t.elapsed().as_millis());
        // within a few seconds the first picture is a texture
        let mut have = false;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            ctx.run_ui(egui::RawInput::default(), |ui| panel(&mut app, ui)).textures_delta.clear();
            if cached(&ctx, 0, 0).is_some() {
                have = true;
                break;
            }
        }
        assert!(have, "a picture arrived");
    }

    #[test]
    fn hundred_models_queue_only_visible_row_indices() {
        // Three columns, 100 model cards: the first 5 rows must not request the rest.
        let indices = visible_indices(0..5, 3, 100);
        assert_eq!(indices, 0..15);
        let bottom = visible_indices(32..34, 3, 100);
        assert_eq!(bottom, 96..100);
        const { assert!(MAX_QUEUE < 100 && MAX_TEXTURES < 100) };
    }

    #[test]
    fn thumb_lru_never_keeps_more_than_its_cap() {
        let ctx = egui::Context::default();
        let rgba = vec![255; THUMB * THUMB * 4];
        for id in 0..(MAX_TEXTURES + 9) {
            let _ = cache_texture(&ctx, (200, id), &rgba);
        }
        assert!(cached(&ctx, 200, 0).is_none(), "oldest texture is evicted");
        assert!(cached(&ctx, 200, MAX_TEXTURES + 8).is_some());
    }

    #[test]
    fn contexts_do_not_steal_thumbnails_or_suppress_each_others_requests() {
        let first = egui::Context::default();
        let second = egui::Context::default();
        let first_worker = worker(&first).expect("first worker");
        let second_worker = worker(&second).expect("second worker");
        // A completed render is waiting for its owner. Let the other Context
        // collect first: this used to drain the shared process-global queue.
        first_worker.asked.lock().expect("requests").insert((99, 0));
        first_worker.done.lock().expect("completed pictures").push_back(((99, 0), vec![255; THUMB * THUMB * 4]));
        collect(&second);
        assert!(cached(&second, 99, 0).is_none(), "a different Context must not consume this picture");
        collect(&first);
        assert!(cached(&first, 99, 0).is_some(), "the requesting Context still receives its picture");
        assert!(first_worker.done.lock().expect("completed pictures").is_empty());

        // The same model needs its own texture in the other Context. A previous
        // request from the first Context must not mark it as already requested.
        assert!(!second_worker.asked.lock().expect("requests").contains(&(99, 0)), "requests belong to their own Context");
        ask(&second, 99, 0, &item());
        assert!(second_worker.asked.lock().expect("requests").contains(&(99, 0)));
        assert!(cached(&second, 99, 0).is_none(), "requests stay asynchronous");
    }

    #[test]
    fn a_thumbnail_is_made_once() {
        let ctx = egui::Context::default();
        let it = item();
        assert!(cached(&ctx, 99, 0).is_none());
        let _ = make(&ctx, 99, 0, &it);
        assert!(cached(&ctx, 99, 0).is_some());
    }
}
