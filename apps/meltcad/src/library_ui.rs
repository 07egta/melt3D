//! The library card: ready-made models in tabs, with a search box and small pictures.
//! Only visible cards are built and queued; finished thumbnails remain reusable.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::library::{self, Item};
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;

const THUMB: usize = 62;
const CELL: f32 = 78.0;
const MAX_PENDING: usize = 48;
type Key = (usize, usize);

fn tex_id(cat: usize, item: usize) -> egui::Id {
    egui::Id::new(("library_thumb", cat, item))
}

fn cached(ctx: &egui::Context, cat: usize, item: usize) -> Option<TextureHandle> {
    ctx.data(|d| d.get_temp::<TextureHandle>(tex_id(cat, item)))
}

fn cache_texture(ctx: &egui::Context, key: Key, rgba: &[u8]) -> TextureHandle {
    let img = egui::ColorImage::from_rgba_unmultiplied([THUMB, THUMB], rgba);
    let texture = ctx.load_texture(format!("lib_{}_{}", key.0, key.1), img, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(tex_id(key.0, key.1), texture.clone()));
    texture
}

/// Preserve the existing fallback: without a worker, make one visible thumbnail
/// per frame. Previously completed pictures never need to be ray-marched again.
fn make(ctx: &egui::Context, cat: usize, item: usize, model: &Item) -> TextureHandle {
    cache_texture(ctx, (cat, item), &crate::cpu_render::thumb(&model.scene(), THUMB))
}

struct Job {
    key: Key,
    scene: crate::model::Scene,
    ctx: egui::Context,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    running: Option<Key>,
    done: VecDeque<(Key, Vec<u8>)>,
    failed: HashSet<Key>,
    shutdown: bool,
}

struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        queue.shutdown = true;
        queue.jobs.clear();
        ready.notify_all();
    }
}

fn worker_id() -> egui::Id {
    egui::Id::new("library_thumbnail_worker")
}

