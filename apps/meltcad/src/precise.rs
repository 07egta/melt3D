//! The precise solid view: SolidCraft's own solid modeller (exact sketches, features, STEP export)
//! shown inside Melt 3D instead of the melt view. A thin bar on top leads back.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{RichText, Ui};
use solidcraft_engine::Session;
use solidcraft_ui_egui::theme;
use solidcraft_ui_egui::{Services, SolveApp};

use crate::app::App;
use crate::model::{Pick, SolidLink};
use crate::style;

pub struct Precise {
    pub app: SolveApp,
    /// Only a mesh that actually reached the Melt scene is acknowledged.
    sent: Option<u64>,
    pending: Option<(u64, crate::mesh_dialog::SolidImport)>,
    message: Option<String>,
    /// The design of the solid being sent, kept on the Melt shape so it can be edited again.
    design: Option<String>,
    /// Editing the solid of this Melt shape (its uid): coming back replaces that shape.
    edit: Option<u32>,
    /// The design file opened for an edit (removed when this workspace goes).
    opened: Option<ExportFile>,
}

const DEPTH_BITS: u8 = 24;
const STENCIL_BITS: u8 = 8;

pub(crate) const PREFS_KEY: &str = "melt3d.solidcraft.preferences.v1";

impl Precise {
    fn new(rs: Option<&egui_wgpu::RenderState>, services: Option<Services>, prefs: Option<&str>) -> Precise {
        let mut app = SolveApp::new(Session::default(), services.unwrap_or_else(|| crate::precise_services::services(rfd::AsyncFileDialog::new())));
        app.apply_off_thread = Some(solidcraft_ui_egui::dialogs::APPLY_AT_ONCE);
        app.preferences.language = "ja".into();
        app.autosave_minutes = 0.5;
        if let Some(prefs) = prefs {
            app.load_prefs(prefs);
        }
        // turn, slide and zoom the view the same way as the Melt view (after the saved settings,
        // so both views always work alike)
        app.preferences.nav = solidcraft_ui_egui::prefs::MELT_NAV.into();
        if !cfg!(test) {
            solidcraft_ui_egui::recovery_ui::start(
                &mut app,
                &crate::recovery::default_directory().join("solidcraft"),
                std::time::Duration::from_secs(30),
            );
            solidcraft_ui_egui::prefs::set_autosave(&mut app);
        }
        if let Some(rs) = rs {
            app.set_wgpu(rs, DEPTH_BITS, STENCIL_BITS, rs.msaa_samples);
            solidcraft_ui_egui::gpu::install_picture_renderer(rs);
            solidcraft_ui_egui::prefs::check_msaa(&mut app, u32::from(crate::precise_services::msaa()));
        }
        Precise { app, sent: None, pending: None, message: None, design: None, edit: None, opened: None }
    }

    fn editing(&self) -> bool {
        self.app.session.active_sketch.is_some()
            || self.app.tool.is_some()
            || self.app.dialog.is_some()
            || self.app.applying.is_some()
            || self.app.file_picker_busy()
    }
}

impl App {
    pub(crate) fn precise_transfer_pending(&self) -> bool {
        self.precise.as_ref().is_some_and(|p| p.pending.is_some())
    }

    /// Switch to SolidCraft's solid modeller (made on first use, kept while Melt 3D runs).
    pub(crate) fn open_precise(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.set_precise_mode(ctx, true) {
            self.say(ctx, e);
        }
    }

    /// The "Solid" and "Sketch" buttons: SolidCraft's sketch screen, drawing starts at once (it asks
    /// for the plane first). When it is done, "Melt 3D に戻る" brings the solid back as a shape.
    pub(crate) fn open_precise_sketch(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.set_precise_mode(ctx, true) {
            self.say(ctx, e);
            return;
        }
        if let Some(p) = self.precise.as_mut()
            && !p.editing()
        {
            p.app.start("sketch.create");
        }
    }

