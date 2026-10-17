//! The library card: ready-made models in tabs, with a search box and small pictures. A click puts
//! the model into the scene as a new group next to what is already there.

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::library::{self, Item};
use crate::model::{BlendMode, MAX_GROUPS, MAX_SHAPES, Pick};
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

fn worker() -> Option<&'static Worker> {
    static W: std::sync::OnceLock<Option<Worker>> = std::sync::OnceLock::new();
    W.get_or_init(|| {
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
    })
    .as_ref()
}

/// Turn the pictures the thread has finished into textures (a few per frame).
fn collect(ctx: &egui::Context) {
    let Some(w) = worker() else { return };
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
    let Some(w) = worker() else { return };
    if !w.asked.lock().unwrap_or_else(|e| e.into_inner()).insert((cat, i)) {
        return;
    }
    let sent = w.tx.lock().unwrap_or_else(|e| e.into_inner()).send(Job { cat, item: i, scene: item.scene(), ctx: ctx.clone() });
    if sent.is_err() {
        w.asked.lock().unwrap_or_else(|e| e.into_inner()).remove(&(cat, i));
    }
}

/// Put an item into the scene. Returns false when the scene is full.
pub fn insert(app: &mut App, ctx: &egui::Context, item: &Item) -> bool {
    if !item.fits(app.scene.groups.len(), app.scene.shape_count()) {
        let why = if app.scene.groups.len() >= MAX_GROUPS { format!("グループは{MAX_GROUPS}個までです") } else { format!("形は{MAX_SHAPES}個までです") };
        app.say(ctx, why);
        return false;
    }
    // stand it just beside the existing scene (or at the middle when the scene is empty)
    let (c, r) = crate::gpu::bounds(&app.scene, None);
    let (_, own_r) = crate::gpu::bounds(&item.scene(), None);
    let at = if r > 0.0 { [c[0] + r + own_r + 6.0, c[1]] } else { [0.0, 0.0] };
    app.scene.groups.push(item.placed(at));
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
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
        app.say(ctx, format!("「{}」を追加。全体のメルトを「なめらか」に変更しました", item.name));
    } else {
        app.say(ctx, format!("「{}」を入れました", item.name));
    }
    true
}

/// The assets tab of the left dock: search box, category chips, and the grid of models.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    let cats = library::all();
    if cats.is_empty() {
        ui.label(RichText::new("ライブラリは空です").color(style::DIM));
        return;
    }
    app.lib_cat = app.lib_cat.min(cats.len() - 1);
    let ctx = ui.ctx().clone();
    collect(&ctx);
    let mut made_one = false;
    let mut pick: Option<(usize, usize)> = None;
    ui.add(egui::TextEdit::singleline(&mut app.lib_query).hint_text("さがす（なまえ・English）").desired_width(f32::INFINITY));
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
                            if worker().is_some() {
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
                                ui.painter().image(t.id(), r.shrink(4.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            None => {
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, "…", egui::FontId::proportional(14.0), style::DIM);
                            }
                        }
                        if resp.on_hover_text(format!("{} をシーンに入れる", it.name)).clicked() {
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
            ui.label(RichText::new("見つかりませんでした").color(style::DIM));
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
    fn a_thumbnail_is_made_once() {
        let ctx = egui::Context::default();
        let it = item();
        assert!(cached(&ctx, 99, 0).is_none());
        let _ = make(&ctx, 99, 0, &it);
        assert!(cached(&ctx, 99, 0).is_some());
    }
}
