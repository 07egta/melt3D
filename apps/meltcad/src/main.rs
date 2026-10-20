//! MELTCAD: a light, easy blob modeller. Melt shapes together, carve with minus shapes, print.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

mod accel;
mod adapt;
mod anim;
mod anim_ui;
mod app;
mod array;
mod automation;
mod backend;
mod bench;
mod cad;
mod camera;
mod clip;
mod control;
mod corner;
mod cpu_render;
mod curve;
mod curve_tool;
mod easy_view;
mod export;
mod export_decimate;
mod export_deflate;
mod export_dialog;
mod export_formats;
mod export_job;
mod export_mesh;
mod fonts;
#[cfg(test)]
mod frame_bench;
mod gif;
mod gizmo;
mod glyphs;
mod glyphs_more;
mod gpu;
mod gpu_log;
mod gpu_status;
mod gpu_timing;
mod gpu_ui;
mod hud;
mod icon;
mod import_3mf;
mod import_glb;
mod import_inflate;
mod import_mesh;
#[cfg(test)]
mod input_tests;
mod library;
mod library_ui;
mod marquee;
mod materials;
mod mcp;
mod mesh;
mod mesh_dialog;
mod mesh_import;
mod model;
mod palette_prefs;
mod panels;
mod pattern;
mod pattern_ui;
mod perf;
mod pipe_tool;
mod png;
mod precise;
mod prims;
mod recovery;
mod sdf;
mod sdf_volume;
#[cfg(test)]
mod shader_portability;
mod sketch;
mod sketch_dimensions;
mod sketch_edit_ops;
mod sketch_icons;
mod sketch_keep;
mod sketch_live;
#[cfg(test)]
mod sketch_live_tests;
mod sketch_plane_pick;
mod sketch_saved;
mod sketch_ui;
mod stroke;
mod style;
mod svg;
mod svg_dialog;
mod view;
mod view_mode;

/// Tell the person why the program cannot start (there is no console in a Windows release build).
fn fatal(message: &str) {
    gpu_log::log(&format!("FATAL: {message}"));
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Melt 3D")
        .set_description(format!("{message}\n\nLog: {}", gpu_log::path().display()))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// The window has no graphics device. Start again with the next backend if there is one, else
/// say so and stop (nothing can be drawn without a device, not even the buttons).
fn no_graphics(choice: backend::Choice, plan: backend::Plan, tried: &[backend::Backend], reason: &str) -> ! {
    gpu_log::log(&format!("no graphics device with {plan:?}: {reason}"));
    if let backend::Plan::Only(b) = plan {
        backend::remember(b, false);
    }
    let settings = backend::load();
    if let Some(next) = backend::next_after_failure(choice, cfg!(target_os = "windows"), &settings, tried, plan) {
        let mut all = tried.to_vec();
        if let backend::Plan::Only(b) = plan {
            all.push(b);
        }
        gpu_log::log(&format!("starting again with {next:?}"));
        let r = backend::Relaunch { choice: None, tried: all, file: control::file_arg(&std::env::args_os().collect::<Vec<_>>()) };
        if backend::relaunch(&r).is_ok() {
            std::process::exit(0);
        }
    }
    fatal(&format!("Could not start graphics.\n{reason}\nUpdating your graphics driver may fix it."));
    std::process::exit(1)
}

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if cpu_render::cli(&args) || bench::cli(&args) || automation::cli(&args) || mcp::cli(&args) {
        return Ok(());
    }
    perf::init();
    let tried = backend::tried_now();
    gpu_log::init(!tried.is_empty());
    gpu_log::install_panic_hook();
    gpu_log::install_logger();
    let (choice, choice_error) = backend::choice_now();
    if let Some(e) = choice_error {
        gpu_log::log(&e);
    }
    let settings = backend::load();
    gpu_log::log(&format!("settings {settings:?}, choice {choice:?}, tried {tried:?}"));
    let Some(plan) = backend::plan(choice, cfg!(target_os = "windows"), &settings, &tried) else {
        fatal("Could not start with any graphics mode.");
        return Ok(());
    };
    gpu_log::log(&format!("graphics plan {plan:?}"));
    // find and read the Japanese font while the window and the graphics card are being set up
    let font_job = fonts::FontJob::start();
    perf::mark("main");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Melt 3D")
            .with_icon(icon::data()),
        wgpu_options: backend::wgpu_configuration(plan),
        // the precise solid view (SolidCraft's) draws with depth
        depth_buffer: 24,
        stencil_buffer: 8,
        ..Default::default()
    };
    let tried_for_app = tried.clone();
    let result = eframe::run_native(
        "MELTCAD",
        options,
        Box::new(move |cc| {
            perf::mark("window and graphics ready");
            fonts::install(&cc.egui_ctx, font_job.finish());
            perf::mark("fonts installed");
            style::apply(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.clone();
            if rs.is_none() {
                no_graphics(choice, plan, &tried_for_app, "Could not create a graphics device (wgpu).");
            }
            let srgb = rs.as_ref().is_none_or(|rs| rs.target_format.is_srgb());
            let all: Vec<std::ffi::OsString> = std::env::args_os().collect();
            let mut app = app::App::new(srgb, control::file_arg(&all));
            if let Some(port) = control::port_arg(&all) {
                let token = control::make_token();
                let ctx = cc.egui_ctx.clone();
                match control::start(port, token.clone(), move || ctx.request_repaint()) {
                    Ok((rx, port)) => {
                        app.control = Some(rx);
                        let _ = std::fs::write(control::token_file(port), &token);
                        eprintln!("control: 127.0.0.1:{port}, token file {}", control::token_file(port).display());
                    }
                    Err(e) => eprintln!("{e}"),
                }
            }
            // the shader compiles on its own thread: the window shows first, with the simple view
            // the GPU picture is the default every launch; the simple view is only a temporary choice
            app.attach_gpu(rs, &cc.egui_ctx, false);
            perf::mark("app created");
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = &result {
        gpu_log::log(&format!("eframe stopped with an error: {e}"));
        no_graphics(choice, plan, &tried, &e.to_string());
    }
    result
}
