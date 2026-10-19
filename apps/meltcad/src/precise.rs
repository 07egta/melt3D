//! The precise solid view: SolidCraft's own solid modeller (exact sketches, features, STEP export)
//! shown inside Melt 3D instead of the melt view. A thin bar on top leads back.

use egui::{Color32, RichText, Ui};
use solidcraft_engine::Session;
use solidcraft_ui_egui::theme::{self, Tokens};
use solidcraft_ui_egui::{Services, SolveApp};

use crate::app::App;
use crate::style;

pub struct Precise {
    pub app: SolveApp,
    /// Fingerprint of the last solid sent to the melt view (the same solid is not sent twice).
    sent: u64,
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

/// SolidCraft in Melt 3D's colours: the same greys, the same green accent, so the two views feel
/// like one app.
fn melt_skin(mut t: Tokens) -> Tokens {
    t.app_bar = style::PANEL_DARK;
    t.app_bar_text = style::TEXT;
    t.app_bar_hover = style::HOVER;
    t.toolbar = style::PANEL;
    t.panel = style::PANEL;
    t.panel_header = style::CARD;
    t.heading_bg = style::CARD;
    t.subheading_bg = style::PANEL_DARK;
    t.dialog_bg = style::PANEL;
    t.dialog_border = style::BORDER;
    t.border = style::BORDER;
    t.text = style::TEXT;
    t.text_dim = style::DIM;
    t.accent = style::ACCENT;
    t.accent_soft = style::ACCENT_SOFT;
    t.chip = style::ACCENT_SOFT;
    t.hover = style::HOVER;
    t.field = style::PANEL_DARK;
    t.timeline = style::PANEL_DARK;
    t.viewport_top = Color32::from_rgb(40, 40, 43);
    t.viewport_bottom = Color32::from_rgb(26, 26, 28);
    t
}

impl Precise {
    fn new(rs: Option<&egui_wgpu::RenderState>) -> Precise {
        theme::set_skin(melt_skin);
        let mut app = SolveApp::new(Session::default(), services());
        app.enable_easy_layout();
        if let Some(rs) = rs {
            app.set_wgpu(rs, DEPTH_BITS, STENCIL_BITS, rs.msaa_samples);
        }
        Precise { app, sent: 0 }
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
        egui::Panel::top("precise_back")
            .exact_size(34.0)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui
                        .button(RichText::new("< Back to Melt 3D").color(style::ACCENT))
                        .on_hover_text("Go back to Melt 3D. The solid you made comes with you, ready to melt")
                        .clicked()
                    {
                        back = true;
                    }
                    ui.label(
                        RichText::new("Precise solid: sketch, extrude, fillet, export STEP. Press Back and the solid appears in Melt 3D")
                            .size(12.0)
                            .color(style::DIM),
                    );
                });
            });
        p.app.logic(&ctx);
        p.app.ui(ui);
        if back {
            self.precise_on = false;
            self.send_precise_solid(&ctx);
            // Melt 3D's own look comes back
            style::apply(&ctx);
            ctx.request_repaint();
        }
        true
    }
}

impl App {
    /// Leaving the precise view puts the solid there into the melt view (as a mesh that melts like
    /// any shape). It does nothing when there is no solid or it was sent already.
    fn send_precise_solid(&mut self, ctx: &egui::Context) {
        let Some(p) = self.precise.as_mut() else { return };
        let path = std::env::temp_dir().join("melt3d_from_precise.stl");
        let Some(text) = path.to_str().map(str::to_owned) else { return };
        if p.app.run("file.export", serde_json::json!({"path": text})).is_err() {
            return;
        }
        let Ok(bytes) = std::fs::read(&path) else { return };
        let hash = fingerprint(&bytes);
        if bytes.len() < 84 || p.sent == hash {
            return;
        }
        p.sent = hash;
        crate::mesh_dialog::open_path_auto(ctx, path);
    }
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(bytes);
    h.finish()
}