    /// Both visible navigation and external commands use the same protected transition.
    pub(crate) fn set_precise_mode(&mut self, ctx: &egui::Context, on: bool) -> Result<(), String> {
        self.sync_precise_transfer(ctx);
        if self.precise_on == on {
            return Ok(());
        }
        if crate::mesh_dialog::is_open(ctx) {
            return Err("取り込みが終わるか、取り消してから画面を切り替えてください".into());
        }
        if on {
            if self.scene_edit_in_progress() {
                return Err("今の操作を確定するか、取り消してから精密ソリッドを開いてください".into());
            }
            self.settle(ctx);
            crate::fonts::ensure(ctx);
            if self.precise.is_none() {
                self.precise = Some(Precise::new(self.gpu.rs.as_ref(), self.precise_services.take(), self.precise_prefs.as_deref()));
            }
            if let Some(p) = self.precise.as_ref() {
                solidcraft_ui_egui::fonts::setup_fonts_without_waiting(ctx, &p.app.preferences.language, &p.app.preferences.ui_font);
            }
            theme::apply(ctx);
        } else {
            if self.precise.as_ref().is_some_and(Precise::editing) {
                return Err("スケッチや立体の操作を完了するか、取り消してから戻ってください".into());
            }
            self.send_precise_solid(ctx)?;
            if self.gpu.rs.is_some() && !self.gpu.easy && matches!(self.gpu.status.snapshot().state, crate::gpu_status::State::Off) {
                self.attach_gpu(self.gpu.rs.clone(), ctx, false);
            }
            crate::fonts::setup(ctx);
            style::apply(ctx);
        }
        self.precise_on = on;
        ctx.request_repaint();
        Ok(())
    }

    pub(crate) fn sync_precise_transfer(&mut self, ctx: &egui::Context) {
        let Some(p) = self.precise.as_mut() else { return };
        let Some((hash, job)) = p.pending.as_ref() else { return };
        let Some(result) = job.take_result() else { return };
        let hash = *hash;
        p.pending = None;
        match result {
            Ok(()) => {
                p.sent = Some(hash);
                p.message = None;
                let (design, edit) = (p.design.take(), p.edit);
                self.link_precise_solid(design, edit);
                // the next visit starts on a clean, new design; this one lives on in the shape.
                // Its settings carry over to the next one.
                if let Some(p) = self.precise.take() {
                    self.precise_prefs = Some(p.app.prefs());
                }
            }
            Err(e) => {
                let message = format!("{e} 精密ソリッドの形は残っています。戻り直すと再び取り込めます");
                p.message = Some(message.clone());
                self.say(ctx, message);
            }
        }
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
        egui::Panel::top("precise_back").frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 4))).show(
            ui,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!p.editing(), egui::Button::new(RichText::new("Melt 3D に戻る").color(style::ACCENT)))
                        .on_hover_text("作った立体を寸法・位置を保って自動で取り込みます。スケッチや操作は先に完了してください")
                        .clicked()
                    {
                        back = true;
                    }
                });
                if let Some(message) = &p.message {
                    ui.label(RichText::new(message).size(11.5).color(style::CUT));
                }
            },
        );
        p.app.logic(&ctx);
        p.app.ui(ui);
        if back
            && let Err(e) = self.set_precise_mode(&ctx, false)
            && let Some(p) = self.precise.as_mut()
        {
            p.message = Some(e);
        }
        true
    }
}

