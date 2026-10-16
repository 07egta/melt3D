//! MELTCAD: a light, easy blob modeller. Melt shapes together, carve with minus shapes, print.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

mod app;
mod array;
mod camera;
mod clip;
mod corner;
mod export;
mod export_decimate;
mod export_dialog;
mod export_deflate;
mod export_formats;
mod export_job;
mod export_mesh;
mod fonts;
mod gizmo;
mod glyphs;
mod glyphs_more;
mod gpu;
mod icon;
mod import_3mf;
mod import_glb;
mod import_inflate;
mod import_mesh;
mod library;
mod library_ui;
mod materials;
mod cad;
mod cpu_render;
mod curve;
mod curve_tool;
mod pipe_tool;
mod model;
mod perf;
mod prims;
mod panels;
mod png;
mod sdf;
mod sketch;
mod sketch_ui;
mod stroke;
mod style;
mod svg;
mod svg_dialog;
mod view;

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if cpu_render::cli(&args) {
        return Ok(());
    }
    perf::init();
    // find and read the Japanese font while the window and the graphics card are being set up
    let font_job = fonts::FontJob::start();
    perf::mark("main");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("MELTCAD")
            .with_icon(icon::data()),
        ..Default::default()
    };
    eframe::run_native(
        "MELTCAD",
        options,
        Box::new(move |cc| {
            perf::mark("window and graphics ready");
            fonts::install(&cc.egui_ctx, font_job.finish());
            perf::mark("fonts installed");
            style::apply(&cc.egui_ctx);
            let mut srgb = true;
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                // the shader compiles on its own thread: the window shows first
                gpu::install(rs, &cc.egui_ctx);
                srgb = rs.target_format.is_srgb();
            }
            let app = app::App::new(srgb, std::env::args_os().nth(1).map(std::path::PathBuf::from));
            perf::mark("app created");
            Ok(Box::new(app))
        }),
    )
}
