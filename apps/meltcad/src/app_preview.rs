//! Renders the real application frame, including the CPU viewport and tool overlays, with the
//! software rasteriser in `style_preview.rs` (no window or graphics device is needed).
//! `YURA_PREVIEW=/path/app.ppm YURA_TAB=0..3 cargo test -p meltcad app_preview -- --ignored`
//! `YURA_SKETCH=draw|ready|saved|extrude|solid|edit` selects a step of the Solid workflow.
//! `YURA_WIDTH=1920 YURA_HEIGHT=1080 YURA_PPP=1` sets logical window size and pixel density;
//! use `960` by `600` to check the same controls in a narrow window.

use super::*;
use crate::style::preview::{Textures, apply_deltas, raster, write_preview};

fn preview_number(name: &str, default: f32, min: f32, max: f32) -> f32 {
    std::env::var(name).ok().and_then(|s| s.parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default).clamp(min, max)
}

/// Reach each preview state through the same sketch/solid commands as the application. In
/// particular, Ready contains no saved shape, and Extrude still requires its own confirmation.
fn sketch_preview(app: &mut App, ctx: &egui::Context, which: &str) {
    use crate::sketch_ui::{Make, Pen, PlaneSel, Stage};

    if which == "edit" {
        app.add_shape(ctx, Kind::Loft);
        if let Some(Pick::Shape(g, s)) = app.sel {
            app.sketch_edit_begin(g, s);
        }
        return;
    }
    if !matches!(which, "draw" | "ready" | "saved" | "extrude" | "solid") {
        return;
    }

    // A dedicated sample keeps the four stages readable: an empty model, a 90 x 60 mm sketch,
    // and the 36 mm extrusion made from that saved sketch.
    app.scene.groups = vec![crate::model::Group::new("Model")];
    app.sel = None;
    app.multi.clear();
    app.solid_mode = true;
    app.set_sketch_tool(true);
    app.sketch_set_plane(PlaneSel::Floor);
    let (a, b) = ([-45.0, -30.0], [45.0, 30.0]);
    if which == "draw" {
        app.sk.pen = Pen::Polygon;
        for p in [a, [b[0], a[1]], b] {
            assert!(app.sk.click(p, false).is_none());
        }
        assert_eq!(app.sk.stage, Stage::Draw);
    } else {
        app.sk.pen = Pen::Rect;
        assert!(app.sk.click(a, false).is_none());
        let done = app.sk.click(b, false).expect("the second left click makes a rectangle draft");
        app.sketch_done(done);
        app.sk.dimension = crate::sketch_dimensions::Dimensions::for_shape(Pen::Rect, a, b, &app.sk.profile, false);
        assert_eq!(app.sk.stage, Stage::Ready);
        assert_eq!(app.scene.shape_count(), 0, "a ready sketch is still only a draft");
        assert!(app.sk.live.is_none());
        if which != "ready" {
            assert!(app.sketch_confirm(ctx), "Finish Sketch saves the guide");
            assert!(!app.sk.on);
            assert_eq!(app.scene.shape_count(), 1);
            assert!(app.scene.groups[0].shapes[0].guide);
            app.settle(ctx);
            if matches!(which, "extrude" | "solid") {
                app.sk.height = 36.0;
                assert!(app.make_from_selected_line(ctx, Make::Extrude));
                assert!(app.sk.live.is_some(), "Extrude has its own unconfirmed preview");
                assert_eq!(app.scene.shape_count(), 2);
                if which == "solid" {
                    assert!(app.sketch_confirm(ctx), "the second confirmation accepts the solid");
                    assert!(app.sk.live.is_none() && !app.sk.on);
                }
            }
        }
    }
    app.fly = None;
    app.cam.ortho = true;
    app.cam.dist = 190.0;
    if matches!(which, "draw" | "ready") {
        app.cam.target = [0.0; 3];
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
    } else {
        app.cam.target = [0.0, 0.0, 12.0];
        app.cam.look(0.6, 0.55);
    }
}