/// Scope work to its egui Context. A previous window's requested keys must not
/// prevent a later window or a test Context from receiving its own textures.
fn worker(ctx: &egui::Context) -> Option<Arc<Worker>> {
    if let Some(saved) = ctx.data(|d| d.get_temp::<Option<Arc<Worker>>>(worker_id())) {
        return saved;
    }
    let started = (|| {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let incoming = queue.clone();
        std::thread::Builder::new()
            .name("meltcad-thumbs".into())
            .spawn(move || {
                loop {
                    let job = {
                        let (lock, ready) = &*incoming;
                        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if queue.shutdown {
                                return;
                            }
                            if let Some(job) = queue.jobs.pop_front() {
                                queue.running = Some(job.key);
                                break job;
                            }
                            queue = ready.wait(queue).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    let rgba =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::cpu_render::thumb(&job.scene, THUMB))).unwrap_or_default();
                    {
                        let (lock, _) = &*incoming;
                        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
                        queue.running = None;
                        if rgba.len() == THUMB * THUMB * 4 {
                            queue.done.push_back((job.key, rgba));
                        } else {
                            queue.failed.insert(job.key);
                        }
                    }
                    job.ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Arc::new(Worker { queue }))
    })();
    ctx.data_mut(|d| d.insert_temp(worker_id(), started.clone()));
    started
}

/// Upload a small completed batch, retaining the exact same thumbnail pixels.
fn collect(ctx: &egui::Context) {
    let Some(worker) = worker(ctx) else { return };
    let (ready, remaining) = {
        let (lock, _) = &*worker.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        let count = queue.done.len().min(6);
        let ready: Vec<_> = queue.done.drain(..count).collect();
        (ready, !queue.done.is_empty())
    };
    for (key, rgba) in ready {
        let _ = cache_texture(ctx, key, &rgba);
    }
    if remaining {
        ctx.request_repaint();
    }
}

/// Discard only work that has not started and is no longer visible. Refresh the
/// order of existing requests too, so returning to a row gives it priority.
fn prioritize_visible(ctx: &egui::Context, visible: &[Key]) {
    let Some(worker) = worker(ctx) else { return };
    let (lock, _) = &*worker.queue;
    let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
    queue.jobs.retain(|job| visible.contains(&job.key));
    queue.jobs.make_contiguous().sort_by_key(|job| visible.iter().position(|key| *key == job.key));
}

/// False means the synchronous fallback is needed. A full queue simply waits
/// for a later frame; it never evicts completed thumbnails or consumes clicks.
fn ask(ctx: &egui::Context, cat: usize, item: usize, model: &Item) -> bool {
    let Some(worker) = worker(ctx) else { return false };
    let key = (cat, item);
    let (lock, ready) = &*worker.queue;
    let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
    if queue.failed.contains(&key) {
        // A renderer that already panicked must not be retried on the UI thread.
        return true;
    }
    if queue.running == Some(key) || queue.jobs.iter().any(|job| job.key == key) || queue.done.iter().any(|(done, _)| *done == key) {
        return true;
    }
    if queue.jobs.len() < MAX_PENDING {
        queue.jobs.push_back(Job { key, scene: model.scene(), ctx: ctx.clone() });
        ready.notify_one();
    }
    true
}

/// A failed render is terminal; an unavailable worker still uses the synchronous fallback.
fn thumbnail_failed(ctx: &egui::Context, key: Key) -> bool {
    let saved = ctx.data(|data| data.get_temp::<Option<Arc<Worker>>>(worker_id()));
    saved.flatten().is_some_and(|worker| worker.queue.0.lock().unwrap_or_else(|e| e.into_inner()).failed.contains(&key))
}

/// Closing the tab cancels only not-yet-started work. In-flight and completed
/// thumbnails finish normally and remain available for reopening the library.
pub fn suspend(ctx: &egui::Context) {
    let saved = ctx.data(|d| d.get_temp::<Option<Arc<Worker>>>(worker_id()));
    if let Some(Some(worker)) = saved {
        worker.queue.0.lock().unwrap_or_else(|e| e.into_inner()).jobs.clear();
    }
}

fn visible_indices(rows: std::ops::Range<usize>, per_row: usize, total: usize) -> std::ops::Range<usize> {
    rows.start.saturating_mul(per_row).min(total)..rows.end.saturating_mul(per_row).min(total)
}

struct SearchResults {
    category: usize,
    query: String,
    hits: Vec<(Key, &'static str)>,
}

fn matching_items(ctx: &egui::Context, category: usize, query: String) -> Arc<SearchResults> {
    let id = egui::Id::new("library_search_results");
    if let Some(saved) = ctx.data(|data| data.get_temp::<Arc<SearchResults>>(id))
        && saved.category == category
        && saved.query == query
    {
        return saved;
    }
    let mut hits = Vec::new();
    for (ci, metadata) in library::catalog().iter().enumerate() {
        if query.is_empty() && ci != category {
            continue;
        }
        for (ii, item) in metadata.items.iter().enumerate() {
            if item.matches(&query) {
                hits.push(((ci, ii), item.name));
            }
        }
    }
    let result = Arc::new(SearchResults { category, query, hits });
    ctx.data_mut(|data| data.insert_temp(id, result.clone()));
    result
}

struct RowMetrics {
    results: Arc<SearchResults>,
    fonts: egui::FontDefinitions,
    text_options: egui::epaint::text::TextOptions,
    pixels_per_point: f32,
    spacing: f32,
    height: f32,
}

/// Measure full wrapping labels once per result/font configuration. Scrolling
/// and thumbnail arrivals reuse the height without laying out offscreen names.
fn row_height(ui: &egui::Ui, results: &Arc<SearchResults>) -> f32 {
    let id = egui::Id::new("library_row_metrics");
    let pixels_per_point = ui.ctx().pixels_per_point();
    let spacing = ui.spacing().item_spacing.y;
    if let Some(saved) = ui.data(|data| data.get_temp::<Arc<RowMetrics>>(id))
        && Arc::ptr_eq(&saved.results, results)
        && saved.pixels_per_point == pixels_per_point
        && saved.spacing == spacing
        && ui.fonts(|fonts| fonts.definitions() == &saved.fonts && fonts.options() == &saved.text_options)
    {
        return saved.height;
    }
    let label_height = results
        .hits
        .iter()
        .map(|(_, name)| ui.painter().layout((*name).to_owned(), egui::FontId::proportional(11.5), style::DIM, CELL - 8.0).size().y)
        .fold(0.0f32, f32::max);
    let height = THUMB as f32 + 8.0 + spacing + label_height;
    let (fonts, text_options) = ui.fonts(|fonts| (fonts.definitions().clone(), *fonts.options()));
    let metrics = Arc::new(RowMetrics { results: results.clone(), fonts, text_options, pixels_per_point, spacing, height });
    ui.data_mut(|data| data.insert_temp(id, metrics));
    height
}

fn cell(ui: &mut egui::Ui, key: Key, name: &str, height: f32, made_one: &mut bool) -> bool {
    ui.allocate_ui_with_layout(Vec2::new(CELL - 4.0, height), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.set_width(CELL - 8.0);
        ui.set_min_height(height);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(THUMB as f32 + 8.0), egui::Sense::click());
        ui.painter().rect_filled(rect, 8.0, if response.hovered() { style::HOVER } else { style::INSET_BOTTOM });
        let texture = cached(ui.ctx(), key.0, key.1).or_else(|| {
            let model = library::item(key.0, key.1)?;
            if ask(ui.ctx(), key.0, key.1, model) || *made_one {
                return None;
            }
            *made_one = true;
            Some(make(ui.ctx(), key.0, key.1, model))
        });
        let failed = texture.is_none() && thumbnail_failed(ui.ctx(), key);
        if let Some(texture) = texture {
            ui.painter().image(texture.id(), rect.shrink(4.0), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, if failed { "!" } else { "…" }, egui::FontId::proportional(14.0), style::DIM);
        }
        let hint = if failed {
            format!("画像を作成できませんでした。\n{name} をシーンに入れる")
        } else {
            format!("{name} をシーンに入れる")
        };
        let clicked = response.on_hover_text(hint).clicked();
        ui.add(egui::Label::new(RichText::new(name).size(11.5)).wrap());
        clicked
    })
    .inner
}

