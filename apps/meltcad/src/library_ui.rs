//! The library card: ready-made models in tabs, with a search box and small pictures. A click puts
//! the model into the scene as a new group next to what is already there.

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::library::{self, Item};
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;

const THUMB: usize = 62;
const CELL: f32 = 78.0;

fn tex_id(cat: usize, item: usize) -> egui::Id {
    egui::Id::new(("library_thumb", cat, item))
}

fn cached(ctx: &egui::Context, cat: usize, i: usize) -> Option<TextureHandle> {
    ctx.data(|d| d.get_temp::<TextureHandle>(tex_id(cat, i)))
}

/// Draw the item small and keep the picture.
fn make(ctx: &egui::Context, cat: usize, i: usize, item: &Item) -> TextureHandle {
    let rgba = crate::cpu_render::thumb(&item.scene(), THUMB);
    let img = egui::ColorImage::from_rgba_unmultiplied([THUMB, THUMB], &rgba);
    let h = ctx.load_texture(format!("lib_{cat}_{i}"), img, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(tex_id(cat, i), h.clone()));
    h
}

/// Pictures of the models are drawn on a thread of their own (each one is a small ray march on the
/// processor): the card never waits for them, it shows "…" until a picture arrives.
/// Finished pictures: (category, item, RGBA pixels).
type Done = std::sync::Arc<std::sync::Mutex<Vec<(usize, usize, Vec<u8>)>>>;

struct Worker {
    tx: std::sync::Mutex<std::sync::mpsc::Sender<Job>>,
    done: Done,
    asked: std::sync::Mutex<std::collections::HashSet<(usize, usize)>>,
}

struct Job {
    cat: usize,
    item: usize,
    scene: crate::model::Scene,
    ctx: egui::Context,
}

fn spawn_worker() -> Option<Worker> {
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    let done: Done = Default::default();
    let out = done.clone();
    std::thread::Builder::new()
        .name("meltcad-thumbs".into())
        .spawn(move || {
            for job in rx {
                let rgba = std::panic::catch_unwind(|| crate::cpu_render::thumb(&job.scene, THUMB)).unwrap_or_default();
                if rgba.len() == THUMB * THUMB * 4 {
                    out.lock().unwrap_or_else(|e| e.into_inner()).push((job.cat, job.item, rgba));
                }
                job.ctx.request_repaint();
            }
        })
        .ok()?;
    Some(Worker { tx: std::sync::Mutex::new(tx), done, asked: Default::default() })
}

/// Texture caches belong to a Context, so their requests and completed pictures must too.
/// A process-global queue lets another window/test consume a picture while global
/// deduplication then prevents its original Context from ever requesting it again.
fn worker(ctx: &egui::Context) -> Option<std::sync::Arc<Worker>> {
    ctx.data_mut(|data| data.get_temp_mut_or_insert_with(egui::Id::new("library_worker"), || spawn_worker().map(std::sync::Arc::new)).clone())
}

/// Turn the pictures the thread has finished into textures (a few per frame).
fn collect(ctx: &egui::Context) {
    let Some(w) = worker(ctx) else { return };
    let ready: Vec<(usize, usize, Vec<u8>)> = {
        let mut g = w.done.lock().unwrap_or_else(|e| e.into_inner());
        let n = g.len().min(6);
        g.drain(..n).collect()
    };
    for (cat, i, rgba) in ready {
        let img = egui::ColorImage::from_rgba_unmultiplied([THUMB, THUMB], &rgba);
        let h = ctx.load_texture(format!("lib_{cat}_{i}"), img, TextureOptions::LINEAR);
        ctx.data_mut(|d| d.insert_temp(tex_id(cat, i), h));
    }
    if !w.done.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        ctx.request_repaint();
    }
}

/// Ask the thread for a picture (once per model).
fn ask(ctx: &egui::Context, cat: usize, i: usize, item: &Item) {
    let Some(w) = worker(ctx) else { return };
    if !w.asked.lock().unwrap_or_else(|e| e.into_inner()).insert((cat, i)) {
        return;
    }
    let sent = w.tx.lock().unwrap_or_else(|e| e.into_inner()).send(Job { cat, item: i, scene: item.scene(), ctx: ctx.clone() });
    if sent.is_err() {
        w.asked.lock().unwrap_or_else(|e| e.into_inner()).remove(&(cat, i));
    }
}

