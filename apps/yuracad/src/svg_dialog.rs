//! The small card for importing an SVG: size, height, rounding, a preview, and the buttons.
//! Its state lives in egui's memory, so `App` needs no new fields.

use egui::{Align2, Color32, Pos2, RichText, Stroke, Vec2};

use crate::app::App;
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick};
use crate::style;
use crate::svg::{self, Artwork, BuildOpts};

#[derive(Clone)]
struct Dlg {
    art: Option<Artwork>,
    name: String,
    target: f32,
    height: f32,
    round: f32,
    flip: bool,
    msg: String,
}

fn id() -> egui::Id {
    egui::Id::new("svg_import_card")
}

/// Open the card (menu entry "SVGを読み込む…"). The file is chosen inside the card.
pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), Dlg::default()));
}

impl Default for Dlg {
    fn default() -> Dlg {
        Dlg { art: None, name: "SVG".into(), target: 60.0, height: 10.0, round: 0.0, flip: false, msg: String::new() }
    }
}

fn set_art(d: &mut Dlg, res: Result<Artwork, String>, name: String) {
    match res {
        Ok(a) => {
            d.msg = if a.dropped > 0 { format!("{}個の小さい・多すぎる線は読みとばしました", a.dropped) } else { String::new() };
            d.art = Some(a);
            d.name = name;
        }
        Err(e) => {
            d.msg = e;
            d.art = None;
        }
    }
}

fn stem(p: &std::path::Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "SVG".into())
}

/// Dropped `.svg` files open the card (other kinds are left for the rest of the app).
fn take_drops(ctx: &egui::Context) {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    for f in files {
        let path = f.path().to_path_buf();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
            continue;
        }
        let mut d = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).unwrap_or_default();
        set_art(&mut d, svg::load_file(&path), stem(&path));
        ctx.data_mut(|m| m.insert_temp(id(), d));
        ctx.request_repaint();
        break;
    }
}

/// Call once per frame: handles dropped files and shows the card when it is open.
pub fn update(app: &mut App, ctx: &egui::Context) {
    take_drops(ctx);
    let Some(mut d) = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())) else { return };
    let mut close = false;
    let mut go = false;
    egui::Area::new(egui::Id::new("svg_import_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().inner_margin(egui::Margin::symmetric(16, 14)).show(ui, |ui| {
            ui.set_width(330.0);
            ui.label(RichText::new("SVGを読み込む").strong().size(15.0));
            ui.label(RichText::new("絵（SVG）をぐーっと押し出して、立体にします。あなは自動でくり抜きます").size(11.5).color(style::DIM));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if style::pill(ui, "ファイルをえらぶ…", false).clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).pick_file()
                {
                    set_art(&mut d, svg::load_file(&p), stem(&p));
                }
                if style::pill(ui, "サンプルの星を入れる", false).clicked() {
                    set_art(&mut d, svg::parse(svg::SAMPLE_SVG), "サンプルの星".into());
                }
            });
            ui.label(RichText::new("（.svg ファイルを、ここにドラッグしてもOK）").size(11.0).color(style::DIM));
            preview(ui, &d);
            if let Some(a) = &d.art {
                ui.label(
                    RichText::new(format!(
                        "もとの大きさ {:.1} × {:.1} mm　かたまり {}・あな {}",
                        a.natural_mm[0],
                        a.natural_mm[1],
                        a.solids(),
                        a.holes()
                    ))
                    .size(11.5)
                    .color(style::DIM),
                );
            }
            if !d.msg.is_empty() {
                ui.label(RichText::new(&d.msg).size(11.5).color(style::CUT));
            }
            ui.add_space(2.0);
            egui::Grid::new("svg_import_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("大きさ").color(style::DIM));
                ui.add(egui::DragValue::new(&mut d.target).speed(0.5).range(1.0..=500.0).suffix(" mm").max_decimals(1)).on_hover_text("長いほうの辺の長さ");
                ui.end_row();
                ui.label(RichText::new("高さ").color(style::DIM));
                ui.add(egui::DragValue::new(&mut d.height).speed(0.2).range(0.5..=200.0).suffix(" mm").max_decimals(1)).on_hover_text("押し出す高さ");
                ui.end_row();
                ui.label(RichText::new("かどの丸み").color(style::DIM));
                let max_round = (d.height * 0.5).min(20.0);
                ui.add(egui::DragValue::new(&mut d.round).speed(0.05).range(0.0..=max_round).suffix(" mm").max_decimals(2))
                    .on_hover_text("角を丸くします。細い所は消えることがあります");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut d.flip, "上下を反転");
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(d.art.is_some(), |ui| {
                    let b = egui::Button::new(RichText::new("読みこむ").strong().color(Color32::WHITE)).fill(style::ACCENT_SOFT).stroke(Stroke::new(1.0, style::ACCENT));
                    if ui.add(b).clicked() {
                        go = true;
                    }
                });
                if ui.button("やめる").clicked() {
                    close = true;
                }
            });
        });
    });
    if go {
        let ok = import(app, ctx, &d);
        close = ok;
    }
    if !close {
        ctx.data_mut(|m| m.insert_temp(id(), d));
    }
}