impl App {
    /// Leaving the precise view puts the solid there into the melt view (as a mesh that melts like
    /// any shape). It does nothing when there is no solid or it was sent already.
    fn send_precise_solid(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let Some(p) = self.precise.as_mut() else { return Ok(()) };
        if let Some(a) = p.app.autosave.as_mut() {
            a.save(&p.app.session).map_err(|e| format!("精密データの自動保存に失敗しました: {e}"))?;
        }
        if p.app.session.model.state().bodies.is_empty() {
            return Ok(());
        }
        if p.pending.is_some() {
            return Err("前の立体を取り込み中です".into());
        }
        let file = ExportFile::new()?;
        let text = file.path().to_str().ok_or("一時ファイルの場所を読み取れません")?.to_owned();
        p.app.run("file.export", serde_json::json!({"path":text,"format":"stl"})).map_err(|e| format!("立体を書き出せませんでした: {e}"))?;
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(file.path())
            .and_then(|f| f.take(256 * 1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|e| format!("立体を読み取れませんでした: {e}"))?;
        let hash = fingerprint(&bytes);
        if p.sent == Some(hash) {
            return Ok(());
        }
        let design = String::from_utf8(solidcraft_engine::io::write_design(&p.app.session.design())).ok();
        p.design = design.filter(|d| d.len() <= crate::model::MAX_SOLID_DESIGN);
        let imported = crate::import_mesh::import_bytes(&bytes, "SolidCraft.stl", &crate::import_mesh::ImportOptions { y_up: Some(false) })?;
        let size = imported.mesh.size();
        let loaded = crate::mesh_import::Loaded { file_name: "SolidCraft".into(), stem: p.app.session.doc.name.clone(), imported, size };
        p.pending = Some((hash, crate::mesh_dialog::open_solid(ctx, loaded)?));
        p.message = None;
        Ok(())
    }
}

impl App {
    /// The solid that just came in (the selected shape) keeps its design. When it was an edit of an
    /// existing shape, that shape takes the new solid (where the person had moved it) instead.
    fn link_precise_solid(&mut self, design: Option<String>, edit: Option<u32>) {
        // the import puts the new solid alone in a new last group
        let Some(gi) = self.scene.groups.len().checked_sub(1) else { return };
        let si = 0;
        let Some(new) = self.scene.shape(gi, si).filter(|s| s.kind == crate::model::Kind::Mesh && s.solid.is_none()).cloned() else { return };
        let link = design.map(|design| SolidLink { design, placed_at: new.pos });
        let target = edit.and_then(|uid| self.scene.shape_by_uid(uid)).filter(|&t| t != (gi, si));
        if let Some((tg, ts)) = target
            && self.scene.groups.get(gi).is_some_and(|g| g.shapes.len() == 1)
        {
            let Some(old) = self.scene.shape_mut(tg, ts) else { return };
            // keep how far the person moved it since it came in
            let placed = old.solid.as_ref().map_or(old.pos, |l| l.placed_at);
            let moved = [old.pos[0] - placed[0], old.pos[1] - placed[1], old.pos[2] - placed[2]];
            old.volume = new.volume.clone();
            old.size = new.size;
            old.pos = [new.pos[0] + moved[0], new.pos[1] + moved[1], new.pos[2] + moved[2]];
            old.solid = link;
            self.scene.remove_group(gi);
            self.sel = self.scene.shape_by_uid(edit.unwrap_or(0)).map(|(g, s)| Pick::Shape(g, s));
            self.multi.clear();
            return;
        }
        if let Some(sh) = self.scene.shape_mut(gi, si) {
            sh.solid = link;
        }
        let _ = self.scene.ensure_shape_uid(gi, si);
    }

    /// "Edit in SolidCraft": open the design behind this shape in SolidCraft; coming back puts the
    /// changed solid in its place.
    pub(crate) fn edit_in_solidcraft(&mut self, ctx: &egui::Context, g: usize, s: usize) -> Result<(), String> {
        let design =
            self.scene.shape(g, s).and_then(|sh| sh.solid.as_ref()).map(|l| l.design.clone()).ok_or("この形には SolidCraft のデータがありません")?;
        if self.precise.as_ref().is_some_and(|p| p.pending.is_some() || p.editing()) {
            return Err("SolidCraft の作業を先に終えてください".into());
        }
        let uid = self.scene.ensure_shape_uid(g, s).ok_or("この形を編集できません")?;
        // a fresh workspace: whatever was there was sent already or is put aside
        if let Some(p) = self.precise.take() {
            self.precise_prefs = Some(p.app.prefs());
        }
        self.set_precise_mode(ctx, true)?;
        let file = ExportFile::named("design.solidcraft")?;
        std::fs::write(file.path(), design.as_bytes()).map_err(|e| format!("データを用意できません: {e}"))?;
        let Some(p) = self.precise.as_mut() else { return Err("SolidCraft を開けませんでした".into()) };
        let path = file.path().to_str().ok_or("一時ファイルの場所を読み取れません")?.to_owned();
        p.app.open_path(&path);
        p.edit = Some(uid);
        p.opened = Some(file);
        // the design itself is unchanged until something is edited: coming back without changes
        // does not bring a second copy in
        p.sent = None;
        Ok(())
    }
}

/// Each export owns a fresh directory; the in-memory importer never depends on a stale STL.
struct ExportFile(PathBuf);

impl ExportFile {
    fn new() -> Result<Self, String> {
        Self::named("solid.stl")
    }

