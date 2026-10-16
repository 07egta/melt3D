//! The library card: ready-made models in tabs, with a search box and small pictures. A click puts
//! the model into the scene as a new group next to what is already there.

use egui::{Align2, Color32, RichText, TextureHandle, TextureOptions, Vec2};

use crate::app::App;
use crate::library::{self, Item};
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;

const THUMB: usize = 72;
const CELL: f32 = 92.0;

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
    app.frame_view(false);
    app.say(ctx, format!("「{}」を入れました", item.name));
    true
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    if !app.lib_open {
        return;
    }
    let cats = library::all();
    if cats.is_empty() {
        return;
    }
    app.lib_cat = app.lib_cat.min(cats.len() - 1);
    let rect = app.view_rect;
    let mut close = false;
    let mut pick: Option<(usize, usize)> = None;
    let mut made_one = false;
    egui::Area::new(egui::Id::new("library_card")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(292.0, 16.0)).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            let w = (CELL * 4.0 + 28.0).min((rect.width() - 300.0).max(CELL * 2.0 + 28.0));
            ui.set_width(w);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ライブラリ").strong().size(15.0));
                ui.label(RichText::new(format!("{}こ", cats.iter().map(|c| c.items.len()).sum::<usize>())).size(12.0).color(style::DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if style::pill(ui, "とじる", false).clicked() {
                        close = true;
                    }
                });
            });
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
            egui::ScrollArea::vertical().max_height((rect.height() - 190.0).clamp(160.0, 520.0)).auto_shrink([false, true]).show(ui, |ui| {
                let per_row = ((ui.available_width() / CELL).floor() as usize).max(1);
                let mut n = 0;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(4.0);
                    for (ci, c) in cats.iter().enumerate() {
                        if !searching && ci != app.lib_cat {
                            continue;
                        }
                        for (ii, it) in c.items.iter().enumerate() {
                            if searching && !it.matches(&app.lib_query) {
                                continue;
                            }
                            n += 1;
                            ui.allocate_ui(Vec2::new(CELL, CELL + 22.0), |ui| {
                                ui.vertical_centered(|ui| {
                                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(THUMB as f32 + 8.0), egui::Sense::click());
                                    let hot = resp.hovered();
                                    ui.painter().rect_filled(r, 10.0, if hot { style::HOVER } else { style::PANEL_DARK });
                                    // one new picture per frame so opening the card never stalls
                                    let tex = cached(ui.ctx(), ci, ii).or_else(|| {
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
        });
    });
    if made_one {
        ctx.request_repaint();
    }
    if let Some((c, i)) = pick
        && let Some(item) = cats.get(c).and_then(|c| c.items.get(i))
    {
        insert(app, ctx, item);
    }
    if close {
        app.lib_open = false;
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
    fn the_card_draws_when_open() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.lib_open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
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