#[test]
#[ignore]
fn app_preview() {
    let ppp = preview_number("YURA_PPP", 1.5, 0.5, 3.0);
    let ctx = egui::Context::default();
    crate::fonts::setup(&ctx);
    style::apply(&ctx);
    ctx.set_pixels_per_point(ppp);
    let mut app = App::new(false, None);
    // `YURA_BOX=1` previews a box with a chamfer and a gradient (the inspector's rounding and the gradient controls)
    if std::env::var("YURA_BOX").is_ok()
        && let Some(sh) = app.scene.groups.first_mut().and_then(|g| g.shapes.first_mut())
    {
        sh.kind = Kind::Box;
        sh.size = Kind::Box.default_size();
        sh.round = 6.0;
        sh.corner = crate::model::Corner::Chamfer;
        sh.material.gradient = 1.0;
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
    if let Ok(which) = std::env::var("YURA_SKETCH") {
        sketch_preview(&mut app, &ctx, &which);
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
        let info = AdapterInfo {
            backend: "Dx12".into(),
            name: "NVIDIA GeForce RTX 3060".into(),
            driver: "NVIDIA 551.23".into(),
            device_type: "DiscreteGpu".into(),
        };
        app.gpu.status.set_info(info);
        let start = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(std::env::var("YURA_GPU_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(12)))
            .unwrap_or_else(std::time::Instant::now);
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
    // A headless frame has no wgpu callback target. Every supported simulated GPU state above
    // deliberately uses the CPU picture, while retaining the real status card for that state.
    assert!(!crate::gpu_ui::use_gpu(&app.gpu.status.snapshot(), app.gpu.easy));
    let size = egui::vec2(preview_number("YURA_WIDTH", 1380.0, 640.0, 2560.0), preview_number("YURA_HEIGHT", 860.0, 480.0, 1600.0));
    let mut tex = Textures::new();
    let mut last = None;
    // YURA_EXPORT=1 shows the export card, YURA_EXPORT=check also runs the print check
    let export = std::env::var("YURA_EXPORT").ok();
    match export.as_deref() {
        Some("check") => crate::export_dialog::open_check(&ctx),
        Some(_) => crate::export_dialog::open(&ctx, None),
        None => {}
    }
    let dialog_work = export.is_some() || mesh.is_some() || gpu_state.is_some();
    let min_frames = if dialog_work { 60 } else { 12 };
    let began = std::time::Instant::now();
    for frame in 0..1500 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            time: Some(2.0 + frame as f64 / 30.0),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.frame_body(ui));
        apply_deltas(&mut tex, &out.textures_delta);
        out.textures_delta.clear();
        last = Some(out);
        // Waiting for the still-quality picture also gives auto-sized panels and font textures
        // time to settle. Never silently publish a flat placeholder as a geometry preview.
        if frame + 1 >= min_frames && app.gpu.easy_view.settled && !app.perf.last_fast {
            break;
        }
        assert!(began.elapsed() < std::time::Duration::from_secs(30), "the CPU viewport did not settle within 30 seconds");
        std::thread::sleep(std::time::Duration::from_millis(if dialog_work { 60 } else { 20 }));
    }
    assert!(app.gpu.easy_view.settled && !app.perf.last_fast, "the preview must include the final CPU viewport");
    type StripTailLayout = (Option<(egui::Rect, egui::Rect)>, f32, f32);
    if let Some((Some((button, clip)), content_height, max_height)) =
        ctx.data(|d| d.get_temp::<StripTailLayout>(egui::Id::new("tool_strip_tail_layout")))
        && content_height <= max_height + 0.5
    {
        assert!(clip.expand(0.5).contains_rect(button), "the final tool button must be visible when the strip fits: {button:?} / {clip:?}");
    }
    if app.sketch_active() {
        let card = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("sketch_card_rect"))).expect("the sketch card was laid out");
        let strip = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("tool_strip_rect"))).expect("the tool strip was laid out");
        assert!(app.view_rect.expand(0.5).contains_rect(card), "the command card must stay inside the viewport: {card:?} / {:?}", app.view_rect);
        assert!(!card.intersects(strip), "the command card must not cover the tool strip: {card:?} / {strip:?}");
    }
    let Some(out) = last else { return };
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let img = raster(&prims, &tex, w, h, ppp, [0.2, 0.2, 0.22]);
    write_preview(img, w, h, &std::env::var("YURA_PREVIEW").unwrap_or_else(|_| "app_preview.ppm".into()));
}
