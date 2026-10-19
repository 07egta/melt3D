//! The precise solid view: SolidCraft's own solid modeller (exact sketches, features, STEP export)
//! shown inside Melt 3D instead of the melt view. A thin bar on top leads back.

use egui::{RichText, Ui};
use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};

use crate::app::App;
use crate::style;

pub struct Precise {
    pub app: SolveApp,
}

const DEPTH_BITS: u8 = 24;
const STENCIL_BITS: u8 = 8;

fn services() -> Services {
    let dialog = rfd::AsyncFileDialog::new();
    let open = dialog.clone();
    let save = dialog.clone();
    Services {
        pick_open: Some(Box::new(move || {
            let future = open
                .clone()
                .add_filter("Designs, STEP, IGES and meshes", &["solidcraft", "json", "step", "stp", "igs", "iges", "3mf", "stl"])
                .add_filter("All files", &["*"])
                .pick_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        pick_save: Some(Box::new(move |name: &str, exts: &[&str]| {
            let future = save.clone().set_file_name(name).add_filter("File", exts).save_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        ..Default::default()
    }
}

impl Precise {
    fn new(rs: Option<&egui_wgpu::RenderState>) -> Precise {
        let mut app = SolveApp::new(Session::default(), services());
        app.enable_easy_layout();
        if let Some(rs) = rs {
            app.set_wgpu(rs, DEPTH_BITS, STENCIL_BITS, rs.msaa_samples);
        }
        Precise { app }
    }
}

impl App {
    /// Switch to SolidCraft's solid modeller (made on first use, kept while Melt 3D runs).
    pub(crate) fn open_precise(&mut self, ctx: &egui::Context) {
        if self.precise.is_none() {
            self.precise = Some(Precise::new(self.gpu.rs.as_ref()));
        }
        self.precise_on = true;
        ctx.request_repaint();
    }

    /// Draws the precise view when it is on; true when it took the whole frame.
    pub(crate) fn precise_frame(&mut self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        if !self.precise_on {
            return false;
        }
        let Some(p) = self.precise.as_mut() else {
            self.precise_on = false;
            return false;
        };
        let mut back = false;
        egui::Panel::top("precise_back").exact_size(34.0).frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if ui.button(RichText::new("< Back to Melt 3D").color(style::ACCENT)).on_hover_text("Leave the precise solid view (your solid is kept)").clicked() {
                    back = true;
                }
                ui.label(RichText::new("Precise solid (SolidCraft engine): sketch, extrude, fillet, export STEP").size(12.0).color(style::DIM));
            });
        });
        p.app.logic(&ctx);
        p.app.ui(ui);
        if back {
            self.precise_on = false;
            // Melt 3D's own look comes back
            style::apply(&ctx);
            ctx.request_repaint();
        }
        true
    }
}
