//! The small card for importing an SVG: size, height, rounding, a preview, and the buttons.
//! Its state lives in egui's memory, so `App` needs no new fields.

use egui::{Align2, Color32, Pos2, RichText, Stroke, Vec2};
use std::sync::Arc;

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
        }
    }
}

fn set_art(d: &mut Dlg, res: Result<Artwork, String>, name: String) {
    match res {
        Ok(a) => {
            let mut notes = Vec::new();
            if a.dropped > 0 {
                notes.push(format!("Skipped {} tiny or extra outlines", a.dropped));
            }
            notes.extend(a.warnings.iter().cloned());
            d.msg = notes.join("\n");
            d.preview_mesh = Arc::new(preview_mesh(&a));
            d.art = Some(Arc::new(a));
            d.name = name;
        }
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
    let modal = egui::Modal::new(egui::Id::new("svg_import_area")).frame(egui::Frame::new()).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(240.0, 380.0));
            ui.label(RichText::new("Import SVG").strong().size(16.0));
            ui.label(RichText::new("Turn filled outlines into a solid. Holes stay open.").size(12.0).color(style::DIM));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 170.0).max(140.0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if style::pill(ui, "Choose SVG…", false).clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).pick_file()
                    {
                        set_art(&mut d, svg::load_file(&p), stem(&p));
                    }
                    if style::pill(ui, "Try a sample", false).clicked() {
                        set_art(&mut d, svg::parse(svg::SAMPLE_SVG), "Sample star".into());
                    }
                });
                if d.art.is_some() {
                    ui.label(RichText::new(&d.name).strong());
                } else {
                    ui.label(RichText::new("You can also drop a .svg file into the window.").size(12.0).color(style::DIM));
                }
                preview(ui, &d);
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
                ui.add_enabled_ui(d.art.is_some() && !app.scene_structure_locked(), |ui| {
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
                ui.add_enabled_ui(d.art.is_some(), |ui| {
                    let b = egui::Button::new(RichText::new("Import solid").strong().color(Color32::WHITE))
                        .fill(style::ACCENT_SOFT)
                        .stroke(Stroke::new(1.0, style::ACCENT));
                    if ui.add_sized([126.0, 32.0], b).clicked() {
                        go = true;
                    }
                });
                if ui.add_sized([80.0, 32.0], egui::Button::new("Cancel")).on_hover_text("Cancel import · Esc").clicked() {
                    close = true;
                }
            });
        });
    });
    close |= modal.should_close();
    if go && !close {
        close = import(app, ctx, &d);
    }
    if !close {
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
    let (group, note) = svg::build_group(art, &d.name, &o, room);
    if group.shapes.is_empty() {
        app.say(ctx, note.unwrap_or_else(|| "No shapes could be added".into()));
        return false;
    }
    let n = group.shapes.len();
    app.scene.groups.push(group);
    app.sel = Some(Pick::Group(app.scene.groups.len() - 1));
    app.say(ctx, note.unwrap_or_else(|| format!("Added SVG ({n} shapes)")));
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
        set_art(&mut d, svg::parse(svg::SAMPLE_SVG), "Star".into());
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
        set_art(&mut d, svg::parse(svg::SAMPLE_SVG), "Star and ring".into());
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