/// Put an item into the scene. Returns false when the scene is full.
pub fn insert(app: &mut App, ctx: &egui::Context, item: &Item) -> bool {
    if !item.fits(app.scene.groups.len(), app.scene.shape_count()) {
        let why = if app.scene.groups.len() >= MAX_GROUPS {
            format!("グループは{MAX_GROUPS}個までです")
        } else {
            format!("形は{MAX_SHAPES}個までです")
        };
        app.say(ctx, why);
        return false;
    }
    // stand it just beside the existing scene (or at the middle when the scene is empty)
    let (c, r) = crate::gpu::bounds(&app.scene, None);
    let (_, own_r) = crate::gpu::bounds(&item.scene(), None);
    let at = if r > 0.0 { [c[0] + r + own_r + 6.0, c[1]] } else { [0.0, 0.0] };
    app.scene.groups.push(item.placed(at));
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    app.frame_view(false);
    app.say(ctx, format!("「{}」を入れました", item.name));
    true
}

/// The assets tab: all search metadata is available without loading model trees.
/// JSON parsing is limited to visible categories; clicks resolve synchronously.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let cats = library::catalog();
    if cats.is_empty() {
        ui.label(RichText::new("ライブラリは空です").color(style::DIM));
        return;
    }
    app.lib_cat = app.lib_cat.min(cats.len() - 1);
    let ctx = ui.ctx().clone();
    collect(&ctx);
    let mut made_one = false;
    let mut pick = None;
    ui.add(egui::TextEdit::singleline(&mut app.lib_query).hint_text("さがす（なまえ・English）").desired_width(f32::INFINITY));
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (index, category) in cats.iter().enumerate() {
            if style::pill(ui, category.name, app.lib_cat == index && app.lib_query.trim().is_empty()).clicked() {
                app.lib_cat = index;
                app.lib_query.clear();
            }
        }
    });
    ui.separator();
    let query = app.lib_query.trim().to_lowercase();
    let results = matching_items(&ctx, app.lib_cat, query);
    let hits = &results.hits;
    if hits.is_empty() {
        prioritize_visible(&ctx, &[]);
        ui.label(RichText::new("見つかりませんでした").color(style::DIM));
        return;
    }

    // Keep full, wrapping model names. The uniform virtual row must fit the
    // longest label at the current font metrics; never truncate to fake a fit.
    let row_height = row_height(ui, &results);
    let per_row = ((ui.available_width() / CELL).floor() as usize).max(1);
    let rows = hits.len().div_ceil(per_row);
    egui::ScrollArea::vertical().id_salt(("library_grid", app.lib_cat, !results.query.is_empty())).auto_shrink([false, false]).show_rows(
        ui,
        row_height,
        rows,
        |ui, visible_rows| {
            let visible = visible_indices(visible_rows.clone(), per_row, hits.len());
            let keys: Vec<Key> = hits.get(visible).unwrap_or_default().iter().map(|(key, _)| *key).collect();
            prioritize_visible(&ctx, &keys);
            for row in visible_rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for index in visible_indices(row..row + 1, per_row, hits.len()) {
                        if let Some((key, name)) = hits.get(index)
                            && cell(ui, *key, name, row_height, &mut made_one)
                        {
                            pick = Some(*key);
                        }
                    }
                });
            }
        },
    );
    if made_one {
        ctx.request_repaint();
    }
    if let Some((category, index)) = pick {
        if let Some(item) = library::item(category, index) {
            insert(app, &ctx, item);
        } else {
            app.say(&ctx, "このモデルを読み込めませんでした");
        }
    }
}

