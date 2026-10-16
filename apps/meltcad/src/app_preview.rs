//! Renders the whole UI (without the 3D view) with the software rasteriser in `style_preview.rs`:
//! `YURA_PREVIEW=/path/app.ppm YURA_TAB=0..3 cargo test -p meltcad app_preview -- --ignored`

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
    // `YURA_PATTERN=mirror|rect|circular|path|geometry|link` previews the group inspector's pattern section
    if let Ok(which) = std::env::var("YURA_PATTERN") {
        use crate::pattern::PatternKind;
        let mut line = crate::model::Shape::new(Kind::Sweep, [0.0; 3], crate::model::Material::default());
        line.path = vec![[-60.0, 0.0, 0.0, 1.0], [0.0, 40.0, 0.0, 1.0], [60.0, 0.0, 0.0, 1.0]];
        let mut g = crate::model::Group::new("線");
        g.shapes.push(line);
        app.scene.groups.push(g);
        app.sel = Some(crate::model::Pick::Group(0));
        match which.as_str() {
            "mirror" => app.toggle_mirror(&ctx, 0),
            "rect" => app.set_pattern_kind(&ctx, PatternKind::Rect),
            "circular" => app.set_pattern_kind(&ctx, PatternKind::Circular),
            "path" => app.set_pattern_kind(&ctx, PatternKind::Path),
            "geometry" => app.set_pattern_kind(&ctx, PatternKind::Geometry),
            "link" => app.make_instance(&ctx),
            _ => {}
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
    // `YURA_MESH=card` shows the mesh import card with a file read; `YURA_MESH=shape` selects an imported mesh (its inspector)
    let mesh = std::env::var("YURA_MESH").ok();
    if let Some(which) = mesh.as_deref() {
        let stl = std::env::temp_dir().join("meltcad_preview_model.stl");
        let _ = std::fs::write(&stl, crate::mesh_import::tests::sample_stl());
        if which == "card" {
            crate::mesh_dialog::open_path(&ctx, stl);
        } else if let Ok(l) = crate::mesh_import::load(&stl) {
            let st = crate::mesh_import::Settings { res: 48, ..crate::mesh_import::Settings::new(l.size) };
            if let Ok(b) = crate::mesh_import::bake(&l.imported.mesh, l.imported.format, &st, &crate::sdf_volume::BakeControl::new()) {
                let sh = crate::mesh_import::shape_of(&b, &l.stem, &l.file_name, [0.0, 0.0, b.size[2] * 0.5], Material::default());
                let mut g = crate::model::Group::new(l.stem.clone());
                g.shapes.push(sh);
                app.scene.groups.push(g);
                app.sel = Some(crate::model::Pick::Shape(1, 0));
            }
        }
    }
    // `YURA_GPU=compiling|failed|failed_easy|easy|ready_easy` previews the graphics path's cards over the simple CPU view
    let gpu_state = std::env::var("YURA_GPU").ok();
    if let Some(which) = gpu_state.as_deref() {
        use crate::gpu_status::AdapterInfo;
        let info = AdapterInfo { backend: "Dx12".into(), name: "NVIDIA GeForce RTX 3060".into(), driver: "NVIDIA 551.23".into(), device_type: "DiscreteGpu".into() };
        app.gpu.status.set_info(info);
        let start = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(std::env::var("YURA_GPU_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(12))).unwrap_or_else(std::time::Instant::now);
        let g = app.gpu.status.begin(start);
        match which {
            "failed" | "failed_easy" => {
                app.gpu.status.finish_err(g, "シーンの描画パイプライン (create_render_pipeline): 検証エラー (Validation): Validation Error\nCaused by:\n  In Device::create_render_pipeline, label = 'meltcad'\n    Internal error in ShaderStages(FRAGMENT) shader: D3DCompile error: error X3511: unable to unroll loop, loop does not appear to terminate in a timely manner (1024 iterations)\n    (shader.wgsl: fs)");
                app.gpu.easy = which == "failed_easy";
            }
            "easy" => app.gpu.easy = true,
            "ready_easy" => {
                app.gpu.status.finish_ok(g, 3.2);
                app.gpu.easy = true;
            }
            _ => {}
        }
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
    for _ in 0..if export.is_some() || mesh.is_some() || gpu_state.is_some() { 60 } else { 4 } {
        if export.is_some() || mesh.is_some() || gpu_state.is_some() {
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
                if gpu_state.is_some() {
                    let rect = app.view_rect;
                    let px = crate::perf::viewport_px(rect, ppp);
                    app.gpu.easy_view.update(ui.ctx(), &app.scene, &app.cam, px, false);
                    app.gpu.easy_view.paint(&ui.painter_at(rect), rect, app.scene.env.backdrop);
                    app.gpu_cards(ui.ctx(), rect);
                }
            });
            app.tool_strip(ui);
            app.tool_pill(ui);
            app.pipe_card(ui.ctx());
            app.sketch_card(ui.ctx());
            app.keys_card(ui);
            crate::export_dialog::update(&mut app, ui.ctx());
            crate::mesh_dialog::update(&mut app, ui.ctx());
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