    fn named(name: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let dir = std::env::temp_dir().join(format!("melt3d-solid-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&dir) {
                Ok(()) => return Ok(Self(dir.join(name))),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("一時ファイルを用意できません: {e}")),
            }
        }
        Err("一時ファイルを用意できませんでした".into())
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ExportFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn exact_box(app: &mut App, ctx: &egui::Context) {
        app.set_precise_mode(ctx, true).unwrap();
        let p = app.precise.as_mut().unwrap();
        p.app.run("sketch.create", json!({"plane":"XY","name":"Base"})).unwrap();
        p.app.run("sketch.rectangle.two_point", json!({"p0":[0,0],"p1":[240,24]})).unwrap();
        p.app.run("sketch.finish", json!({})).unwrap();
        p.app.run("solid.extrude", json!({"sketch":"Base","distance":12,"operation":"new"})).unwrap();
    }

    fn finish_import(app: &mut App, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while crate::mesh_dialog::is_open(ctx) {
            let mut out = ctx.run_ui(egui::RawInput::default(), |_ui| crate::mesh_dialog::update(app, ctx));
            out.textures_delta.clear();
            app.sync_precise_transfer(ctx);
            assert!(std::time::Instant::now() < deadline, "automatic import did not complete");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.sync_precise_transfer(ctx);
    }

    #[test]
    fn the_embedded_workspace_keeps_upstream_default_layout_and_panels() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_precise_mode(&ctx, true).unwrap();
        let embedded = &app.precise.as_ref().unwrap().app;
        let standard = solidcraft_ui_egui::UiState::default();
        assert_eq!(embedded.ui.layout, standard.layout);
        assert_eq!(embedded.ui.tab, standard.tab);
        assert_eq!(embedded.ui.show_browser, standard.show_browser);
        assert_eq!(embedded.ui.show_timeline, standard.show_timeline);
        assert_eq!(embedded.preferences.language, "ja");
        assert!(embedded.services.pick_open.is_some());
        assert!(embedded.services.pick_save.is_some());
    }

