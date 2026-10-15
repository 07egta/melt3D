//! YURACAD: a light, easy blob modeller. Melt shapes together, carve with minus shapes, print.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

mod app;
mod camera;
mod export;
mod fonts;
mod glyphs;
mod gpu;
mod icon;
mod model;
mod panels;
mod sdf;
mod style;
mod view;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("YURACAD")
            .with_icon(icon::data()),
        ..Default::default()
    };
    eframe::run_native(
        "YURACAD",
        options,
        Box::new(|cc| {
            fonts::setup(&cc.egui_ctx);
            style::apply(&cc.egui_ctx);
            let mut srgb = true;
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                gpu::install(rs);
                srgb = rs.target_format.is_srgb();
            }
            Ok(Box::new(app::App::new(srgb, std::env::args_os().nth(1).map(std::path::PathBuf::from))))
        }),
    )
}