#[cfg(test)]
mod tests {
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
        let (a, b) = (app.scene.groups[0].shapes[0].pos[0], app.scene.groups[1].shapes[0].pos[0]);
        assert!(b > a + 10.0, "the new model must not sit inside the old one");
        for _ in 0..100 {
            insert(&mut app, &ctx, &it);
        }
        assert!(app.scene.groups.len() <= MAX_GROUPS && app.scene.shape_count() <= MAX_SHAPES);
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
    fn a_thumbnail_is_made_once() {
        let ctx = egui::Context::default();
        let it = item();
        assert!(cached(&ctx, 99, 0).is_none());
        let _ = make(&ctx, 99, 0, &it);
        assert!(cached(&ctx, 99, 0).is_some());
    }
    fn paused_worker(ctx: &egui::Context) -> Arc<Worker> {
        let worker = Arc::new(Worker { queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())) });
        ctx.data_mut(|data| data.insert_temp(worker_id(), Some(worker.clone())));
        worker
    }

    #[test]
    fn real_constrained_panel_queues_only_its_visible_results() {
        let ctx = egui::Context::default();
        let worker = paused_worker(&ctx);
        let mut app = App::new(true, None);
        app.lib_query = "e".into();
        let total = library::catalog().iter().flat_map(|cat| cat.items).filter(|item| item.matches("e")).count();
        assert!(total > 100, "exercise a long real search result list");
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(260.0, 520.0))), ..Default::default() };
        ctx.run_ui(input, |ui| panel(&mut app, ui)).textures_delta.clear();
        {
            let queue = worker.queue.0.lock().expect("queue");
            assert!(!queue.jobs.is_empty(), "visible cards requested pictures");
            assert!(queue.jobs.len() < total / 2, "offscreen cards must not be queued");
            assert!(queue.jobs.len() <= MAX_PENDING);
        }
        suspend(&ctx);
    }

    #[test]
    fn pending_work_is_bounded_and_reprioritized_without_evicting_pictures() {
        let ctx = egui::Context::default();
        let worker = paused_worker(&ctx);
        let model = item();
        for index in 0..(MAX_PENDING * 2) {
            assert!(ask(&ctx, 999, index, &model));
        }
        assert_eq!(worker.queue.0.lock().expect("queue").jobs.len(), MAX_PENDING);
        prioritize_visible(&ctx, &[(999, MAX_PENDING - 1), (999, 0)]);
        {
            let queue = worker.queue.0.lock().expect("queue");
            assert_eq!(queue.jobs.iter().map(|job| job.key).collect::<Vec<_>>(), [(999, MAX_PENDING - 1), (999, 0)]);
        }
        let rgba = vec![255; THUMB * THUMB * 4];
        let _ = cache_texture(&ctx, (999, 100), &rgba);
        suspend(&ctx);
        assert!(worker.queue.0.lock().expect("queue").jobs.is_empty());
        assert!(cached(&ctx, 999, 100).is_some(), "reopening reuses finished pictures");
    }

    #[test]
    fn unavailable_thumbnail_worker_keeps_the_synchronous_fallback() {
        let ctx = egui::Context::default();
        ctx.data_mut(|data| data.insert_temp(worker_id(), None::<Arc<Worker>>));
        let model = item();
        assert!(!ask(&ctx, 99, 0, &model));
        let _ = make(&ctx, 99, 0, &model);
        assert!(cached(&ctx, 99, 0).is_some());
    }

    #[test]
    fn thumbnail_queues_and_textures_belong_to_their_own_context() {
        let first = egui::Context::default();
        let second = egui::Context::default();
        let a = paused_worker(&first);
        let b = paused_worker(&second);
        assert!(!Arc::ptr_eq(&a, &b));
        let rgba = vec![255; THUMB * THUMB * 4];
        let _ = cache_texture(&first, (99, 0), &rgba);
        assert!(cached(&second, 99, 0).is_none());
        assert!(ask(&second, 99, 0, &item()));
        assert_eq!(b.queue.0.lock().expect("queue").jobs.len(), 1);
        suspend(&second);
    }

    #[test]
    fn repeated_library_frames_reuse_search_and_name_layout() {
        let ctx = egui::Context::default();
        let first = matching_items(&ctx, 0, "e".into());
        let again = matching_items(&ctx, 0, "e".into());
        assert!(Arc::ptr_eq(&first, &again));
        let id = egui::Id::new("library_row_metrics");
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let height = row_height(ui, &first);
            let before = ui.data(|data| data.get_temp::<Arc<RowMetrics>>(id)).expect("metrics");
            assert_eq!(row_height(ui, &again), height);
            let after = ui.data(|data| data.get_temp::<Arc<RowMetrics>>(id)).expect("metrics");
            assert!(Arc::ptr_eq(&before, &after), "scrolling reuses offscreen name layout");
            ui.spacing_mut().item_spacing.y += 5.0;
            assert_eq!(row_height(ui, &again), height + 5.0, "layout responds to spacing changes");
        })
        .textures_delta
        .clear();
        let changed = matching_items(&ctx, 0, "zzzz".into());
        assert!(!Arc::ptr_eq(&first, &changed));
        assert!(changed.hits.is_empty());
    }

    #[test]
    fn metadata_selected_model_inserts_once_and_undo_restores_the_scene() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        let results = matching_items(&ctx, 0, "dog".into());
        let (key, _) = results.hits.first().expect("searchable dog model");
        let model = library::item(key.0, key.1).expect("click resolves its category immediately");
        assert!(insert(&mut app, &ctx, model));
        app.settle(&ctx);
        assert_eq!(app.scene.groups.len(), before.groups.len() + 1);
        app.undo();
        assert_eq!(app.scene, before);
    }
}