    #[test]
    fn embedded_preferences_survive_a_new_host_session() {
        let ctx = egui::Context::default();
        let mut first = App::new(true, None);
        first.set_precise_mode(&ctx, true).unwrap();
        let p = &mut first.precise.as_mut().unwrap().app;
        p.preferences.language = "en".into();
        p.autosave_minutes = 7.0;
        p.preferences.layout = solidcraft_ui_egui::shell::LayoutMode::Docked;
        let mut stored = std::collections::HashMap::default();
        struct Memory(std::collections::HashMap<String, String>);
        impl eframe::Storage for Memory {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.to_owned(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        let mut memory = Memory(std::mem::take(&mut stored));
        eframe::App::save(&mut first, &mut memory);
        let mut second = App::new(true, None);
        second.precise_prefs = eframe::Storage::get_string(&memory, PREFS_KEY);
        second.set_precise_mode(&ctx, true).unwrap();
        let p = &second.precise.as_ref().unwrap().app;
        assert_eq!(p.preferences.language, "en");
        assert_eq!(p.autosave_minutes, 7.0);
        assert_eq!(p.preferences.layout, solidcraft_ui_egui::shell::LayoutMode::Docked);
    }

    #[test]
    fn returning_imports_the_solid_once_then_solidcraft_starts_fresh_and_the_shape_stays_editable() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        exact_box(&mut app, &ctx);
        let features = app.precise.as_ref().unwrap().app.session.doc.features.len();
        let count = app.scene.shape_count();
        let undo = app.undo_depth();
        app.set_precise_mode(&ctx, false).unwrap();
        assert!(!app.precise_on && app.precise_transfer_pending());
        assert!(app.precise.as_ref().unwrap().sent.is_none(), "starting a bake is not successful import");
        finish_import(&mut app, &ctx);
        assert_eq!(app.scene.shape_count(), count + 1);
        assert_eq!(app.undo_depth(), undo + 1, "the imported solid has its own undo step");
        let (g, s) = (app.scene.groups.len() - 1, 0);
        let shape = app.scene.shape(g, s).unwrap().clone();
        assert_eq!(shape.kind, crate::model::Kind::Mesh);
        assert_eq!(shape.size, [240.0, 24.0, 12.0], "no manual import size cap");
        assert_eq!(shape.pos, [120.0, 12.0, 6.0], "keep the exact source placement");
        assert!(shape.volume.as_ref().unwrap().distance([0.0; 3]) < 0.0, "the solid is a meltable distance volume");
        assert!(shape.solid.as_ref().is_some_and(|l| l.design.contains("Base")), "the shape keeps its SolidCraft design");
        assert!(app.precise.is_none(), "the sent design is not left behind in SolidCraft");

        // the next visit is a clean new design, and coming back adds nothing
        app.set_precise_mode(&ctx, true).unwrap();
        assert!(app.precise.as_ref().unwrap().app.session.doc.features.len() < features, "SolidCraft opens fresh");
        app.set_precise_mode(&ctx, false).unwrap();
        assert!(!crate::mesh_dialog::is_open(&ctx));
        assert_eq!(app.scene.shape_count(), count + 1);

        // later: Edit in SolidCraft opens the same design; coming back replaces the shape where it was moved to
        let uid = app.scene.shape(g, s).unwrap().uid;
        app.scene.shape_mut(g, s).unwrap().pos[0] += 10.0;
        app.edit_in_solidcraft(&ctx, g, s).unwrap();
        assert!(app.precise_on);
        assert_eq!(app.precise.as_ref().unwrap().app.session.doc.features.len(), features, "the design is back, history and all");
        app.set_precise_mode(&ctx, false).unwrap();
        finish_import(&mut app, &ctx);
        assert_eq!(app.scene.shape_count(), count + 1, "the edit replaces the shape, not a second copy");
        let (g2, s2) = app.scene.shape_by_uid(uid).expect("same shape");
        let again = app.scene.shape(g2, s2).unwrap();
        assert_eq!(again.pos, [130.0, 12.0, 6.0], "it stays where it was moved");
        assert_eq!(app.sel, Some(Pick::Shape(g2, s2)));
    }

    #[test]
    fn a_cancelled_conversion_can_retry_the_same_exact_solid() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        exact_box(&mut app, &ctx);
        app.set_precise_mode(&ctx, false).unwrap();
        assert!(crate::mesh_dialog::cancel(&ctx));
        app.sync_precise_transfer(&ctx);
        assert!(!app.precise_transfer_pending());
        assert!(app.precise.as_ref().unwrap().sent.is_none());
        app.set_precise_mode(&ctx, true).unwrap();
        app.set_precise_mode(&ctx, false).unwrap();
        assert!(app.precise_transfer_pending(), "retry unchanged geometry after cancellation");
        let count = app.scene.shape_count();
        finish_import(&mut app, &ctx);
        assert_eq!(app.scene.shape_count(), count + 1, "the retry brings it in");
        assert!(app.precise.is_none(), "and SolidCraft starts fresh next time");
    }

    #[test]
    fn mode_changes_protect_melt_drafts_and_unfinished_exact_sketches() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.click([2.0, 3.0], false);
        let scene = app.scene.clone();
        assert!(app.set_precise_mode(&ctx, true).is_err());
        assert!(!app.precise_on && app.sk.on && app.scene == scene);
        app.set_sketch_tool(false);
        app.set_precise_mode(&ctx, true).unwrap();
        app.precise.as_mut().unwrap().app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        assert!(app.set_precise_mode(&ctx, false).is_err());
        assert!(app.precise_on && app.precise.as_ref().unwrap().app.session.active_sketch.is_some());
    }

    #[test]
    fn the_sketch_button_opens_solidcrafts_sketch_screen_and_starts_drawing() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.open_precise_sketch(&ctx);
        assert!(app.precise_on, "the sketch screen is SolidCraft's");
        let p = app.precise.as_ref().unwrap();
        assert!(p.editing(), "asking for the plane has started");
        assert!(!app.sk.on, "Melt 3D's own sketch tool stays off");
    }

    #[test]
    fn exports_have_private_paths_and_clean_up_their_files() {
        let first = ExportFile::new().unwrap();
        let second = ExportFile::new().unwrap();
        let dir = first.path().parent().unwrap().to_path_buf();
        assert_ne!(first.path(), second.path());
        std::fs::write(first.path(), b"owned export").unwrap();
        drop(first);
        assert!(!dir.exists());
        assert!(second.path().parent().unwrap().is_dir());
    }
}
