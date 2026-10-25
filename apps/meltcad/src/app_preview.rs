//! Renders the real application frame, including the CPU viewport and tool overlays, with the
//! software rasteriser for Melt; the precise workspace uses the real offscreen WGPU callbacks.
//! SolidCraft previews need a graphics adapter, but no native window.
//! `YURA_PREVIEW=/path/app.ppm YURA_TAB=0..3 cargo test -p meltcad app_preview -- --ignored`
//! `YURA_SKETCH=draw|ready|saved|extrude|solid|edit` selects a step of the Solid workflow.
//! `YURA_PRECISE=empty|solid|sketch|melt` shows SolidCraft or its solid after transfer into Melt.
//! `YURA_WIDTH=1920 YURA_HEIGHT=1080 YURA_PPP=1` sets logical window size and pixel density;
//! use `960` by `600` to check the same controls in a narrow window.

use super::*;
use crate::style::preview::{Textures, apply_deltas, raster, write_preview};

#[path = "app_preview_gpu.rs"]
mod app_preview_gpu;

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

/// Seed the embedded modeller through its public commands, including the sketch and feature
/// history. Its viewport renders on the CPU when no graphics device was installed.
fn precise_preview(app: &mut App, ctx: &egui::Context, which: &str) {
    use serde_json::json;

    assert!(matches!(which, "empty" | "solid" | "sketch" | "melt"), "YURA_PRECISE must be empty, solid, sketch or melt");
    app.open_precise(ctx);
    assert!(app.precise_on, "the embedded SolidCraft workspace must open");
    let p = app.precise.as_mut().expect("the embedded modeller was created");
    p.app.home.open = false;
    if which != "empty" {
        p.app.run("sketch.create", json!({"plane":"XY", "name":"Base"})).expect("create the exact sketch");
        p.app.run("sketch.rectangle.two_point", json!({"p0":[0,0], "p1":[60,40]})).expect("draw the exact rectangle");
        if matches!(which, "solid" | "melt") {
            p.app.run("sketch.finish", json!({})).expect("finish the exact sketch");
            p.app.run("solid.extrude", json!({"sketch":"Base", "distance":20, "operation":"new"})).expect("extrude the exact solid");
        }
        p.app.fit_view();
    }
    if which == "melt" {
        app.set_precise_mode(ctx, false).expect("return through the real exact-solid transfer");
    }
}

fn precise_viewport_ready(app: &App) -> bool {
    let Some(p) = app.precise.as_ref().filter(|_| app.precise_on) else { return false };
    let Some(rect) = p.app.viewport.rect else { return false };
    if rect.width() < 16.0 || rect.height() < 16.0 {
        return false;
    }

    true
}

fn check_precise_geometry(app: &App, which: &str, size: egui::Vec2) {
    if which == "melt" {
        // after the transfer SolidCraft starts fresh; the design now lives in the melt shape
        assert!(app.precise.is_none(), "the sent design is not left behind in SolidCraft");
        let shape = app.scene.groups.last().and_then(|g| g.shapes.first()).expect("the transferred solid");
        let link = shape.solid.as_ref().expect("the melt shape keeps its SolidCraft design");
        let doc = solidcraft_engine::io::read_design(link.design.as_bytes()).expect("the kept design reads back");
        assert_eq!(doc.features.len(), 2, "the exact feature history stays with the shape");
        return;
    }
    let p = app.precise.as_ref().expect("the precise workspace remains open");
    if which != "melt" {
        let rect = p.app.viewport.rect.expect("the exact modeller viewport was laid out");
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).expand(0.5).contains_rect(rect), "the embedded viewport must fit in the window");
    }
    let state = p.app.session.model.state();
    let solid = matches!(which, "solid" | "melt");
    assert_eq!(state.bodies.len(), usize::from(solid));
    assert_eq!(state.sketches.len(), usize::from(which != "empty"));
    assert_eq!(
        p.app.session.doc.features.len(),
        if solid { 2 } else { usize::from(which == "sketch") },
        "the exact feature history must remain available"
    );
    assert_eq!(p.app.session.active_sketch.is_some(), which == "sketch");
    if solid {
        let mesh = state.bodies[0].mesh();
        assert!(!mesh.triangles.is_empty(), "the exact solid must have visible geometry");
        let extent = mesh.bounds().size();
        for (actual, expected) in [(extent.x, 60.0), (extent.y, 40.0), (extent.z, 20.0)] {
            assert!((actual - expected).abs() < 1e-6, "the exact feature dimensions must be preserved: {extent:?}");
        }
    }
}

fn melt_transfer_ready(app: &App, ctx: &egui::Context, expected_shapes: usize) -> bool {
    !app.precise_on && !app.precise_transfer_pending() && !crate::mesh_dialog::is_open(ctx) && app.scene.shape_count() == expected_shapes
}

