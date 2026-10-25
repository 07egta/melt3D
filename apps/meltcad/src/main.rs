//! MELTCAD: a light, easy blob modeller. Melt shapes together, carve with minus shapes, print.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

mod accel;
mod adapt;
mod add_file;
mod anim;
mod anim_ui;
mod app;
mod array;
mod atomic_files;
mod automation;
mod backend;
mod bench;
mod cad;
mod camera;
mod clip;
mod context_toolbar;
mod control;
mod corner;
mod cpu_render;
mod curve;
mod curve_tool;
mod document_job;
mod easy_view;
mod designer_chair;
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
mod fusion;
mod library;
mod library_ui;
mod marquee;
mod materials;
mod mcp;
mod mesh;
mod mesh_dialog;
mod mesh_import;
mod designer_bag;
mod designer_lamp;
mod puffy_text;
mod puffy_text_ui;
mod model;
mod organic_vase;
mod palette_prefs;
mod organic_furniture;
mod organic_tube;
mod organic_blob;
mod panels;
mod pattern;
mod pattern_ui;
mod perf;
mod pipe_tool;
mod png;
mod polygon;
mod precise;
mod precise_services;
mod prims;
mod recovery;
mod render_job;
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
mod work_budget;

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
        let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
        let r = backend::Relaunch { choice: None, tried: all, file: control::file_arg(&args), keep: backend::kept_options(&args) };
        if backend::relaunch(&r).is_ok() {
            std::process::exit(0);
        }
    }
    fatal(&format!("Could not start graphics.\n{reason}\nUpdating your graphics driver may fix it."));
    std::process::exit(1)
}

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(ok) = automation::run_cli(&args) {
        if !ok {
            std::process::exit(1);
        }
        return Ok(());
    }
    // CLI handlers must run in order: MCP owns stdio until EOF, so never
    // evaluate every handler eagerly before checking which one matched.
    if let Some(success) = cpu_render::cli(&args) {
        if !success {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(success) = bench::cli(&args) {
        if !success {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(success) = mcp::cli(&args) {
        if !success {
            std::process::exit(1);
        }
        return Ok(());
    }
    // An explicit --control must name a usable port; never start without the window it asked for (#264).
    let control_port = match control::port_arg(&args) {
        Ok(port) => port,
        Err(e) => {
            eprintln!("meltcad: {e}");
            std::process::exit(2);
        }
    };
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
        multisampling: precise_services::msaa(),
        depth_buffer: 24,
        stencil_buffer: 8,
        ..Default::default()
    };
    let tried_for_app = tried.clone();
    // Delete per-launch token files on a normal exit, including after an
    // eframe error. A crash may leave an inert file but never exposes it.
    let token_files = std::sync::Arc::new(std::sync::Mutex::new(Vec::<std::path::PathBuf>::new()));
    let files_for_app = token_files.clone();
    let result = eframe::run_native(
        "MELTCAD",
        options,
        Box::new(move |cc| {
            perf::mark("window and graphics ready");
            let mut font_job = font_job;
            if let Some(bytes) = font_job.poll() {
                fonts::install(&cc.egui_ctx, bytes);
                perf::mark("font installed before first frame");
            }
            style::apply(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.clone();
            if rs.is_none() {
                no_graphics(choice, plan, &tried_for_app, "Could not create a graphics device (wgpu).");
            }
            let srgb = rs.as_ref().is_none_or(|rs| rs.target_format.is_srgb());
            let all: Vec<std::ffi::OsString> = std::env::args_os().collect();
            let mut app = app::App::new(srgb, control::file_arg(&all));
            app.attach_font_job(font_job);
            app.precise_services = Some(precise_services::services(rfd::AsyncFileDialog::new().set_parent(cc)));
            app.precise_prefs = cc.storage.and_then(|storage| storage.get_string(precise::PREFS_KEY));
            if let Some(port) = control_port {
                let token = control::make_token();
                let ctx = cc.egui_ctx.clone();
                match control::start(port, token.clone(), move || ctx.request_repaint()) {
                    Ok((rx, port)) => {
                        match control::write_token(port, &token) {
                            Ok(path) => {
                                app.control = Some(rx);
                                eprintln!("control: 127.0.0.1:{port}, token file {}", path.display());
                                if let Ok(mut files) = files_for_app.lock() {
                                    files.push(path);
                                }
                            }
                            Err(e) => {
                                // Fail closed: never activate the window's
                                // command receiver if the secret could not
                                // be published securely.
                                eprintln!("control mode disabled: {e}");
                            }
                        }
                    }
                    Err(e) => eprintln!("{e}"),
                }
            }
            // Open the requested workspace before compiling another workspace's shaders.
            // OpenGL shares a driver lock: starting Melt's large shader here can otherwise
            // stall SolidCraft's first frame for minutes even though Melt is not visible.
            if control::starts_in_solidcraft(control::file_arg(&all).as_deref()) {
                app.gpu.rs = rs.clone();
                app.open_precise(&cc.egui_ctx);
            }
            if !app.precise_on {
                app.attach_gpu(rs, &cc.egui_ctx, false);
            }
            perf::mark("app created");
            Ok(Box::new(app))
        }),
    );
    if let Ok(files) = token_files.lock() {
        for path in files.iter() {
            let _ = std::fs::remove_file(path);
        }
    }
    if let Err(e) = &result {
        gpu_log::log(&format!("eframe stopped with an error: {e}"));
        no_graphics(choice, plan, &tried, &e.to_string());
    }
    result
}