/// The grid place nearest the middle where a disc of radius `r` touches none of `taken`
/// (centre, radius). None when the grid around the middle is full.
pub fn free_spot(taken: &[([f32; 2], f32)], r: f32) -> Option<[f32; 2]> {
    let r = if r.is_finite() { r.clamp(2.0, 400.0) } else { 20.0 };
    let step = 2.0 * r + 6.0;
    for ring in 0..=24i32 {
        let mut best: Option<(f32, [f32; 2])> = None;
        for i in -ring..=ring {
            for j in -ring..=ring {
                if i.abs().max(j.abs()) != ring {
                    continue;
                }
                let p = [i as f32 * step, j as f32 * step];
                let free = taken.iter().all(|(c, tr)| ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2)).sqrt() >= tr + r + 2.0);
                let d = p[0].hypot(p[1]);
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
        free_spot(&discs, own_r).unwrap_or([c[0] + r + own_r + 6.0, c[1]])
    } else {
        [0.0, 0.0]
    };
    app.scene.groups.push(item.placed(at));
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    app.frame_view(false);
    app.say(ctx, format!("Added {}", item.name));
    true
}

/// The assets tab of the left dock: search box, category chips, and the grid of models.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let cats = library::all();
    if cats.is_empty() {
        ui.label(RichText::new("Library is empty").color(style::DIM));
        return;
    }
    app.lib_cat = app.lib_cat.min(cats.len() - 1);
    let ctx = ui.ctx().clone();
    collect(&ctx);
    let mut made_one = false;
    let mut pick: Option<(usize, usize)> = None;
    ui.add(egui::TextEdit::singleline(&mut app.lib_query).hint_text("Search by name").desired_width(f32::INFINITY));
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (i, c) in cats.iter().enumerate() {
            if style::pill(ui, &c.name, app.lib_cat == i && app.lib_query.trim().is_empty()).clicked() {
                app.lib_cat = i;
                app.lib_query.clear();
            }
        }
    });
    ui.separator();
    let searching = !app.lib_query.trim().is_empty();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let per_row = ((ui.available_width() / CELL).floor() as usize).max(1);
        let mut n = 0;
        egui::Grid::new("library_grid").spacing(Vec2::splat(4.0)).show(ui, |ui| {
            for (ci, c) in cats.iter().enumerate() {
                if !searching && ci != app.lib_cat {
                    continue;
                }
                for (ii, it) in c.items.iter().enumerate() {
                    if searching && !it.matches(&app.lib_query) {
                        continue;
                    }
                    n += 1;
                    ui.vertical_centered(|ui| {
                        ui.set_width(CELL - 8.0);
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(THUMB as f32 + 8.0), egui::Sense::click());
                        ui.painter().rect_filled(r, 8.0, if resp.hovered() { style::HOVER } else { style::INSET_BOTTOM });
                        // the pictures come from a thread of their own, so opening the tab never stalls
                        // (without a thread: one new picture per frame)
                        let tex = cached(ui.ctx(), ci, ii).or_else(|| {
                            if worker(ui.ctx()).is_some() {
                                ask(ui.ctx(), ci, ii, it);
                                return None;
                            }
                            if made_one {
                                return None;
                            }
                            made_one = true;
                            Some(make(ui.ctx(), ci, ii, it))
                        });
                        match tex {
                            Some(t) => {
                                ui.painter().image(
                                    t.id(),
                                    r.shrink(4.0),
                                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            None => {
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, "…", egui::FontId::proportional(14.0), style::DIM);
                            }
                        }
                        if resp.on_hover_text(format!("Add {} to the scene", it.name)).clicked() {
                            pick = Some((ci, ii));
                        }
                        ui.label(RichText::new(&it.name).size(11.5));
                    });
                    if n % per_row == 0 {
                        ui.end_row();
                    }
                }
            }
        });
        if n == 0 {
            ui.label(RichText::new("No results").color(style::DIM));
        }
    });
    if made_one {
        ctx.request_repaint();
    }
    if let Some((c, i)) = pick
        && let Some(item) = cats.get(c).and_then(|c| c.items.get(i))
    {
        insert(app, &ctx, item);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_crowd_grows_as_a_block_not_a_line() {
        let mut taken: Vec<([f32; 2], f32)> = Vec::new();
        for _ in 0..30 {
            let Some(p) = super::free_spot(&taken, 10.0) else { panic!("no spot") };
            assert!(taken.iter().all(|(c, r)| (c[0] - p[0]).hypot(c[1] - p[1]) >= r + 10.0), "no overlap");
            taken.push((p, 10.0));
        }
        let xs = taken.iter().map(|(c, _)| c[0]);
        let width = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
        assert!(width < 30.0 * 26.0 / 2.0, "30 items spread {width} wide");
        assert!(super::free_spot(&[], f32::NAN).is_some());
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
    fn contexts_do_not_steal_thumbnails_or_suppress_each_others_requests() {
        let first = egui::Context::default();
        let second = egui::Context::default();
        let first_worker = worker(&first).expect("first worker");
        let second_worker = worker(&second).expect("second worker");
        // A completed render is waiting for its owner. Let the other Context
        // collect first: this used to drain the shared process-global queue.
        first_worker.asked.lock().expect("requests").insert((99, 0));
        first_worker.done.lock().expect("completed pictures").push((99, 0, vec![255; THUMB * THUMB * 4]));
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