fn check_melt_transfer(app: &App, ctx: &egui::Context, expected_shapes: usize) {
    assert!(melt_transfer_ready(app, ctx, expected_shapes), "the exact-solid import must complete before capturing the Melt view");
    let Some(Pick::Shape(g, s)) = app.sel else { panic!("the transferred mesh must be selected") };
    let shape = app.scene.shape(g, s).expect("the transferred shape remains in the scene");
    assert_eq!(shape.kind, Kind::Mesh);
    assert_eq!(app.scene.groups[g].blend, 12.0, "the real transfer must enable group melting");
    assert!(app.tab == Tab::Melt, "the real transfer must show the Melt controls");
    assert!(shape.volume.as_ref().is_some_and(|volume| volume.nodes() > 0), "the transferred mesh must have its baked distance data");
    assert_eq!(shape.size, [60.0, 40.0, 20.0], "transferring into Melt must preserve the original dimensions");
    assert_eq!(shape.pos, [30.0, 20.0, 10.0], "transferring into Melt must preserve the original position");
    assert_eq!(app.scene.groups[0].shapes[0].kind, Kind::Sphere, "the original Melt shape must remain beside the transferred solid");
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
    let precise = std::env::var("YURA_PRECISE").ok();
    let precise_gpu = precise.as_deref().is_some_and(|which| matches!(which, "empty" | "solid" | "sketch"));
    let mut gpu_preview = if precise_gpu {
        Some(app_preview_gpu::GpuPreview::new().expect("a headless WGPU adapter is required for SolidCraft preview"))
    } else {
        None
    };
    if let Some(gpu) = gpu_preview.as_ref() {
        app.gpu.rs = Some(gpu.render_state.clone());
    }
    let melt_transfer = precise.as_deref() == Some("melt");
    let initial_shapes = app.scene.shape_count();
    if melt_transfer {
        assert_eq!(initial_shapes, 1, "the transfer preview starts with Melt's default sphere");
    }
    if let Some(which) = precise.as_deref() {
        precise_preview(&mut app, &ctx, which);
    }
    // `YURA_MESH=card` shows the mesh import card with a file read; `YURA_MESH=shape` selects an imported mesh (its inspector)
    let mesh = std::env::var("YURA_MESH").ok();
    if let Some(which) = mesh.as_deref() {
        let stl = std::env::temp_dir().join("meltcad_preview_model.stl");
        let _ = std::fs::write(&stl, crate::mesh_import::tests::sample_stl());
        if which == "card" {
            crate::mesh_dialog::open_path(&ctx, stl, app.document_generation());
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
    let mut melt_framed_at: Option<usize> = None;
    for frame in 0..1500 {
        if melt_transfer && melt_framed_at.is_none() && melt_transfer_ready(&app, &ctx, initial_shapes + 1) {
            check_melt_transfer(&app, &ctx, initial_shapes + 1);
            app.cam.ortho = true;
            app.cam.look(0.6, 0.55);
            app.frame_now();
            app.settle(&ctx);
            melt_framed_at = Some(frame);
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            time: Some(2.0 + frame as f64 / 30.0),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.frame_body(ui));
        if let Some(gpu) = gpu_preview.as_mut() {
            gpu.render(&ctx, &out, size, ppp);
        } else {
            apply_deltas(&mut tex, &out.textures_delta);
            out.textures_delta.clear();
        }
        last = Some(out);
        // Waiting for the still-quality picture also gives auto-sized panels and font textures
        // time to settle. Never silently publish a flat placeholder as a geometry preview.
        let ready = if melt_transfer {
            melt_framed_at.is_some_and(|started| frame + 1 >= started + min_frames)
                && melt_transfer_ready(&app, &ctx, initial_shapes + 1)
                && app.gpu.easy_view.settled
                && !app.perf.last_fast
        } else if precise.is_some() {
            precise_viewport_ready(&app)
        } else {
            app.gpu.easy_view.settled && !app.perf.last_fast
        };
        if frame + 1 >= min_frames && ready {
            break;
        }
        assert!(began.elapsed() < std::time::Duration::from_secs(30), "the CPU viewport did not settle within 30 seconds");
        std::thread::sleep(std::time::Duration::from_millis(if dialog_work { 60 } else { 20 }));
    }
    if let Some(which) = precise.as_deref() {
        if melt_transfer {
            check_melt_transfer(&app, &ctx, initial_shapes + 1);
            assert!(
                melt_framed_at.is_some() && app.gpu.easy_view.settled && !app.perf.last_fast,
                "the transferred solid must have a final Melt CPU picture"
            );
        } else {
            assert!(precise_viewport_ready(&app), "the embedded modeller viewport must be laid out");
        }
        check_precise_geometry(&app, which, size);
    } else {
        assert!(app.gpu.easy_view.settled && !app.perf.last_fast, "the preview must include the final CPU viewport");
    }
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
    let (w, h) = ((size.x * ppp).ceil() as usize, (size.y * ppp).ceil() as usize);
    let img = if let Some(gpu) = gpu_preview.as_mut() {
        gpu.render(&ctx, &out, size, ppp)
    } else {
        let prims = ctx.tessellate(out.shapes, ppp);
        raster(&prims, &tex, w, h, ppp, [0.2, 0.2, 0.22])
    };
    if precise.as_deref() == Some("solid") {
        let rect = app.precise.as_ref().unwrap().app.viewport.rect.unwrap();
        let centre = rect.shrink2(rect.size() * 0.25);
        let (x0, x1) = ((centre.left() * ppp).max(0.0) as usize, ((centre.right() * ppp) as usize).min(w));
        let (y0, y1) = ((centre.top() * ppp).max(0.0) as usize, ((centre.bottom() * ppp) as usize).min(h));
        let lit = (y0..y1).flat_map(|y| (x0..x1).map(move |x| y * w + x)).filter(|&i| img[i].iter().sum::<f32>() > 1.0).count();
        let area = (x1 - x0) * (y1 - y0);
        assert!(lit > area / 100, "the solid's viewport must contain rendered surfaces, not a blank callback: {lit}/{area}");
    }
    write_preview(img, w, h, &std::env::var("YURA_PREVIEW").unwrap_or_else(|_| "app_preview.ppm".into()));
}
