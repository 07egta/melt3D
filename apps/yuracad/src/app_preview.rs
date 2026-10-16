//! Renders the whole UI (without the 3D view) with the software rasteriser in `style_preview.rs`:
//! `YURA_PREVIEW=/path/app.ppm YURA_TAB=0..3 cargo test -p yuracad app_preview -- --ignored`

use super::*;
use crate::style::preview::{Textures, apply_deltas, raster, write_ppm};

#[test]
#[ignore]
fn app_preview() {
    let ppp = 1.5;
    let ctx = egui::Context::default();
    crate::fonts::setup(&ctx);
    style::apply(&ctx);
    ctx.set_pixels_per_point(ppp);
    let mut app = App::new(false, None);
    // `YURA_BOX=1` previews a box with a chamfer and a gradient (the inspector's rounding and the gradient controls)
    if std::env::var("YURA_BOX").is_ok() {
        if let Some(sh) = app.scene.groups.first_mut().and_then(|g| g.shapes.first_mut()) {
            sh.kind = Kind::Box;
            sh.size = Kind::Box.default_size();
            sh.round = 6.0;
            sh.corner = crate::model::Corner::Chamfer;
            sh.material.gradient = 1.0;
        }
    }
    app.show_keys = std::env::var("YURA_KEYS").is_ok();
    app.lib_open = std::env::var("YURA_LIB").is_ok();
    app.tab = match std::env::var("YURA_TAB").ok().as_deref() {
        Some("1") => Tab::Material,
        Some("2") => Tab::Melt,
        Some("3") => Tab::Room,
        _ => Tab::Shape,
    };
    if std::env::var("YURA_PIPE").is_ok() {
        app.set_pipe_tool(true);
    }
    match std::env::var("YURA_SKETCH").ok().as_deref() {
        Some("draw") => app.set_sketch_tool(true),
        Some("ready") => {
            app.set_sketch_tool(true);
            app.sk.profile = crate::sketch::rect([0.0, 0.0], [30.0, 20.0]);
            app.sk.stage = crate::sketch_ui::Stage::Ready;
            app.sk.make = crate::sketch_ui::Make::Loft;
        }
        Some("edit") => {
            app.add_shape(&ctx, Kind::Loft);
            if let Some(crate::model::Pick::Shape(g, s)) = app.sel {
                app.sketch_edit_begin(g, s);
            }
        }
        _ => {}
    }
    let size = egui::vec2(1380.0, 860.0);
    let mut tex = Textures::new();
    let mut last = None;
    // YURA_EXPORT=1 shows the export card, YURA_EXPORT=check also runs the print check
    let export = std::env::var("YURA_EXPORT").ok();
    match export.as_deref() {
        Some("check") => crate::export_dialog::open_check(&ctx),
        Some(_) => crate::export_dialog::open(&ctx, None),
        None => {}
    }
    for _ in 0..if export.is_some() { 60 } else { 4 } {
        if export.is_some() {
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            app.top_bar(ui);
            app.status_bar(ui);
            app.left_dock(ui);
            app.side(ui);
            egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(52, 52, 56))).show(ui, |ui| {
                app.view_rect = ui.max_rect();
            });
            app.tool_strip(ui);
            app.tool_pill(ui);
            app.pipe_card(ui.ctx());
            app.sketch_card(ui.ctx());
            app.keys_card(ui);
            crate::export_dialog::update(&mut app, ui.ctx());
        });
        apply_deltas(&mut tex, &out.textures_delta);
        out.textures_delta.clear();
        last = Some(out);
    }
    let Some(out) = last else { return };
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let img = raster(&prims, &tex, w, h, ppp, [0.2, 0.2, 0.22]);
    write_ppm(img, w, h, &std::env::var("YURA_PREVIEW").unwrap_or_else(|_| "app_preview.ppm".into()));
}