/// Outlines drawn small: solids in violet, holes in red.
fn preview(ui: &mut egui::Ui, d: &Dlg) {
    let size = Vec2::new(300.0, 180.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, style::PANEL_DARK);
    let Some(a) = &d.art else {
        p.text(rect.center(), Align2::CENTER_CENTER, "ここに絵が出ます", egui::FontId::proportional(12.5), style::DIM);
        return;
    };
    let inner = rect.shrink(14.0);
    // the longest side of the picture fills the box
    let k = (inner.width() / a.aspect[0].max(1e-3)).min(inner.height() / a.aspect[1].max(1e-3));
    let flip = if d.flip { -1.0 } else { 1.0 };
    let at = |q: &[f32; 2]| Pos2::new(inner.center().x + q[0] * k, inner.center().y - q[1] * k * flip);
    for c in &a.contours {
        let pts: Vec<Pos2> = c.pts.iter().map(at).collect();
        let col = if c.hole { style::CUT } else { style::ACCENT };
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.5, col)));
    }
}

/// Put the picture into the scene as a new group. Returns whether the card can close.
fn import(app: &mut App, ctx: &egui::Context, d: &Dlg) -> bool {
    let Some(art) = &d.art else { return false };
    if app.scene.groups.len() >= MAX_GROUPS {
        app.say(ctx, format!("グループは{MAX_GROUPS}個までです"));
        return false;
    }
    let room = MAX_SHAPES.saturating_sub(app.scene.shape_count());
    if room == 0 {
        app.say(ctx, format!("形は{MAX_SHAPES}個までです"));
        return false;
    }
    let o = BuildOpts { target_mm: d.target, height_mm: d.height, round_mm: d.round, flip: d.flip, at: [app.cam.target[0], app.cam.target[1]] };
    let (group, note) = svg::build_group(art, &d.name, &o, room);
    if group.shapes.is_empty() {
        app.say(ctx, "入れられる形がありませんでした");
        return false;
    }
    let n = group.shapes.len();
    app.scene.groups.push(group);
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    app.say(ctx, note.unwrap_or_else(|| format!("SVGを入れました（形 {n}個）")));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_draws_and_imports_the_sample() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let mut d = Dlg::default();
        set_art(&mut d, svg::parse(svg::SAMPLE_SVG), "星".into());
        assert!(d.art.is_some() && d.msg.is_empty());
        ctx.data_mut(|m| m.insert_temp(id(), d.clone()));
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let (groups, shapes) = (app.scene.groups.len(), app.scene.shape_count());
        assert!(import(&mut app, &ctx, &d));
        assert_eq!((app.scene.groups.len(), app.scene.shape_count()), (groups + 1, shapes + 3));
        assert_eq!(app.sel, Some(Pick::Group(groups)));
        // a bad file only shows a message
        set_art(&mut d, svg::parse("hello"), "x".into());
        assert!(d.art.is_none() && !d.msg.is_empty());
        assert!(!import(&mut app, &ctx, &d));
    }
}
